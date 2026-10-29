use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::{
    Response, StatusCode,
    header::{CONTENT_RANGE, ETAG, HeaderMap, IF_RANGE, RANGE},
};
use rusqlite::{
    OptionalExtension, params,
    types::{FromSql, FromSqlError, ValueRef},
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;
use uuid::Uuid;

use crate::{
    archive_install,
    database::{Database, DatabaseState},
    finalize_install,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadJob {
    id: String,
    steam_app_id: u32,
    name: String,
    release_version: String,
    sha256: Option<String>,
    url: String,
    size_bytes: u64,
    downloaded_bytes: u64,
    speed_bps: u64,
    eta_seconds: Option<u64>,
    status: DownloadStatus,
    error: Option<String>,
    updated_at: i64,
    target_game_id: Option<String>,
    pause_reason: Option<String>,
    automatic_update: bool,
    source_id: Option<String>,
    source_entry_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum DownloadStatus {
    Queued,
    Downloading,
    Waiting,
    Paused,
    Failed,
    Downloaded,
    Staging,
    Staged,
    Finalizing,
    Installed,
    Cancelled,
}

impl DownloadJob {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
}

impl DownloadStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Waiting => "waiting",
            Self::Paused => "paused",
            Self::Failed => "failed",
            Self::Downloaded => "downloaded",
            Self::Staging => "staging",
            Self::Staged => "staged",
            Self::Finalizing => "finalizing",
            Self::Installed => "installed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl FromSql for DownloadStatus {
    fn column_result(value: ValueRef<'_>) -> Result<Self, FromSqlError> {
        match value.as_str()? {
            "queued" => Ok(Self::Queued),
            "downloading" => Ok(Self::Downloading),
            "waiting" => Ok(Self::Waiting),
            "paused" => Ok(Self::Paused),
            "failed" => Ok(Self::Failed),
            "downloaded" => Ok(Self::Downloaded),
            "staging" => Ok(Self::Staging),
            "staged" => Ok(Self::Staged),
            "finalizing" => Ok(Self::Finalizing),
            "installed" => Ok(Self::Installed),
            "cancelled" => Ok(Self::Cancelled),
            value => Err(FromSqlError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unknown download status: {value}"),
            )))),
        }
    }
}

#[derive(Debug, Clone)]
struct Transfer {
    id: String,
    url: String,
    size: u64,
    etag: Option<String>,
}

pub struct DownloadQueueState {
    directory: std::sync::Mutex<PathBuf>,
    client: reqwest::Client,
    running: AtomicBool,
    starting: AtomicBool,
    active: std::sync::Mutex<Option<(String, tokio::sync::watch::Sender<bool>)>>,
    bandwidth_limit: AtomicU64,
    wake: tokio::sync::Notify,
    limit_changed: tokio::sync::Notify,
    pub(crate) policy_changes: tokio::sync::watch::Sender<()>,
    pub(crate) staging: tokio::sync::Mutex<()>,
    defer_extraction: AtomicBool,
    staging_changed: tokio::sync::Notify,
    processing: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
}

impl DownloadQueueState {
    pub fn new(data_dir: Result<PathBuf, tauri::Error>, version: &str) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(format!("Legio/{version}"))
            .build()
            .map_err(|error| format!("Could not initialize download client: {error}"))?;
        Ok(Self {
            directory: std::sync::Mutex::new(
                data_dir
                    .map(|path| path.join("downloads"))
                    .map_err(|error| format!("Could not locate download directory: {error}"))?,
            ),
            client,
            running: AtomicBool::new(false),
            starting: AtomicBool::new(false),
            active: std::sync::Mutex::new(None),
            bandwidth_limit: AtomicU64::new(0),
            wake: tokio::sync::Notify::new(),
            limit_changed: tokio::sync::Notify::new(),
            policy_changes: tokio::sync::watch::channel(()).0,
            staging: tokio::sync::Mutex::new(()),
            defer_extraction: AtomicBool::new(false),
            staging_changed: tokio::sync::Notify::new(),
            processing: std::sync::Arc::default(),
        })
    }

    pub(crate) fn set_defer_extraction(&self, enabled: bool) {
        self.defer_extraction.store(enabled, Ordering::Release);
        self.staging_changed.notify_waiters();
        self.policy_changes.send_replace(());
    }

    #[cfg(test)]
    pub(crate) async fn wait_for_idle(
        &self,
        manager: &crate::game_lifecycle::GameLaunchManager,
    ) -> Result<(), String> {
        let mut changes = manager.subscribe_changes();
        loop {
            let settings_changed = self.staging_changed.notified();
            tokio::pin!(settings_changed);
            settings_changed.as_mut().enable();
            changes.borrow_and_update();
            if !self.defer_extraction.load(Ordering::Acquire)
                || !manager
                    .list()?
                    .iter()
                    .any(|state| state.status != crate::game_lifecycle::GameStatus::Idle)
            {
                return Ok(());
            }
            tokio::select! {
                result = changes.changed() => result.map_err(|_| "Game activity notifications stopped".to_owned())?,
                () = settings_changed => {},
            }
        }
    }

    fn directory(&self) -> Result<PathBuf, String> {
        let path = self
            .directory
            .lock()
            .map_err(|_| "Download directory state is unavailable".to_owned())?
            .clone();
        fs::create_dir_all(&path)
            .map_err(|error| format!("Could not create download directory: {error}"))?;
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("Could not inspect download directory: {error}"))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("Download directory is not a regular directory".to_owned());
        }
        Ok(path)
    }

    pub(crate) fn set_storage_root(&self, root: &Path) -> Result<(), String> {
        let mut directory = self
            .directory
            .lock()
            .map_err(|_| "Download directory state is unavailable".to_owned())?;
        *directory = root.join("downloads");
        Ok(())
    }

    pub(crate) fn set_bandwidth_limit(&self, bytes_per_second: u64) {
        self.bandwidth_limit
            .store(bytes_per_second, Ordering::Relaxed);
        self.limit_changed.notify_one();
    }

    pub(crate) fn interrupt(&self, id: &str) -> Result<(), String> {
        let active = self
            .active
            .lock()
            .map_err(|_| "Download control lock was poisoned".to_owned())?;
        if let Some((active_id, stopped)) = active.as_ref()
            && active_id == id
        {
            stopped.send_replace(true);
        }
        Ok(())
    }

    fn path(&self, id: &str, extension: &str) -> Result<PathBuf, String> {
        let id = Uuid::parse_str(id).map_err(|_| "Download ID is invalid".to_owned())?;
        Ok(self.directory()?.join(format!("{id}.{extension}")))
    }
}

fn now() -> Result<i64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok())
        .ok_or_else(|| "System clock is outside the supported date range".to_owned())
}

fn db_error(error: rusqlite::Error) -> String {
    format!("Local download database error: {error}")
}

fn job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DownloadJob> {
    let hash: String = row.get(10)?;
    let updated_at: i64 = row.get(12)?;
    let app_id: i64 = row.get(1)?;
    let size: i64 = row.get(4)?;
    let downloaded: i64 = row.get(5)?;
    let speed: i64 = row.get(6)?;
    let eta: Option<i64> = row.get(7)?;
    Ok(DownloadJob {
        id: row.get(0)?,
        steam_app_id: u32::try_from(app_id)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(1, app_id))?,
        name: row.get(2)?,
        release_version: row.get(3)?,
        sha256: (!hash.is_empty()).then_some(hash),
        url: row.get(11)?,
        size_bytes: u64::try_from(size)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(4, size))?,
        downloaded_bytes: u64::try_from(downloaded)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(5, downloaded))?,
        speed_bps: u64::try_from(speed)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(6, speed))?,
        eta_seconds: eta
            .map(|value| {
                u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(7, value))
            })
            .transpose()?,
        status: row.get(8)?,
        error: row.get(9)?,
        updated_at,
        target_game_id: row.get(13)?,
        pause_reason: row.get(14)?,
        automatic_update: row.get(15)?,
        source_id: row.get(16)?,
        source_entry_id: row.get(17)?,
    })
}

fn list(database: &Database) -> Result<Vec<DownloadJob>, String> {
    database.with_connection(|connection| {
        let mut statement = connection
            .prepare(
                "SELECT id, steam_app_id, name, release_version, size_bytes, downloaded_bytes,
                    speed_bps, eta_seconds, status, error, sha256, url, updated_at, target_game_id, pause_reason, automatic_update, source_id, source_entry_id
                 FROM downloads ORDER BY queue_position, created_at, id",
            )
            .map_err(db_error)?;
        statement
            .query_map([], job_from_row)
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)
    })
}

fn change_status(
    database: &Database,
    id: &str,
    from: &[DownloadStatus],
    to: DownloadStatus,
) -> Result<(), String> {
    let id = Uuid::parse_str(id)
        .map_err(|_| "Download ID is invalid".to_owned())?
        .to_string();
    let updated = now()?;
    database.with_connection(|connection| {
        let status: Option<String> = connection.query_row(
            "SELECT status FROM downloads WHERE id = ?1", [&id], |row| row.get(0),
        ).optional().map_err(db_error)?;
        let Some(status) = status else { return Err("Download was not found".to_owned()); };
        if !from.iter().any(|candidate| candidate.as_str() == status) {
            return Err(format!("Cannot {} a {status} download", to.as_str()));
        }
        connection.execute(
            "UPDATE downloads SET status = ?2, error = NULL, speed_bps = 0, eta_seconds = NULL, updated_at = ?3 WHERE id = ?1",
            params![id, to.as_str(), updated],
        ).map_err(db_error)?;
        Ok(())
    })
}

#[cfg(test)]
pub fn enqueue(
    database: &Database,
    app_id: u32,
    download_url: &str,
    release_version: &str,
    accept_unverified: bool,
) -> Result<DownloadJob, String> {
    enqueue_from_source(
        database,
        app_id,
        download_url,
        release_version,
        accept_unverified,
        None,
    )
}

#[cfg(test)]
pub(crate) fn enqueue_from_source(
    database: &Database,
    app_id: u32,
    download_url: &str,
    release_version: &str,
    accept_unverified: bool,
    source_id: Option<&str>,
) -> Result<DownloadJob, String> {
    enqueue_with_target(
        database,
        app_id,
        download_url,
        release_version,
        accept_unverified,
        source_id,
        None,
    )
}

pub(crate) fn enqueue_with_target(
    database: &Database,
    app_id: u32,
    download_url: &str,
    release_version: &str,
    accept_unverified: bool,
    source_id: Option<&str>,
    update: Option<(&str, bool)>,
) -> Result<DownloadJob, String> {
    enqueue_prepared(
        database,
        app_id,
        download_url,
        release_version,
        accept_unverified,
        source_id,
        update,
        None,
        PlanChoice::Derived,
    )
}

/// How `enqueue_prepared` fills the mirror plan stored with the new row.
pub(crate) enum PlanChoice {
    /// Build a plan from the source's supported candidates.
    Derived,
    /// Store this already checked plan, or none for a single native CDN link.
    Given(Option<crate::download_selection::MirrorPlan>),
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn enqueue_prepared(
    database: &Database,
    app_id: u32,
    download_url: &str,
    release_version: &str,
    accept_unverified: bool,
    source_id: Option<&str>,
    update: Option<(&str, bool)>,
    resolved_size: Option<u64>,
    plan: PlanChoice,
) -> Result<DownloadJob, String> {
    let (source_id, entry, source_verified) = crate::game_updates::find_download(
        database,
        app_id,
        download_url,
        release_version,
        source_id,
    )?;
    if !source_verified && !accept_unverified {
        return Err("Confirm the unverified source before downloading".to_owned());
    }
    if let Some(actual) = resolved_size
        && !crate::download_selection::size_matches(entry.download.size_bytes, actual)
    {
        return Err("Download does not match the size published by the source".into());
    }
    let source_rows = crate::legio_source_cache::cached_rows(database)?;
    let source = source_rows.iter().find(|source| source.id == source_id);
    let exact_size = resolved_size
        .or(crate::link_check::cached_size(database, download_url)?)
        .or(entry.download.size_bytes)
        .filter(|size| *size > 0)
        .ok_or("The host did not provide a download size")?;
    let size = i64::try_from(exact_size)
        .map_err(|_| "Download size exceeds local storage limits".to_owned())?;
    let published_size = entry
        .download
        .size_bytes
        .map(i64::try_from)
        .transpose()
        .map_err(|_| "Published download size exceeds local storage limits".to_owned())?;
    let id = Uuid::new_v4().to_string();
    let created = now()?;

    let mirror_plan = match plan {
        PlanChoice::Given(plan) => plan,
        PlanChoice::Derived => {
            let mut uris = source
                .as_ref()
                .and_then(|source| {
                    entry
                        .id
                        .as_ref()
                        .and_then(|id| source.catalog.candidates.get(id))
                })
                .cloned()
                .unwrap_or_else(|| vec![entry.download.url.clone()]);
            // Unsupported links can never be downloaded; keep them only when
            // nothing else is left so the job still fails with its own error.
            if uris
                .iter()
                .any(|uri| crate::download_selection::supported(uri))
            {
                uris.retain(|uri| crate::download_selection::supported(uri));
            }
            let hydra = source.as_ref().is_some_and(|source| {
                source.catalog.format == crate::source_catalog::SourceFormat::Hydra
            });
            if uris.len() == 1 && !hydra && crate::download_selection::native_cdn(&uris[0]) {
                None
            } else {
                Some(crate::download_selection::MirrorPlan::new(
                    uris,
                    if hydra {
                        None
                    } else {
                        entry.download.size_bytes
                    },
                ))
            }
        }
    }
    .map(|plan| serde_json::to_string(&plan).map_err(|error| error.to_string()))
    .transpose()?;
    database.with_connection(|connection| {
        connection
            .execute(
                "INSERT INTO downloads (id, steam_app_id, name, release_version, url, sha256,
             size_bytes, status, created_at, updated_at, source_verified, queue_position, source_id, source_entry_id, target_game_id, automatic_update, mirror_plan, published_size_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'queued', ?8, ?8, ?9, ?8, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    id,
                    app_id,
                    entry.name,
                    entry.release.version,
                    entry.download.url,
                    entry.download.sha256.as_deref().unwrap_or(""),
                    size,
                    created,
                    source_verified, source_id, entry.id, update.map(|value| value.0), update.is_some_and(|value| value.1), mirror_plan, published_size
                ],
            )
            .map_err(db_error)?;
        Ok(())
    })?;
    Ok(DownloadJob {
        id,
        steam_app_id: app_id,
        name: entry.name.clone(),
        release_version: entry.release.version.clone(),
        sha256: entry.download.sha256.clone(),
        url: entry.download.url.clone(),
        size_bytes: exact_size,
        downloaded_bytes: 0,
        speed_bps: 0,
        eta_seconds: None,
        status: DownloadStatus::Queued,
        error: None,
        updated_at: created,
        target_game_id: update.map(|value| value.0.to_owned()),
        pause_reason: None,
        automatic_update: update.is_some_and(|value| value.1),
        source_id: Some(source_id),
        source_entry_id: entry.id.clone(),
    })
}

