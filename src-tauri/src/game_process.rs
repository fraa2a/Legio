use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

const STOP_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const LAUNCH_TOKEN_ENV: &str = "LEGIO_LAUNCH_TOKEN";

#[derive(Clone)]
pub(crate) enum ProcessTarget {
    Steam {
        app_id: u32,
        #[cfg_attr(target_os = "linux", allow(dead_code))]
        install_path: std::path::PathBuf,
    },
    #[cfg(target_os = "linux")]
    Runner {
        token: String,
        executable_path: std::path::PathBuf,
        launcher_pid: Option<u32>,
    },
    #[cfg(windows)]
    Native {
        game_directory: std::path::PathBuf,
        started_after_ms: i64,
        launcher_pid: Option<u32>,
        known_pids: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<u32>>>,
    },
}

pub(crate) fn matching_pids(target: &ProcessTarget) -> Result<Vec<u32>, String> {
    #[cfg(target_os = "linux")]
    {
        match target {
            ProcessTarget::Steam { app_id, .. } => linux_matching_pids(*app_id),
            ProcessTarget::Runner {
                token,
                executable_path,
                launcher_pid,
            } => linux_runner_matching_pids(token, executable_path, *launcher_pid),
        }
    }
    #[cfg(windows)]
    {
        match target {
            ProcessTarget::Steam { install_path, .. } => windows_matching_pids(install_path),
            ProcessTarget::Native {
                game_directory,
                started_after_ms,
                launcher_pid,
                known_pids,
            } => windows_native_matching_pids(
                game_directory,
                *started_after_ms,
                *launcher_pid,
                known_pids,
            ),
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = target;
        Err("Game process inspection is unsupported on this platform".to_owned())
    }
}

pub(crate) fn stop(target: &ProcessTarget) -> Result<(), String> {
    let pids = matching_pids(target)?;
    if pids.is_empty() {
        return Ok(());
    }
    for pid in pids {
        // Recheck each PID before signaling to avoid acting on a reused process ID.
        if !matching_pids(target)?.contains(&pid) {
            continue;
        }
        #[cfg(windows)]
        if matches!(target, ProcessTarget::Native { .. }) {
            stop_native_pid(pid)?;
            continue;
        }
        stop_pid(pid)?;
    }
    let start = Instant::now();
    while !matching_pids(target)?.is_empty() {
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
            if !is_proton_wrapper(comm.trim().as_bytes(), &command) {
                pids.push(pid);
            }
        }
    }
    Ok(pids)
}

