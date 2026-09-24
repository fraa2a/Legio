use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{OptionalExtension, params};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::{
    archive_install::safe_relative_path,
    database::{Database, DatabaseState, database_error},
};

const MARKER: &str = ".legio-install-intent";
const COPY_TOKEN: &str = ".legio-copy-token";

struct Intent {
    id: String,
    app_id: i64,
    name: String,
    stage: PathBuf,
    target: PathBuf,
    executable: PathBuf,
    token: String,
}

type StoredIntent = (
    i64,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn timestamp() -> Result<i64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok())
        .ok_or_else(|| "System clock is outside the supported date range".to_owned())
}

fn install_root(data_dir: &Path) -> Result<PathBuf, String> {
    let root = data_dir.join("installed");
    fs::create_dir_all(&root)
        .map_err(|error| format!("Could not create install directory: {error}"))?;
    let metadata = fs::symlink_metadata(&root)
        .map_err(|error| format!("Could not inspect install directory: {error}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Install directory is not a regular directory".to_owned());
    }
    Ok(root)
}

fn regular_executable(root: &Path, relative: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("Could not inspect executable root: {error}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Executable root is not a regular directory".to_owned());
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current)
            .map_err(|error| format!("Could not inspect selected executable: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("Selected executable traverses a symbolic link".to_owned());
        }
        if current == root.join(relative) {
            if !metadata.is_file() {
                return Err("Selected executable is not a regular file".to_owned());
            }
        } else if !metadata.is_dir() {
            return Err("Selected executable traverses a non-directory".to_owned());
        }
    }
    Ok(())
}

