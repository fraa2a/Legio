use crate::database::{Database, database_error};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct ChangedFile {
    path: String,
    existed: bool,
}
#[derive(Serialize, Deserialize)]
struct Overlay {
    files: Vec<ChangedFile>,
    directories: Vec<String>,
}

fn metadata(path: &Path) -> Result<Option<fs::Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(value) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if value.file_attributes() & 0x400 != 0 {
                    return Err("Update path traverses a reparse point".into());
                }
            }
            if value.file_type().is_symlink() {
                return Err("Update path traverses a symbolic link".into());
            }
            Ok(Some(value))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Could not inspect update path: {error}")),
    }
}

fn directory(path: &Path) -> Result<(), String> {
    if !metadata(path)?.is_some_and(|value| value.is_dir()) {
        return Err("Update directory is not a regular directory".into());
    }
    Ok(())
}

fn relative(value: &str) -> Result<PathBuf, String> {
    let path = crate::archive_install::safe_relative_path(value)?;
    if path.components().any(|part| {
        part.as_os_str()
            .to_string_lossy()
            .to_ascii_lowercase()
            .starts_with(".legio-")
    }) {
        return Err("Archive contains a reserved Legio path".into());
    }
    Ok(path)
}

fn checked(root: &Path, value: &str) -> Result<PathBuf, String> {
    directory(root)?;
    let relative = relative(value)?;
    let mut current = root.to_path_buf();
    for part in relative.components() {
        current.push(part);
        if let Some(value) = metadata(&current)?
            && current != root.join(&relative)
            && !value.is_dir()
        {
            return Err("Update path traverses a non-directory".into());
        }
    }
    Ok(current)
}

fn inventory(stage: &Path, target: &Path) -> Result<Overlay, String> {
    directory(stage)?;
    directory(target)?;
    let mut result = Overlay {
        files: Vec::new(),
        directories: Vec::new(),
    };
    let mut pending = vec![stage.to_path_buf()];
    while let Some(folder) = pending.pop() {
        for item in fs::read_dir(&folder).map_err(|error| error.to_string())? {
            let item = item.map_err(|error| error.to_string())?;
            let path = item.path();
            let name = crate::manual_import::portable_relative_path(
                path.strip_prefix(stage)
                    .map_err(|error| error.to_string())?,
            )?;
            let source = metadata(&path)?.ok_or("Staged file disappeared")?;
            let destination = checked(target, &name)?;
            let old = metadata(&destination)?;
            if source.is_dir() {
                if old.as_ref().is_some_and(|value| !value.is_dir()) {
                    return Err("Update directory conflicts with an installed file".into());
                }
                if old.is_none() {
                    result.directories.push(name);
                }
                pending.push(path);
            } else if source.is_file() {
                if old.as_ref().is_some_and(|value| !value.is_file()) {
                    return Err("Update file conflicts with an installed directory".into());
                }
                result.files.push(ChangedFile {
                    path: name,
                    existed: old.is_some(),
                });
            } else {
                return Err("Update contains a non-regular file".into());
            }
        }
    }
    if result.files.is_empty() {
        return Err("Update archive contains no files".into());
    }
    result
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    result.directories.sort_by_key(|value| {
        relative(value)
            .map(|path| path.components().count())
            .unwrap_or(usize::MAX)
    });
    Ok(result)
}

fn temporary(destination: &Path, job: &str) -> Result<PathBuf, String> {
    Ok(destination
        .parent()
        .ok_or("Update file has no parent")?
        .join(format!(".legio-update-{job}.tmp")))
}

fn atomic_copy(source: &Path, destination: &Path, job: &str) -> Result<(), String> {
    if !metadata(source)?.is_some_and(|value| value.is_file()) {
        return Err("Update copy source is not a regular file".into());
    }
    let temporary = temporary(destination, job)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("Could not create replacement file: {error}"))?;
    let mut input = fs::File::open(source).map_err(|error| error.to_string())?;
    crate::filesystem_copy::copy(&mut input, &mut output).map_err(|error| error.to_string())?;
    fs::set_permissions(
        &temporary,
        input
            .metadata()
            .map_err(|error| error.to_string())?
            .permissions(),
    )
    .map_err(|error| error.to_string())?;
    output
        .sync_all()
        .map_err(|error| format!("Could not flush replacement file: {error}"))?;
    drop(output);
    fs::rename(&temporary, destination)
        .map_err(|error| format!("Could not publish replacement file: {error}"))?;
    crate::finalize_install::sync_directory(
        destination.parent().ok_or("Update file has no parent")?,
    )
}

