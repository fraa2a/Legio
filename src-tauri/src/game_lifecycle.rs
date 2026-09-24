use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
#[cfg(target_os = "linux")]
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use crate::runner_discovery::RunnerKind;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::database::DatabaseState;
#[cfg(target_os = "linux")]
use crate::database::EffectiveCompatibilityConfig;
use crate::{game_process, runner_discovery, steam_local, steam_switch};

const START_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const EXIT_GRACE: Duration = Duration::from_secs(3);
const SESSION_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
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
    app_id: Option<u32>,
    process_target: game_process::ProcessTarget,
    status: GameStatus,
    error: Option<String>,
    cancel: Arc<AtomicBool>,
}

struct LaunchTarget {
    install_path: PathBuf,
    steam_root: PathBuf,
}

struct SessionTracking {
    app: AppHandle,
    game_id: String,
    last_heartbeat: Instant,
}

impl SessionTracking {
    fn start(app: AppHandle, game_id: &str) -> Result<Self, String> {
        app.state::<DatabaseState>()
            .database()?
            .start_game_session(game_id, crate::database::now_milliseconds())?;
        Ok(Self {
            app,
            game_id: game_id.to_owned(),
            last_heartbeat: Instant::now(),
        })
    }

    fn heartbeat_if_due(&mut self) -> Result<(), String> {
        if self.last_heartbeat.elapsed() < SESSION_HEARTBEAT_INTERVAL {
            return Ok(());
        }
        self.app
            .state::<DatabaseState>()
            .database()?
            .heartbeat_game_session(&self.game_id, crate::database::now_milliseconds())?;
        self.last_heartbeat = Instant::now();
        Ok(())
    }
}

impl Drop for SessionTracking {
    fn drop(&mut self) {
        if let Ok(database) = self.app.state::<DatabaseState>().database() {
            let _ = database
                .end_game_session(&self.game_id, crate::database::now_milliseconds());
        }
    }
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
        let process_target = game_process::ProcessTarget::Steam {
            app_id,
            install_path: target.install_path.clone(),
        };
        let cancel = self.reserve_launch(&game_id, Some(app_id), process_target.clone())?;

