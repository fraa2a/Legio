use super::{SCHEMA_VERSION, database_error, now_milliseconds};
use rusqlite::Connection;
use serde::Serialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const BACKUP_DIRECTORY: &str = "database-backups";
const KEPT_BACKUPS: usize = 3;
const BACKUP_PREFIX: &str = "legio-v";
const BACKUP_EXTENSION: &str = ".sqlite3";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DatabaseBackup {
    pub(crate) file_name: String,
    pub(crate) schema_version: i64,
    pub(crate) created_at: i64,
}

pub(crate) fn backup_directory(data_dir: &Path) -> PathBuf {
    data_dir.join(BACKUP_DIRECTORY)
}

// Only names written by this module are accepted, so a listed name never escapes the folder.
fn parse_backup_name(file_name: &str) -> Option<DatabaseBackup> {
    let (version, created_at) = file_name
        .strip_prefix(BACKUP_PREFIX)?
        .strip_suffix(BACKUP_EXTENSION)?
        .split_once('-')?;
    let is_number =
        |value: &str| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit());
    if !is_number(version) || !is_number(created_at) {
        return None;
    }
    Some(DatabaseBackup {
        file_name: file_name.to_owned(),
        schema_version: version.parse().ok()?,
        created_at: created_at.parse().ok()?,
    })
}

/// Lists the database backups, newest first.
pub(crate) fn list_backups(data_dir: &Path) -> Result<Vec<DatabaseBackup>, String> {
    let entries = match fs::read_dir(backup_directory(data_dir)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "could not read the database backup folder: {error}"
            ));
        }
    };
    let mut backups = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("could not read the database backup folder: {error}"))?;
        let is_file = entry
            .file_type()
            .map_err(|error| format!("could not inspect a database backup: {error}"))?
            .is_file();
        if let (true, Some(backup)) = (
            is_file,
            entry.file_name().to_str().and_then(parse_backup_name),
        ) {
            backups.push(backup);
        }
    }
    backups.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.file_name.cmp(&left.file_name))
    });
    Ok(backups)
}

/// Copies the database into the backup folder when a migration is pending on an existing schema.
pub(super) fn backup_before_migration(
    connection: &Connection,
    data_dir: &Path,
) -> Result<Option<PathBuf>, String> {
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(database_error)?;
    // Version 0 is a database without Legio data; newer versions are rejected by the migration.
    if version <= 0 || version >= SCHEMA_VERSION {
        return Ok(None);
    }
    let directory = backup_directory(data_dir);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("could not create the database backup folder: {error}"))?;
    let file_name = format!(
        "{BACKUP_PREFIX}{version}-{}{BACKUP_EXTENSION}",
        now_milliseconds()?
    );
    let path = directory.join(&file_name);
    let temporary = directory.join(format!(".{file_name}.tmp"));
    if let Err(error) = write_backup(connection, &temporary, &path, &directory) {
        if let Err(cleanup) = fs::remove_file(&temporary)
            && cleanup.kind() != io::ErrorKind::NotFound
        {
            return Err(format!("{error}; backup cleanup failed: {cleanup}"));
        }
        return Err(error);
    }
    crate::application_log::state_change(
        "database_backup",
        &format!("Backed up schema version {version} as {file_name}"),
    );
    if let Err(error) = prune_backups(data_dir) {
        crate::application_log::failure("database_backup_retention", error);
    }
    Ok(Some(path))
}

fn write_backup(
    connection: &Connection,
    temporary: &Path,
    path: &Path,
    directory: &Path,
) -> Result<(), String> {
    let target = temporary
        .to_str()
        .ok_or_else(|| "database backup path is not valid UTF-8".to_owned())?;
    // VACUUM INTO writes a consistent snapshot that includes committed WAL content.
    connection
        .execute("VACUUM INTO ?1", [target])
        .map_err(|error| format!("could not back up the local database: {error}"))?;
    sync_file(temporary).map_err(|error| format!("could not save the database backup: {error}"))?;
    fs::rename(temporary, path)
        .map_err(|error| format!("could not save the database backup: {error}"))?;
    crate::finalize_install::sync_directory(directory)
}

