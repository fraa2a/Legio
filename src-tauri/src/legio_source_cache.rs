use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::OptionalExtension;
use serde::Serialize;
use tauri::Manager;

use crate::{
    database::{Database, DatabaseState},
    legio_source::{self, Manifest},
    network::NetworkState,
};

const STALE_SECONDS: i64 = 24 * 60 * 60;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSnapshot {
    pub manifest: Option<Manifest>,
    pub cached_at: Option<i64>,
    pub stale: bool,
    pub warning: Option<String>,
}

pub(crate) struct CachedSource {
    pub manifest: Manifest,
    pub fetched_at: i64,
}

pub(crate) fn cached(database: &Database) -> Result<Option<CachedSource>, String> {
    let row: Option<(Vec<u8>, i64)> = database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT manifest, fetched_at FROM legio_source_cache WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| format!("Could not read Legio source cache: {error}"))
    })?;
    row.map(|(bytes, fetched_at)| {
        legio_source::parse_manifest(&bytes)
            .map(|manifest| CachedSource {
                manifest,
                fetched_at,
            })
            .map_err(|error| format!("Cached Legio source is invalid: {error}"))
    })
    .transpose()
}

fn store(database: &Database, manifest: &Manifest, fetched_at: i64) -> Result<(), String> {
    let bytes = serde_json::to_vec(manifest)
        .map_err(|error| format!("Could not encode Legio source cache: {error}"))?;
    if bytes.len() > legio_source::MAX_MANIFEST_BYTES {
        return Err("Legio source cache exceeds the size limit".to_owned());
    }
    database.with_connection(|connection| {
        connection
            .execute(
                "INSERT INTO legio_source_cache (id, manifest, fetched_at) VALUES (1, ?1, ?2)
                 ON CONFLICT (id) DO UPDATE SET manifest = excluded.manifest, fetched_at = excluded.fetched_at",
                rusqlite::params![bytes, fetched_at],
            )
            .map(|_| ())
            .map_err(|error| format!("Could not save Legio source cache: {error}"))
    })
}

fn now() -> Result<i64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .ok_or_else(|| "System clock is outside the supported date range".to_owned())
}

pub(crate) fn is_stale(fetched_at: i64, current_time: i64) -> bool {
    fetched_at > current_time || current_time.saturating_sub(fetched_at) >= STALE_SECONDS
}

fn snapshot(
    cache: Option<CachedSource>,
    current_time: i64,
    warning: Option<String>,
) -> SourceSnapshot {
    let stale = warning.is_some()
        || cache
            .as_ref()
            .is_none_or(|entry| is_stale(entry.fetched_at, current_time));
    let (manifest, cached_at) = match cache {
        Some(entry) => (Some(entry.manifest), Some(entry.fetched_at)),
        None => (None, None),
    };
    SourceSnapshot {
        manifest,
        cached_at,
        stale,
        warning,
    }
}

pub async fn cached_source(app: tauri::AppHandle) -> Result<SourceSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let database = app.state::<DatabaseState>();
        Ok(snapshot(cached(database.database()?)?, now()?, None))
    })
    .await
    .map_err(|error| format!("Legio source cache task failed: {error}"))?
}

pub async fn refresh_source(
    app: tauri::AppHandle,
    network: &NetworkState,
) -> Result<SourceSnapshot, String> {
    let fetched = legio_source::fetch_manifest(network).await;
    tauri::async_runtime::spawn_blocking(move || {
        let database = app.state::<DatabaseState>();
        let database = database.database()?;
        let current_time = now()?;
        match fetched {
            Ok(manifest) => match store(database, &manifest, current_time) {
                Ok(()) => Ok(snapshot(
                    Some(CachedSource {
                        manifest,
                        fetched_at: current_time,
                    }),
                    current_time,
                    None,
                )),
                Err(error) => fallback(database, current_time, error),
            },
            Err(error) => fallback(database, current_time, error),
        }
    })
    .await
    .map_err(|error| format!("Legio source refresh task failed: {error}"))?
}

fn fallback(
    database: &Database,
    current_time: i64,
    error: String,
) -> Result<SourceSnapshot, String> {
    let Some(cache) = cached(database)? else {
        return Err(error);
    };
    Ok(snapshot(Some(cache), current_time, Some(error)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legio_source::parse_manifest;

    fn valid(app_id: u32) -> Manifest {
        parse_manifest(
            format!(
                r#"{{"schemaVersion":1,"generatedAt":"2026-09-22T00:00:00Z","verified":[{{"steamAppId":{app_id},"name":"Portal","release":{{"version":"1","publishedAt":"2026-09-22T00:00:00Z"}},"download":{{"url":"https://example.invalid/a.zip","sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","sizeBytes":1}}}}],"unverified":[]}}"#
            )
            .as_bytes(),
        )
        .unwrap()
    }

    #[test]
    fn invalid_refresh_keeps_last_valid_cache_and_marks_stale() {
        let directory =
            std::env::temp_dir().join(format!("legio-source-cache-{}", uuid::Uuid::new_v4()));
        let database = Database::open(&directory).unwrap();
        store(&database, &valid(400), 100).unwrap();
        assert!(store(&database, &valid(401), -1).is_err());
        let result = fallback(&database, 101, "source is invalid".to_owned()).unwrap();
        assert!(result.stale);
        assert_eq!(result.cached_at, Some(100));
        assert_eq!(result.manifest.unwrap().verified[0].steam_app_id, 400);
        assert_eq!(
            cached(&database).unwrap().unwrap().manifest.verified[0].steam_app_id,
            400
        );
        drop(database);
        let reopened = Database::open(&directory).unwrap();
        assert_eq!(
            cached(&reopened).unwrap().unwrap().manifest.verified[0].steam_app_id,
            400
        );
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cache_age_and_clock_rollback_mark_stale() {
        let directory =
            std::env::temp_dir().join(format!("legio-source-age-{}", uuid::Uuid::new_v4()));
        let database = Database::open(&directory).unwrap();
        store(&database, &valid(400), 100).unwrap();
        let fresh = snapshot(cached(&database).unwrap(), 101, None);
        assert!(!fresh.stale);
        let old = snapshot(cached(&database).unwrap(), 100 + STALE_SECONDS, None);
        assert!(old.stale);
        let rollback = snapshot(cached(&database).unwrap(), 99, None);
        assert!(rollback.stale);
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