#[cfg(target_os = "linux")]
fn linux_runner_matching_pids(
    token: &str,
    executable_path: &Path,
    launcher_pid: Option<u32>,
) -> Result<Vec<u32>, String> {
    use std::fs;
    use std::io::Read;

    let executable_name = executable_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Game executable name is invalid".to_owned())?;
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
        if pid == std::process::id() || Some(pid) == launcher_pid {
            continue;
        }
        let process = entry.path();
        let comm = match fs::read(process.join("comm")) {
            Ok(name) => trim_process_name(name),
            Err(error) if process_disappeared(&error) => continue,
            Err(error) => return Err(format!("Could not inspect game process {pid}: {error}")),
        };
        if is_wine_launcher(&comm) {
            continue;
        }
        let mut environ = Vec::new();
        match fs::File::open(process.join("environ")) {
            Ok(file) => match file.take(1024 * 1024).read_to_end(&mut environ) {
                Ok(_) => {}
                Err(error) if process_disappeared(&error) => continue,
                Err(error) => return Err(format!("Could not inspect game process {pid}: {error}")),
            },
            Err(error) if process_disappeared(&error) => continue,
            Err(error) => return Err(format!("Could not inspect game process {pid}: {error}")),
        };
        if !has_launch_token(&environ, token) {
            continue;
        }
        let command = match fs::read(process.join("cmdline")) {
            Ok(command) => command,
            Err(error) if process_disappeared(&error) => continue,
            Err(error) => return Err(format!("Could not inspect game process {pid}: {error}")),
        };
        if is_proton_wrapper(&comm, &command) {
            continue;
        }
        if process_names_executable(&comm, &command, executable_name) {
            pids.push(pid);
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
fn is_proton_wrapper(name: &[u8], command: &[u8]) -> bool {
    (name == b"python" || name == b"python3" || name == b"proton")
        && command
            .split(|byte| *byte == 0)
            .any(|argument| argument.ends_with(b"/proton") || argument.ends_with(b"/proton.py"))
}

#[cfg(target_os = "linux")]
fn trim_process_name(mut name: Vec<u8>) -> Vec<u8> {
    while name.last().is_some_and(|byte| byte.is_ascii_whitespace()) {
        name.pop();
    }
    name
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

#[cfg(target_os = "linux")]
fn has_launch_token(environ: &[u8], token: &str) -> bool {
    environ.split(|byte| *byte == 0).any(|entry| {
        entry.strip_prefix(format!("{LAUNCH_TOKEN_ENV}=").as_bytes()) == Some(token.as_bytes())
    })
}

#[cfg(target_os = "linux")]
fn is_wine_launcher(name: &[u8]) -> bool {
    name == b"wine" || name == b"wine64" || name == b"wineserver"
}

#[cfg(target_os = "linux")]
fn process_names_executable(comm: &[u8], command: &[u8], executable_name: &str) -> bool {
    let executable_name = executable_name.as_bytes();
    let truncated = &executable_name[..executable_name.len().min(15)];
    if comm.eq_ignore_ascii_case(executable_name) || comm.eq_ignore_ascii_case(truncated) {
        return true;
    }
    command.split(|byte| *byte == 0).any(|argument| {
        let name = argument
            .rsplit(|byte| *byte == b'/' || *byte == b'\\')
            .next()
            .unwrap_or_default();
        name.eq_ignore_ascii_case(executable_name)
    })
}

#[cfg(windows)]
fn windows_matching_pids(install_path: &Path) -> Result<Vec<u32>, String> {
    let processes = windows_processes()?;
    Ok(windows_pids_from_json(
        &processes,
        &install_path.to_string_lossy(),
    ))
}

#[cfg(windows)]
fn windows_native_matching_pids(
    game_directory: &Path,
    started_after_ms: i64,
    launcher_pid: Option<u32>,
    known_pids: &std::sync::Mutex<std::collections::HashSet<u32>>,
) -> Result<Vec<u32>, String> {
    let processes = windows_processes()?;
    let mut known = known_pids
        .lock()
        .map_err(|_| "Game process tracking is unavailable".to_owned())?;
    Ok(windows_native_pids_from_json(
        &processes,
        &game_directory.to_string_lossy(),
        started_after_ms,
        launcher_pid,
        &mut known,
    ))
}

#[cfg(windows)]
fn windows_processes() -> Result<serde_json::Value, String> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,ExecutablePath,@{Name='StartedAt';Expression={if ($_.CreationDate) {([DateTimeOffset]$_.CreationDate).ToUnixTimeMilliseconds()}}} | ConvertTo-Json -Compress",
        ])
        .output()
        .map_err(|error| format!("Could not inspect game processes: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Could not inspect game processes: {}",
            output.status
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Could not read game process list: {error}"))
}

#[cfg(any(windows, test))]
fn windows_pids_from_json(processes: &serde_json::Value, root: &str) -> Vec<u32> {
    let root = normalize_windows_path(root);
    let prefix = format!("{}\\", root.trim_end_matches('\\'));
    let rows = processes
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_else(|| std::slice::from_ref(processes));
    rows.iter()
        .filter_map(|process| {
            let path = normalize_windows_path(process.get("ExecutablePath")?.as_str()?);
            if !path.starts_with(&prefix) {
                return None;
            }
            process.get("ProcessId")?.as_u64()?.try_into().ok()
        })
        .collect()
}

#[cfg(any(windows, test))]
fn normalize_windows_path(path: &str) -> String {
    let path = path.replace('/', "\\").to_lowercase();
    if let Some(rest) = path.strip_prefix("\\\\?\\unc\\") {
        format!("\\\\{rest}")
    } else {
        path.strip_prefix("\\\\?\\").unwrap_or(&path).to_owned()
    }
}

#[cfg(any(windows, test))]
fn windows_native_pids_from_json(
    processes: &serde_json::Value,
    root: &str,
    started_after_ms: i64,
    launcher_pid: Option<u32>,
    known: &mut std::collections::HashSet<u32>,
) -> Vec<u32> {
    let root = normalize_windows_path(root);
    let prefix = format!("{}\\", root.trim_end_matches('\\'));
    if let Some(pid) = launcher_pid {
        known.insert(pid);
    }
    let rows = processes
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_else(|| std::slice::from_ref(processes));
    let mut matched = std::collections::HashSet::new();
    loop {
        let before = matched.len();
        for process in rows {
            let Some(pid) = process
                .get("ProcessId")
                .and_then(serde_json::Value::as_u64)
                .and_then(|pid| pid.try_into().ok())
            else {
                continue;
            };
            let Some(parent) = process
                .get("ParentProcessId")
                .and_then(serde_json::Value::as_u64)
                .and_then(|pid| pid.try_into().ok())
            else {
                continue;
            };
            let Some(created) = process.get("StartedAt").and_then(serde_json::Value::as_i64) else {
                continue;
            };
            let Some(path) = process
                .get("ExecutablePath")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            if created >= started_after_ms
                && normalize_windows_path(path).starts_with(&prefix)
                && (Some(pid) == launcher_pid
                    || known.contains(&parent)
                    || matched.contains(&parent))
            {
                matched.insert(pid);
            }
        }
        if matched.len() == before {
            break;
        }
    }
    known.extend(&matched);
    let mut result: Vec<_> = matched.into_iter().collect();
    result.sort_unstable();
    result
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

#[cfg(windows)]
fn stop_native_pid(pid: u32) -> Result<(), String> {
    let status = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
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
    #[cfg(windows)]
    #[test]
    fn native_stop_forces_requested_process_to_exit() {
        let mut child = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .spawn()
            .unwrap();
        let result = super::stop_native_pid(child.id());
        if result.is_err() {
            let _ = child.kill();
        }
        result.unwrap();
        assert!(!child.wait().unwrap().success());
    }
    #[test]
    fn windows_native_processes_follow_launcher_ancestry_and_normalize_paths() {
        let root = r"\\?\C:\Games\One";
        let first = serde_json::json!([
            {"ProcessId": 1, "ParentProcessId": 99, "ExecutablePath": "C:\\Games\\One\\old.exe", "StartedAt": 99},
            {"ProcessId": 2, "ParentProcessId": 99, "ExecutablePath": "C:\\Games\\One\\game.exe", "StartedAt": 100},
            {"ProcessId": 3, "ParentProcessId": 99, "ExecutablePath": "C:\\Games\\OneMore\\game.exe", "StartedAt": 200},
            {"ProcessId": 4, "ParentProcessId": 99, "ExecutablePath": "C:\\Games\\One\\unrelated.exe", "StartedAt": 101}
        ]);
        let mut known = std::collections::HashSet::new();
        assert_eq!(
            super::windows_native_pids_from_json(&first, root, 100, Some(2), &mut known),
            vec![2]
        );
        let second = serde_json::json!([
            {"ProcessId": 6, "ParentProcessId": 5, "ExecutablePath": "C:\\Games\\One\\deep.exe", "StartedAt": 106},
            {"ProcessId": 5, "ParentProcessId": 2, "ExecutablePath": "C:\\Games\\One\\child.exe", "StartedAt": 105},
            {"ProcessId": 4, "ParentProcessId": 99, "ExecutablePath": "C:\\Games\\One\\unrelated.exe", "StartedAt": 101}
        ]);
        assert_eq!(
            super::windows_native_pids_from_json(&second, root, 100, Some(2), &mut known),
            vec![5, 6]
        );
        assert_eq!(
            super::normalize_windows_path(r"\\?\UNC\Server\Share\Game"),
            r"\\server\share\game"
        );
        assert_eq!(super::windows_pids_from_json(&first, root), vec![1, 2, 4]);
    }
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
            b"python3",
            b"python3\0/home/user/Proton/proton\0waitforexitandrun\0game.exe\0"
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn runner_process_requires_launch_token_and_game_executable_name() {
        use std::process::Command;
        use std::thread;
        use std::time::{Duration, Instant};

        let token = uuid::Uuid::new_v4().to_string();
        let executable = std::env::temp_dir().join(format!(
            "legio-runner-target-{}-{}.exe",
            std::process::id(),
            token
        ));
        std::fs::write(&executable, b"stub").unwrap();
        let executable = std::fs::canonicalize(executable).unwrap();
        let executable_name = executable.file_name().unwrap().to_string_lossy();
        let mut game = Command::new("bash")
            .args([
                "-c",
                "exec -a \"$1\" sleep 30",
                "runner-test",
                executable_name.as_ref(),
            ])
            .env(super::LAUNCH_TOKEN_ENV, &token)
            .spawn()
            .unwrap();
        let target = super::ProcessTarget::Runner {
            token: token.clone(),
            executable_path: executable.clone(),
            launcher_pid: None,
        };
        let started = Instant::now();
        while !super::matching_pids(&target).unwrap().contains(&game.id()) {
            if started.elapsed() > Duration::from_secs(2) {
                let _ = game.kill();
                panic!("runner game process was not detected");
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(super::stop(&target).is_ok());
        let _ = game.wait();

        let mut helper = Command::new("bash")
            .args(["-c", "exec -a other.exe sleep 30"])
            .env(super::LAUNCH_TOKEN_ENV, token)
            .spawn()
            .unwrap();
        assert!(super::matching_pids(&target).unwrap().is_empty());
        helper.kill().unwrap();
        helper.wait().unwrap();
        std::fs::remove_file(executable).unwrap();
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
        let target = super::ProcessTarget::Steam {
            app_id: TEST_APP_ID,
            install_path: Path::new("/nonexistent").to_path_buf(),
        };
        while !super::matching_pids(&target).unwrap().contains(&child.id()) {
            if started.elapsed() > Duration::from_secs(2) {
                let _ = child.kill();
                panic!("test game process was not detected");
            }
            thread::sleep(Duration::from_millis(20));
        }
        let result = super::stop(&target);
        let _ = child.wait();
        assert!(result.is_ok(), "stop failed: {result:?}");
    }
}