fn equal_files(left: &Path, right: &Path) -> Result<bool, String> {
    use sha2::{Digest, Sha256};
    fn hash(path: &Path) -> Result<Vec<u8>, String> {
        let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
        let mut hash = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let size = file.read(&mut buffer).map_err(|error| error.to_string())?;
            if size == 0 {
                break;
            }
            hash.update(&buffer[..size]);
        }
        Ok(hash.finalize().to_vec())
    }
    Ok(metadata(left)?.is_some_and(|value| value.is_file()) && hash(left)? == hash(right)?)
}

fn rollback(target: &Path, backup: &Path, overlay: &Overlay, job: &str) -> Result<(), String> {
    directory(target)?;
    directory(backup)?;
    for file in overlay.files.iter().rev() {
        let destination = checked(target, &file.path)?;
        let temp = temporary(&destination, job)?;
        if metadata(&temp)?.is_some() {
            fs::remove_file(&temp).map_err(|error| error.to_string())?;
        }
        if file.existed {
            let saved = checked(backup, &file.path)?;
            if !equal_files(&destination, &saved)? {
                atomic_copy(&saved, &destination, job)?;
            }
        } else if metadata(&destination)?.is_some() {
            fs::remove_file(&destination).map_err(|error| error.to_string())?;
        }
    }
    for name in overlay.directories.iter().rev() {
        let directory = checked(target, name)?;
        match fs::remove_dir(&directory) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
                ) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    crate::finalize_install::sync_directory(target)
}

fn cleanup(database: &Database, job: &str, backup: &Path) -> Result<(), String> {
    if metadata(backup)?.is_some() {
        directory(backup)?;
        crate::finalize_install::remove_stage_directory(backup)?;
    }
    database.with_connection(|connection| {
        connection
            .execute("DELETE FROM game_update_journals WHERE job_id = ?1", [job])
            .map_err(database_error)?;
        Ok(())
    })
}

pub(crate) fn ensure_launchable(database: &Database, game: &str) -> Result<(), String> {
    let blocked: bool = database.with_connection(|connection| connection.query_row("SELECT EXISTS(SELECT 1 FROM game_update_journals WHERE game_id = ?1 AND committed = 0)", [game], |row| row.get(0)).map_err(database_error))?;
    if blocked {
        Err("Recover the interrupted game update before launching this game".into())
    } else {
        Ok(())
    }
}

