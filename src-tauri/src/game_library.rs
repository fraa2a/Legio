use crate::database::{Database, database_error};
use crate::library_manifest::{self as manifest, Entry, FILE, Installation, Library, Update};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

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

pub(crate) fn installation_root(
    recorded: Option<&str>,
    executable: &Path,
) -> Result<PathBuf, String> {
    match recorded {
        Some(root) => {
            let root = dunce::canonicalize(root)
                .map_err(|e| format!("Could not inspect installation root: {e}"))?;
            if !root.is_dir() || !executable.starts_with(&root) {
                return Err("Executable is outside the recorded installation root".into());
            }
            Ok(root)
        }
        None => game_root(executable),
    }
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
        .scan(0, |bytes, character| {
            *bytes += character.len_utf8();
            (*bytes <= 200).then_some(character)
        })
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

pub(crate) fn source_hash(url: &str) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let url = crate::legio_source::validate_manifest_url(url.trim()).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(url.as_str().as_bytes())))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibraryInfo {
    id: String,
    root: String,
    warning: Option<String>,
}

pub(crate) fn libraries(database: &Database) -> Result<Vec<LibraryInfo>, String> {
    database.with_connection(|c| {
        let mut q = c
            .prepare("SELECT id, root, warning FROM game_libraries ORDER BY root")
            .map_err(database_error)?;
        q.query_map([], |r| {
            Ok(LibraryInfo {
                id: r.get(0)?,
                root: r.get(1)?,
                warning: r.get(2)?,
            })
        })
        .map_err(database_error)?
        .collect::<Result<_, _>>()
        .map_err(database_error)
    })
}

fn cached_library(database: &Database, root: &Path) -> Result<Option<(String, i64)>, String> {
    database.with_connection(|c| {
        c.query_row(
            "SELECT id,dirty FROM game_libraries WHERE root = ?1",
            [root.to_string_lossy()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(database_error)
    })
}

fn fingerprint(library: &Library) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(library).map_err(|e| e.to_string())?)
    ))
}

fn library_child(folder: &Path, root: &Path) -> bool {
    folder.parent().is_some_and(|parent| {
        parent == root || dunce::canonicalize(parent).is_ok_and(|parent| parent == root)
    })
}

fn snapshot(database: &Database, root: &Path, id: String) -> Result<(Library, i64), String> {
    database.with_connection(|c| {
        let mut q = c.prepare("SELECT g.id, COALESCE(g.name_override,g.automatic_name), g.installation_root, g.executable_path, g.steam_app_id, g.archive_layout, COALESCE(i.source_sha256,s.url_sha256), i.entry_id, COALESCE(i.installed_version,d.release_version) FROM games g LEFT JOIN game_source_installs i ON i.game_id = g.id LEFT JOIN download_sources s ON s.id = i.source_id LEFT JOIN downloads d ON d.id = g.id AND d.status = 'installed' WHERE g.steam_install_path IS NULL AND g.installation_root IS NOT NULL AND g.executable_path IS NOT NULL ORDER BY g.id").map_err(database_error)?;
        let rows = q.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,Option<u32>>(4)?,r.get::<_,String>(5)?,r.get::<_,Option<String>>(6)?,r.get::<_,Option<String>>(7)?,r.get::<_,Option<String>>(8)?))).map_err(database_error)?;
        let mut games = Vec::new();
        for row in rows {
            let (id,name,folder,executable,steam_app_id,archive_layout,hash,entry,version) = row.map_err(database_error)?;
            let mut folder = PathBuf::from(folder);
            let mut executable = PathBuf::from(executable);
            if !library_child(&folder, root) { continue; }
            if folder.parent() != Some(root) {
                folder = dunce::canonicalize(&folder).map_err(|e| e.to_string())?;
                executable = dunce::canonicalize(&executable).map_err(|e| e.to_string())?;
                c.execute("UPDATE games SET installation_root=?2,executable_path=?3 WHERE id=?1",params![id,folder.to_string_lossy(),executable.to_string_lossy()]).map_err(database_error)?;
            }
            let relative = executable.strip_prefix(&folder).map_err(|_| "Executable is outside its installation root")?;
            games.push(Entry { id,name,steam_app_id,installation: Installation {
                folder: folder.file_name().and_then(|n| n.to_str()).ok_or("Invalid game folder")?.into(),
                executable: crate::manual_import::portable_relative_path(relative)?, archive_layout,
            }, update: match (hash,entry,version) {
                (Some(source_sha256),Some(entry_id),Some(installed_version)) => Some(Update { source_sha256,entry_id,installed_version }),
                _ => None,
            }});
        }
        let revision = c.query_row("SELECT dirty FROM game_libraries WHERE root=?1",[root.to_string_lossy()],|r| r.get(0)).map_err(database_error)?;
        Ok((Library {format:1,library_id:id,games},revision))
    })
}

