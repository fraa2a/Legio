use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{OptionalExtension, params};
use tauri::AppHandle;
use uuid::Uuid;

use crate::{
    archive_install::safe_relative_path,
    database::{Database, database_error},
    manual_import,
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

pub(crate) fn install_root(database: &Database, data_dir: &Path) -> Result<PathBuf, String> {
    let root = if database.settings()?.download_path.is_some() {
        data_dir.to_path_buf()
    } else {
        data_dir.join("installed")
    };
    fs::create_dir_all(&root)
        .map_err(|error| format!("Could not create install directory: {error}"))?;
    let metadata = fs::symlink_metadata(&root)
        .map_err(|error| format!("Could not inspect install directory: {error}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Install directory is not a regular directory".to_owned());
    }
    Ok(root)
}

fn regular_directory(path: &Path, label: &str) -> Result<(), String> {
    if !directory_exists_regular(path, label)? {
        return Err(format!("Could not inspect {label}: path does not exist"));
    }
    Ok(())
}

fn directory_exists_regular(path: &Path, label: &str) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("Could not inspect {label}: {error}")),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!("{label} is not a regular directory"));
    }
    Ok(true)
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
    regular_directory(data_dir, "application data directory")?;
    let id = Uuid::parse_str(id)
        .map_err(|_| "Download ID is invalid".to_owned())?
        .to_string();
    let executable = safe_relative_path(executable)?;
    if executable == Path::new(MARKER) {
        return Err("Selected executable is reserved".to_owned());
    }
    let stage = data_dir.join("downloads").join(format!("{id}.stage"));
    regular_directory(&data_dir.join("downloads"), "download directory")?;
    regular_executable(&stage, &executable)?;
    let root = install_root(database, data_dir)?;
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
        let target = crate::game_library::available_target(connection, &root, &name)?;
        connection.execute(
            "UPDATE downloads SET status = 'finalizing', final_path = ?2, executable_relative = ?3, install_token = ?4, error = NULL, updated_at = ?5 WHERE id = ?1 AND status = 'staged'",
            params![id, target.to_string_lossy(), executable.to_string_lossy(), token, timestamp()?]
        ).map_err(database_error)?;
        Ok(Intent { id, app_id, name, stage, target, executable, token })
    })
}

