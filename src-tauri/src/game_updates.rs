use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::database::{Database, DatabaseState, database_error};
use crate::legio_source::{Manifest, SourceEntry};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone)]
pub(crate) struct Origin {
    pub id: String,
    pub manifest: Arc<Manifest>,
    pub fresh: bool,
}

pub(crate) fn sources(database: &Database) -> Result<Vec<Origin>, String> {
    let now = crate::database::now_milliseconds()? / 1000;
    Ok(crate::legio_source_cache::cached_rows(database)?
        .iter()
        .map(|row| Origin {
            id: row.id.clone(),
            manifest: row.manifest.clone(),
            fresh: row.warning.is_none() && row.fetched_at >= now - 86400,
        })
        .collect())
}

pub(crate) fn find_download(
    database: &Database,
    app_id: u32,
    url: &str,
    version: &str,
    source_id: Option<&str>,
) -> Result<(String, SourceEntry, bool), String> {
    let mut found = None;
    for source in sources(database)? {
        if source_id.is_some_and(|id| id != source.id) {
            continue;
        }
        for (verified, entries) in [
            (true, &source.manifest.verified),
            (false, &source.manifest.unverified),
        ] {
            for entry in entries {
                if entry.steam_app_id == app_id
                    && entry.download.url == url
                    && entry.release.version == version
                {
                    if found.is_some() {
                        return Err(
                            "Select the download source explicitly for this release".to_owned()
                        );
                    }
                    found = Some((source.id.clone(), entry.clone(), verified));
                }
            }
        }
    }
    found.ok_or_else(|| "Download is unavailable from the selected source".to_owned())
}

pub(crate) fn record_install_on(
    connection: &rusqlite::Connection,
    job_id: &str,
    game_id: &str,
) -> Result<(), String> {
    connection.execute("INSERT INTO game_source_installs (game_id, source_id, source_sha256, entry_id, installed_version, accepted_unverified)
            SELECT ?2, source_id, (SELECT url_sha256 FROM download_sources WHERE id=source_id), source_entry_id, release_version, NOT source_verified FROM downloads WHERE id = ?1 AND source_id IS NOT NULL
            ON CONFLICT(game_id) DO UPDATE SET source_id = excluded.source_id, source_sha256 = excluded.source_sha256, entry_id = excluded.entry_id, installed_version = excluded.installed_version, accepted_unverified = excluded.accepted_unverified", params![job_id, game_id]).map_err(database_error)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GameUpdate {
    pub game_id: String,
    pub source_id: String,
    pub entry_id: String,
    pub steam_app_id: u32,
    pub name: String,
    pub installed_version: String,
    pub version: String,
    pub size_bytes: u64,
    pub verified: bool,
    pub accepted_unverified: bool,
    pub fresh: bool,
}

pub(crate) fn list(database: &Database) -> Result<Vec<GameUpdate>, String> {
    let origins = sources(database)?;
    let installs: Vec<(String, String, String, String, u32, bool)> = database.with_connection(|connection| {
        let mut query = connection.prepare("SELECT i.game_id, i.source_id, i.entry_id, i.installed_version, g.steam_app_id, i.accepted_unverified
            FROM game_source_installs i JOIN games g ON g.id = i.game_id WHERE i.entry_id IS NOT NULL AND g.steam_app_id IS NOT NULL AND g.steam_install_path IS NULL").map_err(database_error)?;
        query.query_map([], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?))).map_err(database_error)?.collect::<Result<_,_>>().map_err(database_error)
    })?;
    let mut releases = HashMap::new();
    for source in &origins {
        if !source.manifest.support_game_updates {
            continue;
        }
        for (verified, entries) in [
            (true, &source.manifest.verified),
            (false, &source.manifest.unverified),
        ] {
            for entry in entries {
                if let Some(id) = entry.id.as_deref() {
                    releases.insert(
                        (source.id.as_str(), id, entry.steam_app_id),
                        (source, entry, verified),
                    );
                }
            }
        }
    }
    let mut updates = Vec::new();
    for (game_id, source_id, entry_id, installed_version, app_id, accepted_unverified) in installs {
        if let Some((source, entry, verified)) = releases
            .get(&(source_id.as_str(), entry_id.as_str(), app_id))
            .copied()
            && entry.release.version != installed_version
        {
            updates.push(GameUpdate {
                game_id,
                source_id,
                entry_id,
                steam_app_id: app_id,
                name: entry.name.clone(),
                installed_version,
                version: entry.release.version.clone(),
                size_bytes: entry.download.size_bytes,
                verified,
                accepted_unverified,
                fresh: source.fresh,
            });
        }
    }
    Ok(updates)
}

