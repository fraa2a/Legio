use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::{
    StatusCode,
    header::{CONTENT_RANGE, ETAG, IF_RANGE, RANGE},
};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use tauri::{AppHandle, Manager};
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
    size_bytes: u64,
    downloaded_bytes: u64,
    speed_bps: u64,
    eta_seconds: Option<u64>,
    status: String,
    error: Option<String>,
}

#[derive(Debug)]
struct Transfer {
    id: String,
    url: String,
    size: u64,
    etag: Option<String>,
}

pub struct DownloadQueueState {
    directory: Result<PathBuf, String>,
    client: reqwest::Client,
    running: AtomicBool,
    bandwidth_limit: AtomicU64,
    wake: tokio::sync::Notify,
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
            directory: data_dir
                .map(|path| path.join("downloads"))
                .map_err(|error| format!("Could not locate download directory: {error}")),
            client,
            running: AtomicBool::new(false),
            bandwidth_limit: AtomicU64::new(0),
            wake: tokio::sync::Notify::new(),
        })
    }

    fn directory(&self) -> Result<&Path, String> {
        let path = self.directory.as_ref().map_err(Clone::clone)?;
        fs::create_dir_all(path)
            .map_err(|error| format!("Could not create download directory: {error}"))?;
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("Could not inspect download directory: {error}"))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("Download directory is not a regular directory".to_owned());
        }
        Ok(path)
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
    })
}

fn list(database: &Database) -> Result<Vec<DownloadJob>, String> {
    database.with_connection(|connection| {
        let mut statement = connection
            .prepare(
                "SELECT id, steam_app_id, name, release_version, size_bytes, downloaded_bytes,
                    speed_bps, eta_seconds, status, error FROM downloads ORDER BY created_at, id",
            )
            .map_err(db_error)?;
        statement
            .query_map([], job_from_row)
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)
    })
}

fn change_status(database: &Database, id: &str, from: &[&str], to: &str) -> Result<(), String> {
    let id = Uuid::parse_str(id)
        .map_err(|_| "Download ID is invalid".to_owned())?
        .to_string();
    let updated = now()?;
    database.with_connection(|connection| {
        let status: Option<String> = connection.query_row(
            "SELECT status FROM downloads WHERE id = ?1", [&id], |row| row.get(0),
        ).optional().map_err(db_error)?;
        let Some(status) = status else { return Err("Download was not found".to_owned()); };
        if !from.contains(&status.as_str()) {
            return Err(format!("Cannot {to} a {status} download"));
        }
        connection.execute(
            "UPDATE downloads SET status = ?2, error = NULL, speed_bps = 0, eta_seconds = NULL, updated_at = ?3 WHERE id = ?1",
            params![id, to, updated],
        ).map_err(db_error)?;
        Ok(())
    })
}

pub fn enqueue(
    database: &Database,
    app_id: u32,
    accept_unverified: bool,
) -> Result<DownloadJob, String> {
    let cache = legio_source_cache::cached(database)?
        .ok_or_else(|| "No valid Legio source is cached".to_owned())?;
    let verified = cache
        .manifest
        .verified
        .iter()
        .find(|entry| entry.steam_app_id == app_id);
    let unverified = cache
        .manifest
        .unverified
        .iter()
        .find(|entry| entry.steam_app_id == app_id);
    if unverified.is_some() && !accept_unverified {
        return Err("Confirm the unverified source before downloading".to_owned());
    }
    let entry = verified
        .or(unverified)
        .ok_or_else(|| "Download is unavailable for this game".to_owned())?;
    let size = i64::try_from(entry.download.size_bytes)
        .map_err(|_| "Download size exceeds local storage limits".to_owned())?;
    let id = Uuid::new_v4().to_string();
    let created = now()?;
    database.with_connection(|connection| {
        connection.execute(
            "INSERT INTO downloads (id, steam_app_id, name, release_version, url, sha256,
             size_bytes, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'queued', ?8, ?8)",
            params![id, app_id, entry.name, entry.release.version, entry.download.url,
                entry.download.sha256, size, created],
        ).map_err(db_error)?;
        Ok(())
    })?;
    Ok(DownloadJob {
        id,
        steam_app_id: app_id,
        name: entry.name.clone(),
        release_version: entry.release.version.clone(),
        size_bytes: entry.download.size_bytes,
        downloaded_bytes: 0,
        speed_bps: 0,
        eta_seconds: None,
        status: "queued".to_owned(),
        error: None,
    })
}

