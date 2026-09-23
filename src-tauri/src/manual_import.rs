use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::database::{Database, DatabaseState, Game, database_error, required_name};

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualImportInput {
    pub executable_path: String,
    pub name: Option<String>,
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
             RETURNING id, steam_app_id, automatic_name, name_override, steam_install_path, steam_account_id, executable_path",
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
    let name = required_name(input.name.unwrap_or_else(|| stem.to_owned()), "name")?;
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
            "INSERT INTO games (id, name_override, executable_path) VALUES (?1, ?2, ?3)",
            params![id, name, executable_path],
        ).map_err(database_error)?;
        connection.query_row(
            "SELECT id, steam_app_id, automatic_name, name_override, steam_install_path, steam_account_id, executable_path FROM games WHERE id = ?1",
            [&id], crate::database::game_from_row,
        ).map_err(database_error)
    })
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
    let blocked = [
        "unins",
        "uninstall",
        "setup",
        "install",
        "vcredist",
        "vc_redist",
        "dxsetup",
        "directx",
        "crash",
        "anticheat",
        "easyanticheat",
        "eac",
        "webhelper",
        "helper",
        "redist",
    ];
    blocked.iter().any(|part| name.contains(part))
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
