use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::{
    database::{Database, DatabaseState, database_error},
    network::{NetworkError, NetworkState},
};

const CACHE_LIMIT: u32 = 128;
const MAX_DETAILS_BYTES: usize = 64 * 1024;
const STALE_SECONDS: i64 = 24 * 60 * 60;
const MAX_SCREENSHOTS: usize = 8;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SteamDetails {
    pub steam_app_id: u32,
    pub name: String,
    pub app_type: String,
    pub short_description: Option<String>,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub genres: Vec<String>,
    pub platforms: Option<Platforms>,
    pub release_date: Option<ReleaseDate>,
    pub assets: Assets,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Platforms {
    pub windows: bool,
    pub mac: bool,
    pub linux: bool,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDate {
    #[serde(alias = "coming_soon")]
    pub coming_soon: bool,
    pub date: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Assets {
    pub header: Option<String>,
    pub capsule: Option<String>,
    pub background: Option<String>,
    pub screenshots: Vec<Screenshot>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Screenshot {
    pub thumbnail: Option<String>,
    pub full: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailsResult {
    pub details: Option<SteamDetails>,
    pub cached_at: Option<i64>,
    pub stale: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DetailsErrorKind {
    InvalidAppId,
    Unavailable,
    InvalidResponse,
    Timeout,
    Network,
    Http,
    TooLarge,
    Database,
    Internal,
}

#[derive(Debug, Serialize)]
pub struct DetailsError {
    pub kind: DetailsErrorKind,
    pub message: String,
    pub status: Option<u16>,
}

impl DetailsError {
    fn new(kind: DetailsErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            status: None,
        }
    }

    fn database(message: String) -> Self {
        Self::new(DetailsErrorKind::Database, message)
    }
}

impl From<NetworkError> for DetailsError {
    fn from(error: NetworkError) -> Self {
        match error {
            NetworkError::Timeout => Self::new(
                DetailsErrorKind::Timeout,
                "Steam details request timed out.",
            ),
            NetworkError::Transport { message } => Self::new(
                DetailsErrorKind::Network,
                format!("Could not reach Steam: {message}"),
            ),
            NetworkError::Http { status } => Self {
                kind: DetailsErrorKind::Http,
                message: format!("Steam returned HTTP {status}."),
                status: Some(status),
            },
            NetworkError::TooLarge => Self::new(
                DetailsErrorKind::TooLarge,
                "Steam returned a response larger than the 2 MiB limit.",
            ),
        }
    }
}

#[derive(Deserialize)]
struct SteamResponse {
    success: bool,
    data: Option<SteamData>,
}

#[derive(Deserialize)]
struct SteamData {
    steam_appid: u32,
    name: String,
    #[serde(rename = "type")]
    app_type: String,
    short_description: Option<String>,
    #[serde(default)]
    developers: Vec<String>,
    #[serde(default)]
    publishers: Vec<String>,
    #[serde(default)]
    genres: Vec<Genre>,
    platforms: Option<Platforms>,
    release_date: Option<ReleaseDate>,
    header_image: Option<String>,
    capsule_image: Option<String>,
    background: Option<String>,
    #[serde(default)]
    screenshots: Vec<SteamScreenshot>,
}

#[derive(Deserialize)]
struct Genre {
    description: String,
}

#[derive(Deserialize)]
struct SteamScreenshot {
    path_thumbnail: Option<String>,
    path_full: Option<String>,
}

fn validate_app_id(app_id: u32) -> Result<(), DetailsError> {
    if app_id == 0 {
        return Err(DetailsError::new(
            DetailsErrorKind::InvalidAppId,
            "Steam App ID must be a positive 32-bit integer.",
        ));
    }
    Ok(())
}

fn steam_image(value: Option<String>) -> Option<String> {
    value.filter(|value| {
        if value.len() > 2048 || value.chars().any(char::is_control) {
            return false;
        }
        let Ok(url) = reqwest::Url::parse(value) else {
            return false;
        };
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.host_str().is_some_and(|host| {
                [
                    "steamstatic.com",
                    "steamusercontent.com",
                    "steampowered.com",
                ]
                .iter()
                .any(|domain| {
                    host == *domain
                        || host
                            .strip_suffix(domain)
                            .is_some_and(|prefix| prefix.ends_with('.'))
                })
            })
    })
}

fn decode(bytes: &[u8], app_id: u32) -> Result<SteamDetails, DetailsError> {
    let mut response: HashMap<String, SteamResponse> =
        serde_json::from_slice(bytes).map_err(|error| {
            DetailsError::new(
                DetailsErrorKind::InvalidResponse,
                format!("Steam returned invalid appdetails JSON: {error}"),
            )
        })?;
    let entry = response.remove(&app_id.to_string()).ok_or_else(|| {
        DetailsError::new(
            DetailsErrorKind::InvalidResponse,
            "Steam response did not contain the requested App ID.",
        )
    })?;
    if !entry.success {
        return Err(DetailsError::new(
            DetailsErrorKind::Unavailable,
            "Steam has no public details for this App ID in the current region.",
        ));
    }
    let data = entry.data.ok_or_else(|| {
        DetailsError::new(
            DetailsErrorKind::InvalidResponse,
            "Steam reported success without app details.",
        )
    })?;
    if data.steam_appid != app_id
        || data.name.trim().is_empty()
        || data.name.len() > 512
        || data.name.chars().any(char::is_control)
        || data.app_type.is_empty()
        || data.app_type.len() > 64
    {
        return Err(DetailsError::new(
            DetailsErrorKind::InvalidResponse,
            "Steam returned invalid or mismatched app metadata.",
        ));
    }
    let details = SteamDetails {
        steam_app_id: app_id,
        name: data.name,
        app_type: data.app_type,
        short_description: data
            .short_description
            .filter(|value| !value.trim().is_empty()),
        developers: data.developers,
        publishers: data.publishers,
        genres: data
            .genres
            .into_iter()
            .map(|genre| genre.description)
            .collect(),
        platforms: data.platforms,
        release_date: data.release_date,
        assets: Assets {
            header: steam_image(data.header_image),
            capsule: steam_image(data.capsule_image),
            background: steam_image(data.background),
            screenshots: data
                .screenshots
                .into_iter()
                .filter_map(|image| {
                    let image = Screenshot {
                        thumbnail: steam_image(image.path_thumbnail),
                        full: steam_image(image.path_full),
                    };
                    (image.thumbnail.is_some() || image.full.is_some()).then_some(image)
                })
                .take(MAX_SCREENSHOTS)
                .collect(),
        },
    };
    Ok(details)
}

fn now() -> Result<i64, DetailsError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .ok_or_else(|| {
            DetailsError::new(
                DetailsErrorKind::Internal,
                "System clock is outside the supported date range.",
            )
        })
}

fn store(database: &Database, details: &SteamDetails, fetched_at: i64) -> Result<(), DetailsError> {
    let json = serde_json::to_string(details)
        .map_err(|error| DetailsError::new(DetailsErrorKind::Internal, error.to_string()))?;
    if json.len() > MAX_DETAILS_BYTES {
        return Err(DetailsError::new(
            DetailsErrorKind::TooLarge,
            "Selected Steam metadata exceeds the 64 KiB cache entry limit.",
        ));
    }
    database.with_connection(|connection| {
        let transaction = connection.unchecked_transaction().map_err(database_error)?;
        transaction.execute("INSERT INTO steam_details_cache (steam_app_id, details, fetched_at) VALUES (?1, ?2, ?3) ON CONFLICT (steam_app_id) DO UPDATE SET details = excluded.details, fetched_at = excluded.fetched_at", params![details.steam_app_id, json, fetched_at]).map_err(database_error)?;
        transaction.execute("DELETE FROM steam_details_cache WHERE steam_app_id IN (SELECT steam_app_id FROM steam_details_cache WHERE steam_app_id != ?1 ORDER BY fetched_at DESC, steam_app_id LIMIT -1 OFFSET ?2)", params![details.steam_app_id, CACHE_LIMIT - 1]).map_err(database_error)?;
        transaction.commit().map_err(database_error)
    }).map_err(DetailsError::database)
}

fn cached(
    database: &Database,
    app_id: u32,
    current_time: i64,
) -> Result<DetailsResult, DetailsError> {
    let row: Option<(String, i64)> = database
        .with_connection(|connection| {
            connection
                .query_row(
                    "SELECT details, fetched_at FROM steam_details_cache WHERE steam_app_id = ?1",
                    [app_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(database_error)
        })
        .map_err(DetailsError::database)?;
    match row {
        None => Ok(DetailsResult {
            details: None,
            cached_at: None,
            stale: false,
        }),
        Some((json, fetched_at)) => {
            let mut details: SteamDetails = serde_json::from_str(&json).map_err(|error| {
                DetailsError::database(format!("Invalid cached Steam details: {error}"))
            })?;
            if details.steam_app_id != app_id {
                return Err(DetailsError::database(
                    "Cached Steam App ID does not match its cache key.".to_owned(),
                ));
            }
            details.assets.header = steam_image(details.assets.header);
            details.assets.capsule = steam_image(details.assets.capsule);
            details.assets.background = steam_image(details.assets.background);
            details.assets.screenshots.truncate(MAX_SCREENSHOTS);
            for image in &mut details.assets.screenshots {
                image.thumbnail = steam_image(image.thumbnail.take());
                image.full = steam_image(image.full.take());
            }
            Ok(DetailsResult {
                details: Some(details),
                cached_at: Some(fetched_at),
                stale: current_time < fetched_at
                    || current_time.saturating_sub(fetched_at) >= STALE_SECONDS,
            })
        }
    }
}

pub async fn get_details(
    app: tauri::AppHandle,
    network: &NetworkState,
    app_id: u32,
    refresh: bool,
) -> Result<DetailsResult, DetailsError> {
    validate_app_id(app_id)?;
    let bytes = if refresh {
        Some(network.steam_details(app_id).await?)
    } else {
        None
    };
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DatabaseState>();
        let database = state.database().map_err(DetailsError::database)?;
        let time = now()?;
        if let Some(bytes) = bytes {
            let details = decode(&bytes, app_id)?;
            store(database, &details, time)?;
        }
        cached(database, app_id, time)
    })
    .await
    .map_err(|error| DetailsError::new(DetailsErrorKind::Internal, error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(app_id: u32) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({app_id.to_string(): {"success":true,"data":{
            "steam_appid":app_id,"name":"Portal","type":"game",
            "short_description":"A <b>puzzle</b> game", "developers":["Valve"],
            "publishers":["Valve"],"genres":[{"id":"1","description":"Action"}],
            "platforms":{"windows":true,"mac":true,"linux":true},
            "release_date":{"coming_soon":false,"date":"10 Oct, 2007"},
            "header_image":"https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/400/header.jpg",
            "capsule_image":"https://example.com/not-steam.jpg",
            "screenshots":[{"path_full":"https://cdn.akamai.steamstatic.com/steam/apps/400/ss.jpg"}],
            "movies":[{"webm":{"max":"https://example.com/video"}}],
            "downloadSources":[{"url":"https://example.com/download"}]
        }}})).unwrap()
    }

    fn directory() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("legio-details-test-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn selects_metadata_and_only_present_steam_assets() {
        let details = decode(&fixture(400), 400).unwrap();
        assert_eq!(details.name, "Portal");
        assert_eq!(details.genres, ["Action"]);
        assert_eq!(
            details.short_description.as_deref(),
            Some("A <b>puzzle</b> game")
        );
        assert_eq!(details.release_date.unwrap().date, "10 Oct, 2007");
        assert!(details.assets.header.is_some());
        assert_eq!(details.assets.capsule, None);
        assert_eq!(details.assets.background, None);
        assert_eq!(
            details.assets.screenshots,
            [Screenshot {
                thumbnail: None,
                full: Some("https://cdn.akamai.steamstatic.com/steam/apps/400/ss.jpg".into())
            }]
        );
        let minimal = decode(
            br#"{"400":{"success":true,"data":{"steam_appid":400,"name":"Portal","type":"game"}}}"#,
            400,
        )
        .unwrap();
        assert_eq!(
            minimal.assets,
            Assets {
                header: None,
                capsule: None,
                background: None,
                screenshots: vec![]
            }
        );
    }

    #[test]
    fn rejects_unavailable_mismatched_and_malformed_details() {
        assert_eq!(
            validate_app_id(0).unwrap_err().kind,
            DetailsErrorKind::InvalidAppId
        );
        assert_eq!(
            decode(br#"{"400":{"success":false}}"#, 400)
                .unwrap_err()
                .kind,
            DetailsErrorKind::Unavailable
        );
        for bytes in [
            b"not JSON".as_slice(),
            br#"{}"#,
            br#"{"400":{"success":true}}"#,
            br#"{"400":{"success":true,"data":{"steam_appid":401,"name":"Wrong","type":"game"}}}"#,
        ] {
            assert_eq!(
                decode(bytes, 400).unwrap_err().kind,
                DetailsErrorKind::InvalidResponse
            );
        }
    }

    #[test]
    fn rejects_asset_host_spoofing_and_caps_screenshots() {
        for url in [
            "http://cdn.steamstatic.com/a.jpg",
            "https://steamstatic.com.evil.test/a.jpg",
            "https://evilsteamstatic.com/a.jpg",
            "https://user@steamstatic.com/a.jpg",
            "https://steamstatic.com:444/a.jpg",
            "data:image/png,test",
            "//steamstatic.com/a.jpg",
        ] {
            assert_eq!(steam_image(Some(url.into())), None, "{url}");
        }
        let mut value: serde_json::Value = serde_json::from_slice(&fixture(400)).unwrap();
        value["400"]["data"]["screenshots"] = serde_json::json!((0..20).map(|index| serde_json::json!({"path_full":format!("https://cdn.steamstatic.com/{index}.jpg")})).collect::<Vec<_>>());
        let details = decode(&serde_json::to_vec(&value).unwrap(), 400).unwrap();
        assert_eq!(details.assets.screenshots.len(), MAX_SCREENSHOTS);
        assert_eq!(
            details.assets.screenshots.last().unwrap().full.as_deref(),
            Some("https://cdn.steamstatic.com/7.jpg")
        );
    }

    #[test]
    fn persists_cache_and_reports_fresh_stale_missing_and_clock_rollback() {
        let path = directory();
        let details = decode(&fixture(400), 400).unwrap();
        {
            let database = Database::open(&path).unwrap();
            assert!(cached(&database, 400, 100).unwrap().details.is_none());
            store(&database, &details, 100).unwrap();
        }
        let database = Database::open(&path).unwrap();
        let result = cached(&database, 400, 100 + STALE_SECONDS - 1).unwrap();
        assert_eq!(result.details, Some(details));
        assert_eq!(result.cached_at, Some(100));
        assert!(!result.stale);
        assert!(cached(&database, 400, 100 + STALE_SECONDS).unwrap().stale);
        assert!(cached(&database, 400, 99).unwrap().stale);
        assert!(cached(&database, 401, 100).unwrap().details.is_none());
        drop(database);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn evicts_oldest_and_preserves_previous_entry_on_oversized_refresh() {
        let path = directory();
        let database = Database::open(&path).unwrap();
        let mut details = decode(&fixture(1), 1).unwrap();
        for id in 1..=CACHE_LIMIT {
            details.steam_app_id = id;
            store(&database, &details, i64::from(id)).unwrap();
        }
        details.steam_app_id = 1;
        store(&database, &details, 1000).unwrap();
        details.steam_app_id = CACHE_LIMIT + 1;
        store(&database, &details, 1000).unwrap();
        assert!(cached(&database, 2, 1000).unwrap().details.is_none());
        assert!(cached(&database, 1, 1000).unwrap().details.is_some());
        assert!(
            cached(&database, CACHE_LIMIT + 1, 1000)
                .unwrap()
                .details
                .is_some()
        );
        let previous = cached(&database, CACHE_LIMIT + 1, 1000).unwrap().details;
        details.short_description = Some("x".repeat(MAX_DETAILS_BYTES));
        assert_eq!(
            store(&database, &details, 2000).unwrap_err().kind,
            DetailsErrorKind::TooLarge
        );
        assert_eq!(
            cached(&database, CACHE_LIMIT + 1, 2000).unwrap().details,
            previous
        );
        let count: u32 = database
            .with_connection(|connection| {
                connection
                    .query_row("SELECT count(*) FROM steam_details_cache", [], |row| {
                        row.get(0)
                    })
                    .map_err(database_error)
            })
            .unwrap();
        assert_eq!(count, CACHE_LIMIT);
        drop(database);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn upgrades_v3_without_losing_library_records_and_surfaces_corruption() {
        let path = directory();
        let database = Database::open(&path).unwrap();
        let game = database
            .create_game(crate::database::CreateGameInput {
                name: "My game".into(),
                steam_app_id: None,
            })
            .unwrap();
        database
            .with_connection(|connection| {
                connection
                    .execute_batch("DROP TABLE steam_details_cache; PRAGMA user_version = 3;")
                    .map_err(database_error)
            })
            .unwrap();
        drop(database);
        let database = Database::open(&path).unwrap();
        assert_eq!(database.games().unwrap(), vec![game]);
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "INSERT INTO steam_details_cache VALUES (400, 'broken json', 100)",
                        [],
                    )
                    .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            cached(&database, 400, 100).unwrap_err().kind,
            DetailsErrorKind::Database
        );
        store(&database, &decode(&fixture(400), 400).unwrap(), 200).unwrap();
        assert_eq!(
            cached(&database, 400, 200).unwrap().details.unwrap().name,
            "Portal"
        );
        drop(database);
        std::fs::remove_dir_all(path).unwrap();
    }
}
