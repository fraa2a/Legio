use crate::database::{Database, database_error};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

const FILE: &str = "library.ldf";
static WRITE: Mutex<()> = Mutex::new(());

#[derive(Default, Serialize, Deserialize)]
struct Library {
    format: u32,
    #[serde(default)]
    games: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    id: String,
    name: String,
    folder: String,
    executable: String,
    steam_app_id: Option<u32>,
    source_id: Option<String>,
    entry_id: Option<String>,
    version: Option<String>,
}

pub(crate) fn game_root(executable: &Path) -> Result<PathBuf, String> {
    let parent = executable
        .parent()
        .ok_or("Executable has no parent directory")?;
    let named = |path: &Path, name: &str| {
        path.file_name()
            .is_some_and(|part| part.to_string_lossy().eq_ignore_ascii_case(name))
    };
    let shipping = executable.file_name().is_some_and(|name| {
        name.to_string_lossy()
            .to_ascii_lowercase()
            .ends_with("-win64-shipping.exe")
    });
    if shipping
        && named(parent, "Win64")
        && let Some(binaries) = parent.parent()
        && named(binaries, "Binaries")
        && let Some(root) = binaries.parent().and_then(Path::parent)
        && !root.as_os_str().is_empty()
    {
        return Ok(root.to_path_buf());
    }
    Ok(parent.to_path_buf())
}

pub(crate) fn folder_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(100)
        .collect();
    let name = name.trim().trim_matches(['.', ' ']);
    let reserved = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if name.is_empty() || name == "." || name == ".." {
        return "Game".into();
    }
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&reserved.as_str())
    {
        return format!("_{name}");
    }
    name.to_owned()
}

pub(crate) fn available_target(
    connection: &rusqlite::Connection,
    root: &Path,
    name: &str,
) -> Result<PathBuf, String> {
    let name = folder_name(name);
    let mut occupied: Vec<String> = fs::read_dir(root)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().to_lowercase()))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    occupied.push(FILE.into());
    {
        let mut query = connection
            .prepare("SELECT final_path FROM downloads WHERE status = 'finalizing' UNION ALL SELECT destination FROM game_transfers")
            .map_err(database_error)?;
        for item in query
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(database_error)?
        {
            let item = PathBuf::from(item.map_err(database_error)?);
            if item.parent() == Some(root)
                && let Some(name) = item.file_name()
            {
                occupied.push(name.to_string_lossy().to_lowercase());
            }
        }
    }
    for suffix in 1..=10000 {
        let candidate = if suffix == 1 {
            name.clone()
        } else {
            format!("{name} ({suffix})")
        };
        if !occupied.contains(&candidate.to_lowercase()) {
            return Ok(root.join(candidate));
        }
    }
    Err("Too many game directories have the same name".into())
}

fn read(root: &Path) -> Result<Library, String> {
    match fs::symlink_metadata(root.join(FILE)) {
        Ok(metadata)
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > 16 * 1024 * 1024 =>
        {
            return Err("library.ldf must be a regular file smaller than 16 MiB".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not inspect library.ldf: {error}")),
    }
    match fs::read_to_string(root.join(FILE)) {
        Ok(value) => {
            let library: Library =
                toml::from_str(&value).map_err(|error| format!("Invalid library.ldf: {error}"))?;
            if library.format != 1 {
                return Err("Unsupported library.ldf format".into());
            }
            Ok(library)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Library {
            format: 1,
            games: Vec::new(),
        }),
        Err(error) => Err(format!("Could not read library.ldf: {error}")),
    }
}

fn save(root: &Path, library: &Library) -> Result<(), String> {
    let content = toml::to_string_pretty(library).map_err(|error| error.to_string())?;
    let temporary = root.join(format!(".library-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        output
            .write_all(content.as_bytes())
            .and_then(|()| output.sync_all())
            .map_err(|e| e.to_string())?;
        drop(output);
        replace(&temporary, &root.join(FILE))
            .map_err(|e| format!("Could not save library.ldf: {e}"))?;
        crate::finalize_install::sync_directory(root)
    })();
    if result.is_err() && temporary.exists() {
        fs::remove_file(&temporary)
            .map_err(|e| format!("Could not clean library temporary file: {e}"))?;
    }
    result
}

#[cfg(not(windows))]
fn replace(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
fn replace(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both paths are NUL-terminated and live throughout the call.
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

pub(crate) fn sync(database: &Database, root: &Path) -> Result<(), String> {
    sync_entries(database, root, false)
}

pub(crate) fn sync_default<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    use tauri::Manager;
    let state = app.state::<crate::database::DatabaseState>();
    let database = state.database()?;
    let data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let storage = database.storage_root(&data)?;
    sync_storage(database, &storage)
}

pub(crate) fn sync_storage(database: &Database, storage: &Path) -> Result<(), String> {
    let root = crate::finalize_install::install_root(database, storage)?;
    sync_entries(database, &root, true)
}

pub(crate) fn remove_default<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    id: &str,
) -> Result<(), String> {
    use tauri::Manager;
    let state = app.state::<crate::database::DatabaseState>();
    let database = state.database()?;
    let data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let storage = database.storage_root(&data)?;
    let root = crate::finalize_install::install_root(database, &storage)?;
    remove(&root, id)?;
    sync_entries(database, &root, true)
}

fn sync_entries(database: &Database, root: &Path, include_external: bool) -> Result<(), String> {
    let _guard = WRITE.lock().map_err(|_| "Library file lock is poisoned")?;
    let mut library = read(root)?;
    let games = database.games()?;
    library
        .games
        .retain(|entry| !games.iter().any(|game| game.id == entry.id));
    for game in games {
        let Some(folder) = game.installation_root.as_deref().map(Path::new) else {
            continue;
        };
        if (!include_external && folder.parent() != Some(root)) || game.steam_install_path.is_some()
        {
            continue;
        }
        let Some(executable) = game
            .executable_path
            .as_deref()
            .map(Path::new)
            .and_then(|path| path.strip_prefix(folder).ok())
        else {
            continue;
        };
        let (source_id, entry_id, version) = database.with_connection(|connection| {
            use rusqlite::OptionalExtension;
            connection.query_row("SELECT i.source_id, i.entry_id, COALESCE(i.installed_version, d.release_version) FROM games g LEFT JOIN game_source_installs i ON i.game_id = g.id LEFT JOIN downloads d ON d.id = g.id AND d.status = 'installed' WHERE g.id = ?1", [&game.id], |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<String>>(2)?))).optional().map(|value| value.unwrap_or_default()).map_err(database_error)
        })?;
        library.games.push(Entry {
            id: game.id,
            name: game.name,
            folder: folder
                .strip_prefix(root)
                .unwrap_or(folder)
                .to_string_lossy()
                .into_owned(),
            executable: crate::manual_import::portable_relative_path(executable)?,
            steam_app_id: game.steam_app_id,
            source_id,
            entry_id,
            version,
        });
    }
    library.games.sort_by(|a, b| a.id.cmp(&b.id));
    save(root, &library)
}

