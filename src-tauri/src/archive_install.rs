use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

use compress_tools::{ArchiveContents, ArchiveIteratorBuilder};
use sha2::{Digest, Sha256};

use crate::archive_parts::PartScheme;
#[cfg(test)]
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const MAX_ENTRIES: usize = 100_000;
const MAX_EXPANSION_RATIO: u64 = 200;
// Kept free on the staging filesystem for filesystem overhead and the rest of the system.
pub(crate) const FREE_SPACE_MARGIN: u64 = 512 * 1024 * 1024;

/// Extraction behavior that callers opt into. The default keeps every entry name strict.
#[derive(Debug, Clone, Copy, Default)]
pub struct StageOptions {
    /// Reads `\` in entry names as `/`, as written by some Windows archivers. Every other
    /// path check still applies to the normalized name.
    pub normalize_backslashes: bool,
}

/// Checks an optional SHA-256 hash and extracts ordinary files into an empty staging path.
/// The caller owns the staging path and passes it to finalization only after success.
pub fn verify_and_stage(
    archive: &Path,
    expected_sha256: Option<&str>,
    stage: &Path,
) -> Result<(), String> {
    verify_and_stage_with(archive, expected_sha256, stage, StageOptions::default())
}

/// Same as [`verify_and_stage`] with explicit [`StageOptions`].
pub fn verify_and_stage_with(
    archive: &Path,
    expected_sha256: Option<&str>,
    stage: &Path,
    options: StageOptions,
) -> Result<(), String> {
    stage_with_free_space(archive, expected_sha256, stage, options, |path| {
        crate::installed_folder::disk_space(path).map(|(free, _)| free)
    })
}

/// Checks an optional SHA-256 hash of the parts joined in order and extracts them as one
/// archive, like [`verify_and_stage`]. A mismatch removes every part.
pub(crate) fn verify_and_stage_parts(
    parts: &[PathBuf],
    scheme: PartScheme,
    expected_sha256: Option<&str>,
    stage: &Path,
) -> Result<(), String> {
    stage_parts_with_free_space(parts, scheme, expected_sha256, stage, |path| {
        crate::installed_folder::disk_space(path).map(|(free, _)| free)
    })
}

fn open_regular(path: &Path) -> Result<(File, u64), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("Cannot inspect download: {error}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Download is not a regular file".into());
    }
    let source = File::open(path).map_err(|error| format!("Cannot open download: {error}"))?;
    let bytes = source
        .metadata()
        .map_err(|error| format!("Cannot inspect download: {error}"))?
        .len();
    Ok((source, bytes))
}

fn hash_matches(source: &mut impl Read, expected_sha256: &str) -> Result<bool, String> {
    let mut hasher = Sha256::new();
    crate::sha256::update_from(&mut hasher, source)
        .map_err(|error| format!("Cannot verify download: {error}"))?;
    Ok(crate::sha256::hex(&hasher.finalize()).eq_ignore_ascii_case(expected_sha256))
}

fn read_magic(source: &mut (impl Read + Seek)) -> Result<[u8; 8], String> {
    source
        .seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut magic = [0u8; 8];
    source
        .read_exact(&mut magic)
        .map_err(|error| format!("Cannot inspect archive format: {error}"))?;
    Ok(magic)
}

fn is_zip(magic: &[u8; 8]) -> bool {
    magic.starts_with(b"PK\x03\x04") || magic.starts_with(b"PK\x05\x06")
}

fn is_7z(magic: &[u8; 8]) -> bool {
    magic.starts_with(b"7z\xbc\xaf\x27\x1c")
}

fn is_rar(magic: &[u8; 8]) -> bool {
    magic.starts_with(b"Rar!\x1a\x07\x00") || magic.starts_with(b"Rar!\x1a\x07\x01\x00")
}

