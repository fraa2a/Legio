use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
#[cfg(windows)]
const STEAM_ID64_BASE: u64 = 76_561_197_960_265_728;

pub(crate) fn steam_executable(steam_root: &Path) -> Result<PathBuf, String> {
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
            Command::new(executable)
                .spawn()
                .map_err(|error| format!("Could not start Steam: {error}"))?,
        )
    };
    wait_for_client_interface(timeout, cancel, running_processes, request)
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
    Command::new(executable)
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
    let mut request = Command::new(executable)
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
    Command::new(executable)
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
        windows_active_account_id()
    }
    #[cfg(target_os = "linux")]
    {
        linux_selected_account_id(steam_root)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = steam_root;
        Ok(None)
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
    let output = Command::new("tasklist")
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

#[cfg(not(any(windows, target_os = "linux")))]
fn running_processes() -> Result<SteamProcesses, String> {
    Err("Steam process inspection is unsupported on this platform".to_owned())
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
    selected_account_id_from_loginusers(&bytes)
        .map_err(|error| format!("Could not read Steam account selection: {error}"))
}

#[cfg(target_os = "linux")]
fn selected_account_id_from_loginusers(
    bytes: &[u8],
) -> Result<Option<String>, crate::steam_vdf::VdfError> {
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
            selected_account_id_from_loginusers(loginusers).unwrap(),
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
        assert_eq!(
            selected_account_id_from_loginusers(ambiguous).unwrap(),
            None
        );
        assert_eq!(
            selected_account_id_from_loginusers(conflicting).unwrap(),
            None
        );
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
