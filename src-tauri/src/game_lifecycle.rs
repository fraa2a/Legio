use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::database::DatabaseState;
use crate::{game_process, steam_switch};

const START_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const EXIT_GRACE: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GameStatus {
    Idle,
    Launching,
    Running,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GameLaunchState {
    pub game_id: String,
    pub status: GameStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

struct Entry {
    app_id: u32,
    install_path: PathBuf,
    status: GameStatus,
    error: Option<String>,
    cancel: Arc<AtomicBool>,
}

#[derive(Default, Clone)]
pub(crate) struct GameLaunchManager {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
}

impl GameLaunchManager {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn list(&self) -> Result<Vec<GameLaunchState>, String> {
        let entries = self.lock()?;
        Ok(entries
            .iter()
            .map(|(game_id, entry)| GameLaunchState {
                game_id: game_id.clone(),
                status: entry.status,
                error: entry.error.clone(),
            })
            .collect())
    }

    pub(crate) fn launch(
        &self,
        app: AppHandle,
        game_id: String,
        confirm_account_switch: bool,
    ) -> Result<steam_switch::SteamLaunchResult, String> {
        let game = app.state::<DatabaseState>().database()?.game(&game_id)?;
        let app_id = game
            .steam_app_id
            .filter(|id| *id > 0)
            .ok_or_else(|| "This game has no valid Steam App ID".to_owned())?;
        let install_path = game
            .steam_install_path
            .as_deref()
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .ok_or_else(|| "This game has no available Steam installation".to_owned())?;
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut entries = self.lock()?;
            if entries.get(&game_id).is_some_and(|entry| {
                matches!(entry.status, GameStatus::Launching | GameStatus::Running)
            }) {
                return Err("This game is already launching or running".to_owned());
            }
            if entries.values().any(|entry| {
                entry.app_id == app_id
                    && matches!(entry.status, GameStatus::Launching | GameStatus::Running)
            }) {
                return Err("This Steam App ID is already launching or running".to_owned());
            }
            entries.insert(
                game_id.clone(),
                Entry {
                    app_id,
                    install_path: install_path.clone(),
                    status: GameStatus::Launching,
                    error: None,
                    cancel: Arc::clone(&cancel),
                },
            );
        }

        let worker_id = game_id.clone();
        let manager = self.clone();
        if let Err(error) = thread::Builder::new()
            .name(format!("legio-game-{app_id}"))
            .spawn(move || {
                manager.run_launch(
                    app,
                    worker_id,
                    app_id,
                    install_path,
                    confirm_account_switch,
                    cancel,
                );
            })
        {
            self.set_state(&game_id, GameStatus::Idle, Some(error.to_string()));
            return Err(format!("Could not start game launch task: {error}"));
        }
        Ok(steam_switch::SteamLaunchResult {
            game_id,
            steam_app_id: app_id,
        })
    }

    pub(crate) fn cancel(&self, game_id: &str) -> Result<(), String> {
        let entries = self.lock()?;
        let entry = entries
            .get(game_id)
            .ok_or_else(|| "This game is not launching".to_owned())?;
        if entry.status != GameStatus::Launching {
            return Err("This game is not launching".to_owned());
        }
        entry.cancel.store(true, Ordering::Release);
        Ok(())
    }

    pub(crate) fn stop(&self, game_id: &str) -> Result<(), String> {
        let (app_id, install_path) = {
            let entries = self.lock()?;
            let entry = entries
                .get(game_id)
                .filter(|entry| entry.status == GameStatus::Running)
                .ok_or_else(|| "This game is not running".to_owned())?;
            (entry.app_id, entry.install_path.clone())
        };
        game_process::stop(app_id, &install_path)
    }

    fn run_launch(
        &self,
        app: AppHandle,
        game_id: String,
        app_id: u32,
        install_path: PathBuf,
        confirm_account_switch: bool,
        cancel: Arc<AtomicBool>,
    ) {
        if let Err(error) = steam_switch::launch(&app, &game_id, confirm_account_switch, &cancel) {
            let error =
                (!cancel.load(Ordering::Acquire) || error != "Launch cancelled").then_some(error);
            self.set_state(&game_id, GameStatus::Idle, error);
            return;
        }
        let started = Instant::now();
        loop {
            match game_process::matching_pids(app_id, &install_path) {
                Ok(pids) if !pids.is_empty() => {
                    if cancel.load(Ordering::Acquire) {
                        let result = game_process::stop(app_id, &install_path);
                        self.set_state(&game_id, GameStatus::Idle, result.err());
                        return;
                    }
                    break;
                }
                Ok(_) => {}
                Err(error) => {
                    self.set_state(&game_id, GameStatus::Idle, Some(error));
                    return;
                }
            }
            if started.elapsed() >= START_TIMEOUT {
                let error = (!cancel.load(Ordering::Acquire)).then_some(
                    "Steam did not start a detectable game process within two minutes".to_owned(),
                );
                self.set_state(&game_id, GameStatus::Idle, error);
                return;
            }
            thread::sleep(POLL_INTERVAL);
        }
        self.set_state(&game_id, GameStatus::Running, None);
        let mut missing_since = None;
        loop {
            match game_process::matching_pids(app_id, &install_path) {
                Ok(pids) if pids.is_empty() => {
                    let since = missing_since.get_or_insert_with(Instant::now);
                    if since.elapsed() >= EXIT_GRACE {
                        self.set_state(&game_id, GameStatus::Idle, None);
                        return;
                    }
                }
                Ok(_) => missing_since = None,
                Err(error) => {
                    self.set_state(&game_id, GameStatus::Idle, Some(error));
                    return;
                }
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn set_state(&self, game_id: &str, status: GameStatus, error: Option<String>) {
        if let Ok(mut entries) = self.entries.lock()
            && let Some(entry) = entries.get_mut(game_id)
        {
            entry.status = status;
            entry.error = error;
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, Entry>>, String> {
        self.entries
            .lock()
            .map_err(|_| "Game launch state is unavailable after an earlier task failed".to_owned())
    }
}