fn claim(
    database: &Database,
    data_dir: &Path,
    id: &str,
    executable: &str,
) -> Result<Intent, String> {
    let id = Uuid::parse_str(id)
        .map_err(|_| "Download ID is invalid".to_owned())?
        .to_string();
    let executable = safe_relative_path(executable)?;
    if executable == Path::new(MARKER) {
        return Err("Selected executable is reserved".to_owned());
    }
    let stage = data_dir.join("downloads").join(format!("{id}.stage"));
    let download_root = fs::symlink_metadata(data_dir.join("downloads"))
        .map_err(|error| format!("Could not inspect download directory: {error}"))?;
    if !download_root.is_dir() || download_root.file_type().is_symlink() {
        return Err("Download directory is not a regular directory".to_owned());
    }
    regular_executable(&stage, &executable)?;
    let target = install_root(data_dir)?.join(&id);
    let token = Uuid::new_v4().to_string();
    database.with_connection(|connection| {
        let row: Option<(i64, String, Option<String>, String)> = connection.query_row(
            "SELECT steam_app_id, name, staged_path, status FROM downloads WHERE id = ?1",
            [&id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        ).optional().map_err(database_error)?;
        let Some((app_id, name, stored_stage, status)) = row else { return Err("Download was not found".to_owned()); };
        if status != "staged" { return Err(format!("Cannot finalize a {status} download")); }
        if stored_stage.as_deref() != Some(stage.to_string_lossy().as_ref()) {
            return Err("Stored stage path does not match the app-owned path".to_owned());
        }
        connection.execute(
            "UPDATE downloads SET status = 'finalizing', final_path = ?2, executable_relative = ?3, install_token = ?4, error = NULL, updated_at = ?5 WHERE id = ?1 AND status = 'staged'",
            params![id, target.to_string_lossy(), executable.to_string_lossy(), token, timestamp()?]
        ).map_err(database_error)?;
        Ok(Intent { id, app_id, name, stage, target, executable, token })
    })
}

fn load_intent(database: &Database, data_dir: &Path, id: &str) -> Result<Intent, String> {
    let id = Uuid::parse_str(id)
        .map_err(|_| "Download ID is invalid".to_owned())?
        .to_string();
    database.with_connection(|connection| {
        let row: Option<StoredIntent> = connection.query_row(
            "SELECT steam_app_id, name, staged_path, final_path, executable_relative, install_token FROM downloads WHERE id = ?1 AND status = 'finalizing'",
            [&id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        ).optional().map_err(database_error)?;
        let Some((app_id, name, stage, target, executable, token)) = row else { return Err("Finalization intent was not found".to_owned()); };
        let expected_stage = data_dir.join("downloads").join(format!("{id}.stage"));
        let expected_target = install_root(data_dir)?.join(&id);
        if stage.as_deref() != Some(expected_stage.to_string_lossy().as_ref())
            || target.as_deref() != Some(expected_target.to_string_lossy().as_ref()) {
            return Err("Finalization paths do not match the app-owned paths".to_owned());
        }
        let executable = safe_relative_path(executable.as_deref().ok_or("Finalization executable is missing")?)?;
        let token = token.ok_or("Finalization token is missing")?;
        Uuid::parse_str(&token).map_err(|_| "Finalization token is invalid".to_owned())?;
        Ok(Intent { id, app_id, name, stage: expected_stage, target: expected_target, executable, token })
    })
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    for entry in fs::read_dir(source).map_err(|error| format!("Could not read stage: {error}"))? {
        let entry = entry.map_err(|error| format!("Could not read stage entry: {error}"))?;
        let name = entry.file_name();
        if name == MARKER || name == COPY_TOKEN {
            return Err("Stage contains a reserved install marker".to_owned());
        }
        let src = entry.path();
        let dst = destination.join(name);
        let metadata = fs::symlink_metadata(&src)
            .map_err(|error| format!("Could not inspect stage entry: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("Stage contains a symbolic link".to_owned());
        }
        if metadata.is_dir() {
            fs::create_dir(&dst)
                .map_err(|error| format!("Could not create install directory: {error}"))?;
            copy_tree(&src, &dst)?;
            fs::set_permissions(&dst, metadata.permissions())
                .map_err(|error| format!("Could not set install permissions: {error}"))?;
        } else if metadata.is_file() {
            let mut input = fs::File::open(&src)
                .map_err(|error| format!("Could not open staged file: {error}"))?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dst)
                .map_err(|error| format!("Could not create install file: {error}"))?;
            io::copy(&mut input, &mut output)
                .map_err(|error| format!("Could not copy staged file: {error}"))?;
            output
                .sync_all()
                .map_err(|error| format!("Could not sync install file: {error}"))?;
            fs::set_permissions(&dst, metadata.permissions())
                .map_err(|error| format!("Could not set install permissions: {error}"))?;
        } else {
            return Err("Stage contains an unsupported file type".to_owned());
        }
    }
    sync_directory(destination)
}

fn marker_matches_with(path: &Path, name: &str, token: &str) -> Result<bool, String> {
    let marker = path.join(name);
    match fs::symlink_metadata(&marker) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            Ok(fs::read_to_string(marker)
                .map_err(|error| format!("Could not read install marker: {error}"))?
                == token)
        }
        Ok(_) => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Could not inspect install marker: {error}")),
    }
}

fn marker_matches(path: &Path, token: &str) -> Result<bool, String> {
    marker_matches_with(path, MARKER, token)
}

fn write_token(path: &Path, token: &str) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Could not create install marker: {error}"))?;
    file.write_all(token.as_bytes())
        .map_err(|error| format!("Could not write install marker: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("Could not sync install marker: {error}"))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), String> {
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("Could not sync install directory: {error}"))
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn existing(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Could not inspect install path: {error}")),
    }
}

#[cfg(target_os = "linux")]
fn publish_noreplace(source: &Path, target: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid source path"))?;
    let target = CString::new(target.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid target path"))?;
    // The C strings are NUL-terminated and remain alive for this syscall.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if matches!(
        error.raw_os_error(),
        Some(libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP)
    ) {
        return Err(io::Error::other(
            "Atomic no-replace directory rename is unavailable on this kernel or filesystem",
        ));
    }
    Err(error)
}

