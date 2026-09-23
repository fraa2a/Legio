use std::path::{Path, PathBuf};
use std::process::Command;
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
    running_processes()
}

pub(crate) fn request_shutdown_and_wait(
    steam_root: &Path,
    timeout: Duration,
) -> Result<(), String> {
    if !is_running()? {
        return Ok(());
    }

    let executable = steam_executable(steam_root)?;
    let start = Instant::now();
    let mut request = Command::new(executable)
        .arg("-shutdown")
        .spawn()
        .map_err(|error| format!("Could not request Steam shutdown: {error}"))?;
    loop {
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
    wait_until_stopped(timeout.saturating_sub(start.elapsed()))
}

pub(crate) fn launch_and_wait_selected_account(
    steam_root: &Path,
    expected_account_id: &str,
    timeout: Duration,
) -> Result<(), String> {
    if is_running()? {
        return Err(
            "Steam is already running. Close it before selecting another account.".to_owned(),
        );
    }
    let executable = steam_executable(steam_root)?;
    Command::new(executable)
        .spawn()
        .map_err(|error| format!("Could not start Steam: {error}"))?;

    let start = Instant::now();
    loop {
        if is_running()?
            && let Some(selected_id) = active_account_id(steam_root)?
        {
            if selected_id == expected_account_id {
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

fn wait_until_stopped(timeout: Duration) -> Result<(), String> {
    let start = Instant::now();
    loop {
        if !is_running()? {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            return Err("Timed out waiting for Steam and steamwebhelper to exit".to_owned());
        }
        thread::sleep(POLL_INTERVAL.min(timeout.saturating_sub(start.elapsed())));
    }
}

#[cfg(windows)]
fn running_processes() -> Result<bool, String> {
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
    Ok(output.lines().any(|line| {
        let name = line
            .trim()
            .trim_start_matches('"')
            .split('"')
            .next()
            .unwrap_or("");
        matches!(
            name.to_ascii_lowercase().as_str(),
            "steam.exe" | "steamwebhelper.exe"
        )
    }))
}

#[cfg(target_os = "linux")]
fn running_processes() -> Result<bool, String> {
    let entries = std::fs::read_dir("/proc")
        .map_err(|error| format!("Could not inspect running processes: {error}"))?;
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
        if matches!(comm.trim(), "steam" | "steamwebhelper") {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn running_processes() -> Result<bool, String> {
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
    crate::steam_vdf::selected_account_id(&bytes)
        .map_err(|error| format!("Could not read Steam account selection: {error}"))
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

    #[cfg(target_os = "linux")]
    #[test]
    fn selected_account_reader_returns_unknown_without_loginusers_file() {
        let root = std::env::temp_dir().join(format!("legio-steam-process-{}", std::process::id()));
        assert_eq!(linux_selected_account_id(&root).unwrap(), None);
    }
}
