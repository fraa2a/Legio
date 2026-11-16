//! Bounded, validated filesystem reads shared by the launcher readers.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use super::SourceError;

pub const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_YAML_BYTES: u64 = 256 * 1024;
pub const MAX_DATABASE_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_ROWS: usize = 5000;

fn io_error(error: &std::io::Error) -> SourceError {
    if error.kind() == std::io::ErrorKind::NotFound {
        SourceError::Missing
    } else {
        SourceError::Unreadable(format!("{:?}", error.kind()))
    }
}

pub fn canonical_dir(path: &Path) -> Result<PathBuf, SourceError> {
    let path = dunce::canonicalize(path).map_err(|error| io_error(&error))?;
    if path.is_dir() {
        Ok(path)
    } else {
        Err(SourceError::Missing)
    }
}

/// Reads a UTF-8 file that must resolve inside `root`, so a symlink cannot
/// redirect the read elsewhere.
pub fn read_text_within(root: &Path, path: &Path, max_bytes: u64) -> Result<String, SourceError> {
    let root = dunce::canonicalize(root).map_err(|error| io_error(&error))?;
    let path = dunce::canonicalize(path).map_err(|error| io_error(&error))?;
    if !path.starts_with(&root) {
        return Err(SourceError::Unreadable(
            "file resolves outside the launcher folder".to_owned(),
        ));
    }
    let file = File::open(&path).map_err(|error| io_error(&error))?;
    let metadata = file.metadata().map_err(|error| io_error(&error))?;
    if !metadata.is_file() {
        return Err(SourceError::Unreadable("not a regular file".to_owned()));
    }
    if metadata.len() > max_bytes {
        return Err(SourceError::Unreadable("file is too large".to_owned()));
    }
    let mut bytes = Vec::new();
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error(&error))?;
    if bytes.len() as u64 > max_bytes {
        return Err(SourceError::Unreadable("file is too large".to_owned()));
    }
    String::from_utf8(bytes)
        .map_err(|_| SourceError::Unreadable("file is not valid UTF-8".to_owned()))
}

/// Parses an optional JSON file; a missing file is `Ok(None)`.
pub fn read_json_within(root: &Path, relative: &str) -> Result<Option<serde_json::Value>, String> {
    match read_text_within(root, &root.join(relative), MAX_JSON_BYTES) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|_| format!("{relative} is not valid JSON")),
        Err(SourceError::Missing) => Ok(None),
        Err(SourceError::Unreadable(reason)) => Err(format!("{relative}: {reason}")),
    }
}

fn is_clean(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

/// An absolute path that exists and is a directory, canonicalized.
pub fn existing_dir(value: &str) -> Option<PathBuf> {
    if !is_clean(value) || !Path::new(value).is_absolute() {
        return None;
    }
    dunce::canonicalize(value).ok().filter(|path| path.is_dir())
}

/// An absolute path that exists and is a regular file, canonicalized.
pub fn existing_file(value: &str) -> Option<PathBuf> {
    if !is_clean(value) || !Path::new(value).is_absolute() {
        return None;
    }
    dunce::canonicalize(value)
        .ok()
        .filter(|path| path.is_file())
}

/// A relative file inside `base`; rejects absolute paths, `..` and links leaving `base`.
pub fn file_within(base: &Path, relative: &str) -> Option<PathBuf> {
    let normalized = relative.replace('\\', "/");
    if !is_clean(&normalized) {
        return None;
    }
    let relative = Path::new(&normalized);
    if !relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let base = dunce::canonicalize(base).ok()?;
    let path = dunce::canonicalize(base.join(relative)).ok()?;
    (path.starts_with(&base) && path.is_file()).then_some(path)
}

/// A single path component usable as a file name: no separators or traversal.
pub fn is_safe_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub fn clean_title(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.chars().count() <= 200 && !value.chars().any(char::is_control))
        .then(|| value.to_owned())
}

