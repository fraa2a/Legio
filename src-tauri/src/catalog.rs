use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::{
    database::{Database, DatabaseState},
    network::{NetworkError, NetworkState},
};

const REMOTE_LIMIT: usize = 50;
const LOCAL_LIMIT: u32 = 100;
const CACHE_LIMIT: u32 = 20_000;
const MAX_QUERY_BYTES: usize = 200;
const STALE_SECONDS: i64 = 24 * 60 * 60;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogGame {
    pub steam_app_id: u32,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSearch {
    pub games: Vec<CatalogGame>,
    pub total: u32,
    pub cached_at: Option<i64>,
    pub stale: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CatalogErrorKind {
    InvalidQuery,
    InvalidResponse,
    Timeout,
    Network,
    Http,
    TooLarge,
    Database,
    Internal,
}

#[derive(Debug, Serialize)]
pub struct CatalogError {
    pub kind: CatalogErrorKind,
    pub message: String,
    pub status: Option<u16>,
}

impl CatalogError {
    fn new(kind: CatalogErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            status: None,
        }
    }
    fn database(message: String) -> Self {
        Self::new(CatalogErrorKind::Database, message)
    }
}

impl From<NetworkError> for CatalogError {
    fn from(error: NetworkError) -> Self {
        match error {
            NetworkError::Timeout => Self::new(
                CatalogErrorKind::Timeout,
                "Hydra did not respond before the request timed out. Cached games remain available.",
            ),
            NetworkError::Transport { message } => Self::new(
                CatalogErrorKind::Network,
                format!("Could not reach Hydra: {message}"),
            ),
            NetworkError::Http { status } => Self {
                kind: CatalogErrorKind::Http,
                message: format!("Hydra returned HTTP {status}. Cached games remain available."),
                status: Some(status),
            },
            NetworkError::TooLarge => Self::new(
                CatalogErrorKind::TooLarge,
                "Hydra returned a response larger than the 2 MiB limit.",
            ),
        }
    }
}

#[derive(Serialize)]
struct HydraRequest<'a> {
    title: &'a str,
    take: usize,
    skip: usize,
}

#[derive(Deserialize)]
struct HydraResponse {
    count: u32,
    edges: Vec<HydraGame>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HydraGame {
    object_id: String,
    title: String,
    shop: String,
}

struct CatalogPage {
    games: Vec<CatalogGame>,
    remote_count: u32,
}

fn query(value: &str) -> Result<&str, CatalogError> {
    let value = value.trim();
    if value.len() > MAX_QUERY_BYTES || value.chars().any(char::is_control) {
        return Err(CatalogError::new(
            CatalogErrorKind::InvalidQuery,
            "Search must be at most 200 UTF-8 bytes and contain no control characters.",
        ));
    }
    Ok(value)
}

fn decode(bytes: &[u8]) -> Result<CatalogPage, CatalogError> {
    let response: HydraResponse = serde_json::from_slice(bytes).map_err(|error| {
        CatalogError::new(
            CatalogErrorKind::InvalidResponse,
            format!("Hydra returned invalid catalog JSON: {error}"),
        )
    })?;
    if response.edges.len() > REMOTE_LIMIT || response.edges.len() > response.count as usize {
        return Err(CatalogError::new(
            CatalogErrorKind::InvalidResponse,
            "Hydra returned inconsistent result counts.",
        ));
    }
    let mut seen = HashSet::with_capacity(response.edges.len());
    let mut games = Vec::with_capacity(response.edges.len());
    for record in response.edges {
        if record.shop != "steam" {
            continue;
        }
        let steam_app_id = record
            .object_id
            .parse::<u32>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or_else(|| {
                CatalogError::new(
                    CatalogErrorKind::InvalidResponse,
                    "Hydra returned an invalid Steam App ID.",
                )
            })?;
        let name = record.title.trim();
        if name.is_empty()
            || name.len() > 512
            || name.chars().any(char::is_control)
            || !seen.insert(steam_app_id)
        {
            return Err(CatalogError::new(
                CatalogErrorKind::InvalidResponse,
                "Hydra returned an invalid or duplicate Steam catalog record.",
            ));
        }
        games.push(CatalogGame {
            steam_app_id,
            name: name.to_owned(),
        });
    }
    Ok(CatalogPage {
        games,
        remote_count: response.count,
    })
}

fn now() -> Result<i64, CatalogError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .ok_or_else(|| {
            CatalogError::new(
                CatalogErrorKind::Internal,
                "System clock is outside the supported date range.",
            )
        })
}

fn sql_error(error: rusqlite::Error) -> String {
    format!("catalog cache database error: {error}")
}

