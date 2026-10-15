use crate::{
    database::{Database, DatabaseState, database_error},
    download_queue::DownloadQueueState,
    game_lifecycle::{GameLaunchManager, GameStatus},
};
use rusqlite::params;
use std::{collections::HashSet, time::Duration};
use tauri::{AppHandle, Manager, Runtime};

pub(crate) fn busy(
    database: &Database,
    manager: &GameLaunchManager,
    game_id: &str,
) -> Result<bool, String> {
    let game = database.game(game_id)?;
    let root = game.installation_root.as_deref().map(std::path::Path::new);
    for state in manager.list()? {
        if state.status == GameStatus::Idle {
            continue;
        }
        if state.game_id == game_id {
            return Ok(true);
        }
        if let Some(root) = root
            && let Ok(other) = database.game(&state.game_id)
            && (other
                .installation_root
                .as_deref()
                .is_some_and(|path| std::path::Path::new(path) == root)
                || other
                    .executable_path
                    .as_deref()
                    .is_some_and(|path| std::path::Path::new(path).starts_with(root)))
        {
            return Ok(true);
        }
    }
    if let Some(root) = root {
        return external_busy(root);
    }
    Ok(false)
}

pub(crate) fn external_busy(root: &std::path::Path) -> Result<bool, String> {
    #[cfg(target_os = "linux")]
    {
        let root = std::fs::canonicalize(root).map_err(|error| error.to_string())?;
        for entry in std::fs::read_dir("/proc").map_err(|error| error.to_string())? {
            let Ok(entry) = entry else {
                continue;
            };
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|value| value.parse::<u32>().ok())
            else {
                continue;
            };
            if pid == std::process::id() {
                continue;
            }
            if ["exe", "cwd"].iter().any(|name| {
                std::fs::read_link(entry.path().join(name))
                    .is_ok_and(|path| path.starts_with(&root))
            }) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    #[cfg(windows)]
    {
        let target = crate::game_process::ProcessTarget::Native {
            game_directory: root.to_path_buf(),
            started_after_ms: 0,
            launcher_pid: None,
            known_pids: std::sync::Arc::default(),
        };
        Ok(!crate::game_process::matching_pids(&target)?.is_empty())
    }
}

struct PolicyJob {
    id: String,
    target: Option<String>,
    status: String,
    previous: Option<String>,
    manual: bool,
    automatic: bool,
    interrupted: bool,
}

pub(crate) fn sync(
    database: &Database,
    manager: &GameLaunchManager,
    queue: &DownloadQueueState,
) -> Result<bool, String> {
    let _operation = manager.operation()?;
    let mut playing = manager
        .list()?
        .iter()
        .any(|state| state.status != GameStatus::Idle);
    let jobs: Vec<PolicyJob> = database.with_connection(|connection| {
        let mut query = connection.prepare("SELECT id,target_game_id,status,pause_reason,manual_override,automatic_update,interrupted_by_game FROM downloads WHERE status IN ('queued','downloading','waiting','paused') ORDER BY queue_position,created_at,id").map_err(database_error)?;
        query.query_map([], |row| Ok(PolicyJob { id:row.get(0)?, target:row.get(1)?, status:row.get(2)?, previous:row.get(3)?, manual:row.get(4)?, automatic:row.get(5)?, interrupted:row.get(6)? })).map_err(database_error)?.collect::<Result<_,_>>().map_err(database_error)
    })?;
    if jobs.is_empty() {
        return Ok(false);
    }
    let settings = database.settings()?;
    let mut interruptions = Vec::new();
    let mut changed = false;
    let mut restored_priority = false;
    let mut blocked = HashSet::new();
    let targets: HashSet<_> = jobs
        .iter()
        .filter_map(|job| job.target.as_deref())
        .collect();
    for game in targets {
        if busy(database, manager, game)? {
            blocked.insert(game.to_owned());
        }
    }
    playing |= !blocked.is_empty();
    database.with_connection(|connection| {
        let transaction = connection.unchecked_transaction().map_err(database_error)?;
        for PolicyJob { id, target, status, previous, manual, automatic, interrupted } in &jobs {
            let reason = if target.as_ref().is_some_and(|game| blocked.contains(game)) { Some("game_running") }
                else if *automatic && !settings.automatic_game_updates { Some("automatic_disabled") }
                else if playing && !settings.download_while_playing && !manual { Some("playing") } else { None };
            if status == "paused" && previous.as_deref() == Some("game_running") && !interrupted && !target.as_ref().is_some_and(|game| blocked.contains(game)) {
                transaction.execute("UPDATE downloads SET pause_reason = NULL WHERE id = ?1", [id]).map_err(database_error)?;
                changed = true;
                continue;
            }
            if let Some(reason) = reason {
                if status == "paused" && previous.is_none() && reason != "game_running" { continue; }
                if status == "paused" && previous.as_deref() == Some(reason) { continue; }
                changed = true;
                transaction.execute("UPDATE downloads SET status = 'paused', pause_reason = ?2, speed_bps = 0, eta_seconds = NULL, interrupted_by_game = interrupted_by_game OR ?3 WHERE id = ?1", params![id,reason,reason == "game_running" && (status != "paused" || previous.is_some())]).map_err(database_error)?;
                if status == "downloading" { interruptions.push(id.clone()); }
            } else if previous.is_some() {
                transaction.execute("UPDATE downloads SET status = 'queued', pause_reason = NULL, error = NULL WHERE id = ?1", [id]).map_err(database_error)?;
                if *interrupted {
                    transaction.execute("UPDATE downloads SET queue_position = (SELECT COALESCE(MIN(queue_position),0) - 1 FROM downloads), interrupted_by_game = 0 WHERE id = ?1", [id]).map_err(database_error)?;
                    restored_priority = true;
                }
                changed = true;
            }
        }
        if restored_priority {
            for PolicyJob { id, status, .. } in &jobs {
                if status == "downloading" && !interruptions.contains(id) {
                    transaction.execute("UPDATE downloads SET status = 'queued', speed_bps = 0, eta_seconds = NULL WHERE id = ?1", [id]).map_err(database_error)?;
                    interruptions.push(id.clone());
                }
            }
        }
        transaction.commit().map_err(database_error)
    })?;
    for id in interruptions {
        queue.interrupt(&id)?;
    }
    Ok(changed)
}

pub(crate) fn protect_launch<R: Runtime>(app: &AppHandle<R>, game_id: &str) -> Result<(), String> {
    let database = app.state::<DatabaseState>();
    let database = database.database()?;
    let manager = app.state::<GameLaunchManager>();
    manager.require_idle(game_id)?;
    crate::game_transfer::ensure_launchable(database, game_id)?;
    crate::overlay_update::ensure_launchable(database, game_id)?;
    let game = database.game(game_id)?;
    if let Some(root) = game.installation_root.as_deref().map(std::path::Path::new) {
        let root = dunce::canonicalize(root).map_err(|error| error.to_string())?;
        for other in database.games()? {
            if let Some(other_root) = other.installation_root.as_deref()
                && let Ok(other_root) = dunce::canonicalize(other_root)
                && (root.starts_with(&other_root) || other_root.starts_with(&root))
            {
                manager.require_idle(&other.id)?;
                crate::game_transfer::ensure_launchable(database, &other.id)?;
                crate::overlay_update::ensure_launchable(database, &other.id)?;
            }
        }
    }
    let ids: Vec<String> = database.with_connection(|connection| {
        let mut query = connection.prepare("SELECT id FROM downloads WHERE target_game_id = ?1 AND status IN ('queued','downloading','waiting','paused')").map_err(database_error)?;
        let ids = query.query_map([game_id], |row| row.get(0)).map_err(database_error)?.collect::<Result<_,_>>().map_err(database_error)?;
        connection.execute("UPDATE downloads SET status = 'paused', pause_reason = 'game_running', interrupted_by_game = interrupted_by_game OR (status != 'paused' OR pause_reason IS NOT NULL), speed_bps = 0, eta_seconds = NULL WHERE target_game_id = ?1 AND status IN ('queued','downloading','waiting','paused')", [game_id]).map_err(database_error)?;
        Ok(ids)
    })?;
    for id in ids {
        app.state::<DownloadQueueState>().interrupt(&id)?;
    }
    Ok(())
}

pub(crate) fn manual_start(
    database: &Database,
    manager: &GameLaunchManager,
    id: &str,
) -> Result<(), String> {
    if !database.settings()?.download_while_playing
        && manager
            .list()?
            .iter()
            .any(|state| state.status != GameStatus::Idle)
    {
        database.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE downloads SET manual_override = 1 WHERE id = ?1",
                    [id],
                )
                .map_err(database_error)?;
            Ok(())
        })?;
    }
    Ok(())
}