fn load_intent(database: &Database, data_dir: &Path, id: &str) -> Result<Intent, String> {
    regular_directory(data_dir, "application data directory")?;
    regular_directory(&data_dir.join("downloads"), "download directory")?;
    let id = Uuid::parse_str(id)
        .map_err(|_| "Download ID is invalid".to_owned())?
        .to_string();
    let install_root = install_root(database, data_dir)?;
    database.with_connection(|connection| {
        let row: Option<StoredIntent> = connection.query_row(
            "SELECT steam_app_id, name, staged_path, final_path, executable_relative, install_token FROM downloads WHERE id = ?1 AND status = 'finalizing'",
            [&id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        ).optional().map_err(database_error)?;
        let Some((app_id, name, stage, target, executable, token)) = row else { return Err("Finalization intent was not found".to_owned()); };
        let expected_stage = data_dir.join("downloads").join(format!("{id}.stage"));
        let expected_target = install_root.join(&id);
        let legacy_target = data_dir.join("installed").join(&id);
        if stage.as_deref() != Some(expected_stage.to_string_lossy().as_ref()) {
            return Err("Finalization paths do not match the app-owned paths".to_owned());
        }
        let target = match target.as_deref() {
            Some(path) if path == expected_target.to_string_lossy() => expected_target,
            Some(path) if Path::new(path).parent() == Some(install_root.as_path())
                && Path::new(path).file_name().is_some_and(|name| !name.to_string_lossy().starts_with('.')) => PathBuf::from(path),
            Some(path) if install_root == data_dir && path == legacy_target.to_string_lossy() => {
                regular_directory(&data_dir.join("installed"), "legacy install directory")?;
                legacy_target
            }
            _ => return Err("Finalization paths do not match the app-owned paths".to_owned()),
        };
        let executable = safe_relative_path(executable.as_deref().ok_or("Finalization executable is missing")?)?;
        let token = token.ok_or("Finalization token is missing")?;
        Uuid::parse_str(&token).map_err(|_| "Finalization token is invalid".to_owned())?;
        Ok(Intent { id, app_id, name, stage: expected_stage, target, executable, token })
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
            crate::filesystem_copy::copy(&mut input, &mut output)
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
pub(crate) fn sync_directory(path: &Path) -> Result<(), String> {
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("Could not sync install directory: {error}"))
}

#[cfg(windows)]
pub(crate) fn sync_directory(_path: &Path) -> Result<(), String> {
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
pub(crate) fn publish_noreplace(source: &Path, target: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid source path"))?;
    let target = CString::new(target.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid target path"))?;
    // SAFETY: the C strings are NUL-terminated and remain alive for this syscall.
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
        Some(libc::ENOSYS) | Some(libc::EINVAL) | Some(libc::EOPNOTSUPP)
    ) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Atomic no-replace directory rename is unavailable on this kernel or filesystem",
        ));
    }
    Err(error)
}

#[cfg(windows)]
pub(crate) fn publish_noreplace(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: the UTF-16 buffers are terminated and remain alive for this call.
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileW(source.as_ptr(), target.as_ptr())
    };
    if result != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn install_layout(intent: &Intent) -> Result<(PathBuf, PathBuf), String> {
    if intent
        .target
        .file_name()
        .is_some_and(|name| name == intent.id.as_str())
    {
        return Ok((intent.stage.clone(), intent.executable.clone()));
    }
    let full = intent.stage.join(&intent.executable);
    let root = crate::game_library::game_root(&full)?;
    if !root.starts_with(&intent.stage) {
        return Err("Detected game directory escapes the archive".into());
    }
    let executable = full
        .strip_prefix(&root)
        .map_err(|error| error.to_string())?
        .to_path_buf();
    Ok((root, executable))
}

fn finish(database: &Database, intent: &Intent) -> Result<(), String> {
    let (source, relative) = install_layout(intent)?;
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
                regular_executable(&temporary, &relative)?;
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
            regular_executable(&source, &relative)?;
            fs::create_dir(&temporary).map_err(|error| {
                format!("Could not create install temporary directory: {error}")
            })?;
            write_token(&temporary.join(COPY_TOKEN), &intent.token)?;
            sync_directory(&temporary)?;
            copy_tree(&source, &temporary)?;
            regular_executable(&temporary, &relative)?;
            write_token(&temporary.join(MARKER), &intent.token)?;
            sync_directory(&temporary)?;
        }
        if existing(&intent.target)? {
            return Err("Install target already exists; preserved for inspection".to_owned());
        }
        publish_noreplace(&temporary, &intent.target)
            .map_err(|error| format!("Could not publish install: {error}"))?;
        sync_directory(
            intent
                .target
                .parent()
                .ok_or("Install target has no parent")?,
        )?;
    }
    regular_executable(&intent.target, &relative)?;
    let executable = intent.target.join(&relative);
    database.with_connection(|connection| {
        let transaction = connection.unchecked_transaction().map_err(database_error)?;
        transaction.execute(
            "INSERT INTO games (id, steam_app_id, automatic_name, executable_path, installation_root, archive_layout) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![intent.id, intent.app_id, intent.name, executable.to_string_lossy(), intent.target.to_string_lossy(), if intent.target.file_name().is_some_and(|name|name == intent.id.as_str()) { "legacy" } else { "direct" }]
        ).map_err(database_error)?;
        crate::game_updates::record_install_on(&transaction, &intent.id, &intent.id)?;
        let changed = transaction.execute(
            "UPDATE downloads SET status = 'installed', error = NULL, updated_at = ?2 WHERE id = ?1 AND status = 'finalizing'",
            params![intent.id, timestamp()?]
        ).map_err(database_error)?;
        if changed != 1 { return Err("Finalization intent changed during commit".to_owned()); }
        transaction.commit().map_err(database_error)
    })?;
    crate::game_library::publish(
        database,
        intent
            .target
            .parent()
            .ok_or("Install has no library directory")?,
    );
    remove_install_markers(&intent.target, &intent.token)?;
    if let Err(error) = cleanup_stage(database, &intent.id, &intent.stage) {
        save_cleanup_error(database, &intent.id, &error)
            .map_err(|persist| format!("{error}; could not save cleanup failure: {persist}"))?;
        return Err(error);
    }
    Ok(())
}