#[cfg(windows)]
fn publish_noreplace(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // The UTF-16 buffers are terminated and remain alive for this call.
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileW(source.as_ptr(), target.as_ptr())
    };
    if result != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn finish(database: &Database, intent: &Intent) -> Result<(), String> {
    let temporary = intent
        .target
        .with_file_name(format!(".{}.copying", intent.id));
    if existing(&intent.target)? {
        let metadata = fs::symlink_metadata(&intent.target)
            .map_err(|error| format!("Could not inspect install target: {error}"))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(
                "Install target already exists with different contents; preserved for inspection"
                    .to_owned(),
            );
        }
        if !marker_matches(&intent.target, &intent.token)? {
            return Err(
                "Install target already exists with different contents; preserved for inspection"
                    .to_owned(),
            );
        }
    } else {
        if existing(&temporary)? {
            if marker_matches(&temporary, &intent.token)? {
                regular_executable(&temporary, &intent.executable)?;
            } else {
                let metadata =
                    fs::symlink_metadata(&temporary).map_err(|error| error.to_string())?;
                if !metadata.is_dir()
                    || metadata.file_type().is_symlink()
                    || !marker_matches_with(&temporary, COPY_TOKEN, &intent.token)?
                {
                    return Err(
                        "Install temporary path has conflicting contents; preserved for inspection"
                            .to_owned(),
                    );
                }
                fs::remove_dir_all(&temporary)
                    .map_err(|error| format!("Could not clear interrupted copy: {error}"))?;
            }
        }
        if !existing(&temporary)? {
            regular_executable(&intent.stage, &intent.executable)?;
            fs::create_dir(&temporary).map_err(|error| {
                format!("Could not create install temporary directory: {error}")
            })?;
            write_token(&temporary.join(COPY_TOKEN), &intent.token)?;
            sync_directory(&temporary)?;
            copy_tree(&intent.stage, &temporary)?;
            regular_executable(&temporary, &intent.executable)?;
            write_token(&temporary.join(MARKER), &intent.token)?;
            sync_directory(&temporary)?;
        }
        if existing(&intent.target)? {
            return Err("Install target already exists; preserved for inspection".to_owned());
        }
        publish_noreplace(&temporary, &intent.target)
            .map_err(|error| format!("Could not publish install: {error}"))?;
    }
    regular_executable(&intent.target, &intent.executable)?;
    let executable = intent.target.join(&intent.executable);
    database.with_connection(|connection| {
        let transaction = connection.unchecked_transaction().map_err(database_error)?;
        transaction.execute(
            "INSERT INTO games (id, steam_app_id, automatic_name, executable_path) VALUES (?1, ?2, ?3, ?4)",
            params![intent.id, intent.app_id, intent.name, executable.to_string_lossy()]
        ).map_err(database_error)?;
        let changed = transaction.execute(
            "UPDATE downloads SET status = 'installed', error = NULL, updated_at = ?2 WHERE id = ?1 AND status = 'finalizing'",
            params![intent.id, timestamp()?]
        ).map_err(database_error)?;
        if changed != 1 { return Err("Finalization intent changed during commit".to_owned()); }
        transaction.commit().map_err(database_error)
    })?;
    if let Err(error) = cleanup_stage(database, &intent.id, &intent.stage) {
        save_cleanup_error(database, &intent.id, &error)
            .map_err(|persist| format!("{error}; could not save cleanup failure: {persist}"))?;
        return Err(error);
    }
    Ok(())
}

fn save_cleanup_error(database: &Database, id: &str, error: &str) -> Result<(), String> {
    database.with_connection(|connection| {
        connection.execute("UPDATE downloads SET error = ?2, updated_at = ?3 WHERE id = ?1 AND status = 'installed'", params![id, error, timestamp()?]).map_err(database_error)?;
        Ok(())
    })
}

