use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tauri::{Manager, Runtime};

use crate::{
    database::{Database, DatabaseState, database_error},
    network::{NetworkError, NetworkState, is_steam_asset_url},
};

const CACHE_LIMIT: u32 = 128;
const MAX_DETAILS_BYTES: usize = 512 * 1024;
const STALE_SECONDS: i64 = 3 * 24 * 60 * 60;
const MAX_SCREENSHOTS: usize = 8;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SteamDetails {
    pub steam_app_id: u32,
    pub name: String,
    pub app_type: String,
    pub short_description: Option<String>,
    #[serde(default)]
    pub detailed_description: Option<String>,
    #[serde(default)]
    pub system_requirements: Option<SystemRequirements>,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub genres: Vec<String>,
    pub platforms: Option<Platforms>,
    pub release_date: Option<ReleaseDate>,
    pub assets: Assets,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SystemRequirements {
    pub minimum: Option<String>,
    pub recommended: Option<String>,
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
    Cancelled,
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
    detailed_description: Option<String>,
    #[serde(default)]
    pc_requirements: Option<serde_json::Value>,
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
        is_steam_asset_url(&url)
    })
}

// Steam may key appdetails by a package, sub, or DLC id instead of the
// requested application id. Match the payload id first and use the map key
// only as a fallback.
fn take_entry(response: &mut HashMap<String, SteamResponse>, app_id: u32) -> Option<SteamResponse> {
    let key = response
        .iter()
        .find(|(_, entry)| {
            entry
                .data
                .as_ref()
                .is_some_and(|data| data.steam_appid == app_id)
        })
        .map(|(key, _)| key.clone())
        .or_else(|| {
            response
                .keys()
                .find(|key| key.parse::<u32>() == Ok(app_id))
                .cloned()
        })?;
    response.remove(&key)
}

