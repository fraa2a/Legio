use crate::background_process::command;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
#[cfg(windows)]
const STEAM_ID64_BASE: u64 = 76_561_197_960_265_728;

pub(crate) fn steam_executable(steam_root: &Path) -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    if let Some(launcher) = linux_steam_launcher(std::env::var_os("PATH").as_deref()) {
        return Ok(launcher);
    }
    executable_candidates(steam_root, cfg!(windows))
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            format!(
                "Steam executable was not found under {}",
                steam_root.display()
            )
        })
}

#[cfg(target_os = "linux")]
fn linux_steam_launcher(path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    std::env::split_paths(path?).find_map(|directory| {
        if !directory.is_absolute() {
            return None;
        }
        let launcher = directory.join("steam");
        launcher
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .then_some(launcher)
    })
}

/// Steam must not inherit the AppImage's bundled libraries, which break its shell launcher.
fn steam_command(executable: PathBuf) -> std::process::Command {
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut command = command(executable);
    #[cfg(target_os = "linux")]
    crate::launch_environment::apply(&mut command);
    command
}

fn executable_candidates(steam_root: &Path, windows: bool) -> Vec<PathBuf> {
    let names: &[&str] = if windows {
        &["steam.exe"]
    } else {
        &["steam", "steam.sh"]
    };
    names.iter().map(|name| steam_root.join(name)).collect()
}

pub(crate) fn is_running() -> Result<bool, String> {
    Ok(running_processes()?.is_running())
}

fn client_is_running() -> Result<bool, String> {
    Ok(running_processes()?.client)
}