fn store(
    database: &Database,
    query: &str,
    page: &CatalogPage,
    fetched_at: i64,
) -> Result<(), CatalogError> {
    database.with_connection(|connection| {
        let transaction = connection.unchecked_transaction().map_err(sql_error)?;
        {
            let mut insert = transaction.prepare_cached("INSERT INTO catalog_games (steam_app_id, name, search_name, fetched_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (steam_app_id) DO UPDATE SET name = excluded.name, search_name = excluded.search_name, fetched_at = excluded.fetched_at").map_err(sql_error)?;
            for game in &page.games {
                insert.execute(params![game.steam_app_id, game.name, game.name.to_lowercase(), fetched_at]).map_err(sql_error)?;
            }
        }
        transaction.execute("INSERT INTO catalog_cache (provider, query, fetched_at, remote_count) VALUES ('hydra', ?1, ?2, ?3) ON CONFLICT (provider) DO UPDATE SET query = excluded.query, fetched_at = excluded.fetched_at, remote_count = excluded.remote_count", params![query, fetched_at, page.remote_count]).map_err(sql_error)?;
        transaction.execute("DELETE FROM catalog_games WHERE steam_app_id IN (SELECT steam_app_id FROM catalog_games ORDER BY fetched_at DESC, steam_app_id LIMIT -1 OFFSET ?1)", [CACHE_LIMIT]).map_err(sql_error)?;
        transaction.commit().map_err(sql_error)
    }).map_err(CatalogError::database)
}

fn search(
    database: &Database,
    query: &str,
    current_time: i64,
) -> Result<CatalogSearch, CatalogError> {
    let pattern = format!(
        "%{}%",
        query
            .to_lowercase()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    database.with_connection(|connection| {
        let (total, oldest): (u32, Option<i64>) = connection.query_row("SELECT count(*), min(fetched_at) FROM catalog_games WHERE search_name LIKE ?1 ESCAPE '\\'", [&pattern], |row| Ok((row.get(0)?, row.get(1)?))).map_err(sql_error)?;
        let last_refresh: Option<i64> = connection.query_row("SELECT fetched_at FROM catalog_cache WHERE provider = 'hydra'", [], |row| row.get(0)).optional().map_err(sql_error)?;
        let cached_at = oldest.or(last_refresh);
        let mut statement = connection.prepare_cached("SELECT steam_app_id, name FROM catalog_games WHERE search_name LIKE ?1 ESCAPE '\\' ORDER BY search_name, steam_app_id LIMIT ?2").map_err(sql_error)?;
        let games = statement.query_map(params![pattern, LOCAL_LIMIT], |row| Ok(CatalogGame { steam_app_id: row.get(0)?, name: row.get(1)? })).map_err(sql_error)?.collect::<Result<Vec<_>, _>>().map_err(sql_error)?;
        Ok(CatalogSearch { games, total, cached_at, stale: cached_at.is_none_or(|time| time > current_time || current_time.saturating_sub(time) >= STALE_SECONDS) })
    }).map_err(CatalogError::database)
}

pub async fn search_catalog(
    app: tauri::AppHandle,
    value: String,
) -> Result<CatalogSearch, CatalogError> {
    let query = query(&value)?.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DatabaseState>();
        search(
            state.database().map_err(CatalogError::database)?,
            &query,
            now()?,
        )
    })
    .await
    .map_err(|error| CatalogError::new(CatalogErrorKind::Internal, error.to_string()))?
}

