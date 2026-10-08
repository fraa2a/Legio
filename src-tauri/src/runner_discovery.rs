#[cfg(target_os = "linux")]
use std::collections::HashSet;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

use serde::Serialize;

#[cfg(target_os = "linux")]
use crate::steam_local;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerKind {
    Proton,
    GeProton,
    Wine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledRunner {
    pub kind: RunnerKind,
    pub name: String,
    pub version: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerDiscovery {
    pub ntsync_available: bool,
    pub game_mode_available: bool,
    pub gamescope_available: bool,
    pub runners: Vec<InstalledRunner>,
    pub diagnostics: Vec<String>,
}

#[cfg(target_os = "linux")]
pub(crate) fn resolve_runner(path: &str) -> Result<InstalledRunner, String> {
    let requested = fs::canonicalize(path)
        .map_err(|error| format!("Could not resolve selected compatibility runner: {error}"))?;
    if let Some((kind, name)) = classify_proton_dir(&requested) {
        return Ok(InstalledRunner {
            kind,
            version: read_proton_version(&requested, &name),
            name,
            path: requested.to_string_lossy().into_owned(),
        });
    }
    if !is_executable_file(&requested) {
        return Err("Selected compatibility runner is no longer available".to_owned());
    }
    let version = wine_version(&requested, WINE_PROBE_TIMEOUT)
        .map_err(|error| format!("Selected Wine runner is unavailable: {error}"))?;
    Ok(InstalledRunner {
        kind: RunnerKind::Wine,
        name: "Wine".to_owned(),
        version,
        path: requested.to_string_lossy().into_owned(),
    })
}

#[cfg(target_os = "linux")]
pub(crate) fn launch_command(
    runner: &InstalledRunner,
    executable: &Path,
    arguments_before: &[String],
    arguments_after: &[String],
    umu: Option<&Path>,
) -> Result<Command, String> {
    let mut command = match runner.kind {
        RunnerKind::Proton | RunnerKind::GeProton => {
            let mut command = Command::new(umu.ok_or("Proton launch requires umu-run")?);
            command
                .env("PROTONPATH", &runner.path)
                .env("GAMEID", "umu-default");
            command
        }
        RunnerKind::Wine => Command::new(&runner.path),
    };
    command
        .args(arguments_before)
        .arg(executable)
        .args(arguments_after);
    crate::launch_environment::apply(&mut command);
    Ok(command)
}

#[cfg(target_os = "linux")]
pub(crate) fn umu_path() -> Result<PathBuf, String> {
    executable_path("umu-run").ok_or_else(|| {
        "Install umu-launcher to run Proton games without Online Fix, then restart Legio".to_owned()
    })
}

#[cfg(target_os = "linux")]
pub(crate) fn executable_path(name: &str) -> Option<PathBuf> {
    let search_path =
        crate::launch_environment::host_path(&std::env::var_os("PATH").unwrap_or_default());
    let directories = std::env::split_paths(&search_path)
        .chain(std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/bin")));
    for directory in directories {
        let path = directory.join(name);
        if directory.is_absolute()
            && is_executable_file(&path)
            && let Ok(path) = fs::canonicalize(path)
        {
            return Some(path);
        }
    }
    None
}

pub fn discover() -> RunnerDiscovery {
    #[cfg(target_os = "linux")]
    {
        discover_linux()
    }
    #[cfg(not(target_os = "linux"))]
    {
        RunnerDiscovery {
            ntsync_available: false,
            game_mode_available: false,
            gamescope_available: false,
            runners: Vec::new(),
            diagnostics: vec!["Runner discovery is currently supported on Linux only".to_owned()],
        }
    }
}

#[cfg(target_os = "linux")]
fn discover_linux() -> RunnerDiscovery {
    let mut runners = discover_proton();
    let (wine, mut diagnostics) = discover_wine_from(
        std::env::split_paths(&crate::launch_environment::host_path(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
        WINE_PROBE_TIMEOUT,
    );
    runners.extend(wine);
    if runners.iter().any(|runner| runner.kind != RunnerKind::Wine)
        && let Err(error) = umu_path()
    {
        diagnostics.push(error);
    }
    RunnerDiscovery {
        ntsync_available: fs::File::open("/dev/ntsync").is_ok(),
        game_mode_available: executable_path("gamemoderun").is_some(),
        gamescope_available: executable_path("gamescope").is_some(),
        runners,
        diagnostics,
    }
}

#[cfg(target_os = "linux")]
fn discover_proton() -> Vec<InstalledRunner> {
    let mut directories = vec![PathBuf::from("/usr/share/steam/compatibilitytools.d")];
    for library in steam_local::default_steam_library_paths() {
        directories.push(library.join("compatibilitytools.d"));
        directories.push(library.join("steamapps/common"));
    }

    let mut runners = Vec::new();
    let mut seen_runners = HashSet::new();
    for directory in directories {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let Some((kind, name)) = classify_proton_dir(&path) else {
                continue;
            };
            let Ok(path) = fs::canonicalize(path) else {
                continue;
            };
            if !seen_runners.insert(path.clone()) {
                continue;
            }
            runners.push(InstalledRunner {
                kind,
                name: name.clone(),
                version: read_proton_version(&path, &name),
                path: path.to_string_lossy().into_owned(),
            });
        }
    }
    runners
}

#[cfg(target_os = "linux")]
fn classify_proton_dir(path: &Path) -> Option<(RunnerKind, String)> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_dir() {
        return None;
    }
    let name = path.file_name()?.to_str()?.to_owned();
    let lower = name.to_ascii_lowercase();
    let kind = if lower.starts_with("ge-proton") {
        RunnerKind::GeProton
    } else if lower.contains("proton") {
        RunnerKind::Proton
    } else {
        return None;
    };
    if !is_executable_file(&path.join("proton")) {
        return None;
    }
    Some((kind, name))
}

#[cfg(target_os = "linux")]
fn read_proton_version(path: &Path, name: &str) -> String {
    let version_file = path.join("version");
    if let Ok(metadata) = fs::symlink_metadata(&version_file)
        && metadata.is_file()
        && metadata.len() <= 256
        && let Ok(version) = fs::read_to_string(version_file)
    {
        let version = version.trim();
        if !version.is_empty() && !version.chars().any(char::is_control) {
            return version.to_owned();
        }
    }
    name.strip_prefix("GE-Proton")
        .or_else(|| name.strip_prefix("Proton"))
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .unwrap_or(name)
        .to_owned()
}

#[cfg(target_os = "linux")]
const WINE_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(target_os = "linux")]
fn discover_wine_from(
    directories: impl IntoIterator<Item = std::path::PathBuf>,
    timeout: Duration,
) -> (Option<InstalledRunner>, Vec<String>) {
    let mut diagnostics = Vec::new();
    for directory in directories {
        for binary in ["wine", "wine64"] {
            let path = directory.join(binary);
            if !is_executable_file(&path) {
                continue;
            }
            let canonical = match fs::canonicalize(&path) {
                Ok(canonical) => canonical,
                Err(_) => {
                    diagnostics.push(format!(
                        "Skipped Wine candidate {binary}: could not resolve path"
                    ));
                    continue;
                }
            };
            match wine_version(&canonical, timeout) {
                Ok(version) => {
                    return (
                        Some(InstalledRunner {
                            kind: RunnerKind::Wine,
                            name: "Wine".to_owned(),
                            version,
                            path: canonical.to_string_lossy().into_owned(),
                        }),
                        diagnostics,
                    );
                }
                Err(reason) => {
                    diagnostics.push(format!("Skipped Wine candidate {binary}: {reason}"))
                }
            }
        }
    }
    (None, diagnostics)
}

#[cfg(target_os = "linux")]
fn wine_version(path: &Path, timeout: Duration) -> Result<String, &'static str> {
    let mut command = Command::new(path);
    crate::launch_environment::apply(&mut command);
    let mut child = command
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "could not start executable")?;
    let result = (|| {
        let mut stdout = child.stdout.take().ok_or("version output is unavailable")?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or("version error output is unavailable")?;
        nonblocking(&stdout)?;
        nonblocking(&stderr)?;
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let deadline = Instant::now() + timeout;
        let status = loop {
            let status = child
                .try_wait()
                .map_err(|_| "could not wait for executable")?;
            read_version_output(&mut stdout, &mut output)?;
            read_version_output(&mut stderr, &mut errors)?;
            if output.len() + errors.len() > 4096 {
                return Err("version output exceeds the size limit");
            }
            if let Some(status) = status {
                break status;
            }
            if Instant::now() >= deadline {
                return Err("version check timed out");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if !status.success() {
            return Err("version check exited unsuccessfully");
        }
        let version_output = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output),
            String::from_utf8_lossy(&errors)
        );
        version_output
            .lines()
            .map(str::trim)
            .find(|line| is_wine_version(line))
            .map(str::to_owned)
            .ok_or("version output was not recognized")
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

#[cfg(target_os = "linux")]
fn nonblocking(pipe: &impl std::os::fd::AsRawFd) -> Result<(), &'static str> {
    let fd = pipe.as_raw_fd();
    // SAFETY: fd is borrowed from a live child pipe; these commands read and set integer flags.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err("could not configure version output");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn read_version_output(
    pipe: &mut impl std::io::Read,
    output: &mut Vec<u8>,
) -> Result<(), &'static str> {
    let mut buffer = [0_u8; 1024];
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(size) => {
                if output.len() + size > 4096 {
                    return Err("version output exceeds the size limit");
                }
                output.extend_from_slice(&buffer[..size]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err("could not read version output"),
        }
    }
}

#[cfg(target_os = "linux")]
fn is_wine_version(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("wine-") || lower.starts_with("wine version ")
}

#[cfg(target_os = "linux")]
fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[tauri::command]
pub async fn list_compatibility_runners() -> Result<RunnerDiscovery, String> {
    tauri::async_runtime::spawn_blocking(discover)
        .await
        .map_err(|error| format!("Runner discovery task failed: {error}"))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("legio-runners-{}", uuid::Uuid::new_v4()))
    }

    fn make_runner(parent: &Path, name: &str, version: Option<&str>) -> PathBuf {
        let path = parent.join(name);
        fs::create_dir_all(&path).unwrap();
        let executable = path.join("proton");
        fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        if let Some(version) = version {
            fs::write(path.join("version"), version).unwrap();
        }
        path
    }

    fn make_executable(parent: &Path, name: &str, contents: &str) -> PathBuf {
        let path = parent.join(name);
        fs::write(&path, contents).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn launch_command_keeps_runner_arguments_structured() {
        let runner = InstalledRunner {
            kind: RunnerKind::Wine,
            name: "Wine".to_owned(),
            version: "9.0".to_owned(),
            path: "/usr/bin/wine".to_owned(),
        };
        let before = vec!["-windowed".to_owned()];
        let after = vec!["-safe".to_owned(), "two words".to_owned()];
        let command =
            launch_command(&runner, Path::new("/games/game.exe"), &before, &after, None).unwrap();
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            arguments,
            ["-windowed", "/games/game.exe", "-safe", "two words"]
        );
    }

    #[test]
    fn proton_command_uses_umu_and_preserves_structured_arguments() {
        let runner = InstalledRunner {
            kind: RunnerKind::GeProton,
            name: "GE-Proton".to_owned(),
            version: "11".to_owned(),
            path: "/Proton Tools/GE-Proton".to_owned(),
        };
        let command = launch_command(
            &runner,
            Path::new("/games/Windows Game/game.exe"),
            &["before".to_owned()],
            &["two words".to_owned()],
            Some(Path::new("/tools/umu-run")),
        )
        .unwrap();
        assert_eq!(command.get_program(), "/tools/umu-run");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["before", "/games/Windows Game/game.exe", "two words"]
        );
        assert!(
            command.get_envs().any(|(key, value)| key == "PROTONPATH"
                && value == Some(std::ffi::OsStr::new(&runner.path)))
        );
        assert!(launch_command(&runner, Path::new("/game.exe"), &[], &[], None).is_err());
    }

    #[test]
    fn recognizes_only_named_proton_layouts_with_executable_entrypoint() {
        let root = temp_dir();
        let tools = root.join("compatibilitytools.d");
        fs::create_dir_all(&tools).unwrap();
        let ge = make_runner(&tools, "GE-Proton9-1", Some("GE-Proton9-1-custom\n"));
        let proton = make_runner(&tools, "Proton 9.0", None);
        let fake_game = make_runner(&tools, "Proton Game", None);
        fs::remove_file(fake_game.join("proton")).unwrap();
        let unrelated = make_runner(&tools, "OtherTool", None);

        let mut entries = fs::read_dir(&tools)
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        let found = entries
            .iter()
            .filter_map(|entry| classify_proton_dir(&entry.path()))
            .collect::<Vec<_>>();

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, RunnerKind::GeProton);
        assert_eq!(found[1].0, RunnerKind::Proton);
        assert!(ge.exists() && proton.exists() && unrelated.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uses_version_file_then_directory_version() {
        let root = temp_dir();
        fs::create_dir_all(&root).unwrap();
        let ge = make_runner(&root, "GE-Proton9-2", Some("9.2-custom\n"));
        let proton = make_runner(&root, "Proton 8.0", None);

        assert_eq!(read_proton_version(&ge, "GE-Proton9-2"), "9.2-custom");
        assert_eq!(read_proton_version(&proton, "Proton 8.0"), "8.0");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selected_runner_is_validated_directly_and_noisy_version_output_is_bounded() {
        let root = temp_dir();
        fs::create_dir_all(&root).unwrap();
        let path = make_runner(&root, "GE-Proton11-1", Some("11-1"));
        assert_eq!(
            resolve_runner(path.to_str().unwrap()).unwrap().kind,
            RunnerKind::GeProton
        );
        let noisy = make_executable(&root, "noisy-wine", "#!/bin/sh\nhead -c 65536 /dev/zero\n");
        let started = Instant::now();
        assert_eq!(
            wine_version(&noisy, Duration::from_secs(2)),
            Err("version output exceeds the size limit")
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn accepts_only_wine_version_markers() {
        assert!(is_wine_version("wine-9.0"));
        assert!(is_wine_version("Wine version 8.0"));
        assert!(!is_wine_version("not wine"));
    }

    #[test]
    fn timed_out_and_failed_wine_candidates_do_not_hide_later_runner() {
        let root = temp_dir();
        let slow = root.join("first");
        let invalid = root.join("second");
        fs::create_dir_all(&slow).unwrap();
        fs::create_dir_all(&invalid).unwrap();
        make_executable(&slow, "wine", "#!/bin/sh\nexec /bin/sleep 5\n");
        make_executable(&invalid, "wine", "#!/bin/sh\nexit 2\n");
        let valid = make_executable(&invalid, "wine64", "#!/bin/sh\nprintf 'wine-9.0\\n'\n");

        let started = Instant::now();
        let (runner, diagnostics) = discover_wine_from([slow, invalid], Duration::from_millis(50));

        let runner = runner.expect("later valid Wine binary should be found");
        assert_eq!(runner.version, "wine-9.0");
        assert_eq!(
            runner.path,
            fs::canonicalize(valid).unwrap().to_string_lossy()
        );
        assert!(
            diagnostics
                .iter()
                .any(|message| message.contains("timed out"))
        );
        assert!(
            diagnostics
                .iter()
                .any(|message| message.contains("unsuccessfully"))
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        fs::remove_dir_all(root).unwrap();
    }
}