pub(crate) fn ensure_running(
    steam_root: &Path,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<(), String> {
    check_cancelled(cancel)?;
    let processes = running_processes()?;
    if processes.is_ready() {
        check_cancelled(cancel)?;
        return Ok(());
    }

    let request = if processes.client {
        None
    } else {
        let executable = steam_executable(steam_root)?;
        check_cancelled(cancel)?;
        Some(
            steam_command(executable)
                .spawn()
                .map_err(|error| format!("Could not start Steam: {error}"))?,
        )
    };
    wait_for_client_interface(timeout, cancel, running_processes, request)
}

/// What a launch is waiting for while Steam gets ready.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SteamWait {
    Starting,
    SigningIn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoginState {
    Pending,
    NeedsCredentials,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveAccount {
    SignedIn,
    SignedOut,
    /// Steam wrote a complete registry without an active account entry.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Unreported,
}

/// Starts Steam when needed and waits until it is signed in and its library is ready, so a
/// game that needs Steam starts only after the client can serve it.
pub(crate) fn ensure_signed_in(
    steam_root: &Path,
    timeout: Duration,
    cancel: &AtomicBool,
    report: &mut dyn FnMut(SteamWait),
) -> Result<(), String> {
    let start = Instant::now();
    if !running_processes()?.is_ready() {
        report(SteamWait::Starting);
    }
    ensure_running(steam_root, timeout, cancel)?;
    let mut fallback_logged = false;
    wait_for_sign_in(
        timeout.saturating_sub(start.elapsed()),
        cancel,
        report,
        || {
            let login = login_state(steam_root)?;
            if login.is_none() && !fallback_logged {
                fallback_logged = true;
                crate::application_log::state_change(
                    "steam_login_state",
                    "Steam login log has no state; waiting for the active account only",
                );
            }
            let account = active_account(steam_root)?;
            Ok((client_is_running()?, effective_login_state(account, login)))
        },
    )
}

fn wait_for_sign_in(
    timeout: Duration,
    cancel: &AtomicBool,
    report: &mut dyn FnMut(SteamWait),
    mut inspect: impl FnMut() -> Result<(bool, LoginState), String>,
) -> Result<(), String> {
    let start = Instant::now();
    let mut reported = None;
    loop {
        check_cancelled(cancel)?;
        let (client_running, login) = inspect()?;
        if !client_running {
            return Err("Steam closed before signing in".to_owned());
        }
        let wait = match login {
            LoginState::Ready => {
                check_cancelled(cancel)?;
                return Ok(());
            }
            LoginState::Pending => SteamWait::Starting,
            LoginState::NeedsCredentials => SteamWait::SigningIn,
        };
        if reported != Some(wait) {
            report(wait);
            reported = Some(wait);
        }
        if start.elapsed() >= timeout {
            return Err(match wait {
                SteamWait::SigningIn => "Timed out while waiting for a Steam sign-in",
                SteamWait::Starting => "Timed out while waiting for Steam to sign in",
            }
            .to_owned());
        }
        thread::sleep(POLL_INTERVAL.min(timeout.saturating_sub(start.elapsed())));
    }
}

/// Steam sets the active account at logon, before its library is ready, so the login log
/// decides when the account is active. Without a usable log the active account decides alone,
/// and when Steam does not report an active account the log decides alone.
fn effective_login_state(account: ActiveAccount, login: Option<LoginState>) -> LoginState {
    match (account, login) {
        (ActiveAccount::SignedIn, Some(LoginState::Ready) | None)
        | (ActiveAccount::Unreported, Some(LoginState::Ready)) => LoginState::Ready,
        (_, Some(LoginState::NeedsCredentials)) => LoginState::NeedsCredentials,
        _ => LoginState::Pending,
    }
}

const LOGIN_LOG_TAIL_BYTES: u64 = 64 * 1024;

/// The current login state from Steam's `steamui_login.txt`, or `None` when the log is
/// missing or has no recognizable state.
fn login_state(steam_root: &Path) -> Result<Option<LoginState>, String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = match std::fs::File::open(steam_root.join("logs/steamui_login.txt")) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not read Steam's login log: {error}")),
    };
    let length = file
        .metadata()
        .map_err(|error| format!("Could not read Steam's login log: {error}"))?
        .len();
    file.seek(SeekFrom::Start(length.saturating_sub(LOGIN_LOG_TAIL_BYTES)))
        .map_err(|error| format!("Could not read Steam's login log: {error}"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read Steam's login log: {error}"))?;
    Ok(login_state_from_log(&String::from_utf8_lossy(&bytes)))
}

/// Each Steam start begins a session with a `Client version` line; the last
/// `SetLoginState` after it is the current state.
fn login_state_from_log(log: &str) -> Option<LoginState> {
    let mut state = None;
    for line in log.lines() {
        if line.contains("] Client version:") {
            state = Some(LoginState::Pending);
        } else if let Some((_, rest)) = line.split_once("SetLoginState: ") {
            state = Some(match rest.split_whitespace().next() {
                Some("Success") => LoginState::Ready,
                Some("WaitingForCredentials") => LoginState::NeedsCredentials,
                _ => LoginState::Pending,
            });
        }
    }
    state
}

/// Whether Steam is signed in to an account right now. It is signed out while still starting.
fn active_account(steam_root: &Path) -> Result<ActiveAccount, String> {
    #[cfg(windows)]
    {
        let _ = steam_root;
        Ok(match windows_active_account_id()? {
            Some(_) => ActiveAccount::SignedIn,
            None => ActiveAccount::SignedOut,
        })
    }
    #[cfg(target_os = "linux")]
    {
        let Ok(path) = crate::steam_local::steam_registry_path(steam_root) else {
            // Steam creates registry.vdf on its first start.
            return Ok(ActiveAccount::SignedOut);
        };
        match std::fs::read(path) {
            Ok(bytes) => Ok(active_account_from_registry(&bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(ActiveAccount::SignedOut)
            }
            Err(error) => Err(format!("Could not read Steam's active account: {error}")),
        }
    }
}

/// Whether Steam reports a running game. Steam keeps the app ID in `RunningAppID` while one runs.
pub(crate) fn is_game_running(steam_root: &Path) -> Result<bool, String> {
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;

        let _ = steam_root;
        let steam = match RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam") {
            Ok(key) => key,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(format!("Could not read Steam's running game: {error}")),
        };
        match steam.get_value::<u32, _>("RunningAppID") {
            Ok(app_id) => Ok(app_id != 0),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!("Could not read Steam's running game: {error}")),
        }
    }
    #[cfg(target_os = "linux")]
    {
        let Ok(path) = crate::steam_local::steam_registry_path(steam_root) else {
            return Ok(false);
        };
        match std::fs::read(path) {
            Ok(bytes) => Ok(registry_has_running_game(&bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!("Could not read Steam's running game: {error}")),
        }
    }
}

/// A document Steam is rewriting counts as running, so the account switch still asks first.
#[cfg(target_os = "linux")]
fn registry_has_running_game(bytes: &[u8]) -> bool {
    match crate::steam_vdf::keyvalues_scalar(
        bytes,
        &["HKCU", "Software", "Valve", "Steam", "RunningAppID"],
    ) {
        Ok(value) => value.parse::<u32>() != Ok(0),
        Err(crate::steam_vdf::VdfError::ScalarNotFound) => false,
        Err(_) => true,
    }
}

/// Steam rewrites registry.vdf while it signs in, so an unreadable document counts as signed out.
/// Recent Steam clients write no `ActiveProcess` entry at all.
#[cfg(target_os = "linux")]
fn active_account_from_registry(bytes: &[u8]) -> ActiveAccount {
    let value = match crate::steam_vdf::keyvalues_scalar(
        bytes,
        &[
            "HKCU",
            "Software",
            "Valve",
            "Steam",
            "ActiveProcess",
            "ActiveUser",
        ],
    ) {
        Ok(value) => value,
        Err(crate::steam_vdf::VdfError::ScalarNotFound) => return ActiveAccount::Unreported,
        Err(_) => return ActiveAccount::SignedOut,
    };
    match value.parse::<u32>() {
        Ok(account_id) if account_id != 0 => ActiveAccount::SignedIn,
        _ => ActiveAccount::SignedOut,
    }
}

fn wait_for_client_interface(
    timeout: Duration,
    cancel: &AtomicBool,
    mut inspect: impl FnMut() -> Result<SteamProcesses, String>,
    mut request: Option<Child>,
) -> Result<(), String> {
    let start = Instant::now();
    loop {
        check_cancelled(cancel)?;
        if inspect()?.is_ready() {
            check_cancelled(cancel)?;
            return Ok(());
        }
        let startup_status = match request.as_mut() {
            Some(request) => request
                .try_wait()
                .map_err(|error| format!("Could not wait for Steam startup: {error}"))?,
            None => None,
        };
        if let Some(status) = startup_status {
            if !status.success() {
                return Err(format!("Steam startup exited with {status}"));
            }
            request = None;
        }
        if start.elapsed() >= timeout {
            return Err(
                "Timed out while waiting for the Steam client and interface to start".to_owned(),
            );
        }
        thread::sleep(POLL_INTERVAL.min(timeout.saturating_sub(start.elapsed())));
    }
}

pub(crate) fn request_game_launch(
    steam_root: &Path,
    app_id: u32,
    arguments: &[String],
    cancel: &AtomicBool,
) -> Result<(), String> {
    if app_id == 0 {
        return Err("Steam App ID is invalid".to_owned());
    }
    check_cancelled(cancel)?;
    let executable = steam_executable(steam_root)?;
    check_cancelled(cancel)?;
    steam_command(executable)
        .arg("-applaunch")
        .arg(app_id.to_string())
        .args(arguments)
        .spawn()
        .map_err(|error| format!("Could not ask Steam to launch the game: {error}"))?;
    Ok(())
}

pub(crate) fn request_shutdown_and_wait(
    steam_root: &Path,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<(), String> {
    check_cancelled(cancel)?;
    if !is_running()? {
        check_cancelled(cancel)?;
        return Ok(());
    }

    let executable = steam_executable(steam_root)?;
    check_cancelled(cancel)?;
    let start = Instant::now();
    let mut request = steam_command(executable)
        .arg("-shutdown")
        .spawn()
        .map_err(|error| format!("Could not request Steam shutdown: {error}"))?;
    loop {
        check_cancelled(cancel)?;
        if let Some(status) = request
            .try_wait()
            .map_err(|error| format!("Could not wait for Steam shutdown request: {error}"))?
        {
            if !status.success() {
                return Err(format!("Steam shutdown request exited with {status}"));
            }
            break;
        }
        if start.elapsed() >= timeout {
            return Err("Timed out waiting for Steam shutdown request".to_owned());
        }
        thread::sleep(POLL_INTERVAL.min(timeout.saturating_sub(start.elapsed())));
    }
    wait_until_stopped(timeout.saturating_sub(start.elapsed()), cancel)
}

pub(crate) fn launch_and_wait_selected_account(
    steam_root: &Path,
    expected_account_id: &str,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<(), String> {
    check_cancelled(cancel)?;
    if is_running()? {
        return Err(
            "Steam is already running. Close it before selecting another account.".to_owned(),
        );
    }
    let executable = steam_executable(steam_root)?;
    check_cancelled(cancel)?;
    steam_command(executable)
        .spawn()
        .map_err(|error| format!("Could not start Steam: {error}"))?;

    let start = Instant::now();
    loop {
        check_cancelled(cancel)?;
        if client_is_running()?
            && let Some(selected_id) = active_account_id(steam_root)?
        {
            if selected_id == expected_account_id {
                check_cancelled(cancel)?;
                return Ok(());
            }
            return Err(format!(
                "Steam started with account {selected_id}, expected {expected_account_id}"
            ));
        }
        if start.elapsed() >= timeout {
            return Err(
                "Timed out while waiting for Steam to start with the selected account".to_owned(),
            );
        }
        thread::sleep(POLL_INTERVAL.min(timeout.saturating_sub(start.elapsed())));
    }
}

pub(crate) fn active_account_id(steam_root: &Path) -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        let _ = steam_root;
        windows_active_account_id()
    }
    #[cfg(target_os = "linux")]
    {
        linux_selected_account_id(steam_root)
    }
}

fn wait_until_stopped(timeout: Duration, cancel: &AtomicBool) -> Result<(), String> {
    let start = Instant::now();
    loop {
        check_cancelled(cancel)?;
        if !is_running()? {
            check_cancelled(cancel)?;
            return Ok(());
        }
        if start.elapsed() >= timeout {
            return Err("Timed out waiting for Steam and steamwebhelper to exit".to_owned());
        }
        thread::sleep(POLL_INTERVAL.min(timeout.saturating_sub(start.elapsed())));
    }
}

fn check_cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Launch cancelled".to_owned())
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SteamProcesses {
    client: bool,
    web_helper: bool,
}

impl SteamProcesses {
    fn is_running(self) -> bool {
        self.client || self.web_helper
    }

    fn is_ready(self) -> bool {
        self.client && self.web_helper
    }
}

#[cfg(windows)]
fn running_processes() -> Result<SteamProcesses, String> {
    let output = command("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .output()
        .map_err(|error| format!("Could not inspect running processes: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Could not inspect running processes: {}",
            output.status
        ));
    }
    let output = String::from_utf8_lossy(&output.stdout);
    let mut processes = SteamProcesses::default();
    for line in output.lines() {
        let name = line
            .trim()
            .trim_start_matches('"')
            .split('"')
            .next()
            .unwrap_or("");
        if name.eq_ignore_ascii_case("steam.exe") {
            processes.client = true;
        } else if name.eq_ignore_ascii_case("steamwebhelper.exe") {
            processes.web_helper = true;
        }
    }
    Ok(processes)
}

#[cfg(target_os = "linux")]
fn running_processes() -> Result<SteamProcesses, String> {
    let entries = std::fs::read_dir("/proc")
        .map_err(|error| format!("Could not inspect running processes: {error}"))?;
    let mut processes = SteamProcesses::default();
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Could not inspect running processes: {error}"))?;
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        let comm = match std::fs::read_to_string(entry.path().join("comm")) {
            Ok(comm) => comm,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => continue,
            Err(error) => return Err(format!("Could not inspect a running process: {error}")),
        };
        match comm.trim() {
            "steam" => processes.client = true,
            "steamwebhelper" => processes.web_helper = true,
            _ => {}
        }
        if processes.is_ready() {
            break;
        }
    }
    Ok(processes)
}