fn cleanup_stage(database: &Database, id: &str, stage: &Path) -> Result<(), String> {
    match fs::symlink_metadata(stage) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(stage)
                .map_err(|error| format!("Could not remove staged files: {error}"))?;
        }
        Ok(_) => {
            return Err(
                "Staged path is no longer a regular directory; preserved for inspection".to_owned(),
            );
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not inspect staged files: {error}")),
    }
    database.with_connection(|connection| {
        connection.execute("UPDATE downloads SET staged_path = NULL, error = NULL, updated_at = ?2 WHERE id = ?1 AND status = 'installed'", params![id, timestamp()?]).map_err(database_error)?;
        Ok(())
    })
}

fn record_error(database: &Database, id: &str, error: &str) -> Result<(), String> {
    database.with_connection(|connection| {
        connection.execute("UPDATE downloads SET error = ?2, updated_at = ?3 WHERE id = ?1 AND status = 'finalizing'", params![id, error, timestamp()?]).map_err(database_error)?;
        Ok(())
    })
}

fn finalize(
    database: &Database,
    data_dir: &Path,
    id: &str,
    executable: &str,
) -> Result<(), String> {
    let intent = claim(database, data_dir, id, executable)?;
    let result = finish(database, &intent);
    match result {
        Ok(()) => Ok(()),
        Err(error) => match record_error(database, &intent.id, &error) {
            Ok(()) => Err(error),
            Err(persist) => Err(format!("{error}; could not save failure: {persist}")),
        },
    }
}

pub(crate) fn recover(database: &Database, data_dir: &Path) -> Result<(), String> {
    let ids: Vec<String> = database.with_connection(|connection| {
        let mut statement = connection
            .prepare("SELECT id FROM downloads WHERE status = 'finalizing'")
            .map_err(database_error)?;
        statement
            .query_map([], |row| row.get(0))
            .map_err(database_error)?
            .collect::<Result<_, _>>()
            .map_err(database_error)
    })?;
    for id in ids {
        let result =
            load_intent(database, data_dir, &id).and_then(|intent| finish(database, &intent));
        if let Err(error) = result {
            match record_error(database, &id, &error) {
                Ok(()) => eprintln!("Could not recover installation {id}: {error}"),
                Err(persist) => eprintln!(
                    "Could not recover installation {id}: {error}; could not save failure: {persist}"
                ),
            }
        }
    }
    recover_cleanup(database, data_dir)?;
    Ok(())
}

