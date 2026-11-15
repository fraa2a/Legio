use crate::database::{
    DATABASE_FILE_NAME, DatabaseState,
    backup::{self, DatabaseBackup},
    now_milliseconds,
};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager, State};

// SQLite files that belong to the main database file and must move with it.
const COMPANION_SUFFIXES: [&str; 3] = ["-wal", "-shm", "-journal"];

struct SetAside {
    file_name: String,
    moved: Vec<(PathBuf, PathBuf)>,
}

fn move_back(moved: &[(PathBuf, PathBuf)]) -> Result<(), String> {
    for (source, target) in moved.iter().rev() {
        fs::rename(target, source).map_err(|error| {
            format!(
                "could not move {} back to {}: {error}",
                target.display(),
                source.display()
            )
        })?;
    }
    Ok(())
}

/// Renames the database and its companion files aside; nothing is deleted.
fn set_aside(data_dir: &Path) -> Result<Option<SetAside>, String> {
    match fs::symlink_metadata(data_dir.join(DATABASE_FILE_NAME)) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("could not inspect the local database: {error}")),
    }
    let file_name = format!("legio-broken-{}.sqlite3", now_milliseconds()?);
    if fs::symlink_metadata(data_dir.join(&file_name)).is_ok() {
        return Err(format!("{file_name} already exists"));
    }
    let mut moved = Vec::new();
    for suffix in std::iter::once("").chain(COMPANION_SUFFIXES) {
        let source = data_dir.join(format!("{DATABASE_FILE_NAME}{suffix}"));
        let target = data_dir.join(format!("{file_name}{suffix}"));
        match fs::rename(&source, &target) {
            Ok(()) => moved.push((source, target)),
            Err(error) if !suffix.is_empty() && error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                let error = format!("could not set the local database aside: {error}");
                return Err(match move_back(&moved) {
                    Ok(()) => error,
                    Err(rollback) => format!("{error}; {rollback}"),
                });
            }
        }
    }
    crate::finalize_install::sync_directory(data_dir)?;
    Ok(Some(SetAside { file_name, moved }))
}

/// Replaces the database with a verified copy of a backup, keeping the current file aside.
fn restore_backup(data_dir: &Path, file_name: &str) -> Result<(), String> {
    let backup = backup::list_backups(data_dir)?
        .into_iter()
        .find(|backup| backup.file_name == file_name)
        .ok_or_else(|| "the selected database backup no longer exists".to_owned())?;
    let temporary = data_dir.join(format!(
        ".{DATABASE_FILE_NAME}.restore-{}.tmp",
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        fs::copy(
            backup::backup_directory(data_dir).join(&backup.file_name),
            &temporary,
        )
        .and_then(|_| backup::sync_file(&temporary))
        .map_err(|error| format!("could not copy the database backup: {error}"))?;
        backup::verify_database_copy(&temporary)?;
        let aside = set_aside(data_dir)?;
        if let Err(error) = fs::rename(&temporary, data_dir.join(DATABASE_FILE_NAME)) {
            let error = format!("could not restore the database backup: {error}");
            return Err(match aside.map(|aside| move_back(&aside.moved)) {
                Some(Err(rollback)) => format!("{error}; {rollback}"),
                _ => error,
            });
        }
        crate::finalize_install::sync_directory(data_dir)?;
        Ok(aside.map(|aside| aside.file_name))
    })();
    match result {
        Ok(aside) => {
            crate::application_log::state_change(
                "database_restore",
                &format!(
                    "Restored {} (previous database: {})",
                    backup.file_name,
                    aside.as_deref().unwrap_or("none")
                ),
            );
            Ok(())
        }
        Err(error) => {
            for suffix in std::iter::once("").chain(COMPANION_SUFFIXES) {
                let path = PathBuf::from(format!("{}{suffix}", temporary.display()));
                if let Err(cleanup) = fs::remove_file(&path)
                    && cleanup.kind() != io::ErrorKind::NotFound
                {
                    return Err(format!("{error}; restore cleanup failed: {cleanup}"));
                }
            }
            Err(error)
        }
    }
}

/// Sets the database aside so the next start creates a new one.
fn reset_database(data_dir: &Path) -> Result<(), String> {
    let aside = set_aside(data_dir)?
        .ok_or_else(|| "there is no local database file to set aside".to_owned())?;
    crate::application_log::state_change(
        "database_reset",
        &format!("Previous database kept as {}", aside.file_name),
    );
    Ok(())
}