#[cfg(windows)]
fn windows_active_account_id() -> Result<Option<String>, String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    let active_process = match current_user.open_subkey("Software\\Valve\\Steam\\ActiveProcess") {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not read Steam's active account: {error}")),
    };
    let account_id: u32 = match active_process.get_value("ActiveUser") {
        Ok(account_id) => account_id,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not read Steam's active account: {error}")),
    };
    if account_id == 0 {
        return Ok(None);
    }
    Ok(Some((STEAM_ID64_BASE + u64::from(account_id)).to_string()))
}

#[cfg(target_os = "linux")]
fn linux_selected_account_id(steam_root: &Path) -> Result<Option<String>, String> {
    let path = steam_root.join("config/loginusers.vdf");
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not read Steam account selection: {error}")),
    };
    account_id_from_loginusers(&bytes)
        .map_err(|error| format!("Could not read Steam account selection: {error}"))
}

#[cfg(target_os = "linux")]
fn account_id_from_loginusers(bytes: &[u8]) -> Result<Option<String>, crate::steam_vdf::VdfError> {
    let selected = crate::steam_vdf::selected_account_id(bytes)?;
    if selected.is_some() {
        return Ok(selected);
    }

    let users = crate::steam_vdf::parse_loginusers(bytes)?;
    // Some Linux Steam installations omit MostRecent for every saved account.
    if users.iter().any(|user| user.most_recent.is_some()) {
        return Ok(None);
    }
    let mut auto_login = users
        .iter()
        .filter(|user| user.auto_login.as_deref() == Some("1"));
    let selected = auto_login.next();
    if auto_login.next().is_some() {
        return Ok(None);
    }
    Ok(selected.map(|user| user.steam_id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_launcher_uses_the_executable_bootstrap_on_path() {
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("legio-steam-launcher-{}", uuid::Uuid::new_v4()));
        let ignored = root.join("ignored");
        let packaged = root.join("packaged");
        std::fs::create_dir_all(&ignored).unwrap();
        std::fs::create_dir_all(&packaged).unwrap();
        for (directory, mode) in [(&ignored, 0o600), (&packaged, 0o755)] {
            let file = directory.join("steam");
            std::fs::write(&file, b"#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        let path = std::env::join_paths([&ignored, &packaged]).unwrap();
        assert_eq!(
            linux_steam_launcher(Some(&path)),
            Some(packaged.join("steam"))
        );
        assert!(linux_steam_launcher(None).is_none());
        assert!(linux_steam_launcher(Some(std::ffi::OsStr::new("."))).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn executable_candidates_follow_platform_names() {
        let root = Path::new("/opt/steam");
        assert_eq!(
            executable_candidates(root, true),
            vec![root.join("steam.exe")]
        );
        assert_eq!(
            executable_candidates(root, false),
            vec![root.join("steam"), root.join("steam.sh")]
        );
    }

    #[test]
    fn steam_is_ready_only_when_client_and_web_helper_are_running() {
        assert!(
            !SteamProcesses {
                client: true,
                web_helper: false,
            }
            .is_ready()
        );
        assert!(
            !SteamProcesses {
                client: false,
                web_helper: true,
            }
            .is_ready()
        );
        assert!(
            SteamProcesses {
                client: true,
                web_helper: true,
            }
            .is_ready()
        );
    }

    #[test]
    fn steam_is_running_while_either_client_process_exists() {
        assert!(
            SteamProcesses {
                client: true,
                web_helper: false,
            }
            .is_running()
        );
        assert!(
            SteamProcesses {
                client: false,
                web_helper: true,
            }
            .is_running()
        );
        assert!(!SteamProcesses::default().is_running());
    }

    #[test]
    fn client_interface_waits_for_web_helper_before_returning() {
        let cancel = AtomicBool::new(false);
        let mut calls = 0;
        wait_for_client_interface(
            Duration::from_secs(1),
            &cancel,
            || {
                calls += 1;
                Ok(SteamProcesses {
                    client: true,
                    web_helper: calls > 1,
                })
            },
            None,
        )
        .unwrap();

        assert_eq!(calls, 2);
    }

    #[test]
    fn client_interface_wait_timeout_reports_missing_readiness() {
        let cancel = AtomicBool::new(false);
        let error = wait_for_client_interface(
            Duration::ZERO,
            &cancel,
            || {
                Ok(SteamProcesses {
                    client: true,
                    web_helper: false,
                })
            },
            None,
        )
        .unwrap_err();

        assert_eq!(
            error,
            "Timed out while waiting for the Steam client and interface to start"
        );
    }

    #[test]
    fn sign_in_wait_returns_once_steam_is_ready_and_reports_each_wait_once() {
        let cancel = AtomicBool::new(false);
        let mut calls = 0;
        let mut reports = Vec::new();
        wait_for_sign_in(
            Duration::from_secs(1),
            &cancel,
            &mut |wait| reports.push(wait),
            || {
                calls += 1;
                Ok((
                    true,
                    match calls {
                        1 | 2 => LoginState::Pending,
                        3 => LoginState::NeedsCredentials,
                        _ => LoginState::Ready,
                    },
                ))
            },
        )
        .unwrap();

        assert_eq!(calls, 4);
        assert_eq!(reports, [SteamWait::Starting, SteamWait::SigningIn]);
    }

    #[test]
    fn sign_in_wait_stops_when_steam_closes() {
        let cancel = AtomicBool::new(false);
        let error = wait_for_sign_in(Duration::from_secs(1), &cancel, &mut |_| {}, || {
            Ok((false, LoginState::Pending))
        })
        .unwrap_err();

        assert_eq!(error, "Steam closed before signing in");
    }

    #[test]
    fn sign_in_wait_times_out_while_signed_out() {
        let cancel = AtomicBool::new(false);
        let error = wait_for_sign_in(Duration::ZERO, &cancel, &mut |_| {}, || {
            Ok((true, LoginState::NeedsCredentials))
        })
        .unwrap_err();

        assert_eq!(error, "Timed out while waiting for a Steam sign-in");
    }

    #[test]
    fn login_log_state_follows_the_latest_session() {
        // Lines copied from a real steamui_login.txt.
        let ready = "\
[2026-11-18 21:42:07] Client version: 1794536281
[2026-11-18 21:42:07] [ None ] SetLoginState: WaitingForNetwork - OK
[2026-11-18 21:42:08] [ WaitingForServerResponse ] SetLoginState: WaitingForLibraryReady - OK
[2026-11-18 21:42:44] [ WaitingForLibraryReady ] SetLoginState: Success - OK
";
        let elsewhere = format!(
            "{ready}[2026-11-20 15:30:23] [ Success ] SetLoginState: WaitingForCredentials - Logged In Elsewhere\n"
        );
        let restarted = format!("{elsewhere}[2026-11-20 20:47:36] Client version: 1794536281\n");
        let loading = &ready[..ready.find("[2026-11-18 21:42:44]").unwrap()];

        assert_eq!(login_state_from_log(ready), Some(LoginState::Ready));
        assert_eq!(login_state_from_log(loading), Some(LoginState::Pending));
        assert_eq!(
            login_state_from_log(&elsewhere),
            Some(LoginState::NeedsCredentials)
        );
        assert_eq!(login_state_from_log(&restarted), Some(LoginState::Pending));
        assert_eq!(login_state_from_log("unrelated\n"), None);
    }

    #[test]
    fn active_account_is_ready_only_after_the_library_is_ready() {
        use ActiveAccount::{SignedIn, SignedOut, Unreported};
        assert_eq!(
            effective_login_state(SignedIn, Some(LoginState::Pending)),
            LoginState::Pending
        );
        assert_eq!(
            effective_login_state(SignedIn, Some(LoginState::Ready)),
            LoginState::Ready
        );
        assert_eq!(
            effective_login_state(SignedOut, Some(LoginState::Ready)),
            LoginState::Pending
        );
        // Without a usable login log the active account decides alone.
        assert_eq!(effective_login_state(SignedIn, None), LoginState::Ready);
        assert_eq!(effective_login_state(SignedOut, None), LoginState::Pending);
        assert_eq!(
            effective_login_state(SignedOut, Some(LoginState::NeedsCredentials)),
            LoginState::NeedsCredentials
        );
        // Without an active account entry the login log decides alone.
        assert_eq!(
            effective_login_state(Unreported, Some(LoginState::Ready)),
            LoginState::Ready
        );
        assert_eq!(
            effective_login_state(Unreported, Some(LoginState::Pending)),
            LoginState::Pending
        );
        assert_eq!(effective_login_state(Unreported, None), LoginState::Pending);
    }

    #[test]
    fn cancelled_sign_in_wait_does_not_inspect_steam() {
        let cancel = AtomicBool::new(true);
        let error = wait_for_sign_in(Duration::from_secs(1), &cancel, &mut |_| {}, || {
            unreachable!("a cancelled wait must not inspect Steam")
        })
        .unwrap_err();

        assert_eq!(error, "Launch cancelled");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn registry_active_user_is_the_signed_in_account() {
        let registry = br#""Registry"
{
	"HKCU"
	{
		"Software"
		{
			"Valve"
			{
				"Steam"
				{
					"AutoLoginUser"		"player"
					"ActiveProcess"
					{
						"pid"		"4242"
						"ActiveUser"		"1"
					}
				}
			}
		}
	}
}
"#;
        assert_eq!(
            active_account_from_registry(registry),
            ActiveAccount::SignedIn
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn registry_running_app_id_reports_a_running_game() {
        let registry = |running: &str| {
            format!(
                "\"Registry\"\n{{\n\"HKCU\"\n{{\n\"Software\"\n{{\n\"Valve\"\n{{\n\"Steam\"\n{{\n{running}\n}}\n}}\n}}\n}}\n}}\n"
            )
        };

        assert!(registry_has_running_game(
            registry("\"RunningAppID\" \"480\"").as_bytes()
        ));
        assert!(!registry_has_running_game(
            registry("\"RunningAppID\" \"0\"").as_bytes()
        ));
        assert!(!registry_has_running_game(
            registry("\"AutoLoginUser\" \"player\"").as_bytes()
        ));
        // A document Steam is still writing.
        assert!(registry_has_running_game(b"\"Registry\"\n{\n\"HKCU\"\n{"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn registry_without_an_active_user_is_signed_out() {
        // Steam writes 0 while signed out; a half-written file has no closing braces.
        let signed_out = br#""Registry" { "HKCU" { "Software" { "Valve" { "Steam" { "ActiveProcess" { "ActiveUser" "0" } } } } } }"#;
        let partial = br#""Registry" { "HKCU" { "Software" { "Valve" { "Steam" { "ActiveProcess" { "ActiveUser" "1""#;

        let unreported =
            br#""Registry" { "HKCU" { "Software" { "Valve" { "Steam" { "AutoLoginUser" "player" } } } } }"#;

        assert_eq!(
            active_account_from_registry(signed_out),
            ActiveAccount::SignedOut
        );
        assert_eq!(
            active_account_from_registry(partial),
            ActiveAccount::SignedOut
        );
        assert_eq!(active_account_from_registry(b""), ActiveAccount::SignedOut);
        assert_eq!(
            active_account_from_registry(unreported),
            ActiveAccount::Unreported
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn selected_account_reader_returns_unknown_without_loginusers_file() {
        let root = std::env::temp_dir().join(format!("legio-steam-process-{}", std::process::id()));
        assert_eq!(linux_selected_account_id(&root).unwrap(), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_selection_uses_unique_auto_login_when_most_recent_is_absent() {
        let loginusers = br#""users" {
            "76561198000000001" { "AutoLogin" "1" }
            "76561198000000002" { "AutoLogin" "0" }
        }"#;
        assert_eq!(
            account_id_from_loginusers(loginusers).unwrap(),
            Some("76561198000000001".to_owned())
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_selection_stays_unknown_when_auto_login_is_ambiguous_or_conflicting() {
        let ambiguous = br#""users" {
            "76561198000000001" { "AutoLogin" "1" }
            "76561198000000002" { "AutoLogin" "1" }
        }"#;
        let conflicting = br#""users" {
            "76561198000000001" { "AutoLogin" "1" "MostRecent" "0" }
            "76561198000000002" { "AutoLogin" "0" "MostRecent" "1" }
        }"#;
        assert_eq!(account_id_from_loginusers(ambiguous).unwrap(), None);
        assert_eq!(account_id_from_loginusers(conflicting).unwrap(), None);
    }

    #[test]
    fn cancelled_launch_does_not_start_steam() {
        let cancel = AtomicBool::new(true);
        let result = launch_and_wait_selected_account(
            Path::new("/missing-steam"),
            "123",
            Duration::from_secs(1),
            &cancel,
        );
        assert_eq!(result.unwrap_err(), "Launch cancelled");
    }

    #[test]
    fn game_launch_rejects_invalid_app_id() {
        let cancel = AtomicBool::new(false);
        assert_eq!(
            request_game_launch(Path::new("/missing-steam"), 0, &[], &cancel).unwrap_err(),
            "Steam App ID is invalid"
        );
    }
}