pub(crate) fn remove_install_markers(target: &Path, token: &str) -> Result<(), String> {
    for name in [MARKER, COPY_TOKEN] {
        if marker_matches_with(target, name, token)? {
            fs::remove_file(target.join(name))
                .map_err(|error| format!("Could not remove completed install metadata: {error}"))?;
        }
    }
    sync_directory(target)
}

fn save_cleanup_error(database: &Database, id: &str, error: &str) -> Result<(), String> {
    database.with_connection(|connection| {
        connection.execute("UPDATE downloads SET error = ?2, updated_at = ?3 WHERE id = ?1 AND status = 'installed'", params![id, error, timestamp()?]).map_err(database_error)?;
        Ok(())
    })
}

fn cleanup_stage(database: &Database, id: &str, stage: &Path) -> Result<(), String> {
    let download_root = stage.parent().ok_or("Staged path has no parent")?;
    let data_dir = download_root
        .parent()
        .ok_or("Download path has no application data parent")?;
    if !directory_exists_regular(data_dir, "application data directory")?
        || !directory_exists_regular(download_root, "download directory")?
    {
        return clear_staged_path(database, id);
    }
    remove_stage_directory(stage)?;
    clear_staged_path(database, id)
}

/// Removes an app-owned staging directory. A path that is not a regular
/// directory is never deleted so unexpected content stays available.
pub(crate) fn remove_stage_directory(stage: &Path) -> Result<(), String> {
    match fs::symlink_metadata(stage) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(stage)
                .map_err(|error| format!("Could not remove staged files: {error}"))
        }
        Ok(_) => {
            Err("Staged path is no longer a regular directory; preserved for inspection".to_owned())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not inspect staged files: {error}")),
    }
}

fn clear_staged_path(database: &Database, id: &str) -> Result<(), String> {
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
    let retrying = database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT status = 'finalizing' AND error IS NOT NULL FROM downloads WHERE id = ?1",
                [id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(database_error)
    })?;
    let intent = if retrying {
        let intent = load_intent(database, data_dir, id)?;
        if intent.executable != safe_relative_path(executable)? {
            return Err(
                "Finalization retry must use the previously selected executable".to_owned(),
            );
        }
        database.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE downloads SET error = NULL WHERE id = ?1 AND status = 'finalizing'",
                    [id],
                )
                .map_err(database_error)?;
            Ok(())
        })?;
        intent
    } else {
        claim(database, data_dir, id, executable)?
    };
    let result = finish(database, &intent);
    match result {
        Ok(()) => Ok(()),
        Err(error) => match record_error(database, &intent.id, &error) {
            Ok(()) => Err(error),
            Err(persist) => Err(format!("{error}; could not save failure: {persist}")),
        },
    }
}

pub(crate) fn retry(database: &Database, data_dir: &Path, id: &str) -> Result<(), String> {
    let intent = load_intent(database, data_dir, id)?;
    finalize(database, data_dir, id, &intent.executable.to_string_lossy())
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
                Ok(()) => crate::application_log::failure("recover_installation", error),
                Err(persist) => crate::application_log::failure(
                    "recover_installation",
                    format!("{error}; could not save failure: {persist}"),
                ),
            }
        }
    }
    recover_cleanup(database, data_dir)?;
    Ok(())
}

