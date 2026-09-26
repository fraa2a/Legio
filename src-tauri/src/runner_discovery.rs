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
    pub runners: Vec<InstalledRunner>,
    pub diagnostics: Vec<String>,
}

#[cfg(target_os = "linux")]
pub(crate) fn resolve_runner(path: &str) -> Result<InstalledRunner, String> {
    let requested = fs::canonicalize(path)
        .map_err(|error| format!("Could not resolve selected compatibility runner: {error}"))?;
    discover()
        .runners
        .into_iter()
        .find(|runner| Path::new(&runner.path) == requested)
        .ok_or_else(|| "Selected compatibility runner is no longer available".to_owned())
}

#[cfg(target_os = "linux")]
pub(crate) fn launch_command(
    runner: &InstalledRunner,
    executable: &Path,
    arguments_before: &[String],
    arguments_after: &[String],
    steam_runtime: Option<&Path>,
) -> Command {
    let program = match runner.kind {
        RunnerKind::Proton | RunnerKind::GeProton => Path::new(&runner.path).join("proton"),
        RunnerKind::Wine => Path::new(&runner.path).to_path_buf(),
    };
    let mut command = if let Some(runtime) = steam_runtime {
        let mut command = Command::new(runtime);
        command.arg("--").arg(program);
        command
    } else {
        Command::new(program)
    };
    match runner.kind {
        RunnerKind::Proton | RunnerKind::GeProton => {
            command
                .arg("run")
                .args(arguments_before)
                .arg(executable)
                .args(arguments_after);
        }
        RunnerKind::Wine => {
            command
                .args(arguments_before)
                .arg(executable)
                .args(arguments_after);
        }
    }
    command
}

#[cfg(target_os = "linux")]
pub(crate) fn steam_runtime_path(runner: &InstalledRunner) -> Result<PathBuf, String> {
    let (runtime_dir, runtime_name) = steam_runtime_name(runner, std::env::consts::ARCH)?;
    find_steam_runtime(
        runner,
        steam_local::default_steam_library_paths(),
        runtime_dir,
        runtime_name,
    )
}

#[cfg(target_os = "linux")]
fn steam_runtime_name(
    runner: &InstalledRunner,
    arch: &str,
) -> Result<(&'static str, &'static str), String> {
    if !matches!(runner.kind, RunnerKind::Proton | RunnerKind::GeProton) {
        return Err("Steam Linux Runtime can only wrap Proton runners".to_owned());
    }
    let (major, minor) = proton_version(runner).ok_or_else(|| {
        format!(
            "Could not determine the Proton version for {} ({})",
            runner.name, runner.version
        )
    })?;
    match (major, minor, arch) {
        (11.., _, "aarch64") => Ok(("SteamLinuxRuntime_4-arm64", "Steam Linux Runtime 4.0")),
        (11.., _, "x86_64") => Ok(("SteamLinuxRuntime_4", "Steam Linux Runtime 4.0")),
        (11.., _, _) => Err(format!(
            "Steam Linux Runtime 4.0 is unavailable for Proton {} on {arch}",
            runner.version
        )),
        (8..=10, _, "x86_64") => Ok((
            "SteamLinuxRuntime_sniper",
            "Steam Linux Runtime 3.0 (sniper)",
        )),
        (8..=10, _, _) => Err(format!(
            "Steam Linux Runtime 3.0 is unavailable for Proton {} on {arch}",
            runner.version
        )),
        (5, 13.., "x86_64") | (6..=7, _, "x86_64") => Ok((
            "SteamLinuxRuntime_soldier",
            "Steam Linux Runtime 2.0 (soldier)",
        )),
        (5, 13.., _) | (6..=7, _, _) => Err(format!(
            "Steam Linux Runtime 2.0 is unavailable for Proton {} on {arch}",
            runner.version
        )),
        _ => Err(format!(
            "No Steam Linux Runtime mapping is available for Proton {}",
            runner.version
        )),
    }
}

#[cfg(target_os = "linux")]
fn proton_version(runner: &InstalledRunner) -> Option<(u32, u32)> {
    [runner.version.as_str(), runner.name.as_str()]
        .into_iter()
        .find_map(|value| {
            let lower = value.to_ascii_lowercase();
            let marker = lower.find("proton")? + "proton".len();
            let suffix = value[marker..].trim_start_matches(|ch: char| !ch.is_ascii_digit());
            let (major, remainder) = take_number(suffix)?;
            let minor = remainder
                .strip_prefix('.')
                .and_then(|value| take_number(value))
                .map_or(0, |(number, _)| number);
            Some((major, minor))
        })
}

#[cfg(target_os = "linux")]
fn take_number(value: &str) -> Option<(u32, &str)> {
    let digits = value.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    Some((value[..digits].parse().ok()?, &value[digits..]))
}

