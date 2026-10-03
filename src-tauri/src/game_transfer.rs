use rusqlite::params;
use serde::Serialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::{
    database::{Database, DatabaseState, Game, database_error},
    finalize_install,
    game_lifecycle::GameLaunchManager,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferResult {
    game: Game,
    warning: Option<String>,
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir(destination)
        .map_err(|error| format!("Could not create transfer directory: {error}"))?;
    for entry in
        fs::read_dir(source).map_err(|error| format!("Could not read source directory: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Could not read source entry: {error}"))?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&from)
            .map_err(|error| format!("Could not inspect source entry: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("Transfer source contains a symbolic link".to_owned());
        }
        if metadata.is_dir() {
            copy_directory(&from, &to)?;
        } else if metadata.is_file() {
            let mut options = fs::OpenOptions::new();
            options.read(true);
            #[cfg(target_os = "linux")]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NOFOLLOW);
            }
            let mut input = options
                .open(&from)
                .map_err(|error| format!("Could not open source file: {error}"))?;
            if !input
                .metadata()
                .map_err(|error| error.to_string())?
                .is_file()
            {
                return Err("Transfer source file changed during copy".to_owned());
            }
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&to)
                .map_err(|error| format!("Could not create destination file: {error}"))?;
            io::copy(&mut input, &mut output)
                .map_err(|error| format!("Could not copy game file: {error}"))?;
            output
                .sync_all()
                .map_err(|error| format!("Could not sync game file: {error}"))?;
        } else {
            return Err("Transfer source contains an unsupported file type".to_owned());
        }
        fs::set_permissions(&to, metadata.permissions())
            .map_err(|error| format!("Could not preserve file permissions: {error}"))?;
    }
    finalize_install::sync_directory(destination)
}

fn transfer(
    app: &AppHandle,
    game_id: &str,
    source: &str,
    destination: &str,
) -> Result<TransferResult, String> {
    let manager = app.state::<GameLaunchManager>();
    let _operation = manager.operation()?;
    manager.require_idle(game_id)?;
    let database = app.state::<DatabaseState>();
    let game = database.database()?.game(game_id)?;
    if game.steam_install_path.is_some() {
        return Err("Steam-managed games must be transferred through Steam".to_owned());
    }
    let executable = game
        .executable_path
        .as_deref()
        .ok_or_else(|| "Select a game executable first".to_owned())?;
    let executable = fs::canonicalize(executable)
        .map_err(|error| format!("Could not inspect game executable: {error}"))?;
    let source = fs::canonicalize(source)
        .map_err(|error| format!("Could not inspect source directory: {error}"))?;
    if !source.is_dir() {
        return Err("Transfer source is not a directory".to_owned());
    }
    if source.parent().is_none() {
        return Err("The filesystem root cannot be transferred".to_owned());
    }
    let installed_path = game.installation_root.clone();
    if installed_path
        .as_deref()
        .is_some_and(|path| fs::canonicalize(path).ok().as_deref() != Some(source.as_path()))
    {
        return Err(
            "Select the complete installed game directory as the transfer source".to_owned(),
        );
    }
    if database.database()?.games()?.iter().any(|other| {
        other.id != game_id
            && other
                .executable_path
                .as_deref()
                .is_some_and(|path| Path::new(path).starts_with(&source))
    }) {
        return Err("Another library game uses the selected source directory".to_owned());
    }
    let relative = executable
        .strip_prefix(&source)
        .map_err(|_| "Game executable is outside the selected source directory".to_owned())?;
    if relative.as_os_str().is_empty() {
        return Err("Transfer source must be a directory".to_owned());
    }
    let relative = relative.to_path_buf();
    let destination = PathBuf::from(destination);
    if !destination.is_absolute() || destination.to_string_lossy().chars().any(char::is_control) {
        return Err("Destination must be an absolute directory path".to_owned());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "Destination has no parent directory".to_owned())?;
    let parent = fs::canonicalize(parent)
        .map_err(|error| format!("Could not inspect destination parent: {error}"))?;
    if !parent.is_dir() {
        return Err("Destination parent is not a directory".to_owned());
    }
    let name = destination
        .file_name()
        .ok_or_else(|| "Destination has no folder name".to_owned())?;
    let destination = parent.join(name);
    if destination.exists() {
        return Err("Destination already exists".to_owned());
    }
    if destination.starts_with(&source) || source.starts_with(&destination) {
        return Err("Source and destination directories overlap".to_owned());
    }
    let temporary = parent.join(format!(".legio-transfer-{}", Uuid::new_v4()));
    let intent = TransferIntent {
        game_id: game_id.to_owned(),
        source,
        destination,
        temporary,
        relative,
        phase: "prepared".to_owned(),
    };
    database.database()?.with_connection(|connection| {
        connection.execute(
            "INSERT INTO game_transfers (game_id, source, destination, temporary, executable_relative, phase) VALUES (?1, ?2, ?3, ?4, ?5, 'prepared')",
            params![intent.game_id, intent.source.to_string_lossy(), intent.destination.to_string_lossy(), intent.temporary.to_string_lossy(), intent.relative.to_string_lossy()],
        ).map_err(database_error)?;
        Ok(())
    })?;
    // Keep the source until both publication and the database transaction succeed.
    use std::io::Write;
    let mut marker = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(intent.source.join(MARKER))
        .map_err(|error| format!("Could not reserve transfer source: {error}"))?;
    marker
        .write_all(intent.temporary.to_string_lossy().as_bytes())
        .and_then(|()| marker.sync_all())
        .map_err(|error| format!("Could not sync transfer source marker: {error}"))?;
    drop(marker);
    finalize_install::sync_directory(&intent.source)?;
    copy_directory(&intent.source, &intent.temporary)?;
    fs::write(
        intent.temporary.join(MARKER),
        intent.temporary.to_string_lossy().as_bytes(),
    )
    .map_err(|error| format!("Could not write transfer marker: {error}"))?;
    fs::File::open(intent.temporary.join(MARKER))
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("Could not sync transfer marker: {error}"))?;
    finalize_install::sync_directory(&intent.temporary)?;
    database.database()?.with_connection(|connection| {
        connection
            .execute(
                "UPDATE game_transfers SET phase = 'copied' WHERE game_id = ?1",
                [game_id],
            )
            .map_err(database_error)?;
        Ok(())
    })?;
    commit_transfer(database.database()?, &intent)?;
    let warning = cleanup_transfer(database.database()?, &intent).err();
    Ok(TransferResult {
        game: database.database()?.game(game_id)?,
        warning,
    })
}