fn claim(database: &Database) -> Result<Option<Transfer>, String> {
    database.with_connection(|connection| {
        let row: Option<(String, String, i64, Option<String>)> = connection.query_row(
            "SELECT id, url, size_bytes, etag FROM downloads WHERE status = 'queued' ORDER BY created_at, id LIMIT 1",
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

fn finish(database: &Database, id: &str, to: &str, error: Option<String>) -> Result<(), String> {
    database.with_connection(|connection| {
        connection
            .execute(
                "UPDATE downloads SET status = ?2, error = ?3, speed_bps = 0, eta_seconds = NULL,
             updated_at = ?4 WHERE id = ?1 AND status = 'downloading'",
                params![id, to, error, now()?],
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
    let archive = queue.path(&id, "archive")?;
    let stage = queue.path(&id, "stage")?;
    let hash = database.with_connection(|connection| {
        let row: Option<(String, String)> = connection
            .query_row(
                "SELECT sha256, status FROM downloads WHERE id = ?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(db_error)?;
        let Some((hash, status)) = row else {
            return Err("Download was not found".into());
        };
        if status != "downloaded" {
            return Err(format!("Cannot stage a {status} download"));
        }
        connection.execute(
            "UPDATE downloads SET status = 'staging', error = NULL, updated_at = ?2 WHERE id = ?1",
            params![id, now()?],
        ).map_err(db_error)?;
        Ok(hash)
    })?;

    let outcome = remove_stage(&stage)
        .and_then(|()| archive_install::verify_and_stage(&archive, &hash, &stage));
    match outcome {
        Ok(()) => {
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

fn has_queued(database: &Database) -> Result<bool, String> {
    database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM downloads WHERE status = 'queued')",
                [],
                |row| row.get(0),
            )
            .map_err(db_error)
    })
}

fn has_waiting(database: &Database) -> Result<bool, String> {
    database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM downloads WHERE status = 'waiting')",
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

fn kick(app: AppHandle) {
    let queue = app.state::<DownloadQueueState>();
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
        match app.state::<DatabaseState>().database().and_then(has_queued) {
            Ok(true) => kick(app),
            Ok(false) => {}
            Err(error) => eprintln!("Could not restart download queue: {error}"),
        }
    });
}

async fn run_queue(app: &AppHandle) -> Result<(), String> {
    loop {
        let database = app.state::<DatabaseState>();
        let database = database.database()?;
        let Some(job) = claim(database)? else {
            if !has_waiting(database)? {
                break;
            }
            let queue = app.state::<DownloadQueueState>();
            tokio::select! {
                () = tokio::time::sleep(Duration::from_secs(30)) => {},
                () = queue.wake.notified() => {},
            }
            if wake_waiting(database)? {
                continue;
            }
            break;
        };
        let outcome = transfer(&app.state::<DownloadQueueState>(), database, &job).await;
        match outcome {
            Ok(()) => finish(database, &job.id, "downloaded", None)?,
            Err(error) => {
                if status(database, &job.id)? == "downloading" {
                    let waiting = error.starts_with("Network:") || error.starts_with("HTTP 5");
                    finish(
                        database,
                        &job.id,
                        if waiting { "waiting" } else { "failed" },
                        Some(error),
                    )?;
                }
            }
        }
        if status(database, &job.id)? == "cancelled" {
            let path = app.state::<DownloadQueueState>().path(&job.id, "part")?;
            if let Err(error) = remove_partial(&path) {
                let message = format!("Could not remove partial download: {error}");
                cancellation_error(database, &job.id, &message)?;
            }
        }
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

async fn transfer(
    queue: &DownloadQueueState,
    database: &Database,
    job: &Transfer,
) -> Result<(), String> {
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
    if offset >= job.size || (offset > 0 && job.etag.is_none()) {
        offset = 0;
    }
    let mut request = queue.client.get(&job.url);
    if offset > 0 {
        request = request
            .header(RANGE, format!("bytes={offset}-"))
            .header(IF_RANGE, job.etag.as_deref().unwrap_or_default());
    }
    let mut response = request
        .send()
        .await
        .map_err(|error| format!("Network: {}", error.without_url()))?;
    if offset > 0 {
        let valid = response.status() == StatusCode::PARTIAL_CONTENT
            && response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| content_range_matches(value, offset, job.size))
            && response
                .headers()
                .get(ETAG)
                .and_then(|value| value.to_str().ok())
                == job.etag.as_deref();
        if !valid {
            offset = 0;
            response = queue
                .client
                .get(&job.url)
                .send()
                .await
                .map_err(|error| format!("Network: {}", error.without_url()))?;
        }
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
    let etag = response
        .headers()
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.starts_with("W/"))
        .map(str::to_owned);
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(offset == 0)
        .append(offset > 0)
        .open(&partial)
        .await
        .map_err(|error| format!("Could not open partial download: {error}"))?;
    update_progress(database, &job.id, offset, 0, None, etag.as_deref())?;
    let started = Instant::now();
    let initial = offset;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Network: {}", error.without_url()))?
    {
        if status(database, &job.id)? != "downloading" {
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
        let limit = queue.bandwidth_limit.load(Ordering::Relaxed);
        if limit > 0 {
            let target = Duration::from_secs_f64((offset - initial) as f64 / limit as f64);
            if target > started.elapsed() {
                tokio::time::sleep(target - started.elapsed()).await;
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
        update_progress(database, &job.id, offset, speed, eta, etag.as_deref())?;
    }
    if offset != job.size {
        return Err(format!(
            "Network: Download ended at {offset} of {} bytes",
            job.size
        ));
    }
    file.sync_all()
        .await
        .map_err(|error| format!("Could not sync download: {error}"))?;
    drop(file);
    if status(database, &job.id)? != "downloading" {
        return Err("Download was stopped".to_owned());
    }
    tokio::fs::rename(&partial, &archive)
        .await
        .map_err(|error| format!("Could not finalize downloaded archive: {error}"))?;
    Ok(())
}

#[tauri::command]
pub async fn list_downloads(app: AppHandle) -> Result<Vec<DownloadJob>, String> {
    tauri::async_runtime::spawn_blocking(move || list(app.state::<DatabaseState>().database()?))
        .await
        .map_err(|error| format!("Download list task failed: {error}"))?
}

#[tauri::command]
pub async fn stage_download(app: AppHandle, id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let stage = stage_one(
            &app.state::<DownloadQueueState>(),
            app.state::<DatabaseState>().database()?,
            &id,
        )?;
        Ok(stage.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| format!("Staging task failed: {error}"))?
}

#[tauri::command]
pub async fn queue_download(
    app: AppHandle,
    steam_app_id: u32,
    accept_unverified: bool,
) -> Result<DownloadJob, String> {
    let worker_app = app.clone();
    let job = tauri::async_runtime::spawn_blocking(move || {
        app.state::<DownloadQueueState>().directory()?;
        enqueue(
            app.state::<DatabaseState>().database()?,
            steam_app_id,
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
    tauri::async_runtime::spawn_blocking(move || {
        change_status(
            app.state::<DatabaseState>().database()?,
            &id,
            &["queued", "downloading", "waiting"],
            "paused",
        )
    })
    .await
    .map_err(|error| format!("Pause task failed: {error}"))?
}

#[tauri::command]
pub async fn resume_download(app: AppHandle, id: String) -> Result<(), String> {
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        change_status(
            app.state::<DatabaseState>().database()?,
            &id,
            &["paused", "waiting"],
            "queued",
        )
    })
    .await
    .map_err(|error| format!("Resume task failed: {error}"))??;
    kick(worker_app);
    Ok(())
}

#[tauri::command]
pub async fn retry_download(app: AppHandle, id: String) -> Result<(), String> {
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        change_status(
            app.state::<DatabaseState>().database()?,
            &id,
            &["failed"],
            "queued",
        )
    })
    .await
    .map_err(|error| format!("Retry task failed: {error}"))??;
    kick(worker_app);
    Ok(())
}

#[tauri::command]
pub async fn cancel_download(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let queue = app.state::<DownloadQueueState>();
        let database = app.state::<DatabaseState>();
        let was_downloading = status(database.database()?, &id)? == "downloading";
        change_status(
            database.database()?,
            &id,
            &["queued", "downloading", "paused", "waiting", "failed"],
            "cancelled",
        )?;
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
pub fn set_download_bandwidth_limit(app: AppHandle, bytes_per_second: u64) -> Result<(), String> {
    if bytes_per_second > i64::MAX as u64 {
        return Err("Bandwidth limit is too large".to_owned());
    }
    app.state::<DownloadQueueState>()
        .bandwidth_limit
        .store(bytes_per_second, Ordering::Relaxed);
    Ok(())
}

pub fn start(app: AppHandle) -> Result<(), String> {
    let database = app.state::<DatabaseState>();
    let database = database.database()?;
    clean_cancelled(&app.state::<DownloadQueueState>(), database)?;
    recover_staging(&app.state::<DownloadQueueState>(), database)?;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Could not locate app data: {error}"))?;
    finalize_install::recover(database, &data_dir)?;
    recover(database)?;
    kick(app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn database_with_source() -> (Database, PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("legio-download-test-{}", Uuid::new_v4()));
        let database = Database::open(&directory).unwrap();
        let manifest = br#"{"schemaVersion":1,"generatedAt":"2026-09-22T00:00:00Z","verified":[{"steamAppId":400,"name":"Portal","release":{"version":"1","publishedAt":"2026-09-22T00:00:00Z"},"download":{"url":"https://example.invalid/portal.zip","sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","sizeBytes":10}}],"unverified":[]}"#;
        database.with_connection(|connection| {
            connection.execute("INSERT INTO legio_source_cache (id, manifest, fetched_at) VALUES (1, ?1, 1)", [manifest.as_slice()]).map_err(db_error)?;
            Ok(())
        }).unwrap();
        (database, directory)
    }

    fn downloaded_fixture() -> (Database, DownloadQueueState, DownloadJob, PathBuf) {
        let (database, directory) = database_with_source();
        let job = enqueue(&database, 400, false).unwrap();
        let bytes = include_bytes!("../test-fixtures/archive/safe.zip");
        let queue = DownloadQueueState::new(Ok(directory.clone()), "test").unwrap();
        fs::write(queue.path(&job.id, "archive").unwrap(), bytes).unwrap();
        let hash = format!("{:x}", Sha256::digest(bytes));
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
        assert_eq!(list(&database).unwrap()[0].status, "staged");
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
        assert_eq!(list(&reopened).unwrap()[0].status, "staged");
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
        assert_eq!(list(&database).unwrap()[0].status, "failed");
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
        assert_eq!(list(&database).unwrap()[0].status, "staging");
        recover_staging(&queue, &database).unwrap();
        assert_eq!(list(&database).unwrap()[0].status, "downloaded");
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
                        "ALTER TABLE downloads DROP COLUMN install_token;
                         ALTER TABLE downloads DROP COLUMN executable_relative;
                         ALTER TABLE downloads DROP COLUMN final_path;
                         ALTER TABLE downloads DROP COLUMN staged_path;
                         DROP INDEX games_executable_path_idx;
                         ALTER TABLE games DROP COLUMN executable_path;
                         PRAGMA user_version = 7;",
                    )
                    .map_err(db_error)
            })
            .unwrap();
        drop(database);
        let reopened = Database::open(&directory).unwrap();
        assert_eq!(list(&reopened).unwrap()[0].id, job.id);
        assert_eq!(list(&reopened).unwrap()[0].status, "downloaded");
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
        let job = enqueue(&database, 400, false).unwrap();
        assert_eq!(claim(&database).unwrap().unwrap().id, job.id);
        drop(database);
        let reopened = Database::open(&directory).unwrap();
        recover(&reopened).unwrap();
        assert_eq!(list(&reopened).unwrap()[0].status, "queued");
        change_status(&reopened, &job.id, &["queued"], "paused").unwrap();
        assert_eq!(list(&reopened).unwrap()[0].status, "paused");
        drop(reopened);
        fs::remove_dir_all(directory).unwrap();
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
        let job = enqueue(&database, 400, false).unwrap();
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
        tauri::async_runtime::block_on(transfer(&queue, &database, &transfer_job)).unwrap();
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
        let job = enqueue(&database, 400, false).unwrap();
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
        tauri::async_runtime::block_on(transfer(&queue, &database, &transfer_job)).unwrap();
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
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }
}
