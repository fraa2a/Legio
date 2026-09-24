#[cfg(target_os = "linux")]
use std::collections::HashSet;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(target_os = "linux")]
use std::process::Command;

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
    let mut diagnostics = Vec::new();
    match discover_wine() {
        Ok(Some(runner)) => runners.push(runner),
        Ok(None) => {}
        Err(error) => diagnostics.push(error),
    }
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
fn discover_wine() -> Result<Option<InstalledRunner>, String> {
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        for binary in ["wine", "wine64"] {
            let path = directory.join(binary);
            if !is_executable_file(&path) {
                continue;
            }
            let canonical = fs::canonicalize(&path)
                .map_err(|error| format!("Could not resolve Wine executable: {error}"))?;
            let output = Command::new(&canonical)
                .arg("--version")
                .output()
                .map_err(|error| format!("Could not read Wine version: {error}"))?;
            let version_output = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let version = version_output
                .lines()
                .map(str::trim)
                .find(|line| is_wine_version(line))
                .map(str::to_owned);
            let Some(version) = version.filter(|_| output.status.success()) else {
                continue;
            };
            return Ok(Some(InstalledRunner {
                kind: RunnerKind::Wine,
                name: "Wine".to_owned(),
                version,
                path: canonical.to_string_lossy().into_owned(),
            }));
        }
    }
    Ok(None)
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
}
