use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use crate::compatibility_logs::{CompatibilityLog, CompatibilityLogState};
use crate::database::AppliedCompatibilityOptions;
#[cfg(target_os = "linux")]
use crate::database::EffectiveCompatibilityConfig;
use crate::database::{Database, DatabaseState};
#[cfg(target_os = "linux")]
use crate::runner_discovery::RunnerKind;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

#[cfg(target_os = "linux")]
use crate::steam_process;
use crate::{game_process, runner_discovery, steam_local, steam_switch};

const START_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(target_os = "linux")]
const STEAM_START_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(target_os = "linux")]
const POLL_INTERVAL: Duration = Duration::from_millis(500);
#[cfg(windows)]
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const EXIT_GRACE: Duration = Duration::from_secs(3);
const SESSION_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const LOG_READ_LIMIT: u64 = 128 * 1024;
const LOG_LINE_LIMIT: usize = 8 * 1024;

#[cfg(target_os = "linux")]
fn ensure_steam_for_launch(
    launch_via_steam: bool,
    online_fix: bool,
    steam_root: Option<&Path>,
    ensure_running: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    if !launch_via_steam && !online_fix {
        return Ok(());
    }
    let steam_root = steam_root
        .ok_or_else(|| "Steam launch and Online Fix require a Steam installation".to_owned())?;
    ensure_running(steam_root)
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility_options: Option<AppliedCompatibilityOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility_log_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility_log_error: Option<String>,
    pub compatibility_log_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_exit_code: Option<i32>,
}

struct Entry {
    app_id: Option<u32>,
    process_target: game_process::ProcessTarget,
    status: GameStatus,
    error: Option<String>,
    compatibility_options: Option<AppliedCompatibilityOptions>,
    #[cfg(target_os = "linux")]
    compatibility_log: Option<CompatibilityLogState>,
    runner_exit_code: Option<i32>,
    cancel: Arc<AtomicBool>,
    prefix: Option<PathBuf>,
}

struct LaunchTarget {
    install_path: PathBuf,
    steam_root: PathBuf,
}

struct SessionTracking {
    database: Arc<Database>,
    game_id: String,
    last_heartbeat: Instant,
}

#[derive(Default)]
struct LaunchContext {
    steam_app_id: Option<u32>,
    steam_log_path: Option<PathBuf>,
    session_database: Option<Arc<Database>>,
    #[cfg(target_os = "linux")]
    compatibility_log: Option<CompatibilityLog>,
}

impl SessionTracking {
    fn start(database: Arc<Database>, game_id: &str) -> Result<Self, String> {
        database.start_game_session(game_id, crate::database::now_milliseconds()?)?;
        Ok(Self {
            database,
            game_id: game_id.to_owned(),
            last_heartbeat: Instant::now(),
        })
    }

    fn heartbeat_if_due(&mut self) -> Result<(), String> {
        if self.last_heartbeat.elapsed() < SESSION_HEARTBEAT_INTERVAL {
            return Ok(());
        }
        self.database
            .heartbeat_game_session(&self.game_id, crate::database::now_milliseconds()?)?;
        self.last_heartbeat = Instant::now();
        Ok(())
    }