fn staged_download_directory(
    database: &Database,
    data_dir: &Path,
    id: &str,
) -> Result<(PathBuf, String), String> {
    regular_directory(data_dir, "application data directory")?;
    regular_directory(&data_dir.join("downloads"), "download directory")?;
    let id = Uuid::parse_str(id)
        .map_err(|_| "Download ID is invalid".to_owned())?
        .to_string();
    let row: Option<(String, Option<String>, String)> = database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT name, staged_path, status FROM downloads WHERE id = ?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(database_error)
    })?;
    let Some((name, staged_path, status)) = row else {
        return Err("Download was not found".to_owned());
    };
    if status != "staged" {
        return Err(format!("Cannot scan a {status} download"));
    }
    let expected_stage = data_dir.join("downloads").join(format!("{id}.stage"));
    if staged_path.as_deref() != Some(expected_stage.to_string_lossy().as_ref()) {
        return Err("Stored stage path does not match the app-owned path".to_owned());
    }
    regular_directory(&expected_stage, "staged game directory")?;
    Ok((expected_stage, name))
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
        let result = (|| -> Result<(), String> {
            let stage = data_dir.join("downloads").join(format!("{id}.stage"));
            let update_target = crate::game_updates::target(database, &id)?;
            let game_id = update_target.clone().unwrap_or_else(|| id.clone());
            let game = database.game(&game_id)?;
            if let Some(target) = game.installation_root.as_deref().map(Path::new) {
                if update_target.is_some() {
                    crate::game_library::flush_registered(database);
                } else {
                    crate::game_library::publish(
                        database,
                        target.parent().ok_or("Install has no library directory")?,
                    );
                }
                let token: Option<String> = database.with_connection(|connection| {
                    connection
                        .query_row(
                            "SELECT install_token FROM downloads WHERE id = ?1",
                            [&id],
                            |row| row.get(0),
                        )
                        .map_err(database_error)
                })?;
                if let Some(token) = token {
                    remove_install_markers(target, &token)?;
                }
            }
            if let Err(error) = cleanup_stage(database, &id, &stage) {
                save_cleanup_error(database, &id, &error)?;
                crate::application_log::failure("clean_staged_files", error);
            }
            Ok(())
        })();
        if let Err(error) = result {
            save_cleanup_error(database, &id, &error)?;
            crate::application_log::failure("recover_install_cleanup", error);
        }
    }
    Ok(())
}

pub(crate) fn create_install_shortcut<R: tauri::Runtime>(
    app: &AppHandle<R>,
    database: &Database,
    id: &str,
) -> Result<(), String> {
    let game = database.game(id)?;
    let shortcut_warning = crate::desktop_shortcuts::create_application_menu(app, &game)
        .unwrap_or_else(|error| {
            Some(format!(
                "Could not create application-menu shortcut: {error}"
            ))
        });
    if let Some(warning) = shortcut_warning {
        database.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE downloads SET error = ?2 WHERE id = ?1",
                    [id, &warning],
                )
                .map_err(database_error)?;
            Ok(())
        })?;
    }
    Ok(())
}

