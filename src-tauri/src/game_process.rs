use std::path::Path;
#[cfg(windows)]
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

const STOP_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const LAUNCH_TOKEN_ENV: &str = "LEGIO_LAUNCH_TOKEN";

#[cfg(any(windows, test))]
#[derive(Default)]
pub(crate) struct NativeProcessHistory {
    initialized: bool,
    identities: std::collections::HashMap<u32, i64>,
}

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
        installation_root: Option<std::path::PathBuf>,
        launcher_pid: Option<u32>,
    },
    #[cfg(windows)]
    Native {
        game_directory: std::path::PathBuf,
        started_after_ms: i64,
        launcher_pid: Option<u32>,
        known_pids: std::sync::Arc<std::sync::Mutex<NativeProcessHistory>>,
    },
}

#[cfg(any(windows, test))]
pub(crate) fn matching_pids(target: &ProcessTarget) -> Result<Vec<u32>, String> {
    #[cfg(target_os = "linux")]
    {
        linux_discover(target).map(|pids| pids.into_iter().map(|(pid, _)| pid).collect())
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
}

pub(crate) fn stop(target: &ProcessTarget) -> Result<(), String> {
    let mut monitor = ProcessMonitor::new();
    let pids = monitor.matching_pids(target)?;
    if pids.is_empty() {
        return Ok(());
    }
    for pid in pids {
        // Recheck each PID before signaling to avoid acting on a reused process ID.
        if !monitor.matching_pids(target)?.contains(&pid) {
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
    while !monitor.matching_pids(target)?.is_empty() {
        if start.elapsed() >= STOP_TIMEOUT {
            return Err("The game did not exit after the stop request".to_owned());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

pub(crate) struct ProcessMonitor {
    #[cfg(target_os = "linux")]
    known: Vec<(u32, u64)>,
    #[cfg(target_os = "linux")]
    discovered_at: Option<Instant>,
}

impl ProcessMonitor {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            known: Vec::new(),
            #[cfg(target_os = "linux")]
            discovered_at: None,
        }
    }

    pub(crate) fn matching_pids(&mut self, target: &ProcessTarget) -> Result<Vec<u32>, String> {
        #[cfg(target_os = "linux")]
        {
            let mut live = Vec::with_capacity(self.known.len());
            for &(pid, started_at) in &self.known {
                if linux_start_time(pid)? == Some(started_at) && linux_matches(target, pid)? {
                    live.push((pid, started_at));
                }
            }
            let discover = live.is_empty()
                || self
                    .discovered_at
                    .is_none_or(|last| last.elapsed() >= Duration::from_secs(3));
            self.known = live;
            if discover {
                self.known = linux_discover(target)?;
                self.discovered_at = Some(Instant::now());
            }
            Ok(self.known.iter().map(|&(pid, _)| pid).collect())
        }
        #[cfg(windows)]
        matching_pids(target)
    }
}

#[cfg(target_os = "linux")]
fn linux_discover(target: &ProcessTarget) -> Result<Vec<(u32, u64)>, String> {
    let mut pids = Vec::new();
    for entry in std::fs::read_dir("/proc")
        .map_err(|error| format!("Could not inspect game processes: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Could not inspect game processes: {error}"))?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse().ok())
        else {
            continue;
        };
        // Identity is captured before reading the process attributes.
        let Some(started_at) = linux_start_time(pid)? else {
            continue;
        };
        if linux_matches(target, pid)? && linux_start_time(pid)? == Some(started_at) {
            pids.push((pid, started_at));
        }
    }
    Ok(pids)
}

#[cfg(target_os = "linux")]
fn linux_start_time(pid: u32) -> Result<Option<u64>, String> {
    let Some(stat) = read_linux_process_bytes(
        pid,
        &Path::new("/proc").join(pid.to_string()).join("stat"),
        4096,
    )?
    else {
        return Ok(None);
    };
    Ok(std::str::from_utf8(&stat)
        .ok()
        .and_then(|stat| stat.rsplit_once(')'))
        .and_then(|(_, fields)| {
            let state = fields.split_whitespace().next()?;
            if matches!(state, "Z" | "X") {
                None
            } else {
                fields.split_whitespace().nth(19)
            }
        })
        .and_then(|value| value.parse().ok()))
}

#[cfg(target_os = "linux")]
fn linux_matches(target: &ProcessTarget, pid: u32) -> Result<bool, String> {
    if pid == std::process::id() {
        return Ok(false);
    }
    let process = Path::new("/proc").join(pid.to_string());
    let Some(comm) = read_linux_process_bytes(pid, &process.join("comm"), 4096)? else {
        return Ok(false);
    };
    let comm = trim_process_name(comm);
    if is_steam_helper(&String::from_utf8_lossy(&comm)) || is_wine_launcher(&comm) {
        return Ok(false);
    }
    let Some(environ) = read_linux_process_bytes(pid, &process.join("environ"), 1024 * 1024)?
    else {
        return Ok(false);
    };
    let matches = match target {
        ProcessTarget::Steam { app_id, .. } => has_steam_app_id(&environ, *app_id),
        ProcessTarget::Runner { token, .. } => has_launch_token(&environ, token),
    };
    if !matches {
        return Ok(false);
    }
    let Some(command) = read_linux_process_bytes(pid, &process.join("cmdline"), 1024 * 1024)?
    else {
        return Ok(false);
    };
    if is_proton_wrapper(&comm, &command) {
        return Ok(false);
    }
    match target {
        ProcessTarget::Steam { .. } => Ok(true),
        ProcessTarget::Runner {
            executable_path,
            installation_root,
            launcher_pid,
            ..
        } => {
            let name = executable_path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "Game executable name is invalid".to_owned())?;
            let arguments: Vec<&[u8]> = command
                .split(|byte| *byte == 0)
                .filter(|argument| !argument.is_empty())
                .collect();
            let direct = process_names_executable(
                &comm,
                arguments.first().copied().unwrap_or_default(),
                name,
            );
            let script = arguments.get(1).is_some_and(|argument| {
                *argument == executable_path.as_os_str().as_encoded_bytes()
            });
            if Some(pid) == *launcher_pid {
                return Ok(direct || script);
            }
            if direct || script {
                return Ok(true);
            }
            let interpreter = arguments.first().is_some_and(|argument| {
                matches!(
                    argument
                        .rsplit(|byte| *byte == b'/')
                        .next()
                        .unwrap_or_default(),
                    b"sh" | b"bash" | b"dash" | b"python" | b"python3"
                )
            });
            if interpreter {
                return Ok(false);
            }
            let root = installation_root
                .as_deref()
                .or_else(|| executable_path.parent())
                .ok_or("Game executable has no directory")?;
            let named_game = arguments
                .first()
                .is_some_and(|argument| argument.to_ascii_lowercase().ends_with(b".exe"))
                || comm.to_ascii_lowercase().ends_with(b".exe");
            let owned_working_directory = named_game
                && std::fs::read_link(process.join("cwd")).is_ok_and(|cwd| cwd.starts_with(root));
            Ok(owned_working_directory
                || arguments.iter().any(|argument| {
                    let Ok(argument) = std::str::from_utf8(argument) else {
                        return false;
                    };
                    let path = Path::new(argument);
                    path.starts_with(root)
                        && path
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
                }))
        }
    }
}

#[cfg(target_os = "linux")]
fn read_linux_process_bytes(pid: u32, path: &Path, limit: u64) -> Result<Option<Vec<u8>>, String> {
    use std::fs;
    use std::io::Read;

    let mut bytes = Vec::new();
    match fs::File::open(path).and_then(|file| file.take(limit).read_to_end(&mut bytes)) {
        Ok(_) => Ok(Some(bytes)),
        Err(error) if process_disappeared(&error) => Ok(None),
        Err(error) => Err(format!("Could not inspect game process {pid}: {error}")),
    }
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
        || matches!(
            name,
            "wineserver"
                | "bwrap"
                | "reaper"
                | "wineboot.exe"
                | "winedevice.exe"
                | "services.exe"
                | "explorer.exe"
                | "rpcss.exe"
                | "plugplay.exe"
                | "conhost.exe"
                | "svchost.exe"
        )
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
    environ
        .split(|byte| *byte == 0)
        .any(|entry| entry.strip_prefix(b"LEGIO_LAUNCH_TOKEN=") == Some(token.as_bytes()))
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
    Ok(windows_pids(&processes, &install_path.to_string_lossy()))
}

#[cfg(windows)]
fn windows_native_matching_pids(
    game_directory: &Path,
    started_after_ms: i64,
    launcher_pid: Option<u32>,
    known_pids: &std::sync::Mutex<NativeProcessHistory>,
) -> Result<Vec<u32>, String> {
    let processes = windows_processes()?;
    let mut known = known_pids
        .lock()
        .map_err(|_| "Game process tracking is unavailable".to_owned())?;
    Ok(windows_native_pids(
        &processes,
        &game_directory.to_string_lossy(),
        started_after_ms,
        launcher_pid,
        &mut known,
    ))
}

#[cfg(any(windows, test))]
struct WindowsProcess {
    pid: u32,
    parent: u32,
    executable: String,
    started_at: i64,
}

#[cfg(windows)]
struct ProcessHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: this handle was opened successfully and is owned by this guard.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
fn windows_processes() -> Result<Vec<WindowsProcess>, String> {
    use windows_sys::Win32::Foundation::{ERROR_NO_MORE_FILES, FILETIME, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    // SAFETY: the snapshot flag requests process entries, without pointer arguments.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(format!(
            "Could not inspect game processes: {}",
            std::io::Error::last_os_error()
        ));
    }
    let snapshot = ProcessHandle(raw);
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut path = vec![0_u16; 32768];
    let mut processes = Vec::new();
    // SAFETY: the snapshot is live and entry has the required size and writable storage.
    let mut found = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    while found != 0 {
        // SAFETY: this requests query access only, without handle inheritance.
        let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID) };
        if !raw.is_null() {
            let process = ProcessHandle(raw);
            let mut size = path.len() as u32;
            let mut created = FILETIME::default();
            let mut exited = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            // SAFETY: the process handle is live; all buffers and sizes are writable and bounded.
            let readable = unsafe {
                QueryFullProcessImageNameW(process.0, 0, path.as_mut_ptr(), &mut size) != 0
                    && GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user)
                        != 0
            };
            if readable {
                let ticks =
                    (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
                let started_at = ticks
                    .checked_sub(116_444_736_000_000_000)
                    .and_then(|ticks| i64::try_from(ticks / 10_000).ok());
                if let Some(started_at) = started_at {
                    processes.push(WindowsProcess {
                        pid: entry.th32ProcessID,
                        parent: entry.th32ParentProcessID,
                        executable: String::from_utf16_lossy(&path[..size as usize]),
                        started_at,
                    });
                }
            }
        }
        // SAFETY: the snapshot and correctly sized entry remain live for iteration.
        found = unsafe { Process32NextW(snapshot.0, &mut entry) };
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
        return Err(format!("Could not enumerate game processes: {error}"));
    }
    Ok(processes)
}