pub(crate) fn apply(
    database: &Database,
    job: &str,
    stage: &Path,
    executable: &str,
) -> Result<(), String> {
    let job = uuid::Uuid::parse_str(job)
        .map_err(|error| error.to_string())?
        .to_string();
    let game_id =
        crate::game_updates::target(database, &job)?.ok_or("Update has no target game")?;
    ensure_launchable(database, &game_id)?;
    let game = database.game(&game_id)?;
    let app_id: u32 = database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT steam_app_id FROM downloads WHERE id = ?1",
                [&job],
                |row| row.get(0),
            )
            .map_err(database_error)
    })?;
    if game.steam_app_id != Some(app_id) {
        return Err("Update no longer matches the game's Steam identity".into());
    }
    let original = Path::new(
        game.installation_root
            .as_deref()
            .ok_or("Game has no installation directory")?,
    );
    directory(original)?;
    let target = dunce::canonicalize(original).map_err(|error| error.to_string())?;
    if crate::download_policy::external_busy(&target)? {
        return Err("Close the game before installing its update".into());
    }
    let stage = dunce::canonicalize(stage).map_err(|error| error.to_string())?;
    if stage.starts_with(&target) || target.starts_with(&stage) {
        return Err("Update staging and installation overlap".into());
    }
    let stored: Option<String> = database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT staged_path FROM downloads WHERE id = ?1 AND status = 'staged'",
                [&job],
                |row| row.get(0),
            )
            .optional()
            .map(|value| value.flatten())
            .map_err(database_error)
    })?;
    if stored
        .as_deref()
        .map(Path::new)
        .map(dunce::canonicalize)
        .transpose()
        .map_err(|error| error.to_string())?
        .as_ref()
        != Some(&stage)
    {
        return Err("Update is not staged at the recorded path".into());
    }
    let archive_stage = stage.clone();
    let original_executable = relative(executable)?;
    checked(&archive_stage, executable)?;
    let legacy: bool = database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT archive_layout = 'legacy' FROM games WHERE id = ?1",
                [&game_id],
                |row| row.get(0),
            )
            .map_err(database_error)
    })?;
    let stage = if legacy {
        stage
    } else {
        let detected = crate::game_library::game_root(&stage.join(&original_executable))?;
        if !detected.starts_with(&stage) {
            return Err("Detected update directory escapes its archive".into());
        }
        detected
    };
    let executable = archive_stage
        .join(&original_executable)
        .strip_prefix(&stage)
        .map_err(|error| error.to_string())?
        .to_path_buf();
    let overlay = inventory(&stage, &target)?;
    if !metadata(&checked(&stage, &executable.to_string_lossy())?)?
        .is_some_and(|value| value.is_file())
        && !metadata(&checked(&target, &executable.to_string_lossy())?)?
            .is_some_and(|value| value.is_file())
    {
        return Err("No valid game executable is available after the update".into());
    }
    let backup = target.join(format!(".legio-update-{job}"));
    fs::create_dir(&backup).map_err(|error| format!("Could not create update backup: {error}"))?;
    let prepared = (|| {
        for file in overlay.files.iter().filter(|file| file.existed) {
            let source = checked(&target, &file.path)?;
            let saved = backup.join(relative(&file.path)?);
            fs::create_dir_all(saved.parent().ok_or("Backup has no parent")?)
                .map_err(|error| error.to_string())?;
            let mut input = fs::File::open(&source)
                .map_err(|error| format!("Could not open installed file for backup: {error}"))?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&saved)
                .map_err(|error| format!("Could not create update backup file: {error}"))?;
            crate::filesystem_copy::copy(&mut input, &mut output)
                .map_err(|error| format!("Could not back up installed file: {error}"))?;
            output
                .sync_all()
                .map_err(|error| format!("Could not flush update backup: {error}"))?;
            fs::set_permissions(
                &saved,
                input
                    .metadata()
                    .map_err(|error| error.to_string())?
                    .permissions(),
            )
            .map_err(|error| format!("Could not preserve backup permissions: {error}"))?;
        }
        crate::finalize_install::sync_directory(&backup)?;
        let manifest = serde_json::to_string(&overlay).map_err(|error| error.to_string())?;
        database.with_connection(|connection| { connection.execute("INSERT INTO game_update_journals (job_id,game_id,target,backup,manifest) VALUES (?1,?2,?3,?4,?5)", params![job,game_id,target.to_string_lossy(),backup.to_string_lossy(),manifest]).map_err(database_error)?; Ok(()) })
    })();
    if let Err(error) = prepared {
        crate::finalize_install::remove_stage_directory(&backup)
            .map_err(|cleanup| format!("{error}; backup cleanup failed: {cleanup}"))?;
        return Err(error);
    }
    let outcome = (|| {
        for name in &overlay.directories {
            let path = checked(&target, name)?;
            fs::create_dir(&path).map_err(|error| error.to_string())?;
        }
        for file in &overlay.files {
            atomic_copy(
                &checked(&stage, &file.path)?,
                &checked(&target, &file.path)?,
                &job,
            )?;
        }
        database.with_connection(|connection| {
            let transaction = connection.unchecked_transaction().map_err(database_error)?;
            let changed = transaction.execute("UPDATE downloads SET status = 'installed', error = NULL, updated_at = CAST(strftime('%s','now') AS INTEGER) WHERE id = ?1 AND status = 'staged'", [&job]).map_err(database_error)?;
            if changed != 1 { return Err("Update state changed before commit".into()); }
            transaction.execute("UPDATE games SET executable_path = ?2 WHERE id = ?1", params![game_id,target.join(&executable).to_string_lossy()]).map_err(database_error)?;
            crate::game_updates::record_install_on(&transaction, &job, &game_id)?;
            transaction.execute("UPDATE game_update_journals SET committed = 1 WHERE job_id = ?1", [&job]).map_err(database_error)?;
            transaction.commit().map_err(database_error)
        })
    })();
    if let Err(error) = outcome {
        if let Err(restore) = rollback(&target, &backup, &overlay, &job) {
            return Err(format!(
                "{error}; update rollback failed: {restore}. Installation is locked until recovery"
            ));
        }
        cleanup(database, &job, &backup)?;
        return Err(format!("{error}; previous game files restored"));
    }
    cleanup(database, &job, &backup)?;
    crate::game_library::flush_registered(database);
    crate::finalize_install::remove_stage_directory(&archive_stage)?;
    database.with_connection(|connection| {
        connection
            .execute(
                "UPDATE downloads SET staged_path = NULL WHERE id = ?1",
                [&job],
            )
            .map_err(database_error)?;
        Ok(())
    })
}

