//! Export and import of user data: a zip archive with a manifest, a snapshot of
//! the database and the custom artwork and theme backgrounds stored next to it.
//! An import is staged while Legio runs and applied at the next start, before
//! the database opens; the replaced database and files are kept aside.

use crate::database::{
    DATABASE_FILE_NAME, Database, DatabaseState, SCHEMA_VERSION, backup, now_milliseconds,
};
use crate::database_recovery::{self, move_back, set_aside};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const FORMAT: &str = "legio-data";
const FORMAT_VERSION: u32 = 1;
const MANIFEST_ENTRY: &str = "manifest.json";
// User-authored files outside the database. Shortcut icons are derived and stay on this PC.
const FILE_DIRECTORIES: [&str; 3] = ["game-icons", "game-banners", "theme-backgrounds"];
// Caches the app refills on its own, emptied in the exported copy.
const CACHE_TABLES: [&str; 5] = [
    "catalog_cache",
    "catalog_games",
    "legio_source_cache",
    "link_checks",
    "steam_details_cache",
];
const IMPORT_DIRECTORY: &str = "data-import";
const STAGING_DIRECTORY: &str = "staging";
const READY_DIRECTORY: &str = "ready";
const APPLYING_DIRECTORY: &str = "applying";
const MIB: u64 = 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 4096 * MIB;
const MAX_TOTAL_BYTES: u64 = 4096 * MIB;
const MAX_DATABASE_BYTES: u64 = 2048 * MIB;
const MAX_FILE_BYTES: u64 = 16 * MIB;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_ENTRIES: usize = 20_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: String,
    format_version: u32,
    app_version: String,
    schema_version: i64,
    created_at: i64,
    platform: String,
}

#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Manifest,
    Database,
    File {
        directory: &'static str,
        name: String,
    },
}

/// Why the import staged by the previous run was not applied, shown once at startup.
pub(crate) struct ImportFailure(pub(crate) Option<String>);

fn parse_manifest(bytes: &[u8]) -> Result<Manifest, String> {
    let manifest: Manifest = serde_json::from_slice(bytes)
        .map_err(|error| format!("The export manifest is invalid: {error}"))?;
    if manifest.format != FORMAT {
        return Err("This file is not a Legio data export".to_owned());
    }
    if manifest.format_version > FORMAT_VERSION {
        return Err(
            "This export was made by a newer version of Legio; update Legio first".to_owned(),
        );
    }
    if manifest.format_version != FORMAT_VERSION {
        return Err(format!(
            "Unsupported export format version {}",
            manifest.format_version
        ));
    }
    if manifest.schema_version > SCHEMA_VERSION {
        return Err("This export uses a newer database version; update Legio first".to_owned());
    }
    if manifest.schema_version < 1 {
        return Err(format!(
            "The export has invalid database version {}",
            manifest.schema_version
        ));
    }
    Ok(manifest)
}

// Only `<uuid>.<image extension>` is accepted, so a name never leaves its folder.
fn is_data_file_name(directory: &str, name: &str) -> bool {
    let Some((stem, extension)) = name.split_once('.') else {
        return false;
    };
    let extensions: &[&str] = if directory == "theme-backgrounds" {
        &["png"]
    } else {
        &["png", "jpg", "webp"]
    };
    extensions.contains(&extension)
        && uuid::Uuid::parse_str(stem).is_ok_and(|id| id.to_string() == stem)
}