fn claim(database: &Database) -> Result<Option<Transfer>, String> {
    database.with_connection(|connection| {
        let row: Option<(String, String, i64, Option<String>)> = connection.query_row(
            "SELECT id, url, size_bytes, etag FROM downloads WHERE status = 'queued' ORDER BY queue_position, created_at, id LIMIT 1",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).optional().map_err(db_error)?;
        let Some((id, url, size, etag)) = row else { return Ok(None); };
        connection.execute(
            "UPDATE downloads SET status = 'downloading', updated_at = ?2 WHERE id = ?1",
            params![id, now()?],
        ).map_err(db_error)?;
        Ok(Some(Transfer { id, url, size: u64::try_from(size).map_err(|_| "Invalid stored download size".to_owned())?, etag }))
    })
}

fn status(database: &Database, id: &str) -> Result<String, String> {
    database.with_connection(|connection| {
        connection
            .query_row("SELECT status FROM downloads WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .map_err(db_error)
    })
}

fn update_progress(
    database: &Database,
    id: &str,
    bytes: u64,
    speed: u64,
    remaining: Option<u64>,
    etag: Option<&str>,
) -> Result<(), String> {
    let bytes =
        i64::try_from(bytes).map_err(|_| "Download exceeds local storage limits".to_owned())?;
    let speed = i64::try_from(speed).unwrap_or(i64::MAX);
    let remaining = remaining.and_then(|value| i64::try_from(value).ok());
    database.with_connection(|connection| {
        connection
            .execute(
                "UPDATE downloads SET downloaded_bytes = ?2, speed_bps = ?3, eta_seconds = ?4,
             etag = ?5, updated_at = ?6 WHERE id = ?1 AND status = 'downloading'",
                params![id, bytes, speed, remaining, etag, now()?],
            )
            .map_err(db_error)?;
        Ok(())
    })
}

fn finish(
    database: &Database,
    id: &str,
    to: DownloadStatus,
    error: Option<String>,
) -> Result<(), String> {
    database.with_connection(|connection| {
        connection
            .execute(
                "UPDATE downloads SET status = ?2, error = ?3, speed_bps = 0, eta_seconds = NULL,
             updated_at = ?4 WHERE id = ?1 AND status = 'downloading'",
                params![id, to.as_str(), error, now()?],
            )
            .map_err(db_error)?;
        Ok(())
    })
}

fn recover(database: &Database) -> Result<(), String> {
    database.with_connection(|connection| {
        connection.execute(
            "UPDATE downloads SET status = 'queued', speed_bps = 0, eta_seconds = NULL WHERE status = 'downloading'",
            [],
        ).map_err(db_error)?;
        Ok(())
    })
}

fn remove_stage(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path)
                .map_err(|error| format!("Could not remove staging directory: {error}"))
        }
        Ok(_) => Err(format!(
            "Staging path is not a regular directory: {}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not inspect staging directory: {error}")),
    }
}

fn stage_one(queue: &DownloadQueueState, database: &Database, id: &str) -> Result<PathBuf, String> {
    let id = Uuid::parse_str(id)
        .map_err(|_| "Download ID is invalid".to_owned())?
        .to_string();
    let (hash, verified) = database.with_connection(|connection| {
        let row: Option<(String, String, bool)> = connection
            .query_row(
                "SELECT sha256, status, source_verified FROM downloads WHERE id = ?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(db_error)?;
        let Some((hash, status, verified)) = row else {
            return Err("Download was not found".into());
        };
        if status != "downloaded" {
            return Err(format!("Cannot stage a {status} download"));
        }
        connection.execute(
            "UPDATE downloads SET status = 'staging', error = NULL, updated_at = ?2 WHERE id = ?1",
            params![id, now()?],
        ).map_err(db_error)?;
        Ok((hash, verified))
    })?;

    let outcome: Result<PathBuf, String> = (|| {
        if verified && hash.is_empty() {
            return Err("SHA-256 is required for verified downloads".into());
        }
        let verify = if verified {
            database.settings()?.verify_verified_downloads
        } else {
            !hash.is_empty()
        };
        let archive = queue.path(&id, "archive")?;
        let stage = queue.path(&id, "stage")?;
        remove_stage(&stage)?;
        archive_install::verify_and_stage(&archive, verify.then_some(hash.as_str()), &stage)?;
        Ok(stage)
    })();
    match outcome {
        Ok(stage) => {
            let persisted = database.with_connection(|connection| {
                connection.execute(
                    "UPDATE downloads SET status = 'staged', staged_path = ?2, updated_at = ?3 WHERE id = ?1 AND status = 'staging'",
                    params![id, stage.to_string_lossy(), now()?],
                ).map_err(db_error)?;
                Ok(())
            });
            if let Err(error) = persisted {
                return match remove_stage(&stage) {
                    Ok(()) => Err(format!(
                        "Could not save staged download: {error}. Retry staging"
                    )),
                    Err(cleanup) => Err(format!(
                        "Could not save staged download: {error}; {cleanup}"
                    )),
                };
            }
            Ok(stage)
        }
        Err(error) => {
            database.with_connection(|connection| {
                connection.execute(
                    "UPDATE downloads SET status = 'failed', error = ?2, staged_path = NULL, updated_at = ?3 WHERE id = ?1 AND status = 'staging'",
                    params![id, error, now()?],
                ).map_err(db_error)?;
                Ok(())
            }).map_err(|db| format!("{error}; could not save failure: {db}"))?;
            Err(error)
        }
    }
}

struct ProcessingReservation {
    ids: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    id: String,
}
impl Drop for ProcessingReservation {
    fn drop(&mut self) {
        self.ids
            .lock()
            // Removing this entry is safe even after a panic poisoned the lock; keeping it would block the item forever.
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.id);
    }
}

fn completion_notification(
    database: &Database,
    id: &str,
) -> Result<Option<(String, String)>, String> {
    let settings = database.settings()?;
    if !settings.download_notifications {
        return Ok(None);
    }
    database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT name FROM downloads WHERE id = ?1 AND status = 'installed'",
                [id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(db_error)
            .map(|name| {
                name.map(|name| {
                    (
                        settings
                            .language
                            .text("Download completato", "Download complete")
                            .to_owned(),
                        name,
                    )
                })
            })
    })
}

fn spawn_staging<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let queue = app.state::<DownloadQueueState>();
    let Ok(mut processing) = queue.processing.lock() else {
        crate::application_log::failure("schedule_installation", "Processing lock was poisoned");
        return;
    };
    if !processing.insert(id.to_owned()) {
        return;
    }
    let reservation = ProcessingReservation {
        ids: queue.processing.clone(),
        id: id.to_owned(),
    };
    drop(processing);
    let app = app.clone();
    let id = id.to_owned();
    tauri::async_runtime::spawn(async move {
        let _reservation = reservation;
        let queue = app.state::<DownloadQueueState>();
        let _staging = queue.staging.lock().await;
        let worker = app.clone();
        let job_id = id.clone();
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            let database = worker.state::<DatabaseState>();
            let database = database.database()?;
            if let Some(manager) = worker.try_state::<crate::game_lifecycle::GameLaunchManager>()
                && !crate::download_policy::processable(database, &manager, &job_id)?
            {
                return Ok(());
            }
            if status(database, &job_id)? == "downloaded" {
                stage_one(&worker.state::<DownloadQueueState>(), database, &job_id)?;
            }
            if status(database, &job_id)? != "staged" {
                return Ok(());
            }
            let downloads = worker.state::<DownloadQueueState>().directory()?;
            let root = downloads
                .parent()
                .ok_or("Download directory has no storage root")?
                .to_path_buf();
            let target = crate::game_updates::target(database, &job_id)?;
            let manager = worker.try_state::<crate::game_lifecycle::GameLaunchManager>();
            let _reservation =
                if let (Some(manager), Some(game)) = (manager.as_ref(), target.as_ref()) {
                    let _operation = manager.operation()?;
                    if crate::download_policy::busy(database, manager, game)? {
                        return Ok(());
                    }
                    Some(manager.reserve_transfer(game)?)
                } else {
                    None
                };
            let game_id = finalize_install::automatic(database, &root, &job_id)?;
            crate::game_library::sync_storage(database, &root)?;
            finalize_install::create_install_shortcut(&worker, database, &game_id)?;
            match completion_notification(database, &job_id) {
                Ok(Some((title, name))) => {
                    if let Err(error) = worker
                        .notification()
                        .builder()
                        .title(title)
                        .body(name)
                        .show()
                    {
                        crate::application_log::failure("show_download_notification", error);
                    }
                }
                Ok(None) => {}
                Err(error) => crate::application_log::failure("read_download_notification", error),
            }
            Ok::<_, String>(())
        })
        .await;
        notify(&app);
        if !matches!(outcome, Ok(Ok(()))) {
            let error = match outcome {
                Ok(Err(error)) => error,
                Err(error) => format!("Installation task failed: {error}"),
                Ok(Ok(())) => return,
            };
            let worker = app.clone();
            let job_id = id.clone();
            let saved_error = error.clone();
            let persisted = tauri::async_runtime::spawn_blocking(move || {
                worker.state::<DatabaseState>().database()?.with_connection(|connection| {
                    connection.execute("UPDATE downloads SET error = ?2, status = CASE WHEN status = 'staging' THEN 'failed' ELSE status END WHERE id = ?1", params![job_id,saved_error]).map_err(db_error)?; Ok(())
                })
            }).await;
            crate::application_log::failure(
                "automatic_game_installation",
                format!("{error}; failure persistence: {persisted:?}"),
            );
        }
    });
}

pub(crate) fn schedule_installations<R: Runtime>(app: &AppHandle<R>) {
    let ids = app.state::<DatabaseState>().database().and_then(|database| database.with_connection(|connection| {
        let mut query = connection.prepare("SELECT id FROM downloads WHERE status IN ('downloaded','staged') AND error IS NULL ORDER BY queue_position,created_at,id").map_err(db_error)?;
        query.query_map([], |row| row.get::<_, String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)
    }));
    match ids {
        Ok(ids) => {
            for id in ids {
                spawn_staging(app, &id);
            }
        }
        Err(error) => crate::application_log::failure("schedule_installations", error),
    }
}

fn recover_staging(queue: &DownloadQueueState, database: &Database) -> Result<(), String> {
    let ids: Vec<String> = database.with_connection(|connection| {
        let mut statement = connection
            .prepare("SELECT id FROM downloads WHERE status = 'staging'")
            .map_err(db_error)?;
        statement
            .query_map([], |row| row.get(0))
            .map_err(db_error)?
            .collect::<Result<_, _>>()
            .map_err(db_error)
    })?;
    for id in ids {
        let recovered = (|| {
            let stage = queue.path(&id, "stage")?;
            let archive = queue.path(&id, "archive")?;
            remove_stage(&stage)?;
            Ok::<_, String>(if archive.is_file() {
                "downloaded"
            } else {
                "queued"
            })
        })();
        let (next, error) = match recovered {
            Ok(next) => (next, None),
            Err(error) => ("failed", Some(error)),
        };
        database.with_connection(|connection| {
            connection.execute(
                "UPDATE downloads SET status = ?2, error = ?3, staged_path = NULL, updated_at = ?4 WHERE id = ?1 AND status = 'staging'",
                params![id, next, error, now()?],
            ).map_err(db_error)?;
            Ok(())
        })?;
    }
    Ok(())
}

fn remove_partial(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn cancellation_error(database: &Database, id: &str, error: &str) -> Result<(), String> {
    database.with_connection(|connection| {
        connection.execute(
            "UPDATE downloads SET error = ?2, updated_at = ?3 WHERE id = ?1 AND status = 'cancelled'",
            params![id, error, now()?],
        ).map_err(db_error)?;
        Ok(())
    })
}

/// A part file still open by an active transfer may refuse removal; the
/// transfer removes it itself when it stops.
fn remove_cancelled_files(
    queue: &DownloadQueueState,
    id: &str,
    was_downloading: bool,
) -> Result<(), String> {
    if let Err(error) = remove_partial(&queue.path(id, "part")?)
        && (error.kind() != std::io::ErrorKind::PermissionDenied || !was_downloading)
    {
        return Err(format!("Could not remove partial download: {error}"));
    }
    remove_partial(&queue.path(id, "archive")?)
        .map_err(|error| format!("Could not remove download files: {error}"))?;
    finalize_install::remove_stage_directory(&queue.path(id, "stage")?)
}

fn clean_cancelled(queue: &DownloadQueueState, database: &Database) -> Result<(), String> {
    let ids: Vec<String> = database.with_connection(|connection| {
        let mut statement = connection
            .prepare("SELECT id FROM downloads WHERE status = 'cancelled'")
            .map_err(db_error)?;
        statement
            .query_map([], |row| row.get(0))
            .map_err(db_error)?
            .collect::<Result<_, _>>()
            .map_err(db_error)
    })?;
    for id in ids {
        if let Err(error) = remove_cancelled_files(queue, &id, false) {
            cancellation_error(database, &id, &error)?;
        }
    }
    Ok(())
}

fn has_status(database: &Database, status: DownloadStatus) -> Result<bool, String> {
    database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM downloads WHERE status = ?1)",
                [status.as_str()],
                |row| row.get(0),
            )
            .map_err(db_error)
    })
}