const MARKER: &str = ".legio-transfer-owner";

struct TransferIntent {
    game_id: String,
    source: PathBuf,
    destination: PathBuf,
    temporary: PathBuf,
    relative: PathBuf,
    phase: String,
}

fn remap(value: Option<String>, source: &Path, destination: &Path) -> Option<String> {
    value.map(|value| {
        Path::new(&value).strip_prefix(source).map_or_else(
            |_| value.clone(),
            |relative| destination.join(relative).to_string_lossy().into_owned(),
        )
    })
}

fn owns_directory(directory: &Path, intent: &TransferIntent) -> bool {
    if !fs::symlink_metadata(directory)
        .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
    {
        return false;
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    use std::io::Read;
    let Ok(file) = options.open(directory.join(MARKER)) else {
        return false;
    };
    if !file
        .metadata()
        .is_ok_and(|meta| meta.is_file() && meta.len() <= 4096)
    {
        return false;
    }
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes).is_ok()
        && bytes == intent.temporary.to_string_lossy().as_bytes()
}

fn owns_destination(intent: &TransferIntent) -> bool {
    owns_directory(&intent.destination, intent)
}

fn commit_transfer(database: &Database, intent: &TransferIntent) -> Result<(), String> {
    if !intent.destination.exists() {
        finalize_install::publish_noreplace(&intent.temporary, &intent.destination)
            .map_err(|error| format!("Could not publish transferred game: {error}"))?;
        finalize_install::sync_directory(
            intent
                .destination
                .parent()
                .ok_or("Transfer target has no parent")?,
        )?;
    }
    if !owns_destination(intent) || !intent.destination.join(&intent.relative).is_file() {
        return Err("Transfer target is not the completed copy owned by Legio".to_owned());
    }
    database.with_connection(|connection| {
        let transaction = connection.unchecked_transaction().map_err(database_error)?;
        for (table, column) in [
            ("game_native_launch_config", "working_directory"),
            ("game_compatibility_overrides", "working_directory"),
            ("game_compatibility_overrides", "prefix_path"),
        ] {
            let sql = format!("SELECT {column} FROM {table} WHERE game_id = ?1");
            use rusqlite::OptionalExtension;
            let value: Option<String> = transaction
                .query_row(&sql, [&intent.game_id], |row| row.get(0))
                .optional()
                .map_err(database_error)?
                .flatten();
            let value = remap(value, &intent.source, &intent.destination);
            transaction
                .execute(
                    &format!("UPDATE {table} SET {column} = ?2 WHERE game_id = ?1"),
                    params![intent.game_id, value],
                )
                .map_err(database_error)?;
        }
        transaction
            .execute(
                "UPDATE games SET executable_path = ?2, installation_root = ?3 WHERE id = ?1",
                params![
                    intent.game_id,
                    intent.destination.join(&intent.relative).to_string_lossy(),
                    intent.destination.to_string_lossy()
                ],
            )
            .map_err(database_error)?;
        transaction
            .execute(
                "UPDATE downloads SET final_path = ?2 WHERE id = ?1 AND status = 'installed'",
                params![intent.game_id, intent.destination.to_string_lossy()],
            )
            .map_err(database_error)?;
        transaction
            .execute(
                "UPDATE game_transfers SET phase = 'committed' WHERE game_id = ?1",
                [&intent.game_id],
            )
            .map_err(database_error)?;
        transaction.commit().map_err(database_error)
    })
}