/// Reads free space through `free_space` so tests can inject it.
fn stage_with_free_space(
    archive: &Path,
    expected_sha256: Option<&str>,
    stage: &Path,
    options: StageOptions,
    free_space: impl FnOnce(&Path) -> Result<u64, String>,
) -> Result<(), String> {
    let (mut source, archive_bytes) = open_regular(archive)?;
    if let Some(expected_sha256) = expected_sha256
        && !hash_matches(&mut source, expected_sha256)?
    {
        drop(source);
        fs::remove_file(archive).map_err(|error| {
            format!("Download hash differs and corrupt file could not be removed: {error}")
        })?;
        return Err(
            "Download hash differs. The corrupt download was removed; retry the download".into(),
        );
    }

    let magic = read_magic(&mut source)?;
    let supported = is_zip(&magic) || is_7z(&magic) || is_rar(&magic);
    let name = archive.file_name().unwrap_or_default().to_string_lossy();
    if !supported || crate::archive_parts::looks_like_part(&name) {
        return Err("Only single-file ZIP, 7z and RAR archives are supported".into());
    }
    if magic.starts_with(b"PK") {
        reject_split_zip(&mut source, archive_bytes)?;
    } else if magic.starts_with(b"Rar!") {
        reject_split_rar(&mut source, &magic)?;
    }
    stage_source(source, &magic, archive_bytes, stage, options, free_space)
}

fn stage_parts_with_free_space(
    parts: &[PathBuf],
    scheme: PartScheme,
    expected_sha256: Option<&str>,
    stage: &Path,
    free_space: impl FnOnce(&Path) -> Result<u64, String>,
) -> Result<(), String> {
    let mut source = JoinedParts::open(parts)?;
    let archive_bytes = source.len();
    if let Some(expected_sha256) = expected_sha256
        && !hash_matches(&mut source, expected_sha256)?
    {
        drop(source);
        for part in parts {
            fs::remove_file(part).map_err(|error| {
                format!("Download hash differs and corrupt part could not be removed: {error}")
            })?;
        }
        return Err(
            "Download hash differs. The corrupt parts were removed; retry the download".into(),
        );
    }
    let magic = read_magic(&mut source)?;
    // RAR volumes carry the volume flag that single archives must not have, so only the
    // byte splits go through the single-archive split checks.
    let matches = match scheme {
        PartScheme::RarVolumes | PartScheme::RarLegacyVolumes => is_rar(&magic),
        PartScheme::SevenZipSplit => is_7z(&magic),
        PartScheme::ZipSplit => is_zip(&magic),
    };
    if !matches {
        return Err("The archive parts do not contain the format their names announce".into());
    }
    if scheme == PartScheme::ZipSplit {
        reject_split_zip(&mut source, archive_bytes)?;
    }
    stage_source(
        source,
        &magic,
        archive_bytes,
        stage,
        StageOptions::default(),
        free_space,
    )
}

/// Total size the ZIP central directory declares, read without extracting anything.
/// Only a plain end record without a comment is read, which caps the directory at
/// 65,534 entries. Other formats have no cheap equivalent.
fn declared_size(source: &mut (impl Read + Seek), magic: &[u8; 8]) -> Option<u64> {
    if !is_zip(magic) {
        return None;
    }
    let mut end = [0u8; 22];
    source.seek(SeekFrom::End(-22)).ok()?;
    source.read_exact(&mut end).ok()?;
    if !end.starts_with(b"PK\x05\x06") || end[10..12] == [0xff, 0xff] || end[20..22] != [0, 0] {
        return None;
    }
    let archive = zip::ZipArchive::new(source).ok()?;
    archive
        .decompressed_size()
        .and_then(|size| u64::try_from(size).ok())
}

/// Lower bound of what extracting `files` (one archive or the parts of one) writes:
/// the size a ZIP declares, otherwise the archive size.
pub(crate) fn minimum_extracted_size(files: &[PathBuf]) -> Result<u64, String> {
    let mut source = JoinedParts::open(files)?;
    // A damaged header is reported by extraction, which reads it again.
    let declared = read_magic(&mut source)
        .ok()
        .and_then(|magic| declared_size(&mut source, &magic));
    Ok(declared.unwrap_or_else(|| source.len()))
}