pub(crate) fn require_resumable(
    database: &Database,
    manager: &GameLaunchManager,
    id: &str,
) -> Result<(), String> {
    if let Some(game) = crate::game_updates::target(database, id)?
        && busy(database, manager, &game)?
    {
        return Err("This game's update is locked while the game is open".into());
    }
    Ok(())
}

pub(crate) fn processable(
    database: &Database,
    manager: &GameLaunchManager,
    id: &str,
) -> Result<bool, String> {
    if let Some(game) = crate::game_updates::target(database, id)?
        && busy(database, manager, &game)?
    {
        return Ok(false);
    }
    Ok(!database.settings()?.defer_extraction_while_playing
        || manager
            .list()?
            .iter()
            .all(|state| state.status == GameStatus::Idle))
}

pub(crate) fn start<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = crate::startup_recovery::wait(&app).await {
            crate::application_log::failure("start_download_policy", error);
            return;
        }
        let Some(manager) = app.try_state::<GameLaunchManager>() else {
            return;
        };
        let mut changes = manager.subscribe_changes();
        let mut jobs = app.state::<DownloadQueueState>().policy_changes.subscribe();
        let mut polling = true;
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! { _ = tick.tick(), if polling => {}, result = changes.changed() => { if result.is_err() { return; } }, result = jobs.changed() => { if result.is_err() { return; } } }
            let worker = app.clone();
            let result = tauri::async_runtime::spawn_blocking(move || {
                let database = worker.state::<DatabaseState>();
                let database = database.database()?;
                let changed = sync(database, &worker.state::<GameLaunchManager>(), &worker.state::<DownloadQueueState>())?;
                let pending = database.with_connection(|c| c.query_row("SELECT EXISTS(SELECT 1 FROM downloads WHERE status IN ('queued','downloading','waiting','paused','downloaded','staging','staged','finalizing'))",[],|row| row.get::<_,bool>(0)).map_err(database_error))?;
                Ok::<_,String>((changed,pending))
            })
            .await;
            match result {
                Ok(Ok((changed, pending))) => {
                    polling = pending;
                    if changed {
                        crate::download_queue::notify(&app);
                        crate::download_queue::kick(app.clone());
                    }
                    if pending {
                        crate::download_queue::schedule_installations(&app);
                    }
                }
                other => {
                    crate::application_log::failure("download_game_policy", format!("{other:?}"))
                }
            }
        }
    });
}