pub(crate) fn target(database: &Database, job_id: &str) -> Result<Option<String>, String> {
    database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT target_game_id FROM downloads WHERE id = ?1",
                [job_id],
                |row| row.get(0),
            )
            .optional()
            .map(|row| row.flatten())
            .map_err(database_error)
    })
}

#[tauri::command]
pub(crate) async fn list_game_updates(app: AppHandle) -> Result<Vec<GameUpdate>, String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || list(app.state::<DatabaseState>().database()?))
        .await
        .map_err(|error| error.to_string())?
}

pub(crate) fn enqueue(
    database: &Database,
    game_id: &str,
    accept_unverified: bool,
    automatic: bool,
) -> Result<String, String> {
    let update = list(database)?
        .into_iter()
        .find(|entry| entry.game_id == game_id)
        .ok_or("No update is available for this game")?;
    enqueue_update(
        database,
        &update,
        &sources(database)?,
        accept_unverified,
        automatic,
    )
}

fn enqueue_update(
    database: &Database,
    update: &GameUpdate,
    origins: &[Origin],
    accept_unverified: bool,
    automatic: bool,
) -> Result<String, String> {
    let game_id = update.game_id.as_str();
    if automatic && (!update.fresh || (!update.verified && !update.accepted_unverified)) {
        return Err("This update requires confirmation or a fresh source".to_owned());
    }
    if !update.verified && !accept_unverified {
        return Err("Confirm the unverified update before downloading".to_owned());
    }
    let existing = database.with_connection(|connection| connection.query_row("SELECT id FROM downloads WHERE target_game_id = ?1 AND status NOT IN ('installed','cancelled') LIMIT 1", [game_id], |row| row.get::<_, String>(0)).optional().map_err(database_error))?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let origin = origins
        .iter()
        .find(|source| source.id == update.source_id)
        .ok_or("Source is no longer installed")?;
    let entry = origin
        .manifest
        .verified
        .iter()
        .chain(&origin.manifest.unverified)
        .find(|entry| entry.id.as_deref() == Some(&update.entry_id))
        .ok_or("Release is no longer available")?;
    let job = crate::download_queue::enqueue_with_target(
        database,
        update.steam_app_id,
        &entry.download.url,
        &entry.release.version,
        accept_unverified,
        Some(&update.source_id),
        Some((game_id, automatic)),
    )?;
    let id = job.id().to_owned();
    Ok(id)
}

#[tauri::command]
pub(crate) async fn queue_game_update(
    app: AppHandle,
    game_id: String,
    accept_unverified: bool,
) -> Result<String, String> {
    crate::startup_recovery::wait(&app).await?;
    let worker = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<crate::game_lifecycle::GameLaunchManager>();
        let _operation = manager.operation()?;
        manager.require_idle(&game_id)?;
        let state = app.state::<DatabaseState>();
        let database = state.database()?;
        let id = enqueue(database, &game_id, accept_unverified, false)?;
        crate::download_policy::manual_start(database, &manager, &id)?;
        Ok::<_, String>(id)
    })
    .await
    .map_err(|error| error.to_string())??;
    crate::download_queue::kick(worker);
    Ok(result)
}

pub(crate) fn queue_automatic(database: &Database) -> Result<(), String> {
    if !database.settings()?.automatic_game_updates {
        return Ok(());
    }
    backfill(database)?;
    let origins = sources(database)?;
    for update in list(database)? {
        if update.fresh && (update.verified || update.accepted_unverified) {
            enqueue_update(
                database,
                &update,
                &origins,
                update.accepted_unverified,
                true,
            )?;
        }
    }
    Ok(())
}

struct LegacyInstall {
    game: String,
    app_id: u32,
    name: String,
    url: String,
    version: String,
    known_source: Option<String>,
}

