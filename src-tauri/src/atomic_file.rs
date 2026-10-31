//! Replacing a file so that readers see either the old or the new content.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
};

use uuid::Uuid;

/// Writes `bytes` to `path` through a temporary file in the same folder. The
/// permissions of an existing file are kept. The parent folder must exist.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path has no folder"))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path has no file name"))?
        .to_string_lossy();
    let temporary = parent.join(format!(".{name}.{}.tmp", Uuid::new_v4()));
    let result = write_temporary(path, &temporary, bytes);
    if result.is_err()
        && let Err(error) = fs::remove_file(&temporary)
        && error.kind() != io::ErrorKind::NotFound
    {
        crate::application_log::failure("remove_temporary_file", error);
    }
    result?;
    crate::finalize_install::sync_directory(parent).map_err(io::Error::other)
}

fn write_temporary(path: &Path, temporary: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)?;
    match fs::metadata(path) {
        Ok(metadata) => file.set_permissions(metadata.permissions())?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!("legio-atomic-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn creates_and_replaces_files_without_leftovers() {
        let directory = temp_dir();
        let target = directory.join("data.txt");
        write_atomic(&target, b"first").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"first");
        write_atomic(&target, b"second").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"second");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn keeps_the_old_file_and_cleans_up_when_the_replace_fails() {
        let directory = temp_dir();
        // A folder cannot be replaced by a file.
        let target = directory.join("folder");
        fs::create_dir(&target).unwrap();
        assert!(write_atomic(&target, b"x").is_err());
        assert!(target.is_dir());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_paths_without_a_folder_or_name() {
        assert!(write_atomic(Path::new(""), b"x").is_err());
        assert!(write_atomic(Path::new("/"), b"x").is_err());
    }
}
