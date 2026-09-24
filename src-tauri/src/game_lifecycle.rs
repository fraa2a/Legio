use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::database::DatabaseState;
use crate::{game_process, steam_local, steam_switch};

const START_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const EXIT_GRACE: Duration = Duration::from_secs(3);
const LOG_READ_LIMIT: u64 = 128 * 1024;
const LOG_LINE_LIMIT: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
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

struct LaunchTarget {
    app_id: u32,
    install_path: PathBuf,
    steam_root: PathBuf,
}

impl LaunchTarget {
    fn prepare(app_id: u32, install_path: PathBuf, steam_root: PathBuf) -> Result<Self, String> {
        if app_id == 0 {
            return Err("This game has no valid Steam App ID".to_owned());
        }
        if !install_path.is_dir() {
            return Err("This game has no available Steam installation".to_owned());
        }
        Ok(Self {
            app_id,
            install_path,
            steam_root,
        })
    }
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
            .ok_or_else(|| "This game has no valid Steam App ID".to_owned())?;
        let install_path = game
            .steam_install_path
            .as_deref()
            .map(PathBuf::from)
            .ok_or_else(|| "This game has no available Steam installation".to_owned())?;
        let steam_root = steam_local::find_steam_root_for_game(&game)?;
        let target = LaunchTarget::prepare(app_id, install_path, steam_root)?;
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
                    install_path: target.install_path.clone(),
                    status: GameStatus::Launching,
                    error: None,
                    cancel: Arc::clone(&cancel),
                },
            );
        }

        let worker_id = game_id.clone();
        let manager = self.clone();
        let launch_game_id = worker_id.clone();
        if let Err(error) = thread::Builder::new()
            .name(format!("legio-game-{app_id}"))
            .spawn(move || {
                manager.run_launch(worker_id, target, cancel, move |cancel| {
                    steam_switch::launch(&app, &launch_game_id, confirm_account_switch, cancel)
                        .map(|_| ())
                });
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
        game_id: String,
        target: LaunchTarget,
        cancel: Arc<AtomicBool>,
        launch: impl FnOnce(&AtomicBool) -> Result<(), String>,
    ) {
        let LaunchTarget {
            app_id,
            install_path,
            steam_root,
        } = target;
        let mut steam_log = SteamLaunchLog::new(steam_root.join("logs/console_log.txt"));
        if let Err(error) = launch(&cancel) {
            let error = (!cancel.load(Ordering::Acquire) || error != "Launch cancelled")
                .then(|| format!("Launch stage failed: {error}"));
            self.set_state(&game_id, GameStatus::Idle, error);
            return;
        }
        let started = Instant::now();
        let mut steam_progress = SteamLaunchProgress::default();
        let mut cancelled_without_process_since = None;
        loop {
            match game_process::matching_pids(app_id, &install_path) {
                Ok(pids) if !pids.is_empty() => {
                    if cancel.load(Ordering::Acquire) {
                        let error = game_process::stop(app_id, &install_path)
                            .err()
                            .map(|error| format!("Terminate stage failed: {error}"));
                        self.set_state(&game_id, GameStatus::Idle, error);
                        return;
                    }
                    break;
                }
                Ok(_) => {}
                Err(error) => {
                    self.set_state(
                        &game_id,
                        GameStatus::Idle,
                        Some(format!("Monitor stage failed: {error}")),
                    );
                    return;
                }
            }
            while let Some(event) = steam_log.launch_event(app_id) {
                if let Some(error) = steam_progress.observe(event) {
                    let error = (!cancel.load(Ordering::Acquire))
                        .then(|| format!("Launch stage failed: {error}"));
                    self.set_state(&game_id, GameStatus::Idle, error);
                    return;
                }
            }
            if cancel.load(Ordering::Acquire)
                && steam_progress.completed
                && steam_progress.active_processes == 0
            {
                let since = cancelled_without_process_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= EXIT_GRACE {
                    self.set_state(&game_id, GameStatus::Idle, None);
                    return;
                }
            } else {
                cancelled_without_process_since = None;
            }
            if started.elapsed() >= START_TIMEOUT {
                let error = if cancel.load(Ordering::Acquire) {
                    (steam_progress.active_processes > 0).then_some(
                        "Monitor stage failed: Steam did not confirm that the game launch stopped within two minutes"
                            .to_owned(),
                    )
                } else {
                    Some(
                        "Monitor stage failed: Steam did not start a detectable game process within two minutes"
                            .to_owned(),
                    )
                };
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
                    self.set_state(
                        &game_id,
                        GameStatus::Idle,
                        Some(format!("Monitor stage failed: {error}")),
                    );
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

struct SteamLaunchLog {
    path: PathBuf,
    offset: u64,
    pending: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
enum SteamLaunchEvent {
    Failed(String),
    Completed,
    ProcessAdded,
    ProcessUpdated,
    ProcessRemoved,
}

#[derive(Default)]
struct SteamLaunchProgress {
    completed: bool,
    active_processes: usize,
}

impl SteamLaunchProgress {
    fn observe(&mut self, event: SteamLaunchEvent) -> Option<String> {
        match event {
            SteamLaunchEvent::Failed(error) => Some(error),
            SteamLaunchEvent::Completed => {
                self.completed = true;
                None
            }
            SteamLaunchEvent::ProcessAdded => {
                self.active_processes = self.active_processes.saturating_add(1);
                None
            }
            SteamLaunchEvent::ProcessUpdated => {
                self.active_processes = self.active_processes.max(1);
                None
            }
            SteamLaunchEvent::ProcessRemoved => {
                self.active_processes = self.active_processes.saturating_sub(1);
                None
            }
        }
    }
}

impl SteamLaunchLog {
    fn new(path: PathBuf) -> Self {
        let offset = fs::metadata(&path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        Self {
            path,
            offset,
            pending: Vec::new(),
        }
    }

    fn launch_event(&mut self, app_id: u32) -> Option<SteamLaunchEvent> {
        let mut file = File::open(&self.path).ok()?;
        let len = file.metadata().ok()?.len();
        if len < self.offset {
            self.offset = 0;
            self.pending.clear();
        }
        file.seek(SeekFrom::Start(self.offset)).ok()?;
        let mut chunk = Vec::new();
        file.take(LOG_READ_LIMIT).read_to_end(&mut chunk).ok()?;
        self.offset += chunk.len() as u64;
        self.pending.extend_from_slice(&chunk);

        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line = self.pending.drain(..=end).collect::<Vec<_>>();
            if let Ok(line) = std::str::from_utf8(&line)
                && let Some(event) = parse_launch_event(line, app_id)
            {
                return Some(event);
            }
        }
        if self.pending.len() > LOG_LINE_LIMIT {
            self.pending.clear();
        }
        None
    }
}

fn parse_launch_event(line: &str, app_id: u32) -> Option<SteamLaunchEvent> {
    if line.contains(&format!("Game process added : AppID {app_id} ")) {
        return Some(SteamLaunchEvent::ProcessAdded);
    }
    if line.contains(&format!("Game process updated : AppID {app_id} ")) {
        return Some(SteamLaunchEvent::ProcessUpdated);
    }
    if line.contains(&format!("Game process removed: AppID {app_id} ")) {
        return Some(SteamLaunchEvent::ProcessRemoved);
    }
    let marker = format!("GameAction [AppID {app_id}, ActionID ");
    let action = line.split_once(&marker)?.1;
    let action = action.split_once("] : LaunchApp ")?.1;
    if let Some(detail) = action.strip_prefix("failed with ") {
        let code = detail.split_whitespace().next()?;
        if code.starts_with("AppError_")
            && code.len() <= 32
            && code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Some(SteamLaunchEvent::Failed(format!(
                "Steam refused to launch this game ({code})"
            )));
        }
        return Some(SteamLaunchEvent::Failed(
            "Steam refused to launch this game".to_owned(),
        ));
    }
    if action.starts_with("changed task to Failed") {
        return Some(SteamLaunchEvent::Failed(
            "Steam reported that this game launch failed".to_owned(),
        ));
    }
    action
        .starts_with("changed task to Completed")
        .then_some(SteamLaunchEvent::Completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[cfg(target_os = "linux")]
    struct ChildGuard(std::process::Child);

    #[cfg(target_os = "linux")]
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn test_dir(label: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "legio-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn controlled_process_runs_through_prepare_launch_monitor_and_terminate() {
        const APP_ID: u32 = 4_294_967_293;
        let base = test_dir("lifecycle-process");
        let install_path = base.join("game");
        let steam_root = base.join("steam");
        fs::create_dir(&install_path).unwrap();
        fs::create_dir(&steam_root).unwrap();
        let target = LaunchTarget::prepare(APP_ID, install_path.clone(), steam_root).unwrap();

        let manager = GameLaunchManager::new();
        let cancel = Arc::new(AtomicBool::new(false));
        manager.entries.lock().unwrap().insert(
            "controlled".to_owned(),
            Entry {
                app_id: APP_ID,
                install_path,
                status: GameStatus::Launching,
                error: None,
                cancel: Arc::clone(&cancel),
            },
        );

        let (child_sender, child_receiver) = std::sync::mpsc::channel();
        let worker_manager = manager.clone();
        let worker = thread::spawn(move || {
            worker_manager.run_launch("controlled".to_owned(), target, cancel, move |_| {
                let child = std::process::Command::new("sleep")
                    .arg("30")
                    .env("SteamAppId", APP_ID.to_string())
                    .spawn()
                    .map_err(|_| "Could not start controlled test executable".to_owned())?;
                child_sender
                    .send(ChildGuard(child))
                    .map_err(|_| "Could not retain controlled test process".to_owned())?;
                Ok(())
            });
        });
        let _child = child_receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("controlled process did not start");

        let started = Instant::now();
        while manager.list().unwrap()[0].status != GameStatus::Running {
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "controlled process was not detected"
            );
            thread::sleep(Duration::from_millis(20));
        }
        manager.stop("controlled").unwrap();
        worker.join().unwrap();

        let state = &manager.list().unwrap()[0];
        assert_eq!(state.status, GameStatus::Idle);
        assert_eq!(state.error, None);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn missing_launch_executable_is_reported_with_its_stage() {
        let base = test_dir("lifecycle-missing-executable");
        let install_path = base.join("game");
        let steam_root = base.join("steam");
        fs::create_dir(&install_path).unwrap();
        fs::create_dir(&steam_root).unwrap();
        let target = LaunchTarget::prepare(42, install_path.clone(), steam_root).unwrap();
        let manager = GameLaunchManager::new();
        let cancel = Arc::new(AtomicBool::new(false));
        manager.entries.lock().unwrap().insert(
            "missing".to_owned(),
            Entry {
                app_id: 42,
                install_path,
                status: GameStatus::Launching,
                error: None,
                cancel: Arc::clone(&cancel),
            },
        );

        let missing_executable = base.join("missing-game");
        manager.run_launch("missing".to_owned(), target, cancel, move |_| {
            std::process::Command::new(missing_executable)
                .spawn()
                .map(|_| ())
                .map_err(|_| "Executable was not found".to_owned())
        });

        let state = &manager.list().unwrap()[0];
        assert_eq!(state.status, GameStatus::Idle);
        assert_eq!(
            state.error.as_deref(),
            Some("Launch stage failed: Executable was not found")
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn reads_only_new_failures_for_the_requested_app() {
        let path = std::env::temp_dir().join(format!(
            "legio-steam-launch-log-{}-{:?}",
            std::process::id(),
            thread::current().id()
        ));
        fs::write(
            &path,
            b"[time] GameAction [AppID 42, ActionID 1] : LaunchApp failed with AppError_5 with \"\"\n",
        )
        .unwrap();
        let mut watch = SteamLaunchLog::new(path.clone());
        assert_eq!(watch.launch_event(42), None);
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(
            b"[time] GameAction [AppID 43, ActionID 2] : LaunchApp failed with AppError_5 with \"\"\n",
        )
        .unwrap();
        assert_eq!(watch.launch_event(42), None);
        file.write_all(
            b"[time] GameAction [AppID 42, ActionID 3] : LaunchApp failed with AppError_5 with \"\"\n",
        )
        .unwrap();
        assert_eq!(
            watch.launch_event(42),
            Some(SteamLaunchEvent::Failed(
                "Steam refused to launch this game (AppError_5)".to_owned()
            ))
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn parse_failure_never_echoes_untrusted_steam_log_content() {
        assert_eq!(
            parse_launch_event(
                "GameAction [AppID 42, ActionID 1] : LaunchApp failed with secret=value with \"\"",
                42,
            ),
            Some(SteamLaunchEvent::Failed(
                "Steam refused to launch this game".to_owned()
            ))
        );
        assert_eq!(
            parse_launch_event(
                "GameAction [AppID 42, ActionID 1] : LaunchApp changed task to Failed with \"\"",
                42,
            ),
            Some(SteamLaunchEvent::Failed(
                "Steam reported that this game launch failed".to_owned()
            ))
        );
        assert_eq!(
            parse_launch_event(
                "GameAction [AppID 42, ActionID 1] : LaunchApp changed task to Completed with \"\"",
                42,
            ),
            Some(SteamLaunchEvent::Completed)
        );
    }

    #[test]
    fn reads_failure_after_steam_log_is_truncated() {
        let path = std::env::temp_dir().join(format!(
            "legio-steam-truncated-log-{}-{:?}",
            std::process::id(),
            thread::current().id()
        ));
        fs::write(&path, vec![b'x'; 1024]).unwrap();
        let mut watch = SteamLaunchLog::new(path.clone());
        fs::write(
            &path,
            b"GameAction [AppID 42, ActionID 1] : LaunchApp failed with AppError_5 with \"\"\n",
        )
        .unwrap();
        assert_eq!(
            watch.launch_event(42),
            Some(SteamLaunchEvent::Failed(
                "Steam refused to launch this game (AppError_5)".to_owned()
            ))
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn reads_completed_launch_only_for_requested_app() {
        let path = std::env::temp_dir().join(format!(
            "legio-steam-completed-log-{}-{:?}",
            std::process::id(),
            thread::current().id()
        ));
        fs::write(&path, b"").unwrap();
        let mut watch = SteamLaunchLog::new(path.clone());
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(
            b"GameAction [AppID 43, ActionID 1] : LaunchApp changed task to Completed with \"\"\n",
        )
        .unwrap();
        assert_eq!(watch.launch_event(42), None);
        file.write_all(
            b"GameAction [AppID 42, ActionID 2] : LaunchApp changed task to Completed with \"\"\n",
        )
        .unwrap();
        assert_eq!(watch.launch_event(42), Some(SteamLaunchEvent::Completed));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn steam_process_events_keep_proton_launch_active_after_completion() {
        let path = std::env::temp_dir().join(format!(
            "legio-steam-process-log-{}-{:?}",
            std::process::id(),
            thread::current().id()
        ));
        fs::write(&path, b"").unwrap();
        let mut watch = SteamLaunchLog::new(path.clone());
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(concat!(
            "Game process added : AppID 42 \"steam-launch-wrapper\", ProcID 10\n",
            "GameAction [AppID 42, ActionID 1] : LaunchApp changed task to Completed with \"\"\n",
            "Game process updated : AppID 42 \"steam-launch-wrapper\", ProcID 11\n",
            "Game process removed: AppID 42 \"steam-launch-wrapper\", ProcID 11\n",
        ).as_bytes())
        .unwrap();
        assert_eq!(watch.launch_event(42), Some(SteamLaunchEvent::ProcessAdded));
        assert_eq!(watch.launch_event(42), Some(SteamLaunchEvent::Completed));
        assert_eq!(
            watch.launch_event(42),
            Some(SteamLaunchEvent::ProcessUpdated)
        );
        assert_eq!(
            watch.launch_event(42),
            Some(SteamLaunchEvent::ProcessRemoved)
        );
        assert_eq!(watch.launch_event(42), None);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn completion_waits_for_all_steam_game_processes_to_end() {
        let mut progress = SteamLaunchProgress::default();
        for event in [
            SteamLaunchEvent::ProcessAdded,
            SteamLaunchEvent::ProcessAdded,
            SteamLaunchEvent::Completed,
            SteamLaunchEvent::ProcessUpdated,
            SteamLaunchEvent::ProcessRemoved,
        ] {
            assert_eq!(progress.observe(event), None);
        }
        assert!(progress.completed);
        assert_eq!(progress.active_processes, 1);
        assert_eq!(progress.observe(SteamLaunchEvent::ProcessRemoved), None);
        assert_eq!(progress.active_processes, 0);
    }
}
