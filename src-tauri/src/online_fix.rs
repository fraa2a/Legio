use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;

use crate::database::{Database, Game};

pub(crate) fn detected(database: &Database, game: &Game) -> Result<bool, String> {
    if game.steam_install_path.is_some() {
        return Ok(false);
    }
    let Some(executable) = game.executable_path.as_deref() else {
        return Ok(false);
    };
    let executable = fs::canonicalize(executable)
        .map_err(|error| format!("Could not inspect game executable for OnlineFix: {error}"))?;
    let installed_root: Option<String> = database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT final_path FROM downloads WHERE id = ?1 AND status = 'installed'",
                [&game.id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("Could not find installed game directory: {error}"))
    })?;
    let installed_root = installed_root
        .as_deref()
        .map(|root| {
            fs::canonicalize(root)
                .map_err(|error| format!("Could not inspect installed game directory: {error}"))
        })
        .transpose()?
        .filter(|root| executable.starts_with(root));
    let root = installed_root
        .as_deref()
        .or_else(|| executable.parent())
        .ok_or_else(|| "Game executable has no parent directory".to_owned())?;
    contains_online_fix(root)
}

fn contains_online_fix(root: &Path) -> Result<bool, String> {
    let mut pending = vec![PathBuf::from(root)];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("Could not scan game directory for OnlineFix: {error}"))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("Could not scan game directory for OnlineFix: {error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("Could not inspect game file for OnlineFix: {error}"))?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case("OnlineFix.dll"))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn enabled(steam_managed: bool, override_value: Option<bool>, detected: bool) -> bool {
    !steam_managed && override_value.unwrap_or(detected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::CreateGameInput;
    use rusqlite::params;

    #[test]
    fn finds_nested_online_fix() {
        let root = std::env::temp_dir().join(format!("legio-online-fix-{}", uuid::Uuid::new_v4()));
        let nested = root.join("bin").join("fix");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("ONLINEFIX.DLL"), b"").unwrap();
        assert!(contains_online_fix(&root).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_fix_from_installed_root_above_executable() {
        let root = std::env::temp_dir().join(format!("legio-online-fix-{}", uuid::Uuid::new_v4()));
        let install = root.join("game");
        let executable = install.join("bin").join("game.exe");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"").unwrap();
        fs::write(install.join("OnlineFix.dll"), b"").unwrap();
        let database = Database::open(&root.join("data")).unwrap();
        let game = database
            .create_game(CreateGameInput {
                name: "Game".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE games SET executable_path = ?2 WHERE id = ?1",
                        params![&game.id, executable.to_string_lossy()],
                    )
                    .map_err(|error| error.to_string())?;
                connection
                    .execute(
                        "INSERT INTO downloads
                         (id, steam_app_id, name, release_version, url, sha256, size_bytes,
                          status, created_at, updated_at, final_path)
                         VALUES (?1, 42, 'Game', '1', 'https://example.test/game', 'hash', 1,
                                 'installed', 1, 1, ?2)",
                        params![&game.id, install.to_string_lossy()],
                    )
                    .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        assert!(detected(&database, &database.game(&game.id).unwrap()).unwrap());
        drop(database);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn steam_managed_and_explicit_disable_win_over_detection() {
        assert!(!enabled(true, Some(true), true));
        assert!(!enabled(false, Some(false), true));
        assert!(enabled(false, None, true));
    }
}