/// Title fallback: the last component of the install directory.
pub fn directory_title(directory: &Path) -> Option<String> {
    directory
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(clean_title)
}

fn percent_encode_path(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    if !text.starts_with('/') {
        text.insert(0, '/');
    }
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/' | b'~' | b':') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// Opens a SQLite database read-only and immutable, so SQLite never creates
/// journal or WAL files next to another launcher's data.
pub fn open_database(path: &Path) -> Result<Connection, SourceError> {
    let path = dunce::canonicalize(path).map_err(|error| io_error(&error))?;
    let metadata = fs::metadata(&path).map_err(|error| io_error(&error))?;
    if !metadata.is_file() {
        return Err(SourceError::Missing);
    }
    if metadata.len() > MAX_DATABASE_BYTES {
        return Err(SourceError::Unreadable("database is too large".to_owned()));
    }
    let uri = format!("file:{}?mode=ro&immutable=1", percent_encode_path(&path));
    Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| SourceError::Unreadable("database could not be opened".to_owned()))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn temp() -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("legio-launcher-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn reads_within_root_and_rejects_oversized_and_non_utf8() {
        let root = temp();
        fs::write(root.join("a.json"), "{}").unwrap();
        assert_eq!(
            read_text_within(&root, &root.join("a.json"), 10).unwrap(),
            "{}"
        );
        assert_eq!(
            read_text_within(&root, &root.join("none"), 10),
            Err(SourceError::Missing)
        );
        fs::write(root.join("big"), vec![b'a'; 20]).unwrap();
        assert!(matches!(
            read_text_within(&root, &root.join("big"), 10),
            Err(SourceError::Unreadable(_))
        ));
        fs::write(root.join("bin"), [0xff, 0xfe]).unwrap();
        assert!(matches!(
            read_text_within(&root, &root.join("bin"), 10),
            Err(SourceError::Unreadable(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_leaving_root() {
        let root = temp();
        let outside = temp();
        fs::write(outside.join("secret"), "x").unwrap();
        std::os::unix::fs::symlink(outside.join("secret"), root.join("link")).unwrap();
        assert!(matches!(
            read_text_within(&root, &root.join("link"), 10),
            Err(SourceError::Unreadable(_))
        ));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn validates_paths() {
        let root = temp();
        fs::write(root.join("game.exe"), "").unwrap();
        assert!(existing_dir(root.to_str().unwrap()).is_some());
        assert!(existing_dir("relative").is_none());
        assert!(existing_dir(root.join("game.exe").to_str().unwrap()).is_none());
        assert!(existing_file(root.join("game.exe").to_str().unwrap()).is_some());
        assert!(file_within(&root, "game.exe").is_some());
        assert!(file_within(&root, "../game.exe").is_none());
        assert!(file_within(&root, "/etc/passwd").is_none());
        assert!(!is_safe_name("../x") && !is_safe_name("a/b") && is_safe_name("abc-1.2"));
        assert!(clean_title("a\nb").is_none() && clean_title("  ").is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn opens_database_read_only_without_side_files() {
        let parent = temp();
        let root = parent.join("with space#and%percent");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("db.sqlite");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE t (a INTEGER); INSERT INTO t VALUES (1);")
            .unwrap();
        drop(connection);
        let before = fs::read_dir(&root).unwrap().count();
        let connection = open_database(&path).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT a FROM t", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(connection.execute("INSERT INTO t VALUES (2)", []).is_err());
        drop(connection);
        assert_eq!(fs::read_dir(&root).unwrap().count(), before);
        assert_eq!(
            open_database(&root.join("none")).err(),
            Some(SourceError::Missing)
        );
        fs::write(root.join("garbage"), b"not a database at all, just text").unwrap();
        let garbage = open_database(&root.join("garbage")).unwrap();
        assert!(
            garbage
                .query_row("SELECT count(*) FROM sqlite_master", [], |row| row
                    .get::<_, i64>(0))
                .is_err()
        );
        drop(garbage);
        fs::remove_dir_all(parent).unwrap();
    }
}