    fn finish(self) -> Result<(), String> {
        self.database
            .end_game_session(&self.game_id, crate::database::now_milliseconds()?)
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

#[derive(Clone)]
pub(crate) struct GameLaunchManager {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    operations: Arc<Mutex<()>>,
    changes: tokio::sync::watch::Sender<()>,
    #[cfg(target_os = "linux")]
    compatibility_log_root: Option<Result<PathBuf, String>>,
}

impl Default for GameLaunchManager {
    fn default() -> Self {
        Self {
            entries: Arc::default(),
            operations: Arc::default(),
            changes: tokio::sync::watch::channel(()).0,
            #[cfg(target_os = "linux")]
            compatibility_log_root: None,
        }
    }
}

impl GameLaunchManager {
    #[cfg(any(not(target_os = "linux"), test))]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn with_log_directory(directory: Result<PathBuf, String>) -> Self {
        Self {
            compatibility_log_root: Some(directory),
            ..Self::default()
        }
    }

    pub(crate) fn subscribe_changes(&self) -> tokio::sync::watch::Receiver<()> {
        self.changes.subscribe()
    }

    pub(crate) fn operation(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.operations
            .lock()
            .map_err(|_| "Game operation lock was poisoned".to_owned())
    }

    pub(crate) fn require_idle(&self, game_id: &str) -> Result<(), String> {
        if self
            .lock()?
            .get(game_id)
            .is_some_and(|entry| entry.status != GameStatus::Idle)
        {
            return Err("Stop the game before changing its installation".to_owned());
        }
        Ok(())
    }

    pub(crate) fn list(&self) -> Result<Vec<GameLaunchState>, String> {
        let entries = self.lock()?;
        Ok(entries
            .iter()
            .map(|(game_id, entry)| {
                #[cfg(target_os = "linux")]
                let (compatibility_log_path, compatibility_log_error, compatibility_log_truncated) =
                    entry
                        .compatibility_log
                        .as_ref()
                        .map_or((None, None, false), |log| {
                            (
                                Some(log.directory().to_path_buf()),
                                log.output_error(),
                                log.truncated(),
                            )
                        });
                #[cfg(not(target_os = "linux"))]
                let (compatibility_log_path, compatibility_log_error, compatibility_log_truncated) =
                    (None, None, false);
                GameLaunchState {
                    game_id: game_id.clone(),
                    status: entry.status,
                    error: entry.error.clone(),
                    compatibility_options: entry.compatibility_options.clone(),
                    compatibility_log_path,
                    compatibility_log_error,
                    compatibility_log_truncated,
                    runner_exit_code: entry.runner_exit_code,
                }
            })
            .collect())
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn compatibility_logs_directory(&self) -> Result<PathBuf, String> {
        let directory = self
            .compatibility_log_root
            .as_ref()
            .ok_or_else(|| "Compatibility log directory is unavailable".to_owned())?
            .as_ref()
            .map_err(|error| format!("Could not resolve application log directory: {error}"))?;
        crate::compatibility_logs::directory(directory)
    }

    #[cfg(not(target_os = "linux"))]
    pub(crate) fn compatibility_logs_directory(&self) -> Result<PathBuf, String> {
        Err("Compatibility logs are supported on Linux only".to_owned())
    }

    pub(crate) fn launch(
        &self,
        app: AppHandle,
        game_id: String,
        confirm_account_switch: bool,
    ) -> Result<steam_switch::SteamLaunchResult, String> {
        let _operation = self.operation()?;
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
        let cancel = self.reserve_launch(&game_id, Some(app_id), process_target.clone(), None)?;

        let worker_id = game_id.clone();
        let manager = self.clone();
        let launch_game_id = worker_id.clone();
        let session_database = app.state::<DatabaseState>().shared_database()?;
        let steam_log_path = target.steam_root.join("logs/console_log.txt");
        if let Err(error) = thread::Builder::new()
            .name(format!("legio-game-{app_id}"))
            .spawn(move || {
                manager.run_launch(
                    worker_id,
                    process_target,
                    cancel,
                    LaunchContext {
                        steam_app_id: Some(app_id),
                        steam_log_path: Some(steam_log_path),
                        session_database: Some(session_database),
                        #[cfg(target_os = "linux")]
                        compatibility_log: None,
                    },
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

    #[cfg(windows)]
    pub(crate) fn launch_native<R: Runtime>(
        &self,
        app: AppHandle<R>,
        game_id: String,
    ) -> Result<(), String> {
        let _operation = self.operation()?;
        let database = app.state::<DatabaseState>().shared_database()?;
        let game = database.game(&game_id)?;
        if game.steam_install_path.is_some() {
            return Err("Steam-managed games must launch through Steam".to_owned());
        }
        let executable = game
            .executable_path
            .as_deref()
            .ok_or_else(|| "This game has no selected executable".to_owned())?;
        let executable = fs::canonicalize(executable)
            .map_err(|error| format!("Could not inspect game executable: {error}"))?;
        if !executable.is_file()
            || !executable
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        {
            return Err("Selected file is not a Windows executable".to_owned());
        }
        let game_directory = executable
            .parent()
            .ok_or_else(|| "Game executable has no parent directory".to_owned())?;
        let config = database.native_launch_config(&game_id)?;
        validate_launch_arguments(&config.arguments)?;
        let working_directory =
            game_working_directory(config.working_directory.as_deref(), game_directory)?;
        let mut command = Command::new(&executable);
        command
            .args(&config.arguments)
            .envs(&config.environment)
            .current_dir(working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let process_target = game_process::ProcessTarget::Native {
            game_directory: game_directory.to_path_buf(),
            started_after_ms: crate::database::now_milliseconds()?,
            launcher_pid: None,
            known_pids: Arc::default(),
        };
        let cancel = self.reserve_launch(&game_id, None, process_target.clone(), None)?;
        let manager = self.clone();
        let worker_id = game_id.clone();
        if let Err(error) = thread::Builder::new()
            .name(format!("legio-native-{game_id}"))
            .spawn(move || {
                manager.run_launch(
                    worker_id,
                    process_target,
                    cancel,
                    LaunchContext {
                        session_database: Some(database),
                        ..LaunchContext::default()
                    },
                    move |_| {
                        command
                            .spawn()
                            .map(Some)
                            .map_err(|error| format!("Could not start native game: {error}"))
                    },
                );
            })
        {
            self.set_state(&game_id, GameStatus::Idle, Some(error.to_string()));
            return Err(format!("Could not start game launch task: {error}"));
        }
        Ok(())
    }

    #[cfg(not(windows))]
    pub(crate) fn launch_native<R: Runtime>(
        &self,
        _app: AppHandle<R>,
        _game_id: String,
    ) -> Result<(), String> {
        Err("Native executable launch is supported on Windows only".to_owned())
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
        self.launch_with_compatibility_config(
            app,
            game_id,
            config,
            runner_discovery::resolve_runner,
        )
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn launch_configured(&self, app: AppHandle, game_id: String) -> Result<(), String> {
        let config = app
            .state::<DatabaseState>()
            .database()?
            .effective_compatibility_config(&game_id)?;
        if config
            .runner_path
            .as_deref()
            .is_some_and(|path| !path.is_empty())
        {
            self.launch_with_compatibility_config(
                app,
                game_id,
                config,
                runner_discovery::resolve_runner,
            )
        } else {
            let runner = runner_discovery::discover()
                .runners
                .into_iter()
                .next()
                .ok_or_else(|| "No compatible Proton or Wine runner is installed".to_owned())?;
            let config = EffectiveCompatibilityConfig {
                runner_path: Some(runner.path.clone()),
                ..config
            };
            self.launch_with_compatibility_config(app, game_id, config, move |_| Ok(runner))
        }
    }

    #[cfg(target_os = "linux")]
    fn launch_with_compatibility_config<R: Runtime>(
        &self,
        app: AppHandle<R>,
        game_id: String,
        mut config: EffectiveCompatibilityConfig,
        resolve_runner: impl FnOnce(&str) -> Result<runner_discovery::InstalledRunner, String>,
    ) -> Result<(), String> {
        let _operation = self.operation()?;
        let database = app.state::<DatabaseState>().shared_database()?;
        let game = database.game(&game_id)?;
        if game.steam_install_path.is_some() {
            return Err("Compatibility runners can only launch manually imported games".to_owned());
        }
        let overrides = database.game_compatibility_overrides(&game_id)?;
        let launch_via_steam = overrides
            .launch_via_steam
            .unwrap_or(game.steam_app_id.is_some());
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
        let detected_online_fix = if overrides.online_fix.is_some() {
            false
        } else {
            crate::online_fix::detected(&database, &game)?
        };
        config.online_fix = crate::online_fix::enabled(
            game.steam_install_path.is_some(),
            overrides.online_fix,
            detected_online_fix,
        );
        let runner_path = config
            .runner_path
            .as_deref()
            .filter(|path| !path.is_empty())
            .ok_or_else(|| "No compatibility runner is selected".to_owned())?;
        let runner = resolve_runner(runner_path)?;
        if launch_via_steam && !matches!(runner.kind, RunnerKind::Proton | RunnerKind::GeProton) {
            return Err("Launch via Steam requires a Proton runner".to_owned());
        }
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
        let wine_prefix = game_wine_prefix(&compat_data_path, runner.kind)?;
        let umu = if runner.kind != RunnerKind::Wine {
            Some(runner_discovery::umu_path()?)
        } else {
            None
        };
        let steam_root = if config.online_fix || launch_via_steam {
            Some(steam_client_root()?)
        } else {
            None
        };
        let options = crate::compatibility_options::PreparedOptions::prepare(
            &config,
            &runner,
            steam_root.as_deref(),
            launch_via_steam,
        )?;
        let command = runner_discovery::launch_command(
            &runner,
            &executable_path,
            &config.arguments_before,
            &config.arguments_after,
            umu.as_deref(),
        )?;
        let mut command = crate::linux_performance::wrap(command, &config.linux_performance)?;
        command.current_dir(working_directory).stdin(Stdio::null());
        options.apply(&mut command, &config)?;
        if !config.environment.contains_key("WINEDEBUG") && !config.debug_logging {
            command.env("WINEDEBUG", "-all");
        }
        apply_dll_overrides(&mut command, &config);
        command
            .env("WINEPREFIX", &wine_prefix)
            .env_remove("STEAM_COMPAT_DATA_PATH");
        if let Some(steam_root) = steam_root.as_ref() {
            command.env("STEAM_COMPAT_CLIENT_INSTALL_PATH", steam_root);
        } else {
            command.env_remove("STEAM_COMPAT_CLIENT_INSTALL_PATH");
        }
        if runner.kind != RunnerKind::Wine
            && !config.environment.contains_key("VKD3D_SHADER_CACHE_PATH")
            && std::env::var_os("VKD3D_SHADER_CACHE_PATH").is_none()
        {
            let shader_cache = app
                .path()
                .app_cache_dir()
                .map_err(|error| format!("Could not resolve shader cache directory: {error}"))?
                .join("compatibility-shaders")
                .join(&game.id);
            fs::create_dir_all(&shader_cache)
                .map_err(|error| format!("Could not create game shader cache: {error}"))?;
            command.env("VKD3D_SHADER_CACHE_PATH", shader_cache);
        }

        let token = uuid::Uuid::new_v4().to_string();
        command.env(game_process::LAUNCH_TOKEN_ENV, &token);
        let process_target = game_process::ProcessTarget::Runner {
            token,
            executable_path,
            launcher_pid: None,
        };
        let applied_options = crate::compatibility_options::PreparedOptions::diagnostic(
            &runner,
            &config,
            launch_via_steam,
        );
        let cancel = self.reserve_launch_with_prefix(
            &game_id,
            None,
            process_target.clone(),
            Some(applied_options.clone()),
            Some(&wine_prefix),
        )?;
        let mut compatibility_log = if config.debug_logging {
            let result = self
                .compatibility_logs_directory()
                .and_then(|log_directory| {
                    CompatibilityLog::create(
                        &log_directory,
                        &game.id,
                        &compat_data_path,
                        &runner,
                        &config,
                        &applied_options,
                    )
                });
            match result {
                Ok(log) => Some(log),
                Err(error) => {
                    self.set_state(&game_id, GameStatus::Idle, Some(error.clone()));
                    return Err(error);
                }
            }
        } else {
            None
        };
        if let Some(log) = compatibility_log.as_mut() {
            crate::compatibility_options::PreparedOptions::apply_debug_logging(
                &mut command,
                &config,
                &runner,
                log.state().directory(),
            );
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
            if let Err(error) = self.set_compatibility_log_state(&game_id, Some(log.state())) {
                self.set_state(&game_id, GameStatus::Idle, Some(error.clone()));
                return Err(error);
            }
        } else {
            command.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let manager = self.clone();
        let worker_id = game_id.clone();
        let session_database = database;
        if let Err(error) = thread::Builder::new()
            .name(format!("legio-game-runner-{}", game.id))
            .spawn(move || {
                manager.run_launch(
                    worker_id,
                    process_target,
                    cancel,
                    LaunchContext {
                        session_database: Some(session_database),
                        compatibility_log,
                        ..LaunchContext::default()
                    },
                    move |cancel| {
                        ensure_steam_for_launch(
                            launch_via_steam,
                            config.online_fix,
                            steam_root.as_deref(),
                            |root| steam_process::ensure_running(root, STEAM_START_TIMEOUT, cancel),
                        )?;
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
        cancel: Arc<AtomicBool>,
        context: LaunchContext,
        launch: impl FnOnce(&AtomicBool) -> Result<Option<Child>, String>,
    ) {
        let LaunchContext {
            steam_app_id,
            steam_log_path,
            session_database,
            #[cfg(target_os = "linux")]
            compatibility_log,
        } = context;
        #[cfg(target_os = "linux")]
        let mut compatibility_log = compatibility_log;
        let mut steam_log = steam_log_path.map(SteamLaunchLog::new);
        if cancel.load(Ordering::Acquire) {
            self.set_state(&game_id, GameStatus::Idle, None);
            return;
        }
        let mut child = match launch(&cancel) {
            Ok(child) => child,
            Err(error) => {
                #[cfg(target_os = "linux")]
                if let Some(log) = compatibility_log.as_mut() {
                    log.record_launch_error(&error);
                }
                let error = (!cancel.load(Ordering::Acquire) || error != "Launch cancelled")
                    .then(|| format!("Launch stage failed: {error}"));
                self.set_state(&game_id, GameStatus::Idle, error);
                return;
            }
        };
        #[cfg(target_os = "linux")]
        if let Some(log) = compatibility_log.as_mut() {
            let capture = child
                .as_mut()
                .ok_or_else(|| "Compatibility runner did not return a process".to_owned())
                .and_then(|child| log.capture_output(child));
            if let Err(error) = capture {
                let cleanup_error = terminate_child(&mut child).err();
                self.set_state(
                    &game_id,
                    GameStatus::Idle,
                    Some(append_cleanup_error(
                        format!("Diagnostics stage failed: {error}"),
                        cleanup_error,
                    )),
                );
                return;
            }
        }
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
        #[cfg(windows)]
        if let game_process::ProcessTarget::Native { launcher_pid, .. } = &mut process_target
            && let Some(pid) = child.as_ref().map(Child::id)
        {
            *launcher_pid = Some(pid);
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
        #[cfg(windows)]
        let native_launch = matches!(process_target, game_process::ProcessTarget::Native { .. });
        #[cfg(not(windows))]
        let native_launch = false;
        let mut native_helper_exited_at = None;
        let mut steam_progress = SteamLaunchProgress::default();
        let mut process_monitor = game_process::ProcessMonitor::new();
        let mut cancelled_without_process_since = None;
        loop {
            #[cfg(windows)]
            // Process discovery can lag behind a live native child.
            if native_launch
                && !cancel.load(Ordering::Acquire)
                && child
                    .as_mut()
                    .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
            {
                break;
            }
            match process_monitor.matching_pids(&process_target) {
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
            #[cfg(target_os = "linux")]
            if let Some(log) = compatibility_log.as_ref() {
                log.cap_runner_log();
            }
            if let Some(runner_child) = child.as_mut() {
                match runner_child.try_wait() {
                    Ok(Some(status)) => {
                        #[cfg(target_os = "linux")]
                        self.record_runner_exit(
                            &game_id,
                            status.code(),
                            compatibility_log.as_mut(),
                        );
                        child.take();
                        if !status.success() && !cancel.load(Ordering::Acquire) {
                            let kind = if native_launch {
                                "native launcher"
                            } else {
                                "runner"
                            };
                            self.set_state(
                                &game_id,
                                GameStatus::Idle,
                                Some(format!("Launch stage failed: {kind} exited before the game started ({status})")),
                            );
                            return;
                        }
                        if native_launch {
                            native_helper_exited_at = Some(Instant::now());
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
            if native_helper_exited_at.is_some_and(|exited: Instant| exited.elapsed() >= EXIT_GRACE)
            {
                self.set_state(
                    &game_id,
                    GameStatus::Idle,
                    Some("Launch stage failed: native launcher exited without starting a detectable game process".to_owned()),
                );
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
        let mut session = session_database.and_then(|database| {
            match SessionTracking::start(database, &game_id) {
                Ok(session) => Some(session),
                Err(error) => {
                    session_error = Some(format!("Session tracking failed: {error}"));
                    None
                }
            }
        });
        self.set_state(&game_id, GameStatus::Running, session_error);
        let mut missing_since = None;
        loop {
            let heartbeat_error = session
                .as_mut()
                .and_then(|session| session.heartbeat_if_due().err());
            if let Some(error) = heartbeat_error {
                let end_error = finish_session(&mut session);
                self.set_state(
                    &game_id,
                    GameStatus::Running,
                    Some(append_cleanup_error(
                        format!("Session tracking failed: {error}"),
                        end_error,
                    )),
                );
            }
            match process_monitor.matching_pids(&process_target) {
                Ok(pids) if pids.is_empty() => {
                    let since = missing_since.get_or_insert_with(Instant::now);
                    if since.elapsed() >= EXIT_GRACE {
                        let cleanup_error = terminate_child(&mut child).err();
                        let error =
                            cleanup_error.map(|error| format!("Stop stage failed: {error}"));
                        let error = combine_errors(error, finish_session(&mut session));
                        self.set_state(&game_id, GameStatus::Idle, error);
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
                    let error = append_cleanup_error(error, finish_session(&mut session));
                    self.set_state(&game_id, GameStatus::Idle, Some(error));
                    return;
                }
            }
            #[cfg(target_os = "linux")]
            if let Some(runner_child) = child.as_mut() {
                match runner_child.try_wait() {
                    Ok(Some(status)) => {
                        self.record_runner_exit(
                            &game_id,
                            status.code(),
                            compatibility_log.as_mut(),
                        );
                        child.take();
                    }
                    Ok(None) => {}
                    Err(error) => {
                        child.take();
                        self.set_state(
                            &game_id,
                            GameStatus::Running,
                            Some(format!(
                                "Monitor stage failed: could not wait for compatibility runner: {error}"
                            )),
                        );
                    }
                }
            }
            #[cfg(target_os = "linux")]
            if let Some(log) = compatibility_log.as_ref() {
                log.cap_runner_log();
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn reserve_launch(
        &self,
        game_id: &str,
        app_id: Option<u32>,
        process_target: game_process::ProcessTarget,
        compatibility_options: Option<AppliedCompatibilityOptions>,
    ) -> Result<Arc<AtomicBool>, String> {
        self.reserve_launch_with_prefix(
            game_id,
            app_id,
            process_target,
            compatibility_options,
            None,
        )
    }

    fn reserve_launch_with_prefix(
        &self,
        game_id: &str,
        app_id: Option<u32>,
        process_target: game_process::ProcessTarget,
        compatibility_options: Option<AppliedCompatibilityOptions>,
        prefix: Option<&Path>,
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
        if prefix.is_some_and(|prefix| {
            entries.values().any(|entry| {
                entry.prefix.as_deref() == Some(prefix) && entry.status != GameStatus::Idle
            })
        }) {
            return Err(
                "This compatibility prefix is already used by a launching or running game"
                    .to_owned(),
            );
        }
        entries.insert(
            game_id.to_owned(),
            Entry {
                app_id,
                process_target,
                status: GameStatus::Launching,
                error: None,
                compatibility_options,
                #[cfg(target_os = "linux")]
                compatibility_log: None,
                runner_exit_code: None,
                cancel: Arc::clone(&cancel),
                prefix: prefix.map(Path::to_path_buf),
            },
        );
        self.changes.send_replace(());
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

    #[cfg(target_os = "linux")]
    fn set_compatibility_log_state(
        &self,
        game_id: &str,
        log: Option<CompatibilityLogState>,
    ) -> Result<(), String> {
        let mut entries = self.lock()?;
        let entry = entries
            .get_mut(game_id)
            .ok_or_else(|| "Game launch state disappeared".to_owned())?;
        entry.compatibility_log = log;
        Ok(())
    }

    fn set_runner_exit_code(&self, game_id: &str, code: i32) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(entry) = entries.get_mut(game_id)
            && entry.compatibility_options.is_some()
        {
            entry.runner_exit_code = Some(code);
        }
    }

    #[cfg(target_os = "linux")]
    fn record_runner_exit(
        &self,
        game_id: &str,
        code: Option<i32>,
        log: Option<&mut CompatibilityLog>,
    ) {
        if let Some(code) = code {
            self.set_runner_exit_code(game_id, code);
            if let Some(log) = log {
                log.record_exit_code(code);
            }
        }
    }

    fn set_state(&self, game_id: &str, status: GameStatus, error: Option<String>) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(entry) = entries.get_mut(game_id) {
            entry.status = status;
            entry.error = error;
            self.changes.send_replace(());
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, Entry>>, String> {
        self.entries
            .lock()
            .map_err(|_| "Game launch state is unavailable after an earlier task failed".to_owned())
    }
}

pub(crate) fn remove_game(app: &AppHandle, id: &str) -> Result<(), String> {
    let manager = app.state::<crate::game_lifecycle::GameLaunchManager>();
    let _operation = manager.operation()?;
    manager.require_idle(id)?;
    app.state::<DatabaseState>().database()?.remove_game(id)?;
    let mut errors = Vec::new();
    if let Err(error) = app
        .state::<crate::game_artwork::GameArtworkStore>()
        .remove_for_game(id)
    {
        errors.push(format!("custom artwork: {error}"));
    }
    if let Err(error) = crate::desktop_shortcuts::remove(id) {
        errors.push(format!("desktop shortcuts: {error}"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Game was removed, but cleanup failed: {}",
            errors.join("; ")
        ))
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
fn game_wine_prefix(compatdata: &Path, kind: RunnerKind) -> Result<PathBuf, String> {
    let nested = compatdata.join("pfx");
    let root_is_prefix =
        compatdata.join("system.reg").is_file() || compatdata.join("drive_c").is_dir();
    if root_is_prefix && nested.exists() {
        let nested_prefix = fs::canonicalize(&nested)
            .map_err(|error| format!("Could not inspect existing Wine prefix: {error}"))?;
        if nested_prefix != compatdata
            && (kind != RunnerKind::Wine
                || nested_prefix.join("system.reg").is_file()
                || nested_prefix.join("drive_c").is_dir())
        {
            return Err("Existing Wine prefix has a separate pfx directory. Resolve the conflicting layouts before launching".to_owned());
        }
    }
    if root_is_prefix {
        return Ok(compatdata.to_path_buf());
    }
    if nested.exists() || kind != RunnerKind::Wine {
        fs::create_dir_all(&nested)
            .map_err(|error| format!("Could not create Wine prefix: {error}"))?;
        let prefix = fs::canonicalize(nested)
            .map_err(|error| format!("Could not resolve Wine prefix: {error}"))?;
        if !prefix.starts_with(compatdata) {
            return Err("Wine prefix escapes its compatibility directory".to_owned());
        }
        return Ok(prefix);
    }
    Ok(compatdata.to_path_buf())
}

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
            "PROTONPATH",
            "SteamAppId",
            "SteamGameId",
            "STEAM_COMPAT_APP_ID",
            "UMU_ID",
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
fn apply_dll_overrides(command: &mut Command, config: &EffectiveCompatibilityConfig) {
    let overrides = if config.online_fix {
        baseline_dll_overrides(&config.dll_overrides)
    } else {
        config.dll_overrides.clone()
    };
    if !overrides.is_empty() {
        command.env("WINEDLLOVERRIDES", format_dll_overrides(&overrides));
    }
}

#[cfg(target_os = "linux")]
const BASELINE_DLL_OVERRIDES: &[(&str, &str)] = &[
    ("OnlineFix64", "n"),
    ("SteamOverlay64", "n"),
    ("winmm", "n,b"),
    ("dnet", "n"),
    ("steam_api64", "n"),
    ("winhttp", "n,b"),
];

#[cfg(target_os = "linux")]
fn baseline_dll_overrides(
    configured: &std::collections::BTreeMap<String, String>,
) -> std::collections::BTreeMap<String, String> {
    let mut merged: std::collections::BTreeMap<String, String> = BASELINE_DLL_OVERRIDES
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    merged.extend(
        configured
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    merged
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

fn combine_errors(primary: Option<String>, secondary: Option<String>) -> Option<String> {
    match primary {
        Some(error) => Some(append_cleanup_error(error, secondary)),
        None => secondary,
    }
}

fn finish_session(session: &mut Option<SessionTracking>) -> Option<String> {
    session
        .take()
        .and_then(|session| session.finish().err())
        .map(|error| format!("Session tracking failed: {error}"))
}

struct SteamLaunchLog {
    path: PathBuf,
    offset: u64,
    pending: Vec<u8>,
    cursor: usize,
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
            cursor: 0,
        }
    }

    fn launch_event(&mut self, app_id: u32) -> Option<SteamLaunchEvent> {
        let mut file = File::open(&self.path).ok()?;
        let len = file.metadata().ok()?.len();
        if len < self.offset {
            self.offset = 0;
            self.pending.clear();
            self.cursor = 0;
        }
        if let Some(event) = self.pending_event(app_id) {
            return Some(event);
        }
        if self.cursor > 0 {
            self.pending.drain(..self.cursor);
            self.cursor = 0;
        }
        file.seek(SeekFrom::Start(self.offset)).ok()?;
        let mut chunk = Vec::new();
        file.take(LOG_READ_LIMIT).read_to_end(&mut chunk).ok()?;
        self.offset += chunk.len() as u64;
        self.pending.extend_from_slice(&chunk);

        self.pending_event(app_id)
    }

    fn pending_event(&mut self, app_id: u32) -> Option<SteamLaunchEvent> {
        while let Some(end) = self.pending[self.cursor..]
            .iter()
            .position(|byte| *byte == b'\n')
        {
            let end = self.cursor + end + 1;
            let event = std::str::from_utf8(&self.pending[self.cursor..end])
                .ok()
                .and_then(|line| parse_launch_event(line, app_id));
            self.cursor = end;
            if event.is_some() {
                return event;
            }
        }
        if self.pending.len() - self.cursor > LOG_LINE_LIMIT {
            self.pending.clear();
            self.cursor = 0;
        }
        None
    }
}

fn parse_launch_event(line: &str, app_id: u32) -> Option<SteamLaunchEvent> {
    for (marker, event) in [
        (
            "Game process added : AppID ",
            SteamLaunchEvent::ProcessAdded,
        ),
        (
            "Game process updated : AppID ",
            SteamLaunchEvent::ProcessUpdated,
        ),
        (
            "Game process removed: AppID ",
            SteamLaunchEvent::ProcessRemoved,
        ),
    ] {
        if line
            .split_once(marker)
            .and_then(|(_, detail)| detail.split_once(' '))
            .is_some_and(|(id, _)| id.parse::<u32>() == Ok(app_id))
        {
            return Some(event);
        }
    }
    let action = line.split_once("GameAction [AppID ")?.1;
    let (id, action) = action.split_once(", ActionID ")?;
    if id.parse::<u32>() != Ok(app_id) {
        return None;
    }
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
    use tauri::Manager;

    #[cfg(target_os = "linux")]
    #[test]
    fn wine_and_proton_reuse_the_actual_prefix_without_moving_saves() {
        let root = test_dir("prefix-layout");
        fs::create_dir_all(root.join("pfx/drive_c")).unwrap();
        let saved = root.join("pfx/drive_c/save.dat");
        fs::write(&saved, b"save").unwrap();
        let expected = fs::canonicalize(root.join("pfx")).unwrap();
        assert_eq!(game_wine_prefix(&root, RunnerKind::Wine).unwrap(), expected);
        assert_eq!(
            game_wine_prefix(&root, RunnerKind::GeProton).unwrap(),
            expected
        );
        assert_eq!(fs::read(saved).unwrap(), b"save");
        let direct = test_dir("wine-prefix-layout");
        fs::create_dir_all(direct.join("drive_c")).unwrap();
        assert_eq!(
            game_wine_prefix(&direct, RunnerKind::Proton).unwrap(),
            direct
        );
        assert!(!direct.join("pfx").exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(direct).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn prefix_layout_rejects_separate_empty_pfx_and_escaping_links_but_accepts_umu_alias() {
        let root = test_dir("prefix-conflict");
        fs::create_dir_all(root.join("drive_c")).unwrap();
        fs::write(root.join("drive_c/save.dat"), b"saved").unwrap();
        fs::create_dir(root.join("pfx")).unwrap();
        assert!(
            game_wine_prefix(&root, RunnerKind::GeProton)
                .unwrap_err()
                .contains("separate pfx")
        );
        assert_eq!(game_wine_prefix(&root, RunnerKind::Wine).unwrap(), root);
        assert_eq!(fs::read(root.join("drive_c/save.dat")).unwrap(), b"saved");
        fs::remove_dir(root.join("pfx")).unwrap();
        std::os::unix::fs::symlink(&root, root.join("pfx")).unwrap();
        assert_eq!(game_wine_prefix(&root, RunnerKind::Proton).unwrap(), root);
        fs::remove_file(root.join("pfx")).unwrap();
        let compatdata = root.join("other-game");
        fs::create_dir(&compatdata).unwrap();
        std::os::unix::fs::symlink(&root, compatdata.join("pfx")).unwrap();
        assert!(
            game_wine_prefix(&compatdata, RunnerKind::Wine)
                .unwrap_err()
                .contains("escapes")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn prefix_reservation_prevents_concurrent_games_and_releases_when_idle() {
        let manager = GameLaunchManager::new();
        let prefix = Path::new("/test/shared-prefix");
        let target = game_process::ProcessTarget::Runner {
            token: "test".to_owned(),
            executable_path: PathBuf::from("/test/game.exe"),
            launcher_pid: None,
        };
        manager
            .reserve_launch_with_prefix("one", None, target.clone(), None, Some(prefix))
            .unwrap();
        assert!(
            manager
                .reserve_launch_with_prefix("two", None, target.clone(), None, Some(prefix))
                .is_err()
        );
        manager.set_state("one", GameStatus::Running, None);
        assert!(
            manager
                .reserve_launch_with_prefix("two", None, target.clone(), None, Some(prefix))
                .is_err()
        );
        manager.set_state("one", GameStatus::Idle, None);
        manager
            .reserve_launch_with_prefix("two", None, target, None, Some(prefix))
            .unwrap();
    }

    #[tokio::test]
    async fn deferred_extraction_resumes_on_game_exit_or_setting_change() {
        let root = test_dir("extraction-scheduling");
        let queue =
            crate::download_queue::DownloadQueueState::new(Ok(root.clone()), "test").unwrap();
        let manager = GameLaunchManager::new();
        manager
            .reserve_launch(
                "game",
                Some(42),
                game_process::ProcessTarget::Steam {
                    app_id: 42,
                    install_path: root.clone(),
                },
                None,
            )
            .unwrap();
        queue.set_defer_extraction(true);
        assert!(
            tokio::time::timeout(Duration::from_millis(25), queue.wait_for_idle(&manager))
                .await
                .is_err()
        );
        let release = async {
            tokio::task::yield_now().await;
            manager.set_state("game", GameStatus::Idle, None);
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(1), async {
            tokio::join!(queue.wait_for_idle(&manager), release)
        })
        .await
        .unwrap();
        result.unwrap();
        manager.set_state("game", GameStatus::Running, None);
        let disable = async {
            tokio::task::yield_now().await;
            queue.set_defer_extraction(false);
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(1), async {
            tokio::join!(queue.wait_for_idle(&manager), disable)
        })
        .await
        .unwrap();
        result.unwrap();
        assert_eq!(manager.list().unwrap()[0].status, GameStatus::Running);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn installation_changes_require_an_idle_game() {
        let manager = GameLaunchManager::new();
        let target = game_process::ProcessTarget::Steam {
            app_id: 400,
            install_path: PathBuf::from("game"),
        };
        manager.require_idle("game").unwrap();
        manager
            .reserve_launch("game", Some(400), target, None)
            .unwrap();
        assert!(manager.require_idle("game").is_err());
        manager.require_idle("other-game").unwrap();
        manager.set_state("game", GameStatus::Running, None);
        assert!(manager.require_idle("game").is_err());
        manager.set_state("game", GameStatus::Idle, None);
        manager.require_idle("game").unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn steam_starts_for_steam_launch_or_online_fix() {
        let steam_root = Path::new("/steam");
        let mut started = false;

        ensure_steam_for_launch(false, false, None, |_| {
            started = true;
            Ok(())
        })
        .unwrap();
        assert!(!started);

        ensure_steam_for_launch(true, false, Some(steam_root), |root| {
            assert_eq!(root, steam_root);
            started = true;
            Ok(())
        })
        .unwrap();
        assert!(started);

        started = false;
        ensure_steam_for_launch(false, true, Some(steam_root), |_| {
            started = true;
            Ok(())
        })
        .unwrap();
        assert!(started);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn launch_via_steam_requires_a_steam_installation() {
        let error = ensure_steam_for_launch(true, false, None, |_| Ok(())).unwrap_err();
        assert!(error.contains("require a Steam installation"));
        assert!(ensure_steam_for_launch(false, true, None, |_| Ok(())).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn steam_start_failure_is_reported_before_runner_launch() {
        let error = ensure_steam_for_launch(true, false, Some(Path::new("/steam")), |_| {
            Err("Could not start Steam: permission denied".to_owned())
        })
        .unwrap_err();

        assert!(error.contains("Could not start Steam: permission denied"));
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
    fn test_app(database_dir: &Path) -> tauri::App<tauri::test::MockRuntime> {
        test_app_with_logs(database_dir, database_dir)
    }

    #[cfg(target_os = "linux")]
    fn test_app_with_logs(
        database_dir: &Path,
        log_directory: &Path,
    ) -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = format!("org.legio.test.{}", uuid::Uuid::new_v4());
        tauri::test::mock_builder()
            .manage(DatabaseState::new(Ok(database_dir.to_path_buf())))
            .manage(GameLaunchManager::with_log_directory(Ok(
                log_directory.to_path_buf()
            )))
            .build(context)
            .unwrap()
    }

    #[cfg(windows)]
    fn native_test_app(database_dir: &Path) -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = format!("org.legio.test.{}", uuid::Uuid::new_v4());
        tauri::test::mock_builder()
            .manage(DatabaseState::new(Ok(database_dir.to_path_buf())))
            .manage(GameLaunchManager::new())
            .build(context)
            .unwrap()
    }

    #[cfg(target_os = "linux")]
    fn create_manual_game(state: &DatabaseState, executable_path: &Path) -> crate::database::Game {
        let game = state
            .database()
            .unwrap()
            .create_game(crate::database::CreateGameInput {
                name: "Controlled game".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        fs::write(executable_path, b"controlled test executable").unwrap();
        crate::manual_import::set_executable(state, &game.id, executable_path.to_str().unwrap())
            .unwrap();
        game
    }

    #[cfg(target_os = "linux")]
    fn test_runner(path: &Path, script: &str) -> runner_discovery::InstalledRunner {
        use std::os::unix::fs::PermissionsExt;

        fs::write(path, format!("#!/bin/bash\n{script}")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        runner_discovery::InstalledRunner {
            kind: RunnerKind::Wine,
            name: "Controlled Wine".to_owned(),
            version: "test".to_owned(),
            path: fs::canonicalize(path)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        }
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
    #[test]
    fn launch_state_serializes_applied_compatibility_options() {
        let base = test_dir("compatibility-diagnostics");
        let manager = GameLaunchManager::new();
        let target = runner_test_target(&base, "diagnostic-token");
        manager
            .reserve_launch(
                "manual-game",
                None,
                target,
                Some(AppliedCompatibilityOptions {
                    runner: "GE-Proton10-33".to_owned(),
                    version: "GE-Proton10-33".to_owned(),
                    launch_via_steam: true,
                    graphics_renderer: crate::database::GraphicsRenderer::WineD3d,
                    wayland: crate::database::WaylandMode::Native,
                    debug_logging: true,
                }),
            )
            .unwrap();
        let state = manager.list().unwrap().pop().unwrap();
        let serialized = serde_json::to_value(state).unwrap();
        assert_eq!(
            serialized["compatibilityOptions"]["runner"],
            "GE-Proton10-33"
        );
        assert_eq!(serialized["compatibilityOptions"]["launchViaSteam"], true);
        assert_eq!(
            serialized["compatibilityOptions"]["graphicsRenderer"],
            "wine_d3d"
        );
        assert_eq!(serialized["compatibilityOptions"]["wayland"], "native");
        assert_eq!(serialized["compatibilityOptions"]["debugLogging"], true);
        fs::remove_dir_all(base).unwrap();
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

    #[cfg(windows)]
    fn wait_for_native_status(
        manager: &GameLaunchManager,
        game_id: &str,
        status: GameStatus,
    ) -> GameLaunchState {
        let started = Instant::now();
        loop {
            let state = manager
                .list()
                .unwrap()
                .into_iter()
                .find(|entry| entry.game_id == game_id)
                .unwrap();
            if state.status == status {
                return state;
            }
            if status == GameStatus::Running && state.status == GameStatus::Idle {
                panic!("game returned to idle before running: {:?}", state.error);
            }
            assert!(
                started.elapsed() < Duration::from_secs(45),
                "game did not reach {status:?}; last state was {:?}: {:?}",
                state.status,
                state.error
            );
            thread::sleep(Duration::from_millis(100));
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_native_child_waits_for_stop() {
        if std::env::args().any(|arg| arg == "controlled_native_child_waits_for_stop") {
            thread::sleep(Duration::from_secs(30));
        }
    }

    #[cfg(windows)]
    #[test]
    fn native_manager_lifecycle_tracks_a_controlled_process() {
        let base = test_dir("native-manager-lifecycle");
        let database_dir = base.join("database");
        let app = native_test_app(&database_dir);
        let executable = std::env::current_exe().unwrap();
        let (game, database) = {
            let state = app.state::<DatabaseState>();
            let database = state.shared_database().unwrap();
            let game = database
                .create_game(crate::database::CreateGameInput {
                    name: "Controlled native game".to_owned(),
                    steam_app_id: None,
                })
                .unwrap();
            crate::manual_import::set_executable(&state, &game.id, executable.to_str().unwrap())
                .unwrap();
            database
                .save_native_launch_config(
                    &game.id,
                    crate::database::NativeLaunchConfig {
                        arguments: vec!["controlled_native_child_waits_for_stop".to_owned()],
                        working_directory: None,
                        environment: Default::default(),
                    },
                )
                .unwrap();
            (game, database)
        };

        let manager = app.state::<GameLaunchManager>().inner().clone();
        manager
            .launch_native(app.handle().clone(), game.id.clone())
            .unwrap();
        let state = wait_for_native_status(&manager, &game.id, GameStatus::Running);
        assert_eq!(state.error, None);
        let active = database
            .playtime_summaries(crate::database::now_milliseconds().unwrap())
            .unwrap()
            .into_iter()
            .find(|summary| summary.game_id == game.id)
            .unwrap();
        assert_eq!(active.active_sessions, 1);

        manager.stop(&game.id).unwrap();
        let state = wait_for_native_status(&manager, &game.id, GameStatus::Idle);
        assert_eq!(state.error, None);
        let reopened = Database::open(&database_dir).unwrap();
        let summary = reopened
            .playtime_summaries(crate::database::now_milliseconds().unwrap())
            .unwrap()
            .into_iter()
            .find(|summary| summary.game_id == game.id)
            .unwrap();
        assert!(summary.total_milliseconds > 0);
        assert_eq!(summary.active_sessions, 0);
        drop(app);
        drop(database);
        drop(reopened);
        // The mock runtime can retain this SQLite file until process exit on Windows.
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
    fn online_fix_toggle_controls_whether_baseline_overrides_are_exported() {
        fn exported(online_fix: bool, configured: &[(&str, &str)]) -> Option<String> {
            let dll_overrides = configured
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect();
            let mut command = Command::new("true");
            apply_dll_overrides(
                &mut command,
                &EffectiveCompatibilityConfig {
                    online_fix,
                    dll_overrides,
                    ..EffectiveCompatibilityConfig::default()
                },
            );
            command
                .get_envs()
                .find(|(key, _)| *key == "WINEDLLOVERRIDES")
                .and_then(|(_, value)| value.map(|value| value.to_string_lossy().into_owned()))
        }

        assert_eq!(
            exported(true, &[]).as_deref(),
            Some("OnlineFix64=n;SteamOverlay64=n;dnet=n;steam_api64=n;winhttp=n,b;winmm=n,b")
        );
        assert_eq!(exported(false, &[]), None);
        assert_eq!(
            exported(false, &[("d3d11", "n,b")]).as_deref(),
            Some("d3d11=n,b")
        );
        assert_eq!(
            exported(true, &[("winmm", "b")]).as_deref(),
            Some("OnlineFix64=n;SteamOverlay64=n;dnet=n;steam_api64=n;winhttp=n,b;winmm=b")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn baseline_dll_overrides_apply_to_every_launch_and_yield_to_configured_values() {
        let baseline = baseline_dll_overrides(&std::collections::BTreeMap::new());
        assert_eq!(
            format_dll_overrides(&baseline),
            "OnlineFix64=n;SteamOverlay64=n;dnet=n;steam_api64=n;winhttp=n,b;winmm=n,b"
        );
        assert!(validate_dll_overrides(&baseline).is_ok());

        let mut configured = std::collections::BTreeMap::new();
        configured.insert("winmm".to_owned(), "b".to_owned());
        configured.insert("d3d11".to_owned(), "n,b".to_owned());
        let merged = baseline_dll_overrides(&configured);
        assert_eq!(merged.get("winmm").map(String::as_str), Some("b"));
        assert_eq!(merged.get("OnlineFix64").map(String::as_str), Some("n"));
        assert_eq!(
            format_dll_overrides(&merged),
            "OnlineFix64=n;SteamOverlay64=n;d3d11=n,b;dnet=n;steam_api64=n;winhttp=n,b;winmm=b"
        );
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
    fn runner_launch_persists_session_until_stop() {
        let base = test_dir("runner-lifecycle");
        let database_dir = base.join("database");
        let database = Arc::new(Database::open(&database_dir).unwrap());
        let game = database
            .create_game(crate::database::CreateGameInput {
                name: "Controlled game".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
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
            .reserve_launch(&game.id, None, target.clone(), None)
            .unwrap();
        assert_eq!(manager.list().unwrap()[0].status, GameStatus::Launching);
        let worker_manager = manager.clone();
        let game_id = game.id.clone();
        let session_database = Arc::clone(&database);
        let worker = thread::spawn(move || {
            worker_manager.run_launch(
                game_id,
                target,
                cancel,
                LaunchContext {
                    session_database: Some(session_database),
                    ..LaunchContext::default()
                },
                move |_| Ok(Some(runner_stub(&executable_name, &token))),
            );
        });
        wait_for_status(&manager, &game.id, GameStatus::Running);
        thread::sleep(Duration::from_millis(100));
        manager.stop(&game.id).unwrap();
        worker.join().unwrap();
        assert_eq!(manager.list().unwrap()[0].status, GameStatus::Idle);
        assert_eq!(manager.list().unwrap()[0].error, None);

        drop(database);
        let reopened = Database::open(&database_dir).unwrap();
        let summaries = reopened
            .playtime_summaries(crate::database::now_milliseconds().unwrap())
            .unwrap();
        let summary = summaries
            .iter()
            .find(|summary| summary.game_id == game.id)
            .unwrap();
        assert!(summary.total_milliseconds > 0);
        assert_eq!(summary.active_sessions, 0);
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn manager_launch_tracks_and_stops_a_controlled_game() {
        let base = test_dir("manager-lifecycle");
        let database_dir = base.join("database");
        let app = test_app_with_logs(&database_dir, &base.join("logs"));
        let (database, game) = {
            let state = app.state::<DatabaseState>();
            let database = state.shared_database().unwrap();
            let game = create_manual_game(&state, &base.join("Test Game.exe"));
            (database, game)
        };

        let ready = base.join("runner-ready");
        let gate = base.join("start-game");
        let runner_path = base.join("controlled-wine");
        let runner = test_runner(
            &runner_path,
            "printf 'controlled stdout\\n'; printf 'controlled stderr\\n' >&2\n\
             touch \"$LEGIO_TEST_READY\"\n\
             for _ in {1..500}; do\n\
                 [[ -e \"$LEGIO_TEST_GATE\" ]] && break\n\
                 sleep 0.01\n\
             done\n\
             [[ -e \"$LEGIO_TEST_GATE\" ]] || exit 1\n\
             bash -c 'exec -a \"$1\" sleep 60' _ \"$(basename \"$1\")\" &\n\
             wait\n",
        );
        let config = EffectiveCompatibilityConfig {
            runner_path: Some(runner.path.clone()),
            prefix_root: Some(base.join("prefixes").to_string_lossy().into_owned()),
            debug_logging: true,
            environment: std::collections::BTreeMap::from([
                (
                    "LEGIO_TEST_READY".to_owned(),
                    ready.to_string_lossy().into_owned(),
                ),
                (
                    "LEGIO_TEST_GATE".to_owned(),
                    gate.to_string_lossy().into_owned(),
                ),
            ]),
            ..EffectiveCompatibilityConfig::default()
        };
        let manager = app.state::<GameLaunchManager>().inner().clone();
        manager
            .launch_with_compatibility_config(
                app.handle().clone(),
                game.id.clone(),
                config,
                move |selected_path| {
                    assert_eq!(selected_path, runner.path);
                    Ok(runner)
                },
            )
            .unwrap();

        let started = Instant::now();
        while !ready.exists() {
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "controlled runner did not start"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(manager.list().unwrap()[0].status, GameStatus::Launching);
        fs::write(&gate, b"start").unwrap();
        wait_for_status(&manager, &game.id, GameStatus::Running);
        assert_eq!(
            database
                .playtime_summaries(crate::database::now_milliseconds().unwrap())
                .unwrap()[0]
                .active_sessions,
            1
        );

        manager.stop(&game.id).unwrap();
        wait_for_status(&manager, &game.id, GameStatus::Idle);
        let state = manager.list().unwrap().remove(0);
        assert_eq!(state.error, None);
        assert!(state.compatibility_log_error.is_none());
        assert!(!state.compatibility_log_truncated);
        let log_directory = state.compatibility_log_path.unwrap();
        let output_timeout = Instant::now() + Duration::from_secs(3);
        loop {
            let stdout = fs::read_to_string(log_directory.join("stdout.log")).unwrap_or_default();
            let stderr = fs::read_to_string(log_directory.join("stderr.log")).unwrap_or_default();
            if stdout.contains("controlled stdout") && stderr.contains("controlled stderr") {
                break;
            }
            assert!(
                Instant::now() < output_timeout,
                "runner output was not captured"
            );
            thread::sleep(Duration::from_millis(20));
        }
        let report = fs::read_to_string(log_directory.join("launch.json")).unwrap();
        assert!(report.contains("Controlled Wine"));
        assert!(report.contains("prefixPath"));

        drop(app);
        drop(database);
        let reopened = Database::open(&database_dir).unwrap();
        let summary = reopened
            .playtime_summaries(crate::database::now_milliseconds().unwrap())
            .unwrap()
            .into_iter()
            .find(|summary| summary.game_id == game.id)
            .unwrap();
        assert!(summary.total_milliseconds > 0);
        assert_eq!(summary.active_sessions, 0);
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires a local Windows game executable and Proton installation"]
    fn installed_proton_game_launch_tracks_and_stops() {
        let executable = PathBuf::from(
            std::env::var_os("LEGIO_PHASE07_GAME_EXE")
                .expect("set LEGIO_PHASE07_GAME_EXE to an installed Windows game"),
        );
        let runner_path = std::env::var_os("LEGIO_PHASE07_RUNNER")
            .expect("set LEGIO_PHASE07_RUNNER to an installed Proton directory");
        let runner_path = Path::new(&runner_path)
            .to_str()
            .expect("Proton path must be valid UTF-8");
        let runner = runner_discovery::resolve_runner(runner_path).unwrap();
        let option_profile = std::env::var("LEGIO_PHASE07_TYPED_OPTIONS").unwrap_or_default();
        let typed_options = option_profile == "graphics";
        let proton_logging = typed_options;
        assert!(
            option_profile.is_empty() || typed_options,
            "set LEGIO_PHASE07_TYPED_OPTIONS to graphics"
        );
        assert!(matches!(
            runner.kind,
            RunnerKind::Proton | RunnerKind::GeProton
        ));
        if typed_options {
            assert_eq!(runner.kind, RunnerKind::GeProton);
        }
        let expected_runner = runner.name.clone();

        let base = test_dir("installed-proton-lifecycle");
        let database_dir = base.join("database");
        let app = test_app_with_logs(&database_dir, &base.join("logs"));
        let (database, game) = {
            let state = app.state::<DatabaseState>();
            let database = state.shared_database().unwrap();
            let game = crate::manual_import::import(
                &state,
                crate::manual_import::ManualImportInput {
                    executable_path: executable.to_string_lossy().into_owned(),
                    name: Some("Phase 07 Proton smoke".to_owned()),
                },
            )
            .unwrap();
            (database, game)
        };
        let config = EffectiveCompatibilityConfig {
            runner_path: Some(runner.path.clone()),
            prefix_root: Some(base.join("prefixes").to_string_lossy().into_owned()),
            debug_logging: proton_logging,
            graphics_renderer: if typed_options {
                crate::database::GraphicsRenderer::WineD3d
            } else {
                Default::default()
            },
            wayland: if typed_options {
                crate::database::WaylandMode::Native
            } else {
                Default::default()
            },
            ..EffectiveCompatibilityConfig::default()
        };
        let manager = app.state::<GameLaunchManager>().inner().clone();
        manager
            .launch_with_compatibility_config(
                app.handle().clone(),
                game.id.clone(),
                config,
                move |selected_path| {
                    assert_eq!(selected_path, runner.path);
                    Ok(runner)
                },
            )
            .unwrap();

        let started = Instant::now();
        let launch_timeout = if typed_options {
            Duration::from_secs(180)
        } else {
            Duration::from_secs(90)
        };
        let mut launch_error = None;
        let mut launch_diagnostic = None;
        loop {
            let state = manager
                .list()
                .unwrap()
                .into_iter()
                .find(|state| state.game_id == game.id)
                .unwrap();
            match state.status {
                GameStatus::Running => {
                    launch_diagnostic = state.compatibility_options;
                    eprintln!("Observed {} game process in Running state", expected_runner);
                    break;
                }
                GameStatus::Idle => {
                    launch_error = state.error;
                    break;
                }
                GameStatus::Launching if started.elapsed() >= launch_timeout => {
                    match manager.cancel(&game.id) {
                        Ok(()) => wait_for_status(&manager, &game.id, GameStatus::Idle),
                        Err(_) => {
                            let latest = manager
                                .list()
                                .unwrap()
                                .into_iter()
                                .find(|state| state.game_id == game.id)
                                .unwrap();
                            if latest.status == GameStatus::Running {
                                manager.stop(&game.id).unwrap();
                            }
                            wait_for_status(&manager, &game.id, GameStatus::Idle);
                        }
                    }
                    panic!(
                        "Proton game did not become observable within {} seconds",
                        launch_timeout.as_secs()
                    );
                }
                GameStatus::Launching => thread::sleep(POLL_INTERVAL),
            }
        }
        assert!(
            launch_error.is_none(),
            "Proton game launch failed: {}",
            launch_error.unwrap_or_default()
        );

        let typed_option_evidence =
            typed_options.then(|| game_process_option_evidence(&manager, &game.id));
        let proton_log_evidence = proton_logging.then(|| {
            wait_for_proton_graphics_log(
                &base
                    .join("logs/compatibility")
                    .join(&game.id)
                    .join("steam-default.log"),
            )
        });
        let active = database
            .playtime_summaries(crate::database::now_milliseconds().unwrap())
            .unwrap()
            .into_iter()
            .find(|summary| summary.game_id == game.id)
            .unwrap();
        assert_eq!(active.active_sessions, 1);
        manager.stop(&game.id).unwrap();
        wait_for_status(&manager, &game.id, GameStatus::Idle);
        assert_eq!(manager.list().unwrap()[0].error, None);

        drop(app);
        drop(database);
        let reopened = Database::open(&database_dir).unwrap();
        let summary = reopened
            .playtime_summaries(crate::database::now_milliseconds().unwrap())
            .unwrap()
            .into_iter()
            .find(|summary| summary.game_id == game.id)
            .unwrap();
        assert!(summary.total_milliseconds > 0);
        assert_eq!(summary.active_sessions, 0);
        fs::remove_dir_all(base).unwrap();

        let diagnostic = launch_diagnostic.expect("running game has launch diagnostics");
        assert_eq!(diagnostic.runner, expected_runner);
        assert_eq!(diagnostic.debug_logging, proton_logging);
        if typed_options {
            assert!(!diagnostic.launch_via_steam);
            assert_eq!(
                diagnostic.graphics_renderer,
                crate::database::GraphicsRenderer::WineD3d
            );
            assert_eq!(diagnostic.wayland, crate::database::WaylandMode::Native);
        }
        if let Some(evidence) = proton_log_evidence {
            let entries = evidence.unwrap_or_else(|error| panic!("{error}"));
            let options = entries
                .iter()
                .find(|line| line.contains("Options:"))
                .expect("Proton log records its compatibility options");
            assert!(options.contains("'wined3d'"), "{options}");
            assert!(options.contains("'wayland'"), "{options}");
            assert!(
                entries
                    .iter()
                    .any(|line| line.contains("wined3d.dll") && line.contains("builtin")),
                "Proton did not load its WineD3D implementation: {entries:?}"
            );
            assert!(
                entries.iter().any(|line| line.contains("d3d11.dll")),
                "The game did not load its Direct3D 11 implementation: {entries:?}"
            );
            eprintln!(
                "Proton applied WineD3D and Wayland, then loaded WineD3D and D3D11: {entries:?}"
            );
        }
        if let Some(evidence) = typed_option_evidence {
            eprintln!("{}", evidence.unwrap_or_else(|error| panic!("{error}")));
        }
    }

    #[cfg(target_os = "linux")]
    fn game_process_option_evidence(
        manager: &GameLaunchManager,
        game_id: &str,
    ) -> Result<String, String> {
        let target = manager
            .lock()?
            .get(game_id)
            .map(|entry| entry.process_target.clone())
            .ok_or_else(|| "Running game process target is unavailable".to_owned())?;
        let pids = game_process::matching_pids(&target)?;
        let mut failures = Vec::new();

        for pid in pids {
            let process = PathBuf::from(format!("/proc/{pid}"));
            let environment = fs::read(process.join("environ")).map_err(|error| {
                format!("Could not read game process {pid} environment: {error}")
            })?;
            let expected = ["PROTON_USE_WINED3D=1", "PROTON_ENABLE_WAYLAND=1"];
            let missing = expected
                .iter()
                .filter(|value| {
                    !environment
                        .split(|byte| *byte == 0)
                        .any(|entry| entry == value.as_bytes())
                })
                .copied()
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                failures.push(format!(
                    "Game process {pid} is missing typed environment entries: {}",
                    missing.join(", ")
                ));
                continue;
            }

            let maps = fs::read_to_string(process.join("maps"))
                .map_err(|error| format!("Could not read game process {pid} mappings: {error}"))?;
            let wined3d_loaded = maps
                .lines()
                .any(|line| line.to_ascii_lowercase().contains("wined3d"));
            let wayland_client_loaded = maps
                .lines()
                .any(|line| line.contains("libwayland-client.so"));
            let graphics_maps = maps
                .lines()
                .filter(|line| {
                    let line = line.to_ascii_lowercase();
                    ["wined3d", "d3d", "wayland", "vulkan", "libgl"]
                        .iter()
                        .any(|marker| line.contains(marker))
                })
                .take(30)
                .collect::<Vec<_>>();
            return Ok(format!(
                "Game process {pid} received WineD3D and GE-Proton Wayland settings; WineD3D module mapped: {wined3d_loaded}; Wayland client mapped: {wayland_client_loaded}; graphics mappings: {graphics_maps:?}"
            ));
        }

        Err(if failures.is_empty() {
            "No running game process was available for typed option inspection".to_owned()
        } else {
            failures.join("; ")
        })
    }

    #[cfg(target_os = "linux")]
    fn wait_for_proton_graphics_log(path: &Path) -> Result<Vec<String>, String> {
        let started = Instant::now();
        let timeout = Duration::from_secs(60);
        let mut entries = Vec::new();
        loop {
            if let Ok(log) = fs::read_to_string(path) {
                entries = log
                    .lines()
                    .filter(|line| {
                        let line = line.to_ascii_lowercase();
                        line.contains("wined3d") || line.contains("d3d11")
                    })
                    .take(20)
                    .map(str::to_owned)
                    .collect();
                let has_wined3d = entries
                    .iter()
                    .any(|line| line.contains("wined3d.dll") && line.contains("builtin"));
                let has_d3d11 = entries.iter().any(|line| line.contains("d3d11.dll"));
                if has_wined3d && has_d3d11 {
                    return Ok(entries);
                }
            }
            if started.elapsed() >= timeout {
                return Err(format!(
                    "Proton did not load WineD3D and D3D11 within {} seconds: {entries:?}",
                    timeout.as_secs()
                ));
            }
            thread::sleep(Duration::from_millis(500));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn manager_reports_missing_executable_during_preparation() {
        let base = test_dir("manager-prepare-failure");
        let database_dir = base.join("database");
        let app = test_app(&database_dir);
        let game = app
            .state::<DatabaseState>()
            .database()
            .unwrap()
            .create_game(crate::database::CreateGameInput {
                name: "Unconfigured game".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        let manager = app.state::<GameLaunchManager>().inner().clone();
        let error = manager
            .launch_with_compatibility_config(
                app.handle().clone(),
                game.id,
                EffectiveCompatibilityConfig::default(),
                |_| unreachable!("runner resolution follows executable preparation"),
            )
            .unwrap_err();

        assert_eq!(error, "This manual game has no selected executable");
        assert!(manager.list().unwrap().is_empty());
        drop(app);
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn manager_reports_runner_exit_with_launch_stage_diagnostic() {
        let base = test_dir("manager-launch-failure");
        let database_dir = base.join("database");
        let app = test_app_with_logs(&database_dir, &base.join("logs"));
        let (game, database) = {
            let state = app.state::<DatabaseState>();
            let game = create_manual_game(&state, &base.join("Test Game.exe"));
            (game, state.shared_database().unwrap())
        };
        let runner = test_runner(
            &base.join("failed-wine"),
            "printf 'runner failure stdout\\n'; printf 'runner failure stderr\\n' >&2\nexit 7\n",
        );
        let config = EffectiveCompatibilityConfig {
            runner_path: Some(runner.path.clone()),
            prefix_root: Some(base.join("prefixes").to_string_lossy().into_owned()),
            debug_logging: true,
            ..EffectiveCompatibilityConfig::default()
        };
        let manager = app.state::<GameLaunchManager>().inner().clone();
        manager
            .launch_with_compatibility_config(
                app.handle().clone(),
                game.id.clone(),
                config,
                move |selected_path| {
                    assert_eq!(selected_path, runner.path);
                    Ok(runner)
                },
            )
            .unwrap();

        wait_for_status(&manager, &game.id, GameStatus::Idle);
        let state = manager.list().unwrap().remove(0);
        assert!(
            state
                .error
                .as_deref()
                .unwrap()
                .contains("Launch stage failed: runner exited before the game started")
        );
        assert_eq!(state.runner_exit_code, Some(7));
        let log_directory = state.compatibility_log_path.unwrap();
        assert_eq!(
            fs::read_to_string(log_directory.join("runner-exit-code.txt"))
                .unwrap()
                .trim(),
            "7"
        );
        let output_deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let stdout = fs::read_to_string(log_directory.join("stdout.log")).unwrap_or_default();
            let stderr = fs::read_to_string(log_directory.join("stderr.log")).unwrap_or_default();
            if stdout.contains("runner failure stdout") && stderr.contains("runner failure stderr")
            {
                break;
            }
            assert!(
                Instant::now() < output_deadline,
                "runner failure output was not captured"
            );
            thread::sleep(Duration::from_millis(20));
        }
        drop(app);
        drop(database);
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
            .reserve_launch("runner", None, target.clone(), None)
            .unwrap();
        manager.run_launch(
            "runner".to_owned(),
            target,
            cancel,
            LaunchContext::default(),
            |_| Err("Could not start compatibility runner: no such file".to_owned()),
        );
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
            .reserve_launch("runner", None, target.clone(), None)
            .unwrap();
        manager.run_launch(
            "runner".to_owned(),
            target,
            cancel,
            LaunchContext::default(),
            |_| {
                std::process::Command::new("/bin/bash")
                    .args(["-c", "exit 7"])
                    .spawn()
                    .map(Some)
                    .map_err(|error| error.to_string())
            },
        );
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
            .reserve_launch("runner", None, target.clone(), None)
            .unwrap();
        let worker_manager = manager.clone();
        let ready_path = ready.to_string_lossy().into_owned();
        let worker = thread::spawn(move || {
            worker_manager.run_launch(
                "runner".to_owned(),
                target,
                cancel,
                LaunchContext::default(),
                move |_| {
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
                },
            );
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
                compatibility_options: None,
                #[cfg(target_os = "linux")]
                compatibility_log: None,
                runner_exit_code: None,
                cancel: Arc::clone(&cancel),
                prefix: None,
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
                cancel,
                LaunchContext {
                    steam_app_id: Some(APP_ID),
                    ..LaunchContext::default()
                },
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
                compatibility_options: None,
                #[cfg(target_os = "linux")]
                compatibility_log: None,
                runner_exit_code: None,
                cancel: Arc::clone(&cancel),
                prefix: None,
            },
        );

        let missing_executable = base.join("missing-game");
        manager.run_launch(
            "missing".to_owned(),
            game_process::ProcessTarget::Steam {
                app_id: 42,
                install_path: base.join("game"),
            },
            cancel,
            LaunchContext {
                steam_app_id: Some(42),
                ..LaunchContext::default()
            },
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