fn stage_source<R: Read + Seek>(
    mut source: R,
    magic: &[u8; 8],
    archive_bytes: u64,
    stage: &Path,
    options: StageOptions,
    free_space: impl FnOnce(&Path) -> Result<u64, String>,
) -> Result<(), String> {
    let staging_parent = stage.parent().ok_or("Staging path has no parent")?;
    fs::create_dir_all(staging_parent)
        .map_err(|error| format!("Cannot create staging parent: {error}"))?;
    // Unknown free space falls back to the ratio limit so unusual filesystems can still install.
    let free_bytes = free_space(staging_parent)
        .inspect_err(|error| crate::application_log::failure("extraction_disk_space", error))
        .ok();
    // A declared size that cannot fit fails before anything is written.
    if let (Some(declared), Some(free)) = (declared_size(&mut source, magic), free_bytes)
        && declared > free.saturating_sub(FREE_SPACE_MARGIN)
    {
        return Err(crate::download_space::extraction_shortfall(
            declared,
            free.saturating_sub(FREE_SPACE_MARGIN),
        ));
    }
    source
        .seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let limit = ExtractionLimit::new(archive_bytes, free_bytes);
    fs::create_dir(stage).map_err(|error| format!("Cannot create staging directory: {error}"))?;
    let result = extract(source, stage, limit, options);
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

/// Upper bound on bytes written while extracting, and which bound it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExtractionLimit {
    bytes: u64,
    by_free_space: bool,
}

impl ExtractionLimit {
    fn new(archive_bytes: u64, free_bytes: Option<u64>) -> Self {
        let ratio = archive_bytes.saturating_mul(MAX_EXPANSION_RATIO);
        match free_bytes.map(|free| free.saturating_sub(FREE_SPACE_MARGIN)) {
            Some(space) if space < ratio => Self {
                bytes: space,
                by_free_space: true,
            },
            _ => Self {
                bytes: ratio,
                by_free_space: false,
            },
        }
    }

    fn exceeded(self) -> String {
        if self.by_free_space {
            "Not enough free disk space to extract the archive. Free disk space and retry".into()
        } else {
            "Archive exceeds the extraction size limit".into()
        }
    }
}