#[cfg(target_os = "linux")]
fn find_steam_runtime(
    runner: &InstalledRunner,
    libraries: impl IntoIterator<Item = PathBuf>,
    runtime_dir: &str,
    runtime_name: &str,
) -> Result<PathBuf, String> {
    for library in libraries {
        let candidate = library
            .join("steamapps/common")
            .join(runtime_dir)
            .join("run");
        if is_executable_file(&candidate) {
            return fs::canonicalize(candidate).map_err(|error| {
                format!(
                    "Could not resolve {runtime_name} for {}: {error}",
                    runner.name
                )
            });
        }
    }
    Err(format!(
        "{runtime_name} is not installed for {}. Install it in a Steam library and retry",
        runner.name
    ))
}

pub fn discover() -> RunnerDiscovery {
    #[cfg(target_os = "linux")]
    {
        discover_linux()
    }
    #[cfg(not(target_os = "linux"))]
    {
        RunnerDiscovery {
            runners: Vec::new(),
            diagnostics: vec!["Runner discovery is currently supported on Linux only".to_owned()],
        }
    }
}

#[cfg(target_os = "linux")]
fn discover_linux() -> RunnerDiscovery {
    let mut runners = discover_proton();
    let (wine, diagnostics) = discover_wine_from(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        WINE_PROBE_TIMEOUT,
    );
    runners.extend(wine);
    RunnerDiscovery {
        runners,
        diagnostics,
    }
}

#[cfg(target_os = "linux")]
fn discover_proton() -> Vec<InstalledRunner> {
    let mut directories = Vec::new();
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
    } else if lower == "proton" || lower.starts_with("proton ") || lower.starts_with("proton-") {
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
    let mut child = Command::new(path)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "could not start executable")?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("version check timed out");
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("could not wait for executable");
            }
        }
    };
    let output = child
        .wait_with_output()
        .map_err(|_| "could not read version output")?;
    if !status.success() {
        return Err("version check exited unsuccessfully");
    }
    let version_output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    version_output
        .lines()
        .map(str::trim)
        .find(|line| is_wine_version(line))
        .map(str::to_owned)
        .ok_or("version output was not recognized")
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
        let command = launch_command(&runner, Path::new("/games/game.exe"), &before, &after, None);
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
    fn launch_command_places_proton_inside_the_selected_runtime() {
        let runner = InstalledRunner {
            kind: RunnerKind::Proton,
            name: "Proton 11.0".to_owned(),
            version: "proton-11.0".to_owned(),
            path: "/Steam Library/steamapps/common/Proton 11.0".to_owned(),
        };
        let command = launch_command(
            &runner,
            Path::new("/games/Windows Game/game.exe"),
            &["before".to_owned()],
            &["after".to_owned()],
            Some(Path::new(
                "/Steam Library/steamapps/common/SteamLinuxRuntime_4/run",
            )),
        );
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            command.get_program(),
            "/Steam Library/steamapps/common/SteamLinuxRuntime_4/run"
        );
        assert_eq!(
            arguments,
            [
                "--",
                "/Steam Library/steamapps/common/Proton 11.0/proton",
                "run",
                "before",
                "/games/Windows Game/game.exe",
                "after"
            ]
        );
    }

    #[test]
    fn chooses_steam_runtime_from_proton_version_and_architecture() {
        let runner = InstalledRunner {
            kind: RunnerKind::GeProton,
            name: "GE-Proton10-33".to_owned(),
            version: "1776473842 GE-Proton10-33-rtsp23-4-1".to_owned(),
            path: "/unused".to_owned(),
        };
        assert_eq!(
            steam_runtime_name(&runner, "x86_64"),
            Ok((
                "SteamLinuxRuntime_sniper",
                "Steam Linux Runtime 3.0 (sniper)"
            ))
        );
        let proton_11 = InstalledRunner {
            name: "Proton 11.0".to_owned(),
            version: "1788504981 proton-11.0-2c-x86_64".to_owned(),
            kind: RunnerKind::Proton,
            path: "/unused".to_owned(),
        };
        assert_eq!(
            steam_runtime_name(&proton_11, "x86_64"),
            Ok(("SteamLinuxRuntime_4", "Steam Linux Runtime 4.0"))
        );
        assert_eq!(
            steam_runtime_name(&proton_11, "aarch64"),
            Ok(("SteamLinuxRuntime_4-arm64", "Steam Linux Runtime 4.0"))
        );
    }

    #[test]
    fn runtime_discovery_reports_a_missing_local_runtime() {
        let runner = InstalledRunner {
            kind: RunnerKind::Proton,
            name: "Proton 11.0".to_owned(),
            version: "proton-11.0".to_owned(),
            path: "/unused".to_owned(),
        };
        let error = find_steam_runtime(
            &runner,
            [],
            "SteamLinuxRuntime_4",
            "Steam Linux Runtime 4.0",
        )
        .unwrap_err();
        assert!(error.contains("is not installed"));
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