fn cleanup_transfer(database: &Database, intent: &TransferIntent) -> Result<(), String> {
    if !owns_destination(intent) {
        return Err("Transfer cleanup requires the owned destination".to_owned());
    }
    if intent.source.exists() && !owns_directory(&intent.source, intent) {
        return Err(
            "Game transferred, but source ownership changed; old directory preserved".to_owned(),
        );
    }
    match fs::remove_dir_all(&intent.source) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Game transferred, but old copy cleanup failed: {error}"
            ));
        }
    }
    database.with_connection(|connection| {
        connection
            .execute(
                "DELETE FROM game_transfers WHERE game_id = ?1",
                [&intent.game_id],
            )
            .map_err(database_error)?;
        Ok(())
    })?;
    fs::remove_file(intent.destination.join(MARKER))
        .map_err(|error| format!("Could not remove transfer marker: {error}"))
}

pub(crate) fn recover(database: &Database) -> Result<(), String> {
    let intents: Vec<TransferIntent> = database.with_connection(|connection| {
        let mut statement = connection.prepare("SELECT game_id, source, destination, temporary, executable_relative, phase FROM game_transfers").map_err(database_error)?;
        statement.query_map([], |row| Ok(TransferIntent {
            game_id: row.get(0)?, source: PathBuf::from(row.get::<_, String>(1)?),
            destination: PathBuf::from(row.get::<_, String>(2)?), temporary: PathBuf::from(row.get::<_, String>(3)?),
            relative: PathBuf::from(row.get::<_, String>(4)?), phase: row.get(5)?,
        })).map_err(database_error)?.collect::<Result<Vec<_>, _>>().map_err(database_error)
    })?;
    for intent in intents {
        if intent.phase == "prepared" {
            if owns_directory(&intent.source, &intent) {
                fs::remove_file(intent.source.join(MARKER))
                    .map_err(|error| format!("Could not release transfer source: {error}"))?;
            }
            match fs::remove_dir_all(&intent.temporary) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("Could not clean incomplete transfer: {error}")),
            }
            database.with_connection(|connection| {
                connection
                    .execute(
                        "DELETE FROM game_transfers WHERE game_id = ?1",
                        [&intent.game_id],
                    )
                    .map_err(database_error)?;
                Ok(())
            })?;
        } else {
            if intent.phase == "copied" {
                commit_transfer(database, &intent)?;
            }
            cleanup_transfer(database, &intent)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn transfer_game(
    app: AppHandle,
    game_id: String,
    source_directory: String,
    destination_directory: String,
) -> Result<TransferResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        transfer(&app, &game_id, &source_directory, &destination_directory)
    })
    .await
    .map_err(|error| format!("Transfer task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{CreateGameInput, NativeLaunchConfig};

    fn fixture(phase: &str) -> (PathBuf, Database, TransferIntent) {
        let root = std::env::temp_dir().join(format!("legio-transfer-test-{}", Uuid::new_v4()));
        let source = root.join("source");
        fs::create_dir_all(source.join("bin")).unwrap();
        fs::write(source.join("bin/game.exe"), b"game").unwrap();
        let database = Database::open(&root.join("data")).unwrap();
        let game = database
            .create_game(CreateGameInput {
                name: "Test".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        database.with_connection(|connection| {
            connection.execute("UPDATE games SET executable_path = ?2, installation_root = ?3 WHERE id = ?1",
                params![game.id, source.join("bin/game.exe").to_string_lossy(), source.to_string_lossy()]).map_err(database_error)?;
            Ok(())
        }).unwrap();
        database
            .save_native_launch_config(
                &game.id,
                NativeLaunchConfig {
                    arguments: vec!["--keep".to_owned()],
                    working_directory: Some(source.join("bin").to_string_lossy().into_owned()),
                },
            )
            .unwrap();
        let intent = TransferIntent {
            game_id: game.id,
            source,
            destination: root.join("target"),
            temporary: root.join(".legio-transfer-test"),
            relative: PathBuf::from("bin/game.exe"),
            phase: phase.to_owned(),
        };
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "INSERT INTO game_transfers VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            intent.game_id,
                            intent.source.to_string_lossy(),
                            intent.destination.to_string_lossy(),
                            intent.temporary.to_string_lossy(),
                            intent.relative.to_string_lossy(),
                            phase
                        ],
                    )
                    .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        fs::write(
            intent.source.join(MARKER),
            intent.temporary.to_string_lossy().as_bytes(),
        )
        .unwrap();
        (root, database, intent)
    }

    #[test]
    fn recovers_published_copy_after_database_reopen() {
        let (root, database, intent) = fixture("copied");
        copy_directory(&intent.source, &intent.temporary).unwrap();
        fs::write(
            intent.temporary.join(MARKER),
            intent.temporary.to_string_lossy().as_bytes(),
        )
        .unwrap();
        finalize_install::publish_noreplace(&intent.temporary, &intent.destination).unwrap();
        drop(database);
        let database = Database::open(&root.join("data")).unwrap();
        recover(&database).unwrap();
        let game = database.game(&intent.game_id).unwrap();
        assert_eq!(
            game.executable_path.as_deref(),
            intent.destination.join(&intent.relative).to_str()
        );
        assert_eq!(
            game.installation_root.as_deref(),
            intent.destination.to_str()
        );
        assert_eq!(
            database
                .native_launch_config(&intent.game_id)
                .unwrap()
                .working_directory
                .as_deref(),
            intent.destination.join("bin").to_str()
        );
        assert!(!intent.source.exists());
        assert_eq!(
            fs::read(intent.destination.join(&intent.relative)).unwrap(),
            b"game"
        );
        recover(&database).unwrap();
        drop(database);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incomplete_copy_keeps_source_and_database_paths() {
        let (root, database, intent) = fixture("prepared");
        fs::create_dir(&intent.temporary).unwrap();
        fs::write(intent.temporary.join("partial"), b"partial").unwrap();
        recover(&database).unwrap();
        assert!(intent.source.join(&intent.relative).exists());
        assert!(!intent.temporary.exists());
        assert_eq!(
            database
                .game(&intent.game_id)
                .unwrap()
                .installation_root
                .as_deref(),
            intent.source.to_str()
        );
        drop(database);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn database_failure_preserves_source_and_all_old_paths() {
        let (root, database, intent) = fixture("copied");
        copy_directory(&intent.source, &intent.temporary).unwrap();
        fs::write(
            intent.temporary.join(MARKER),
            intent.temporary.to_string_lossy().as_bytes(),
        )
        .unwrap();
        database.with_connection(|connection| {
            connection.execute_batch("CREATE TRIGGER fail_transfer BEFORE UPDATE ON games BEGIN SELECT RAISE(ABORT, 'injected'); END;").map_err(database_error)
        }).unwrap();
        assert!(commit_transfer(&database, &intent).is_err());
        assert_eq!(
            database
                .native_launch_config(&intent.game_id)
                .unwrap()
                .working_directory
                .as_deref(),
            intent.source.join("bin").to_str()
        );
        assert!(intent.source.join(&intent.relative).exists());
        database
            .with_connection(|connection| {
                connection
                    .execute_batch("DROP TRIGGER fail_transfer;")
                    .map_err(database_error)
            })
            .unwrap();
        recover(&database).unwrap();
        drop(database);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remaps_only_paths_inside_the_source() {
        let source = Path::new("/games/Game");
        let target = Path::new("/other/Game");
        assert_eq!(
            remap(Some("/games/Game/bin".to_owned()), source, target).as_deref(),
            Some("/other/Game/bin")
        );
        assert_eq!(
            remap(Some("/games/GameTwo/prefix".to_owned()), source, target).as_deref(),
            Some("/games/GameTwo/prefix")
        );
        assert_eq!(remap(None, source, target), None);
    }
}