/// True while any job can still progress or finish installing. Failed jobs and
/// extraction or installation errors are left for the user and do not count.
pub(crate) fn has_unfinished_work(
    queue: &DownloadQueueState,
    database: &Database,
) -> Result<bool, String> {
    let processing = !queue
        .processing
        .lock()
        .map_err(|_| "Processing lock was poisoned".to_owned())?
        .is_empty();
    if processing {
        return Ok(true);
    }
    database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM downloads WHERE status IN ('queued','downloading','waiting') OR (status = 'paused' AND pause_reason IS NOT 'automatic_disabled') OR (status IN ('downloaded','staging','staged','finalizing') AND error IS NULL))",
                [],
                |row| row.get(0),
            )
            .map_err(db_error)
    })
}

fn wake_waiting(database: &Database) -> Result<bool, String> {
    database.with_connection(|connection| {
        let count = connection.execute(
            "UPDATE downloads SET status = 'queued', error = NULL, updated_at = ?1 WHERE status = 'waiting'",
            [now()?],
        ).map_err(db_error)?;
        Ok(count > 0)
    })
}

pub(crate) async fn resume_waiting<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let database_app = app.clone();
    let resumed = tauri::async_runtime::spawn_blocking(move || {
        wake_waiting(database_app.state::<DatabaseState>().database()?)
    })
    .await
    .map_err(|error| format!("Waiting download resume task failed: {error}"))??;
    if resumed {
        kick(app);
    }
    Ok(())
}

pub(crate) fn notify<R: Runtime>(app: &AppHandle<R>) {
    if let Some(queue) = app.try_state::<DownloadQueueState>() {
        queue.policy_changes.send_replace(());
    }
    if let Err(error) = app.emit("downloads-changed", ()) {
        crate::application_log::failure("publish_download_state", error);
    }
}

pub(crate) fn kick<R: Runtime>(app: AppHandle<R>) {
    let queue = app.state::<DownloadQueueState>();
    if queue.starting.load(Ordering::Acquire) {
        return;
    }
    queue.wake.notify_one();
    queue.policy_changes.send_replace(());
    if !queue.running.load(Ordering::Acquire)
        && !app
            .state::<DatabaseState>()
            .database()
            .and_then(|database| {
                Ok(has_status(database, DownloadStatus::Queued)?
                    || has_status(database, DownloadStatus::Waiting)?)
            })
            .unwrap_or_else(|error| {
                crate::application_log::failure("check_download_queue", error);
                false
            })
    {
        return;
    }
    if queue.running.swap(true, Ordering::AcqRel) {
        return;
    }
    notify(&app);
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_queue(&app).await {
            crate::application_log::failure("download_queue_stopped", error);
        }
        app.state::<DownloadQueueState>()
            .running
            .store(false, Ordering::Release);
        // A command may enqueue after the last claim and before the flag is cleared.
        match app
            .state::<DatabaseState>()
            .database()
            .and_then(|database| has_status(database, DownloadStatus::Queued))
        {
            Ok(true) => kick(app),
            Ok(false) => {}
            Err(error) => crate::application_log::failure("restart_download_queue", error),
        }
    });
}

async fn run_queue<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    loop {
        let database = app.state::<DatabaseState>().shared_database()?;
        let reader = database.clone();
        let policy_app = app.clone();
        let (job, waiting) = tauri::async_runtime::spawn_blocking(move || {
            if let Some(manager) =
                policy_app.try_state::<crate::game_lifecycle::GameLaunchManager>()
            {
                crate::download_policy::sync(
                    &reader,
                    &manager,
                    &policy_app.state::<DownloadQueueState>(),
                )?;
            }
            let job = claim(&reader)?;
            let waiting = job.is_none() && has_status(&reader, DownloadStatus::Waiting)?;
            Ok::<_, String>((job, waiting))
        })
        .await
        .map_err(|error| format!("Download queue task failed: {error}"))??;
        let Some(job) = job else {
            if !waiting {
                break;
            }
            let queue = app.state::<DownloadQueueState>();
            queue.wake.notified().await;
            continue;
        };
        let network = app.try_state::<crate::network::NetworkState>();
        let outcome = transfer(
            &app.state::<DownloadQueueState>(),
            app.state::<DatabaseState>().shared_database()?,
            &job,
            || is_online(network.as_deref()),
        )
        .await;
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            match outcome {
                Ok(()) => {
                    finish(&database, &job.id, DownloadStatus::Downloaded, None)?;
                    spawn_staging(&app, &job.id);
                }
                Err(error) => {
                    if status(&database, &job.id)? == "downloading" {
                        let waiting =
                            matches!(error, TransferError::Network(_) | TransferError::Server(_));
                        finish(
                            &database,
                            &job.id,
                            if waiting {
                                DownloadStatus::Waiting
                            } else {
                                DownloadStatus::Failed
                            },
                            Some(error.to_string()),
                        )?;
                    }
                }
            }
            if status(&database, &job.id)? == "cancelled" {
                let path = app.state::<DownloadQueueState>().path(&job.id, "part")?;
                if let Err(error) = remove_partial(&path) {
                    let message = format!("Could not remove partial download: {error}");
                    cancellation_error(&database, &job.id, &message)?;
                }
            }
            Ok::<_, String>(())
        })
        .await
        .map_err(|error| format!("Download completion task failed: {error}"))??;
    }
    Ok(())
}

fn content_range_matches(value: &str, start: u64, size: u64) -> bool {
    let Some((range, total)) = value
        .strip_prefix("bytes ")
        .and_then(|value| value.split_once('/'))
    else {
        return false;
    };
    let Some((first, last)) = range.split_once('-') else {
        return false;
    };
    first.parse::<u64>() == Ok(start)
        && last
            .parse::<u64>()
            .is_ok_and(|end| end >= start && end < size)
        && total.parse::<u64>() == Ok(size)
}

fn strong_etag(headers: &HeaderMap) -> Option<String> {
    headers
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.starts_with("W/"))
        .map(str::to_owned)
}

fn resume_matches(response: &Response, job: &Transfer, offset: u64) -> bool {
    response.status() == StatusCode::PARTIAL_CONTENT
        && response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| content_range_matches(value, offset, job.size))
        && job
            .etag
            .as_deref()
            .zip(strong_etag(response.headers()).as_deref())
            .is_none_or(|(stored, served)| stored == served)
}

struct BandwidthWindow {
    limit: u64,
    initial: u64,
    started: Instant,
}

impl BandwidthWindow {
    fn new(initial: u64, limit: u64) -> Self {
        Self {
            limit,
            initial,
            started: Instant::now(),
        }
    }

    fn delay(&mut self, bytes: u64, limit: u64) -> Duration {
        if limit != self.limit {
            *self = Self::new(bytes, limit);
        }
        if limit == 0 {
            return Duration::ZERO;
        }
        let target =
            Duration::from_secs_f64(bytes.saturating_sub(self.initial) as f64 / limit as f64);
        let delay = target.saturating_sub(self.started.elapsed());
        if delay.is_zero() && self.started.elapsed() >= Duration::from_millis(250) {
            *self = Self::new(bytes, limit);
        }
        delay
    }
}

async fn persist_progress(
    database: std::sync::Arc<Database>,
    id: &str,
    bytes: u64,
    speed: u64,
    eta: Option<u64>,
    etag: Option<&str>,
) -> Result<(), String> {
    let id = id.to_owned();
    let etag = etag.map(str::to_owned);
    tauri::async_runtime::spawn_blocking(move || {
        update_progress(&database, &id, bytes, speed, eta, etag.as_deref())
    })
    .await
    .map_err(|error| format!("Progress task failed: {error}"))?
}

/// Decides whether a failed transfer waits, tries another link or fails.
#[derive(Debug)]
enum TransferError {
    Stopped,
    /// The partial file cannot be used, so other links would fail the same way.
    Local(String),
    Network(String),
    Server(String),
    Link(String),
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped => formatter.write_str("Download was stopped"),
            Self::Local(message)
            | Self::Network(message)
            | Self::Server(message)
            | Self::Link(message) => formatter.write_str(message),
        }
    }
}

// The resolver and database helpers report errors as text.
impl From<String> for TransferError {
    fn from(error: String) -> Self {
        if error.starts_with("Network:") {
            Self::Network(error)
        } else if error.starts_with("HTTP 5") {
            Self::Server(error)
        } else {
            Self::Link(error)
        }
    }
}

impl From<&str> for TransferError {
    fn from(error: &str) -> Self {
        error.to_owned().into()
    }
}

/// Without a network monitor, links fail over as if the network were online.
async fn is_online(network: Option<&crate::network::NetworkState>) -> bool {
    match network {
        Some(network) => {
            network.check_connectivity().await.status == crate::network::NetworkStatus::Online
        }
        None => true,
    }
}

async fn transfer<F: std::future::Future<Output = bool>>(
    queue: &DownloadQueueState,
    database: std::sync::Arc<Database>,
    job: &Transfer,
    online: impl Fn() -> F,
) -> Result<(), TransferError> {
    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    *queue
        .active
        .lock()
        .map_err(|_| "Download control lock was poisoned".to_owned())? =
        Some((job.id.clone(), stop));
    let checker = database.clone();
    let check_id = job.id.clone();
    if tauri::async_runtime::spawn_blocking(move || status(&checker, &check_id))
        .await
        .map_err(|error| error.to_string())??
        != "downloading"
    {
        return Err(TransferError::Stopped);
    }
    let reader = database.clone();
    let id = job.id.clone();
    let stored: Option<String> = tauri::async_runtime::spawn_blocking(move || {
        reader.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT mirror_plan FROM downloads WHERE id=?1",
                    [&id],
                    |row| row.get(0),
                )
                .map_err(db_error)
        })
    })
    .await
    .map_err(|error| error.to_string())??;
    let Some(stored) = stored else {
        return transfer_once(queue, database, job, &mut stopped).await;
    };
    let mut plan: crate::download_selection::MirrorPlan = serde_json::from_str(&stored)
        .map_err(|error| format!("Invalid saved download links: {error}"))?;
    let mut current = job.clone();
    let mut failure = "No supported download links are available".to_owned();
    while let Some(uri) = plan.current().map(str::to_owned) {
        if *stopped.borrow() {
            return Err(TransferError::Stopped);
        }
        let result = if current.url != uri {
            let inspected = tokio::select! {
                biased;
                _ = stopped.changed() => return Err(TransferError::Stopped),
                result = crate::download_selection::inspect(&queue.client, &uri) => result,
            };
            match inspected {
                Ok(size) if crate::download_selection::size_matches(plan.expected_size, size) => {
                    for extension in ["part", "archive"] {
                        match tokio::fs::remove_file(queue.path(&job.id, extension)?).await {
                            Ok(()) => {}
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                            Err(error) => {
                                return Err(TransferError::Local(format!(
                                    "Could not reset partial download: {error}"
                                )));
                            }
                        }
                    }
                    current.url = uri;
                    current.size = size;
                    current.etag = None;
                    save_mirror_state(database.clone(), &current, &plan, true).await?;
                    transfer_once(queue, database.clone(), &current, &mut stopped).await
                }
                Ok(_) => Err("Download does not match the size published by the source".into()),
                Err(error) => Err(error.into()),
            }
        } else {
            transfer_once(queue, database.clone(), &current, &mut stopped).await
        };
        match result {
            Ok(()) => return Ok(()),
            Err(error)
                if *stopped.borrow()
                    || matches!(error, TransferError::Stopped | TransferError::Local(_)) =>
            {
                return Err(error);
            }
            // Offline, every link would fail and the next one would discard the partial file.
            Err(error @ TransferError::Network(_)) if !online().await => {
                return Err(error);
            }
            Err(error) => failure = error.to_string(),
        }
        plan.fail();
        save_mirror_state(database.clone(), &current, &plan, false).await?;
        if plan.current().is_some() {
            tokio::select! {
                biased;
                _ = stopped.changed() => return Err(TransferError::Stopped),
                () = tokio::time::sleep(Duration::from_millis(250)) => {},
            }
        }
    }
    Err(TransferError::Link(format!(
        "All download links failed after three retries per link: {failure}"
    )))
}

async fn save_mirror_state(
    database: std::sync::Arc<Database>,
    job: &Transfer,
    plan: &crate::download_selection::MirrorPlan,
    reset: bool,
) -> Result<(), TransferError> {
    let job = job.clone();
    let plan = serde_json::to_string(plan).map_err(|error| error.to_string())?;
    let changed = tauri::async_runtime::spawn_blocking(move || {
        database.with_connection(|connection| {
            let changed = if reset {
                connection.execute("UPDATE downloads SET url=?2,size_bytes=?3,downloaded_bytes=0,speed_bps=0,eta_seconds=NULL,etag=NULL,mirror_plan=?4 WHERE id=?1 AND status='downloading'", params![job.id,job.url,i64::try_from(job.size).map_err(|_| "Download exceeds local storage limits")?,plan]).map_err(db_error)?
            } else {
                connection.execute("UPDATE downloads SET mirror_plan=?2 WHERE id=?1 AND status='downloading'", params![job.id,plan]).map_err(db_error)?
            };
            Ok(changed)
        })
    }).await.map_err(|error| error.to_string())??;
    if changed == 0 {
        return Err(TransferError::Stopped);
    }
    Ok(())
}

