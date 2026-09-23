use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

const STOP_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn matching_pids(app_id: u32, install_path: &Path) -> Result<Vec<u32>, String> {
    #[cfg(target_os = "linux")]
    {
        let _ = install_path;
        linux_matching_pids(app_id)
    }
    #[cfg(windows)]
    {
        let _ = app_id;
        windows_matching_pids(install_path)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (app_id, install_path);
        Err("Game process inspection is unsupported on this platform".to_owned())
    }
}

pub(crate) fn stop(app_id: u32, install_path: &Path) -> Result<(), String> {
    let pids = matching_pids(app_id, install_path)?;
    if pids.is_empty() {
        return Ok(());
    }
    for pid in pids {
        // Recheck each PID before signaling to avoid acting on a reused process ID.
        if !matching_pids(app_id, install_path)?.contains(&pid) {
            continue;
        }
        stop_pid(pid)?;
    }
    let start = Instant::now();
    while !matching_pids(app_id, install_path)?.is_empty() {
        if start.elapsed() >= STOP_TIMEOUT {
            return Err("The game did not exit after the stop request".to_owned());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn linux_matching_pids(app_id: u32) -> Result<Vec<u32>, String> {
    use std::fs;
    use std::io::Read;

    let mut pids = Vec::new();
    for entry in fs::read_dir("/proc")
        .map_err(|error| format!("Could not inspect game processes: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Could not inspect game processes: {error}"))?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == std::process::id() {
            continue;
        }
        let process = entry.path();
        let comm = match fs::read_to_string(process.join("comm")) {
            Ok(name) => name,
            Err(error) if process_disappeared(&error) => continue,
            Err(error) => return Err(format!("Could not inspect game process {pid}: {error}")),
        };
        if is_steam_helper(comm.trim()) {
            continue;
        }
        let mut environ = Vec::new();
        match fs::File::open(process.join("environ")) {
            Ok(file) => match file.take(1024 * 1024).read_to_end(&mut environ) {
                Ok(_) => {}
                Err(error) if process_disappeared(&error) => continue,
                Err(error) => {
                    return Err(format!("Could not inspect game process {pid}: {error}"));
                }
            },
            Err(error) if process_disappeared(&error) => continue,
            Err(error) => return Err(format!("Could not inspect game process {pid}: {error}")),
        };
        if has_steam_app_id(&environ, app_id) {
            let command = match fs::read(process.join("cmdline")) {
                Ok(command) => command,
                Err(error) if process_disappeared(&error) => continue,
                Err(error) => return Err(format!("Could not inspect game process {pid}: {error}")),
            };
            if !is_proton_wrapper(comm.trim(), &command) {
                pids.push(pid);
            }
        }
    }
    Ok(pids)
}

#[cfg(target_os = "linux")]
fn process_disappeared(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
    ) || error.raw_os_error() == Some(3)
}

#[cfg(target_os = "linux")]
fn is_steam_helper(name: &str) -> bool {
    name == "steam"
        || name == "steamwebhelper"
        || name.starts_with("steam-launch")
        || name.starts_with("steam-runtime")
        || name.starts_with("pressure-vessel")
        || matches!(name, "wineserver" | "bwrap" | "reaper")
}

#[cfg(target_os = "linux")]
fn is_proton_wrapper(name: &str, command: &[u8]) -> bool {
    matches!(name, "python" | "python3" | "proton")
        && command
            .split(|byte| *byte == 0)
            .any(|argument| argument.ends_with(b"/proton") || argument.ends_with(b"/proton.py"))
}

#[cfg(target_os = "linux")]
fn has_steam_app_id(environ: &[u8], app_id: u32) -> bool {
    let id = app_id.to_string();
    environ.split(|byte| *byte == 0).any(|entry| {
        entry
            .strip_prefix(b"SteamAppId=")
            .or_else(|| entry.strip_prefix(b"SteamGameId="))
            == Some(id.as_bytes())
    })
}

#[cfg(windows)]
fn windows_matching_pids(install_path: &Path) -> Result<Vec<u32>, String> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process | Select-Object ProcessId,ExecutablePath | ConvertTo-Json -Compress",
        ])
        .output()
        .map_err(|error| format!("Could not inspect game processes: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Could not inspect game processes: {}",
            output.status
        ));
    }
    let processes: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Could not read game process list: {error}"))?;
    let root = install_path
        .to_string_lossy()
        .replace('/', "\\")
        .to_lowercase();
    let prefix = format!("{}\\", root.trim_end_matches('\\'));
    let rows = processes
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_else(|| std::slice::from_ref(&processes));
    Ok(rows
        .iter()
        .filter_map(|process| {
            let path = process.get("ExecutablePath")?.as_str()?.to_lowercase();
            path.starts_with(&prefix)
                .then(|| process.get("ProcessId")?.as_u64()?.try_into().ok())
                .flatten()
        })
        .collect())
}

#[cfg(target_os = "linux")]
fn stop_pid(pid: u32) -> Result<(), String> {
    let status = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .map_err(|error| format!("Could not stop game process {pid}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Could not stop game process {pid}: {status}"))
    }
}

#[cfg(windows)]
fn stop_pid(pid: u32) -> Result<(), String> {
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T"])
        .status()
        .map_err(|error| format!("Could not stop game process {pid}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Could not stop game process {pid}: {status}"))
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn stop_pid(_pid: u32) -> Result<(), String> {
    Err("Stopping games is unsupported on this platform".to_owned())
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    #[test]
    fn steam_app_id_matches_complete_environment_entry() {
        assert!(super::has_steam_app_id(
            b"OTHER=1\0SteamGameId=42\0SteamAppId=420\0",
            42
        ));
        assert!(!super::has_steam_app_id(b"SteamAppId=420\0", 42));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn proton_launcher_is_not_counted_as_game_process() {
        assert!(super::is_proton_wrapper(
            "python3",
            b"python3\0/home/user/Proton/proton\0waitforexitandrun\0game.exe\0"
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stop_terminates_only_the_game_process_with_matching_app_id() {
        use std::path::Path;
        use std::process::Command;
        use std::thread;
        use std::time::{Duration, Instant};

        const TEST_APP_ID: u32 = 4_294_967_294;
        let mut child = Command::new("sleep")
            .arg("30")
            .env("SteamAppId", TEST_APP_ID.to_string())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while !super::matching_pids(TEST_APP_ID, Path::new("/nonexistent"))
            .unwrap()
            .contains(&child.id())
        {
            if started.elapsed() > Duration::from_secs(2) {
                let _ = child.kill();
                panic!("test game process was not detected");
            }
            thread::sleep(Duration::from_millis(20));
        }
        let result = super::stop(TEST_APP_ID, Path::new("/nonexistent"));
        let _ = child.wait();
        assert!(result.is_ok(), "stop failed: {result:?}");
    }
}