fn classify_entry(name: &str) -> Result<Entry, String> {
    if name == MANIFEST_ENTRY {
        return Ok(Entry::Manifest);
    }
    if name == DATABASE_FILE_NAME {
        return Ok(Entry::Database);
    }
    name.split_once('/')
        .and_then(|(directory, file_name)| {
            let directory = FILE_DIRECTORIES
                .into_iter()
                .find(|known| *known == directory)?;
            is_data_file_name(directory, file_name).then(|| Entry::File {
                directory,
                name: file_name.to_owned(),
            })
        })
        .ok_or_else(|| format!("The export contains an unexpected entry: {name}"))
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn remove_directory_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Copies the database without caches into `target` and returns its schema version.
fn write_database_snapshot(database: &Database, target: &Path) -> Result<i64, String> {
    database.with_connection(|connection| backup::snapshot_database(connection, target))?;
    let connection = Connection::open(target)
        .map_err(|error| format!("could not open the exported database: {error}"))?;
    for table in CACHE_TABLES {
        connection
            .execute(&format!("DELETE FROM {table}"), [])
            .map_err(|error| {
                format!("could not clear {table} in the exported database: {error}")
            })?;
    }
    connection
        .execute_batch("VACUUM")
        .map_err(|error| format!("could not compact the exported database: {error}"))?;
    let version = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(crate::database::database_error)?;
    connection
        .close()
        .map_err(|(_, error)| format!("could not close the exported database: {error}"))?;
    Ok(version)
}

fn collect_data_files(data_dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut files = Vec::new();
    for directory in FILE_DIRECTORIES {
        let entries = match fs::read_dir(data_dir.join(directory)) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("Could not read {directory}: {error}")),
        };
        for entry in entries {
            let entry = entry.map_err(|error| format!("Could not read {directory}: {error}"))?;
            let metadata = entry
                .metadata()
                .map_err(|error| format!("Could not inspect {directory}: {error}"))?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            // Symbolic links, temporary files and oversized files are not user data.
            if entry.file_type().is_ok_and(|kind| kind.is_file())
                && metadata.len() <= MAX_FILE_BYTES
                && is_data_file_name(directory, &name)
            {
                files.push((format!("{directory}/{name}"), entry.path()));
            }
        }
    }
    files.sort();
    Ok(files)
}

fn write_archive(
    file: File,
    manifest: &Manifest,
    database: &Path,
    files: &[(String, PathBuf)],
) -> Result<File, String> {
    let mut writer = ZipWriter::new(file);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    // Images are already compressed.
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let manifest = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
    let mut add = |name: &str, options: SimpleFileOptions, source: &mut dyn Read| {
        writer
            .start_file(name, options)
            .and_then(|()| io::copy(source, &mut writer).map_err(Into::into))
            .map(|_| ())
            .map_err(|error| format!("Could not add {name} to the export: {error}"))
    };
    add(MANIFEST_ENTRY, deflated, &mut manifest.as_slice())?;
    let open = |path: &Path| {
        File::open(path).map_err(|error| format!("Could not read {}: {error}", path.display()))
    };
    add(DATABASE_FILE_NAME, deflated, &mut open(database)?)?;
    for (name, path) in files {
        add(name, stored, &mut open(path)?)?;
    }
    writer
        .finish()
        .map_err(|error| format!("Could not finish the export: {error}"))
}

/// Writes the user data archive to `destination`, replacing it only once complete.
pub(crate) fn export_data(
    database: &Database,
    data_dir: &Path,
    destination: &Path,
    app_version: &str,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "The export path has no folder".to_owned())?;
    let file_name = destination
        .file_name()
        .ok_or_else(|| "The export path has no file name".to_owned())?
        .to_string_lossy();
    let id = uuid::Uuid::new_v4();
    let snapshot = data_dir.join(format!(".legio-export-{id}.sqlite3.tmp"));
    let temporary = parent.join(format!(".{file_name}.{id}.tmp"));
    let result = (|| {
        let schema_version = write_database_snapshot(database, &snapshot)?;
        let manifest = Manifest {
            format: FORMAT.to_owned(),
            format_version: FORMAT_VERSION,
            app_version: app_version.to_owned(),
            schema_version,
            created_at: now_milliseconds()?,
            platform: std::env::consts::OS.to_owned(),
        };
        let files = collect_data_files(data_dir)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Could not create the export: {error}"))?;
        write_archive(file, &manifest, &snapshot, &files)?
            .sync_all()
            .map_err(|error| format!("Could not save the export: {error}"))?;
        fs::rename(&temporary, destination)
            .map_err(|error| format!("Could not save the export: {error}"))?;
        crate::finalize_install::sync_directory(parent)
    })();
    let mut errors: Vec<String> = result.err().into_iter().collect();
    for path in [&snapshot, &temporary] {
        if let Err(error) = remove_if_present(path) {
            errors.push(format!("could not remove {}: {error}", path.display()));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn copy_bounded(source: &mut impl Read, target: &Path, limit: u64) -> Result<u64, String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|error| format!("Could not stage {}: {error}", target.display()))?;
    let copied = io::copy(&mut source.take(limit.saturating_add(1)), &mut file)
        .map_err(|error| format!("Could not read the export: {error}"))?;
    if copied > limit {
        return Err("The export contains a file that is too large".to_owned());
    }
    file.sync_all()
        .map_err(|error| format!("Could not stage {}: {error}", target.display()))?;
    Ok(copied)
}