        let worker_id = game_id.clone();
        let manager = self.clone();
        let launch_game_id = worker_id.clone();
        let session_app = app.clone();
        let steam_log_path = target.steam_root.join("logs/console_log.txt");
        if let Err(error) = thread::Builder::new()
            .name(format!("legio-game-{app_id}"))
            .spawn(move || {
                manager.run_launch(
                    worker_id,
                    process_target,
                    Some(app_id),
                    Some(steam_log_path),
                    cancel,
                    Some(session_app),
                    move |cancel| {
                        steam_switch::launch(&app, &launch_game_id, confirm_account_switch, cancel)
                            .map(|_| None)
                    },
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

    #[cfg(target_os = "linux")]
    pub(crate) fn launch_with_runner(
        &self,
        app: AppHandle,
        game_id: String,
        runner_path: String,
    ) -> Result<(), String> {
        let config = EffectiveCompatibilityConfig {
            runner_path: Some(runner_path),
            ..EffectiveCompatibilityConfig::default()
        };
        self.launch_with_compatibility_config(app, game_id, config)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn launch_configured(&self, app: AppHandle, game_id: String) -> Result<(), String> {
        let config = app
            .state::<DatabaseState>()
            .database()?
            .effective_compatibility_config(&game_id)?;
        let config = if config
            .runner_path
            .as_deref()
            .is_some_and(|path| !path.is_empty())
        {
            config
        } else {
            let runner_path = runner_discovery::discover()
                .runners
                .into_iter()
                .next()
                .map(|runner| runner.path)
                .ok_or_else(|| "No compatible Proton or Wine runner is installed".to_owned())?;
            EffectiveCompatibilityConfig {
                runner_path: Some(runner_path),
                ..config
            }
        };
        self.launch_with_compatibility_config(app, game_id, config)
    }

    #[cfg(target_os = "linux")]
    fn launch_with_compatibility_config(
        &self,
        app: AppHandle,
        game_id: String,
        config: EffectiveCompatibilityConfig,
    ) -> Result<(), String> {
        let game = app.state::<DatabaseState>().database()?.game(&game_id)?;
        if game.steam_app_id.is_some() || game.steam_install_path.is_some() {
            return Err("Compatibility runners can only launch manually imported games".to_owned());
        }
        let executable_path = game
            .executable_path
            .as_deref()
            .ok_or_else(|| "This manual game has no selected executable".to_owned())?;
        let executable_path = fs::canonicalize(executable_path)
            .map_err(|error| format!("Could not inspect game executable: {error}"))?;
        if !executable_path.is_file()
            || !executable_path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        {
            return Err("Selected file is not a Windows executable".to_owned());
        }
        let runner_path = config
            .runner_path
            .as_deref()
            .filter(|path| !path.is_empty())
            .ok_or_else(|| "No compatibility runner is selected".to_owned())?;
        let runner = runner_discovery::resolve_runner(runner_path)?;
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("Could not resolve application data directory: {error}"))?;
        validate_launch_arguments(&config.arguments_before)?;
        validate_launch_arguments(&config.arguments_after)?;
        validate_launch_environment(&config.environment)?;
        validate_dll_overrides(&config.dll_overrides)?;
        let working_directory = game_working_directory(
            config.working_directory.as_deref(),
            executable_path
                .parent()
                .ok_or_else(|| "Game executable has no parent directory".to_owned())?,
        )?;
        let compat_data_path = game_compatdata_path(
            &data_dir,
            &game.id,
            config.prefix_root.as_deref(),
            config.prefix_path.as_deref(),
        )?;
        let mut command = runner_discovery::launch_command(
            &runner,
            &executable_path,
            &config.arguments_before,
            &config.arguments_after,
        );
        command
            .current_dir(working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (key, value) in &config.environment {
            command.env(key, value);
        }
        if !config.dll_overrides.is_empty() {
            command.env(
                "WINEDLLOVERRIDES",
                format_dll_overrides(&config.dll_overrides),
            );
        }
        if matches!(runner.kind, RunnerKind::Proton | RunnerKind::GeProton) {
            command
                .env("STEAM_COMPAT_DATA_PATH", &compat_data_path)
                .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", steam_client_root()?);
        } else {
            command.env("WINEPREFIX", &compat_data_path);
        }

        let token = uuid::Uuid::new_v4().to_string();
        command.env(game_process::LAUNCH_TOKEN_ENV, &token);
        let process_target = game_process::ProcessTarget::Runner {
            token,
            executable_path,
            launcher_pid: None,
        };
        let cancel = self.reserve_launch(&game_id, None, process_target.clone())?;
        let manager = self.clone();
        let worker_id = game_id.clone();
        let session_app = app.clone();
        if let Err(error) = thread::Builder::new()
            .name(format!("legio-game-runner-{}", game.id))
            .spawn(move || {
                manager.run_launch(
                    worker_id,
                    process_target,
                    None,
                    None,
                    cancel,
                    Some(session_app),
                    move |_| {
                        command.spawn().map(Some).map_err(|error| {
                            format!("Could not start compatibility runner: {error}")
                        })
                    },
                );
            })
        {
            self.set_state(&game_id, GameStatus::Idle, Some(error.to_string()));
            return Err(format!("Could not start game launch task: {error}"));
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub(crate) fn launch_configured(
        &self,
        _app: AppHandle,
        _game_id: String,
    ) -> Result<(), String> {
        Err("Compatibility runner launches are supported on Linux only".to_owned())
    }

    #[cfg(not(target_os = "linux"))]
    pub(crate) fn launch_with_runner(
        &self,
        _app: AppHandle,
        _game_id: String,
        _runner_path: String,
    ) -> Result<(), String> {
        Err("Compatibility runner launches are supported on Linux only".to_owned())
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
        let process_target = {
            let entries = self.lock()?;
            let entry = entries
                .get(game_id)
                .filter(|entry| entry.status == GameStatus::Running)
                .ok_or_else(|| "This game is not running".to_owned())?;
            entry.process_target.clone()
        };
        game_process::stop(&process_target)
    }

    fn run_launch(
        &self,
        game_id: String,
        mut process_target: game_process::ProcessTarget,
        steam_app_id: Option<u32>,
        steam_log_path: Option<PathBuf>,
        cancel: Arc<AtomicBool>,
        session_app: Option<AppHandle>,
        launch: impl FnOnce(&AtomicBool) -> Result<Option<Child>, String>,
    ) {
        let mut steam_log = steam_log_path.map(SteamLaunchLog::new);
        if cancel.load(Ordering::Acquire) {
            self.set_state(&game_id, GameStatus::Idle, None);
            return;
        }
        let mut child = match launch(&cancel) {
            Ok(child) => child,
            Err(error) => {
                let error = (!cancel.load(Ordering::Acquire) || error != "Launch cancelled")
                    .then(|| format!("Launch stage failed: {error}"));
                self.set_state(&game_id, GameStatus::Idle, error);
                return;
            }
        };
        #[cfg(target_os = "linux")]
        if matches!(process_target, game_process::ProcessTarget::Runner { .. })
            && let Some(pid) = child.as_ref().map(Child::id)
        {
            if let game_process::ProcessTarget::Runner { launcher_pid, .. } = &mut process_target {
                *launcher_pid = Some(pid);
            }
            if let Err(error) = self.set_process_target(&game_id, process_target.clone()) {
                let cleanup_error = terminate_child(&mut child).err();
                self.set_state(
                    &game_id,
                    GameStatus::Idle,
                    Some(append_cleanup_error(error, cleanup_error)),
                );
                return;
            }
        }
        let started = Instant::now();
        let mut steam_progress = SteamLaunchProgress::default();
        let mut cancelled_without_process_since = None;
        loop {
            match game_process::matching_pids(&process_target) {
                Ok(pids) if !pids.is_empty() => {
                    if cancel.load(Ordering::Acquire) {
                        let error = game_process::stop(&process_target).err();
                        let cleanup_error = terminate_child(&mut child).err();
                        let error = error
                            .map(|error| {
                                append_cleanup_error(
                                    format!("Stop stage failed: {error}"),
                                    cleanup_error.clone(),
                                )
                            })
                            .or_else(|| {
                                cleanup_error.map(|error| format!("Stop stage failed: {error}"))
                            });
                        self.set_state(&game_id, GameStatus::Idle, error);
                        return;
                    }
                    break;
                }
                Ok(_) => {}
                Err(error) => {
                    let cleanup_error = terminate_child(&mut child).err();
                    self.set_state(
                        &game_id,
                        GameStatus::Idle,
                        Some(append_cleanup_error(
                            format!("Monitor stage failed: {error}"),
                            cleanup_error,
                        )),
                    );
                    return;
                }
            }
            if let (Some(steam_log), Some(app_id)) = (&mut steam_log, steam_app_id) {
                while let Some(event) = steam_log.launch_event(app_id) {
                    if let Some(error) = steam_progress.observe(event) {
                        let error = (!cancel.load(Ordering::Acquire))
                            .then(|| format!("Launch stage failed: {error}"));
                        let cleanup_error = terminate_child(&mut child).err();
                        self.set_state(
                            &game_id,
                            GameStatus::Idle,
                            error.map(|error| append_cleanup_error(error, cleanup_error)),
                        );
                        return;
                    }
                }
            }
            if let Some(runner_child) = child.as_mut() {
                match runner_child.try_wait() {
                    Ok(Some(status)) => {
                        child.take();
                        if !status.success() && !cancel.load(Ordering::Acquire) {
                            self.set_state(
                                &game_id,
                                GameStatus::Idle,
                                Some(format!("Launch stage failed: runner exited before the game started ({status})")),
                            );
                            return;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let cleanup_error = terminate_child(&mut child).err();
                        self.set_state(
                            &game_id,
                            GameStatus::Idle,
                            Some(append_cleanup_error(
                                format!("Monitor stage failed: could not wait for runner: {error}"),
                                cleanup_error,
                            )),
                        );
                        return;
                    }
                }
            }
            if cancel.load(Ordering::Acquire) && steam_app_id.is_none() {
                let cleanup_error = terminate_child(&mut child).err();
                let error = game_process::stop(&process_target).err();
                let error = error
                    .map(|error| {
                        append_cleanup_error(
                            format!("Cancel stage failed: {error}"),
                            cleanup_error.clone(),
                        )
                    })
                    .or_else(|| cleanup_error.map(|error| format!("Cancel stage failed: {error}")));
                self.set_state(&game_id, GameStatus::Idle, error);
                return;
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
                let error = if steam_app_id.is_none() {
                    Some("Monitor stage failed: compatibility runner did not start a detectable game process within two minutes".to_owned())
                } else if cancel.load(Ordering::Acquire) {
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
                let cleanup_error = terminate_child(&mut child).err();
                self.set_state(
                    &game_id,
                    GameStatus::Idle,
                    error.map(|error| append_cleanup_error(error, cleanup_error)),
                );
                return;
            }
            thread::sleep(POLL_INTERVAL);
        }
        let mut session_error = None;
        let mut session = session_app.and_then(|app| match SessionTracking::start(app, &game_id) {
            Ok(session) => Some(session),
            Err(error) => {
                session_error = Some(format!("Session tracking failed: {error}"));
                None
            }
        });
        self.set_state(&game_id, GameStatus::Running, session_error);
        let mut missing_since = None;
        loop {
            let heartbeat_error = session
                .as_mut()
                .and_then(|session| session.heartbeat_if_due().err());
            if let Some(error) = heartbeat_error {
                session.take();
                self.set_state(
                    &game_id,
                    GameStatus::Running,
                    Some(format!("Session tracking failed: {error}")),
                );
            }
            match game_process::matching_pids(&process_target) {
                Ok(pids) if pids.is_empty() => {
                    let since = missing_since.get_or_insert_with(Instant::now);
                    if since.elapsed() >= EXIT_GRACE {
                        let cleanup_error = terminate_child(&mut child).err();
                        if let Some(error) = cleanup_error {
                            self.set_state(
                                &game_id,
                                GameStatus::Idle,
                                Some(format!("Stop stage failed: {error}")),
                            );
                            return;
                        }
                        self.set_state(&game_id, GameStatus::Idle, None);
                        return;
                    }
                }
                Ok(_) => missing_since = None,
                Err(error) => {
                    let cleanup_error = terminate_child(&mut child).err();
                    let error = append_cleanup_error(
                        format!("Monitor stage failed: {error}"),
                        cleanup_error,
                    );
                    self.set_state(&game_id, GameStatus::Idle, Some(error));
                    return;
                }
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn reserve_launch(
        &self,
        game_id: &str,
        app_id: Option<u32>,
        process_target: game_process::ProcessTarget,
    ) -> Result<Arc<AtomicBool>, String> {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut entries = self.lock()?;
        if entries.get(game_id).is_some_and(|entry| {
            matches!(entry.status, GameStatus::Launching | GameStatus::Running)
        }) {
            return Err("This game is already launching or running".to_owned());
        }
        if app_id.is_some_and(|app_id| {
            entries.values().any(|entry| {
                entry.app_id == Some(app_id)
                    && matches!(entry.status, GameStatus::Launching | GameStatus::Running)
            })
        }) {
            return Err("This Steam App ID is already launching or running".to_owned());
        }
        entries.insert(
            game_id.to_owned(),
            Entry {
                app_id,
                process_target,
                status: GameStatus::Launching,
                error: None,
                cancel: Arc::clone(&cancel),
            },
        );
        Ok(cancel)
    }

    fn set_process_target(
        &self,
        game_id: &str,
        process_target: game_process::ProcessTarget,
    ) -> Result<(), String> {
        let mut entries = self.lock()?;
        let entry = entries
            .get_mut(game_id)
            .ok_or_else(|| "Game launch state disappeared".to_owned())?;
        entry.process_target = process_target;
        Ok(())
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

#[cfg(target_os = "linux")]
fn game_compatdata_path(
    data_dir: &Path,
    game_id: &str,
    prefix_root: Option<&str>,
    prefix_path: Option<&str>,
) -> Result<PathBuf, String> {
    if let Some(prefix_path) = prefix_path.filter(|path| !path.is_empty()) {
        return create_prefix_directory(Path::new(prefix_path), None);
    }

    let root = match prefix_root.filter(|path| !path.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => data_dir.join("compatdata"),
    };
    create_prefix_directory(&root, Some(game_id))
}

#[cfg(target_os = "linux")]
fn create_prefix_directory(root: &Path, game_id: Option<&str>) -> Result<PathBuf, String> {
    if !root.is_absolute() || root.to_string_lossy().chars().any(char::is_control) {
        return Err(
            "Compatibility prefix paths must be absolute and contain no control characters"
                .to_owned(),
        );
    }
    fs::create_dir_all(root)
        .map_err(|error| format!("Could not create compatibility prefix directory: {error}"))?;
    let root = fs::canonicalize(root)
        .map_err(|error| format!("Could not resolve compatibility prefix directory: {error}"))?;
    let Some(game_id) = game_id else {
        if !root.is_dir() {
            return Err("Compatibility prefix path is not a directory".to_owned());
        }
        return Ok(root);
    };
    let prefix = root.join(game_id);
    fs::create_dir_all(&prefix)
        .map_err(|error| format!("Could not create game compatibility prefix: {error}"))?;
    let prefix = fs::canonicalize(&prefix)
        .map_err(|error| format!("Could not resolve game compatibility prefix: {error}"))?;
    if !prefix.starts_with(&root) {
        return Err("Game compatibility prefix escapes its configured root".to_owned());
    }
    Ok(prefix)
}

#[cfg(target_os = "linux")]
fn game_working_directory(path: Option<&str>, game_directory: &Path) -> Result<PathBuf, String> {
    let Some(path) = path.filter(|path| !path.is_empty()) else {
        return Ok(game_directory.to_path_buf());
    };
    let path = Path::new(path);
    if !path.is_absolute() || path.to_string_lossy().chars().any(char::is_control) {
        return Err(
            "Working directory must be an absolute path without control characters".to_owned(),
        );
    }
    let path = fs::canonicalize(path)
        .map_err(|error| format!("Could not resolve working directory: {error}"))?;
    if !path.is_dir() {
        return Err("Working directory is not a directory".to_owned());
    }
    Ok(path)
}

#[cfg(target_os = "linux")]
fn validate_launch_arguments(arguments: &[String]) -> Result<(), String> {
    if arguments.iter().any(|argument| argument.contains('\0')) {
        return Err("Launch arguments cannot contain null characters".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn validate_launch_environment(
    environment: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    for (key, value) in environment {
        if key.is_empty() || key.contains('=') || key.contains('\0') || value.contains('\0') {
            return Err("Compatibility environment contains an invalid variable".to_owned());
        }
        if [
            "WINEPREFIX",
            "STEAM_COMPAT_DATA_PATH",
            "STEAM_COMPAT_CLIENT_INSTALL_PATH",
            "WINEDLLOVERRIDES",
            game_process::LAUNCH_TOKEN_ENV,
        ]
        .iter()
        .any(|reserved| key.eq_ignore_ascii_case(reserved))
        {
            return Err(format!(
                "Compatibility environment variable {key} is managed by Legio"
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn validate_dll_overrides(
    overrides: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    for (name, value) in overrides {
        if name.is_empty()
            || !name.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'*')
            })
            || value.is_empty()
            || !value
                .split(',')
                .all(|part| matches!(part, "native" | "builtin" | "n" | "b"))
        {
            return Err("Compatibility DLL override is invalid".to_owned());
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn format_dll_overrides(overrides: &std::collections::BTreeMap<String, String>) -> String {
    overrides
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(target_os = "linux")]
fn steam_client_root() -> Result<PathBuf, String> {
    steam_local::default_steam_roots()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|root| fs::canonicalize(root).ok())
        .find(|root| {
            root.join("steamapps").is_dir()
                && (root.join("steam.sh").is_file() || root.join("steam").is_dir())
        })
        .ok_or_else(|| "Proton requires an available local Steam client installation".to_owned())
}

fn terminate_child(child: &mut Option<Child>) -> Result<(), String> {
    let Some(process) = child.as_mut() else {
        return Ok(());
    };
    match process.try_wait() {
        Ok(Some(_)) => {
            child.take();
            return Ok(());
        }
        Ok(None) => {}
        Err(error) => {
            let kill_error = process.kill().err();
            let wait_error = process.wait().err();
            if wait_error.is_none() {
                child.take();
            }
            return Err(format!(
                "Could not inspect runner process: {error}{}{}",
                kill_error
                    .map(|error| format!("; could not stop runner process: {error}"))
                    .unwrap_or_default(),
                wait_error
                    .map(|error| format!("; could not wait for runner process: {error}"))
                    .unwrap_or_default()
            ));
        }
    }
    let kill_error = process.kill().err();
    let wait_error = process.wait().err();
    if wait_error.is_none() {
        child.take();
    }
    match (kill_error, wait_error) {
        (None, None) => Ok(()),
        (kill_error, wait_error) => Err(format!(
            "{}{}",
            kill_error
                .map(|error| format!("Could not stop runner process: {error}"))
                .unwrap_or_default(),
            wait_error
                .map(|error| format!("; could not wait for runner process: {error}"))
                .unwrap_or_default()
        )),
    }
}

fn append_cleanup_error(error: String, cleanup_error: Option<String>) -> String {
    match cleanup_error {
        Some(cleanup_error) => format!("{error}; {cleanup_error}"),
        None => error,
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
    fn runner_test_target(base: &Path, token: &str) -> game_process::ProcessTarget {
        let executable = base.join("Test Game.exe");
        fs::write(&executable, b"stub").unwrap();
        game_process::ProcessTarget::Runner {
            token: token.to_owned(),
            executable_path: fs::canonicalize(executable).unwrap(),
            launcher_pid: None,
        }
    }

    #[cfg(target_os = "linux")]
    fn wait_for_status(manager: &GameLaunchManager, game_id: &str, status: GameStatus) {
        let started = Instant::now();
        while manager
            .list()
            .unwrap()
            .iter()
            .find(|entry| entry.game_id == game_id)
            .unwrap()
            .status
            != status
        {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "game did not reach {status:?}"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn compatibility_launch_config_validates_arguments_environment_and_dll_overrides() {
        assert!(validate_launch_arguments(&["-safe".to_owned()]).is_ok());
        assert!(validate_launch_arguments(&["bad\0arg".to_owned()]).is_err());

        let mut environment = std::collections::BTreeMap::new();
        environment.insert("WINEDEBUG".to_owned(), "-all".to_owned());
        assert!(validate_launch_environment(&environment).is_ok());
        environment.insert("WINEPREFIX".to_owned(), "/tmp/override".to_owned());
        assert!(validate_launch_environment(&environment).is_err());

        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert("d3d11".to_owned(), "native,builtin".to_owned());
        overrides.insert("dxgi".to_owned(), "n".to_owned());
        assert!(validate_dll_overrides(&overrides).is_ok());
        assert_eq!(
            format_dll_overrides(&overrides),
            "d3d11=native,builtin;dxgi=n"
        );
        overrides.insert(
            "dxgi".to_owned(),
            "anything;STEAM_COMPAT_DATA_PATH=/tmp".to_owned(),
        );
        assert!(validate_dll_overrides(&overrides).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn compatibility_prefix_and_working_directory_resolve_configured_paths() {
        let base = test_dir("compat-paths");
        let app_data = base.join("app-data");
        let prefix_root = base.join("prefixes");
        let game_id = "00000000-0000-0000-0000-000000000001";
        let generated = game_compatdata_path(
            &app_data,
            game_id,
            Some(prefix_root.to_str().unwrap()),
            None,
        )
        .unwrap();
        assert_eq!(
            generated,
            fs::canonicalize(&prefix_root).unwrap().join(game_id)
        );

        let custom_prefix = base.join("custom-prefix");
        assert_eq!(
            game_compatdata_path(
                &app_data,
                game_id,
                Some(prefix_root.to_str().unwrap()),
                Some(custom_prefix.to_str().unwrap()),
            )
            .unwrap(),
            fs::canonicalize(custom_prefix).unwrap()
        );

        let game_directory = base.join("game");
        fs::create_dir_all(&game_directory).unwrap();
        assert_eq!(
            game_working_directory(None, &game_directory).unwrap(),
            game_directory
        );
        assert!(game_working_directory(Some("relative"), &game_directory).is_err());
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(target_os = "linux")]
    fn runner_stub(executable_name: &str, token: &str) -> Child {
        std::process::Command::new("/bin/bash")
            .args([
                "-c",
                "bash -c 'exec -a \"$1\" sleep 60' _ \"$1\" & wait",
                "legio-runner-stub",
                executable_name,
            ])
            .env(game_process::LAUNCH_TOKEN_ENV, token)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn runner_launch_tracks_game_then_stop_returns_to_idle() {
        let base = test_dir("runner-lifecycle");
        let token = uuid::Uuid::new_v4().to_string();
        let target = runner_test_target(&base, &token);
        let executable_name = match &target {
            game_process::ProcessTarget::Runner {
                executable_path, ..
            } => executable_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            _ => unreachable!(),
        };
        let manager = GameLaunchManager::new();
        let cancel = manager
            .reserve_launch("runner", None, target.clone())
            .unwrap();
        assert_eq!(manager.list().unwrap()[0].status, GameStatus::Launching);
        let worker_manager = manager.clone();
        let worker = thread::spawn(move || {
            worker_manager.run_launch("runner".to_owned(), target, None, None, cancel, None, move |_| {
                Ok(Some(runner_stub(&executable_name, &token)))
            });
        });
        wait_for_status(&manager, "runner", GameStatus::Running);
        manager.stop("runner").unwrap();
        worker.join().unwrap();
        assert_eq!(manager.list().unwrap()[0].status, GameStatus::Idle);
        assert_eq!(manager.list().unwrap()[0].error, None);
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn runner_spawn_failure_returns_to_idle_with_error() {
        let base = test_dir("runner-spawn-failure");
        let manager = GameLaunchManager::new();
        let token = uuid::Uuid::new_v4().to_string();
        let target = runner_test_target(&base, &token);
        let cancel = manager
            .reserve_launch("runner", None, target.clone())
            .unwrap();
        manager.run_launch("runner".to_owned(), target, None, None, cancel, None, |_| {
            Err("Could not start compatibility runner: no such file".to_owned())
        });
        let state = &manager.list().unwrap()[0];
        assert_eq!(state.status, GameStatus::Idle);
        assert_eq!(
            state.error.as_deref(),
            Some("Launch stage failed: Could not start compatibility runner: no such file")
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn runner_wrapper_failure_returns_to_idle() {
        let base = test_dir("runner-wrapper-failure");
        let manager = GameLaunchManager::new();
        let token = uuid::Uuid::new_v4().to_string();
        let target = runner_test_target(&base, &token);
        let cancel = manager
            .reserve_launch("runner", None, target.clone())
            .unwrap();
        manager.run_launch("runner".to_owned(), target, None, None, cancel, None, |_| {
            std::process::Command::new("/bin/bash")
                .args(["-c", "exit 7"])
                .spawn()
                .map(Some)
                .map_err(|error| error.to_string())
        });
        let state = &manager.list().unwrap()[0];
        assert_eq!(state.status, GameStatus::Idle);
        assert!(
            state
                .error
                .as_deref()
                .unwrap()
                .contains("runner exited before the game started")
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn runner_cancel_before_game_process_returns_to_idle() {
        let base = test_dir("runner-cancel");
        let ready = base.join("ready");
        let manager = GameLaunchManager::new();
        let token = uuid::Uuid::new_v4().to_string();
        let target = runner_test_target(&base, &token);
        let cancel = manager
            .reserve_launch("runner", None, target.clone())
            .unwrap();
        let worker_manager = manager.clone();
        let ready_path = ready.to_string_lossy().into_owned();
        let worker = thread::spawn(move || {
            worker_manager.run_launch("runner".to_owned(), target, None, None, cancel, None, move |_| {
                std::process::Command::new("/bin/bash")
                    .args([
                        "-c",
                        "touch \"$1\"; exec sleep 60",
                        "legio-stub",
                        &ready_path,
                    ])
                    .env(game_process::LAUNCH_TOKEN_ENV, token)
                    .spawn()
                    .map(Some)
                    .map_err(|error| error.to_string())
            });
        });
        let started = Instant::now();
        while !ready.exists() {
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "runner stub did not start"
            );
            thread::sleep(Duration::from_millis(10));
        }
        manager.cancel("runner").unwrap();
        worker.join().unwrap();
        assert_eq!(manager.list().unwrap()[0].status, GameStatus::Idle);
        assert_eq!(manager.list().unwrap()[0].error, None);
        fs::remove_dir_all(base).unwrap();
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
        let manager = GameLaunchManager::new();
        let cancel = Arc::new(AtomicBool::new(false));
        manager.entries.lock().unwrap().insert(
            "controlled".to_owned(),
            Entry {
                app_id: Some(APP_ID),
                process_target: game_process::ProcessTarget::Steam {
                    app_id: APP_ID,
                    install_path,
                },
                status: GameStatus::Launching,
                error: None,
                cancel: Arc::clone(&cancel),
            },
        );

        let worker_manager = manager.clone();
        let worker_install_path = base.join("game");
        let worker = thread::spawn(move || {
            worker_manager.run_launch(
                "controlled".to_owned(),
                game_process::ProcessTarget::Steam {
                    app_id: APP_ID,
                    install_path: worker_install_path,
                },
                Some(APP_ID),
                None,
                cancel,
                None,
                move |_| {
                    let child = std::process::Command::new("sleep")
                        .arg("30")
                        .env("SteamAppId", APP_ID.to_string())
                        .spawn()
                        .map_err(|_| "Could not start controlled test executable".to_owned())?;
                    Ok(Some(child))
                },
            );
        });

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
        let manager = GameLaunchManager::new();
        let cancel = Arc::new(AtomicBool::new(false));
        manager.entries.lock().unwrap().insert(
            "missing".to_owned(),
            Entry {
                app_id: Some(42),
                process_target: game_process::ProcessTarget::Steam {
                    app_id: 42,
                    install_path,
                },
                status: GameStatus::Launching,
                error: None,
                cancel: Arc::clone(&cancel),
            },
        );

        let missing_executable = base.join("missing-game");
        manager.run_launch(
            "missing".to_owned(),
            game_process::ProcessTarget::Steam {
                app_id: 42,
                install_path: base.join("game"),
            },
            Some(42),
            None,
            cancel,
            None,
            move |_| {
                std::process::Command::new(missing_executable)
                    .spawn()
                    .map(Some)
                    .map_err(|_| "Executable was not found".to_owned())
            },
        );

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