async fn transfer_once(
    queue: &DownloadQueueState,
    database: std::sync::Arc<Database>,
    job: &Transfer,
    stopped: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<(), TransferError> {
    use tokio::io::AsyncWriteExt;
    let mut current = job.clone();
    let job = &mut current;
    let partial = queue.path(&job.id, "part")?;
    let archive = queue.path(&job.id, "archive")?;
    if tokio::fs::symlink_metadata(&archive)
        .await
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() == job.size)
    {
        return Ok(());
    }
    let mut offset = match tokio::fs::symlink_metadata(&partial).await {
        Ok(metadata) if metadata.is_file() => metadata.len(),
        Ok(_) => {
            return Err(TransferError::Local(
                "Partial download is not a regular file".to_owned(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => {
            return Err(TransferError::Local(format!(
                "Could not inspect partial download: {error}"
            )));
        }
    };
    if offset >= job.size {
        offset = 0;
    }
    let mut resolved = tokio::select! {
        biased;
        _ = stopped.changed() => return Err(TransferError::Stopped),
        result = crate::download_resolver::resolve(&queue.client, &job.url) => result?,
    };
    if resolved.files.len() != 1 {
        return Err("Grouped downloads are not supported by this transfer yet".into());
    }
    let resolved = resolved
        .files
        .pop()
        .ok_or("Resolved download contains no files")?;
    let mut headers = resolved.headers.clone();
    if offset > 0 {
        headers.insert(
            RANGE,
            reqwest::header::HeaderValue::from_str(&format!("bytes={offset}-"))
                .map_err(|_| "Invalid resume offset")?,
        );
        if let Some(etag) = job.etag.as_deref() {
            headers.insert(
                IF_RANGE,
                reqwest::header::HeaderValue::from_str(etag)
                    .map_err(|_| "Invalid saved validator")?,
            );
        }
    }
    let mut response = tokio::select! {
        biased;
        _ = stopped.changed() => return Err(TransferError::Stopped),
        result = crate::download_resolver::get(&queue.client, &resolved.url, headers) => result?,
    };
    if offset > 0 && !resume_matches(&response, job, offset) {
        offset = 0;
        response = tokio::select! {
            biased;
            _ = stopped.changed() => return Err(TransferError::Stopped),
            result = crate::download_resolver::get(&queue.client, &resolved.url, resolved.headers.clone()) => result?,
        };
    }
    if response.status()
        != if offset > 0 {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        }
    {
        let message = format!("HTTP {}", response.status());
        return Err(if response.status().is_server_error() {
            TransferError::Server(message)
        } else {
            TransferError::Link(message)
        });
    }
    if crate::download_resolver::is_web_page(&response) {
        return Err("The download link returned a web page".into());
    }
    if offset == 0 {
        let actual = response
            .content_length()
            .filter(|size| *size > 0)
            .ok_or("The download host did not provide an exact archive size")?;
        let reader = database.clone();
        let id = job.id.clone();
        let published: Option<i64> = tauri::async_runtime::spawn_blocking(move || {
            reader.with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT published_size_bytes FROM downloads WHERE id=?1",
                        [&id],
                        |row| row.get(0),
                    )
                    .map_err(db_error)
            })
        })
        .await
        .map_err(|error| error.to_string())??;
        let published = published
            .map(u64::try_from)
            .transpose()
            .map_err(|_| "Invalid published download size")?;
        if !crate::download_selection::size_matches(published, actual)
            || (published.is_none() && actual != job.size)
        {
            return Err("Download does not match the size published by the source".into());
        }
        if actual != job.size {
            let writer = database.clone();
            let id = job.id.clone();
            let size =
                i64::try_from(actual).map_err(|_| "Download exceeds local storage limits")?;
            tauri::async_runtime::spawn_blocking(move || {
                writer.with_connection(|connection| {
                    connection
                        .execute(
                            "UPDATE downloads SET size_bytes=?2 WHERE id=?1",
                            params![id, size],
                        )
                        .map_err(db_error)?;
                    Ok(())
                })
            })
            .await
            .map_err(|error| error.to_string())??;
            job.size = actual;
        }
    }
    let etag = strong_etag(response.headers())
        .or_else(|| if offset > 0 { job.etag.clone() } else { None });
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(offset == 0)
        .append(offset > 0)
        .open(&partial)
        .await
        .map_err(|error| {
            TransferError::Local(format!("Could not open partial download: {error}"))
        })?;
    persist_progress(database.clone(), &job.id, offset, 0, None, etag.as_deref()).await?;
    let started = Instant::now();
    let initial = offset;
    let mut persisted = Instant::now();
    let mut limiter = BandwidthWindow::new(offset, queue.bandwidth_limit.load(Ordering::Relaxed));
    while let Some(chunk) = interruptible_response(stopped, response.chunk()).await? {
        if *stopped.borrow() {
            return Err(TransferError::Stopped);
        }
        let next = offset
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| "Download size overflow".to_owned())?;
        if next > job.size {
            return Err("Download exceeds the expected size".into());
        }
        file.write_all(&chunk).await.map_err(|error| {
            TransferError::Local(format!("Could not write partial download: {error}"))
        })?;
        offset = next;
        loop {
            if *stopped.borrow() {
                persist_progress(database.clone(), &job.id, offset, 0, None, etag.as_deref())
                    .await?;
                return Err(TransferError::Stopped);
            }
            let delay = limiter.delay(offset, queue.bandwidth_limit.load(Ordering::Relaxed));
            if delay.is_zero() {
                break;
            }
            tokio::select! {
                biased;
                _ = stopped.changed() => {},
                _ = queue.limit_changed.notified() => {},
                _ = tokio::time::sleep(delay) => {},
            }
        }
        let elapsed = started.elapsed().as_secs_f64();
        let speed = if elapsed > 0.0 {
            ((offset - initial) as f64 / elapsed) as u64
        } else {
            0
        };
        let eta = if speed > 0 {
            Some((job.size - offset).div_ceil(speed))
        } else {
            None
        };
        if persisted.elapsed() >= Duration::from_millis(500) {
            persist_progress(
                database.clone(),
                &job.id,
                offset,
                speed,
                eta,
                etag.as_deref(),
            )
            .await?;
            persisted = Instant::now();
        }
    }
    if offset != job.size {
        return Err(TransferError::Network(format!(
            "Network: Download ended at {offset} of {} bytes",
            job.size
        )));
    }
    persist_progress(database.clone(), &job.id, offset, 0, None, etag.as_deref()).await?;
    file.sync_all()
        .await
        .map_err(|error| TransferError::Local(format!("Could not sync download: {error}")))?;
    drop(file);
    if *stopped.borrow() {
        return Err(TransferError::Stopped);
    }
    tokio::fs::rename(&partial, &archive)
        .await
        .map_err(|error| {
            TransferError::Local(format!("Could not finalize downloaded archive: {error}"))
        })?;
    Ok(())
}

async fn interruptible_response<T>(
    stopped: &mut tokio::sync::watch::Receiver<bool>,
    response: impl std::future::Future<Output = Result<T, reqwest::Error>>,
) -> Result<T, TransferError> {
    if *stopped.borrow() {
        return Err(TransferError::Stopped);
    }
    tokio::select! {
        biased;
        _ = stopped.changed() => Err(TransferError::Stopped),
        result = response => result.map_err(|error| TransferError::Network(format!("Network: {}", error.without_url()))),
    }
}

#[tauri::command]
pub async fn list_downloads(app: AppHandle) -> Result<Vec<DownloadJob>, String> {
    tauri::async_runtime::spawn_blocking(move || list(app.state::<DatabaseState>().database()?))
        .await
        .map_err(|error| format!("Download list task failed: {error}"))?
}

#[tauri::command]
pub async fn queue_download(
    app: AppHandle,
    steam_app_id: u32,
    download_url: String,
    release_version: String,
    accept_unverified: bool,
    source_id: Option<String>,
    preferred_uri: Option<String>,
) -> Result<DownloadJob, String> {
    crate::startup_recovery::wait(&app).await?;
    let database = app.state::<DatabaseState>().shared_database()?;
    let read_database = database.clone();
    let lookup_url = download_url.clone();
    let lookup_version = release_version.clone();
    let lookup_source = source_id.clone();
    let (selected_source, entry, mut candidates, hydra) =
        tauri::async_runtime::spawn_blocking(move || {
            let (origin, entry, verified) = crate::game_updates::find_download(
                &read_database,
                steam_app_id,
                &lookup_url,
                &lookup_version,
                lookup_source.as_deref(),
            )?;
            if !verified && !accept_unverified {
                return Err("Confirm the unverified source before downloading".to_owned());
            }
            let rows = crate::legio_source_cache::cached_rows(&read_database)?;
            let candidates = rows
                .iter()
                .find(|row| row.id == origin)
                .and_then(|row| {
                    entry
                        .id
                        .as_ref()
                        .and_then(|id| row.catalog.candidates.get(id))
                })
                .cloned()
                .unwrap_or_else(|| vec![entry.download.url.clone()]);
            let origin_catalog = rows
                .iter()
                .find(|row| row.id == origin)
                .ok_or("Download source is unavailable")?;
            Ok::<_, String>((
                origin,
                entry,
                candidates,
                origin_catalog.catalog.format == crate::source_catalog::SourceFormat::Hydra,
            ))
        })
        .await
        .map_err(|error| error.to_string())??;
    let mut download_url = download_url;
    let mut resolved_size = None;
    if let Some(preferred) = preferred_uri {
        if !candidates.contains(&preferred) {
            return Err("Selected link is not part of this source release".into());
        }
        candidates.retain(|uri| uri != &preferred);
        candidates.insert(0, preferred);
    }
    candidates.retain(|uri| crate::download_selection::supported(uri));
    let legacy_cdn =
        !hydra && candidates.len() == 1 && crate::download_selection::native_cdn(&candidates[0]);
    let mirror_plan = if legacy_cdn {
        None
    } else {
        let expected_size = if hydra {
            None
        } else {
            entry.download.size_bytes
        };
        let checked = crate::link_check::check_links(
            app.state::<DatabaseState>().shared_database()?,
            &crate::download_resolver::client()?,
            candidates
                .iter()
                .map(|uri| (uri.clone(), expected_size))
                .collect(),
        )
        .await?;
        let mut first_error = None;
        let mut usable = Vec::new();
        for uri in candidates {
            match checked.get(&uri) {
                Some(Ok(size)) => usable.push((uri, *size)),
                Some(Err(error)) => {
                    first_error.get_or_insert_with(|| error.clone());
                }
                None => {}
            }
        }
        let Some((uri, size)) = usable.first() else {
            return Err(
                first_error.unwrap_or_else(|| "No supported download links are available".into())
            );
        };
        resolved_size = Some(*size);
        download_url = uri.clone();
        Some(crate::download_selection::MirrorPlan::new(
            usable.into_iter().map(|(uri, _)| uri).collect(),
            expected_size,
        ))
    };
    let source_id = Some(selected_source);
    let worker_app = app.clone();
    let job = tauri::async_runtime::spawn_blocking(move || {
        app.state::<DownloadQueueState>().directory()?;
        let manager = app.state::<crate::game_lifecycle::GameLaunchManager>();
        let _operation = manager.operation()?;
        let job = enqueue_prepared(
            app.state::<DatabaseState>().database()?,
            steam_app_id,
            &download_url,
            &release_version,
            accept_unverified,
            source_id.as_deref(),
            None,
            resolved_size,
            PlanChoice::Given(mirror_plan),
        )?;
        crate::download_policy::manual_start(
            app.state::<DatabaseState>().database()?,
            &manager,
            job.id(),
        )?;
        Ok::<_, String>(job)
    })
    .await
    .map_err(|error| format!("Queue task failed: {error}"))??;
    kick(worker_app);
    Ok(job)
}

#[tauri::command]
pub async fn pause_download(app: AppHandle, id: String) -> Result<(), String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        change_status(
            app.state::<DatabaseState>().database()?,
            &id,
            &[
                DownloadStatus::Queued,
                DownloadStatus::Downloading,
                DownloadStatus::Waiting,
            ],
            DownloadStatus::Paused,
        )?;
        app.state::<DatabaseState>().database()?.with_connection(|connection| { connection.execute("UPDATE downloads SET pause_reason = NULL, interrupted_by_game = 0 WHERE id = ?1", [&id]).map_err(db_error)?; Ok(()) })?;
        app.state::<DownloadQueueState>().interrupt(&id)
    })
    .await
    .map_err(|error| format!("Pause task failed: {error}"))?
}

#[tauri::command]
pub async fn resume_download(app: AppHandle, id: String) -> Result<(), String> {
    crate::startup_recovery::wait(&app).await?;
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<crate::game_lifecycle::GameLaunchManager>();
        let _operation = manager.operation()?;
        let database_state = app.state::<DatabaseState>();
        let database = database_state.database()?;
        crate::download_policy::require_resumable(database, &manager, &id)?;
        change_status(database, &id, &[DownloadStatus::Paused, DownloadStatus::Waiting], DownloadStatus::Queued)?;
        database.with_connection(|connection| { connection.execute("UPDATE downloads SET manual_override = 1, automatic_update = 0, pause_reason = NULL WHERE id = ?1", [&id]).map_err(db_error)?; Ok(()) })
    })
    .await
    .map_err(|error| format!("Resume task failed: {error}"))??;
    kick(worker_app);
    Ok(())
}

#[tauri::command]
pub async fn retry_download<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    crate::startup_recovery::wait(&app).await?;
    let worker_app = app.clone();
    let queue = app.state::<DownloadQueueState>();
    let _staging = queue.staging.lock().await;
    let retry_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let app = retry_app;
        let database = app.state::<DatabaseState>();
        let database = database.database()?;
        let manager = app.state::<crate::game_lifecycle::GameLaunchManager>();
        let _operation = manager.operation()?;
        crate::download_policy::require_resumable(database, &manager, &id)?;
        if status(database, &id)? == "staged" {
            crate::overlay_update::recover(database)?;
            if let Some(game) = crate::game_updates::target(database, &id)? {
                crate::overlay_update::ensure_launchable(database, &game)?;
            }
            database.with_connection(|connection| {
                connection
                    .execute("UPDATE downloads SET error = NULL WHERE id = ?1", [&id])
                    .map_err(db_error)?;
                Ok(())
            })?;
            return Ok(());
        }
        if status(database, &id)? == "finalizing" {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| format!("Could not locate app data: {error}"))?;
            return crate::finalize_install::retry(
                database,
                &database.storage_root(&data_dir)?,
                &id,
            );
        }
        change_status(
            database,
            &id,
            &[DownloadStatus::Failed],
            DownloadStatus::Queued,
        )?;
        database.with_connection(|connection| {
            let stored: Option<String> = connection
                .query_row(
                    "SELECT mirror_plan FROM downloads WHERE id=?1",
                    [&id],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            if let Some(stored) = stored {
                let mut plan: crate::download_selection::MirrorPlan =
                    serde_json::from_str(&stored).map_err(|error| error.to_string())?;
                plan.restart();
                connection
                    .execute(
                        "UPDATE downloads SET mirror_plan=?2 WHERE id=?1",
                        params![
                            id,
                            serde_json::to_string(&plan).map_err(|error| error.to_string())?
                        ],
                    )
                    .map_err(db_error)?;
            }
            Ok(())
        })
    })
    .await
    .map_err(|error| format!("Retry task failed: {error}"))??;
    schedule_installations(&worker_app);
    kick(worker_app);
    Ok(())
}