/// Extracts and validates an untrusted export into `staging`.
fn extract_archive(source: &Path, staging: &Path) -> Result<Manifest, String> {
    let file = File::open(source).map_err(|error| format!("Could not open the export: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Could not open the export: {error}"))?;
    if !metadata.is_file() || metadata.len() > MAX_ARCHIVE_BYTES {
        return Err("Select a Legio data export of at most 4 GiB".to_owned());
    }
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("The file is not a valid Legio data export: {error}"))?;
    if archive.len() > MAX_ENTRIES {
        return Err("The export has too many entries".to_owned());
    }
    let mut manifest = None;
    let mut has_database = false;
    let mut seen = HashSet::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("The export has an unreadable entry: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .name()
            .map_err(|_| "The export has an entry with an invalid name".to_owned())?
            .into_owned();
        if !seen.insert(name.clone()) {
            return Err(format!("The export has a duplicate entry: {name}"));
        }
        let remaining = MAX_TOTAL_BYTES.saturating_sub(total);
        total += match classify_entry(&name)? {
            Entry::Manifest => {
                let mut bytes = Vec::new();
                entry
                    .by_ref()
                    .take(MAX_MANIFEST_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|error| format!("Could not read the export manifest: {error}"))?;
                if bytes.len() as u64 > MAX_MANIFEST_BYTES {
                    return Err("The export manifest is too large".to_owned());
                }
                manifest = Some(parse_manifest(&bytes)?);
                bytes.len() as u64
            }
            Entry::Database => {
                has_database = true;
                copy_bounded(
                    &mut entry,
                    &staging.join(DATABASE_FILE_NAME),
                    MAX_DATABASE_BYTES.min(remaining),
                )?
            }
            Entry::File {
                directory,
                name: file_name,
            } => {
                let folder = staging.join(directory);
                fs::create_dir_all(&folder)
                    .map_err(|error| format!("Could not stage {directory}: {error}"))?;
                copy_bounded(
                    &mut entry,
                    &folder.join(file_name),
                    MAX_FILE_BYTES.min(remaining),
                )?
            }
        };
    }
    let manifest = manifest.ok_or_else(|| "The export has no manifest".to_owned())?;
    if !has_database {
        return Err("The export has no database".to_owned());
    }
    let version = backup::verify_database_copy(&staging.join(DATABASE_FILE_NAME))?;
    if version != manifest.schema_version {
        return Err("The export database does not match its manifest".to_owned());
    }
    Ok(manifest)
}