fn recover_cleanup(database: &Database, data_dir: &Path) -> Result<(), String> {
    let ids: Vec<String> = database.with_connection(|connection| {
        let mut statement = connection
            .prepare(
                "SELECT id FROM downloads WHERE status = 'installed' AND staged_path IS NOT NULL",
            )
            .map_err(database_error)?;
        statement
            .query_map([], |row| row.get(0))
            .map_err(database_error)?
            .collect::<Result<_, _>>()
            .map_err(database_error)
    })?;
    for id in ids {
        let id = Uuid::parse_str(&id)
            .map_err(|_| "Stored download ID is invalid".to_owned())?
            .to_string();
        let stage = data_dir.join("downloads").join(format!("{id}.stage"));
        if let Err(error) = cleanup_stage(database, &id, &stage) {
            save_cleanup_error(database, &id, &error)?;
            eprintln!("Could not clean staged files for installed game {id}: {error}");
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn finalize_download(
    app: AppHandle,
    id: String,
    executable_relative: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("Could not locate app data: {error}"))?;
        finalize(
            app.state::<DatabaseState>().database()?,
            &data_dir,
            &id,
            &executable_relative,
        )
    })
    .await
    .map_err(|error| format!("Finalization task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Database, PathBuf, String) {
        let data_dir = std::env::temp_dir().join(format!("legio-finalize-test-{}", Uuid::new_v4()));
        let database = Database::open(&data_dir).unwrap();
        let id = Uuid::new_v4().to_string();
        let stage = data_dir.join("downloads").join(format!("{id}.stage"));
        fs::create_dir_all(stage.join("bin")).unwrap();
        fs::write(stage.join("bin/game.exe"), b"game").unwrap();
        database.with_connection(|connection| {
            connection.execute(
                "INSERT INTO downloads (id, steam_app_id, name, release_version, url, sha256, size_bytes, downloaded_bytes, status, created_at, updated_at, staged_path)
                 VALUES (?1, 42, 'Test Game', '1.0', 'https://example.test/game.zip', 'hash', 4, 4, 'staged', 1, 1, ?2)",
                params![id, stage.to_string_lossy()],
            ).map_err(database_error)?;
            Ok(())
        }).unwrap();
        (database, data_dir, id)
    }

    fn state(database: &Database, id: &str) -> (String, Option<String>) {
        database
            .with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT status, error FROM downloads WHERE id = ?1",
                        [id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(database_error)
            })
            .unwrap()
    }

    #[test]
    fn finalizes_and_commits_game_and_download() {
        let (database, data_dir, id) = fixture();
        finalize(&database, &data_dir, &id, "bin/game.exe").unwrap();
        let target = data_dir.join("installed").join(&id);
        assert_eq!(fs::read(target.join("bin/game.exe")).unwrap(), b"game");
        assert_eq!(state(&database, &id).0, "installed");
        assert!(
            !data_dir
                .join("downloads")
                .join(format!("{id}.stage"))
                .exists()
        );
        let games = database.games().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].steam_app_id, Some(42));
        assert_eq!(
            games[0].executable_path.as_deref(),
            Some(target.join("bin/game.exe").to_str().unwrap())
        );
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn refuses_existing_target_and_keeps_evidence() {
        let (database, data_dir, id) = fixture();
        let target = data_dir.join("installed").join(&id);
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("foreign"), b"keep").unwrap();
        let error = finalize(&database, &data_dir, &id, "bin/game.exe").unwrap_err();
        assert!(error.contains("already exists"));
        assert_eq!(state(&database, &id).0, "finalizing");
        assert_eq!(fs::read(target.join("foreign")).unwrap(), b"keep");
        assert!(
            data_dir
                .join("downloads")
                .join(format!("{id}.stage"))
                .exists()
        );
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn recovers_after_database_commit_fails() {
        let (database, data_dir, id) = fixture();
        database.with_connection(|connection| {
            connection.execute_batch("CREATE TRIGGER deny_install BEFORE UPDATE OF status ON downloads WHEN NEW.status = 'installed' BEGIN SELECT RAISE(FAIL, 'blocked'); END;")
                .map_err(database_error)
        }).unwrap();
        assert!(finalize(&database, &data_dir, &id, "bin/game.exe").is_err());
        assert_eq!(state(&database, &id).0, "finalizing");
        assert!(database.games().unwrap().is_empty());
        assert!(data_dir.join("installed").join(&id).is_dir());
        assert!(
            data_dir
                .join("downloads")
                .join(format!("{id}.stage"))
                .is_dir()
        );
        database
            .with_connection(|connection| {
                connection
                    .execute_batch("DROP TRIGGER deny_install;")
                    .map_err(database_error)
            })
            .unwrap();
        recover(&database, &data_dir).unwrap();
        recover(&database, &data_dir).unwrap();
        assert_eq!(state(&database, &id).0, "installed");
        assert_eq!(database.games().unwrap().len(), 1);
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn rejects_unsafe_executable_paths() {
        let (database, data_dir, id) = fixture();
        for path in ["/tmp/game.exe", "../game.exe", "bin/../game.exe"] {
            assert!(finalize(&database, &data_dir, &id, path).is_err());
            assert_eq!(state(&database, &id).0, "staged");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                "game.exe",
                data_dir
                    .join("downloads")
                    .join(format!("{id}.stage/bin/link.exe")),
            )
            .unwrap();
            assert!(finalize(&database, &data_dir, &id, "bin/link.exe").is_err());
        }
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn recovers_token_scoped_interrupted_copy() {
        let (database, data_dir, id) = fixture();
        let intent = claim(&database, &data_dir, &id, "bin/game.exe").unwrap();
        let temporary = intent.target.with_file_name(format!(".{id}.copying"));
        fs::create_dir(&temporary).unwrap();
        fs::write(temporary.join(COPY_TOKEN), &intent.token).unwrap();
        fs::write(temporary.join("partial"), b"incomplete").unwrap();
        recover(&database, &data_dir).unwrap();
        assert_eq!(state(&database, &id).0, "installed");
        assert!(!intent.target.join("partial").exists());
        assert_eq!(
            fs::read(intent.target.join("bin/game.exe")).unwrap(),
            b"game"
        );
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn preserves_foreign_temporary_directory() {
        let (database, data_dir, id) = fixture();
        let intent = claim(&database, &data_dir, &id, "bin/game.exe").unwrap();
        let temporary = intent.target.with_file_name(format!(".{id}.copying"));
        fs::create_dir(&temporary).unwrap();
        fs::write(temporary.join("foreign"), b"keep").unwrap();
        recover(&database, &data_dir).unwrap();
        assert_eq!(state(&database, &id).0, "finalizing");
        assert_eq!(fs::read(temporary.join("foreign")).unwrap(), b"keep");
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_stage_root() {
        use std::os::unix::fs::symlink;
        let (database, data_dir, id) = fixture();
        let stage = data_dir.join("downloads").join(format!("{id}.stage"));
        let moved = data_dir.join("moved-stage");
        fs::rename(&stage, &moved).unwrap();
        symlink(&moved, &stage).unwrap();
        assert!(finalize(&database, &data_dir, &id, "bin/game.exe").is_err());
        assert_eq!(state(&database, &id).0, "staged");
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn retries_installed_stage_cleanup_after_conflict() {
        use std::os::unix::fs::symlink;
        let (database, data_dir, id) = fixture();
        let stage = data_dir.join("downloads").join(format!("{id}.stage"));
        let intent = claim(&database, &data_dir, &id, "bin/game.exe").unwrap();
        let moved = data_dir.join("held-stage");
        fs::rename(&stage, &moved).unwrap();
        symlink(&moved, &stage).unwrap();
        fs::create_dir_all(intent.target.join("bin")).unwrap();
        fs::write(intent.target.join("bin/game.exe"), b"game").unwrap();
        fs::write(intent.target.join(MARKER), &intent.token).unwrap();
        assert!(finish(&database, &intent).is_err());
        assert_eq!(state(&database, &id).0, "installed");
        assert!(state(&database, &id).1.unwrap().contains("Staged path"));
        fs::remove_file(&stage).unwrap();
        fs::rename(&moved, &stage).unwrap();
        recover(&database, &data_dir).unwrap();
        assert_eq!(state(&database, &id), ("installed".to_owned(), None));
        assert!(!stage.exists());
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn copies_between_filesystems_when_available() {
        use std::os::unix::fs::MetadataExt;
        let source = Path::new("/dev/shm");
        let destination_root = std::env::temp_dir();
        if !source.is_dir()
            || fs::metadata(source).unwrap().dev() == fs::metadata(&destination_root).unwrap().dev()
        {
            return;
        }
        let source = source.join(format!("legio-copy-{}", Uuid::new_v4()));
        let destination = destination_root.join(format!("legio-copy-{}", Uuid::new_v4()));
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("game.exe"), b"cross filesystem").unwrap();
        copy_tree(&source, &destination).unwrap();
        assert_eq!(
            fs::read(destination.join("game.exe")).unwrap(),
            b"cross filesystem"
        );
        fs::remove_dir_all(source).unwrap();
        fs::remove_dir_all(destination).unwrap();
    }
}