fn index(
    database: &Database,
    root: &Path,
    library: &Library,
    remove_missing: bool,
) -> Result<(), String> {
    manifest::validate(library)?;
    let mut paths = Vec::new();
    for game in &library.games {
        let folder = manifest::checked(root, &game.installation.folder, false)?;
        let executable = manifest::checked(&folder, &game.installation.executable, true)?;
        paths.push((
            dunce::canonicalize(folder).map_err(|e| e.to_string())?,
            dunce::canonicalize(executable).map_err(|e| e.to_string())?,
        ));
    }
    database.with_connection(|c| {
        let tx = c.unchecked_transaction().map_err(database_error)?;
        for (game,(folder,executable)) in library.games.iter().zip(paths) {
            let current: Option<(Option<String>,Option<String>)> = tx.query_row("SELECT installation_root,steam_install_path FROM games WHERE id = ?1", [&game.id], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(database_error)?;
            if let Some((old,steam)) = current
                && (steam.is_some() || old.as_deref().is_some_and(|old| Path::new(old).exists() && dunce::canonicalize(old).unwrap_or_else(|_|PathBuf::from(old)) != folder)) {
                    return Err("An imported game ID already belongs to another installation".into());
                }
            let duplicate: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM games WHERE executable_path = ?1 AND id <> ?2)", params![executable.to_string_lossy(),game.id], |r| r.get(0)).map_err(database_error)?;
            if duplicate { return Err("An imported executable already belongs to another game".into()); }
            tx.execute("INSERT INTO games (id,name_override,steam_app_id,installation_root,executable_path,archive_layout) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET steam_app_id=excluded.steam_app_id, installation_root=excluded.installation_root,executable_path=excluded.executable_path,archive_layout=excluded.archive_layout",params![game.id,game.name,game.steam_app_id,folder.to_string_lossy(),executable.to_string_lossy(),game.installation.archive_layout]).map_err(database_error)?;
            if let Some(update) = &game.update {
                let source: Option<String> = tx.query_row("SELECT id FROM download_sources WHERE url_sha256 = ?1", [&update.source_sha256], |r| r.get(0)).optional().map_err(database_error)?;
                let source = source.unwrap_or_else(|| format!("sha256:{}",update.source_sha256));
                tx.execute("INSERT INTO game_source_installs(game_id,source_id,source_sha256,entry_id,installed_version,accepted_unverified) VALUES (?1,?2,?3,?4,?5,0) ON CONFLICT(game_id) DO UPDATE SET source_id=excluded.source_id,source_sha256=excluded.source_sha256,entry_id=excluded.entry_id,installed_version=excluded.installed_version,accepted_unverified=CASE WHEN game_source_installs.source_sha256=excluded.source_sha256 AND game_source_installs.entry_id=excluded.entry_id THEN game_source_installs.accepted_unverified ELSE 0 END",params![game.id,source,update.source_sha256,update.entry_id,update.installed_version]).map_err(database_error)?;
            } else {
                tx.execute("DELETE FROM game_source_installs WHERE game_id=?1",[&game.id]).map_err(database_error)?;
            }
        }
        if remove_missing {
            let removed = {
                let mut query = tx.prepare("SELECT id,installation_root FROM games WHERE steam_install_path IS NULL AND installation_root IS NOT NULL").map_err(database_error)?;
                let rows = query.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).map_err(database_error)?;
                let ids: std::collections::HashSet<_> = library.games.iter().map(|game| game.id.as_str()).collect();
                let mut removed = Vec::new();
                for row in rows {
                    let (id,path) = row.map_err(database_error)?;
                    if library_child(Path::new(&path), root) && !ids.contains(id.as_str()) {
                        removed.push(id);
                    }
                }
                removed
            };
            for id in removed {
                tx.execute("DELETE FROM game_source_installs WHERE game_id=?1",[&id]).map_err(database_error)?;
                tx.execute("UPDATE games SET installation_root=NULL,executable_path=NULL WHERE id=?1",[&id]).map_err(database_error)?;
            }
        }
        tx.commit().map_err(database_error)
    })
}