pub(crate) fn recover(database: &Database) -> Result<(), String> {
    let records: Vec<(String,String,String,String,String,bool)> = database.with_connection(|connection| {
        let mut query = connection.prepare("SELECT job_id, game_id, target, backup, manifest, committed FROM game_update_journals").map_err(database_error)?;
        query.query_map([], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?))).map_err(database_error)?.collect::<Result<_,_>>().map_err(database_error)
    })?;
    for (job, game, target, backup, manifest, committed) in records {
        let result = (|| -> Result<(), String> {
            let parsed = uuid::Uuid::parse_str(&job)
                .map_err(|error| error.to_string())?
                .to_string();
            let target = PathBuf::from(target);
            let backup = PathBuf::from(backup);
            let expected = target.join(format!(".legio-update-{parsed}"));
            let install = database
                .game(&game)?
                .installation_root
                .ok_or("Update game has no directory")?;
            if dunce::canonicalize(install).map_err(|error| error.to_string())? != target
                || backup != expected
            {
                return Err("Update journal does not match its installation".into());
            }
            let overlay: Overlay =
                serde_json::from_str(&manifest).map_err(|error| error.to_string())?;
            if !committed {
                if crate::download_policy::external_busy(&target)? {
                    return Err("Close the game before recovering its interrupted update".into());
                }
                rollback(&target, &backup, &overlay, &job)?;
                database.with_connection(|connection| { connection.execute("UPDATE downloads SET status = 'staged', error = 'Interrupted update restored. Retry installation.' WHERE id = ?1", [&job]).map_err(database_error)?; Ok(()) })?;
            }
            cleanup(database, &job, &backup)?;
            Ok(())
        })();
        if let Err(error) = result {
            database.with_connection(|c| {
                c.execute(
                    "UPDATE downloads SET error=?2 WHERE id=?1",
                    params![job, error],
                )
                .map_err(database_error)?;
                Ok(())
            })?;
            crate::application_log::failure("recover_game_update", error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{CreateGameInput, Database};
    use std::fs;
    fn fixture() -> (std::path::PathBuf, Database, String, String) {
        let root = std::env::temp_dir().join(format!("legio-overlay-{}", uuid::Uuid::new_v4()));
        let database = Database::open(&root.join("db")).unwrap();
        let game = database
            .create_game(CreateGameInput {
                name: "Game".into(),
                steam_app_id: Some(400),
            })
            .unwrap();
        let target = root.join("game");
        let stage = root.join("stage");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(stage.join("assets")).unwrap();
        fs::write(target.join("game.exe"), b"old").unwrap();
        fs::write(target.join("keep.ini"), b"custom").unwrap();
        fs::write(stage.join("game.exe"), b"new").unwrap();
        fs::write(stage.join("assets/new.dat"), b"data").unwrap();
        let job = uuid::Uuid::new_v4().to_string();
        database.with_connection(|connection| {
            connection.execute("UPDATE games SET executable_path = ?2, installation_root = ?3 WHERE id = ?1", rusqlite::params![game.id, target.join("game.exe").to_string_lossy(), target.to_string_lossy()]).unwrap();
            connection.execute("INSERT INTO downloads (id,steam_app_id,name,release_version,url,sha256,size_bytes,status,created_at,updated_at,staged_path,target_game_id) VALUES (?1,400,'Game','2','https://example.test/game.zip','',1,'staged',1,1,?2,?3)", rusqlite::params![job, stage.to_string_lossy(), game.id]).unwrap(); Ok(())
        }).unwrap();
        (root, database, game.id, job)
    }
    #[test]
    fn overlays_only_archive_files_and_keeps_game_identity() {
        let (root, database, game, job) = fixture();
        apply(&database, &job, &root.join("stage"), "game.exe").unwrap();
        assert_eq!(fs::read(root.join("game/game.exe")).unwrap(), b"new");
        assert_eq!(fs::read(root.join("game/keep.ini")).unwrap(), b"custom");
        assert_eq!(fs::read(root.join("game/assets/new.dat")).unwrap(), b"data");
        assert_eq!(database.games().unwrap().len(), 1);
        assert_eq!(database.games().unwrap()[0].id, game);
        drop(database);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn commit_failure_restores_old_files_and_removes_only_new_files() {
        let (root, database, _, job) = fixture();
        database.with_connection(|connection| { connection.execute_batch("CREATE TRIGGER fail_update BEFORE UPDATE OF status ON downloads WHEN NEW.status = 'installed' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap(); Ok(()) }).unwrap();
        assert!(apply(&database, &job, &root.join("stage"), "game.exe").is_err());
        assert_eq!(fs::read(root.join("game/game.exe")).unwrap(), b"old");
        assert_eq!(fs::read(root.join("game/keep.ini")).unwrap(), b"custom");
        assert!(!root.join("game/assets/new.dat").exists());
        assert!(!root.join("game/assets").exists());
        drop(database);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn recovers_interrupted_overwrite_after_database_reopen() {
        let (root, database, game, job) = fixture();
        let target = dunce::canonicalize(root.join("game")).unwrap();
        let stage = root.join("stage");
        let overlay = inventory(&stage, &target).unwrap();
        let backup = target.join(format!(".legio-update-{job}"));
        fs::create_dir(&backup).unwrap();
        fs::copy(target.join("game.exe"), backup.join("game.exe")).unwrap();
        database.with_connection(|c| {
            c.execute("INSERT INTO game_update_journals (job_id,game_id,target,backup,manifest) VALUES (?1,?2,?3,?4,?5)", params![job,game,target.to_string_lossy(),backup.to_string_lossy(),serde_json::to_string(&overlay).unwrap()]).map_err(database_error)?;
            Ok(())
        }).unwrap();
        fs::write(target.join("game.exe"), b"interrupted").unwrap();
        fs::create_dir(target.join("assets")).unwrap();
        fs::write(target.join("assets/new.dat"), b"new").unwrap();
        assert!(ensure_launchable(&database, &game).is_err());
        drop(database);
        let database = Database::open(&root.join("db")).unwrap();
        recover(&database).unwrap();
        assert_eq!(fs::read(target.join("game.exe")).unwrap(), b"old");
        assert_eq!(fs::read(target.join("keep.ini")).unwrap(), b"custom");
        assert!(!target.join("assets").exists());
        ensure_launchable(&database, &game).unwrap();
        drop(database);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_destination_symlinks_before_changing_any_file() {
        let (root, database, _, job) = fixture();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("game/assets")).unwrap();
        assert!(apply(&database, &job, &root.join("stage"), "game.exe").is_err());
        assert_eq!(fs::read(root.join("game/game.exe")).unwrap(), b"old");
        assert!(!outside.join("new.dat").exists());
        drop(database);
        fs::remove_dir_all(root).unwrap();
    }
}
