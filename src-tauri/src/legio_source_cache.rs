use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{Manager, Runtime};

use crate::{
    database::{Database, DatabaseState},
    legio_source::{self, Manifest},
    network::NetworkState,
};

const STALE_SECONDS: i64 = 24 * 60 * 60;
const MAX_SOURCES: i64 = 16;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSource {
    pub id: String,
    pub url: String,
    pub cached_at: i64,
    pub stale: bool,
    pub warning: Option<String>,
    pub game_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSnapshot {
    pub manifest: Option<Arc<Manifest>>,
    pub cached_at: Option<i64>,
    pub stale: bool,
    pub warning: Option<String>,
    pub sources: Vec<InstalledSource>,
}

#[derive(Clone)]
pub(crate) struct CachedSource {
    pub manifest: Arc<Manifest>,
    pub fetched_at: i64,
    pub warning: Option<String>,
}

struct SourceRow {
    id: String,
    url: String,
    manifest: Manifest,
    fetched_at: i64,
    warning: Option<String>,
}

fn rows(database: &Database) -> Result<Vec<SourceRow>, String> {
    database.with_connection(|connection| {
        let mut statement = connection.prepare(
            "SELECT id, url, manifest, fetched_at, warning FROM download_sources ORDER BY rowid",
        ).map_err(db_error)?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(db_error)?
            .map(|row| {
                let (id, url, bytes, fetched_at, warning) = row.map_err(db_error)?;
                let manifest = legio_source::parse_manifest(&bytes)
                    .map_err(|error| format!("Cached source is invalid: {error}"))?;
                Ok(SourceRow {
                    id,
                    url,
                    manifest,
                    fetched_at,
                    warning,
                })
            })
            .collect()
    })
}

fn merge(rows: &[SourceRow]) -> Option<CachedSource> {
    if rows.is_empty() {
        return None;
    }
    let mut entries = BTreeMap::new();
    let mut conflicts = false;
    for row in rows {
        for (verified, list) in [
            (true, &row.manifest.verified),
            (false, &row.manifest.unverified),
        ] {
            for entry in list {
                let key = (
                    entry.steam_app_id,
                    entry.download.url.as_str(),
                    entry.release.version.as_str(),
                );
                let saved = entries.entry(key).or_insert(Some((verified, entry)));
                if saved.is_some_and(|(old_verified, old)| old != entry || old_verified != verified)
                {
                    *saved = None;
                    conflicts = true;
                }
            }
        }
    }
    let mut manifest = Manifest {
        schema_version: 1,
        generated_at: rows[0].manifest.generated_at.clone(),
        verified: Vec::new(),
        unverified: Vec::new(),
    };
    for (verified, entry) in entries.into_values().flatten() {
        if verified {
            manifest.verified.push(entry.clone());
        } else {
            manifest.unverified.push(entry.clone());
        }
    }
    Some(CachedSource {
        manifest: Arc::new(manifest),
        fetched_at: rows.iter().map(|row| row.fetched_at).min()?,
        warning: if conflicts {
            Some("Conflicting releases were omitted. Review the installed sources.".to_owned())
        } else if rows.iter().any(|row| row.warning.is_some()) {
            Some(
                "Some sources could not be refreshed. Their cached releases are still available."
                    .to_owned(),
            )
        } else {
            None
        },
    })
}

pub(crate) fn cached(database: &Database) -> Result<Option<CachedSource>, String> {
    let mut cache = database
        .source_cache
        .lock()
        .map_err(|_| "Source cache is unavailable".to_owned())?;
    if cache.is_none() {
        *cache = merge(&rows(database)?);
    }
    Ok(cache.clone())
}

fn snapshot(database: &Database, current_time: i64) -> Result<SourceSnapshot, String> {
    let mut cache = database
        .source_cache
        .lock()
        .map_err(|_| "Source cache is unavailable".to_owned())?;
    let rows = rows(database)?;
    if cache.is_none() {
        *cache = merge(&rows);
    }
    let mut warning = cache.as_ref().and_then(|cache| cache.warning.clone());
    if warning.is_none() && rows.iter().any(|row| row.warning.is_some()) {
        warning = Some(
            "Some sources could not be refreshed. Their cached releases are still available."
                .to_owned(),
        );
    }
    let sources: Vec<_> = rows
        .into_iter()
        .map(|row| InstalledSource {
            id: row.id,
            url: row.url,
            cached_at: row.fetched_at,
            stale: row.warning.is_some() || is_stale(row.fetched_at, current_time),
            warning: row.warning,
            game_count: row.manifest.verified.len() + row.manifest.unverified.len(),
        })
        .collect();
    Ok(SourceSnapshot {
        manifest: cache.as_ref().map(|cache| cache.manifest.clone()),
        cached_at: cache.as_ref().map(|cache| cache.fetched_at),
        stale: warning.is_some() || sources.iter().any(|source| source.stale),
        warning,
        sources,
    })
}

fn encode(manifest: &Manifest) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(manifest)
        .map_err(|error| format!("Could not encode source: {error}"))?;
    if bytes.len() > legio_source::MAX_MANIFEST_BYTES {
        return Err("Source exceeds the size limit".to_owned());
    }
    Ok(bytes)
}

