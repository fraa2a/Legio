use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

use compress_tools::{ArchiveContents, ArchiveIteratorBuilder};
use sha2::{Digest, Sha256};
#[cfg(test)]
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const MAX_ENTRIES: usize = 100_000;
const MAX_EXPANDED_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_EXPANSION_RATIO: u64 = 200;

/// Verifies a queue-owned archive and extracts ordinary files into an empty staging path.
/// The caller owns the staging path and passes it to finalization only after success.
pub fn verify_and_stage(archive: &Path, expected_sha256: &str, stage: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(archive)
        .map_err(|error| format!("Cannot inspect download: {error}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Download is not a regular file".into());
    }
    let mut source =
        File::open(archive).map_err(|error| format!("Cannot open download: {error}"))?;
    let archive_bytes = source
        .metadata()
        .map_err(|error| format!("Cannot inspect download: {error}"))?
        .len();
    let mut hasher = Sha256::new();
    io::copy(&mut source, &mut hasher)
        .map_err(|error| format!("Cannot verify download: {error}"))?;
    let digest = format!("{:x}", hasher.finalize());
    if digest != expected_sha256 {
        drop(source);
        fs::remove_file(archive).map_err(|error| {
            format!("Download hash differs and corrupt file could not be removed: {error}")
        })?;
        return Err(
            "Download hash differs. The corrupt download was removed; retry the download".into(),
        );
    }

    source
        .seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut magic = [0u8; 8];
    source
        .read_exact(&mut magic)
        .map_err(|error| format!("Cannot inspect archive format: {error}"))?;
    let supported = magic.starts_with(b"PK\x03\x04")
        || magic.starts_with(b"PK\x05\x06")
        || magic.starts_with(b"7z\xbc\xaf\x27\x1c")
        || magic.starts_with(b"Rar!\x1a\x07\x00")
        || magic.starts_with(b"Rar!\x1a\x07\x01\x00");
    if !supported || is_multipart_name(archive) {
        return Err("Only single-file ZIP, 7z and RAR archives are supported".into());
    }
    if magic.starts_with(b"PK") {
        reject_split_zip(&mut source, archive_bytes)?;
    } else if magic.starts_with(b"Rar!") {
        reject_split_rar(&mut source, &magic)?;
    }

    source
        .seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let staging_parent = stage.parent().ok_or("Staging path has no parent")?;
    fs::create_dir_all(staging_parent)
        .map_err(|error| format!("Cannot create staging parent: {error}"))?;
    fs::create_dir(stage).map_err(|error| format!("Cannot create staging directory: {error}"))?;
    let result = extract(source, stage, archive_bytes);
    if let Err(error) = result {
        fs::remove_dir_all(stage).map_err(|cleanup| {
            format!(
                "{error}; staging cleanup failed at {}: {cleanup}",
                stage.display()
            )
        })?;
        return Err(error);
    }
    Ok(())
}

fn extract(source: File, stage: &Path, archive_bytes: u64) -> Result<(), String> {
    let entries = ArchiveIteratorBuilder::new(source)
        .mtree_format(false)
        .build()
        .map_err(|error| format!("Cannot read archive: {error}"))?;
    let mut current: Option<File> = None;
    #[cfg(unix)]
    let mut current_mode = 0u32;
    let mut seen = HashSet::new();
    let mut expanded = 0u64;
    let limit = MAX_EXPANDED_BYTES.min(archive_bytes.saturating_mul(MAX_EXPANSION_RATIO));

    for entry in entries {
        match entry {
            ArchiveContents::StartOfEntry(name, stat) => {
                if seen.len() >= MAX_ENTRIES {
                    return Err("Archive contains too many files. Choose a smaller archive".into());
                }
                let relative = safe_relative_path(&name)?;
                let key = relative.to_string_lossy().to_lowercase();
                if !seen.insert(key) {
                    return Err(format!("Archive repeats a path: {name}"));
                }
                #[cfg(windows)]
                let kind = u32::from(stat.st_mode) & 0o170000;
                #[cfg(not(windows))]
                let kind = stat.st_mode & 0o170000;
                let target = stage.join(&relative);
                if kind == 0o040000 {
                    fs::create_dir_all(&target)
                        .map_err(|error| format!("Cannot create directory {name}: {error}"))?;
                } else if kind == 0o100000 {
                    let declared = u64::try_from(stat.st_size).unwrap_or(0);
                    if expanded.saturating_add(declared) > limit {
                        return Err("Archive exceeds the extraction size limit".into());
                    }
                    let parent = target.parent().ok_or("Archive path has no parent")?;
                    fs::create_dir_all(parent)
                        .map_err(|error| format!("Cannot create directory for {name}: {error}"))?;
                    current = Some(
                        OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&target)
                            .map_err(|error| format!("Cannot create {name}: {error}"))?,
                    );
                    #[cfg(unix)]
                    {
                        current_mode = stat.st_mode & 0o777;
                    }
                } else {
                    return Err(format!(
                        "Archive contains an unsupported link or special file: {name}"
                    ));
                }
            }
            ArchiveContents::DataChunk(bytes) => {
                let file = current
                    .as_mut()
                    .ok_or("Archive contains data outside a file")?;
                expanded = expanded.saturating_add(bytes.len() as u64);
                if expanded > limit {
                    return Err("Archive exceeds the extraction size limit".into());
                }
                file.write_all(&bytes).map_err(|error| {
                    format!("Cannot write staged file: {error}. Free disk space and retry")
                })?;
            }
            ArchiveContents::EndOfEntry => {
                #[cfg(unix)]
                if let Some(file) = &current
                    && current_mode != 0
                {
                    file.set_permissions(fs::Permissions::from_mode(current_mode))
                        .map_err(|error| format!("Cannot set staged file permissions: {error}"))?;
                }
                current = None;
            }
            ArchiveContents::Err(error) => {
                return Err(format!(
                    "Archive cannot be extracted: {error}. Check that it is complete and unencrypted"
                ));
            }
        }
    }
    if seen.is_empty() {
        return Err("Archive contains no files".into());
    }
    Ok(())
}