pub(crate) fn backfill(database: &Database) -> Result<(), String> {
    let origins = sources(database)?;
    let old: Vec<LegacyInstall> = database.with_connection(|connection| {
        let mut query = connection.prepare("SELECT g.id, d.steam_app_id, d.name, d.url, d.release_version, i.source_id FROM games g JOIN downloads d ON d.id = g.id LEFT JOIN game_source_installs i ON i.game_id = g.id WHERE d.status = 'installed' AND i.entry_id IS NULL").map_err(database_error)?;
        query.query_map([], |row| Ok(LegacyInstall { game:row.get(0)?, app_id:row.get(1)?, name:row.get(2)?, url:row.get(3)?, version:row.get(4)?, known_source:row.get(5)? })).map_err(database_error)?.collect::<Result<_,_>>().map_err(database_error)
    })?;
    for LegacyInstall {
        game,
        app_id,
        name,
        url,
        version,
        known_source,
    } in old
    {
        let mut matches = Vec::new();
        for origin in &origins {
            if known_source.as_ref().is_some_and(|id| id != &origin.id) {
                continue;
            }
            for entry in origin
                .manifest
                .verified
                .iter()
                .chain(&origin.manifest.unverified)
            {
                if entry.steam_app_id == app_id
                    && (if known_source.is_some() {
                        entry.name == name
                    } else {
                        entry.download.url == url && entry.release.version == version
                    })
                {
                    matches.push((origin, entry));
                }
            }
        }
        if matches.len() != 1 {
            continue;
        }
        let (origin, entry) = matches[0];
        database.with_connection(|connection| {
            let transaction = connection.unchecked_transaction().map_err(database_error)?;
            transaction
                .execute(
                    "UPDATE downloads SET source_id = ?2, source_entry_id = ?3 WHERE id = ?1",
                    params![game, origin.id, entry.id],
                )
                .map_err(database_error)?;
            record_install_on(&transaction, &game, &game)?;
            transaction.commit().map_err(database_error)
        })?;
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GameSourceBinding {
    pub source_id: Option<String>,
    pub entry_id: Option<String>,
    pub installed_version: String,
}

#[tauri::command]
pub(crate) async fn get_game_source_binding(
    app: AppHandle,
    game_id: String,
) -> Result<GameSourceBinding, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DatabaseState>().database()?.with_connection(|connection| {
            let binding = connection.query_row("SELECT source_id, entry_id, installed_version FROM game_source_installs WHERE game_id = ?1", [&game_id], |row| Ok(GameSourceBinding { source_id: row.get(0)?, entry_id: row.get(1)?, installed_version: row.get(2)? })).optional().map_err(database_error)?;
            if let Some(binding) = binding { return Ok(binding); }
            let version = connection.query_row("SELECT release_version FROM downloads WHERE id = ?1 AND status = 'installed'", [&game_id], |row| row.get(0)).optional().map_err(database_error)?.unwrap_or_default();
            Ok(GameSourceBinding { source_id: None, entry_id: None, installed_version: version })
        })
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn bind_game_source(
    app: AppHandle,
    game_id: String,
    source_id: String,
    entry_id: String,
    installed_version: String,
    accept_unverified: bool,
) -> Result<(), String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<crate::game_lifecycle::GameLaunchManager>(); let _operation = manager.operation()?;
        manager.require_idle(&game_id)?;
        let state = app.state::<DatabaseState>(); let database = state.database()?;
        let game = database.game(&game_id)?;
        if game.steam_install_path.is_some() || game.installation_root.is_none() { return Err("Only installed local games can be linked to an update source".into()); }
        let version = installed_version.trim();
        if version.is_empty() || version.len() > 128 { return Err("Enter a valid installed version".into()); }
        let origin = sources(database)?.into_iter().find(|origin| origin.id == source_id && origin.manifest.support_game_updates).ok_or("Source does not support updates")?;
        let verified = origin.manifest.verified.iter().any(|entry| entry.id.as_deref() == Some(&entry_id));
        let entry = origin.manifest.verified.iter().chain(&origin.manifest.unverified).find(|entry| entry.id.as_deref() == Some(&entry_id)).ok_or("Variant is not available")?;
        if game.steam_app_id != Some(entry.steam_app_id) { return Err("Variant does not match this game's Steam identity".into()); }
        if !verified && !accept_unverified { return Err("Confirm the unverified source before linking".into()); }
        database.with_connection(|connection| {
            let active: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM downloads WHERE target_game_id = ?1 AND status NOT IN ('installed','cancelled'))", [&game_id], |row| row.get(0)).map_err(database_error)?;
            if active { return Err("Cancel the pending update before changing its source".into()); }
            connection.execute("INSERT INTO game_source_installs (game_id,source_id,source_sha256,entry_id,installed_version,accepted_unverified) VALUES (?1,?2,(SELECT url_sha256 FROM download_sources WHERE id=?2),?3,?4,?5) ON CONFLICT(game_id) DO UPDATE SET source_id=excluded.source_id,source_sha256=excluded.source_sha256,entry_id=excluded.entry_id,installed_version=excluded.installed_version,accepted_unverified=excluded.accepted_unverified", params![game_id,source_id,entry_id,version,!verified]).map_err(database_error)?;
            Ok(())
        })?;
        crate::game_library::sync_default(&app)
    }).await.map_err(|error| error.to_string())?
}