fn extract<R: Read + Seek>(
    source: R,
    stage: &Path,
    limit: ExtractionLimit,
    options: StageOptions,
) -> Result<(), String> {
    let entries = ArchiveIteratorBuilder::new(source)
        .mtree_format(false)
        .decoder(decode_entry_name)
        .build()
        .map_err(|error| format!("Cannot read archive: {error}"))?;
    let mut current: Option<File> = None;
    #[cfg(unix)]
    let mut current_mode = 0u32;
    let mut seen = HashSet::new();
    let mut expanded = 0u64;

    for entry in entries {
        match entry {
            ArchiveContents::StartOfEntry(name, stat) => {
                if seen.len() >= MAX_ENTRIES {
                    return Err("Archive contains too many files. Choose a smaller archive".into());
                }
                let name = if options.normalize_backslashes {
                    name.replace('\\', "/")
                } else {
                    name
                };
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
                    if expanded.saturating_add(declared) > limit.bytes {
                        return Err(limit.exceeded());
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
                if expanded > limit.bytes {
                    return Err(limit.exceeded());
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

/// CP437 characters for bytes 0x80..=0xFF.
const CP437_HIGH: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}',
    '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}',
    '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}',
    '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}',
    '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}',
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}',
    '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}',
    '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}',
    '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

/// Entry names are UTF-8 when valid. Otherwise they are legacy ZIP names in CP437, which maps
/// every byte to one distinct character, so no two names collapse into one.
fn decode_entry_name(bytes: &[u8]) -> compress_tools::Result<String> {
    Ok(match std::str::from_utf8(bytes) {
        Ok(name) => name.to_owned(),
        Err(_) => bytes
            .iter()
            .map(|&byte| match byte {
                0x80..=0xFF => CP437_HIGH[usize::from(byte - 0x80)],
                _ => char::from(byte),
            })
            .collect(),
    })
}

pub(crate) fn safe_relative_path(name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || name.contains('\\') || name.contains(':') || name.contains('\0') {
        return Err(format!("Archive has an unsafe path: '{name}'"));
    }
    let name = name.strip_suffix('/').unwrap_or(name);
    let path = Path::new(name);
    if path
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("Archive has an unsafe path: '{name}'"));
    }
    if name.split('/').any(|part| {
        part.is_empty() || part == "." || part == ".." || part.ends_with('.') || part.ends_with(' ')
    }) {
        return Err(format!("Archive has an unsafe path: '{name}'"));
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
        return Err(format!("Archive has a reserved path: '{name}'"));
    }
    Ok(path.to_path_buf())
}

/// The volumes of a multipart archive read as one seekable stream. libarchive reads
/// RAR volumes and byte splits this way exactly as it reads their concatenation.
struct JoinedParts {
    files: Vec<File>,
    /// Offset in the joined stream where each part ends.
    ends: Vec<u64>,
    position: u64,
}

impl JoinedParts {
    fn open(parts: &[PathBuf]) -> Result<Self, String> {
        if parts.is_empty() {
            return Err("The multipart archive has no parts".into());
        }
        let mut files = Vec::with_capacity(parts.len());
        let mut ends = Vec::with_capacity(parts.len());
        let mut end = 0u64;
        for part in parts {
            let (file, bytes) = open_regular(part)?;
            end = end
                .checked_add(bytes)
                .ok_or("The multipart archive is too large")?;
            files.push(file);
            ends.push(end);
        }
        Ok(Self {
            files,
            ends,
            position: 0,
        })
    }

    fn len(&self) -> u64 {
        self.ends.last().copied().unwrap_or(0)
    }
}

impl Read for JoinedParts {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let Some(index) = self.ends.iter().position(|end| self.position < *end) else {
            return Ok(0);
        };
        let start = index
            .checked_sub(1)
            .map_or(0, |previous| self.ends[previous]);
        let available = usize::try_from(self.ends[index] - self.position).unwrap_or(usize::MAX);
        let length = buffer.len().min(available);
        let file = &mut self.files[index];
        file.seek(SeekFrom::Start(self.position - start))?;
        let read = file.read(&mut buffer[..length])?;
        if read == 0 && length > 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "An archive part changed while it was read",
            ));
        }
        self.position += read as u64;
        Ok(read)
    }
}

impl Seek for JoinedParts {
    fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
        let (base, offset) = match target {
            SeekFrom::Start(position) => {
                self.position = position;
                return Ok(position);
            }
            SeekFrom::End(offset) => (self.len(), offset),
            SeekFrom::Current(offset) => (self.position, offset),
        };
        self.position = base.checked_add_signed(offset).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid seek in archive parts")
        })?;
        Ok(self.position)
    }
}