/// Validates an export and stages it to replace the user data at the next start.
pub(crate) fn stage_import(data_dir: &Path, source: &Path) -> Result<(), String> {
    let root = data_dir.join(IMPORT_DIRECTORY);
    // A newer import replaces one that was staged but not applied yet.
    remove_directory_if_present(&root)
        .map_err(|error| format!("Could not clear the previous import: {error}"))?;
    let staging = root.join(STAGING_DIRECTORY);
    let result = (|| {
        fs::create_dir_all(&staging)
            .map_err(|error| format!("Could not create the import folder: {error}"))?;
        let manifest = extract_archive(source, &staging)?;
        fs::rename(&staging, root.join(READY_DIRECTORY))
            .map_err(|error| format!("Could not stage the import: {error}"))?;
        crate::finalize_install::sync_directory(&root)?;
        Ok(manifest)
    })();
    match result {
        Ok(manifest) => {
            crate::application_log::state_change(
                "data_import",
                &format!(
                    "Staged an export from Legio {} on {} (schema {}) for the next start",
                    manifest.app_version, manifest.platform, manifest.schema_version
                ),
            );
            Ok(())
        }
        Err(error) => Err(match remove_directory_if_present(&root) {
            Ok(()) => error,
            Err(cleanup) => format!("{error}; import cleanup failed: {cleanup}"),
        }),
    }
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn move_into_place(
    data_dir: &Path,
    staged: &Path,
    files_aside: &Path,
    moved: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), String> {
    let mut rename = |source: PathBuf, target: PathBuf| {
        fs::rename(&source, &target).map_err(|error| {
            format!(
                "could not move {} to {}: {error}",
                source.display(),
                target.display()
            )
        })?;
        moved.push((source, target));
        Ok::<(), String>(())
    };
    for directory in FILE_DIRECTORIES {
        let current = data_dir.join(directory);
        if exists(&current) {
            fs::create_dir_all(files_aside).map_err(|error| {
                format!("could not create the folder for replaced files: {error}")
            })?;
            rename(current, files_aside.join(directory))?;
        }
    }
    rename(
        staged.join(DATABASE_FILE_NAME),
        data_dir.join(DATABASE_FILE_NAME),
    )?;
    for directory in FILE_DIRECTORIES {
        let source = staged.join(directory);
        if exists(&source) {
            rename(source, data_dir.join(directory))?;
        }
    }
    crate::finalize_install::sync_directory(data_dir)
}

/// Swaps in the staged import and returns the name the previous database was kept as.
fn replace_with_import(data_dir: &Path, staged: &Path) -> Result<String, String> {
    backup::verify_database_copy(&staged.join(DATABASE_FILE_NAME))?;
    let aside = set_aside(data_dir, "replaced")?;
    let (kept_as, mut moved) = aside.map_or_else(
        || ("none".to_owned(), Vec::new()),
        |aside| (aside.file_name, aside.moved),
    );
    let files_aside = data_dir.join(format!("legio-replaced-files-{}", now_milliseconds()?));
    if let Err(error) = move_into_place(data_dir, staged, &files_aside, &mut moved) {
        let error = match move_back(&moved) {
            Ok(()) => error,
            Err(rollback) => format!("{error}; {rollback}"),
        };
        // Fails and keeps the folder when the rollback could not empty it.
        if let Err(cleanup) = fs::remove_dir(&files_aside)
            && cleanup.kind() != io::ErrorKind::NotFound
        {
            crate::application_log::failure("data_import_cleanup", cleanup);
        }
        return Err(error);
    }
    Ok(kept_as)
}

/// Applies an import staged by the previous run. Runs before the database opens.
pub(crate) fn apply_staged_import(data_dir: &Path) -> Result<(), String> {
    let root = data_dir.join(IMPORT_DIRECTORY);
    let ready = root.join(READY_DIRECTORY);
    if !exists(&ready) {
        return Ok(());
    }
    // Renamed first so an interrupted apply is never repeated at the next start.
    let applying = root.join(APPLYING_DIRECTORY);
    let result = fs::rename(&ready, &applying)
        .map_err(|error| format!("could not open the staged import: {error}"))
        .and_then(|()| replace_with_import(data_dir, &applying));
    if let Err(error) = remove_directory_if_present(&root) {
        crate::application_log::failure("data_import_cleanup", error);
    }
    match result {
        Ok(kept_as) => {
            crate::application_log::state_change(
                "data_import",
                &format!("Applied the staged import (previous database: {kept_as})"),
            );
            Ok(())
        }
        Err(error) => {
            crate::application_log::failure("data_import", &error);
            Err(format!("The data import was not applied: {error}"))
        }
    }
}

fn require_absolute(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err("Data export paths must be absolute".to_owned())
    }
}

async fn run_blocking(
    task: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| format!("Data transfer task failed: {error}"))?
}

#[tauri::command]
pub async fn export_user_data(app: AppHandle, destination_path: String) -> Result<(), String> {
    let destination = require_absolute(&destination_path)?;
    let data_dir = database_recovery::data_directory(&app)?;
    let database = app.state::<DatabaseState>().shared_database()?;
    let version = app.package_info().version.to_string();
    run_blocking(move || export_data(&database, &data_dir, &destination, &version)).await
}

