use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tauri::{Manager, Runtime};

use crate::{
    database::{Database, DatabaseState},
    legio_source_cache,
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
    pub availability: SourceAvailability,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceAvailability {
    Unknown,
    Unavailable,
    Verified,
    Unverified,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogSort {
    #[default]
    Relevance,
    NameAsc,
    NameDesc,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogAvailability {
    #[default]
    All,
    Available,
    Verified,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CatalogOptions {
    pub sort: CatalogSort,
    pub availability: CatalogAvailability,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSearch {
    pub games: Vec<CatalogGame>,
    pub total: u32,
    pub next_offset: Option<usize>,
    pub cached_at: Option<i64>,
    pub stale: bool,
    pub source_cached_at: Option<i64>,
    pub source_stale: bool,
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
    fetched_count: usize,
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
    let fetched_count = response.edges.len();
    let mut seen = HashSet::with_capacity(fetched_count);
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
            availability: SourceAvailability::Unknown,
        });
    }
    Ok(CatalogPage {
        games,
        remote_count: response.count,
        fetched_count,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SearchRank {
    tier: u8,
    missing_words: usize,
    partial_words: usize,
    span: usize,
}

fn word_quality(name: &str, query: &str) -> Option<usize> {
    if name == query {
        Some(0)
    } else if name.starts_with(query) {
        Some(1)
    } else if name.contains(query) {
        Some(2)
    } else {
        None
    }
}

fn rank_normalized(name: &str, query: &str, query_words: &[&str]) -> Option<SearchRank> {
    if query.is_empty() || name == query {
        return Some(SearchRank {
            tier: 0,
            missing_words: 0,
            partial_words: 0,
            span: 0,
        });
    }
    if name.starts_with(query) {
        return Some(SearchRank {
            tier: 1,
            missing_words: 0,
            partial_words: 0,
            span: 0,
        });
    }

    let name_words: Vec<_> = name
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let mut used = vec![false; name_words.len()];
    let mut positions = Vec::with_capacity(query_words.len());
    let mut partial_words = 0;
    for query_word in query_words {
        let found = name_words
            .iter()
            .enumerate()
            .filter(|(index, _)| !used[*index])
            .filter_map(|(index, name_word)| {
                word_quality(name_word, query_word).map(|quality| (quality, index))
            })
            .min();
        if let Some((quality, index)) = found {
            used[index] = true;
            partial_words += usize::from(quality > 0);
            positions.push(index);
        }
    }

    if positions.is_empty() && name.contains(query) {
        return Some(SearchRank {
            tier: 5,
            missing_words: 0,
            partial_words: query_words.len(),
            span: 0,
        });
    }
    if positions.is_empty() {
        return None;
    }

    let missing_words = query_words.len() - positions.len();
    let in_order = positions.windows(2).all(|pair| pair[0] < pair[1]);
    let span = positions.iter().max().unwrap_or(&0) - positions.iter().min().unwrap_or(&0);
    let tier = if missing_words > 0 {
        5
    } else if in_order && partial_words == 0 {
        2
    } else if in_order {
        3
    } else {
        4
    };
    Some(SearchRank {
        tier,
        missing_words,
        partial_words,
        span,
    })
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

fn search_with_options(
    database: &Database,
    query: &str,
    current_time: i64,
    limit: u32,
    options: CatalogOptions,
) -> Result<CatalogSearch, CatalogError> {
    let (last_refresh, mut rows) = database
        .with_connection(|connection| {
            let last_refresh: Option<i64> = connection
                .query_row(
                    "SELECT fetched_at FROM catalog_cache WHERE provider = 'hydra'",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .map_err(sql_error)?;
            let mut statement = connection
                .prepare("SELECT steam_app_id, name, search_name, fetched_at FROM catalog_games")
                .map_err(sql_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, u32>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            Ok((last_refresh, rows))
        })
        .map_err(CatalogError::database)?;
    let source = legio_source_cache::cached(database).map_err(CatalogError::database)?;
    if let Some(source) = &source {
        let mut known: HashSet<_> = rows.iter().map(|row| row.0).collect();
        for entry in source
            .manifest
            .verified
            .iter()
            .chain(&source.manifest.unverified)
        {
            if known.insert(entry.steam_app_id) {
                rows.push((
                    entry.steam_app_id,
                    entry.name.clone(),
                    entry.name.to_lowercase(),
                    source.fetched_at,
                ));
            }
        }
    }
    let normalized_query = query.to_lowercase();
    let query_words: Vec<_> = normalized_query.split_whitespace().collect();
    let verified: HashSet<_> = source
        .iter()
        .flat_map(|source| {
            source
                .manifest
                .verified
                .iter()
                .map(|entry| entry.steam_app_id)
        })
        .collect();
    let unverified: HashSet<_> = source
        .iter()
        .flat_map(|source| {
            source
                .manifest
                .unverified
                .iter()
                .map(|entry| entry.steam_app_id)
        })
        .collect();
    let mut ranked = Vec::new();
    let mut oldest = None;
    for (steam_app_id, name, search_name, fetched_at) in rows {
        if !query_words.is_empty() && !query_words.iter().any(|word| search_name.contains(word)) {
            continue;
        }
        let availability = if source.is_none() {
            SourceAvailability::Unknown
        } else if verified.contains(&steam_app_id) {
            SourceAvailability::Verified
        } else if unverified.contains(&steam_app_id) {
            SourceAvailability::Unverified
        } else {
            SourceAvailability::Unavailable
        };
        let included = match options.availability {
            CatalogAvailability::All => true,
            CatalogAvailability::Available => matches!(
                availability,
                SourceAvailability::Verified | SourceAvailability::Unverified
            ),
            CatalogAvailability::Verified => availability == SourceAvailability::Verified,
        };
        if !included {
            continue;
        }
        if let Some(rank) = rank_normalized(&search_name, &normalized_query, &query_words) {
            oldest = Some(oldest.map_or(fetched_at, |value: i64| value.min(fetched_at)));
            ranked.push((
                rank,
                search_name,
                steam_app_id,
                CatalogGame {
                    steam_app_id,
                    name,
                    availability,
                },
            ));
        }
    }
    let total = ranked.len() as u32;
    let cached_at = oldest.or(last_refresh);
    ranked.sort_unstable_by(|left, right| {
        let order = match options.sort {
            CatalogSort::Relevance => left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)),
            CatalogSort::NameAsc => left.1.cmp(&right.1),
            CatalogSort::NameDesc => right.1.cmp(&left.1),
        };
        order.then_with(|| left.2.cmp(&right.2))
    });
    let games = ranked
        .into_iter()
        .take(limit as usize)
        .map(|entry| entry.3)
        .collect();
    Ok(merge_source(
        CatalogSearch {
            games,
            total,
            next_offset: None,
            cached_at,
            stale: cached_at.is_none_or(|time| {
                time > current_time || current_time.saturating_sub(time) >= STALE_SECONDS
            }),
            source_cached_at: None,
            source_stale: true,
        },
        source,
        current_time,
    ))
}

#[cfg(test)]
fn search_with_limit(
    database: &Database,
    query: &str,
    current_time: i64,
    limit: u32,
) -> Result<CatalogSearch, CatalogError> {
    search_with_options(
        database,
        query,
        current_time,
        limit,
        CatalogOptions::default(),
    )
}

#[cfg(test)]
fn search(
    database: &Database,
    query: &str,
    current_time: i64,
) -> Result<CatalogSearch, CatalogError> {
    search_with_limit(database, query, current_time, LOCAL_LIMIT)
}

fn merge_source(
    mut result: CatalogSearch,
    source: Option<legio_source_cache::CachedSource>,
    current_time: i64,
) -> CatalogSearch {
    if let Some(source) = source {
        result.source_cached_at = Some(source.fetched_at);
        result.source_stale = source.warning.is_some()
            || legio_source_cache::is_stale(source.fetched_at, current_time);
        let verified: std::collections::HashSet<_> = source
            .manifest
            .verified
            .iter()
            .map(|entry| entry.steam_app_id)
            .collect();
        let unverified: std::collections::HashSet<_> = source
            .manifest
            .unverified
            .iter()
            .map(|entry| entry.steam_app_id)
            .collect();
        for game in &mut result.games {
            game.availability = if verified.contains(&game.steam_app_id) {
                SourceAvailability::Verified
            } else if unverified.contains(&game.steam_app_id) {
                SourceAvailability::Unverified
            } else {
                SourceAvailability::Unavailable
            };
        }
    }
    result
}

pub async fn search_catalog<R: Runtime>(
    app: tauri::AppHandle<R>,
    value: String,
    limit: Option<u32>,
    options: Option<CatalogOptions>,
) -> Result<CatalogSearch, CatalogError> {
    let query = query(&value)?.to_owned();
    let limit = limit.unwrap_or(LOCAL_LIMIT).max(1);
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DatabaseState>();
        let database = state.database().map_err(CatalogError::database)?;
        let current_time = now()?;
        search_with_options(
            database,
            &query,
            current_time,
            limit,
            options.unwrap_or_default(),
        )
    })
    .await
    .map_err(|error| CatalogError::new(CatalogErrorKind::Internal, error.to_string()))?
}

pub async fn refresh_catalog(
    app: tauri::AppHandle,
    network: &NetworkState,
    value: String,
    skip: Option<usize>,
    limit: Option<u32>,
    options: Option<CatalogOptions>,
) -> Result<CatalogSearch, CatalogError> {
    let query = query(&value)?.to_owned();
    let skip = skip.unwrap_or(0);
    if skip >= CACHE_LIMIT as usize {
        return Err(CatalogError::new(
            CatalogErrorKind::InvalidQuery,
            "Catalog page is outside the cache limit.",
        ));
    }
    let limit = limit.unwrap_or(LOCAL_LIMIT).max(1);
    let body = serde_json::to_vec(&HydraRequest {
        title: &query,
        take: REMOTE_LIMIT,
        skip,
    })
    .map_err(|error| CatalogError::new(CatalogErrorKind::Internal, error.to_string()))?;
    let bytes = network.hydra_search(body).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let page = decode(&bytes)?;
        let state = app.state::<DatabaseState>();
        let database = state.database().map_err(CatalogError::database)?;
        let now = now()?;
        store(database, &query, &page, now)?;
        let mut result =
            search_with_options(database, &query, now, limit, options.unwrap_or_default())?;
        let next = skip.saturating_add(page.fetched_count);
        if page.fetched_count > 0
            && next < page.remote_count as usize
            && next < CACHE_LIMIT as usize
        {
            result.next_offset = Some(next);
        }
        Ok(result)
    })
    .await
    .map_err(|error| CatalogError::new(CatalogErrorKind::Internal, error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legio_source::parse_manifest;

    fn test_app(directory: &std::path::Path) -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = format!("org.legio.test.{}", uuid::Uuid::new_v4());
        tauri::test::mock_builder()
            .manage(DatabaseState::new(Ok(directory.to_path_buf())))
            .build(context)
            .unwrap()
    }

    #[tokio::test]
    async fn searches_source_only_titles_without_a_catalog_refresh() {
        let directory =
            std::env::temp_dir().join(format!("legio-catalog-test-{}", uuid::Uuid::new_v4()));
        let app = test_app(&directory);
        let state = app.state::<DatabaseState>();
        let database = state.database().unwrap();
        let manifest = serde_json::json!({
            "schemaVersion": 1, "generatedAt": "2026-10-10T00:00:00Z",
            "verified": [{
                "steamAppId": 700, "name": "Source Exclusive",
                "release": { "version": "1", "publishedAt": "2026-10-10T00:00:00Z" },
                "download": { "url": "https://example.invalid/a.zip", "sizeBytes": 1,
                    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }
            }], "unverified": []
        });
        database.with_connection(|connection| {
            connection.execute(
                "INSERT INTO download_sources (id, url, manifest, fetched_at) VALUES ('source', 'https://example.invalid/store.json', ?1, 100)",
                [serde_json::to_vec(&manifest).unwrap()],
            ).map_err(sql_error)?;
            Ok(())
        }).unwrap();
        let result = search_catalog(app.handle().clone(), "source".into(), Some(1), None)
            .await
            .unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(
            result.games,
            vec![CatalogGame {
                steam_app_id: 700,
                name: "Source Exclusive".into(),
                availability: SourceAvailability::Verified,
            }]
        );
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn filters_and_sorts_before_limiting_results() {
        let directory =
            std::env::temp_dir().join(format!("legio-catalog-test-{}", uuid::Uuid::new_v4()));
        let app = test_app(&directory);
        let state = app.state::<DatabaseState>();
        let database = state.database().unwrap();
        let names = ["Game Alpha", "Game beta", "Game Zulu", "Game", "Other Game"];
        store(
            database,
            "fixture",
            &CatalogPage {
                games: names
                    .iter()
                    .enumerate()
                    .map(|(index, name)| CatalogGame {
                        steam_app_id: 400 + index as u32,
                        name: (*name).into(),
                        availability: SourceAvailability::Unknown,
                    })
                    .collect(),
                remote_count: 5,
                fetched_count: 5,
            },
            100,
        )
        .unwrap();
        let entry = |steam_app_id, name| {
            serde_json::json!({
                "steamAppId": steam_app_id, "name": name,
                "release": { "version": "1", "publishedAt": "2026-10-10T00:00:00Z" },
                "download": { "url": format!("https://example.invalid/{steam_app_id}.zip"), "sizeBytes": 1,
                    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }
            })
        };
        let mut alternate = entry(402, "Game Zulu");
        alternate["download"]["url"] =
            serde_json::json!("https://example.invalid/402-alternate.zip");
        let manifest = serde_json::json!({
            "schemaVersion": 1, "generatedAt": "2026-10-10T00:00:00Z",
            "verified": [entry(402, "Different source name"), entry(405, "Game Omega")],
            "unverified": [entry(401, "Game beta"), alternate]
        });
        database.with_connection(|connection| {
            connection.execute("INSERT INTO download_sources (id, url, manifest, fetched_at) VALUES ('source', 'https://example.invalid/store.json', ?1, 100)", [serde_json::to_vec(&manifest).unwrap()]).map_err(sql_error)?;
            Ok(())
        }).unwrap();
        for (sort, availability, expected, total) in [
            ("relevance", "all", vec![403, 400], 6),
            ("name_asc", "all", vec![403, 400], 6),
            ("name_desc", "all", vec![404, 402], 6),
            ("name_asc", "available", vec![401, 405], 3),
            ("name_desc", "verified", vec![402, 405], 2),
            ("relevance", "verified", vec![405, 402], 2),
        ] {
            let options = serde_json::from_value(
                serde_json::json!({ "sort": sort, "availability": availability }),
            )
            .unwrap();
            let result =
                search_catalog(app.handle().clone(), "game".into(), Some(2), Some(options))
                    .await
                    .unwrap();
            assert_eq!(
                result
                    .games
                    .iter()
                    .map(|game| game.steam_app_id)
                    .collect::<Vec<_>>(),
                expected,
                "{sort}, {availability}"
            );
            assert_eq!(result.total, total, "{sort}, {availability}");
            if let Some(game) = result.games.iter().find(|game| game.steam_app_id == 402) {
                assert_eq!(game.name, "Game Zulu");
                assert_eq!(game.availability, SourceAvailability::Verified);
            }
        }
        let result = search_catalog(
            app.handle().clone(),
            "".into(),
            Some(1),
            Some(CatalogOptions {
                sort: CatalogSort::NameDesc,
                availability: CatalogAvailability::All,
            }),
        )
        .await
        .unwrap();
        assert_eq!(result.games[0].steam_app_id, 404);
        let alias = search_catalog(app.handle().clone(), "different".into(), None, None)
            .await
            .unwrap();
        assert!(
            alias.games.is_empty(),
            "source aliases must not replace a known catalog title"
        );
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn available_filter_without_sources_excludes_unknown_games() {
        let directory =
            std::env::temp_dir().join(format!("legio-catalog-test-{}", uuid::Uuid::new_v4()));
        let app = test_app(&directory);
        let state = app.state::<DatabaseState>();
        store(
            state.database().unwrap(),
            "fixture",
            &CatalogPage {
                games: vec![CatalogGame {
                    steam_app_id: 400,
                    name: "Game".into(),
                    availability: SourceAvailability::Unknown,
                }],
                remote_count: 1,
                fetched_count: 1,
            },
            100,
        )
        .unwrap();
        let result = search_catalog(
            app.handle().clone(),
            "game".into(),
            None,
            Some(CatalogOptions {
                sort: CatalogSort::Relevance,
                availability: CatalogAvailability::Available,
            }),
        )
        .await
        .unwrap();
        assert_eq!(result.total, 0);
        assert!(result.games.is_empty());
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn source_titles_remain_reachable_beyond_the_remote_cache_limit() {
        let directory =
            std::env::temp_dir().join(format!("legio-catalog-test-{}", uuid::Uuid::new_v4()));
        let app = test_app(&directory);
        let state = app.state::<DatabaseState>();
        let database = state.database().unwrap();
        store(
            database,
            "fixture",
            &CatalogPage {
                games: (1..=CACHE_LIMIT)
                    .map(|steam_app_id| CatalogGame {
                        steam_app_id,
                        name: format!("Game {steam_app_id:05}"),
                        availability: SourceAvailability::Unknown,
                    })
                    .collect(),
                remote_count: CACHE_LIMIT,
                fetched_count: CACHE_LIMIT as usize,
            },
            100,
        )
        .unwrap();
        let manifest = serde_json::json!({
            "schemaVersion": 1, "generatedAt": "2026-10-10T00:00:00Z", "verified": [],
            "unverified": [{ "steamAppId": 30000, "name": "ZZZ Source",
                "release": { "version": "1", "publishedAt": "2026-10-10T00:00:00Z" },
                "download": { "url": "https://example.invalid/source.zip", "sizeBytes": 1 } }]
        });
        database.with_connection(|connection| {
            connection.execute("INSERT INTO download_sources (id, url, manifest, fetched_at) VALUES ('source', 'https://example.invalid/store.json', ?1, 100)", [serde_json::to_vec(&manifest).unwrap()]).map_err(sql_error)?;
            Ok(())
        }).unwrap();
        let result = search_catalog(app.handle().clone(), "".into(), Some(CACHE_LIMIT + 1), None)
            .await
            .unwrap();
        assert_eq!(result.total, CACHE_LIMIT + 1);
        assert_eq!(result.games.len(), (CACHE_LIMIT + 1) as usize);
        assert_eq!(result.games.last().unwrap().steam_app_id, 30000);
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn merges_source_by_app_id_without_replacing_catalog_identity() {
        let manifest = parse_manifest(br#"{"schemaVersion":1,"generatedAt":"2026-09-22T00:00:00Z","verified":[{"steamAppId":400,"name":"Source title","release":{"version":"1","publishedAt":"2026-09-22T00:00:00Z"},"download":{"url":"https://example.invalid/a.zip","sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","sizeBytes":1}}],"unverified":[{"steamAppId":401,"name":"Other source title","release":{"version":"1","publishedAt":"2026-09-22T00:00:00Z"},"download":{"url":"https://example.invalid/b.zip","sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","sizeBytes":1}}]}"#).unwrap();
        let games = [400, 401, 402]
            .into_iter()
            .map(|steam_app_id| CatalogGame {
                steam_app_id,
                name: format!("Catalog {steam_app_id}"),
                availability: SourceAvailability::Unknown,
            })
            .collect();
        let search = CatalogSearch {
            games,
            total: 3,
            next_offset: None,
            cached_at: Some(100),
            stale: false,
            source_cached_at: None,
            source_stale: true,
        };
        let merged = merge_source(
            search,
            Some(legio_source_cache::CachedSource {
                manifest: manifest.into(),
                fetched_at: 100,
                warning: None,
            }),
            101,
        );
        assert_eq!(
            merged
                .games
                .iter()
                .map(|game| game.availability)
                .collect::<Vec<_>>(),
            vec![
                SourceAvailability::Verified,
                SourceAvailability::Unverified,
                SourceAvailability::Unavailable
            ]
        );
        assert_eq!(merged.games[0].name, "Catalog 400");
        assert_eq!(merged.source_cached_at, Some(100));
        assert!(!merged.source_stale);
    }

    #[test]
    fn isolates_steam_identity_and_ignores_provider_download_sources() {
        let page = decode(br#"{"count":2,"edges":[{"objectId":"400","title":" Portal ","shop":"steam","downloadSources":[{"url":"https://untrusted.invalid"}]},{"objectId":"not-steam","title":"Other","shop":"gog"}]}"#).unwrap();
        assert_eq!(page.fetched_count, 2);
        assert_eq!(
            page.games,
            vec![CatalogGame {
                steam_app_id: 400,
                name: "Portal".into(),
                availability: SourceAvailability::Unknown,
            }]
        );
        assert_eq!(
            serde_json::to_value(&page.games).unwrap(),
            serde_json::json!([{"steamAppId":400,"name":"Portal","availability":"unknown"}])
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
                availability: SourceAvailability::Unknown,
            }],
            remote_count: 1,
            fetched_count: 1,
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
    fn ranks_cached_search_results_by_query_relevance() {
        let directory =
            std::env::temp_dir().join(format!("legio-catalog-test-{}", uuid::Uuid::new_v4()));
        let state = DatabaseState::new(Ok(directory.clone()));
        let database = state.database().unwrap();
        let names = [
            "Rain World",
            "Risk of Rain 2",
            "Risk of Rain Returns",
            "Rain Risk",
            "The Risk Collection",
            "Portal Knights",
            "Portal 2",
            "Portal 2: Companion Collection",
            "Portals 2",
            "Portal",
            "God of War",
            "War Gods",
        ];
        let page = CatalogPage {
            games: names
                .iter()
                .enumerate()
                .map(|(index, name)| CatalogGame {
                    steam_app_id: 400 + index as u32,
                    name: (*name).to_owned(),
                    availability: SourceAvailability::Unknown,
                })
                .collect(),
            remote_count: names.len() as u32,
            fetched_count: names.len(),
        };
        store(database, "fixture", &page, 100).unwrap();

        for (query, expected) in [
            ("risk rain", "Risk of Rain 2"),
            ("portal 2", "Portal 2"),
            ("port 2", "Portal 2"),
            ("god war", "God of War"),
            ("war god", "War Gods"),
            ("rain risk", "Rain Risk"),
        ] {
            let result = search(database, query, 101).unwrap();
            assert_eq!(
                result.games.first().map(|game| game.name.as_str()),
                Some(expected)
            );
        }

        let risk_games = search(database, "risk rain", 101)
            .unwrap()
            .games
            .into_iter()
            .map(|game| game.name)
            .collect::<Vec<_>>();
        assert_eq!(
            risk_games,
            [
                "Risk of Rain 2",
                "Risk of Rain Returns",
                "Rain Risk",
                "Rain World",
                "The Risk Collection"
            ]
        );
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn app_catalog_search_works_from_cache_without_a_network_client() {
        let directory =
            std::env::temp_dir().join(format!("legio-catalog-test-{}", uuid::Uuid::new_v4()));
        let app = test_app(&directory);
        let page = CatalogPage {
            games: vec![CatalogGame {
                steam_app_id: 400,
                name: "Risk of Rain 2".into(),
                availability: SourceAvailability::Unknown,
            }],
            remote_count: 1,
            fetched_count: 1,
        };
        let fetched_at = now().unwrap();
        store(
            app.state::<DatabaseState>().database().unwrap(),
            "risk rain",
            &page,
            fetched_at,
        )
        .unwrap();

        let result = search_catalog(app.handle().clone(), "risk rain".into(), None, None)
            .await
            .unwrap();
        assert_eq!(result.games, page.games);
        assert_eq!(result.cached_at, Some(fetched_at));
        assert!(!result.stale);
        drop(app);
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
                availability: SourceAvailability::Unknown,
            }],
            remote_count: 1,
            fetched_count: 1,
        };
        store(database, "portal", &original, 100).unwrap();
        let invalid = CatalogPage {
            games: vec![
                CatalogGame {
                    steam_app_id: 400,
                    name: "Changed".into(),
                    availability: SourceAvailability::Unknown,
                },
                CatalogGame {
                    steam_app_id: 0,
                    name: "Invalid".into(),
                    availability: SourceAvailability::Unknown,
                },
            ],
            remote_count: 2,
            fetched_count: 2,
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
                    availability: SourceAvailability::Unknown,
                })
                .collect(),
            remote_count: CACHE_LIMIT + 1,
            fetched_count: (CACHE_LIMIT + 1) as usize,
        };
        store(database, "", &page, 100).unwrap();
        let newer = CatalogPage {
            games: vec![CatalogGame {
                steam_app_id: CACHE_LIMIT + 2,
                name: "Newest".into(),
                availability: SourceAvailability::Unknown,
            }],
            remote_count: 1,
            fetched_count: 1,
        };
        store(database, "newest", &newer, 200).unwrap();
        let result = search(database, "", 201).unwrap();
        assert_eq!(result.total, CACHE_LIMIT);
        assert_eq!(result.games.len(), LOCAL_LIMIT as usize);
        assert_eq!(
            search_with_limit(database, "", 201, 120)
                .unwrap()
                .games
                .len(),
            120
        );
        let ranked = search(database, "Game 19999", 201).unwrap();
        assert_eq!(ranked.games[0].name, "Game 19999");
        assert_eq!(search(database, "newest", 201).unwrap().games, newer.games);
        assert_eq!(result.cached_at, Some(100));
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