fn data_directory(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("Could not locate app data: {error}"))
}

// The open database holds the file, so it is only replaced after a failed start.
fn failed_data_directory(app: &AppHandle) -> Result<PathBuf, String> {
    if app.state::<DatabaseState>().database().is_ok() {
        return Err("The local database is working and cannot be replaced".to_owned());
    }
    data_directory(app)
}

async fn run_blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| format!("Database recovery task failed: {error}"))?
}

#[tauri::command]
pub async fn get_database_failure(
    state: State<'_, DatabaseState>,
) -> Result<Option<String>, String> {
    Ok(state.database().err())
}

#[tauri::command]
pub async fn list_database_backups(app: AppHandle) -> Result<Vec<DatabaseBackup>, String> {
    let data_dir = data_directory(&app)?;
    run_blocking(move || backup::list_backups(&data_dir)).await
}

#[tauri::command]
pub async fn open_data_folder(app: AppHandle) -> Result<(), String> {
    let data_dir = data_directory(&app)?;
    run_blocking(move || crate::installed_folder::open_directory(&data_dir)).await
}

#[tauri::command]
pub async fn restore_database_backup(app: AppHandle, file_name: String) -> Result<(), String> {
    let data_dir = failed_data_directory(&app)?;
    run_blocking(move || restore_backup(&data_dir, &file_name)).await
}

#[tauri::command]
pub async fn reset_local_database(app: AppHandle) -> Result<(), String> {
    let data_dir = failed_data_directory(&app)?;
    run_blocking(move || reset_database(&data_dir)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn temporary_directory() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "legio-database-recovery-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn write_database(path: &Path, version: i64, name: &str) {
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(&format!(
                "CREATE TABLE marker (name TEXT NOT NULL);
                 INSERT INTO marker VALUES ('{name}');
                 PRAGMA user_version = {version};"
            ))
            .unwrap();
    }

    fn marker(path: &Path) -> String {
        Connection::open(path)
            .unwrap()
            .query_row("SELECT name FROM marker", [], |row| row.get(0))
            .unwrap()
    }

    fn broken_files(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.starts_with("legio-broken-"))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn restore_replaces_the_database_and_keeps_the_broken_one() {
        let directory = temporary_directory();
        let backups = backup::backup_directory(&directory);
        fs::create_dir_all(&backups).unwrap();
        write_database(&backups.join("legio-v30-5.sqlite3"), 30, "backup");
        fs::write(directory.join(DATABASE_FILE_NAME), b"not a database").unwrap();
        fs::write(directory.join("legio.sqlite3-wal"), b"stale wal").unwrap();

        restore_backup(&directory, "legio-v30-5.sqlite3").unwrap();

        assert_eq!(marker(&directory.join(DATABASE_FILE_NAME)), "backup");
        assert!(!directory.join("legio.sqlite3-wal").exists());
        let broken = broken_files(&directory);
        assert_eq!(broken.len(), 2);
        assert_eq!(
            fs::read(directory.join(&broken[0])).unwrap(),
            b"not a database"
        );
        assert!(broken[1].ends_with(".sqlite3-wal"));
        assert!(backups.join("legio-v30-5.sqlite3").exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn damaged_or_unknown_backup_leaves_the_database_in_place() {
        let directory = temporary_directory();
        let backups = backup::backup_directory(&directory);
        fs::create_dir_all(&backups).unwrap();
        fs::write(backups.join("legio-v30-5.sqlite3"), b"garbage").unwrap();
        fs::write(directory.join(DATABASE_FILE_NAME), b"current").unwrap();

        assert!(restore_backup(&directory, "legio-v30-5.sqlite3").is_err());
        assert!(restore_backup(&directory, "../legio-v30-5.sqlite3").is_err());

        assert_eq!(
            fs::read(directory.join(DATABASE_FILE_NAME)).unwrap(),
            b"current"
        );
        assert!(broken_files(&directory).is_empty());
        let leftovers: Vec<_> = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reset_keeps_the_old_database() {
        let directory = temporary_directory();
        write_database(&directory.join(DATABASE_FILE_NAME), 39, "old");

        reset_database(&directory).unwrap();

        assert!(!directory.join(DATABASE_FILE_NAME).exists());
        let broken = broken_files(&directory);
        assert_eq!(broken.len(), 1);
        assert_eq!(marker(&directory.join(&broken[0])), "old");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reset_without_a_database_fails() {
        let directory = temporary_directory();
        assert!(reset_database(&directory).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