fn insert(
    database: &Database,
    url: &str,
    manifest: Manifest,
    fetched_at: i64,
) -> Result<(), String> {
    let bytes = encode(&manifest)?;
    let mut cache = database
        .source_cache
        .lock()
        .map_err(|_| "Source cache is unavailable".to_owned())?;
    database.with_connection(|connection| {
        let exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM download_sources WHERE url = ?1)", [url], |row| row.get(0)).map_err(db_error)?;
        if exists { return Ok(()); }
        let count: i64 = connection.query_row("SELECT COUNT(*) FROM download_sources", [], |row| row.get(0)).map_err(db_error)?;
        if count >= MAX_SOURCES { return Err(format!("You can install up to {MAX_SOURCES} sources")); }
        connection.execute("INSERT INTO download_sources (id, url, manifest, fetched_at) VALUES (?1, ?2, ?3, ?4)", rusqlite::params![uuid::Uuid::new_v4().to_string(), url, bytes, fetched_at]).map_err(db_error)?;
        Ok(())
    })?;
    *cache = None;
    Ok(())
}

fn update(
    database: &Database,
    id: &str,
    fetched: Result<Manifest, String>,
    current_time: i64,
) -> Result<(), String> {
    let mut cache = database
        .source_cache
        .lock()
        .map_err(|_| "Source cache is unavailable".to_owned())?;
    database.with_connection(|connection| {
        match fetched {
            Ok(manifest) => {
                connection.execute("UPDATE download_sources SET manifest = ?1, fetched_at = ?2, warning = NULL WHERE id = ?3", rusqlite::params![encode(&manifest)?, current_time, id]).map_err(db_error)?;
            }
            Err(error) => {
                connection.execute("UPDATE download_sources SET warning = ?1 WHERE id = ?2", rusqlite::params![error, id]).map_err(db_error)?;
            }
        }
        Ok(())
    })?;
    *cache = None;
    Ok(())
}

fn remove(database: &Database, id: &str) -> Result<(), String> {
    let mut cache = database
        .source_cache
        .lock()
        .map_err(|_| "Source cache is unavailable".to_owned())?;
    database.with_connection(|connection| {
        connection
            .execute("DELETE FROM download_sources WHERE id = ?1", [id])
            .map(|_| ())
            .map_err(db_error)
    })?;
    *cache = None;
    Ok(())
}

fn db_error(error: rusqlite::Error) -> String {
    format!("Could not access installed sources: {error}")
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

pub async fn cached_source<R: Runtime>(app: tauri::AppHandle<R>) -> Result<SourceSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        snapshot(app.state::<DatabaseState>().database()?, now()?)
    })
    .await
    .map_err(|error| format!("Source cache task failed: {error}"))?
}