pub(crate) fn adopt_existing(database: &Database) -> Result<(), String> {
    for game in database.games()? {
        if game.steam_install_path.is_some() || game.installation_root.is_some() {
            continue;
        }
        let Some(executable) = game.executable_path else {
            continue;
        };
        let root = game_root(Path::new(&executable))?;
        database.with_connection(|connection| {
            connection.execute("UPDATE games SET installation_root = ?2 WHERE id = ?1 AND installation_root IS NULL", rusqlite::params![game.id, root.to_string_lossy()]).map_err(database_error)?;
            Ok(())
        })?;
    }
    let completed: Vec<(String, String)> = database.with_connection(|connection| {
        let mut query = connection.prepare("SELECT g.installation_root, d.install_token FROM games g JOIN downloads d ON d.id = g.id WHERE d.status = 'installed' AND d.install_token IS NOT NULL AND g.installation_root IS NOT NULL").map_err(database_error)?;
        query.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).map_err(database_error)?.collect::<Result<_, _>>().map_err(database_error)
    })?;
    for (root, token) in completed {
        let root = Path::new(&root);
        if fs::symlink_metadata(root)
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
        {
            crate::finalize_install::remove_install_markers(root, &token)?;
        }
    }
    Ok(())
}

pub(crate) fn remove(root: &Path, id: &str) -> Result<(), String> {
    if !root.join(FILE).exists() {
        return Ok(());
    }
    let _guard = WRITE.lock().map_err(|_| "Library file lock is poisoned")?;
    let mut library = read(root)?;
    library.games.retain(|entry| entry.id != id);
    save(root, &library)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_normal_and_unreal_game_roots() {
        assert_eq!(
            game_root(Path::new("Games/Game/Game.exe")).unwrap(),
            Path::new("Games/Game")
        );
        assert_eq!(
            game_root(Path::new(
                "Games/Outer/Inner/Binaries/Win64/Inner-Win64-Shipping.exe"
            ))
            .unwrap(),
            Path::new("Games/Outer")
        );
        assert_eq!(
            game_root(Path::new(
                "Games/Outer/Other/binaries/win64/Any-WIN64-SHIPPING.EXE"
            ))
            .unwrap(),
            Path::new("Games/Outer")
        );
        assert_eq!(
            game_root(Path::new("Games/Game/Any-Win64-Shipping.exe")).unwrap(),
            Path::new("Games/Game")
        );
        assert_eq!(
            game_root(Path::new("Games/Inner/Binaries/Win64/Game.exe")).unwrap(),
            Path::new("Games/Inner/Binaries/Win64")
        );
    }

    #[test]
    fn folder_names_are_portable_and_never_overwrite_existing_games() {
        let directory =
            std::env::temp_dir().join(format!("legio-library-{}", uuid::Uuid::new_v4()));
        let database = Database::open(&directory).unwrap();
        fs::create_dir(directory.join("GAME")).unwrap();
        database
            .with_connection(|connection| {
                assert_eq!(
                    available_target(connection, &directory, "Game")?,
                    directory.join("Game (2)")
                );
                Ok(())
            })
            .unwrap();
        database
            .with_connection(|connection| {
                assert_eq!(
                    available_target(connection, &directory, "library.ldf")?,
                    directory.join("library.ldf (2)")
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(folder_name("CON.exe"), "_CON.exe");
        assert_eq!(folder_name("../A:B?*"), "_A_B__");
        assert_eq!(folder_name("..."), "Game");
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupt_library_is_preserved() {
        let directory =
            std::env::temp_dir().join(format!("legio-library-{}", uuid::Uuid::new_v4()));
        let database = Database::open(&directory).unwrap();
        fs::write(directory.join(FILE), "[broken").unwrap();
        assert!(sync(&database, &directory).is_err());
        assert_eq!(fs::read_to_string(directory.join(FILE)).unwrap(), "[broken");
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }
}