#[tauri::command]
pub async fn cancel_download(app: AppHandle, id: String) -> Result<(), String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let queue = app.state::<DownloadQueueState>();
        let database = app.state::<DatabaseState>();
        let was_downloading = status(database.database()?, &id)? == "downloading";
        change_status(
            database.database()?,
            &id,
            &[
                DownloadStatus::Queued,
                DownloadStatus::Downloading,
                DownloadStatus::Paused,
                DownloadStatus::Waiting,
                DownloadStatus::Failed,
            ],
            DownloadStatus::Cancelled,
        )?;
        queue.interrupt(&id)?;
        if let Err(message) = remove_cancelled_files(&queue, &id, was_downloading) {
            cancellation_error(database.database()?, &id, &message)?;
            return Err(message);
        }
        Ok(())
    })
    .await
    .map_err(|error| format!("Cancel task failed: {error}"))?
}

#[tauri::command]
pub async fn get_download_bandwidth_limit(app: AppHandle) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DatabaseState>()
            .database()?
            .download_bandwidth_limit()
    })
    .await
    .map_err(|error| format!("Bandwidth limit load task failed: {error}"))?
}

/// Statuses whose entry can be dropped once nothing further can happen to it.
/// A retryable failure is included so a job the user does not want to retry
/// does not stay in the list forever.
const REMOVABLE: [DownloadStatus; 3] = [
    DownloadStatus::Cancelled,
    DownloadStatus::Failed,
    DownloadStatus::Installed,
];

/// Statuses that carry no pending work and no error worth reading, so bulk
/// cleanup may drop them without a confirmation.
const FINISHED: [DownloadStatus; 2] = [DownloadStatus::Cancelled, DownloadStatus::Installed];

fn remove_entry(queue: &DownloadQueueState, database: &Database, id: &str) -> Result<(), String> {
    let id = Uuid::parse_str(id)
        .map_err(|_| "Download ID is invalid".to_owned())?
        .to_string();
    let status = status(database, &id)?;
    if !REMOVABLE
        .iter()
        .any(|candidate| candidate.as_str() == status)
    {
        return Err(format!("Cannot remove a {status} download"));
    }
    // Files are removed before the row so a cleanup failure keeps the entry and
    // its error visible instead of leaking files silently. The installed game
    // lives in a separate app-owned directory and is never touched here.
    for extension in ["part", "archive"] {
        remove_partial(&queue.path(&id, extension)?)
            .map_err(|error| format!("Could not remove download files: {error}"))?;
    }
    finalize_install::remove_stage_directory(&queue.path(&id, "stage")?)?;
    database.with_connection(|connection| {
        let transaction = connection.unchecked_transaction().map_err(db_error)?;
        transaction.execute("UPDATE games SET installation_root = COALESCE(installation_root, (SELECT final_path FROM downloads WHERE id = ?1 AND status = 'installed')) WHERE id = ?1", [&id]).map_err(db_error)?;
        transaction
            .execute("DELETE FROM downloads WHERE id = ?1", [&id])
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)
    })
}

#[tauri::command]
pub async fn remove_download(app: AppHandle, id: String) -> Result<(), String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        remove_entry(
            &app.state::<DownloadQueueState>(),
            app.state::<DatabaseState>().database()?,
            &id,
        )
    })
    .await
    .map_err(|error| format!("Remove task failed: {error}"))?
}

#[tauri::command]
pub async fn remove_finished_downloads(app: AppHandle) -> Result<Vec<String>, String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let queue = app.state::<DownloadQueueState>();
        let state = app.state::<DatabaseState>();
        let database = state.database()?;
        let ids = finished_ids(database)?;
        let mut removed = Vec::with_capacity(ids.len());
        for id in ids {
            remove_entry(&queue, database, &id)?;
            removed.push(id);
        }
        Ok(removed)
    })
    .await
    .map_err(|error| format!("Cleanup task failed: {error}"))?
}

fn finished_ids(database: &Database) -> Result<Vec<String>, String> {
    let placeholders = FINISHED.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let query = format!(
        "SELECT id FROM downloads WHERE status IN ({placeholders}) ORDER BY queue_position, created_at, id"
    );
    database.with_connection(|connection| {
        let mut statement = connection.prepare(&query).map_err(db_error)?;
        statement
            .query_map(
                rusqlite::params_from_iter(FINISHED.map(DownloadStatus::as_str)),
                |row| row.get(0),
            )
            .map_err(db_error)?
            .collect::<Result<_, _>>()
            .map_err(db_error)
    })
}

const REORDERABLE: [DownloadStatus; 4] = [
    DownloadStatus::Queued,
    DownloadStatus::Paused,
    DownloadStatus::Waiting,
    DownloadStatus::Failed,
];

#[tauri::command]
pub async fn reorder_downloads(app: AppHandle, ids: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        reorder(app.state::<DatabaseState>().database()?, &ids)
    })
    .await
    .map_err(|error| format!("Reorder task failed: {error}"))?
}

fn reorder(database: &Database, ids: &[String]) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    let mut parsed = Vec::with_capacity(ids.len());
    for id in ids {
        let id = Uuid::parse_str(id)
            .map_err(|_| "Download ID is invalid".to_owned())?
            .to_string();
        if parsed.contains(&id) {
            return Err("The queue lists a download twice".to_owned());
        }
        parsed.push(id);
    }
    database.with_connection(|connection| {
        let transaction = connection.unchecked_transaction().map_err(db_error)?;
        for id in &parsed {
            let status: Option<String> = transaction
                .query_row("SELECT status FROM downloads WHERE id = ?1", [id], |row| {
                    row.get(0)
                })
                .optional()
                .map_err(db_error)?;
            let Some(status) = status else {
                return Err("Download was not found".to_owned());
            };
            if !REORDERABLE
                .iter()
                .any(|candidate| candidate.as_str() == status)
            {
                return Err(format!("Cannot reorder a {status} download"));
            }
        }
        for (index, id) in parsed.iter().enumerate() {
            let position =
                i64::try_from(index).map_err(|_| "The queue is too long to reorder".to_owned())?;
            transaction
                .execute(
                    "UPDATE downloads SET queue_position = ?2 WHERE id = ?1",
                    params![id, position],
                )
                .map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)
    })
}

#[tauri::command]
pub async fn set_download_bandwidth_limit(
    app: AppHandle,
    bytes_per_second: u64,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DatabaseState>()
            .database()?
            .save_download_bandwidth_limit(bytes_per_second)?;
        app.state::<DownloadQueueState>()
            .set_bandwidth_limit(bytes_per_second);
        Ok(())
    })
    .await
    .map_err(|error| format!("Bandwidth limit save task failed: {error}"))?
}

fn recover_on_start(app: &AppHandle) -> Result<(), String> {
    let database = app.state::<DatabaseState>();
    let database = database.database()?;
    clean_cancelled(&app.state::<DownloadQueueState>(), database)?;
    recover_staging(&app.state::<DownloadQueueState>(), database)?;

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Could not locate app data: {error}"))?;
    crate::overlay_update::recover(database)?;
    finalize_install::recover(database, &database.storage_root(&data_dir)?)?;
    recover(database)?;
    Ok(())
}

