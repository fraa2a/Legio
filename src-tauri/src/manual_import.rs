use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tauri::Manager;
use uuid::Uuid;

use crate::{
    catalog::{self, CatalogGame},
    database::{Database, DatabaseState, Game, database_error, required_name},
    network::NetworkState,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableCandidate {
    pub path: String,
    pub score: u32,
    pub signals: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableScan {
    pub candidates: Vec<ExecutableCandidate>,
    pub selected_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedExecutableCandidate {
    pub relative_path: String,
    pub score: u32,
    pub signals: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedExecutableScan {
    pub candidates: Vec<StagedExecutableCandidate>,
    pub selected_relative_path: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualImportInput {
    pub executable_path: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamIdentityCandidate {
    pub steam_app_id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SteamIdentificationStatus {
    Matched,
    NoMatch,
    Ambiguous,
    Unavailable,
    AlreadyLinked,
    NotManual,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamIdentificationResult {
    pub game: Game,
    pub status: SteamIdentificationStatus,
    pub candidates: Vec<SteamIdentityCandidate>,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamIdentificationPreview {
    pub status: SteamIdentificationStatus,
    pub candidates: Vec<SteamIdentityCandidate>,
    pub message: Option<String>,
}

pub fn scan_directory(directory: &str, game_name: Option<&str>) -> Result<ExecutableScan, String> {
    let root = fs::canonicalize(directory)
        .map_err(|error| format!("could not open game directory: {error}"))?;
    if !root.is_dir() {
        return Err("game directory is not a directory".to_owned());
    }
    let expected = game_name.map(normalize_name).unwrap_or_default();
    let mut pending = vec![(root.clone(), 0usize)];
    let mut candidates = Vec::new();
    let mut visited = 0usize;
    while let Some((directory, depth)) = pending.pop() {
        let entries = fs::read_dir(&directory).map_err(|error| {
            format!(
                "could not read game directory {}: {error}",
                directory.display()
            )
        })?;
        for entry in entries {
            let entry =
                entry.map_err(|error| format!("could not inspect game directory: {error}"))?;
            visited += 1;
            if visited > 10_000 {
                return Err("game directory contains too many files to scan".to_owned());
            }
            let kind = entry
                .file_type()
                .map_err(|error| format!("could not inspect game file: {error}"))?;
            let path = entry.path();
            if kind.is_dir() && depth < 8 {
                pending.push((path, depth + 1));
            } else if kind.is_file() && is_executable(&path)? && !is_false_positive(&path) {
                let stem = path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                let mut score = 0;
                let mut signals = Vec::new();
                if !expected.is_empty() && normalize_name(stem) == expected {
                    score += 100;
                    signals.push("game_name_match");
                }
                if depth == 0 {
                    score += 20;
                    signals.push("game_root");
                }
                let path = path
                    .to_str()
                    .ok_or("executable path is not valid UTF-8")?
                    .to_owned();
                candidates.push(ExecutableCandidate {
                    path,
                    score,
                    signals,
                });
            }
        }
    }
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
    });
    let selected_path = candidates
        .first()
        .filter(|first| {
            candidates.len() == 1
                || (first.signals.contains(&"game_name_match")
                    && candidates
                        .get(1)
                        .is_some_and(|second| first.score > second.score))
        })
        .map(|candidate| candidate.path.clone());
    Ok(ExecutableScan {
        candidates,
        selected_path,
    })
}

pub(crate) fn scan_staged_directory(
    directory: &Path,
    game_name: Option<&str>,
) -> Result<StagedExecutableScan, String> {
    let root = fs::canonicalize(directory)
        .map_err(|error| format!("could not open staged game directory: {error}"))?;
    if !root.is_dir() {
        return Err("staged game path is not a directory".to_owned());
    }
    let root_text = root.to_str().ok_or("staged game path is not valid UTF-8")?;
    let scan = scan_directory(root_text, game_name)?;
    let candidates = scan
        .candidates
        .into_iter()
        .map(|candidate| {
            let path = Path::new(&candidate.path);
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| "scanned executable is outside the staged game directory")?;
            let relative_path = portable_relative_path(relative)?;
            Ok(StagedExecutableCandidate {
                relative_path,
                score: candidate.score,
                signals: candidate.signals,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let selected_relative_path = scan
        .selected_path
        .map(|selected| {
            let relative = Path::new(&selected)
                .strip_prefix(&root)
                .map_err(|_| "selected executable is outside the staged game directory")?;
            portable_relative_path(relative)
        })
        .transpose()?;
    Ok(StagedExecutableScan {
        candidates,
        selected_relative_path,
    })
}

fn portable_relative_path(path: &Path) -> Result<String, String> {
    let components = path
        .components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .filter(|value| !value.contains('\\'))
                .ok_or_else(|| "executable path is not valid UTF-8 or portable".to_owned()),
            _ => Err("executable path contains an invalid component".to_owned()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if components.is_empty() {
        return Err("executable path is empty".to_owned());
    }
    Ok(components.join("/"))
}

pub fn import(state: &DatabaseState, input: ManualImportInput) -> Result<Game, String> {
    import_into(state.database()?, input)
}

pub fn set_executable(state: &DatabaseState, game_id: &str, value: &str) -> Result<Game, String> {
    let id = Uuid::parse_str(game_id)
        .map_err(|_| "game id is invalid".to_owned())?
        .to_string();
    let path = validate_executable(value)?;
    let path = path.to_str().ok_or("executable path is not valid UTF-8")?;
    state.database()?.with_connection(|connection| {
        connection.query_row(
            "UPDATE games SET executable_path = ?2 WHERE id = ?1 AND steam_install_path IS NULL
             RETURNING id, steam_app_id, automatic_name, name_override, steam_install_path, steam_account_id, executable_path, installation_root",
            params![id, path], crate::database::game_from_row,
        ).optional().map_err(database_error)?.ok_or_else(|| "game was not found or is Steam-managed".to_owned())
    })
}

fn import_into(database: &Database, input: ManualImportInput) -> Result<Game, String> {
    let path = validate_executable(&input.executable_path)?;
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or("executable filename is invalid")?;
    let automatic_name = required_name(stem.to_owned(), "automatic name")?;
    let name_override = input
        .name
        .map(|name| required_name(name, "name"))
        .transpose()?;
    let executable_path = path
        .to_str()
        .ok_or("executable path is not valid UTF-8")?
        .to_owned();
    let id = Uuid::new_v4().to_string();
    database.with_connection(|connection| {
        let existing: Option<String> = connection.query_row(
            "SELECT id FROM games WHERE executable_path = ?1", [&executable_path], |row| row.get(0),
        ).optional().map_err(database_error)?;
        if existing.is_some() {
            return Err("executable is already in the library".to_owned());
        }
        connection.execute(
            "INSERT INTO games (id, automatic_name, name_override, executable_path) VALUES (?1, ?2, ?3, ?4)",
            params![id, automatic_name, name_override, executable_path],
        ).map_err(database_error)?;
        connection.query_row(
            "SELECT id, steam_app_id, automatic_name, name_override, steam_install_path, steam_account_id, executable_path, installation_root FROM games WHERE id = ?1",
            [&id], crate::database::game_from_row,
        ).map_err(database_error)
    })
}

pub async fn identify_steam_app_id(
    app: tauri::AppHandle,
    network: NetworkState,
    game_id: String,
) -> Result<SteamIdentificationResult, String> {
    let game = {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            app.state::<DatabaseState>().database()?.game(&game_id)
        })
        .await
        .map_err(|error| format!("Steam identity lookup task failed: {error}"))??
    };

    let Some(executable_path) = game.executable_path.as_deref() else {
        return Ok(identification_result(
            game,
            SteamIdentificationStatus::NotManual,
            Vec::new(),
            None,
        ));
    };
    if game.steam_install_path.is_some() {
        return Ok(identification_result(
            game,
            SteamIdentificationStatus::NotManual,
            Vec::new(),
            None,
        ));
    }
    if game.steam_app_id.is_some() {
        return Ok(identification_result(
            game,
            SteamIdentificationStatus::AlreadyLinked,
            Vec::new(),
            None,
        ));
    }

    let executable = PathBuf::from(executable_path);
    let legacy_name = game
        .name_override
        .as_deref()
        .filter(|name| {
            game.automatic_name.is_none()
                && executable.file_stem().and_then(|stem| stem.to_str()) == Some(*name)
        })
        .map(str::to_owned);
    let preview = detect_steam_identity(&app, &network, &executable).await;
    if preview.status == SteamIdentificationStatus::Matched {
        let candidate = &preview.candidates[0];
        let game = set_identified_steam_game(&app, &game.id, candidate, legacy_name).await?;
        return Ok(identification_result(
            game,
            preview.status,
            preview.candidates,
            preview.message,
        ));
    }
    Ok(identification_result(
        game,
        preview.status,
        preview.candidates,
        preview.message,
    ))
}

pub async fn preview_steam_app_id(
    app: tauri::AppHandle,
    network: NetworkState,
    executable_path: String,
) -> Result<SteamIdentificationPreview, String> {
    let executable =
        tauri::async_runtime::spawn_blocking(move || validate_executable(&executable_path))
            .await
            .map_err(|error| format!("Executable validation task failed: {error}"))??;
    Ok(detect_steam_identity(&app, &network, &executable).await)
}

async fn detect_steam_identity(
    app: &tauri::AppHandle,
    network: &NetworkState,
    executable: &Path,
) -> SteamIdentificationPreview {
    let names = title_candidates(executable);
    if names.is_empty() {
        return SteamIdentificationPreview {
            status: SteamIdentificationStatus::NoMatch,
            candidates: Vec::new(),
            message: None,
        };
    }

    let mut remote_attempted = false;
    let mut remote_error = None;
    for name in names {
        let cached = match catalog::search_catalog(app.clone(), name.clone()).await {
            Ok(result) => result,
            Err(error) => {
                return SteamIdentificationPreview {
                    status: SteamIdentificationStatus::Unavailable,
                    candidates: Vec::new(),
                    message: Some(error.message),
                };
            }
        };
        let mut candidates = exact_matches(&cached.games, &name);
        if candidates.is_empty() && !remote_attempted {
            remote_attempted = true;
            match catalog::refresh_catalog(app.clone(), network, name.clone()).await {
                Ok(result) => candidates = exact_matches(&result.games, &name),
                Err(error) => remote_error = Some(error.message),
            }
        }
        if candidates.is_empty() {
            continue;
        }

        return SteamIdentificationPreview {
            status: if candidates.len() == 1 {
                SteamIdentificationStatus::Matched
            } else {
                SteamIdentificationStatus::Ambiguous
            },
            candidates,
            message: None,
        };
    }

    let status = if remote_error.is_some() {
        SteamIdentificationStatus::Unavailable
    } else {
        SteamIdentificationStatus::NoMatch
    };
    SteamIdentificationPreview {
        status,
        candidates: Vec::new(),
        message: remote_error,
    }
}

fn title_candidates(executable: &Path) -> Vec<String> {
    let generic_names = [
        "bin",
        "binaries",
        "bootstrapper",
        "crashreporter",
        "debug",
        "game",
        "games",
        "launch",
        "launcher",
        "release",
        "retail",
        "runtime",
        "server",
        "shipping",
        "start",
        "system",
        "system32",
        "win32",
        "win64",
        "x64",
        "x86",
    ];
    let mut candidates = Vec::new();
    if let Some(stem) = executable.file_stem().and_then(|name| name.to_str()) {
        candidates.push(stem.to_owned());
    }
    let mut directory = executable.parent();
    while let Some(path) = directory {
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            candidates.push(name.to_owned());
        }
        directory = path.parent();
    }

    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|name| {
            let normalized = normalize_name(name);
            name.len() <= 200
                && !name.chars().any(char::is_control)
                && !normalized.is_empty()
                && !generic_names.contains(&normalized.as_str())
                && !normalized.ends_with("shipping")
                && seen.insert(normalized)
        })
        .take(3)
        .collect()
}

fn exact_matches(games: &[CatalogGame], query: &str) -> Vec<SteamIdentityCandidate> {
    let expected = normalize_name(query);
    let mut matches: Vec<_> = games
        .iter()
        .filter(|game| normalize_name(&game.name) == expected)
        .map(|game| SteamIdentityCandidate {
            steam_app_id: game.steam_app_id,
            name: game.name.clone(),
        })
        .collect();
    matches.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.steam_app_id.cmp(&right.steam_app_id))
    });
    matches.dedup_by_key(|candidate| candidate.steam_app_id);
    matches
}

async fn set_identified_steam_game(
    app: &tauri::AppHandle,
    game_id: &str,
    candidate: &SteamIdentityCandidate,
    legacy_name: Option<String>,
) -> Result<Game, String> {
    let app = app.clone();
    let game_id = game_id.to_owned();
    let candidate = candidate.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DatabaseState>()
            .database()?
            .with_connection(|connection| {
                connection
                    .query_row(
                        "UPDATE games SET steam_app_id = ?2, automatic_name = ?3,
                         name_override = CASE WHEN automatic_name IS NULL AND name_override = ?4 THEN NULL ELSE name_override END
                         WHERE id = ?1 AND steam_app_id IS NULL AND steam_install_path IS NULL AND executable_path IS NOT NULL
                         RETURNING id, steam_app_id, automatic_name, name_override, steam_install_path, steam_account_id, executable_path, installation_root",
                        rusqlite::params![game_id, candidate.steam_app_id, candidate.name, legacy_name],
                        crate::database::game_from_row,
                    )
                    .optional()
                    .map_err(database_error)?
                    .ok_or_else(|| "game was not found or is not a manual import".to_owned())
            })
    })
    .await
    .map_err(|error| format!("Steam identity save task failed: {error}"))?
}

fn identification_result(
    game: Game,
    status: SteamIdentificationStatus,
    candidates: Vec<SteamIdentityCandidate>,
    message: Option<String>,
) -> SteamIdentificationResult {
    SteamIdentificationResult {
        game,
        status,
        candidates,
        message,
    }
}

fn validate_executable(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || !Path::new(value).is_absolute() {
        return Err("executable path must be absolute".to_owned());
    }
    let path =
        fs::canonicalize(value).map_err(|error| format!("could not open executable: {error}"))?;
    if !path.is_file() || !is_executable(&path)? {
        return Err("selected path is not an executable file".to_owned());
    }
    Ok(path)
}

fn normalize_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_false_positive(path: &Path) -> bool {
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let normalized: String = name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect();
    let blocked = [
        "unins",
        "uninstall",
        "setup",
        "install",
        "vcredist",
        "dxsetup",
        "directx",
        "crashreport",
        "crashhandler",
        "crashpad",
        "anticheat",
        "easyanticheat",
        "webhelper",
        "helper",
        "redist",
    ];
    normalized == "eac"
        || blocked.iter().any(|part| normalized.contains(part))
        || path.components().any(|part| {
            let part = part.as_os_str().to_string_lossy().to_ascii_lowercase();
            [
                "_redist",
                "redist",
                "redistributables",
                "directx",
                "__installer",
            ]
            .contains(&part.as_str())
        })
}

fn is_executable(path: &Path) -> Result<bool, String> {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Ok(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(fs::metadata(path)
            .map_err(|error| format!("could not inspect executable: {error}"))?
            .permissions()
            .mode()
            & 0o111
            != 0)
    }
    #[cfg(not(unix))]
    {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        let path = std::env::temp_dir().join(format!("legio-executable-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn name_match_selects_clear_candidate() {
        let root = fixture();
        fs::write(root.join("Portal.exe"), []).unwrap();
        fs::write(root.join("Other.exe"), []).unwrap();
        let scan = scan_directory(root.to_str().unwrap(), Some("Portal")).unwrap();
        assert_eq!(
            scan.selected_path,
            Some(root.join("Portal.exe").to_str().unwrap().to_owned())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn equal_candidates_require_choice() {
        let root = fixture();
        fs::write(root.join("A.exe"), []).unwrap();
        fs::write(root.join("B.exe"), []).unwrap();
        let scan = scan_directory(root.to_str().unwrap(), None).unwrap();
        assert_eq!(scan.selected_path, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn staged_scan_returns_paths_relative_to_its_root() {
        let root = fixture();
        fs::create_dir(root.join("bin")).unwrap();
        fs::write(root.join("bin/Game.exe"), []).unwrap();
        let scan = scan_staged_directory(&root, Some("Game")).unwrap();
        assert_eq!(scan.selected_relative_path.as_deref(), Some("bin/Game.exe"));
        assert_eq!(scan.candidates[0].relative_path, "bin/Game.exe");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn root_location_alone_does_not_choose_between_candidates() {
        let root = fixture();
        fs::create_dir(root.join("bin")).unwrap();
        fs::write(root.join("Launcher.exe"), []).unwrap();
        fs::write(root.join("bin/Game.exe"), []).unwrap();
        let scan = scan_directory(root.to_str().unwrap(), None).unwrap();
        assert_eq!(scan.selected_path, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn installer_is_excluded_from_candidates() {
        let root = fixture();
        fs::write(root.join("Game.exe"), []).unwrap();
        fs::write(root.join("Setup.exe"), []).unwrap();
        let scan = scan_directory(root.to_str().unwrap(), None).unwrap();
        assert_eq!(scan.candidates.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn game_names_with_helper_substrings_are_not_filtered() {
        assert!(!is_false_positive(Path::new("Peace.exe")));
        assert!(!is_false_positive(Path::new("Peach.exe")));
        assert!(!is_false_positive(Path::new("Crash Bandicoot.exe")));
    }

    #[test]
    fn manual_import_persists_selected_executable() {
        let root = fixture();
        let executable = root.join("Portal.exe");
        fs::write(&executable, []).unwrap();
        let database = Database::open(&root).unwrap();
        let game = import_into(
            &database,
            ManualImportInput {
                executable_path: executable.to_str().unwrap().to_owned(),
                name: Some("Portal".to_owned()),
            },
        )
        .unwrap();
        drop(database);
        let reopened = Database::open(&root).unwrap();
        assert_eq!(reopened.games().unwrap(), vec![game]);
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_non_executable_path() {
        let root = fixture();
        let file = root.join("readme.txt");
        fs::write(&file, []).unwrap();
        let result = validate_executable(file.to_str().unwrap());
        assert!(result.is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