fn safe_relative_path(name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || name.contains('\\') || name.contains(':') || name.contains('\0') {
        return Err(format!("Archive has an unsafe path: {name:?}"));
    }
    let name = name.strip_suffix('/').unwrap_or(name);
    let path = Path::new(name);
    if path
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("Archive has an unsafe path: {name:?}"));
    }
    if name.split('/').any(|part| {
        part.is_empty() || part == "." || part == ".." || part.ends_with('.') || part.ends_with(' ')
    }) {
        return Err(format!("Archive has an unsafe path: {name:?}"));
    }
    if name.split('/').any(|part| {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
    }) {
        return Err(format!("Archive has a reserved path: {name:?}"));
    }
    Ok(path.to_path_buf())
}

fn is_multipart_name(path: &Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    name.ends_with(".001")
        || name.rsplit_once(".part").is_some_and(|(_, tail)| {
            tail.strip_suffix(".rar").is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
        || name.rsplit_once('.').is_some_and(|(_, ext)| {
            ext.len() == 3
                && ext.starts_with('r')
                && ext[1..].bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn reject_split_zip(source: &mut File, size: u64) -> Result<(), String> {
    let tail_len = size.min(65_557) as usize;
    source
        .seek(SeekFrom::End(-(tail_len as i64)))
        .map_err(|error| error.to_string())?;
    let mut tail = vec![0; tail_len];
    source
        .read_exact(&mut tail)
        .map_err(|error| error.to_string())?;
    let eocd = tail
        .windows(4)
        .enumerate()
        .rev()
        .find_map(|(index, bytes)| {
            (bytes == b"PK\x05\x06"
                && index + 22 <= tail.len()
                && index + 22 + u16::from_le_bytes([tail[index + 20], tail[index + 21]]) as usize
                    == tail.len())
            .then_some(index)
        })
        .ok_or("ZIP central directory is missing or damaged")?;
    if tail[eocd + 4..eocd + 8].iter().any(|byte| *byte != 0)
        || tail[eocd + 8..eocd + 10] != tail[eocd + 10..eocd + 12]
    {
        return Err("Multipart ZIP archives are unsupported".into());
    }
    if eocd >= 20 && &tail[eocd - 20..eocd - 16] == b"PK\x06\x07" {
        let disk = u32::from_le_bytes(
            tail[eocd - 16..eocd - 12]
                .try_into()
                .map_err(|_| "Invalid ZIP64 locator")?,
        );
        let disks = u32::from_le_bytes(
            tail[eocd - 4..eocd]
                .try_into()
                .map_err(|_| "Invalid ZIP64 locator")?,
        );
        if disk != 0 || disks != 1 {
            return Err("Multipart ZIP archives are unsupported".into());
        }
    }
    Ok(())
}

fn reject_split_rar(source: &mut File, magic: &[u8; 8]) -> Result<(), String> {
    source
        .seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut header = [0u8; 64];
    let len = source
        .read(&mut header)
        .map_err(|error| error.to_string())?;
    if magic.starts_with(b"Rar!\x1a\x07\x00") {
        if len < 13 || header[9] != 0x73 {
            return Err("RAR main header is missing or damaged".into());
        }
        let flags = u16::from_le_bytes([header[10], header[11]]);
        if flags & 1 != 0 {
            return Err("Multipart RAR archives are unsupported".into());
        }
    } else {
        let mut rest = &header[12..len];
        let _header_size = rar_vint(&mut rest)?;
        if rar_vint(&mut rest)? != 1 {
            return Err("RAR main header is missing or encrypted".into());
        }
        let flags = rar_vint(&mut rest)?;
        if flags & 1 != 0 {
            let _extra_size = rar_vint(&mut rest)?;
        }
        if flags & 2 != 0 {
            let _data_size = rar_vint(&mut rest)?;
        }
        if rar_vint(&mut rest)? & 1 != 0 {
            return Err("Multipart RAR archives are unsupported".into());
        }
    }
    Ok(())
}

fn rar_vint(bytes: &mut &[u8]) -> Result<u64, String> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let (&byte, rest) = bytes.split_first().ok_or("RAR header is truncated")?;
        *bytes = rest;
        if shift == 63 && byte > 1 {
            return Err("RAR header is invalid".into());
        }
        value |= u64::from(byte & 0x7f)
            .checked_shl(shift)
            .ok_or("RAR header is invalid")?;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err("RAR header is invalid".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(bytes: &[u8], name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("legio-archive-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let archive = root.join(name);
        fs::write(&archive, bytes).unwrap();
        (root, archive)
    }

    fn run_fixture(bytes: &[u8], name: &str) -> (PathBuf, PathBuf, Result<PathBuf, String>) {
        let (root, archive) = fixture(bytes, name);
        let hash = format!("{:x}", Sha256::digest(bytes));
        let stage = root.join("staging");
        let result = verify_and_stage(&archive, &hash, &stage).map(|()| stage);
        (root, archive, result)
    }

    #[test]
    fn rejects_unsafe_paths() {
        for name in [
            "../escape",
            "/absolute",
            "a/../escape",
            "C:/drive",
            "a\\escape",
            "a//b",
            "a./b",
            "a /b",
        ] {
            assert!(safe_relative_path(name).is_err(), "{name}");
        }
        assert_eq!(
            safe_relative_path("Game/data.bin").unwrap(),
            PathBuf::from("Game/data.bin")
        );
        assert_eq!(safe_relative_path("Game/").unwrap(), PathBuf::from("Game"));
    }

    #[test]
    fn verified_zip_is_staged() {
        let (root, _, result) = run_fixture(
            include_bytes!("../test-fixtures/archive/safe.zip"),
            "safe.zip",
        );
        let stage = result.unwrap();
        assert_eq!(fs::read(stage.join("Game/data.bin")).unwrap(), b"hello");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verified_7z_is_staged() {
        let (root, _, result) = run_fixture(
            include_bytes!("../test-fixtures/archive/safe.7z"),
            "safe.7z",
        );
        let stage = result.unwrap();
        assert!(
            stage
                .join("src-tauri/test-fixtures/archive/safe.zip")
                .exists()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verified_rar5_is_staged() {
        let (root, _, result) = run_fixture(
            include_bytes!("../test-fixtures/archive/safe.rar"),
            "safe.rar",
        );
        let stage = result.unwrap();
        assert_eq!(
            fs::read(stage.join("helloworld.txt")).unwrap(),
            b"hello libarchive test suite!\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn executable_permission_is_preserved() {
        let (root, _, result) = run_fixture(
            include_bytes!("../test-fixtures/archive/executable.zip"),
            "executable.zip",
        );
        let stage = result.unwrap();
        let mode = fs::metadata(stage.join("Game/start.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bad_hash_removes_download_before_staging() {
        let (root, archive) = fixture(
            include_bytes!("../test-fixtures/archive/safe.zip"),
            "bad.zip",
        );
        let error = verify_and_stage(&archive, &"0".repeat(64), &root.join("staging")).unwrap_err();
        assert!(error.contains("corrupt download was removed"));
        assert!(!archive.exists());
        assert!(!root.join("staging").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn split_zip_is_rejected_before_staging() {
        let mut bytes = include_bytes!("../test-fixtures/archive/safe.zip").to_vec();
        let eocd = bytes
            .windows(4)
            .position(|part| part == b"PK\x05\x06")
            .unwrap();
        bytes[eocd + 4] = 1;
        let (root, _, result) = run_fixture(&bytes, "split.zip");
        assert!(result.unwrap_err().contains("Multipart ZIP"));
        assert!(!root.join("staging").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malicious_zip_entries_leave_no_staging_content() {
        for (name, bytes, message) in [
            (
                "traversal.zip",
                include_bytes!("../test-fixtures/archive/traversal.zip").as_slice(),
                "unsafe path",
            ),
            (
                "symlink.zip",
                include_bytes!("../test-fixtures/archive/symlink.zip").as_slice(),
                "unsupported link",
            ),
            (
                "bomb.zip",
                include_bytes!("../test-fixtures/archive/bomb.zip").as_slice(),
                "size limit",
            ),
            (
                "encrypted.7z",
                include_bytes!("../test-fixtures/archive/encrypted.7z").as_slice(),
                "unencrypted",
            ),
        ] {
            let (root, _, result) = run_fixture(bytes, name);
            assert!(result.unwrap_err().contains(message), "{name}");
            assert!(!root.join("staging").exists(), "{name}");
            fs::remove_dir_all(root).unwrap();
        }
    }
}