pub(crate) fn automatic(database: &Database, data_dir: &Path, id: &str) -> Result<String, String> {
    let (stage, name) = staged_download_directory(database, data_dir, id)?;
    let target = crate::game_updates::target(database, id)?;
    let scan = manual_import::scan_staged_directory(&stage, Some(&name))?;
    let old_relative = if let Some(game_id) = &target {
        let game = database.game(game_id)?;
        game.executable_path
            .as_deref()
            .zip(game.installation_root.as_deref())
            .and_then(|(executable, root)| Path::new(executable).strip_prefix(root).ok())
            .and_then(|path| manual_import::portable_relative_path(path).ok())
    } else {
        None
    };
    let executable = old_relative
        .as_ref()
        .filter(|path| stage.join(path).is_file())
        .cloned()
        .or(scan.selected_relative_path)
        .or_else(|| {
            scan.candidates
                .first()
                .map(|candidate| candidate.relative_path.clone())
        })
        .or(old_relative)
        .ok_or("Archive contains no usable game executable")?;
    if let Some(game_id) = target {
        crate::overlay_update::apply(database, id, &stage, &executable)?;
        Ok(game_id)
    } else {
        finalize(database, data_dir, id, &executable)?;
        Ok(id.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Database, PathBuf, String) {
        fixture_with_custom_path(false)
    }

    fn fixture_with_custom_path(custom_path: bool) -> (Database, PathBuf, String) {
        let data_dir = std::env::temp_dir().join(format!("legio-finalize-test-{}", Uuid::new_v4()));
        let database = Database::open(&data_dir).unwrap();
        let data_dir = dunce::canonicalize(data_dir).unwrap();
        if custom_path {
            let mut settings = database.settings().unwrap();
            settings.download_path = Some(data_dir.to_string_lossy().into_owned());
            database.save_settings(settings).unwrap();
        }
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
    fn readable_install_unwraps_game_and_records_library() {
        let (database, data_dir, id) = fixture();
        let stage = data_dir.join("downloads").join(format!("{id}.stage"));
        fs::create_dir_all(stage.join("Wrapper/Game")).unwrap();
        fs::write(stage.join("Wrapper/Game/Game.exe"), b"play").unwrap();
        finalize(&database, &data_dir, &id, "Wrapper/Game/Game.exe").unwrap();
        let game = database.game(&id).unwrap();
        let root = data_dir.join("installed/Test Game");
        assert_eq!(fs::read(root.join("Game.exe")).unwrap(), b"play");
        assert_eq!(state(&database, &id).0, "installed");
        assert!(!stage.exists());
        assert_eq!(database.games().unwrap().len(), 1);
        assert_eq!(game.steam_app_id, Some(42));
        assert_eq!(
            Path::new(game.executable_path.as_ref().unwrap()),
            root.join("Game.exe")
        );
        assert!(!root.join(MARKER).exists());
        assert!(!root.join(COPY_TOKEN).exists());
        let manifest = fs::read_to_string(root.parent().unwrap().join("library.ldf")).unwrap();
        assert!(manifest.contains(&id));
        assert!(manifest.contains("Game.exe"));
        let manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap();
        assert_eq!(manifest["format"].as_i64(), Some(1));
        assert_eq!(
            manifest["games"][0]["installation"]["folder"].as_str(),
            Some("Test Game")
        );
        assert_eq!(
            manifest["games"][0]["installation"]["executable"].as_str(),
            Some("Game.exe")
        );
        fs::write(
            root.join(MARKER),
            database
                .with_connection(|connection| {
                    connection
                        .query_row(
                            "SELECT install_token FROM downloads WHERE id = ?1",
                            [&id],
                            |row| row.get::<_, String>(0),
                        )
                        .map_err(database_error)
                })
                .unwrap(),
        )
        .unwrap();
        crate::game_library::adopt_existing(&database).unwrap();
        assert!(!root.join(MARKER).exists());
        database.remove_game(&id).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(root.parent().unwrap().join("library.ldf")).unwrap(),
        )
        .unwrap();
        assert!(manifest["games"].as_array().unwrap().is_empty());
        drop(database);
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn unreal_install_keeps_outer_game_directory_and_engine_files() {
        let (database, data_dir, id) = fixture();
        let stage = data_dir.join("downloads").join(format!("{id}.stage"));
        let executable = "Wrapper/Outer/Inner/Binaries/Win64/Anything-Win64-Shipping.exe";
        fs::create_dir_all(stage.join("Wrapper/Outer/Inner/Binaries/Win64")).unwrap();
        fs::create_dir_all(stage.join("Wrapper/Outer/Engine")).unwrap();
        fs::write(stage.join(executable), b"unreal").unwrap();
        fs::write(stage.join("Wrapper/Outer/Engine/runtime.dat"), b"engine").unwrap();
        finalize(&database, &data_dir, &id, executable).unwrap();
        let target = data_dir.join("installed/Test Game");
        assert_eq!(
            fs::read(target.join("Engine/runtime.dat")).unwrap(),
            b"engine"
        );
        assert_eq!(
            fs::read(target.join("Inner/Binaries/Win64/Anything-Win64-Shipping.exe")).unwrap(),
            b"unreal"
        );
        assert!(!target.join("Wrapper").exists());
        drop(database);
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn staged_scan_is_limited_to_a_matching_staged_download() {
        let (database, data_dir, id) = fixture();
        let (stage, name) = staged_download_directory(&database, &data_dir, &id).unwrap();
        let scan = manual_import::scan_staged_directory(&stage, Some(&name)).unwrap();
        assert_eq!(scan.selected_relative_path.as_deref(), Some("bin/game.exe"));

        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET status = 'downloaded' WHERE id = ?1",
                        [&id],
                    )
                    .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        assert!(staged_download_directory(&database, &data_dir, &id).is_err());
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET status = 'staged', staged_path = ?2 WHERE id = ?1",
                        params![id, data_dir.to_string_lossy()],
                    )
                    .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        assert!(staged_download_directory(&database, &data_dir, &id).is_err());
        drop(database);
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn automatically_selects_and_installs_the_executable() {
        let (database, data_dir, id) = fixture();
        assert_eq!(automatic(&database, &data_dir, &id).unwrap(), id);
        assert_eq!(state(&database, &id).0, "installed");
        let game = database.game(&id).unwrap();
        assert!(Path::new(game.executable_path.as_deref().unwrap()).is_file());
        drop(database);
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn custom_folder_is_the_install_root() {
        let (database, data_dir, id) = fixture_with_custom_path(true);
        finalize(&database, &data_dir, &id, "bin/game.exe").unwrap();
        let target = data_dir.join("Test Game");
        assert_eq!(fs::read(target.join("game.exe")).unwrap(), b"game");
        assert!(!data_dir.join("installed").exists());
        assert_eq!(
            database.games().unwrap()[0].installation_root.as_deref(),
            target.to_str()
        );
        drop(database);
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn recovers_an_install_started_in_the_old_custom_subfolder() {
        let (database, data_dir, id) = fixture_with_custom_path(true);
        let legacy_root = data_dir.join("installed");
        fs::create_dir(&legacy_root).unwrap();
        claim(&database, &data_dir, &id, "bin/game.exe").unwrap();
        let legacy_target = legacy_root.join(&id);
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET final_path = ?2 WHERE id = ?1",
                        params![id, legacy_target.to_string_lossy()],
                    )
                    .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        recover(&database, &data_dir).unwrap();
        assert_eq!(
            fs::read(legacy_target.join("bin/game.exe")).unwrap(),
            b"game"
        );
        assert_eq!(state(&database, &id).0, "installed");
        drop(database);
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn failed_finalization_retries_without_restart() {
        let (database, data_dir, id) = fixture();
        let intent = claim(&database, &data_dir, &id, "bin/game.exe").unwrap();
        let target = intent.target.clone();
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("foreign"), b"keep").unwrap();
        let error = finish(&database, &intent).unwrap_err();
        assert!(error.contains("already exists"));
        assert_eq!(state(&database, &id).0, "finalizing");
        assert!(intent.stage.is_dir());
        record_error(&database, &id, "conflict").unwrap();
        assert_eq!(fs::read(target.join("foreign")).unwrap(), b"keep");
        fs::remove_dir_all(&target).unwrap();
        finalize(&database, &data_dir, &id, "bin/game.exe").unwrap();
        assert_eq!(state(&database, &id).0, "installed");
        assert_eq!(fs::read(target.join("game.exe")).unwrap(), b"game");
        drop(database);
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn recovers_after_database_commit_fails_and_database_reopens() {
        let (database, data_dir, id) = fixture();
        database.with_connection(|connection| {
            connection.execute_batch("CREATE TRIGGER deny_install BEFORE UPDATE OF status ON downloads WHEN NEW.status = 'installed' BEGIN SELECT RAISE(FAIL, 'blocked'); END;")
                .map_err(database_error)
        }).unwrap();
        assert!(finalize(&database, &data_dir, &id, "bin/game.exe").is_err());
        assert_eq!(state(&database, &id).0, "finalizing");
        assert!(database.games().unwrap().is_empty());
        assert!(data_dir.join("installed/Test Game").is_dir());
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
        drop(database);
        let database = Database::open(&data_dir).unwrap();
        recover(&database, &data_dir).unwrap();
        recover(&database, &data_dir).unwrap();
        assert_eq!(state(&database, &id).0, "installed");
        assert_eq!(database.games().unwrap().len(), 1);
        assert_eq!(
            fs::read(data_dir.join("installed/Test Game/game.exe")).unwrap(),
            b"game"
        );
        assert!(
            !data_dir
                .join("downloads")
                .join(format!("{id}.stage"))
                .exists()
        );
        drop(database);
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
        drop(database);
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
        assert_eq!(fs::read(intent.target.join("game.exe")).unwrap(), b"game");
        drop(database);
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
        drop(database);
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
        drop(database);
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_application_roots() {
        use std::os::unix::fs::symlink;
        let (database, data_dir, id) = fixture();
        let downloads = data_dir.join("downloads");
        let moved_downloads = data_dir.join("moved-downloads");
        fs::rename(&downloads, &moved_downloads).unwrap();
        symlink(&moved_downloads, &downloads).unwrap();
        assert!(finalize(&database, &data_dir, &id, "bin/game.exe").is_err());
        assert_eq!(state(&database, &id).0, "staged");
        fs::remove_file(&downloads).unwrap();
        fs::rename(&moved_downloads, &downloads).unwrap();

        let install = data_dir.join("installed");
        fs::create_dir(&install).unwrap();
        let moved_install = data_dir.join("moved-installed");
        fs::rename(&install, &moved_install).unwrap();
        symlink(&moved_install, &install).unwrap();
        assert!(finalize(&database, &data_dir, &id, "bin/game.exe").is_err());
        assert_eq!(state(&database, &id).0, "staged");
        fs::remove_file(&install).unwrap();
        fs::rename(&moved_install, &install).unwrap();

        let moved_data = data_dir.with_extension("moved");
        fs::rename(&data_dir, &moved_data).unwrap();
        symlink(&moved_data, &data_dir).unwrap();
        assert!(finalize(&database, &data_dir, &id, "bin/game.exe").is_err());
        fs::remove_file(&data_dir).unwrap();
        fs::rename(&moved_data, &data_dir).unwrap();
        drop(database);
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
        fs::create_dir_all(intent.target.clone()).unwrap();
        fs::write(intent.target.join("game.exe"), b"game").unwrap();
        fs::write(intent.target.join(MARKER), &intent.token).unwrap();
        assert!(finish(&database, &intent).is_err());
        assert_eq!(state(&database, &id).0, "installed");
        assert!(state(&database, &id).1.unwrap().contains("Staged path"));
        fs::remove_file(&stage).unwrap();
        fs::rename(&moved, &stage).unwrap();
        recover(&database, &data_dir).unwrap();
        assert_eq!(state(&database, &id), ("installed".to_owned(), None));
        assert!(!stage.exists());
        drop(database);
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn clears_cleanup_intent_when_download_directory_is_already_gone() {
        let (database, data_dir, id) = fixture();
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE downloads SET status = 'installed' WHERE id = ?1",
                        [&id],
                    )
                    .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        let stage = data_dir.join("downloads").join(format!("{id}.stage"));
        fs::remove_dir_all(data_dir.join("downloads")).unwrap();
        cleanup_stage(&database, &id, &stage).unwrap();
        let staged_path: Option<String> = database
            .with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT staged_path FROM downloads WHERE id = ?1",
                        [&id],
                        |row| row.get(0),
                    )
                    .map_err(database_error)
            })
            .unwrap();
        assert_eq!(staged_path, None);
        drop(database);
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