pub(crate) fn schedule(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let worker = app.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            queue_automatic(worker.state::<DatabaseState>().database()?)
        })
        .await;
        match result {
            Ok(Ok(())) => crate::download_queue::kick(app),
            other => crate::application_log::failure("schedule_game_updates", format!("{other:?}")),
        }
    });
}

pub(crate) fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = crate::startup_recovery::wait(&app).await {
            crate::application_log::failure("game_update_start", error);
            return;
        }
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15 * 60));
        loop {
            interval.tick().await;
            let reader = app.clone();
            let migrated = tauri::async_runtime::spawn_blocking(move || {
                backfill(reader.state::<DatabaseState>().database()?)
            })
            .await;
            if !matches!(migrated, Ok(Ok(()))) {
                crate::application_log::failure(
                    "recover_source_provenance",
                    format!("{migrated:?}"),
                );
            }
            if let Err(error) = crate::legio_source_cache::refresh_source(
                app.clone(),
                &app.state::<crate::network::NetworkState>(),
            )
            .await
            {
                crate::application_log::failure("refresh_game_updates", error);
            }
            if let Err(error) = app.emit("game-updates-changed", ()) {
                crate::application_log::failure("publish_game_updates", error);
            }
            let worker = app.clone();
            let result = tauri::async_runtime::spawn_blocking(move || {
                queue_automatic(worker.state::<DatabaseState>().database()?)
            })
            .await;
            match result {
                Ok(Ok(())) => crate::download_queue::kick(app.clone()),
                other => {
                    crate::application_log::failure("schedule_game_updates", format!("{other:?}"))
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_updates_keep_provenance_without_download_history() {
        let root =
            std::env::temp_dir().join(format!("legio-update-origin-{}", uuid::Uuid::new_v4()));
        let database = Database::open(&root).unwrap();
        let game = database
            .create_game(crate::database::CreateGameInput {
                name: "Custom name".into(),
                steam_app_id: Some(400),
            })
            .unwrap();
        let manifest = serde_json::json!({"schemaVersion":2,"supportGameUpdates":true,"generatedAt":"2026-10-14T00:00:00Z","verified":[{"id":"portal-onlinefix","steamAppId":400,"name":"Renamed source variant","release":{"version":"2","publishedAt":"2026-10-14T00:00:00Z"},"download":{"url":"https://example.test/game.zip","sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","sizeBytes":100}}],"unverified":[]});
        database.with_connection(|c| {
            c.execute("INSERT INTO download_sources (id,url,manifest,fetched_at) VALUES ('source','https://example.test/source.json',?1,strftime('%s','now'))", [serde_json::to_vec(&manifest).unwrap()]).map_err(database_error)?;
            c.execute("INSERT INTO game_source_installs (game_id,source_id,entry_id,installed_version) VALUES (?1,'source','portal-onlinefix','1')", [&game.id]).map_err(database_error)?;
            Ok(())
        }).unwrap();
        let updates = list(&database).unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].installed_version, "1");
        assert_eq!(updates[0].version, "2");
        assert_eq!(updates[0].game_id, game.id);
        queue_automatic(&database).unwrap();
        let count = || {
            database
                .with_connection(|c| {
                    c.query_row("SELECT COUNT(*) FROM downloads", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map_err(database_error)
                })
                .unwrap()
        };
        assert_eq!(count(), 0);
        let mut settings = database.settings().unwrap();
        settings.automatic_game_updates = true;
        database.save_settings(settings).unwrap();
        queue_automatic(&database).unwrap();
        queue_automatic(&database).unwrap();
        assert_eq!(count(), 1);
        let mut other = manifest.clone();
        other["verified"][0]["id"] = serde_json::json!("other-variant");
        database
            .with_connection(|c| {
                c.execute(
                    "UPDATE download_sources SET manifest=?1",
                    [serde_json::to_vec(&other).unwrap()],
                )
                .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        assert!(list(&database).unwrap().is_empty());
        other["verified"][0]["id"] = serde_json::json!("portal-onlinefix");
        other["supportGameUpdates"] = serde_json::json!(false);
        database
            .with_connection(|c| {
                c.execute(
                    "UPDATE download_sources SET manifest=?1",
                    [serde_json::to_vec(&other).unwrap()],
                )
                .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        assert!(list(&database).unwrap().is_empty());
        drop(database);
        std::fs::remove_dir_all(root).unwrap();
    }
}