pub async fn add_source(
    app: tauri::AppHandle,
    network: &NetworkState,
    url: String,
) -> Result<SourceSnapshot, String> {
    let url = legio_source::validate_manifest_url(url.trim())
        .map_err(|error| error.to_string())?
        .to_string();
    let database = app.state::<DatabaseState>().shared_database()?;
    let check_database = database.clone();
    let check_url = url.clone();
    let exists = tauri::async_runtime::spawn_blocking(move || {
        check_database.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM download_sources WHERE url = ?1)",
                    [check_url],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(db_error)
        })
    })
    .await
    .map_err(|error| format!("Source lookup task failed: {error}"))??;
    if exists {
        return cached_source(app).await;
    }
    let manifest = legio_source::fetch_manifest(network, &url).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let current_time = now()?;
        insert(&database, &url, manifest, current_time)?;
        snapshot(&database, current_time)
    })
    .await
    .map_err(|error| format!("Source installation task failed: {error}"))?
}

pub async fn remove_source(app: tauri::AppHandle, id: String) -> Result<SourceSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let database = app.state::<DatabaseState>();
        remove(database.database()?, &id)?;
        snapshot(database.database()?, now()?)
    })
    .await
    .map_err(|error| format!("Source removal task failed: {error}"))?
}

pub async fn refresh_source(
    app: tauri::AppHandle,
    network: &NetworkState,
) -> Result<SourceSnapshot, String> {
    let database = app.state::<DatabaseState>().shared_database()?;
    let read_database = database.clone();
    let sources = tauri::async_runtime::spawn_blocking(move || rows(&read_database))
        .await
        .map_err(|error| format!("Source lookup task failed: {error}"))??;
    let mut tasks = tokio::task::JoinSet::new();
    // Each source has a bounded request and an independent cache.
    for row in sources {
        let network = network.clone();
        let database = database.clone();
        tasks.spawn(async move {
            let fetched = legio_source::fetch_manifest(&network, &row.url).await;
            tauri::async_runtime::spawn_blocking(move || {
                update(&database, &row.id, fetched, now()?)
            })
            .await
            .map_err(|error| format!("Source refresh task failed: {error}"))?
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.map_err(|error| format!("Source refresh task failed: {error}"))??;
    }
    cached_source(app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid(app_id: u32) -> Manifest {
        legio_source::parse_manifest(format!(r#"{{"schemaVersion":1,"generatedAt":"2026-09-22T00:00:00Z","verified":[{{"steamAppId":{app_id},"name":"Portal","release":{{"version":"1","publishedAt":"2026-09-22T00:00:00Z"}},"download":{{"url":"https://example.invalid/a.zip","sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","sizeBytes":1}}}}],"unverified":[]}}"#).as_bytes()).unwrap()
    }

    fn database() -> (std::path::PathBuf, Database) {
        let directory =
            std::env::temp_dir().join(format!("legio-sources-{}", uuid::Uuid::new_v4()));
        let database = Database::open(&directory).unwrap();
        (directory, database)
    }

    #[test]
    fn sources_start_empty_and_persist_independently() {
        let (directory, database) = database();
        let empty = snapshot(&database, 100).unwrap();
        assert!(empty.sources.is_empty());
        assert!(empty.manifest.is_none());
        assert!(!empty.stale);
        insert(
            &database,
            "https://example.invalid/one.json",
            valid(400),
            100,
        )
        .unwrap();
        insert(
            &database,
            "https://example.invalid/one.json",
            valid(401),
            101,
        )
        .unwrap();
        insert(
            &database,
            "https://example.invalid/two.json",
            valid(402),
            100,
        )
        .unwrap();
        let first = cached(&database).unwrap().unwrap();
        assert!(Arc::ptr_eq(
            &first.manifest,
            &cached(&database).unwrap().unwrap().manifest
        ));
        let loaded = snapshot(&database, 101).unwrap();
        assert_eq!(loaded.sources.len(), 2);
        assert_eq!(loaded.manifest.unwrap().verified.len(), 2);
        remove(&database, &loaded.sources[0].id).unwrap();
        drop(database);
        let reopened = Database::open(&directory).unwrap();
        let remaining = snapshot(&reopened, 101).unwrap();
        assert_eq!(remaining.sources.len(), 1);
        assert_eq!(remaining.manifest.unwrap().verified[0].steam_app_id, 402);
        remove(&reopened, &remaining.sources[0].id).unwrap();
        assert!(cached(&reopened).unwrap().is_none());
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn offline_refresh_preserves_cache_and_late_refresh_cannot_restore_removed_source() {
        let (directory, database) = database();
        let url = "https://example.invalid/one.json";
        insert(&database, url, valid(400), 100).unwrap();
        let id = snapshot(&database, 101).unwrap().sources[0].id.clone();
        update(&database, &id, Err("Offline".to_owned()), 101).unwrap();
        let stale = snapshot(&database, 101).unwrap();
        assert!(stale.stale);
        assert_eq!(stale.sources[0].warning.as_deref(), Some("Offline"));
        assert_eq!(stale.manifest.unwrap().verified[0].steam_app_id, 400);
        remove(&database, &id).unwrap();
        insert(&database, url, valid(401), 102).unwrap();
        update(&database, &id, Ok(valid(402)), 103).unwrap();
        assert_eq!(
            snapshot(&database, 103).unwrap().manifest.unwrap().verified[0].steam_app_id,
            401
        );
        let new_id = snapshot(&database, 103).unwrap().sources[0].id.clone();
        update(&database, &new_id, Ok(valid(402)), 104).unwrap();
        assert!(snapshot(&database, 104).unwrap().warning.is_none());
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn duplicate_releases_are_deduplicated_and_conflicts_are_omitted() {
        let (directory, database) = database();
        insert(
            &database,
            "https://example.invalid/one.json",
            valid(400),
            100,
        )
        .unwrap();
        insert(
            &database,
            "https://example.invalid/two.json",
            valid(400),
            100,
        )
        .unwrap();
        assert_eq!(
            cached(&database).unwrap().unwrap().manifest.verified.len(),
            1
        );
        let mut conflict = valid(400);
        conflict.verified[0].download.size_bytes = 2;
        insert(
            &database,
            "https://example.invalid/three.json",
            conflict,
            100,
        )
        .unwrap();
        let snapshot = snapshot(&database, 100).unwrap();
        assert!(snapshot.warning.is_some());
        assert!(snapshot.manifest.unwrap().verified.is_empty());
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn upgrade_clears_legacy_cache_without_removing_installed_games() {
        let (directory, database) = database();
        let game = database
            .create_game(crate::database::CreateGameInput {
                steam_app_id: Some(400),
                name: "Portal".to_owned(),
            })
            .unwrap();
        database.with_connection(|connection| {
            connection.execute("INSERT INTO legio_source_cache (id, manifest, fetched_at) VALUES (1, ?1, 100)", [encode(&valid(400)).unwrap()]).map_err(db_error)?;
            connection.execute_batch("DROP TABLE download_sources; PRAGMA user_version = 24;").map_err(db_error)
        }).unwrap();
        drop(database);
        let reopened = Database::open(&directory).unwrap();
        assert!(snapshot(&reopened, 101).unwrap().sources.is_empty());
        let legacy_count: i64 = reopened
            .with_connection(|connection| {
                connection
                    .query_row("SELECT COUNT(*) FROM legio_source_cache", [], |row| {
                        row.get(0)
                    })
                    .map_err(db_error)
            })
            .unwrap();
        assert_eq!(legacy_count, 0);
        assert_eq!(reopened.game(&game.id).unwrap().name, "Portal");
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cache_age_and_clock_rollback_mark_stale() {
        assert!(!is_stale(100, 101));
        assert!(is_stale(100, 100 + STALE_SECONDS));
        assert!(is_stale(100, 99));
    }
}