/// Checks that a copied backup is an intact database this build can open.
pub(crate) fn verify_database_copy(path: &Path) -> Result<(), String> {
    let connection = Connection::open(path)
        .map_err(|error| format!("could not open the database backup: {error}"))?;
    let integrity: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| format!("could not check the database backup: {error}"))?;
    if integrity != "ok" {
        return Err(format!("the database backup is damaged: {integrity}"));
    }
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(database_error)?;
    if !(1..=SCHEMA_VERSION).contains(&version) {
        return Err(format!(
            "the database backup has unsupported schema version {version}"
        ));
    }
    connection
        .close()
        .map_err(|(_, error)| format!("could not close the database backup: {error}"))
}

pub(crate) fn sync_file(path: &Path) -> io::Result<()> {
    // Windows only flushes handles opened for writing.
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
}

fn prune_backups(data_dir: &Path) -> Result<(), String> {
    let directory = backup_directory(data_dir);
    for backup in list_backups(data_dir)?.iter().skip(KEPT_BACKUPS) {
        fs::remove_file(directory.join(&backup.file_name)).map_err(|error| {
            format!(
                "could not remove old database backup {}: {error}",
                backup.file_name
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "legio-database-backup-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    // A fully migrated schema rolled back to version 34 leaves migration 35 pending.
    fn pending_database(data_dir: &Path) -> Connection {
        let connection = Connection::open(data_dir.join("legio.sqlite3")).unwrap();
        connection
            .execute_batch("PRAGMA journal_mode = WAL;")
            .unwrap();
        super::super::migrations::migrate(&connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO games (id, automatic_name) VALUES ('kept', 'Portal');
                 ALTER TABLE downloads DROP COLUMN library_id;
                 PRAGMA user_version = 34;",
            )
            .unwrap();
        connection
    }

    fn user_version(connection: &Connection) -> i64 {
        connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn pending_migration_writes_a_readable_backup() {
        let directory = temporary_directory();
        let connection = pending_database(&directory);
        let path = backup_before_migration(&connection, &directory)
            .unwrap()
            .unwrap();
        let backups = list_backups(&directory).unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].schema_version, 34);
        assert_eq!(
            path,
            backup_directory(&directory).join(&backups[0].file_name)
        );
        let copy = Connection::open(&path).unwrap();
        assert_eq!(user_version(&copy), 34);
        let name: String = copy
            .query_row(
                "SELECT automatic_name FROM games WHERE id = 'kept'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "Portal");
        drop((copy, connection));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn current_or_new_database_is_not_backed_up() {
        let directory = temporary_directory();
        let connection = Connection::open(directory.join("legio.sqlite3")).unwrap();
        assert_eq!(
            backup_before_migration(&connection, &directory).unwrap(),
            None
        );
        super::super::migrations::migrate(&connection).unwrap();
        assert_eq!(
            backup_before_migration(&connection, &directory).unwrap(),
            None
        );
        assert!(!backup_directory(&directory).exists());
        drop(connection);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn only_the_newest_backups_are_kept() {
        let directory = temporary_directory();
        let backups = backup_directory(&directory);
        fs::create_dir_all(&backups).unwrap();
        for created_at in [1, 2, 3, 4] {
            fs::write(
                backups.join(format!("legio-v30-{created_at}.sqlite3")),
                b"old",
            )
            .unwrap();
        }
        fs::write(backups.join("notes.txt"), b"unrelated").unwrap();
        let connection = pending_database(&directory);
        backup_before_migration(&connection, &directory).unwrap();
        let kept = list_backups(&directory).unwrap();
        assert_eq!(kept.len(), KEPT_BACKUPS);
        assert_eq!(kept[0].schema_version, 34);
        assert_eq!(
            kept[1..]
                .iter()
                .map(|backup| backup.created_at)
                .collect::<Vec<_>>(),
            [4, 3]
        );
        assert!(backups.join("notes.txt").exists());
        drop(connection);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn foreign_names_are_not_backups() {
        for name in [
            "legio-v34-1.sqlite3-wal",
            "legio-v-1.sqlite3",
            "legio-v34-.sqlite3",
            "legio-v3a-1.sqlite3",
            "legio-v34-1-2.sqlite3",
            ".legio-v34-1.sqlite3.tmp",
        ] {
            assert_eq!(parse_backup_name(name), None, "{name}");
        }
        assert_eq!(
            parse_backup_name("legio-v34-17.sqlite3"),
            Some(DatabaseBackup {
                file_name: "legio-v34-17.sqlite3".to_owned(),
                schema_version: 34,
                created_at: 17,
            })
        );
    }
}