#[cfg(any(windows, test))]
fn windows_pids(processes: &[WindowsProcess], root: &str) -> Vec<u32> {
    let root = normalize_windows_path(root);
    let prefix = format!("{}\\", root.trim_end_matches('\\'));
    processes
        .iter()
        .filter(|process| normalize_windows_path(&process.executable).starts_with(&prefix))
        .map(|process| process.pid)
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
fn windows_native_pids(
    processes: &[WindowsProcess],
    root: &str,
    started_after_ms: i64,
    launcher_pid: Option<u32>,
    known: &mut NativeProcessHistory,
) -> Vec<u32> {
    let root = normalize_windows_path(root);
    let prefix = format!("{}\\", root.trim_end_matches('\\'));
    known.identities.retain(|pid, started| {
        !processes
            .iter()
            .any(|process| process.pid == *pid && process.started_at != *started)
    });
    if !known.initialized {
        known.initialized = true;
        if let Some(process) = processes.iter().find(|process| {
            Some(process.pid) == launcher_pid && process.started_at >= started_after_ms
        }) {
            known.identities.insert(process.pid, process.started_at);
        }
    }
    let mut matched = std::collections::HashSet::new();
    loop {
        let before = matched.len();
        for process in processes {
            if process.started_at >= started_after_ms
                && normalize_windows_path(&process.executable).starts_with(&prefix)
                && (known.identities.get(&process.pid) == Some(&process.started_at)
                    || known
                        .identities
                        .get(&process.parent)
                        .is_some_and(|started| process.started_at >= *started)
                    || matched.contains(&process.parent))
            {
                matched.insert(process.pid);
            }
        }
        if matched.len() == before {
            break;
        }
    }
    for process in processes
        .iter()
        .filter(|process| matched.contains(&process.pid))
    {
        known.identities.insert(process.pid, process.started_at);
    }
    let mut result: Vec<_> = matched.into_iter().collect();
    result.sort_unstable();
    result
}

#[cfg(target_os = "linux")]
fn stop_pid(pid: u32) -> Result<(), String> {
    let pid = i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| "Game process ID is invalid".to_owned())?;
    // SAFETY: the positive PID targets one process and SIGTERM is a valid signal.
    if unsafe { libc::kill(pid, libc::SIGTERM) } == 0 {
        Ok(())
    } else {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(format!("Could not stop game process {pid}: {error}"))
        }
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

#[cfg(test)]
mod tests {
    fn test_processes(value: serde_json::Value) -> Vec<super::WindowsProcess> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|process| super::WindowsProcess {
                pid: process["ProcessId"].as_u64().unwrap() as u32,
                parent: process["ParentProcessId"].as_u64().unwrap() as u32,
                executable: process["ExecutablePath"].as_str().unwrap().to_owned(),
                started_at: process["StartedAt"].as_i64().unwrap(),
            })
            .collect()
    }

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
        let first = test_processes(serde_json::json!([
            {"ProcessId": 1, "ParentProcessId": 99, "ExecutablePath": "C:\\Games\\One\\old.exe", "StartedAt": 99},
            {"ProcessId": 2, "ParentProcessId": 99, "ExecutablePath": "C:\\Games\\One\\game.exe", "StartedAt": 100},
            {"ProcessId": 3, "ParentProcessId": 99, "ExecutablePath": "C:\\Games\\OneMore\\game.exe", "StartedAt": 200},
            {"ProcessId": 4, "ParentProcessId": 99, "ExecutablePath": "C:\\Games\\One\\unrelated.exe", "StartedAt": 101}
        ]));
        let mut known = super::NativeProcessHistory::default();
        assert_eq!(
            super::windows_native_pids(&first, root, 100, Some(2), &mut known),
            vec![2]
        );
        let second = test_processes(serde_json::json!([
            {"ProcessId": 6, "ParentProcessId": 5, "ExecutablePath": "C:\\Games\\One\\deep.exe", "StartedAt": 106},
            {"ProcessId": 5, "ParentProcessId": 2, "ExecutablePath": "C:\\Games\\One\\child.exe", "StartedAt": 105},
            {"ProcessId": 4, "ParentProcessId": 99, "ExecutablePath": "C:\\Games\\One\\unrelated.exe", "StartedAt": 101}
        ]));
        assert_eq!(
            super::windows_native_pids(&second, root, 100, Some(2), &mut known),
            vec![5, 6]
        );
        assert_eq!(
            super::normalize_windows_path(r"\\?\UNC\Server\Share\Game"),
            r"\\server\share\game"
        );
        assert_eq!(super::windows_pids(&first, root), vec![1, 2, 4]);
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
    fn process_reader_limits_bytes_and_skips_disappeared_files() {
        let pid = std::process::id();
        let command =
            super::read_linux_process_bytes(pid, std::path::Path::new("/proc/self/cmdline"), 1)
                .unwrap()
                .unwrap();
        assert_eq!(command.len(), 1);
        assert!(
            super::read_linux_process_bytes(
                pid,
                std::path::Path::new("/proc/self/no-such-file"),
                1
            )
            .unwrap()
            .is_none()
        );
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
            installation_root: None,
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
    fn runner_exec_and_different_executable_handoff_are_tracked() {
        use std::process::Command;
        use std::time::{Duration, Instant};
        let token = uuid::Uuid::new_v4().to_string();
        let root = std::env::temp_dir().join(format!("legio-handoff-{token}"));
        std::fs::create_dir(&root).unwrap();
        for (name, launcher) in [("Launcher.exe", true), ("ActualGame.exe", false)] {
            let path = root.join(name);
            let mut child = Command::new("bash")
                .args([
                    "-c",
                    "exec -a \"$1\" sleep 30",
                    "test",
                    path.to_str().unwrap(),
                ])
                .env(super::LAUNCH_TOKEN_ENV, &token)
                .spawn()
                .unwrap();
            let target = super::ProcessTarget::Runner {
                token: token.clone(),
                executable_path: root.join("Launcher.exe"),
                installation_root: Some(root.clone()),
                launcher_pid: launcher.then_some(child.id()),
            };
            let started = Instant::now();
            let mut found = false;
            while started.elapsed() < Duration::from_secs(2) {
                if super::matching_pids(&target).unwrap().contains(&child.id()) {
                    found = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            child.kill().unwrap();
            child.wait().unwrap();
            assert!(found, "{name} was not tracked");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reused_windows_parent_pid_does_not_adopt_foreign_processes() {
        let mut history = super::NativeProcessHistory::default();
        let initial = test_processes(
            serde_json::json!([{ "ProcessId": 2, "ParentProcessId": 99, "ExecutablePath": "C:\\Games\\One\\game.exe", "StartedAt": 100 }]),
        );
        assert_eq!(
            super::windows_native_pids(&initial, r"C:\Games\One", 100, Some(2), &mut history),
            vec![2]
        );
        let reused = test_processes(serde_json::json!([
            { "ProcessId": 2, "ParentProcessId": 99, "ExecutablePath": "C:\\Other\\foreign.exe", "StartedAt": 200 },
            { "ProcessId": 3, "ParentProcessId": 2, "ExecutablePath": "C:\\Games\\One\\unrelated.exe", "StartedAt": 201 }
        ]));
        assert!(
            super::windows_native_pids(&reused, r"C:\Games\One", 100, Some(2), &mut history)
                .is_empty()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn monitor_reuses_live_identity_and_rediscovers_after_exit() {
        let app_id = 4_294_967_280;
        let target = super::ProcessTarget::Steam {
            app_id,
            install_path: std::path::PathBuf::new(),
        };
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .env("SteamAppId", app_id.to_string())
            .spawn()
            .unwrap();
        let mut monitor = super::ProcessMonitor::new();
        assert_eq!(monitor.matching_pids(&target).unwrap(), vec![child.id()]);
        let first_scan = monitor.discovered_at;
        assert_eq!(monitor.matching_pids(&target).unwrap(), vec![child.id()]);
        assert_eq!(monitor.discovered_at, first_scan);
        child.kill().unwrap();
        child.wait().unwrap();
        let mut replacement = std::process::Command::new("sleep")
            .arg("30")
            .env("SteamAppId", app_id.to_string())
            .spawn()
            .unwrap();
        let result = monitor.matching_pids(&target);
        replacement.kill().unwrap();
        replacement.wait().unwrap();
        assert_eq!(result.unwrap(), vec![replacement.id()]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stop_terminates_only_the_game_process_with_matching_app_id() {
        use std::path::Path;
        use std::process::{Child, Command};
        use std::thread;
        use std::time::{Duration, Instant};

        struct ChildGuard(Child);

        impl Drop for ChildGuard {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }

        const TEST_APP_ID: u32 = 4_294_967_294;
        const PROTECTED_APP_ID: u32 = 4_294_967_279;
        let mut child = ChildGuard(
            Command::new("sleep")
                .arg("30")
                .env("SteamAppId", TEST_APP_ID.to_string())
                .env("SteamGameId", TEST_APP_ID.to_string())
                .spawn()
                .unwrap(),
        );
        let mut protected = ChildGuard(
            Command::new("sleep")
                .arg("30")
                .env("SteamAppId", PROTECTED_APP_ID.to_string())
                .env("SteamGameId", PROTECTED_APP_ID.to_string())
                .spawn()
                .unwrap(),
        );
        let target = super::ProcessTarget::Steam {
            app_id: TEST_APP_ID,
            install_path: Path::new("/nonexistent").to_path_buf(),
        };
        let protected_target = super::ProcessTarget::Steam {
            app_id: PROTECTED_APP_ID,
            install_path: Path::new("/nonexistent").to_path_buf(),
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let target_pids = super::matching_pids(&target).unwrap();
            let protected_pids = super::matching_pids(&protected_target).unwrap();
            if target_pids.contains(&child.0.id()) && protected_pids.contains(&protected.0.id()) {
                assert!(!target_pids.contains(&protected.0.id()));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "test processes were not detected"
            );
            thread::sleep(Duration::from_millis(20));
        }

        let deadline = Instant::now() + Duration::from_secs(2);
        let result = super::stop(&target);
        assert!(result.is_ok(), "stop failed: {result:?}");
        loop {
            assert!(Instant::now() < deadline, "target did not exit promptly");
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(
                    !status.success(),
                    "target exited naturally instead of being stopped"
                );
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(protected.0.try_wait().unwrap().is_none());
    }
}