pub fn start(app: AppHandle) -> Result<(), String> {
    recover_on_start(&app)?;
    app.state::<DownloadQueueState>()
        .starting
        .store(false, Ordering::Release);
    kick(app.clone());
    schedule_installations(&app);
    crate::download_policy::start(app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_interrupts_a_silent_response_and_is_not_lost_before_waiting() {
        let (stop, mut stopped) = tokio::sync::watch::channel(false);
        let pending = interruptible_response(
            &mut stopped,
            std::future::pending::<Result<(), reqwest::Error>>(),
        );
        let cancel = async {
            tokio::task::yield_now().await;
            stop.send_replace(true);
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(1), async {
            tokio::join!(pending, cancel)
        })
        .await
        .unwrap();
        assert!(matches!(result.unwrap_err(), TransferError::Stopped));
        assert!(
            interruptible_response(
                &mut stopped,
                std::future::pending::<Result<(), reqwest::Error>>()
            )
            .await
            .is_err()
        );
    }
    use sha2::{Digest, Sha256};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
    };

    const TEST_HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn ordered_mirrors_retry_three_times_then_discard_partial_bytes_before_fallback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let first = format!("http://{}/first", listener.local_addr().unwrap());
        let second = format!("http://{}/second", listener.local_addr().unwrap());
        let payload = b"PK\x03\x04second-archive";
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(12);
            let mut requests = Vec::new();
            for index in 0..6 {
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => panic!("Mirror request {index} was not received: {error}"),
                    }
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let request = String::from_utf8(request).unwrap().to_lowercase();
                if index < 4 {
                    assert!(request.starts_with("get /first "));
                    let offset = request
                        .split("range: bytes=")
                        .nth(1)
                        .and_then(|value| value.split('-').next())
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(0);
                    let remaining = 10 - offset;
                    let range = if offset > 0 {
                        format!("Content-Range: bytes {offset}-9/10\r\n")
                    } else {
                        String::new()
                    };
                    let status = if offset > 0 {
                        "206 Partial Content"
                    } else {
                        "200 OK"
                    };
                    socket.write_all(format!("HTTP/1.1 {status}\r\n{range}ETag: \"first\"\r\nContent-Length: {remaining}\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
                    socket
                        .write_all(&b"bad"[..remaining.saturating_sub(1).min(3)])
                        .unwrap();
                } else {
                    assert!(request.starts_with("get /second "));
                    if index == 5 {
                        assert!(
                            !request.contains("range:"),
                            "a different mirror starts from byte zero"
                        );
                    }
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload.len()).as_bytes()).unwrap();
                    socket.write_all(payload).unwrap();
                }
                requests.push(request);
            }
            requests
        });
        let (database, directory) = database_with_source();
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        let plan =
            crate::download_selection::MirrorPlan::new(vec![first.clone(), second.clone()], None);
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET url=?2,mirror_plan=?3,published_size_bytes=NULL WHERE id=?1",
                        params![job.id, first, serde_json::to_string(&plan).unwrap()],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let database = std::sync::Arc::new(database);
        let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
        let transfer_job = claim(&database).unwrap().unwrap();
        tauri::async_runtime::block_on(transfer(&queue, database.clone(), &transfer_job, online))
            .unwrap();
        assert_eq!(
            fs::read(queue.path(&job.id, "archive").unwrap()).unwrap(),
            payload
        );
        let saved = list(&database).unwrap().remove(0);
        assert_eq!(saved.url, second);
        assert_eq!(saved.size_bytes, payload.len() as u64);
        assert_eq!(server.join().unwrap().len(), 6);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn offline_network_error_keeps_the_mirror_and_partial_file() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let first = format!("http://{}/first", listener.local_addr().unwrap());
        let second = format!("http://{}/second", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            // The connection drops after 3 of 10 bytes.
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nbad")
                .unwrap();
        });
        let (database, directory) = database_with_source();
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        let plan = crate::download_selection::MirrorPlan::new(vec![first.clone(), second], None);
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET url=?2,size_bytes=10,mirror_plan=?3,published_size_bytes=NULL WHERE id=?1",
                        params![job.id, first, serde_json::to_string(&plan).unwrap()],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let database = std::sync::Arc::new(database);
        let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
        let transfer_job = claim(&database).unwrap().unwrap();
        let result = tauri::async_runtime::block_on(transfer(
            &queue,
            database.clone(),
            &transfer_job,
            || std::future::ready(false),
        ));
        server.join().unwrap();
        assert!(
            matches!(result, Err(TransferError::Network(_))),
            "{result:?}"
        );
        assert!(queue.path(&job.id, "part").unwrap().is_file());
        let saved: crate::download_selection::MirrorPlan = database
            .with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT mirror_plan FROM downloads WHERE id=?1",
                        [&job.id],
                        |row| row.get::<_, String>(0),
                    )
                    .map_err(db_error)
            })
            .map(|plan| serde_json::from_str(&plan).unwrap())
            .unwrap();
        assert_eq!((saved.index, saved.failures), (0, 0));
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn exhausting_all_mirrors_marks_the_job_failed_without_an_automatic_waiting_loop() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let first = format!("http://{}/first", listener.local_addr().unwrap());
        let second = format!("http://{}/second", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(12);
            let mut paths = Vec::new();
            for _ in 0..8 {
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => panic!("Expected mirror retry: {error}"),
                    }
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                paths.push(
                    String::from_utf8(request)
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap()
                        .to_owned(),
                );
                socket
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
            }
            paths
        });
        let (database, directory) = database_with_source();
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        let plan =
            crate::download_selection::MirrorPlan::new(vec![first.clone(), second], Some(10));
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET url=?2,mirror_plan=?3,published_size_bytes=NULL WHERE id=?1",
                        params![job.id, first, serde_json::to_string(&plan).unwrap()],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let app = test_app(&directory);
        tauri::async_runtime::block_on(run_queue(app.handle())).unwrap();
        let saved = list(&database).unwrap().remove(0);
        assert_eq!(saved.status, DownloadStatus::Failed);
        assert!(
            saved
                .error
                .unwrap()
                .starts_with("All download links failed")
        );
        assert_eq!(
            server.join().unwrap(),
            [vec!["/first"; 4], vec!["/second"; 4]].concat()
        );
        drop(app);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn completion_notification_waits_for_installation_and_respects_settings() {
        let (database, directory) = database_with_source();
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        let mut settings = database.settings().unwrap();
        settings.download_notifications = true;
        database.save_settings(settings.clone()).unwrap();
        for state in [
            "downloading",
            "downloaded",
            "staging",
            "staged",
            "finalizing",
            "failed",
            "cancelled",
        ] {
            database
                .with_connection(|connection| {
                    connection
                        .execute(
                            "UPDATE downloads SET status = ?2 WHERE id = ?1",
                            params![job.id, state],
                        )
                        .map_err(db_error)?;
                    Ok(())
                })
                .unwrap();
            assert_eq!(
                completion_notification(&database, &job.id).unwrap(),
                None,
                "{state}"
            );
        }
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET status = 'installed' WHERE id = ?1",
                        [&job.id],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            completion_notification(&database, &job.id)
                .unwrap()
                .unwrap()
                .1,
            job.name
        );
        settings.download_notifications = false;
        database.save_settings(settings).unwrap();
        assert_eq!(completion_notification(&database, &job.id).unwrap(), None);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    fn online() -> std::future::Ready<bool> {
        std::future::ready(true)
    }

    fn test_app(directory: &Path) -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = format!("org.legio.test.{}", Uuid::new_v4());
        let state = DatabaseState::new(Ok(directory.to_path_buf()));
        if let Ok(database) = state.database() {
            let mut settings = database.settings().unwrap();
            settings.download_notifications = false;
            database.save_settings(settings).unwrap();
        }
        tauri::test::mock_builder()
            .manage(state)
            .manage(crate::game_lifecycle::GameLaunchManager::new())
            .manage(crate::game_artwork::GameArtworkStore::new(Err(
                "No shortcut artwork in download fixture".into(),
            )))
            .manage(DownloadQueueState::new(Ok(directory.to_path_buf()), "test").unwrap())
            .build(context)
            .unwrap()
    }

    fn wait_for_status(database: &Database, id: &str, expected: &str) {
        let started = Instant::now();
        loop {
            let current = status(database, id).unwrap();
            if current == expected {
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "download did not reach {expected}, current status is {current}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn database_with_source() -> (Database, PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("legio-download-test-{}", Uuid::new_v4()));
        let database = Database::open(&directory).unwrap();
        let manifest = br#"{"schemaVersion":1,"generatedAt":"2026-09-22T00:00:00Z","verified":[{"steamAppId":400,"name":"Portal","release":{"version":"1","publishedAt":"2026-09-22T00:00:00Z"},"download":{"url":"https://example.invalid/portal.zip","sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","sizeBytes":10}}],"unverified":[]}"#;
        database.with_connection(|connection| {
            connection.execute("INSERT INTO download_sources (id, url, manifest, fetched_at) VALUES ('test-source', 'https://example.invalid/games.json', ?1, 1)", [manifest.as_slice()]).map_err(db_error)?;
            Ok(())
        }).unwrap();
        (database, directory)
    }

    #[test]
    fn queues_exact_release_for_shared_app_id() {
        let (database, directory) = database_with_source();
        let second_hash = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let manifest = format!(
            "{{\"schemaVersion\":1,\"generatedAt\":\"2026-09-22T00:00:00Z\",\"verified\":[{{\"steamAppId\":400,\"name\":\"Portal\",\"release\":{{\"version\":\"1\",\"publishedAt\":\"2026-09-22T00:00:00Z\"}},\"download\":{{\"url\":\"https://example.invalid/one.zip\",\"sha256\":\"{TEST_HASH}\",\"sizeBytes\":10}}}},{{\"steamAppId\":400,\"name\":\"Portal moddato\",\"release\":{{\"version\":\"2\",\"publishedAt\":\"2026-09-22T00:00:00Z\"}},\"download\":{{\"url\":\"https://example.invalid/two.zip\",\"sha256\":\"{second_hash}\",\"sizeBytes\":20}}}}],\"unverified\":[]}}"
        );
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE download_sources SET manifest = ?1 WHERE id = 'test-source'",
                        [manifest.as_bytes()],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();

        let second = enqueue(
            &database,
            400,
            "https://example.invalid/two.zip",
            "2",
            false,
        )
        .unwrap();
        let first = enqueue(
            &database,
            400,
            "https://example.invalid/one.zip",
            "1",
            false,
        )
        .unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(
            (
                second.name.as_str(),
                second.release_version.as_str(),
                second.sha256.as_deref().unwrap()
            ),
            ("Portal moddato", "2", second_hash)
        );
        assert_eq!(
            (first.name.as_str(), first.release_version.as_str()),
            ("Portal", "1")
        );
        assert!(
            enqueue(
                &database,
                400,
                "https://example.invalid/missing.zip",
                "1",
                false
            )
            .is_err()
        );

        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reordering_rewrites_the_queue_order_and_worker_priority() {
        let (database, directory) = database_with_source();
        let first = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        let second = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();

        reorder(&database, &[second.id.clone(), first.id.clone()]).unwrap();
        let ordered: Vec<String> = list(&database)
            .unwrap()
            .into_iter()
            .map(|job| job.id)
            .collect();
        assert_eq!(ordered, vec![second.id.clone(), first.id.clone()]);

        assert_eq!(claim(&database).unwrap().unwrap().id, second.id);
        assert_eq!(
            reorder(&database, &[first.id.clone(), second.id.clone()]).unwrap_err(),
            "Cannot reorder a downloading download"
        );
        assert_eq!(
            reorder(&database, &[first.id.clone(), first.id.clone()]).unwrap_err(),
            "The queue lists a download twice"
        );
        assert_eq!(
            reorder(&database, &["not-a-uuid".to_owned()]).unwrap_err(),
            "Download ID is invalid"
        );
        let ordered: Vec<String> = list(&database)
            .unwrap()
            .into_iter()
            .map(|job| job.id)
            .collect();
        assert_eq!(ordered, vec![second.id.clone(), first.id.clone()]);

        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reorder_rolls_back_when_a_later_update_fails() {
        let (database, directory) = database_with_source();
        let first = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        let mut second = first.clone();
        second.id = Uuid::new_v4().to_string();
        database.with_connection(|connection| {
            connection.execute("INSERT INTO downloads (id, steam_app_id, name, release_version, url, sha256, size_bytes, status, created_at, updated_at) SELECT ?2, steam_app_id, name, release_version, url, sha256, size_bytes, status, created_at, updated_at FROM downloads WHERE id = ?1", params![first.id, second.id]).map_err(db_error)?;
            Ok(())
        }).unwrap();
        database.with_connection(|connection| {
            connection.execute("UPDATE downloads SET queue_position = 5 WHERE id = ?1", [&first.id]).map_err(db_error)?;
            connection.execute("UPDATE downloads SET queue_position = 6 WHERE id = ?1", [&second.id]).map_err(db_error)?;
            connection.execute_batch(&format!("CREATE TRIGGER fail_reorder BEFORE UPDATE OF queue_position ON downloads WHEN OLD.id = '{}' BEGIN SELECT RAISE(ABORT, 'injected'); END;", second.id)).map_err(db_error)
        }).unwrap();
        assert!(reorder(&database, &[first.id.clone(), second.id.clone()]).is_err());
        let position: i64 = database
            .with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT queue_position FROM downloads WHERE id = ?1",
                        [&first.id],
                        |row| row.get(0),
                    )
                    .map_err(db_error)
            })
            .unwrap();
        assert_eq!(position, 5);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn changing_bandwidth_does_not_charge_previous_bytes() {
        let mut window = BandwidthWindow::new(0, 0);
        assert!(window.delay(100 * 1024 * 1024, 1024 * 1024).is_zero());
        let before = window.started.elapsed();
        let delay = window.delay(101 * 1024 * 1024, 1024 * 1024);
        let after = window.started.elapsed();
        assert!(delay >= Duration::from_secs(1).saturating_sub(after));
        assert!(delay <= Duration::from_secs(1).saturating_sub(before));
        assert!(window.delay(101 * 1024 * 1024, 0).is_zero());
    }

    fn archive_fixture() -> (&'static [u8], String) {
        let bytes: &'static [u8] = include_bytes!("../test-fixtures/archive/safe.zip");
        (bytes, format!("{:x}", Sha256::digest(bytes)))
    }

    fn install_archive_fixture() -> (&'static [u8], String) {
        let bytes: &'static [u8] = include_bytes!("../test-fixtures/archive/executable.zip");
        (bytes, format!("{:x}", Sha256::digest(bytes)))
    }

    fn wait_for_installation(
        app: &tauri::App<tauri::test::MockRuntime>,
        database: &Database,
        id: &str,
    ) {
        wait_for_status(database, id, "installed");
        let started = Instant::now();
        while app
            .state::<DownloadQueueState>()
            .processing
            .lock()
            .unwrap()
            .contains(id)
        {
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "installation did not settle"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            list(database)
                .unwrap()
                .into_iter()
                .find(|job| job.id == id)
                .unwrap()
                .error
                .as_deref(),
            Some(
                "Could not create application-menu shortcut: No shortcut artwork in download fixture"
            )
        );
    }

    fn archive_response(archive: &[u8]) -> Vec<u8> {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            archive.len()
        )
        .into_bytes()
        .into_iter()
        .chain(archive.iter().copied())
        .collect()
    }

    fn downloaded_fixture() -> (Database, DownloadQueueState, DownloadJob, PathBuf) {
        let (database, directory) = database_with_source();
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        let (bytes, hash) = archive_fixture();
        let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
        fs::write(queue.path(&job.id, "archive").unwrap(), bytes).unwrap();
        database.with_connection(|connection| {
            connection.execute(
                "UPDATE downloads SET status = 'downloaded', sha256 = ?2, size_bytes = ?3, published_size_bytes = ?3, downloaded_bytes = ?3 WHERE id = ?1",
                params![job.id, hash, bytes.len() as i64],
            ).map_err(db_error)?;
            Ok(())
        }).unwrap();
        (database, queue, job, directory)
    }

    #[test]
    fn stage_command_persists_verified_path_for_recovery() {
        let (database, queue, job, directory) = downloaded_fixture();
        let stage = stage_one(&queue, &database, &job.id).unwrap();
        assert_eq!(fs::read(stage.join("Game/data.bin")).unwrap(), b"hello");
        assert_eq!(list(&database).unwrap()[0].status, DownloadStatus::Staged);
        let stored: String = database
            .with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT staged_path FROM downloads WHERE id = ?1",
                        [&job.id],
                        |row| row.get(0),
                    )
                    .map_err(db_error)
            })
            .unwrap();
        assert_eq!(stored, stage.to_string_lossy());
        drop(database);
        let reopened = Database::open(&directory).unwrap();
        assert_eq!(list(&reopened).unwrap()[0].status, DownloadStatus::Staged);
        drop(reopened);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn staging_hash_failure_removes_archive_and_records_failure() {
        let (database, queue, job, directory) = downloaded_fixture();
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET sha256 = ?2 WHERE id = ?1",
                        params![job.id, "0".repeat(64)],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        assert!(
            stage_one(&queue, &database, &job.id)
                .unwrap_err()
                .contains("hash differs")
        );
        assert!(!queue.path(&job.id, "archive").unwrap().exists());
        assert!(!queue.path(&job.id, "stage").unwrap().exists());
        assert_eq!(list(&database).unwrap()[0].status, DownloadStatus::Failed);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn database_failure_removes_stage_and_restart_recovers() {
        let (database, queue, job, directory) = downloaded_fixture();
        database.with_connection(|connection| {
            connection.execute_batch("CREATE TRIGGER reject_staged BEFORE UPDATE ON downloads WHEN NEW.status = 'staged' BEGIN SELECT RAISE(FAIL, 'db failure'); END;").map_err(db_error)
        }).unwrap();
        assert!(
            stage_one(&queue, &database, &job.id)
                .unwrap_err()
                .contains("Could not save staged download")
        );
        assert!(!queue.path(&job.id, "stage").unwrap().exists());
        assert_eq!(list(&database).unwrap()[0].status, DownloadStatus::Staging);
        recover_staging(&queue, &database).unwrap();
        assert_eq!(
            list(&database).unwrap()[0].status,
            DownloadStatus::Downloaded
        );
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn unverified_releases_without_hash_are_distinct_and_skip_verification() {
        let (database, directory) = database_with_source();
        let mut manifest = serde_json::to_value(
            crate::game_updates::sources(&database)
                .unwrap()
                .remove(0)
                .manifest,
        )
        .unwrap();
        let mut entry = manifest["verified"][0].clone();
        entry["download"].as_object_mut().unwrap().remove("sha256");
        let mut second = entry.clone();
        second["release"]["version"] = serde_json::json!("2");
        manifest["verified"] = serde_json::json!([]);
        manifest["unverified"] = serde_json::json!([entry, second]);
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE download_sources SET manifest = ?1",
                        [serde_json::to_vec(&manifest).unwrap()],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        *database.source_cache.lock().unwrap() = None;
        let url = "https://example.invalid/portal.zip";
        assert!(enqueue(&database, 400, url, "1", false).is_err());
        let first = enqueue(&database, 400, url, "1", true).unwrap();
        let second = enqueue(&database, 400, url, "2", true).unwrap();
        assert_eq!(first.sha256, None);
        assert_eq!(first.release_version, "1");
        assert_eq!(second.release_version, "2");
        assert_ne!(first.id, second.id);
        let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
        fs::write(
            queue.path(&first.id, "archive").unwrap(),
            archive_fixture().0,
        )
        .unwrap();
        set_status(&database, &first.id, "downloaded");
        stage_one(&queue, &database, &first.id).unwrap();
        assert_eq!(status(&database, &first.id).unwrap(), "staged");
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn migration_recovers_unverified_trust_for_existing_downloads() {
        let (database, _queue, job, directory) = downloaded_fixture();
        let mut manifest = serde_json::to_value(
            crate::game_updates::sources(&database)
                .unwrap()
                .remove(0)
                .manifest,
        )
        .unwrap();
        manifest["unverified"] = manifest["verified"].clone();
        manifest["verified"] = serde_json::json!([]);
        database.with_connection(|connection| {
            connection.execute("INSERT INTO legio_source_cache (id, manifest, fetched_at) VALUES (1, ?1, 1)", [serde_json::to_vec(&manifest).unwrap()]).map_err(db_error)?;
            connection.execute_batch("ALTER TABLE downloads DROP COLUMN source_verified; PRAGMA user_version = 19;").map_err(db_error)
        }).unwrap();
        drop(database);
        let database = Database::open(&directory).unwrap();
        let verified: bool = database
            .with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT source_verified FROM downloads WHERE id = ?1",
                        [&job.id],
                        |row| row.get(0),
                    )
                    .map_err(db_error)
            })
            .unwrap();
        assert!(!verified);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn staging_hash_policy_respects_trust_and_the_global_setting() {
        // The stored hash never matches the archive, so staging succeeds only when verification is skipped.
        for (verified, enabled) in [(false, true), (false, false), (true, true), (true, false)] {
            let (database, queue, job, directory) = downloaded_fixture();
            database
                .save_settings(crate::database::Settings {
                    verify_verified_downloads: enabled,
                    ..crate::database::Settings::default()
                })
                .unwrap();
            database
                .with_connection(|connection| {
                    connection
                        .execute(
                            "UPDATE downloads SET sha256 = ?1, source_verified = ?2 WHERE id = ?3",
                            params![TEST_HASH, verified, job.id],
                        )
                        .map_err(db_error)?;
                    Ok(())
                })
                .unwrap();
            let skipped = verified && !enabled;
            assert_eq!(stage_one(&queue, &database, &job.id).is_ok(), skipped);
            assert_eq!(queue.path(&job.id, "archive").unwrap().exists(), skipped);
            assert_eq!(
                status(&database, &job.id).unwrap(),
                if skipped { "staged" } else { "failed" }
            );
            drop(database);
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn verified_download_still_requires_a_hash_when_checking_is_disabled() {
        let (database, queue, job, directory) = downloaded_fixture();
        database
            .save_settings(crate::database::Settings {
                verify_verified_downloads: false,
                ..crate::database::Settings::default()
            })
            .unwrap();
        database
            .with_connection(|connection| {
                connection
                    .execute("UPDATE downloads SET sha256 = '' WHERE id = ?1", [&job.id])
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        assert!(
            stage_one(&queue, &database, &job.id)
                .unwrap_err()
                .contains("required")
        );
        assert_eq!(status(&database, &job.id).unwrap(), "failed");
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn inaccessible_staging_storage_is_a_retryable_job_failure() {
        let (database, queue, job, directory) = downloaded_fixture();
        database
            .with_connection(|c| {
                c.execute(
                    "UPDATE downloads SET status='staging' WHERE id=?1",
                    [&job.id],
                )
                .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let blocked = directory.join("unavailable-storage");
        fs::write(&blocked, b"not a directory").unwrap();
        queue.set_storage_root(&blocked).unwrap();
        recover_staging(&queue, &database).unwrap();
        assert_eq!(status(&database, &job.id).unwrap(), "failed");
        queue.set_storage_root(&directory).unwrap();
        change_status(
            &database,
            &job.id,
            &[DownloadStatus::Failed],
            DownloadStatus::Downloaded,
        )
        .unwrap();
        stage_one(&queue, &database, &job.id).unwrap();
        assert_eq!(status(&database, &job.id).unwrap(), "staged");
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn staging_path_failure_is_persisted_for_the_user() {
        let (database, queue, job, directory) = downloaded_fixture();
        let blocked = directory.join("blocked");
        fs::write(&blocked, b"not a directory").unwrap();
        queue.set_storage_root(&blocked).unwrap();

        let error = stage_one(&queue, &database, &job.id).unwrap_err();
        let jobs = list(&database).unwrap();
        assert_eq!(jobs[0].status, DownloadStatus::Failed);
        assert_eq!(jobs[0].error.as_deref(), Some(error.as_str()));
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn automatic_staging_waits_for_the_previous_extraction() {
        let (database, _queue, job, directory) = downloaded_fixture();
        let app = test_app(&directory);
        let queue = app.state::<DownloadQueueState>();
        let staging = queue.staging.lock().await;

        spawn_staging(app.handle(), &job.id);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(status(&database, &job.id).unwrap(), "downloaded");

        drop(staging);
        wait_for_status(&database, &job.id, "staged");
        assert_eq!(
            fs::read(queue.path(&job.id, "stage").unwrap().join("Game/data.bin")).unwrap(),
            b"hello"
        );
        drop(app);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn startup_schedules_completed_downloads_and_preserves_queued_and_failed_jobs() {
        let (database, queue, completed, directory) = downloaded_fixture();
        let (archive, hash) = install_archive_fixture();
        fs::write(queue.path(&completed.id, "archive").unwrap(), archive).unwrap();
        database.with_connection(|connection| {
            connection.execute(
                "UPDATE downloads SET sha256 = ?2, size_bytes = ?3, published_size_bytes = ?3, downloaded_bytes = ?3 WHERE id = ?1",
                params![completed.id, hash, archive.len() as i64],
            ).map_err(db_error)?;
            Ok(())
        }).unwrap();
        let queued = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        let failed = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        set_status(&database, &failed.id, "failed");
        let app = test_app(&directory);

        schedule_installations(app.handle());
        wait_for_installation(&app, &database, &completed.id);

        let game = database.game(&completed.id).unwrap();
        assert_eq!(
            fs::read(game.executable_path.unwrap()).unwrap(),
            b"#!/bin/sh\nexit 0\n"
        );
        assert_eq!(status_of(&database, &queued.id).as_deref(), Some("queued"));
        assert_eq!(status_of(&database, &failed.id).as_deref(), Some("failed"));
        drop(app);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn schema_v8_preserves_existing_downloads() {
        let (database, _, job, directory) = downloaded_fixture();
        database
            .with_connection(|connection| {
                connection
                    .execute_batch(
                        "ALTER TABLE downloads DROP COLUMN source_verified;
                         ALTER TABLE downloads DROP COLUMN queue_position;
                         ALTER TABLE downloads DROP COLUMN install_token;
                         ALTER TABLE downloads DROP COLUMN executable_relative;
                         ALTER TABLE downloads DROP COLUMN final_path;
                         ALTER TABLE downloads DROP COLUMN staged_path;
                         DROP INDEX games_executable_path_idx;
                         DROP TRIGGER IF EXISTS library_dirty_games_UPDATE; ALTER TABLE games DROP COLUMN executable_path;
                         DROP TABLE game_native_launch_config; DROP TABLE game_sessions;
                         DROP TABLE game_compatibility_overrides;
                         DROP TABLE compatibility_defaults;
                         PRAGMA user_version = 7;",
                    )
                    .map_err(db_error)
            })
            .unwrap();
        drop(database);
        let reopened = Database::open(&directory).unwrap();
        assert_eq!(list(&reopened).unwrap()[0].id, job.id);
        assert_eq!(
            list(&reopened).unwrap()[0].status,
            DownloadStatus::Downloaded
        );
        let staged_path: Option<String> = reopened
            .with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT staged_path FROM downloads WHERE id = ?1",
                        [&job.id],
                        |row| row.get(0),
                    )
                    .map_err(db_error)
            })
            .unwrap();
        assert_eq!(staged_path, None);
        drop(reopened);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn queue_survives_reopen_and_recovers_interrupted_transfer() {
        let (database, directory) = database_with_source();
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        assert_eq!(claim(&database).unwrap().unwrap().id, job.id);
        drop(database);
        let reopened = Database::open(&directory).unwrap();
        recover(&reopened).unwrap();
        assert_eq!(list(&reopened).unwrap()[0].status, DownloadStatus::Queued);
        change_status(
            &reopened,
            &job.id,
            &[DownloadStatus::Queued],
            DownloadStatus::Paused,
        )
        .unwrap();
        assert_eq!(list(&reopened).unwrap()[0].status, DownloadStatus::Paused);
        drop(reopened);
        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn waiting_download_waits_for_connectivity_retry() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/archive", listener.local_addr().unwrap());
        let (archive, hash) = install_archive_fixture();
        let response = archive_response(archive);
        let (request_sent, request_received) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            request_sent
                .send(String::from_utf8(request).unwrap())
                .unwrap();
            stream.write_all(&response).unwrap();
        });

        let (database, directory) = database_with_source();
        let app = test_app(&directory);
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET mirror_plan = NULL, url = ?2, status = 'waiting', error = 'Network: unavailable',
                         sha256 = ?3, size_bytes = ?4, published_size_bytes = ?4 WHERE id = ?1",
                        params![job.id, url, hash, archive.len() as i64],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();

        let app_handle = app.handle().clone();
        kick(app_handle.clone());
        tokio::time::sleep(Duration::from_millis(1)).await;
        tokio::time::advance(Duration::from_secs(60)).await;
        assert!(matches!(
            request_received.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert_eq!(status(&database, &job.id).unwrap(), "waiting");
        assert!(
            app.state::<DownloadQueueState>()
                .running
                .load(Ordering::Acquire)
        );

        resume_waiting(app_handle.clone()).await.unwrap();
        let request = request_received
            .recv_timeout(Duration::from_secs(3))
            .expect("connectivity retry should resume the waiting download");
        assert!(request.starts_with("GET /archive HTTP/1.1"));
        wait_for_installation(&app, &database, &job.id);
        server.join().unwrap();

        drop(app);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn completed_download_is_verified_and_extracted_without_a_user_action() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/archive", listener.local_addr().unwrap());
        let (archive, hash) = install_archive_fixture();
        let response = archive_response(archive);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            stream.write_all(&response).unwrap();
        });

        let (database, directory) = database_with_source();
        let app = test_app(&directory);
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET mirror_plan = NULL, url = ?2, sha256 = ?3, size_bytes = ?4, published_size_bytes = ?4 WHERE id = ?1",
                        params![job.id, url, hash, archive.len() as i64],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();

        kick(app.handle().clone());
        wait_for_installation(&app, &database, &job.id);
        let installed = directory.join("installed/Portal");
        assert_eq!(
            fs::read(installed.join("start.sh")).unwrap(),
            b"#!/bin/sh\nexit 0\n"
        );
        assert!(database.game(&job.id).unwrap().executable_path.is_some());
        server.join().unwrap();

        drop(app);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn remote_download_waits_then_retries_after_connectivity() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/archive", listener.local_addr().unwrap());
        let (archive, hash) = install_archive_fixture();
        let (request_sent, request_received) = mpsc::channel();
        let server = thread::spawn(move || {
            for (index, response) in [
                b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                archive_response(archive),
            ]
            .into_iter()
            .enumerate()
            {
                let deadline = Instant::now() + Duration::from_secs(15);
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(connection) => break connection,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("download request {index} was not received: {error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                request_sent
                    .send((index, String::from_utf8(request).unwrap()))
                    .unwrap();
                stream.write_all(&response).unwrap();
            }
        });

        let (database, directory) = database_with_source();
        let app = test_app(&directory);
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET mirror_plan = NULL, url = ?2, sha256 = ?3, size_bytes = ?4, published_size_bytes = ?4 WHERE id = ?1",
                        params![job.id, url, hash, archive.len() as i64],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();

        let app_handle = app.handle().clone();
        kick(app_handle.clone());
        let (index, request) = request_received
            .recv_timeout(Duration::from_secs(3))
            .expect("first remote request should reach the controlled server");
        assert_eq!(index, 0);
        assert!(request.starts_with("GET /archive HTTP/1.1"));
        wait_for_status(&database, &job.id, "waiting");
        tokio::time::advance(Duration::from_secs(60)).await;
        assert!(matches!(
            request_received.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert_eq!(status(&database, &job.id).unwrap(), "waiting");

        resume_waiting(app_handle.clone()).await.unwrap();
        let (index, _) = request_received
            .recv_timeout(Duration::from_secs(3))
            .expect("connectivity retry should issue one request");
        assert_eq!(index, 1);
        wait_for_status(&database, &job.id, "failed");
        assert!(
            list(&database).unwrap()[0]
                .error
                .as_deref()
                .unwrap()
                .starts_with("HTTP 404")
        );

        retry_download(app_handle.clone(), job.id.clone())
            .await
            .unwrap();
        let (index, _) = request_received
            .recv_timeout(Duration::from_secs(3))
            .expect("manual retry should issue one request");
        assert_eq!(index, 2);
        wait_for_installation(&app, &database, &job.id);
        assert!(retry_download(app_handle, job.id.clone()).await.is_err());
        assert_eq!(status(&database, &job.id).unwrap(), "installed");
        server.join().unwrap();

        drop(app);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn waiting_download_resume_reports_database_failure() {
        let app = test_app(Path::new("\0"));
        let error = resume_waiting(app.handle().clone()).await.unwrap_err();
        assert!(!error.is_empty());
    }

    #[test]
    fn content_range_requires_matching_start_and_total() {
        assert!(content_range_matches("bytes 5-9/10", 5, 10));
        assert!(!content_range_matches("bytes 4-9/10", 5, 10));
        assert!(!content_range_matches("bytes 5-10/10", 5, 10));
        assert!(!content_range_matches("bytes 5-9/11", 5, 10));
    }

    #[test]
    fn published_size_tolerance_keeps_host_length_exact_and_source_size_immutable() {
        for (actual, accepted) in [(97usize, true), (103, true), (96, false), (104, false)] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/archive", listener.local_addr().unwrap());
            let server = thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let mut payload=format!("HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: {actual}\r\nConnection: close\r\n\r\n").into_bytes();
                payload.extend(vec![42; actual]);
                socket.write_all(&payload).unwrap();
            });
            let (database, directory) = database_with_source();
            let database = std::sync::Arc::new(database);
            let job = enqueue(
                &database,
                400,
                "https://example.invalid/portal.zip",
                "1",
                false,
            )
            .unwrap();
            database.with_connection(|connection| {
                connection.execute("UPDATE downloads SET mirror_plan=NULL,url=?2,size_bytes=100,published_size_bytes=100 WHERE id=?1",params![job.id,url]).map_err(db_error)?;
                Ok(())
            }).unwrap();
            let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
            let claimed = claim(&database).unwrap().unwrap();
            let result = tauri::async_runtime::block_on(transfer(
                &queue,
                database.clone(),
                &claimed,
                online,
            ));
            assert_eq!(result.is_ok(), accepted, "size {actual}: {result:?}");
            if accepted {
                assert_eq!(
                    fs::metadata(queue.path(&job.id, "archive").unwrap())
                        .unwrap()
                        .len(),
                    actual as u64
                );
                let sizes: (i64, i64) = database
                    .with_connection(|connection| {
                        connection
                            .query_row(
                                "SELECT size_bytes,published_size_bytes FROM downloads WHERE id=?1",
                                [&job.id],
                                |row| Ok((row.get(0)?, row.get(1)?)),
                            )
                            .map_err(db_error)
                    })
                    .unwrap();
                assert_eq!(sizes, (actual as i64, 100));
            }
            server.join().unwrap();
            drop(database);
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn resumes_with_range_and_keeps_existing_bytes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/archive", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 5-9/10\r\nETag: \"abc\"\r\nContent-Length: 5\r\nConnection: close\r\n\r\nfghij").unwrap();
            String::from_utf8(request).unwrap()
        });
        let (database, directory) = database_with_source();
        let database = std::sync::Arc::new(database);
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET mirror_plan = NULL, url = ?2, etag = '\"abc\"' WHERE id = ?1",
                        params![job.id, url],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
        fs::write(queue.path(&job.id, "part").unwrap(), b"abcde").unwrap();
        let transfer_job = claim(&database).unwrap().unwrap();
        tauri::async_runtime::block_on(transfer(&queue, database.clone(), &transfer_job, online))
            .unwrap();
        assert_eq!(
            fs::read(queue.path(&job.id, "archive").unwrap()).unwrap(),
            b"abcdefghij"
        );
        let request = server.join().unwrap();
        assert!(request.to_ascii_lowercase().contains("range: bytes=5-\r\n"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("if-range: \"abc\"\r\n")
        );
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn resumes_without_a_server_entity_tag() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/archive", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 5-9/10\r\nContent-Length: 5\r\nConnection: close\r\n\r\nfghij").unwrap();
            String::from_utf8(request).unwrap()
        });
        let (database, directory) = database_with_source();
        let database = std::sync::Arc::new(database);
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET mirror_plan = NULL, url = ?2 WHERE id = ?1",
                        params![job.id, url],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
        fs::write(queue.path(&job.id, "part").unwrap(), b"abcde").unwrap();
        let transfer_job = claim(&database).unwrap().unwrap();
        tauri::async_runtime::block_on(transfer(&queue, database.clone(), &transfer_job, online))
            .unwrap();
        assert_eq!(
            fs::read(queue.path(&job.id, "archive").unwrap()).unwrap(),
            b"abcdefghij"
        );
        let request = server.join().unwrap().to_ascii_lowercase();
        assert!(request.contains("range: bytes=5-\r\n"));
        assert!(!request.contains("if-range:"));
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn restarts_when_the_entity_tag_changed() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/archive", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for response in [
                b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 5-9/10\r\nETag: \"changed\"\r\nContent-Length: 5\r\nConnection: close\r\n\r\nfghij".as_slice(),
                b"HTTP/1.1 200 OK\r\nETag: \"changed\"\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123456789".as_slice(),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                stream.write_all(response).unwrap();
                requests.push(String::from_utf8(request).unwrap());
            }
            requests
        });
        let (database, directory) = database_with_source();
        let database = std::sync::Arc::new(database);
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET mirror_plan = NULL, url = ?2, etag = '\"abc\"' WHERE id = ?1",
                        params![job.id, url],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
        fs::write(queue.path(&job.id, "part").unwrap(), b"abcde").unwrap();
        let transfer_job = claim(&database).unwrap().unwrap();
        tauri::async_runtime::block_on(transfer(&queue, database.clone(), &transfer_job, online))
            .unwrap();
        assert_eq!(
            fs::read(queue.path(&job.id, "archive").unwrap()).unwrap(),
            b"0123456789"
        );
        let requests = server.join().unwrap();
        assert!(!requests[1].to_ascii_lowercase().contains("range:"));
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn restarts_when_server_ignores_range() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/archive", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123456789").unwrap();
                requests.push(String::from_utf8(request).unwrap());
            }
            requests
        });
        let (database, directory) = database_with_source();
        let database = std::sync::Arc::new(database);
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET mirror_plan = NULL, url = ?2, etag = '\"abc\"' WHERE id = ?1",
                        params![job.id, url],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
        fs::write(queue.path(&job.id, "part").unwrap(), b"abcde").unwrap();
        let transfer_job = claim(&database).unwrap().unwrap();
        tauri::async_runtime::block_on(transfer(&queue, database.clone(), &transfer_job, online))
            .unwrap();
        assert_eq!(
            fs::read(queue.path(&job.id, "archive").unwrap()).unwrap(),
            b"0123456789"
        );
        let requests = server.join().unwrap();
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .contains("range: bytes=5-\r\n")
        );
        assert!(!requests[1].to_ascii_lowercase().contains("range:"));
        let etag: Option<String> = database
            .with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT etag FROM downloads WHERE id = ?1",
                        [&job.id],
                        |row| row.get(0),
                    )
                    .map_err(db_error)
            })
            .unwrap();
        assert_eq!(etag, None);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn enqueue_stores_the_given_mirror_plan_in_the_new_row() {
        let (database, directory) = database_with_source();
        let plan = crate::download_selection::MirrorPlan::new(
            vec!["https://example.invalid/portal.zip".into()],
            Some(10),
        );
        let job = enqueue_prepared(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
            None,
            None,
            None,
            PlanChoice::Given(Some(plan)),
        )
        .unwrap();
        let stored: Option<String> = database
            .with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT mirror_plan FROM downloads WHERE id = ?1",
                        [&job.id],
                        |row| row.get(0),
                    )
                    .map_err(db_error)
            })
            .unwrap();
        let stored: crate::download_selection::MirrorPlan =
            serde_json::from_str(&stored.unwrap()).unwrap();
        assert_eq!(stored.uris, ["https://example.invalid/portal.zip"]);
        assert_eq!(stored.expected_size, Some(10));
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    fn set_status(database: &Database, id: &str, status: &str) {
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET status = ?2 WHERE id = ?1",
                        params![id, status],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
    }

    fn status_of(database: &Database, id: &str) -> Option<String> {
        database
            .with_connection(|connection| {
                connection
                    .query_row("SELECT status FROM downloads WHERE id = ?1", [id], |row| {
                        row.get(0)
                    })
                    .optional()
                    .map_err(db_error)
            })
            .unwrap()
    }

    #[test]
    fn unfinished_work_ignores_failures_and_user_decided_states() {
        let (database, directory) = database_with_source();
        let queue = DownloadQueueState::new(Ok(directory), "test").unwrap();
        let job = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        let unfinished = |status: &str, reason: Option<&str>, error: Option<&str>| {
            database
                .with_connection(|connection| {
                    connection.execute(
                    "UPDATE downloads SET status = ?2, pause_reason = ?3, error = ?4 WHERE id = ?1",
                    params![job.id, status, reason, error],
                ).map_err(db_error)?;
                    Ok(())
                })
                .unwrap();
            has_unfinished_work(&queue, &database).unwrap()
        };
        for status in [
            "queued",
            "downloading",
            "waiting",
            "downloaded",
            "staging",
            "staged",
            "finalizing",
        ] {
            assert!(unfinished(status, None, None), "{status}");
        }
        assert!(unfinished("paused", None, None));
        assert!(unfinished("paused", Some("game_running"), None));
        assert!(!unfinished("paused", Some("automatic_disabled"), None));
        for status in ["downloaded", "staged", "finalizing"] {
            assert!(
                !unfinished(status, None, Some("extraction failed")),
                "{status}"
            );
        }
        for status in ["failed", "installed", "cancelled"] {
            assert!(!unfinished(status, None, None), "{status}");
        }
        queue.processing.lock().unwrap().insert(job.id.clone());
        assert!(has_unfinished_work(&queue, &database).unwrap());
    }

    #[test]
    fn removes_a_cancelled_entry_with_its_queued_files() {
        let (database, queue, job, directory) = downloaded_fixture();
        set_status(&database, &job.id, "cancelled");
        let partial = queue.path(&job.id, "part").unwrap();
        let stage = queue.path(&job.id, "stage").unwrap();
        fs::write(&partial, b"partial").unwrap();
        fs::create_dir_all(stage.join("Game")).unwrap();
        fs::write(stage.join("Game/game.exe"), b"binary").unwrap();

        remove_entry(&queue, &database, &job.id).unwrap();

        assert_eq!(status_of(&database, &job.id), None);
        assert!(list(&database).unwrap().is_empty());
        assert!(!partial.exists());
        assert!(!queue.path(&job.id, "archive").unwrap().exists());
        assert!(!stage.exists());
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cancelling_a_failed_job_removes_its_archive_and_stage() {
        let (database, queue, job, directory) = downloaded_fixture();
        set_status(&database, &job.id, "failed");
        let archive = queue.path(&job.id, "archive").unwrap();
        let stage = queue.path(&job.id, "stage").unwrap();
        fs::create_dir_all(stage.join("Game")).unwrap();
        fs::write(stage.join("Game/game.exe"), b"binary").unwrap();
        assert!(archive.exists());

        remove_cancelled_files(&queue, &job.id, false).unwrap();

        assert!(!archive.exists());
        assert!(!stage.exists());
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn startup_cleanup_removes_the_archive_of_a_cancelled_job() {
        let (database, queue, job, directory) = downloaded_fixture();
        set_status(&database, &job.id, "cancelled");
        let archive = queue.path(&job.id, "archive").unwrap();
        let stage = queue.path(&job.id, "stage").unwrap();
        fs::create_dir_all(&stage).unwrap();

        clean_cancelled(&queue, &database).unwrap();

        assert!(!archive.exists());
        assert!(!stage.exists());
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn removes_a_failed_entry_without_a_confirmation() {
        let (database, queue, job, directory) = downloaded_fixture();
        set_status(&database, &job.id, "failed");

        remove_entry(&queue, &database, &job.id).unwrap();

        assert_eq!(status_of(&database, &job.id), None);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn refuses_to_remove_an_entry_that_can_still_progress() {
        let (database, queue, job, directory) = downloaded_fixture();
        for status in ["downloaded", "staged", "finalizing", "downloading"] {
            set_status(&database, &job.id, status);
            let error = remove_entry(&queue, &database, &job.id).unwrap_err();
            assert_eq!(error, format!("Cannot remove a {status} download"));
            assert_eq!(status_of(&database, &job.id).as_deref(), Some(status));
        }
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn removing_an_installed_entry_keeps_the_installed_game() {
        let (database, queue, job, directory) = downloaded_fixture();
        set_status(&database, &job.id, "installed");
        let installed = directory.join("installed").join(&job.id);
        fs::create_dir_all(installed.join("bin")).unwrap();
        let executable = installed.join("bin/game.exe");
        fs::write(&executable, b"binary").unwrap();
        fs::write(installed.join("OnlineFix64.dll"), b"fix").unwrap();
        database.with_connection(|connection| {
            connection.execute("INSERT INTO games (id, automatic_name, executable_path) VALUES (?1, 'Game', ?2)", params![job.id, executable.to_string_lossy()]).map_err(db_error)?;
            connection.execute("UPDATE downloads SET final_path = ?2 WHERE id = ?1", params![job.id, installed.to_string_lossy()]).map_err(db_error)?;
            Ok(())
        }).unwrap();

        remove_entry(&queue, &database, &job.id).unwrap();

        assert_eq!(status_of(&database, &job.id), None);
        assert!(executable.exists());
        let game = database.game(&job.id).unwrap();
        assert_eq!(game.installation_root.as_deref(), installed.to_str());
        assert!(crate::online_fix::detected(&database, &game).unwrap());
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn finished_cleanup_keeps_retryable_failures() {
        let (database, _queue, cancelled, directory) = downloaded_fixture();
        let mut ids = vec![cancelled.id.clone()];
        for _ in 0..2 {
            ids.push(
                enqueue(
                    &database,
                    400,
                    "https://example.invalid/portal.zip",
                    "1",
                    false,
                )
                .unwrap()
                .id,
            );
        }
        let installed = ids[1].clone();
        let failed = ids[2].clone();
        set_status(&database, &cancelled.id, "cancelled");
        set_status(&database, &installed, "installed");
        set_status(&database, &failed, "failed");

        let mut selected = finished_ids(&database).unwrap();
        selected.sort();
        let mut expected = vec![cancelled.id.clone(), installed.clone()];
        expected.sort();
        assert_eq!(selected, expected);
        for id in [cancelled.id.clone(), installed.clone()] {
            remove_entry(&_queue, &database, &id).unwrap();
        }

        assert_eq!(status_of(&database, &cancelled.id), None);
        assert_eq!(status_of(&database, &installed), None);
        assert_eq!(status_of(&database, &failed).as_deref(), Some("failed"));
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_remove_a_symlinked_stage_directory() {
        use std::os::unix::fs::symlink;

        let (database, queue, job, directory) = downloaded_fixture();
        set_status(&database, &job.id, "cancelled");
        let outside = directory.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.bin"), b"keep").unwrap();
        symlink(&outside, queue.path(&job.id, "stage").unwrap()).unwrap();

        let error = remove_entry(&queue, &database, &job.id).unwrap_err();
        assert!(error.contains("preserved for inspection"), "{error}");

        assert_eq!(fs::read(outside.join("keep.bin")).unwrap(), b"keep");
        assert_eq!(status_of(&database, &job.id).as_deref(), Some("cancelled"));
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }
}
