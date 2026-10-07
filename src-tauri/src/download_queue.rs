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
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;
use uuid::Uuid;

use crate::{
    archive_install,
    database::{Database, DatabaseState},
    finalize_install, legio_source_cache,
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

#[derive(Debug)]
struct Transfer {
    id: String,
    name: String,
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
    staging: tokio::sync::Mutex<()>,
    defer_extraction: AtomicBool,
    staging_changed: tokio::sync::Notify,
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
            staging: tokio::sync::Mutex::new(()),
            defer_extraction: AtomicBool::new(false),
            staging_changed: tokio::sync::Notify::new(),
        })
    }

    pub(crate) fn set_defer_extraction(&self, enabled: bool) {
        self.defer_extraction.store(enabled, Ordering::Release);
        self.staging_changed.notify_waiters();
    }

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

    fn interrupt(&self, id: &str) -> Result<(), String> {
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
    })
}

fn list(database: &Database) -> Result<Vec<DownloadJob>, String> {
    database.with_connection(|connection| {
        let mut statement = connection
            .prepare(
                "SELECT id, steam_app_id, name, release_version, size_bytes, downloaded_bytes,
                    speed_bps, eta_seconds, status, error, sha256, url, updated_at
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

pub fn enqueue(
    database: &Database,
    app_id: u32,
    download_url: &str,
    release_version: &str,
    accept_unverified: bool,
) -> Result<DownloadJob, String> {
    let cache = legio_source_cache::cached(database)?
        .ok_or_else(|| "No valid Legio source is cached".to_owned())?;
    let verified = cache.manifest.verified.iter().find(|entry| {
        entry.steam_app_id == app_id
            && entry.download.url == download_url
            && entry.release.version == release_version
    });
    let unverified = cache.manifest.unverified.iter().find(|entry| {
        entry.steam_app_id == app_id
            && entry.download.url == download_url
            && entry.release.version == release_version
    });
    if verified.is_none() && unverified.is_some() && !accept_unverified {
        return Err("Confirm the unverified source before downloading".to_owned());
    }
    let source_verified = verified.is_some();
    let entry = verified
        .or(unverified)
        .ok_or_else(|| "Download is unavailable for this game".to_owned())?;
    let size = i64::try_from(entry.download.size_bytes)
        .map_err(|_| "Download size exceeds local storage limits".to_owned())?;
    let id = Uuid::new_v4().to_string();
    let created = now()?;
    database.with_connection(|connection| {
        connection
            .execute(
                "INSERT INTO downloads (id, steam_app_id, name, release_version, url, sha256,
             size_bytes, status, created_at, updated_at, source_verified, queue_position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'queued', ?8, ?8, ?9, ?8)",
                params![
                    id,
                    app_id,
                    entry.name,
                    entry.release.version,
                    entry.download.url,
                    entry.download.sha256.as_deref().unwrap_or(""),
                    size,
                    created,
                    source_verified
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
        size_bytes: entry.download.size_bytes,
        downloaded_bytes: 0,
        speed_bps: 0,
        eta_seconds: None,
        status: DownloadStatus::Queued,
        error: None,
        updated_at: created,
    })
}

fn claim(database: &Database) -> Result<Option<Transfer>, String> {
    database.with_connection(|connection| {
        let row: Option<(String, String, String, i64, Option<String>)> = connection.query_row(
            "SELECT id, name, url, size_bytes, etag FROM downloads WHERE status = 'queued' ORDER BY queue_position, created_at, id LIMIT 1",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        ).optional().map_err(db_error)?;
        let Some((id, name, url, size, etag)) = row else { return Ok(None); };
        connection.execute(
            "UPDATE downloads SET status = 'downloading', updated_at = ?2 WHERE id = ?1",
            params![id, now()?],
        ).map_err(db_error)?;
        Ok(Some(Transfer { id, name, url, size: u64::try_from(size).map_err(|_| "Invalid stored download size".to_owned())?, etag }))
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
        let verify = verified && database.settings()?.verify_verified_downloads;
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

fn spawn_staging<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let app = app.clone();
    let id = id.to_owned();
    tauri::async_runtime::spawn(async move {
        let queue = app.state::<DownloadQueueState>();
        let _staging = queue.staging.lock().await;
        if let Some(manager) = app.try_state::<crate::game_lifecycle::GameLaunchManager>()
            && let Err(error) = queue.wait_for_idle(&manager).await
        {
            eprintln!("Could not schedule download extraction {id}: {error}");
            return;
        }
        let worker_app = app.clone();
        let worker_id = id.clone();
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            stage_one(
                &worker_app.state::<DownloadQueueState>(),
                worker_app.state::<DatabaseState>().database()?,
                &worker_id,
            )
        })
        .await;
        match outcome {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => eprintln!("Could not verify and extract download {id}: {error}"),
            Err(error) => eprintln!("Download staging task failed for {id}: {error}"),
        }
    });
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
        let stage = queue.path(&id, "stage")?;
        let archive = queue.path(&id, "archive")?;
        let cleanup = remove_stage(&stage);
        let next = if cleanup.is_ok() {
            if archive.is_file() {
                "downloaded"
            } else {
                "queued"
            }
        } else {
            "failed"
        };
        database.with_connection(|connection| {
            connection.execute(
                "UPDATE downloads SET status = ?2, error = ?3, staged_path = NULL, updated_at = ?4 WHERE id = ?1 AND status = 'staging'",
                params![id, next, cleanup.err(), now()?],
            ).map_err(db_error)?;
            Ok(())
        })?;
    }
    Ok(())
}

fn downloaded_ids(database: &Database) -> Result<Vec<String>, String> {
    database.with_connection(|connection| {
        let mut statement = connection
            .prepare("SELECT id FROM downloads WHERE status = 'downloaded' ORDER BY queue_position, created_at, id")
            .map_err(db_error)?;
        statement
            .query_map([], |row| row.get(0))
            .map_err(db_error)?
            .collect::<Result<_, _>>()
            .map_err(db_error)
    })
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
        if let Err(error) = remove_partial(&queue.path(&id, "part")?) {
            cancellation_error(
                database,
                &id,
                &format!("Could not remove partial download: {error}"),
            )?;
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

fn kick<R: Runtime>(app: AppHandle<R>) {
    let queue = app.state::<DownloadQueueState>();
    if queue.starting.load(Ordering::Acquire) {
        return;
    }
    queue.wake.notify_one();
    if queue.running.swap(true, Ordering::AcqRel) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_queue(&app).await {
            eprintln!("Download queue stopped: {error}");
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
            Err(error) => eprintln!("Could not restart download queue: {error}"),
        }
    });
}

async fn run_queue<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    loop {
        let database = app.state::<DatabaseState>().shared_database()?;
        let reader = database.clone();
        let (job, waiting) = tauri::async_runtime::spawn_blocking(move || {
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
        let outcome = transfer(
            &app.state::<DownloadQueueState>(),
            app.state::<DatabaseState>().shared_database()?,
            &job,
        )
        .await;
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            match outcome {
                Ok(()) => {
                    finish(&database, &job.id, DownloadStatus::Downloaded, None)?;
                    spawn_staging(&app, &job.id);
                    match database.settings() {
                        Ok(settings) if settings.download_notifications => {
                            if let Err(error) = app
                                .notification()
                                .builder()
                                .title(
                                    settings
                                        .language
                                        .text("Download completato", "Download complete"),
                                )
                                .body(&job.name)
                                .show()
                            {
                                eprintln!("Could not show download notification: {error}");
                            }
                        }
                        Ok(_) => {}
                        Err(error) => {
                            eprintln!("Could not read download notification setting: {error}")
                        }
                    }
                }
                Err(error) => {
                    if status(&database, &job.id)? == "downloading" {
                        let waiting = error.starts_with("Network:") || error.starts_with("HTTP 5");
                        finish(
                            &database,
                            &job.id,
                            if waiting {
                                DownloadStatus::Waiting
                            } else {
                                DownloadStatus::Failed
                            },
                            Some(error),
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

async fn transfer(
    queue: &DownloadQueueState,
    database: std::sync::Arc<Database>,
    job: &Transfer,
) -> Result<(), String> {
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
        return Err("Download was stopped".to_owned());
    }
    use tokio::io::AsyncWriteExt;
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
        Ok(_) => return Err("Partial download is not a regular file".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(format!("Could not inspect partial download: {error}")),
    };
    if offset >= job.size {
        offset = 0;
    }
    let mut request = queue.client.get(&job.url);
    if offset > 0 {
        request = request.header(RANGE, format!("bytes={offset}-"));
        if let Some(etag) = job.etag.as_deref() {
            request = request.header(IF_RANGE, etag);
        }
    }
    let mut response = interruptible_response(&mut stopped, request.send()).await?;
    if offset > 0 && !resume_matches(&response, job, offset) {
        offset = 0;
        response = interruptible_response(&mut stopped, queue.client.get(&job.url).send()).await?;
    }
    if response.status()
        != if offset > 0 {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        }
    {
        return Err(format!("HTTP {}", response.status()));
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
        .map_err(|error| format!("Could not open partial download: {error}"))?;
    persist_progress(database.clone(), &job.id, offset, 0, None, etag.as_deref()).await?;
    let started = Instant::now();
    let initial = offset;
    let mut persisted = Instant::now();
    let mut limiter = BandwidthWindow::new(offset, queue.bandwidth_limit.load(Ordering::Relaxed));
    while let Some(chunk) = interruptible_response(&mut stopped, response.chunk()).await? {
        if *stopped.borrow() {
            return Err("Download was stopped".to_owned());
        }
        let next = offset
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| "Download size overflow".to_owned())?;
        if next > job.size {
            return Err("Download exceeds the expected size".to_owned());
        }
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("Could not write partial download: {error}"))?;
        offset = next;
        loop {
            if *stopped.borrow() {
                persist_progress(database.clone(), &job.id, offset, 0, None, etag.as_deref())
                    .await?;
                return Err("Download was stopped".to_owned());
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
        return Err(format!(
            "Network: Download ended at {offset} of {} bytes",
            job.size
        ));
    }
    persist_progress(database.clone(), &job.id, offset, 0, None, etag.as_deref()).await?;
    file.sync_all()
        .await
        .map_err(|error| format!("Could not sync download: {error}"))?;
    drop(file);
    if *stopped.borrow() {
        return Err("Download was stopped".to_owned());
    }
    tokio::fs::rename(&partial, &archive)
        .await
        .map_err(|error| format!("Could not finalize downloaded archive: {error}"))?;
    Ok(())
}

async fn interruptible_response<T>(
    stopped: &mut tokio::sync::watch::Receiver<bool>,
    response: impl std::future::Future<Output = Result<T, reqwest::Error>>,
) -> Result<T, String> {
    if *stopped.borrow() {
        return Err("Download was stopped".to_owned());
    }
    tokio::select! {
        biased;
        _ = stopped.changed() => Err("Download was stopped".to_owned()),
        result = response => result.map_err(|error| format!("Network: {}", error.without_url())),
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
) -> Result<DownloadJob, String> {
    crate::startup_recovery::wait(&app).await?;
    let worker_app = app.clone();
    let job = tauri::async_runtime::spawn_blocking(move || {
        app.state::<DownloadQueueState>().directory()?;
        enqueue(
            app.state::<DatabaseState>().database()?,
            steam_app_id,
            &download_url,
            &release_version,
            accept_unverified,
        )
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
        change_status(
            app.state::<DatabaseState>().database()?,
            &id,
            &[DownloadStatus::Paused, DownloadStatus::Waiting],
            DownloadStatus::Queued,
        )
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
    tauri::async_runtime::spawn_blocking(move || {
        change_status(
            app.state::<DatabaseState>().database()?,
            &id,
            &[DownloadStatus::Failed],
            DownloadStatus::Queued,
        )
    })
    .await
    .map_err(|error| format!("Retry task failed: {error}"))??;
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
        if let Err(error) = remove_partial(&queue.path(&id, "part")?)
            && (error.kind() != std::io::ErrorKind::PermissionDenied || !was_downloading)
        {
            let message = format!("Could not remove partial download: {error}");
            cancellation_error(database.database()?, &id, &message)?;
            return Err(message);
        }
        Ok(())
    })
    .await
    .map_err(|error| format!("Cancel task failed: {error}"))?
}

#[tauri::command]
pub fn get_download_bandwidth_limit(app: AppHandle) -> Result<u64, String> {
    app.state::<DatabaseState>()
        .database()?
        .download_bandwidth_limit()
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
pub fn set_download_bandwidth_limit(app: AppHandle, bytes_per_second: u64) -> Result<(), String> {
    app.state::<DatabaseState>()
        .database()?
        .save_download_bandwidth_limit(bytes_per_second)?;
    app.state::<DownloadQueueState>()
        .set_bandwidth_limit(bytes_per_second);
    Ok(())
}

fn recover_on_start(app: &AppHandle) -> Result<(), String> {
    let database = app.state::<DatabaseState>();
    let database = database.database()?;
    clean_cancelled(&app.state::<DownloadQueueState>(), database)?;
    recover_staging(&app.state::<DownloadQueueState>(), database)?;
    for id in downloaded_ids(database)? {
        spawn_staging(app, &id);
    }
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Could not locate app data: {error}"))?;
    finalize_install::recover(database, &database.storage_root(&data_dir)?)?;
    recover(database)?;
    Ok(())
}

pub fn start(app: AppHandle) -> Result<(), String> {
    app.state::<DownloadQueueState>()
        .starting
        .store(true, Ordering::Release);
    std::thread::Builder::new()
        .name("download-recovery".to_owned())
        .spawn(move || {
            let result = recover_on_start(&app);
            app.state::<DownloadQueueState>()
                .starting
                .store(false, Ordering::Release);
            if let Err(error) = result {
                eprintln!("Could not recover downloads: {error}");
                if let Err(notification_error) = app
                    .notification()
                    .builder()
                    .title("Legio download recovery failed")
                    .body(&error)
                    .show()
                {
                    eprintln!(
                        "Could not show download recovery notification: {notification_error}"
                    );
                }
            }
            kick(app);
        })
        .map(|_| ())
        .map_err(|error| format!("Could not start download recovery: {error}"))
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
        assert_eq!(result.unwrap_err(), "Download was stopped");
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

    fn test_app(directory: &Path) -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = format!("org.legio.test.{}", Uuid::new_v4());
        tauri::test::mock_builder()
            .manage(DatabaseState::new(Ok(directory.to_path_buf())))
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
        assert!(window.delay(101 * 1024 * 1024, 1024 * 1024) <= Duration::from_secs(1));
        assert!(window.delay(101 * 1024 * 1024, 0).is_zero());
    }

    fn archive_fixture() -> (&'static [u8], String) {
        let bytes: &'static [u8] = include_bytes!("../test-fixtures/archive/safe.zip");
        (bytes, format!("{:x}", Sha256::digest(bytes)))
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
                "UPDATE downloads SET status = 'downloaded', sha256 = ?2, size_bytes = ?3, downloaded_bytes = ?3 WHERE id = ?1",
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
            legio_source_cache::cached(&database)
                .unwrap()
                .unwrap()
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
        assert_eq!(second.release_version, "2");
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
            legio_source_cache::cached(&database)
                .unwrap()
                .unwrap()
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
        for (verified, enabled) in [(false, true), (false, false), (true, false)] {
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
            stage_one(&queue, &database, &job.id).unwrap();
            assert!(queue.path(&job.id, "archive").unwrap().exists());
            assert_eq!(status(&database, &job.id).unwrap(), "staged");
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
    fn startup_staging_covers_only_completed_downloads() {
        let (database, _queue, completed, directory) = downloaded_fixture();
        let queued = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        let staged = enqueue(
            &database,
            400,
            "https://example.invalid/portal.zip",
            "1",
            false,
        )
        .unwrap();
        set_status(&database, &staged.id, "staged");

        assert_eq!(downloaded_ids(&database).unwrap(), vec![completed.id]);
        assert_eq!(status_of(&database, &queued.id).as_deref(), Some("queued"));
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
                         ALTER TABLE games DROP COLUMN executable_path;
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
        let (archive, hash) = archive_fixture();
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
                        "UPDATE downloads SET url = ?2, status = 'waiting', error = 'Network: unavailable',
                         sha256 = ?3, size_bytes = ?4 WHERE id = ?1",
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

        resume_waiting(app_handle).await.unwrap();
        let request = request_received
            .recv_timeout(Duration::from_secs(3))
            .expect("connectivity retry should resume the waiting download");
        assert!(request.starts_with("GET /archive HTTP/1.1"));
        wait_for_status(&database, &job.id, "staged");
        assert_eq!(list(&database).unwrap()[0].error, None);
        server.join().unwrap();

        drop(app);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn completed_download_is_verified_and_extracted_without_a_user_action() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/archive", listener.local_addr().unwrap());
        let (archive, hash) = archive_fixture();
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
                        "UPDATE downloads SET url = ?2, sha256 = ?3, size_bytes = ?4 WHERE id = ?1",
                        params![job.id, url, hash, archive.len() as i64],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();

        kick(app.handle().clone());
        wait_for_status(&database, &job.id, "staged");

        let staged = directory
            .join("downloads")
            .join(format!("{}.stage", job.id));
        assert_eq!(fs::read(staged.join("Game/data.bin")).unwrap(), b"hello");
        assert_eq!(list(&database).unwrap()[0].error, None);
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
        let (archive, hash) = archive_fixture();
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
                        "UPDATE downloads SET url = ?2, sha256 = ?3, size_bytes = ?4 WHERE id = ?1",
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
        wait_for_status(&database, &job.id, "staged");
        assert_eq!(list(&database).unwrap()[0].error, None);
        assert!(retry_download(app_handle, job.id.clone()).await.is_err());
        assert_eq!(status(&database, &job.id).unwrap(), "staged");
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
                        "UPDATE downloads SET url = ?2, etag = '\"abc\"' WHERE id = ?1",
                        params![job.id, url],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
        fs::write(queue.path(&job.id, "part").unwrap(), b"abcde").unwrap();
        let transfer_job = claim(&database).unwrap().unwrap();
        tauri::async_runtime::block_on(transfer(&queue, database.clone(), &transfer_job)).unwrap();
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
                        "UPDATE downloads SET url = ?2 WHERE id = ?1",
                        params![job.id, url],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
        fs::write(queue.path(&job.id, "part").unwrap(), b"abcde").unwrap();
        let transfer_job = claim(&database).unwrap().unwrap();
        tauri::async_runtime::block_on(transfer(&queue, database.clone(), &transfer_job)).unwrap();
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
                        "UPDATE downloads SET url = ?2, etag = '\"abc\"' WHERE id = ?1",
                        params![job.id, url],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
        fs::write(queue.path(&job.id, "part").unwrap(), b"abcde").unwrap();
        let transfer_job = claim(&database).unwrap().unwrap();
        tauri::async_runtime::block_on(transfer(&queue, database.clone(), &transfer_job)).unwrap();
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
                        "UPDATE downloads SET url = ?2, etag = '\"abc\"' WHERE id = ?1",
                        params![job.id, url],
                    )
                    .map_err(db_error)?;
                Ok(())
            })
            .unwrap();
        let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
        fs::write(queue.path(&job.id, "part").unwrap(), b"abcde").unwrap();
        let transfer_job = claim(&database).unwrap().unwrap();
        tauri::async_runtime::block_on(transfer(&queue, database.clone(), &transfer_job)).unwrap();
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
    fn removes_a_failed_entry_without_a_confirmation() {
        let (database, queue, job, directory) = downloaded_fixture();
        set_status(&database, &job.id, "failed");

        assert!(REMOVABLE.contains(&DownloadStatus::Failed));
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

        assert!(outside.join("keep.bin").exists());
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }
}