pub async fn refresh_catalog(
    app: tauri::AppHandle,
    network: &NetworkState,
    value: String,
) -> Result<CatalogSearch, CatalogError> {
    let query = query(&value)?.to_owned();
    let body = serde_json::to_vec(&HydraRequest {
        title: &query,
        take: REMOTE_LIMIT,
        skip: 0,
    })
    .map_err(|error| CatalogError::new(CatalogErrorKind::Internal, error.to_string()))?;
    let bytes = network.hydra_search(body).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let page = decode(&bytes)?;
        let state = app.state::<DatabaseState>();
        let database = state.database().map_err(CatalogError::database)?;
        let now = now()?;
        store(database, &query, &page, now)?;
        search(database, &query, now)
    })
    .await
    .map_err(|error| CatalogError::new(CatalogErrorKind::Internal, error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolates_steam_identity_and_ignores_provider_download_sources() {
        let page = decode(br#"{"count":2,"edges":[{"objectId":"400","title":" Portal ","shop":"steam","downloadSources":[{"url":"https://untrusted.invalid"}]},{"objectId":"not-steam","title":"Other","shop":"gog"}]}"#).unwrap();
        assert_eq!(
            page.games,
            vec![CatalogGame {
                steam_app_id: 400,
                name: "Portal".into()
            }]
        );
        assert_eq!(
            serde_json::to_value(&page.games).unwrap(),
            serde_json::json!([{"steamAppId":400,"name":"Portal"}])
        );
    }

    #[test]
    fn rejects_invalid_remote_pages_before_cache_mutation() {
        for bytes in [
            br#"{"count":1,"edges":[{"objectId":"0","title":"Game","shop":"steam"}]}"#.as_slice(),
            br#"{"count":1,"edges":[{"objectId":"1","title":" ","shop":"steam"}]}"#.as_slice(),
            br#"{"count":0,"edges":[{"objectId":"1","title":"Game","shop":"steam"}]}"#.as_slice(),
            br#"{"count":2,"edges":[{"objectId":"1","title":"Game","shop":"steam"},{"objectId":"1","title":"Other","shop":"steam"}]}"#.as_slice(),
        ] {
            assert!(matches!(decode(bytes), Err(CatalogError { kind: CatalogErrorKind::InvalidResponse, .. })));
        }
    }

    #[test]
    fn searches_persistent_cache_literally_and_preserves_library_overrides() {
        let directory =
            std::env::temp_dir().join(format!("legio-catalog-test-{}", uuid::Uuid::new_v4()));
        let state = DatabaseState::new(Ok(directory.clone()));
        let database = state.database().unwrap();
        let library_game = database
            .create_game(crate::database::CreateGameInput {
                name: "My title".into(),
                steam_app_id: Some(400),
            })
            .unwrap();
        let page = CatalogPage {
            games: vec![CatalogGame {
                steam_app_id: 400,
                name: "Portal 100%_É".into(),
            }],
            remote_count: 1,
        };
        store(database, "portal", &page, 100).unwrap();
        drop(state);
        let reopened = DatabaseState::new(Ok(directory.clone()));
        let database = reopened.database().unwrap();
        let result = search(database, "%_é", 101).unwrap();
        assert_eq!(result.games, page.games);
        assert!(!result.stale);
        assert_eq!(result.cached_at, Some(100));
        assert!(
            search(database, "Portal", 100 + STALE_SECONDS)
                .unwrap()
                .stale
        );
        assert!(search(database, "missing", 101).unwrap().games.is_empty());
        assert_eq!(database.games().unwrap(), vec![library_game]);
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_refresh_rolls_back_records_and_cache_freshness() {
        let directory =
            std::env::temp_dir().join(format!("legio-catalog-test-{}", uuid::Uuid::new_v4()));
        let state = DatabaseState::new(Ok(directory.clone()));
        let database = state.database().unwrap();
        let original = CatalogPage {
            games: vec![CatalogGame {
                steam_app_id: 400,
                name: "Portal".into(),
            }],
            remote_count: 1,
        };
        store(database, "portal", &original, 100).unwrap();
        let invalid = CatalogPage {
            games: vec![
                CatalogGame {
                    steam_app_id: 400,
                    name: "Changed".into(),
                },
                CatalogGame {
                    steam_app_id: 0,
                    name: "Invalid".into(),
                },
            ],
            remote_count: 2,
        };
        assert!(store(database, "changed", &invalid, 200).is_err());
        let result = search(database, "", 201).unwrap();
        assert_eq!(result.games, original.games);
        assert_eq!(result.cached_at, Some(100));
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn bounds_disk_index_and_local_results_without_hiding_match_count() {
        let directory =
            std::env::temp_dir().join(format!("legio-catalog-test-{}", uuid::Uuid::new_v4()));
        let state = DatabaseState::new(Ok(directory.clone()));
        let database = state.database().unwrap();
        let page = CatalogPage {
            games: (1..=CACHE_LIMIT + 1)
                .map(|id| CatalogGame {
                    steam_app_id: id,
                    name: format!("Game {id:05}"),
                })
                .collect(),
            remote_count: CACHE_LIMIT + 1,
        };
        store(database, "", &page, 100).unwrap();
        let newer = CatalogPage {
            games: vec![CatalogGame {
                steam_app_id: CACHE_LIMIT + 2,
                name: "Newest".into(),
            }],
            remote_count: 1,
        };
        store(database, "newest", &newer, 200).unwrap();
        let result = search(database, "", 201).unwrap();
        assert_eq!(result.total, CACHE_LIMIT);
        assert_eq!(result.games.len(), LOCAL_LIMIT as usize);
        assert_eq!(search(database, "newest", 201).unwrap().games, newer.games);
        assert_eq!(result.cached_at, Some(100));
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
