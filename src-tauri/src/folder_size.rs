//! On-disk size of a folder tree, measured without following links.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

/// Entries visited between two pauses, so a large install does not saturate the disk.
const BATCH: u32 = 1024;
const BATCH_PAUSE: Duration = Duration::from_millis(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Measurement {
    pub(crate) bytes: u64,
    /// Some entries could not be read, so `bytes` is a lower bound.
    pub(crate) partial: bool,
}

/// Allocated size where the stat call already reports it (Unix blocks), apparent
/// length otherwise (Windows), so the walk never needs a second call per file.
#[cfg(unix)]
fn file_bytes(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn file_bytes(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}

fn read_directory(path: &Path) -> io::Result<fs::ReadDir> {
    fs::read_dir(path)
}

/// Sums regular files under `root`. Links are skipped, never followed, and
/// unreadable entries mark the result partial. Returns `None` once `cancel` is set.
pub(crate) fn measure(root: &Path, cancel: &AtomicBool) -> Option<Measurement> {
    measure_with(root, cancel, BATCH_PAUSE, &read_directory)
}

fn measure_with(
    root: &Path,
    cancel: &AtomicBool,
    pause: Duration,
    read_dir: &dyn Fn(&Path) -> io::Result<fs::ReadDir>,
) -> Option<Measurement> {
    let mut result = Measurement {
        bytes: 0,
        partial: false,
    };
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.is_dir() => {}
        _ => {
            result.partial = true;
            return Some(result);
        }
    }
    let mut pending: Vec<PathBuf> = vec![root.to_path_buf()];
    let mut visited = 0_u32;
    while let Some(directory) = pending.pop() {
        let Ok(entries) = read_dir(&directory) else {
            result.partial = true;
            continue;
        };
        for entry in entries {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            visited += 1;
            if visited.is_multiple_of(BATCH) && !pause.is_zero() {
                std::thread::sleep(pause);
            }
            let Ok(entry) = entry else {
                result.partial = true;
                continue;
            };
            let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
                result.partial = true;
                continue;
            };
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                result.bytes = result.bytes.saturating_add(file_bytes(&metadata));
            }
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    fn run(root: &Path) -> Measurement {
        measure_with(
            root,
            &AtomicBool::new(false),
            Duration::ZERO,
            &read_directory,
        )
        .unwrap()
    }

    #[test]
    fn sums_nested_files() {
        let root = TempDir::new();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("one.bin"), vec![1; 10_000]).unwrap();
        fs::write(root.join("a/b/two.bin"), vec![2; 20_000]).unwrap();
        let measured = run(&root);
        assert!(!measured.partial);
        assert!(measured.bytes >= 30_000);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_followed() {
        let root = TempDir::new();
        let outside = TempDir::new();
        fs::write(outside.join("big.bin"), vec![0; 1_000_000]).unwrap();
        fs::write(root.join("small.bin"), b"x").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("dir-link")).unwrap();
        std::os::unix::fs::symlink(outside.join("big.bin"), root.join("file-link")).unwrap();
        let measured = run(&root);
        assert!(!measured.partial);
        assert!(measured.bytes < 100_000);
    }

    #[cfg(unix)]
    #[test]
    fn root_link_is_partial_and_empty() {
        let target = TempDir::new();
        fs::write(target.join("big.bin"), vec![0; 100_000]).unwrap();
        let parent = TempDir::new();
        std::os::unix::fs::symlink(target.path(), parent.join("link")).unwrap();
        let measured = run(&parent.join("link"));
        assert_eq!(
            measured,
            Measurement {
                bytes: 0,
                partial: true
            }
        );
    }

    #[test]
    fn unreadable_directory_marks_partial() {
        let root = TempDir::new();
        fs::create_dir_all(root.join("locked")).unwrap();
        fs::write(root.join("locked/hidden.bin"), vec![0; 50_000]).unwrap();
        fs::write(root.join("seen.bin"), vec![0; 10_000]).unwrap();
        let locked = root.join("locked");
        // Injected failure: permission bits do not stop a test runner that is root.
        let read = |path: &Path| {
            if path == locked {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                fs::read_dir(path)
            }
        };
        let measured = measure_with(&root, &AtomicBool::new(false), Duration::ZERO, &read).unwrap();
        assert!(measured.partial);
        assert!(measured.bytes >= 10_000 && measured.bytes < 50_000);
    }

    #[test]
    fn missing_root_is_partial() {
        let directory = TempDir::new();
        let root = directory.join("missing");
        assert!(run(&root).partial);
    }

    #[test]
    fn cancellation_stops_the_walk() {
        let root = TempDir::new();
        fs::write(root.join("file.bin"), b"data").unwrap();
        let cancel = AtomicBool::new(true);
        assert_eq!(
            measure_with(&root, &cancel, Duration::ZERO, &read_directory),
            None
        );
    }
}