fn load(database: &Database, root: &Path) -> Result<Option<Library>, String> {
    match manifest::read(root) {
        Ok(library) => Ok(library),
        Err(original) => {
            let path = root.join(FILE);
            let metadata = fs::symlink_metadata(&path).map_err(|_| original.clone())?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > 16 * 1024 * 1024
            {
                return Err(original);
            }
            let text = fs::read_to_string(&path).map_err(|_| original.clone())?;
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Legacy {
                format: u32,
                #[serde(default)]
                games: Vec<OldEntry>,
            }
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct OldEntry {
                id: String,
                name: String,
                folder: String,
                executable: String,
                steam_app_id: Option<u32>,
                source_id: Option<String>,
                entry_id: Option<String>,
                version: Option<String>,
            }
            let old: Legacy = toml::from_str(&text).map_err(|_| original.clone())?;
            if old.format != 1 {
                return Err(original);
            }
            let id = cached_library(database, root)?
                .map(|r| r.0)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let mut games = Vec::new();
            for game in old.games {
                if Path::new(&game.folder).is_absolute() {
                    continue;
                }
                let update = match (game.source_id, game.entry_id, game.version) {
                    (Some(source), Some(entry_id), Some(installed_version)) => {
                        let hash: Option<String> = database.with_connection(|c| {
                            c.query_row(
                                "SELECT url_sha256 FROM download_sources WHERE id=?1",
                                [source],
                                |r| r.get(0),
                            )
                            .optional()
                            .map(|v| v.flatten())
                            .map_err(database_error)
                        })?;
                        hash.map(|source_sha256| Update {
                            source_sha256,
                            entry_id,
                            installed_version,
                        })
                    }
                    _ => None,
                };
                let layout = if game.folder == game.id {
                    "legacy"
                } else {
                    "direct"
                };
                games.push(Entry {
                    id: game.id,
                    name: game.name,
                    steam_app_id: game.steam_app_id,
                    installation: Installation {
                        folder: game.folder,
                        executable: game.executable,
                        archive_layout: layout.into(),
                    },
                    update,
                });
            }
            let library = Library {
                format: 1,
                library_id: id,
                games,
            };
            manifest::validate(&library)?;
            let backup = root.join(format!("library.ldf.legacy-{}", uuid::Uuid::new_v4()));
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(backup)
                .map_err(|e| e.to_string())?;
            use std::io::Write;
            output
                .write_all(text.as_bytes())
                .and_then(|()| output.sync_all())
                .map_err(|e| e.to_string())?;
            manifest::save(root, &library)?;
            Ok(Some(library))
        }
    }
}

