//! Shared fixtures for unit tests.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Removes the directory when dropped. Declare it after any database in the
/// same struct so the database file is closed first.
pub(crate) struct TempDir(pub(crate) PathBuf);

impl TempDir {
    pub(crate) fn new() -> Self {
        let path = std::env::temp_dir().join(format!("legio-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl std::ops::Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