fn reject_split_zip(source: &mut (impl Read + Seek), size: u64) -> Result<(), String> {
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

fn reject_split_rar(source: &mut (impl Read + Seek), magic: &[u8; 8]) -> Result<(), String> {
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
        if len < 13 {
            return Err("RAR main header is missing or damaged".into());
        }
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
        let hash = crate::sha256::hex_digest(bytes);
        let stage = root.join("staging");
        let result = verify_and_stage(&archive, Some(&hash), &stage).map(|()| stage);
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
    fn decodes_utf8_and_cp437_names() {
        assert_eq!(
            decode_entry_name("caf\u{e9}/a".as_bytes()).unwrap(),
            "caf\u{e9}/a"
        );
        assert_eq!(
            decode_entry_name(b"caf\x82 \xe1\xb0\xff").unwrap(),
            "caf\u{e9} \u{df}\u{2591}\u{a0}"
        );
    }

    #[test]
    fn legacy_cp437_names_are_staged() {
        use zip::{ZipWriter, write::SimpleFileOptions};
        let mut writer = ZipWriter::new(io::Cursor::new(Vec::new()));
        writer
            .start_file("Game/cafX.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"data").unwrap();
        let mut bytes = writer.finish().unwrap().into_inner();
        // The zip crate sets the UTF-8 flag for non-ASCII names, so patch the placeholder in
        // both the local header and the central directory.
        let mut patched = 0;
        for index in 0..bytes.len() - 4 {
            if &bytes[index..index + 4] == b"cafX" {
                bytes[index + 3] = 0x82;
                patched += 1;
            }
        }
        assert_eq!(patched, 2);
        let (root, archive) = fixture(&bytes, "legacy.zip");
        let stage = root.join("staging");
        verify_and_stage(&archive, None, &stage).unwrap();
        assert_eq!(fs::read(stage.join("Game/caf\u{e9}.txt")).unwrap(), b"data");
        fs::remove_dir_all(root).unwrap();
    }

    fn backslash_zip() -> Vec<u8> {
        use zip::{ZipWriter, write::SimpleFileOptions};
        let mut writer = ZipWriter::new(io::Cursor::new(Vec::new()));
        for name in ["Pack\\plugins\\mod.dll", "Pack\\readme.txt"] {
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"data").unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn backslash_names_are_normalized_on_request() {
        let (root, archive) = fixture(&backslash_zip(), "backslash.zip");
        let stage = root.join("staging");
        let options = StageOptions {
            normalize_backslashes: true,
        };
        verify_and_stage_with(&archive, None, &stage, options).unwrap();
        assert_eq!(
            fs::read(stage.join("Pack/plugins/mod.dll")).unwrap(),
            b"data"
        );
        assert_eq!(fs::read(stage.join("Pack/readme.txt")).unwrap(), b"data");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn normalized_names_still_reject_traversal() {
        use zip::{ZipWriter, write::SimpleFileOptions};
        let mut writer = ZipWriter::new(io::Cursor::new(Vec::new()));
        writer
            .start_file("..\\escape.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"x").unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let (root, archive) = fixture(&bytes, "escape.zip");
        let options = StageOptions {
            normalize_backslashes: true,
        };
        let stage = root.join("staging");
        assert!(verify_and_stage_with(&archive, None, &stage, options).is_err());
        assert!(!root.join("escape.txt").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn truncated_rar5_returns_an_error_without_panicking() {
        for length in 8..12 {
            let mut bytes = b"Rar!\x1a\x07\x01\x00".to_vec();
            bytes.resize(length, 0);
            let (root, _, result) = run_fixture(&bytes, "broken.rar");
            assert!(result.is_err());
            fs::remove_dir_all(root).unwrap();
        }
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
        assert_eq!(
            fs::read(stage.join("src-tauri/test-fixtures/archive/safe.zip")).unwrap(),
            include_bytes!("../test-fixtures/archive/safe.zip")
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
        let error =
            verify_and_stage(&archive, Some(&"0".repeat(64)), &root.join("staging")).unwrap_err();
        assert!(error.contains("corrupt download was removed"));
        assert!(!archive.exists());
        assert!(!root.join("staging").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uppercase_expected_hash_matches() {
        let bytes = include_bytes!("../test-fixtures/archive/safe.zip");
        let (root, archive) = fixture(bytes, "archive.zip");
        let hash = crate::sha256::hex_digest(bytes).to_uppercase();
        verify_and_stage(&archive, Some(&hash), &root.join("staging")).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn skipping_sha256_keeps_archive_path_and_size_protections() {
        for (bytes, allowed) in [
            (
                include_bytes!("../test-fixtures/archive/safe.zip").as_slice(),
                true,
            ),
            (
                include_bytes!("../test-fixtures/archive/traversal.zip").as_slice(),
                false,
            ),
            (
                include_bytes!("../test-fixtures/archive/symlink.zip").as_slice(),
                false,
            ),
            (
                include_bytes!("../test-fixtures/archive/bomb.zip").as_slice(),
                false,
            ),
        ] {
            let (root, archive) = fixture(bytes, "archive.zip");
            let stage = root.join("staging");
            assert_eq!(verify_and_stage(&archive, None, &stage).is_ok(), allowed);
            if allowed {
                assert_eq!(fs::read(stage.join("Game/data.bin")).unwrap(), b"hello");
            } else {
                assert!(!stage.exists());
            }
            fs::remove_dir_all(root).unwrap();
        }
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
    fn empty_zip_is_rejected_before_staging() {
        use zip::ZipWriter;
        let bytes = ZipWriter::new(io::Cursor::new(Vec::new()))
            .finish()
            .unwrap()
            .into_inner();
        let (root, _, result) = run_fixture(&bytes, "empty.zip");
        assert!(result.unwrap_err().contains("no files"));
        assert!(!root.join("staging").exists());
        fs::remove_dir_all(root).unwrap();
    }

    fn stage_safe_zip(
        free_space: impl FnOnce(&Path) -> Result<u64, String>,
    ) -> (PathBuf, Result<(), String>) {
        let (root, archive) = fixture(
            include_bytes!("../test-fixtures/archive/safe.zip"),
            "safe.zip",
        );
        let stage = root.join("staging");
        let result =
            stage_with_free_space(&archive, None, &stage, StageOptions::default(), free_space);
        (root, result)
    }

    #[test]
    fn archive_within_free_space_is_staged() {
        // safe.zip holds one 5-byte file.
        let (root, result) = stage_safe_zip(|_| Ok(FREE_SPACE_MARGIN + 5));
        result.unwrap();
        assert_eq!(
            fs::read(root.join("staging/Game/data.bin")).unwrap(),
            b"hello"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archive_beyond_free_space_leaves_no_staging() {
        for free in [FREE_SPACE_MARGIN + 4, 0] {
            let (root, result) = stage_safe_zip(|_| Ok(free));
            assert!(
                result.unwrap_err().contains("Not enough free disk space"),
                "{free}"
            );
            assert!(!root.join("staging").exists(), "{free}");
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn unknown_free_space_falls_back_to_ratio_limit() {
        let (root, result) = stage_safe_zip(|_| Err("unsupported filesystem".into()));
        result.unwrap();
        fs::remove_dir_all(root).unwrap();

        let (root, archive) = fixture(
            include_bytes!("../test-fixtures/archive/bomb.zip"),
            "bomb.zip",
        );
        let stage = root.join("staging");
        let error = stage_with_free_space(&archive, None, &stage, StageOptions::default(), |_| {
            Err("unsupported filesystem".into())
        })
        .unwrap_err();
        assert!(error.contains("size limit"));
        assert!(!stage.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extraction_limit_takes_the_lower_bound() {
        let ratio = ExtractionLimit::new(10, Some(u64::MAX));
        assert_eq!(ratio.bytes, 10 * MAX_EXPANSION_RATIO);
        assert!(!ratio.by_free_space);
        let space = ExtractionLimit::new(10, Some(FREE_SPACE_MARGIN + 7));
        assert_eq!(space.bytes, 7);
        assert!(space.by_free_space);
        assert_eq!(ExtractionLimit::new(10, None), ratio);
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

    /// Writes each part beside the others, as the download queue does.
    fn parts_fixture(parts: &[(&str, &[u8])]) -> (PathBuf, Vec<PathBuf>) {
        let root = std::env::temp_dir().join(format!("legio-parts-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let paths = parts
            .iter()
            .map(|(name, bytes)| {
                let path = root.join(name);
                fs::write(&path, bytes).unwrap();
                path
            })
            .collect();
        (root, paths)
    }

    fn split(bytes: &[u8], pieces: usize) -> Vec<Vec<u8>> {
        bytes
            .chunks(bytes.len().div_ceil(pieces))
            .map(<[u8]>::to_vec)
            .collect()
    }

    fn crc(path: &Path) -> u32 {
        let mut crc = flate2::Crc::new();
        crc.update(&fs::read(path).unwrap());
        crc.sum()
    }

    fn assert_volume_fixture(stage: &Path) {
        assert_eq!(crc(&stage.join("vols/bigfile.txt")), 0x509a_d74c);
        assert_eq!(crc(&stage.join("vols/smallfile.txt")), 0xd08a_1f86);
    }

    #[test]
    fn joined_parts_read_and_seek_across_boundaries() {
        let (root, paths) = parts_fixture(&[("a", b"abc"), ("b", b""), ("c", b"defg")]);
        let mut joined = JoinedParts::open(&paths).unwrap();
        assert_eq!(joined.len(), 7);
        let mut all = Vec::new();
        joined.read_to_end(&mut all).unwrap();
        assert_eq!(all, b"abcdefg");
        let mut two = [0u8; 2];
        joined.seek(SeekFrom::Start(2)).unwrap();
        joined.read_exact(&mut two).unwrap();
        assert_eq!(&two, b"cd");
        assert_eq!(joined.seek(SeekFrom::End(-1)).unwrap(), 6);
        joined.read_exact(&mut two[..1]).unwrap();
        assert_eq!(&two[..1], b"g");
        assert_eq!(joined.seek(SeekFrom::Current(-4)).unwrap(), 3);
        joined.read_exact(&mut two).unwrap();
        assert_eq!(&two, b"de");
        assert!(joined.seek(SeekFrom::Current(-10)).is_err());
        joined.seek(SeekFrom::Start(20)).unwrap();
        assert_eq!(joined.read(&mut two).unwrap(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_rar4_volumes_are_staged() {
        let (root, parts) = parts_fixture(&[
            (
                "1.archive",
                include_bytes!("../test-fixtures/archive/rar3-old.rar"),
            ),
            (
                "2.archive",
                include_bytes!("../test-fixtures/archive/rar3-old.r00"),
            ),
            (
                "3.archive",
                include_bytes!("../test-fixtures/archive/rar3-old.r01"),
            ),
        ]);
        let stage = root.join("staging");
        verify_and_stage_parts(&parts, PartScheme::RarLegacyVolumes, None, &stage).unwrap();
        assert_volume_fixture(&stage);
        fs::remove_dir_all(root).unwrap();
    }

    fn rar5_volumes() -> [(&'static str, &'static [u8]); 3] {
        [
            (
                "1.archive",
                include_bytes!("../test-fixtures/archive/rar5-vols.part1.rar"),
            ),
            (
                "2.archive",
                include_bytes!("../test-fixtures/archive/rar5-vols.part2.rar"),
            ),
            (
                "3.archive",
                include_bytes!("../test-fixtures/archive/rar5-vols.part3.rar"),
            ),
        ]
    }

    #[test]
    fn rar5_volumes_are_staged_and_hashed_as_one_stream() {
        let volumes = rar5_volumes();
        let joined: Vec<u8> = volumes
            .iter()
            .flat_map(|(_, bytes)| bytes.to_vec())
            .collect();
        let (root, parts) = parts_fixture(&volumes);
        let stage = root.join("staging");
        let hash = crate::sha256::hex_digest(&joined);
        verify_and_stage_parts(&parts, PartScheme::RarVolumes, Some(&hash), &stage).unwrap();
        assert_volume_fixture(&stage);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_missing_last_volume_fails_without_staging() {
        let (root, parts) = parts_fixture(&rar5_volumes()[..2]);
        let stage = root.join("staging");
        let error =
            verify_and_stage_parts(&parts, PartScheme::RarVolumes, None, &stage).unwrap_err();
        assert!(error.contains("complete"), "{error}");
        assert!(!stage.exists());
        assert!(parts.iter().all(|part| part.is_file()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn volumes_in_the_wrong_order_fail_without_staging() {
        let volumes = rar5_volumes();
        let (root, parts) = parts_fixture(&[volumes[0], volumes[2], volumes[1]]);
        let stage = root.join("staging");
        assert!(verify_and_stage_parts(&parts, PartScheme::RarVolumes, None, &stage).is_err());
        assert!(!stage.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hash_mismatch_removes_every_part() {
        let (root, parts) = parts_fixture(&rar5_volumes());
        let stage = root.join("staging");
        let error = verify_and_stage_parts(
            &parts,
            PartScheme::RarVolumes,
            Some(&"0".repeat(64)),
            &stage,
        )
        .unwrap_err();
        assert!(error.contains("corrupt parts were removed"));
        assert!(parts.iter().all(|part| !part.exists()));
        assert!(!stage.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn byte_splits_of_7z_and_zip_are_staged() {
        for (scheme, bytes, file, expected) in [
            (
                PartScheme::SevenZipSplit,
                include_bytes!("../test-fixtures/archive/safe.7z").as_slice(),
                "src-tauri/test-fixtures/archive/safe.zip",
                include_bytes!("../test-fixtures/archive/safe.zip").as_slice(),
            ),
            (
                PartScheme::ZipSplit,
                include_bytes!("../test-fixtures/archive/safe.zip").as_slice(),
                "Game/data.bin",
                b"hello".as_slice(),
            ),
        ] {
            let pieces = split(bytes, 3);
            let named: Vec<(String, &[u8])> = pieces
                .iter()
                .enumerate()
                .map(|(index, piece)| (format!("{}.archive", index + 1), piece.as_slice()))
                .collect();
            let named: Vec<(&str, &[u8])> = named
                .iter()
                .map(|(name, piece)| (name.as_str(), *piece))
                .collect();
            let (root, parts) = parts_fixture(&named);
            let stage = root.join("staging");
            verify_and_stage_parts(&parts, scheme, None, &stage).unwrap();
            assert_eq!(fs::read(stage.join(file)).unwrap(), expected, "{scheme:?}");
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn parts_must_contain_the_announced_format() {
        let pieces = split(include_bytes!("../test-fixtures/archive/safe.zip"), 2);
        let (root, parts) = parts_fixture(&[("1.archive", &pieces[0]), ("2.archive", &pieces[1])]);
        for scheme in [PartScheme::SevenZipSplit, PartScheme::RarVolumes] {
            let stage = root.join("staging");
            let error = verify_and_stage_parts(&parts, scheme, None, &stage).unwrap_err();
            assert!(error.contains("format"), "{scheme:?}: {error}");
            assert!(!stage.exists());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn split_archives_keep_every_extraction_safety_check() {
        for (bytes, message) in [
            (
                include_bytes!("../test-fixtures/archive/traversal.zip").as_slice(),
                "unsafe path",
            ),
            (
                include_bytes!("../test-fixtures/archive/symlink.zip").as_slice(),
                "unsupported link",
            ),
            (
                include_bytes!("../test-fixtures/archive/bomb.zip").as_slice(),
                "size limit",
            ),
        ] {
            let pieces = split(bytes, 2);
            let (root, parts) =
                parts_fixture(&[("1.archive", &pieces[0]), ("2.archive", &pieces[1])]);
            let stage = root.join("staging");
            let error =
                stage_parts_with_free_space(&parts, PartScheme::ZipSplit, None, &stage, |_| {
                    Err("unsupported filesystem".into())
                })
                .unwrap_err();
            assert!(error.contains(message), "{message}: {error}");
            assert!(!stage.exists(), "{message}");
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn declared_zip_size_beyond_free_space_fails_before_staging() {
        let (root, archive) = fixture(
            include_bytes!("../test-fixtures/archive/bomb.zip"),
            "bomb.zip",
        );
        let stage = root.join("staging");
        // The bomb declares 1 MiB, more than the free space, so nothing is extracted.
        let error = stage_with_free_space(&archive, None, &stage, StageOptions::default(), |_| {
            Ok(FREE_SPACE_MARGIN + 1024)
        })
        .unwrap_err();
        assert!(error.contains("Not enough free disk space"), "{error}");
        assert!(error.contains("needs"), "{error}");
        assert!(!stage.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