#[tauri::command]
pub async fn stage_user_data_import(app: AppHandle, source_path: String) -> Result<(), String> {
    let source = require_absolute(&source_path)?;
    let data_dir = database_recovery::data_directory(&app)?;
    run_blocking(move || {
        // The app restarts to apply the import, which would stop running games.
        crate::commands::require_idle_games(&app)?;
        stage_import(&data_dir, &source)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const GAME_ID: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";

    fn temporary_directory() -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("legio-data-transfer-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn manifest(schema_version: i64) -> Manifest {
        Manifest {
            format: FORMAT.to_owned(),
            format_version: FORMAT_VERSION,
            app_version: "0.0.0".to_owned(),
            schema_version,
            created_at: 1,
            platform: "linux".to_owned(),
        }
    }

    fn game_names(path: &Path) -> Vec<String> {
        let connection = Connection::open(path).unwrap();
        let mut statement = connection
            .prepare("SELECT automatic_name FROM games ORDER BY automatic_name")
            .unwrap();
        statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn database_with_game(data_dir: &Path, name: &str) -> Database {
        let database = Database::open(data_dir).unwrap();
        database
            .with_connection(|connection| {
                connection
                    .execute(
                        "INSERT INTO games (id, automatic_name) VALUES (?1, ?2)",
                        [uuid::Uuid::new_v4().to_string(), name.to_owned()],
                    )
                    .map(|_| ())
                    .map_err(crate::database::database_error)
            })
            .unwrap();
        database
    }

    fn write_zip(path: &Path, entries: &[(&str, Vec<u8>)]) {
        let mut writer = ZipWriter::new(File::create(path).unwrap());
        for (name, contents) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
    }

    fn database_bytes(directory: &Path, version: i64) -> Vec<u8> {
        let path = directory.join(format!("source-{version}.sqlite3"));
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(&format!(
                "CREATE TABLE marker (id INTEGER); PRAGMA user_version = {version};"
            ))
            .unwrap();
        drop(connection);
        fs::read(path).unwrap()
    }

    #[test]
    fn manifest_validation() {
        let json = |manifest: &Manifest| serde_json::to_vec(manifest).unwrap();
        assert_eq!(
            parse_manifest(&json(&manifest(SCHEMA_VERSION))).unwrap(),
            manifest(SCHEMA_VERSION)
        );
        let mut foreign = manifest(1);
        foreign.format = "other".to_owned();
        let mut newer_format = manifest(1);
        newer_format.format_version = FORMAT_VERSION + 1;
        for invalid in [
            json(&foreign),
            json(&newer_format),
            json(&manifest(SCHEMA_VERSION + 1)),
            json(&manifest(0)),
            b"{}".to_vec(),
            b"not json".to_vec(),
        ] {
            assert!(parse_manifest(&invalid).is_err());
        }
    }

    #[test]
    fn unsafe_entry_names_are_rejected() {
        assert_eq!(
            classify_entry(&format!("game-icons/{GAME_ID}.png")).unwrap(),
            Entry::File {
                directory: "game-icons",
                name: format!("{GAME_ID}.png"),
            }
        );
        for name in [
            "../legio.sqlite3",
            "/legio.sqlite3",
            "./legio.sqlite3",
            "game-icons/../../evil.png",
            &format!("game-icons/../{GAME_ID}.png"),
            &format!("game-icons/{GAME_ID}.png/x"),
            &format!("game-icons\\{GAME_ID}.png"),
            &format!("theme-backgrounds/{GAME_ID}.jpg"),
            &format!("game-shortcut-icons/{GAME_ID}.ico"),
            &format!("game-icons/{}.png", GAME_ID.to_uppercase()),
            "game-icons/.png",
            "settings.json",
        ] {
            assert!(classify_entry(name).is_err(), "{name}");
        }
    }

    #[test]
    fn traversal_archive_is_rejected_without_leftovers() {
        let directory = temporary_directory();
        let archive = directory.join("export.zip");
        write_zip(
            &archive,
            &[
                (MANIFEST_ENTRY, serde_json::to_vec(&manifest(1)).unwrap()),
                (DATABASE_FILE_NAME, database_bytes(&directory, 1)),
                ("../escaped.png", b"x".to_vec()),
            ],
        );
        let data_dir = directory.join("data");
        let error = stage_import(&data_dir, &archive).unwrap_err();
        assert!(error.contains("unexpected entry"), "{error}");
        assert!(!directory.join("escaped.png").exists());
        assert!(!data_dir.join(IMPORT_DIRECTORY).exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn newer_schema_is_rejected() {
        let directory = temporary_directory();
        let archive = directory.join("export.zip");
        // The manifest claims a supported version, but the database is newer.
        write_zip(
            &archive,
            &[
                (MANIFEST_ENTRY, serde_json::to_vec(&manifest(1)).unwrap()),
                (
                    DATABASE_FILE_NAME,
                    database_bytes(&directory, SCHEMA_VERSION + 1),
                ),
            ],
        );
        let data_dir = directory.join("data");
        let error = stage_import(&data_dir, &archive).unwrap_err();
        assert!(error.contains("unsupported schema version"), "{error}");
        assert!(!data_dir.join(IMPORT_DIRECTORY).exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_database_is_rejected() {
        let directory = temporary_directory();
        let archive = directory.join("export.zip");
        write_zip(
            &archive,
            &[(MANIFEST_ENTRY, serde_json::to_vec(&manifest(1)).unwrap())],
        );
        assert!(stage_import(&directory.join("data"), &archive).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn round_trip_replaces_the_data_and_keeps_the_previous_copy() {
        let directory = temporary_directory();
        let source_dir = directory.join("source");
        let target_dir = directory.join("target");
        let icon = format!("{GAME_ID}.png");

        let source = database_with_game(&source_dir, "Exported");
        source
            .with_connection(|connection| {
                connection
                    .execute(
                        "INSERT INTO link_checks (url, checked_at) VALUES ('https://cached', 1)",
                        [],
                    )
                    .map(|_| ())
                    .map_err(crate::database::database_error)
            })
            .unwrap();
        fs::create_dir_all(source_dir.join("game-icons")).unwrap();
        fs::write(source_dir.join("game-icons").join(&icon), b"exported icon").unwrap();
        fs::write(source_dir.join("game-icons").join("notes.txt"), b"skip").unwrap();
        let archive = directory.join("export.zip");
        export_data(&source, &source_dir, &archive, "1.2.3").unwrap();
        drop(source);

        let target = database_with_game(&target_dir, "Current");
        fs::create_dir_all(target_dir.join("game-banners")).unwrap();
        fs::write(target_dir.join("game-banners").join(&icon), b"old banner").unwrap();
        drop(target);

        stage_import(&target_dir, &archive).unwrap();
        // Staging alone leaves the current data untouched.
        assert_eq!(
            game_names(&target_dir.join(DATABASE_FILE_NAME)),
            ["Current"]
        );
        apply_staged_import(&target_dir).unwrap();

        assert_eq!(
            game_names(&target_dir.join(DATABASE_FILE_NAME)),
            ["Exported"]
        );
        let cached: i64 = Connection::open(target_dir.join(DATABASE_FILE_NAME))
            .unwrap()
            .query_row("SELECT COUNT(*) FROM link_checks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(cached, 0);
        assert_eq!(
            fs::read(target_dir.join("game-icons").join(&icon)).unwrap(),
            b"exported icon"
        );
        assert!(!target_dir.join("game-icons/notes.txt").exists());
        assert!(!target_dir.join("game-banners").exists());
        assert!(!target_dir.join(IMPORT_DIRECTORY).exists());

        let mut kept: Vec<String> = fs::read_dir(&target_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.starts_with("legio-replaced-"))
            .collect();
        kept.sort();
        let database = kept.iter().find(|name| name.ends_with(".sqlite3")).unwrap();
        assert_eq!(game_names(&target_dir.join(database)), ["Current"]);
        let files = kept
            .iter()
            .find(|name| name.starts_with("legio-replaced-files-"))
            .unwrap();
        assert_eq!(
            fs::read(target_dir.join(files).join("game-banners").join(&icon)).unwrap(),
            b"old banner"
        );

        // A second start has nothing left to apply.
        apply_staged_import(&target_dir).unwrap();
        assert_eq!(
            game_names(&target_dir.join(DATABASE_FILE_NAME)),
            ["Exported"]
        );
        Database::open(&target_dir).unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn damaged_staged_database_keeps_the_current_one() {
        let directory = temporary_directory();
        drop(database_with_game(&directory, "Current"));
        let ready = directory.join(IMPORT_DIRECTORY).join(READY_DIRECTORY);
        fs::create_dir_all(&ready).unwrap();
        fs::write(ready.join(DATABASE_FILE_NAME), b"garbage").unwrap();

        assert!(apply_staged_import(&directory).is_err());

        assert_eq!(game_names(&directory.join(DATABASE_FILE_NAME)), ["Current"]);
        assert!(!directory.join(IMPORT_DIRECTORY).exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
