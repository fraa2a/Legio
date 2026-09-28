use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::{
    database::{DatabaseState, Game, database_error},
    game_lifecycle::{GameLaunchManager, GameStatus},
    manual_import,
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
            let mut input = fs::File::open(&from)
                .map_err(|error| format!("Could not open source file: {error}"))?;
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
    Ok(())
}

fn transfer(
    app: &AppHandle,
    game_id: &str,
    source: &str,
    destination: &str,
) -> Result<TransferResult, String> {
    let database = app.state::<DatabaseState>();
    let game = database.database()?.game(game_id)?;
    if game.steam_install_path.is_some() {
        return Err("Steam-managed games must be transferred through Steam".to_owned());
    }
    if app
        .state::<GameLaunchManager>()
        .list()?
        .iter()
        .any(|state| state.game_id == game_id && state.status != GameStatus::Idle)
    {
        return Err("Stop the game before transferring its files".to_owned());
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
    let installed_path: Option<String> = database.database()?.with_connection(|connection| {
        connection
            .query_row(
                "SELECT final_path FROM downloads WHERE id = ?1 AND status = 'installed'",
                [game_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(database_error)
    })?;
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
    let new_executable = destination.join(relative);
    let moved = match fs::rename(&source, &destination) {
        Ok(()) => true,
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            let temporary = parent.join(format!(".legio-transfer-{}", Uuid::new_v4()));
            if let Err(error) = copy_directory(&source, &temporary).and_then(|_| {
                fs::rename(&temporary, &destination)
                    .map_err(|error| format!("Could not publish transferred game: {error}"))
            }) {
                let _ = fs::remove_dir_all(&temporary);
                return Err(error);
            }
            false
        }
        Err(error) => return Err(format!("Could not move game directory: {error}")),
    };
    let updated =
        manual_import::set_executable(&database, game_id, &new_executable.to_string_lossy());
    let updated = match updated {
        Ok(updated) => updated,
        Err(error) => {
            if moved {
                fs::rename(&destination, &source)
                    .map_err(|rollback| format!("{error}; could not restore source: {rollback}"))?;
            } else {
                fs::remove_dir_all(&destination).map_err(|rollback| {
                    format!("{error}; could not remove copied game: {rollback}")
                })?;
            }
            return Err(error);
        }
    };
    let mut warnings = Vec::new();
    if installed_path.is_some() {
        if let Err(error) = database.database()?.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE downloads SET final_path = ?2 WHERE id = ?1 AND status = 'installed'",
                    params![game_id, destination.to_string_lossy()],
                )
                .map_err(database_error)?;
            Ok(())
        }) {
            warnings.push(format!(
                "Could not update installed download location: {error}"
            ));
        }
    }
    let cleanup_warning = if moved {
        None
    } else {
        fs::remove_dir_all(&source)
            .err()
            .map(|error| format!("Game transferred, but could not remove the old copy: {error}"))
    };
    if let Some(warning) = cleanup_warning {
        warnings.push(warning);
    }
    Ok(TransferResult {
        game: updated,
        warning: (!warnings.is_empty()).then(|| warnings.join("; ")),
    })
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