fn decode(bytes: &[u8], app_id: u32) -> Result<SteamDetails, DetailsError> {
    let mut response: HashMap<String, SteamResponse> =
        serde_json::from_slice(bytes).map_err(|error| {
            DetailsError::new(
                DetailsErrorKind::InvalidResponse,
                format!("Steam returned invalid appdetails JSON: {error}"),
            )
        })?;
    let entry = take_entry(&mut response, app_id).ok_or_else(|| {
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
        detailed_description: data
            .detailed_description
            .filter(|value| !value.trim().is_empty()),
        system_requirements: data.pc_requirements.and_then(|value| {
            let value = value.as_object()?;
            let minimum = value
                .get("minimum")
                .and_then(|entry| entry.as_str())
                .filter(|entry| !entry.trim().is_empty())
                .map(str::to_owned);
            let recommended = value
                .get("recommended")
                .and_then(|entry| entry.as_str())
                .filter(|entry| !entry.trim().is_empty())
                .map(str::to_owned);
            (minimum.is_some() || recommended.is_some()).then_some(SystemRequirements {
                minimum,
                recommended,
            })
        }),
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

fn store_localized(
    database: &Database,
    details: &SteamDetails,
    fetched_at: i64,
    language: crate::locale::LanguagePreference,
) -> Result<(), DetailsError> {
    let json = serde_json::to_string(details)
        .map_err(|error| DetailsError::new(DetailsErrorKind::Internal, error.to_string()))?;
    if json.len() > MAX_DETAILS_BYTES {
        return Err(DetailsError::new(
            DetailsErrorKind::TooLarge,
            "Selected Steam metadata exceeds the 512 KiB cache entry limit.",
        ));
    }
    database.with_connection(|connection| {
        let transaction = connection.unchecked_transaction().map_err(database_error)?;
        transaction.execute("INSERT INTO steam_details_cache (steam_app_id, details, fetched_at, language) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (steam_app_id, language) DO UPDATE SET details = excluded.details, fetched_at = excluded.fetched_at", params![details.steam_app_id, json, fetched_at, language.code()]).map_err(database_error)?;
        transaction.execute("DELETE FROM steam_details_cache WHERE language = ?3 AND steam_app_id IN (SELECT steam_app_id FROM steam_details_cache WHERE language = ?3 AND steam_app_id != ?1 ORDER BY fetched_at DESC, steam_app_id LIMIT -1 OFFSET ?2)", params![details.steam_app_id, CACHE_LIMIT - 1, language.code()]).map_err(database_error)?;
        transaction.commit().map_err(database_error)
    }).map_err(DetailsError::database)
}

fn cached_localized(
    database: &Database,
    app_id: u32,
    current_time: i64,
    language: crate::locale::LanguagePreference,
) -> Result<DetailsResult, DetailsError> {
    let row: Option<(String, i64)> = database
        .with_connection(|connection| {
            connection
                .query_row(
                    "SELECT details, fetched_at FROM steam_details_cache WHERE steam_app_id = ?1 AND language = ?2",
                    params![app_id, language.code()],
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
            let cached_value: serde_json::Value = serde_json::from_str(&json).map_err(|error| {
                DetailsError::database(format!("Invalid cached Steam details: {error}"))
            })?;
            let missing_description = cached_value.get("detailedDescription").is_none();
            let missing_requirements = cached_value.get("systemRequirements").is_none();
            let mut details: SteamDetails =
                serde_json::from_value(cached_value).map_err(|error| {
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
                stale: missing_description
                    || missing_requirements
                    || current_time < fetched_at
                    || current_time.saturating_sub(fetched_at) >= STALE_SECONDS,
            })
        }
    }
}

#[cfg(test)]
fn store(database: &Database, details: &SteamDetails, fetched_at: i64) -> Result<(), DetailsError> {
    store_localized(
        database,
        details,
        fetched_at,
        crate::locale::LanguagePreference::English,
    )
}

#[cfg(test)]
fn cached(
    database: &Database,
    app_id: u32,
    current_time: i64,
) -> Result<DetailsResult, DetailsError> {
    cached_localized(
        database,
        app_id,
        current_time,
        crate::locale::LanguagePreference::English,
    )
}

pub(crate) fn cached_details(
    database: &Database,
    app_id: u32,
) -> Result<Option<SteamDetails>, DetailsError> {
    validate_app_id(app_id)?;
    let language = database
        .settings()
        .map_err(DetailsError::database)?
        .language;
    let time = now()?;
    let details = cached_localized(database, app_id, time, language)?.details;
    if details.is_some() {
        return Ok(details);
    }
    let alternate = if language.resolve() == crate::locale::LanguagePreference::Italian {
        crate::locale::LanguagePreference::English
    } else {
        crate::locale::LanguagePreference::Italian
    };
    Ok(cached_localized(database, app_id, time, alternate)?.details)
}

async fn blocking<T, F>(task: F) -> Result<T, DetailsError>
where
    F: FnOnce() -> Result<T, DetailsError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| DetailsError::new(DetailsErrorKind::Internal, error.to_string()))?
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DetailsPolicy {
    CacheOrFetch,
    RefreshIfExpired,
    ForceRefresh,
}
type MetadataLocks = HashMap<(u32, String), std::sync::Weak<tokio::sync::Mutex<()>>>;

#[derive(Default)]
pub struct DetailsFetchState {
    locks: std::sync::Mutex<MetadataLocks>,
    failures: std::sync::Mutex<HashMap<(u32, String), tokio::time::Instant>>,
}
impl DetailsFetchState {
    fn lock(&self, key: &(u32, String)) -> Result<Arc<tokio::sync::Mutex<()>>, DetailsError> {
        let mut locks = self.locks.lock().map_err(|_| {
            DetailsError::new(DetailsErrorKind::Internal, "Metadata scheduler unavailable")
        })?;
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(key).and_then(std::sync::Weak::upgrade) {
            return Ok(lock);
        }
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        locks.insert(key.clone(), Arc::downgrade(&lock));
        Ok(lock)
    }
}
fn should_fetch(previous: &DetailsResult, policy: DetailsPolicy) -> bool {
    previous.details.is_none()
        || policy == DetailsPolicy::ForceRefresh
        || (policy == DetailsPolicy::RefreshIfExpired && previous.stale)
}
#[allow(clippy::too_many_arguments)]
async fn details_with_fetch<F, Fetch, Clock>(
    database: Arc<Database>,
    state: &DetailsFetchState,
    scheduler: &crate::artwork_scheduler::ArtworkScheduler,
    work: &crate::artwork_scheduler::WorkHandle,
    app_id: u32,
    policy: DetailsPolicy,
    language: crate::locale::LanguagePreference,
    mut fetch: Fetch,
    clock: Clock,
) -> Result<DetailsResult, DetailsError>
where
    F: std::future::Future<Output = Result<Vec<u8>, String>>,
    Fetch: FnMut() -> F,
    Clock: Fn() -> Result<i64, DetailsError>,
{
    let time = clock()?;
    let read = |database: Arc<Database>| async move {
        blocking(
            move || match cached_localized(&database, app_id, time, language) {
                Err(error)
                    if error.message.starts_with("Invalid cached Steam details:")
                        || error.message.starts_with("Cached Steam App ID") =>
                {
                    Ok(DetailsResult {
                        details: None,
                        cached_at: None,
                        stale: false,
                    })
                }
                result => result,
            },
        )
        .await
    };
    let previous = read(database.clone()).await?;
    if !should_fetch(&previous, policy) {
        return Ok(previous);
    }
    let key = (app_id, language.code().to_owned());
    let lock = state.lock(&key)?;
    let waiting = lock.try_lock().is_err();
    let _guard = lock.lock().await;
    let previous = read(database.clone()).await?;
    if !should_fetch(&previous, policy)
        || (waiting && previous.details.is_some() && !previous.stale)
    {
        return Ok(previous);
    }
    if policy != DetailsPolicy::ForceRefresh && previous.details.is_some() {
        let failures = state.failures.lock().map_err(|_| {
            DetailsError::new(
                DetailsErrorKind::Internal,
                "Metadata retry state unavailable",
            )
        })?;
        if failures
            .get(&key)
            .is_some_and(|retry| *retry > tokio::time::Instant::now())
        {
            return Ok(previous);
        }
    }
    let fetched = async {
        let bytes =
            scheduler
                .run_metadata(work, &mut fetch)
                .await
                .map_err(|error| match error {
                    crate::artwork_scheduler::WorkError::Cancelled => {
                        DetailsError::new(DetailsErrorKind::Cancelled, "Artwork request cancelled")
                    }
                    crate::artwork_scheduler::WorkError::Failed(message) => {
                        DetailsError::new(DetailsErrorKind::Network, message)
                    }
                })?;
        let fetched_at = clock()?;
        blocking(move || {
            let details = decode(&bytes, app_id)?;
            store_localized(&database, &details, fetched_at, language)?;
            Ok(DetailsResult {
                details: Some(details),
                cached_at: Some(fetched_at),
                stale: false,
            })
        })
        .await
    }
    .await;
    let mut failures = state.failures.lock().map_err(|_| {
        DetailsError::new(
            DetailsErrorKind::Internal,
            "Metadata retry state unavailable",
        )
    })?;
    failures.retain(|_, retry| *retry > tokio::time::Instant::now());
    match fetched {
        Ok(result) => {
            failures.remove(&key);
            Ok(result)
        }
        Err(error) if error.kind == DetailsErrorKind::Cancelled => Err(error),
        Err(error) => {
            if failures.len() >= CACHE_LIMIT as usize {
                failures.clear();
            }
            failures.insert(
                key,
                tokio::time::Instant::now() + std::time::Duration::from_secs(300),
            );
            crate::application_log::failure("artwork_metadata_refresh", &error.message);
            if policy != DetailsPolicy::ForceRefresh && previous.details.is_some() {
                Ok(previous)
            } else {
                Err(error)
            }
        }
    }
}

pub async fn get_details_localized_with_policy<R: Runtime>(
    app: tauri::AppHandle<R>,
    network: &NetworkState,
    app_id: u32,
    policy: DetailsPolicy,
    language: crate::locale::LanguagePreference,
    work: &crate::artwork_scheduler::WorkHandle,
) -> Result<DetailsResult, DetailsError> {
    validate_app_id(app_id)?;
    let database = blocking({
        let app = app.clone();
        move || {
            app.state::<DatabaseState>()
                .shared_database()
                .map_err(DetailsError::database)
        }
    })
    .await?;
    details_with_fetch(
        database,
        app.state::<DetailsFetchState>().inner(),
        app.state::<crate::artwork_scheduler::ArtworkScheduler>()
            .inner(),
        work,
        app_id,
        policy,
        language,
        || async {
            network
                .steam_details(app_id, language)
                .await
                .map_err(|error| {
                    let error = DetailsError::from(error);
                    error.message
                })
        },
        now,
    )
    .await
}

#[cfg(test)]
pub async fn get_details_localized<R: Runtime>(
    app: tauri::AppHandle<R>,
    network: &NetworkState,
    app_id: u32,
    refresh: bool,
    language: crate::locale::LanguagePreference,
) -> Result<DetailsResult, DetailsError> {
    let scheduler = app.state::<crate::artwork_scheduler::ArtworkScheduler>();
    let work = scheduler
        .begin(
            uuid::Uuid::new_v4().to_string(),
            app_id,
            crate::artwork_scheduler::WorkOrigin::Visible,
        )
        .map_err(|error| DetailsError::new(DetailsErrorKind::Internal, error.to_string()))?;
    get_details_localized_with_policy(
        app.clone(),
        network,
        app_id,
        if refresh {
            DetailsPolicy::ForceRefresh
        } else {
            DetailsPolicy::CacheOrFetch
        },
        language,
        &work,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn policy_fixture(
        policy: DetailsPolicy,
        time: i64,
        fail: bool,
        state: &DetailsFetchState,
        database: Arc<Database>,
        calls: &std::sync::atomic::AtomicUsize,
    ) -> Result<DetailsResult, DetailsError> {
        let scheduler = crate::artwork_scheduler::ArtworkScheduler::default();
        let work = scheduler
            .begin(
                uuid::Uuid::new_v4().to_string(),
                400,
                crate::artwork_scheduler::WorkOrigin::Visible,
            )
            .unwrap();
        details_with_fetch(
            database,
            state,
            &scheduler,
            &work,
            400,
            policy,
            crate::locale::LanguagePreference::English,
            || async {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if fail {
                    Err("offline".to_owned())
                } else {
                    Ok(fixture(400))
                }
            },
            || Ok(time),
        )
        .await
    }
    fn policy_database() -> (std::path::PathBuf, Arc<Database>) {
        let path = directory();
        let database = Arc::new(Database::open(&path).unwrap());
        let mut details = decode(&fixture(400), 400).unwrap();
        details.detailed_description = Some("Cached text".into());
        details.system_requirements = Some(SystemRequirements {
            minimum: Some("Cached requirements".into()),
            recommended: None,
        });
        store(&database, &details, 100).unwrap();
        (path, database)
    }
    #[tokio::test]
    async fn stale_list_metadata_returns_without_fetch() {
        let (path, database) = policy_database();
        let state = DetailsFetchState::default();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        for _ in 0..3 {
            let result = policy_fixture(
                DetailsPolicy::CacheOrFetch,
                400_000,
                false,
                &state,
                database.clone(),
                &calls,
            )
            .await
            .unwrap();
            assert_eq!(result.cached_at, Some(100));
        }
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        drop(database);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[tokio::test]
    async fn page_open_refreshes_at_three_day_boundary() {
        let (path, database) = policy_database();
        let state = DetailsFetchState::default();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        policy_fixture(
            DetailsPolicy::RefreshIfExpired,
            259_299,
            false,
            &state,
            database.clone(),
            &calls,
        )
        .await
        .unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        let result = policy_fixture(
            DetailsPolicy::RefreshIfExpired,
            259_300,
            false,
            &state,
            database.clone(),
            &calls,
        )
        .await
        .unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(result.cached_at, Some(259_300));
        drop(database);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[tokio::test]
    async fn failed_refresh_preserves_previous_and_does_not_advance_age() {
        let (path, database) = policy_database();
        let state = DetailsFetchState::default();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        for _ in 0..2 {
            let result = policy_fixture(
                DetailsPolicy::RefreshIfExpired,
                400_000,
                true,
                &state,
                database.clone(),
                &calls,
            )
            .await
            .unwrap();
            assert_eq!(result.cached_at, Some(100));
            assert_eq!(
                result.details.unwrap().detailed_description.as_deref(),
                Some("Cached text")
            );
        }
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(
            policy_fixture(
                DetailsPolicy::ForceRefresh,
                400_000,
                true,
                &state,
                database.clone(),
                &calls
            )
            .await
            .is_err()
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
        drop(database);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[tokio::test]
    async fn future_timestamp_revalidates_only_on_open() {
        let (path, database) = policy_database();
        let state = DetailsFetchState::default();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        policy_fixture(
            DetailsPolicy::CacheOrFetch,
            99,
            false,
            &state,
            database.clone(),
            &calls,
        )
        .await
        .unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        policy_fixture(
            DetailsPolicy::RefreshIfExpired,
            99,
            false,
            &state,
            database.clone(),
            &calls,
        )
        .await
        .unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        drop(database);
        std::fs::remove_dir_all(path).unwrap();
    }

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

    fn test_app(directory: &std::path::Path) -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = format!("org.legio.test.{}", uuid::Uuid::new_v4());
        tauri::test::mock_builder()
            .manage(DatabaseState::new(Ok(directory.to_path_buf())))
            .manage(crate::artwork_scheduler::ArtworkScheduler::default())
            .manage(DetailsFetchState::default())
            .build(context)
            .unwrap()
    }

    #[test]
    fn keeps_language_caches_independent() {
        let root =
            std::env::temp_dir().join(format!("legio-localized-details-{}", uuid::Uuid::new_v4()));
        let database = Database::open(&root).unwrap();
        let mut details = decode(br#"{"400":{"success":true,"data":{"steam_appid":400,"name":"English","type":"game"}}}"#, 400).unwrap();
        store_localized(
            &database,
            &details,
            100,
            crate::locale::LanguagePreference::English,
        )
        .unwrap();
        details.name = "Italian".to_owned();
        store_localized(
            &database,
            &details,
            200,
            crate::locale::LanguagePreference::Italian,
        )
        .unwrap();
        assert_eq!(
            cached_localized(
                &database,
                400,
                201,
                crate::locale::LanguagePreference::English
            )
            .unwrap()
            .details
            .unwrap()
            .name,
            "English"
        );
        assert_eq!(
            cached_localized(
                &database,
                400,
                201,
                crate::locale::LanguagePreference::Italian
            )
            .unwrap()
            .details
            .unwrap()
            .name,
            "Italian"
        );
        drop(database);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fetches_only_when_the_cache_cannot_answer() {
        let fresh = DetailsResult {
            details: Some(decode(&fixture(400), 400).unwrap()),
            cached_at: Some(100),
            stale: false,
        };
        let stale = DetailsResult {
            details: Some(decode(&fixture(400), 400).unwrap()),
            cached_at: Some(100),
            stale: true,
        };
        let missing = DetailsResult {
            details: None,
            cached_at: None,
            stale: false,
        };
        assert!(!should_fetch(&fresh, DetailsPolicy::CacheOrFetch));
        assert!(should_fetch(&fresh, DetailsPolicy::ForceRefresh));
        assert!(should_fetch(&stale, DetailsPolicy::RefreshIfExpired));
        assert!(should_fetch(&missing, DetailsPolicy::CacheOrFetch));
    }

    #[test]
    fn matches_metadata_keyed_by_a_package_id() {
        let mut value: serde_json::Value = serde_json::from_slice(&fixture(2780980)).unwrap();
        let data = value["2780980"]["data"].clone();
        value.as_object_mut().unwrap().clear();
        value["4177100"] = serde_json::json!({ "success": true, "data": data });
        let details = decode(&serde_json::to_vec(&value).unwrap(), 2780980).unwrap();
        assert_eq!(details.steam_app_id, 2780980);
        assert_eq!(details.name, "Portal");
        assert!(details.assets.header.is_some());
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
            br#"{"4177100":{"success":true,"data":{"steam_appid":401,"name":"Wrong","type":"game"}}}"#,
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

    #[tokio::test]
    async fn app_reads_cached_details_without_a_network_request() {
        let path = directory();
        let app = test_app(&path);
        let details = decode(&fixture(400), 400).unwrap();
        let fetched_at = now().unwrap();
        store(
            app.state::<DatabaseState>().database().unwrap(),
            &details,
            fetched_at,
        )
        .unwrap();
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();

        let result = get_details_localized(
            app.handle().clone(),
            &network,
            400,
            false,
            crate::locale::LanguagePreference::English,
        )
        .await
        .unwrap();
        assert_eq!(result.details, Some(details));
        assert_eq!(result.cached_at, Some(fetched_at));
        assert!(!result.stale);
        drop(app);
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
                    .execute_batch("DROP TABLE downloads; DROP TABLE steam_details_cache; DROP TABLE game_native_launch_config; DROP TABLE game_sessions; DROP TABLE game_compatibility_overrides; DROP TABLE compatibility_defaults; DROP INDEX games_executable_path_idx; ALTER TABLE games DROP COLUMN executable_path; ALTER TABLE games DROP COLUMN steam_account_id; PRAGMA user_version = 3;")
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
                        "INSERT INTO steam_details_cache (steam_app_id, details, fetched_at) VALUES (400, 'broken json', 100)",
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