fn register(
    database: &Database,
    root: &Path,
    existing: Option<&Library>,
) -> Result<String, String> {
    let id = existing
        .map(|l| l.library_id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    database.with_connection(|c| {
        if let Some(other) = c
            .query_row(
                "SELECT root FROM game_libraries WHERE id = ?1 AND root <> ?2",
                params![id, root.to_string_lossy()],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(database_error)?
        {
            if Path::new(&other).exists() {
                return Err("This library ID is already registered at another location".into());
            }
            c.execute(
                "UPDATE game_libraries SET root=?2 WHERE id=?1",
                params![id, root.to_string_lossy()],
            )
            .map_err(database_error)?;
        }
        c.execute(
            "INSERT INTO game_libraries(id,root) VALUES (?1,?2) ON CONFLICT(root) DO NOTHING",
            params![id, root.to_string_lossy()],
        )
        .map_err(database_error)?;
        Ok(())
    })?;
    cached_library(database, root)?
        .map(|r| r.0)
        .ok_or_else(|| "Could not register library".into())
}

pub(crate) fn import(database: &Database, root: &Path) -> Result<(), String> {
    if !root.is_absolute() {
        return Err("Library directory must be absolute".into());
    }
    let root = dunce::canonicalize(root).map_err(|e| e.to_string())?;
    let _lock = manifest::lock(&root)?;
    let library = load(database, &root)?.ok_or("No library.ldf in the selected directory")?;
    if cached_library(database, &root)?.is_some_and(|(_, dirty)| dirty != 0) {
        return Err("Publish pending local changes before importing this library".into());
    }
    if cached_library(database, &root)?.is_some_and(|(id, _)| id != library.library_id) {
        return Err("Library identity changed".into());
    }
    let conflict: Option<String> = database.with_connection(|c| {
        c.query_row(
            "SELECT root FROM game_libraries WHERE id=?1 AND root<>?2",
            params![library.library_id, root.to_string_lossy()],
            |r| r.get(0),
        )
        .optional()
        .map_err(database_error)
    })?;
    if conflict.is_some_and(|path| Path::new(&path).exists()) {
        return Err("This library is already registered at another location".into());
    }
    index(database, &root, &library, true)?;
    let id = register(database, &root, Some(&library))?;
    if id != library.library_id {
        return Err("Library identity changed".into());
    }
    database.with_connection(|c| {
        c.execute(
            "UPDATE game_libraries SET dirty=0,warning=NULL,manifest_hash=?2 WHERE root=?1",
            params![root.to_string_lossy(), fingerprint(&library)?],
        )
        .map_err(database_error)?;
        Ok(())
    })
}

pub(crate) fn sync(database: &Database, root: &Path) -> Result<(), String> {
    let root = dunce::canonicalize(root).map_err(|e| e.to_string())?;
    let _lock = manifest::lock(&root)?;
    let existing = load(database, &root)?;
    let known = cached_library(database, &root)?;
    if known.is_none()
        && let Some(library) = &existing
    {
        index(database, &root, library, false)?;
    }
    let id = register(database, &root, existing.as_ref())?;
    if existing.as_ref().is_some_and(|l| l.library_id != id) {
        return Err("Library identity does not match the registered directory".into());
    }
    let (desired, revision) = snapshot(database, &root, id)?;
    let expected: Option<String> = database.with_connection(|c| {
        c.query_row(
            "SELECT manifest_hash FROM game_libraries WHERE root=?1",
            [root.to_string_lossy()],
            |r| r.get(0),
        )
        .map_err(database_error)
    })?;
    let desired_hash = fingerprint(&desired)?;
    let current_hash = existing.as_ref().map(fingerprint).transpose()?;
    if let (Some(expected), Some(current)) = (expected.as_deref(), current_hash.as_deref())
        && current != expected
        && current != desired_hash
    {
        return Err("Library changed outside this profile; local publication is pending. Import it before making local changes, or repair after reviewing the backup.".into());
    }
    if current_hash.as_deref() != Some(desired_hash.as_str()) {
        manifest::save(&root, &desired)?;
    }
    database.with_connection(|c| {
        c.execute(
            "UPDATE game_libraries SET dirty=CASE WHEN dirty=?2 THEN 0 ELSE dirty END,warning=NULL,manifest_hash=?3 WHERE root=?1",
            params![root.to_string_lossy(), revision,desired_hash],
        )
        .map_err(database_error)?;
        Ok(())
    })
}

pub(crate) fn publish(database: &Database, root: &Path) {
    let normalized = dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let root = normalized.as_path();
    if let Err(error) = sync(database, root) {
        crate::application_log::failure("library_publication", &error);
        if cached_library(database, root).ok().flatten().is_none()
            && let Err(register_error) = register(database, root, None)
        {
            crate::application_log::failure("library_registration", register_error);
        }
        if let Err(persist) = database.with_connection(|c| {
            c.execute(
                "UPDATE game_libraries SET warning=?2 WHERE root=?1",
                params![root.to_string_lossy(), error],
            )
            .map_err(database_error)?;
            Ok(())
        }) {
            crate::application_log::failure("library_publication_warning", persist);
        }
    }
}

pub(crate) fn sync_default<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    use tauri::Manager;
    let state = app.state::<crate::database::DatabaseState>();
    flush_registered(state.database()?);
    Ok(())
}

pub(crate) fn sync_storage(database: &Database, storage: &Path) -> Result<(), String> {
    match crate::finalize_install::install_root(database, storage) {
        Ok(root) => publish(database, &root),
        Err(error) => crate::application_log::failure("library_directory", error),
    }
    flush_registered(database);
    Ok(())
}

pub(crate) fn flush_registered(database: &Database) {
    let pending = database.with_connection(|c| {
        let mut query = c
            .prepare("SELECT root FROM game_libraries WHERE dirty != 0 ORDER BY root")
            .map_err(database_error)?;
        query
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(database_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(database_error)
    });
    match pending {
        Ok(roots) => {
            for root in roots {
                publish(database, Path::new(&root));
            }
        }
        Err(error) => crate::application_log::failure("library_index", error),
    }
}

pub(crate) fn remove_default<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    _id: &str,
) -> Result<(), String> {
    sync_default(app)
}

pub(crate) fn initialize(database: &Database, storage: &Path) {
    match libraries(database) {
        Ok(libraries) => {
            for library in libraries {
                let root = Path::new(&library.root);
                match cached_library(database, root) {
                    Ok(Some((_, 0))) if root.join(FILE).is_file() => {
                        if let Err(error) = import(database, root) {
                            crate::application_log::failure("library_import", error);
                        }
                    }
                    _ => publish(database, root),
                }
            }
        }
        Err(error) => crate::application_log::failure("library_index", error),
    }
    if let Err(error) = sync_storage(database, storage) {
        crate::application_log::failure("library_startup", error);
    }
}

pub(crate) fn repair(database: &Database, root: &Path) -> Result<(), String> {
    let root = dunce::canonicalize(root).map_err(|e| e.to_string())?;
    if cached_library(database, &root)?.is_none() {
        return Err("Library is not registered".into());
    }
    {
        let _lock = manifest::lock(&root)?;
        if root.join(FILE).exists() {
            let backup = root.join(format!("library.ldf.backup-{}", uuid::Uuid::new_v4()));
            fs::rename(root.join(FILE), backup).map_err(|e| e.to_string())?;
        }
    }
    sync(database, &root)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn installed_fixture() -> (PathBuf, Database, String) {
        let directory =
            std::env::temp_dir().join(format!("legio-catalog-{}", uuid::Uuid::new_v4()));
        let database = Database::open(&directory.join("profile")).unwrap();
        fs::create_dir_all(directory.join("games/Game")).unwrap();
        fs::write(directory.join("games/Game/game.exe"), b"game").unwrap();
        let game = database
            .create_game(crate::database::CreateGameInput {
                name: "Game".into(),
                steam_app_id: Some(42),
            })
            .unwrap();
        database
            .with_connection(|c| {
                c.execute(
                    "UPDATE games SET executable_path=?2,installation_root=?3 WHERE id=?1",
                    params![
                        game.id,
                        directory.join("games/Game/game.exe").to_string_lossy(),
                        directory.join("games/Game").to_string_lossy()
                    ],
                )
                .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        (directory, database, game.id)
    }

    #[test]
    fn local_playtime_changes_do_not_dirty_portable_metadata() {
        let (directory, database, id) = installed_fixture();
        let root = dunce::canonicalize(directory.join("games")).unwrap();
        sync(&database, &root).unwrap();
        database.set_playtime(&id, 12000, 12000).unwrap();
        assert_eq!(cached_library(&database, &root).unwrap().unwrap().1, 0);
        database
            .with_connection(|c| {
                c.execute(
                    "UPDATE games SET name_override='Renamed' WHERE id=?1",
                    [&id],
                )
                .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        assert!(cached_library(&database, &root).unwrap().unwrap().1 > 0);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn unchanged_sync_preserves_manifest_contents_and_modification_time() {
        let (directory, database, _) = installed_fixture();
        let root = directory.join("games");
        sync(&database, &root).unwrap();
        let path = root.join(FILE);
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        let modified = std::time::UNIX_EPOCH + std::time::Duration::from_secs(100);
        file.set_modified(modified).unwrap();
        drop(file);
        let before = fs::read(&path).unwrap();
        sync(&database, &root).unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn another_profile_cannot_overwrite_a_manifest_that_changed_since_import() {
        let (directory, database, id) = installed_fixture();
        let root = directory.join("games");
        sync(&database, &root).unwrap();
        let other = Database::open(&directory.join("other")).unwrap();
        import(&other, &root).unwrap();
        let mut input = crate::database::UpdateGameInput {
            id: id.clone(),
            steam_app_id: Some(42),
            automatic_name: None,
            name_override: Some("Changed elsewhere".into()),
        };
        other.update_game(input.clone()).unwrap();
        sync(&other, &root).unwrap();
        input.name_override = Some("Local change".into());
        database.update_game(input).unwrap();
        assert!(sync(&database, &root).is_err());
        assert_eq!(
            manifest::read(&root).unwrap().unwrap().games[0].name,
            "Changed elsewhere"
        );
        drop(other);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn importing_removed_installations_preserves_history_without_republishing_them() {
        let (directory, database, id) = installed_fixture();
        let root = directory.join("games");
        sync(&database, &root).unwrap();
        let mut library = manifest::read(&root).unwrap().unwrap();
        library.games.clear();
        manifest::save(&root, &library).unwrap();
        import(&database, &root).unwrap();
        assert!(database.game(&id).unwrap().installation_root.is_none());
        sync(&database, &root).unwrap();
        assert!(manifest::read(&root).unwrap().unwrap().games.is_empty());
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn equivalent_parent_paths_are_indexed_and_normalized_for_pending_writes() {
        let (directory, database, id) = installed_fixture();
        let root = directory.join("games");
        let alias = root.join("../games/Game");
        database
            .with_connection(|c| {
                c.execute(
                    "UPDATE games SET installation_root=?2,executable_path=?3 WHERE id=?1",
                    params![
                        id,
                        alias.to_string_lossy(),
                        alias.join("game.exe").to_string_lossy()
                    ],
                )
                .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        sync(&database, &root).unwrap();
        assert_eq!(manifest::read(&root).unwrap().unwrap().games.len(), 1);
        assert_eq!(
            PathBuf::from(database.game(&id).unwrap().installation_root.unwrap()),
            dunce::canonicalize(root.join("Game")).unwrap()
        );
        assert_eq!(
            cached_library(&database, &dunce::canonicalize(&root).unwrap())
                .unwrap()
                .unwrap()
                .1,
            0
        );
        publish(&database, &root.join("../games"));
        assert_eq!(libraries(&database).unwrap().len(), 1);
        database
            .with_connection(|c| {
                c.execute("DELETE FROM games WHERE id=?1", [&id])
                    .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        assert!(
            cached_library(&database, &dunce::canonicalize(&root).unwrap())
                .unwrap()
                .unwrap()
                .1
                > 0
        );
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn simultaneous_publications_keep_every_game() {
        let (directory, database, _) = installed_fixture();
        let root = directory.join("games");
        sync(&database, &root).unwrap();
        let database = std::sync::Arc::new(database);
        let workers: Vec<_> = (0..4)
            .map(|n| {
                let db = database.clone();
                let root = root.clone();
                std::thread::spawn(move || {
                    let folder = root.join(format!("Game{n}"));
                    fs::create_dir(&folder).unwrap();
                    fs::write(folder.join("game.exe"), b"game").unwrap();
                    let game = db
                        .create_game(crate::database::CreateGameInput {
                            name: format!("Game{n}"),
                            steam_app_id: None,
                        })
                        .unwrap();
                    db.with_connection(|c| {
                        c.execute(
                            "UPDATE games SET installation_root=?2,executable_path=?3 WHERE id=?1",
                            params![
                                game.id,
                                folder.to_string_lossy(),
                                folder.join("game.exe").to_string_lossy()
                            ],
                        )
                        .map_err(database_error)?;
                        Ok(())
                    })
                    .unwrap();
                    sync(&db, &root).unwrap();
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(manifest::read(&root).unwrap().unwrap().games.len(), 5);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn source_hash_is_stable_for_the_validated_url() {
        assert_eq!(
            source_hash("https://example.com/legio/main.json").unwrap(),
            "8f756aa62590a9b82af74a5ddc9f3e24124f511fdf8cc585c358db0e5a80b7d8"
        );
        assert_eq!(
            source_hash("https://EXAMPLE.com:443/legio/main.json").unwrap(),
            source_hash("https://example.com/legio/main.json").unwrap()
        );
        assert!(source_hash("http://example.com/source.json").is_err());
    }

    #[test]
    fn legacy_manifest_migration_retains_update_layout_and_original_backup() {
        let (directory, database, id) = installed_fixture();
        let root = directory.join("games");
        let old = format!(
            "format = 1\n[[games]]\nid = '{id}'\nname = 'Old game'\nfolder = '{id}'\nexecutable = 'Game/Game.exe'\n"
        );
        fs::write(root.join(FILE), &old).unwrap();
        let library = load(&database, &root).unwrap().unwrap();
        assert_eq!(library.games[0].installation.archive_layout, "legacy");
        assert_eq!(manifest::read(&root).unwrap().unwrap().games[0].id, id);
        let backup = fs::read_dir(&root)
            .unwrap()
            .find_map(|entry| {
                let path = entry.unwrap().path();
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("library.ldf.legacy-")
                    .then_some(path)
            })
            .unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), old);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn importing_into_a_new_profile_restores_paths_and_unresolved_update_binding() {
        let (directory, database, id) = installed_fixture();
        let url = "https://example.com/legio/main.json";
        let hash = source_hash(url).unwrap();
        database.with_connection(|c| {
            c.execute("INSERT INTO download_sources(id,url,url_sha256,manifest,fetched_at) VALUES('source',?1,?2,x'7b7d',0)",params![url,hash]).map_err(database_error)?;
            c.execute("INSERT INTO game_source_installs(game_id,source_id,source_sha256,entry_id,installed_version,accepted_unverified) VALUES(?1,'source',?2,'variant','1.0',1)",params![id,hash]).map_err(database_error)?;
            Ok(())
        }).unwrap();
        sync(&database, &directory.join("games")).unwrap();
        let text = fs::read_to_string(directory.join("games/library.ldf")).unwrap();
        assert!(!text.contains(url));
        assert!(text.contains(&hash));
        let restored = Database::open(&directory.join("restored")).unwrap();
        import(&restored, &directory.join("games")).unwrap();
        assert_eq!(
            restored.game(&id).unwrap().executable_path,
            database.game(&id).unwrap().executable_path
        );
        let (source,accepted): (String,bool) = restored.with_connection(|c| c.query_row("SELECT source_id,accepted_unverified FROM game_source_installs WHERE game_id=?1",[&id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(database_error)).unwrap();
        assert_eq!(source, format!("sha256:{hash}"));
        assert!(!accepted);
        drop(restored);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn pending_deletion_does_not_resurrect_a_game_after_restart() {
        let (directory, database, id) = installed_fixture();
        let root = directory.join("games");
        sync(&database, &root).unwrap();
        database
            .with_connection(|c| {
                c.execute("DELETE FROM games WHERE id=?1", [&id])
                    .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        drop(database);
        let database = Database::open(&directory.join("profile")).unwrap();
        initialize(&database, &directory);
        assert!(database.games().unwrap().is_empty());
        assert!(manifest::read(&root).unwrap().unwrap().games.is_empty());
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupt_manifest_does_not_fail_a_committed_removal_and_can_be_repaired() {
        let (directory, database, id) = installed_fixture();
        let root = directory.join("games");
        sync(&database, &root).unwrap();
        fs::write(root.join(FILE), "{broken").unwrap();
        database.remove_game(&id).unwrap();
        assert!(database.games().unwrap().is_empty());
        assert_eq!(fs::read_to_string(root.join(FILE)).unwrap(), "{broken");
        assert!(libraries(&database).unwrap()[0].warning.is_some());
        repair(&database, &root).unwrap();
        assert!(manifest::read(&root).unwrap().unwrap().games.is_empty());
        assert!(fs::read_dir(&root).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("library.ldf.backup-")
        }));
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn import_rejects_paths_that_escape_the_library_without_creating_games() {
        let (directory, database, _) = installed_fixture();
        let root = directory.join("games");
        sync(&database, &root).unwrap();
        let mut value = manifest::read(&root).unwrap().unwrap();
        value.games[0].installation.executable = "../../outside.exe".into();
        fs::write(root.join(FILE), serde_json::to_vec(&value).unwrap()).unwrap();
        let fresh = Database::open(&directory.join("fresh")).unwrap();
        assert!(import(&fresh, &root).is_err());
        assert!(fresh.games().unwrap().is_empty());
        drop(fresh);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn json_manifest_is_portable_and_does_not_export_source_urls() {
        let directory = std::env::temp_dir().join(format!("legio-json-{}", uuid::Uuid::new_v4()));
        let database = Database::open(&directory).unwrap();
        let root = directory.join("games");
        fs::create_dir_all(root.join("Game/bin")).unwrap();
        fs::write(root.join("Game/bin/game.exe"), b"game").unwrap();
        let game = database
            .create_game(crate::database::CreateGameInput {
                name: "Game".into(),
                steam_app_id: Some(42),
            })
            .unwrap();
        database
            .with_connection(|c| {
                c.execute(
                    "UPDATE games SET executable_path = ?2, installation_root = ?3 WHERE id = ?1",
                    rusqlite::params![
                        game.id,
                        root.join("Game/bin/game.exe").to_string_lossy(),
                        root.join("Game").to_string_lossy()
                    ],
                )
                .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        sync(&database, &root).unwrap();
        let text = fs::read_to_string(root.join(FILE)).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(manifest["games"][0]["installation"]["folder"], "Game");
        assert_eq!(
            manifest["games"][0]["installation"]["executable"],
            "bin/game.exe"
        );
        assert!(!text.contains(directory.to_str().unwrap()));
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recorded_root_preserves_sibling_resources() {
        let directory = std::env::temp_dir().join(format!("legio-root-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(directory.join("Game/bin")).unwrap();
        fs::write(directory.join("Game/bin/game.exe"), b"game").unwrap();
        fs::write(directory.join("Game/assets.dat"), b"resources").unwrap();
        let executable = dunce::canonicalize(directory.join("Game/bin/game.exe")).unwrap();
        let recorded = directory.join("Game");
        assert_eq!(
            installation_root(Some(recorded.to_str().unwrap()), &executable).unwrap(),
            dunce::canonicalize(&recorded).unwrap()
        );
        assert_eq!(
            installation_root(None, &executable).unwrap(),
            executable.parent().unwrap()
        );
        fs::remove_dir_all(directory).unwrap();
    }

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
        let unicode = folder_name(&"🎮".repeat(100));
        assert!(unicode.len() <= 200);
        fs::create_dir(directory.join(unicode)).unwrap();
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
