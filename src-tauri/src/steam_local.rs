use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::{env, fmt, fs, io};

use serde::Serialize;

pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_OBJECT_DEPTH: usize = 32;
const MAX_APPINFO_BYTES: usize = 256 * 1024 * 1024;
const MAX_APPINFO_KEYS: usize = 100_000;
const MAX_APPINFO_RECORDS: usize = 100_000;

/// Metadata declared by an app manifest, not proof that its files are installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SteamAppManifest {
    pub app_id: u32,
    pub name: String,
    /// A single directory component; filesystem containment still needs checking.
    pub install_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ManifestDiagnostic {
    InputTooLarge,
    InvalidUtf8,
    Malformed { offset: usize },
    NestingTooDeep,
    MissingField(&'static str),
    InvalidField(&'static str),
    DuplicateField(&'static str),
}

impl fmt::Display for ManifestDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge => write!(
                formatter,
                "Steam manifest exceeds {MAX_MANIFEST_BYTES} bytes"
            ),
            Self::InvalidUtf8 => formatter.write_str("Steam manifest is not valid UTF-8"),
            Self::Malformed { offset } => {
                write!(formatter, "Malformed Steam KeyValues at byte {offset}")
            }
            Self::NestingTooDeep => {
                formatter.write_str("Steam manifest object nesting is too deep")
            }
            Self::MissingField(field) => write!(formatter, "Steam manifest is missing {field}"),
            Self::InvalidField(field) => write!(formatter, "Steam manifest has invalid {field}"),
            Self::DuplicateField(field) => write!(formatter, "Steam manifest repeats {field}"),
        }
    }
}

impl std::error::Error for ManifestDiagnostic {}

/// Parses one quoted KeyValues AppState object, including unknown nested fields.
/// Input size and nesting are bounded before allocating records or recursing.
pub fn parse_app_manifest(bytes: &[u8]) -> Result<SteamAppManifest, ManifestDiagnostic> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(ManifestDiagnostic::InputTooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ManifestDiagnostic::InvalidUtf8)?;
    let mut parser = Parser {
        text,
        offset: usize::from(text.starts_with('\u{feff}')) * 3,
    };
    if !matches!(parser.next()?, Some(Token::Text(root)) if root.eq_ignore_ascii_case("AppState"))
        || !matches!(parser.next()?, Some(Token::Open))
    {
        return Err(parser.malformed());
    }
    let mut fields = Fields::default();
    parser.object(1, &mut fields)?;
    if parser.next()?.is_some() {
        return Err(parser.malformed());
    }
    let app_id = fields
        .app_id
        .ok_or(ManifestDiagnostic::MissingField("appid"))?;
    let app_id = if !app_id.is_empty() && app_id.bytes().all(|byte| byte.is_ascii_digit()) {
        app_id.parse::<u32>().ok().filter(|id| *id > 0)
    } else {
        None
    }
    .ok_or(ManifestDiagnostic::InvalidField("appid"))?;
    let name = fields
        .name
        .ok_or(ManifestDiagnostic::MissingField("name"))?;
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        return Err(ManifestDiagnostic::InvalidField("name"));
    }
    let install_dir = fields
        .install_dir
        .ok_or(ManifestDiagnostic::MissingField("installdir"))?;
    if install_dir.trim().is_empty()
        || matches!(install_dir.as_ref(), "." | "..")
        || install_dir
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | ':'))
    {
        return Err(ManifestDiagnostic::InvalidField("installdir"));
    }
    Ok(SteamAppManifest {
        app_id,
        name: name.into_owned(),
        install_dir: install_dir.into_owned(),
    })
}

#[derive(Default)]
struct Fields<'a> {
    app_id: Option<Cow<'a, str>>,
    name: Option<Cow<'a, str>>,
    install_dir: Option<Cow<'a, str>>,
}

enum Token<'a> {
    Text(Cow<'a, str>),
    Open,
    Close,
}

struct Parser<'a> {
    text: &'a str,
    offset: usize,
}

impl<'a> Parser<'a> {
    fn malformed(&self) -> ManifestDiagnostic {
        ManifestDiagnostic::Malformed {
            offset: self.offset,
        }
    }

    fn next(&mut self) -> Result<Option<Token<'a>>, ManifestDiagnostic> {
        let bytes = self.text.as_bytes();
        loop {
            while bytes.get(self.offset).is_some_and(u8::is_ascii_whitespace) {
                self.offset += 1;
            }
            if bytes.get(self.offset..self.offset + 2) != Some(b"//") {
                break;
            }
            while bytes.get(self.offset).is_some_and(|byte| *byte != b'\n') {
                self.offset += 1;
            }
        }
        let Some(byte) = bytes.get(self.offset) else {
            return Ok(None);
        };
        self.offset += 1;
        match byte {
            b'{' => Ok(Some(Token::Open)),
            b'}' => Ok(Some(Token::Close)),
            b'"' => self.quoted().map(|text| Some(Token::Text(text))),
            _ => Err(self.malformed()),
        }
    }

    fn quoted(&mut self) -> Result<Cow<'a, str>, ManifestDiagnostic> {
        let bytes = self.text.as_bytes();
        let mut start = self.offset;
        let mut decoded: Option<String> = None;
        while let Some(&byte) = bytes.get(self.offset) {
            match byte {
                b'"' => {
                    let end = self.offset;
                    self.offset += 1;
                    return Ok(match decoded {
                        Some(mut text) => {
                            text.push_str(&self.text[start..end]);
                            Cow::Owned(text)
                        }
                        None => Cow::Borrowed(&self.text[start..end]),
                    });
                }
                b'\\' => {
                    let escaped = match bytes.get(self.offset + 1) {
                        Some(b'"') => Some('"'),
                        Some(b'\\') => Some('\\'),
                        Some(b'n') => Some('\n'),
                        Some(b'r') => Some('\r'),
                        Some(b't') => Some('\t'),
                        None => return Err(self.malformed()),
                        _ => None,
                    };
                    if let Some(escaped) = escaped {
                        let text = decoded.get_or_insert_with(String::new);
                        text.push_str(&self.text[start..self.offset]);
                        text.push(escaped);
                        self.offset += 2;
                        start = self.offset;
                        continue;
                    }
                }
                0..=31 => return Err(self.malformed()),
                _ => {}
            }
            self.offset += 1;
        }
        Err(self.malformed())
    }

    fn object(&mut self, depth: usize, fields: &mut Fields<'a>) -> Result<(), ManifestDiagnostic> {
        if depth > MAX_OBJECT_DEPTH {
            return Err(ManifestDiagnostic::NestingTooDeep);
        }
        loop {
            let key = match self.next()? {
                Some(Token::Close) => return Ok(()),
                Some(Token::Text(key)) => key,
                _ => return Err(self.malformed()),
            };
            let target = if depth == 1 && key.eq_ignore_ascii_case("appid") {
                Some((&mut fields.app_id, "appid"))
            } else if depth == 1 && key.eq_ignore_ascii_case("name") {
                Some((&mut fields.name, "name"))
            } else if depth == 1 && key.eq_ignore_ascii_case("installdir") {
                Some((&mut fields.install_dir, "installdir"))
            } else {
                None
            };
            match (self.next()?, target) {
                (Some(Token::Text(value)), Some((slot, field))) => {
                    if slot.is_some() {
                        return Err(ManifestDiagnostic::DuplicateField(field));
                    }
                    *slot = Some(value);
                }
                (Some(Token::Text(_)), None) => {}
                (Some(Token::Open), None) => self.object(depth + 1, fields)?,
                (Some(Token::Open), Some((_, field))) => {
                    return Err(ManifestDiagnostic::InvalidField(field));
                }
                _ => return Err(self.malformed()),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSteamGame {
    pub app_id: u32,
    pub name: String,
    pub install_dir: String,
    pub install_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamScan {
    pub games: Vec<InstalledSteamGame>,
    pub diagnostics: Vec<String>,
    #[serde(skip)]
    pub(crate) excluded_non_games: Vec<InstalledSteamGame>,
}

fn parse_library_folders(bytes: &[u8]) -> Result<Vec<PathBuf>, ManifestDiagnostic> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(ManifestDiagnostic::InputTooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ManifestDiagnostic::InvalidUtf8)?;
    let mut parser = Parser {
        text,
        offset: usize::from(text.starts_with('\u{feff}')) * 3,
    };
    if !matches!(parser.next()?, Some(Token::Text(root)) if root.eq_ignore_ascii_case("libraryfolders"))
        || !matches!(parser.next()?, Some(Token::Open))
    {
        return Err(parser.malformed());
    }
    let mut paths = Vec::new();
    let mut indices = HashSet::new();
    loop {
        let key = match parser.next()? {
            Some(Token::Close) => break,
            Some(Token::Text(key)) => key,
            _ => return Err(parser.malformed()),
        };
        let is_library = !key.is_empty() && key.bytes().all(|byte| byte.is_ascii_digit());
        if is_library {
            let index = key
                .parse::<u32>()
                .map_err(|_| ManifestDiagnostic::InvalidField("library index"))?;
            if !indices.insert(index) {
                return Err(ManifestDiagnostic::DuplicateField("library index"));
            }
        }
        let path = match (is_library, parser.next()?) {
            (true, Some(Token::Text(path))) => Some(path),
            (true, Some(Token::Open)) => Some(parser.library_path()?),
            (false, Some(Token::Open)) => {
                parser.object(2, &mut Fields::default())?;
                None
            }
            (false, Some(Token::Text(_))) => None,
            _ => return Err(parser.malformed()),
        };
        if let Some(path) = path {
            let directory = Path::new(path.as_ref());
            if !directory.is_absolute()
                || path.chars().any(char::is_control)
                || directory
                    .components()
                    .any(|part| part == Component::ParentDir)
            {
                return Err(ManifestDiagnostic::InvalidField("library path"));
            }
            paths.push(directory.to_path_buf());
        }
    }
    if parser.next()?.is_some() {
        return Err(parser.malformed());
    }
    Ok(paths)
}

impl<'a> Parser<'a> {
    fn library_path(&mut self) -> Result<Cow<'a, str>, ManifestDiagnostic> {
        let mut path = None;
        loop {
            let key = match self.next()? {
                Some(Token::Close) => break,
                Some(Token::Text(key)) => key,
                _ => return Err(self.malformed()),
            };
            match (key.eq_ignore_ascii_case("path"), self.next()?) {
                (true, Some(Token::Text(value))) => {
                    if path.replace(value).is_some() {
                        return Err(ManifestDiagnostic::DuplicateField("library path"));
                    }
                }
                (true, Some(Token::Open)) => {
                    return Err(ManifestDiagnostic::InvalidField("library path"));
                }
                (false, Some(Token::Open)) => self.object(3, &mut Fields::default())?,
                (false, Some(Token::Text(_))) => {}
                _ => return Err(self.malformed()),
            }
        }
        path.ok_or(ManifestDiagnostic::MissingField("library path"))
    }
}

// Reject special files and limit allocation before parsing untrusted metadata.
fn read_metadata(path: &Path) -> io::Result<Vec<u8>> {
    read_bounded_metadata(path, MAX_MANIFEST_BYTES)
}

fn read_bounded_metadata(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    Ok(bytes)
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let value = u32::from_le_bytes(bytes.get(*offset..end)?.try_into().ok()?);
    *offset = end;
    Some(value)
}

fn read_u64(bytes: &[u8], offset: &mut usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    let value = u64::from_le_bytes(bytes.get(*offset..end)?.try_into().ok()?);
    *offset = end;
    Some(value)
}

fn read_cstr<'a>(bytes: &'a [u8], offset: &mut usize) -> Option<&'a [u8]> {
    let start = *offset;
    let length = bytes.get(start..)?.iter().position(|byte| *byte == 0)?;
    *offset = start.checked_add(length)?.checked_add(1)?;
    bytes.get(start..start + length)
}

fn appinfo_object(
    bytes: &[u8],
    offset: &mut usize,
    keys: &[&[u8]],
    depth: usize,
    in_common: bool,
    app_type: &mut Option<bool>,
) -> Option<()> {
    if depth > MAX_OBJECT_DEPTH {
        return None;
    }
    loop {
        let kind = *bytes.get(*offset)?;
        *offset += 1;
        if kind == 8 {
            return Some(());
        }
        let key_index = usize::try_from(read_u32(bytes, offset)?).ok()?;
        let key = *keys.get(key_index)?;
        match kind {
            0 => appinfo_object(
                bytes,
                offset,
                keys,
                depth + 1,
                depth == 0 && key == b"common",
                app_type,
            )?,
            1 => {
                let value = read_cstr(bytes, offset)?;
                if in_common
                    && key == b"type"
                    && app_type
                        .replace(value.eq_ignore_ascii_case(b"game"))
                        .is_some()
                {
                    return None;
                }
            }
            2..=4 | 6 => *offset = offset.checked_add(4)?,
            5 => loop {
                let end = offset.checked_add(2)?;
                let pair = bytes.get(*offset..end)?;
                *offset = end;
                if pair == [0, 0] {
                    break;
                }
            },
            7 | 10 => *offset = offset.checked_add(8)?,
            _ => return None,
        }
        if *offset > bytes.len() {
            return None;
        }
    }
}

fn parse_appinfo_types(bytes: &[u8]) -> Option<HashMap<u32, bool>> {
    if bytes.len() > MAX_APPINFO_BYTES {
        return None;
    }
    let mut offset = 0;
    if read_u32(bytes, &mut offset)? != 0x0756_4429 || read_u32(bytes, &mut offset)? != 1 {
        return None;
    }
    let table_offset = usize::try_from(read_u64(bytes, &mut offset)?).ok()?;
    if table_offset <= offset || table_offset >= bytes.len() {
        return None;
    }
    let mut table_cursor = table_offset;
    let key_count = usize::try_from(read_u32(bytes, &mut table_cursor)?).ok()?;
    if key_count > MAX_APPINFO_KEYS {
        return None;
    }
    let mut keys = Vec::with_capacity(key_count);
    for _ in 0..key_count {
        keys.push(read_cstr(bytes, &mut table_cursor)?);
    }
    if table_cursor != bytes.len() {
        return None;
    }
    let mut types = HashMap::new();
    let mut seen_ids = HashSet::new();
    while offset < table_offset {
        let app_id = read_u32(bytes, &mut offset)?;
        if app_id == 0 {
            return (offset == table_offset).then_some(types);
        }
        if seen_ids.len() >= MAX_APPINFO_RECORDS || !seen_ids.insert(app_id) {
            return None;
        }
        let size = usize::try_from(read_u32(bytes, &mut offset)?).ok()?;
        if size < 60 {
            return None;
        }
        let end = offset.checked_add(size)?;
        if end > table_offset {
            return None;
        }
        let data = bytes.get(offset + 60..end)?;
        let mut cursor = 0;
        if *data.get(cursor)? != 0 {
            return None;
        }
        cursor += 1;
        let root = usize::try_from(read_u32(data, &mut cursor)?).ok()?;
        if *keys.get(root)? != b"appinfo" {
            return None;
        }
        let mut app_type = None;
        appinfo_object(data, &mut cursor, &keys, 0, false, &mut app_type)?;
        if data.get(cursor) != Some(&8) || cursor + 1 != data.len() {
            return None;
        }
        if let Some(is_game) = app_type {
            types.insert(app_id, is_game);
        }
        offset = end;
    }
    None
}

#[cfg(test)]
pub(crate) fn appinfo_fixture(entries: &[(u32, &str)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0x0756_4429u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    for &(app_id, kind) in entries {
        let mut value = vec![0];
        value.extend_from_slice(&0u32.to_le_bytes());
        value.push(0);
        value.extend_from_slice(&1u32.to_le_bytes());
        value.push(1);
        value.extend_from_slice(&2u32.to_le_bytes());
        value.extend_from_slice(kind.as_bytes());
        value.extend_from_slice(&[0, 8, 8, 8]);
        bytes.extend_from_slice(&app_id.to_le_bytes());
        bytes.extend_from_slice(&(60u32 + value.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 60]);
        bytes.extend_from_slice(&value);
    }
    bytes.extend_from_slice(&0u32.to_le_bytes());
    let table_offset = bytes.len() as u64;
    bytes[8..16].copy_from_slice(&table_offset.to_le_bytes());
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(b"appinfo\0common\0type\0");
    bytes
}

#[cfg(any(windows, test))]
fn windows_installation_roots(
    program_files_x86: Option<std::ffi::OsString>,
    program_files: Option<std::ffi::OsString>,
) -> Vec<PathBuf> {
    [program_files_x86, program_files]
        .into_iter()
        .flatten()
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .map(|path| path.join("Steam"))
        .collect()
}

#[cfg(windows)]
pub fn scan_default_installations() -> SteamScan {
    let roots = windows_installation_roots(
        env::var_os("ProgramFiles(x86)"),
        env::var_os("ProgramFiles"),
    );
    if roots.is_empty() {
        return SteamScan {
            games: Vec::new(),
            diagnostics: vec!["Program Files directories are unavailable".to_owned()],
            excluded_non_games: Vec::new(),
        };
    }
    scan_installations(roots)
}

#[cfg(not(windows))]
pub fn scan_default_installations() -> SteamScan {
    let Some(home) = env::var_os("HOME") else {
        return SteamScan {
            games: Vec::new(),
            diagnostics: vec!["home directory is unavailable".to_owned()],
            excluded_non_games: Vec::new(),
        };
    };
    scan_installations([
        PathBuf::from(&home).join(".local/share/Steam"),
        PathBuf::from(&home).join(".steam/steam"),
        PathBuf::from(&home).join(".steam/root"),
        PathBuf::from(&home).join(".steam/debian-installation"),
        PathBuf::from(&home).join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ])
}

pub(crate) fn scan_installations(roots: impl IntoIterator<Item = PathBuf>) -> SteamScan {
    let mut scan = SteamScan {
        games: Vec::new(),
        diagnostics: Vec::new(),
        excluded_non_games: Vec::new(),
    };
    let mut libraries = Vec::new();
    let mut seen_roots = HashSet::new();
    for root in roots {
        let steamapps = match fs::canonicalize(root.join("steamapps")) {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                scan.diagnostics.push(format!(
                    "could not access a Steam installation: {:?}",
                    error.kind()
                ));
                continue;
            }
        };
        if !seen_roots.insert(steamapps.clone()) {
            continue;
        }
        match read_metadata(&steamapps.join("libraryfolders.vdf")) {
            Ok(bytes) => match parse_library_folders(&bytes) {
                Ok(paths) => libraries.extend(paths.into_iter().map(|path| path.join("steamapps"))),
                Err(error) => scan
                    .diagnostics
                    .push(format!("ignored Steam library metadata: {error}")),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => scan.diagnostics.push(format!(
                "could not read Steam library metadata: {:?}",
                error.kind()
            )),
        }
        libraries.push(steamapps);
    }
    let mut seen_libraries = HashSet::new();
    let mut resolved_libraries = Vec::new();
    let mut app_types = HashMap::new();
    for library in libraries {
        let steamapps = match fs::canonicalize(library) {
            Ok(path) => path,
            Err(error) => {
                scan.diagnostics.push(format!(
                    "could not access a configured Steam library: {:?}",
                    error.kind()
                ));
                continue;
            }
        };
        if !seen_libraries.insert(steamapps.clone()) {
            continue;
        }
        if let Some(root) = steamapps.parent() {
            let path = root.join("appcache/appinfo.vdf");
            match read_bounded_metadata(&path, MAX_APPINFO_BYTES) {
                Ok(bytes) => match parse_appinfo_types(&bytes) {
                    Some(types) => {
                        for (id, is_game) in types {
                            app_types
                                .entry(id)
                                .and_modify(|known| *known &= is_game)
                                .or_insert(is_game);
                        }
                    }
                    None => scan
                        .diagnostics
                        .push("ignored unsupported or invalid Steam app type metadata".to_owned()),
                },
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => scan.diagnostics.push(format!(
                    "could not read Steam app type metadata: {:?}",
                    error.kind()
                )),
            }
        }
        resolved_libraries.push(steamapps);
    }
    let mut seen_games = HashSet::new();
    for steamapps in resolved_libraries {
        let entries = match fs::read_dir(&steamapps) {
            Ok(entries) => entries,
            Err(error) => {
                scan.diagnostics.push(format!(
                    "could not scan a Steam library: {:?}",
                    error.kind()
                ));
                continue;
            }
        };
        let common = fs::canonicalize(steamapps.join("common")).ok();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    scan.diagnostics.push(format!(
                        "could not inspect a Steam library entry: {:?}",
                        error.kind()
                    ));
                    continue;
                }
            };
            let filename = entry.file_name();
            let Some(filename) = filename.to_str() else {
                continue;
            };
            let Some(id) = filename
                .strip_prefix("appmanifest_")
                .and_then(|name| name.strip_suffix(".acf"))
            else {
                continue;
            };
            let bytes = match read_metadata(&entry.path()) {
                Ok(bytes) => bytes,
                Err(error) => {
                    scan.diagnostics.push(format!(
                        "could not read a Steam app manifest: {:?}",
                        error.kind()
                    ));
                    continue;
                }
            };
            let game = match parse_app_manifest(&bytes) {
                Ok(game) => game,
                Err(error) => {
                    scan.diagnostics
                        .push(format!("ignored a Steam app manifest: {error}"));
                    continue;
                }
            };
            if id.parse::<u32>().ok() != Some(game.app_id)
                || !id.bytes().all(|byte| byte.is_ascii_digit())
            {
                scan.diagnostics
                    .push("ignored a Steam manifest with a mismatched filename App ID".to_owned());
                continue;
            }
            let installed = common.as_ref().and_then(|common| {
                fs::canonicalize(common.join(&game.install_dir))
                    .ok()
                    .filter(|directory| {
                        directory.starts_with(common) && directory != common && directory.is_dir()
                    })
            });
            let Some(installed) = installed else {
                scan.diagnostics.push("ignored a Steam manifest whose install directory is unavailable or outside its library".to_owned());
                continue;
            };
            let Some(install_path) = installed.to_str() else {
                scan.diagnostics
                    .push("ignored a Steam installation whose path is not valid UTF-8".to_owned());
                continue;
            };
            if seen_games.insert(game.app_id) {
                let installed = InstalledSteamGame {
                    app_id: game.app_id,
                    name: game.name,
                    install_dir: game.install_dir,
                    install_path: install_path.to_owned(),
                };
                match app_types.get(&installed.app_id) {
                    Some(true) => scan.games.push(installed),
                    Some(false) => scan.excluded_non_games.push(installed),
                    None => scan
                        .diagnostics
                        .push("ignored a Steam app without verified game type".to_owned()),
                }
            }
        }
    }
    scan.games.sort_by_key(|game| game.app_id);
    scan.excluded_non_games.sort_by_key(|app| app.app_id);
    scan
}
#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = env::temp_dir().join(format!("legio-steam-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn library(&self, name: &str, app_id: u32) -> PathBuf {
            let path = self.0.join(name);
            let steamapps = path.join("steamapps");
            fs::create_dir_all(steamapps.join("common/Game")).unwrap();
            fs::create_dir_all(path.join("appcache")).unwrap();
            fs::write(
                path.join("appcache/appinfo.vdf"),
                appinfo_fixture(&[(app_id, "Game")]),
            )
            .unwrap();
            fs::write(
                steamapps.join(format!("appmanifest_{app_id}.acf")),
                format!(r#""AppState" {{ "appid" "{app_id}" "name" "Game" "installdir" "Game" }}"#),
            )
            .unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn windows_default_roots_scan_both_program_files_directories() {
        let fixture = Fixture::new();
        fixture.library("Program Files (x86)/Steam", 1);
        fixture.library("Program Files/Steam", 2);
        let roots = windows_installation_roots(
            Some(fixture.0.join("Program Files (x86)").into_os_string()),
            Some(fixture.0.join("Program Files").into_os_string()),
        );
        let scan = scan_installations(roots);
        assert_eq!(
            scan.games
                .iter()
                .map(|game| game.app_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(scan.diagnostics.is_empty());
    }

    #[test]
    fn windows_default_roots_ignore_relative_paths() {
        assert!(windows_installation_roots(Some("relative".into()), None).is_empty());
    }

    #[test]
    fn appinfo_type_excludes_tools_without_matching_names() {
        let fixture = Fixture::new();
        let root = fixture.library("steam", 1);
        let steamapps = root.join("steamapps");
        fs::create_dir_all(steamapps.join("common/Compatibility")).unwrap();
        fs::write(
            steamapps.join("appmanifest_2.acf"),
            r#""AppState" { "appid" "2" "name" "A tool with an arbitrary name" "installdir" "Compatibility" }"#,
        )
        .unwrap();
        fs::write(
            root.join("appcache/appinfo.vdf"),
            appinfo_fixture(&[(1, "game"), (2, "Tool")]),
        )
        .unwrap();
        let scan = scan_installations([root]);
        assert_eq!(
            scan.games
                .iter()
                .map(|game| game.app_id)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(
            scan.excluded_non_games
                .iter()
                .map(|app| app.app_id)
                .collect::<Vec<_>>(),
            vec![2]
        );
        assert!(scan.diagnostics.is_empty());
    }

    #[test]
    fn missing_or_invalid_type_cache_does_not_guess_games() {
        let fixture = Fixture::new();
        let root = fixture.library("steam", 1);
        let path = root.join("appcache/appinfo.vdf");
        fs::remove_file(&path).unwrap();
        let missing = scan_installations([root.clone()]);
        assert!(missing.games.is_empty());
        assert_eq!(
            missing.diagnostics,
            ["ignored a Steam app without verified game type"]
        );
        fs::write(&path, b"invalid").unwrap();
        let invalid = scan_installations([root]);
        assert!(invalid.games.is_empty());
        assert_eq!(invalid.diagnostics.len(), 2);
    }

    #[test]
    fn appinfo_parser_rejects_truncated_and_duplicate_data() {
        let mut bytes = appinfo_fixture(&[(1, "Game")]);
        assert_eq!(
            parse_appinfo_types(&bytes).and_then(|types| types.get(&1).copied()),
            Some(true)
        );
        bytes.pop();
        assert!(parse_appinfo_types(&bytes).is_none());
        assert!(parse_appinfo_types(&appinfo_fixture(&[(1, "Game"), (1, "Tool")])).is_none());
        let many = (1..=MAX_APPINFO_RECORDS as u32 + 1)
            .map(|id| (id, "Game"))
            .collect::<Vec<_>>();
        assert!(parse_appinfo_types(&appinfo_fixture(&many)).is_none());
    }

    #[test]
    fn parses_modern_and_legacy_libraries_without_confusing_nested_paths() {
        assert_eq!(
            parse_library_folders(
                r#"// library metadata
                "LibraryFolders" {
                    "TimeNextStatsReport" "1234"
                    "0" { "path" "/games/日本語" "apps" { "570" "100" }
                          "extra" { "path" "/ignored" } }
                    "1" "/other games"
                }"#
                .as_bytes()
            ),
            Ok(vec![
                PathBuf::from("/games/日本語"),
                PathBuf::from("/other games")
            ])
        );
    }

    #[test]
    fn rejects_unsafe_or_ambiguous_library_paths() {
        for entry in [
            r#""0" "relative""#,
            r#""0" "/games/../outside""#,
            r#""0" "/games\nprivate""#,
            r#""0" { "apps" { "path" "/nested" } }"#,
            r#""0" { "path" "/one" "PATH" "/two" }"#,
            r#""0" "/one" "00" "/two""#,
            r#""0" { "path" {} }"#,
        ] {
            let text = format!(r#""libraryfolders" {{ {entry} }}"#);
            assert!(parse_library_folders(text.as_bytes()).is_err());
        }
    }

    #[test]
    fn library_metadata_is_bounded_and_requires_a_complete_document() {
        assert_eq!(
            parse_library_folders(&vec![b' '; MAX_MANIFEST_BYTES + 1]),
            Err(ManifestDiagnostic::InputTooLarge)
        );
        assert_eq!(
            parse_library_folders(&[0xff]),
            Err(ManifestDiagnostic::InvalidUtf8)
        );
        for text in [
            r#""libraryfolders" { "0" "/games""#,
            r#""libraryfolders" { } "extra" "data""#,
        ] {
            assert!(matches!(
                parse_library_folders(text.as_bytes()),
                Err(ManifestDiagnostic::Malformed { .. })
            ));
        }
        let text = format!(
            r#""libraryfolders" {{ "0" {{ "path" "/games" {}{} }} }}"#,
            "\"nested\" {".repeat(MAX_OBJECT_DEPTH),
            "}".repeat(MAX_OBJECT_DEPTH)
        );
        assert_eq!(
            parse_library_folders(text.as_bytes()),
            Err(ManifestDiagnostic::NestingTooDeep)
        );
    }

    #[test]
    fn discovers_configured_libraries_and_deduplicates_app_ids() {
        let fixture = Fixture::new();
        let root = fixture.library("steam", 1);
        let external = fixture.library("external disk", 2);
        let duplicate = fixture.library("duplicate", 2);
        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!(
                r#""libraryfolders" {{ "0" {{ "path" "{}" }} "1" {{ "path" "{}" }} "2" "{}" }}"#,
                root.display(),
                external.display(),
                duplicate.display()
            ),
        )
        .unwrap();
        let scan = scan_installations([root.clone(), root]);
        assert_eq!(
            scan.games
                .iter()
                .map(|game| game.app_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(scan.diagnostics, Vec::<String>::new());
    }

    #[test]
    fn invalid_library_metadata_does_not_hide_default_games_or_leak_paths() {
        let fixture = Fixture::new();
        let root = fixture.library("private-account-name", 1);
        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            br#""libraryfolders" { "0" "private-account-name" }"#,
        )
        .unwrap();
        let scan = scan_installations([root]);
        assert_eq!(
            scan.games
                .iter()
                .map(|game| game.app_id)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(scan.diagnostics.len(), 1);
        assert!(!scan.diagnostics[0].contains("private-account-name"));
        assert!(!scan.diagnostics[0].contains(&fixture.0.to_string_lossy().to_string()));
    }

    #[test]
    fn missing_external_library_and_oversized_manifest_are_diagnosed() {
        let fixture = Fixture::new();
        let root = fixture.library("steam", 1);
        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!(
                r#""libraryfolders" {{ "1" "{}" }}"#,
                fixture.0.join("missing-private-disk").display()
            ),
        )
        .unwrap();
        fs::write(
            root.join("steamapps/appmanifest_2.acf"),
            vec![b' '; MAX_MANIFEST_BYTES + 1],
        )
        .unwrap();
        let scan = scan_installations([root]);
        assert_eq!(
            scan.games
                .iter()
                .map(|game| game.app_id)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(scan.diagnostics.len(), 2);
        assert_ne!(scan.diagnostics[0], scan.diagnostics[1]);
        assert!(
            scan.diagnostics
                .iter()
                .all(|message| !message.contains("missing-private-disk"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn canonical_libraries_are_scanned_once_and_escaped_installs_are_rejected() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let root = fixture.library("steam", 1);
        let alias = fixture.0.join("alias");
        symlink(&root, &alias).unwrap();
        let outside = fixture.0.join("private-outside");
        fs::create_dir(&outside).unwrap();
        fs::remove_dir(root.join("steamapps/common/Game")).unwrap();
        symlink(outside, root.join("steamapps/common/Game")).unwrap();
        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!(
                r#""libraryfolders" {{ "0" "{}" "1" "{}" }}"#,
                root.display(),
                alias.display()
            ),
        )
        .unwrap();
        let scan = scan_installations([root, alias]);
        assert!(scan.games.is_empty());
        // The aliased library must not generate the same rejection twice.
        assert_eq!(scan.diagnostics.len(), 1);
        assert!(!scan.diagnostics[0].contains("private-outside"));
    }

    #[test]
    fn mismatched_manifest_app_id_is_not_imported() {
        let fixture = Fixture::new();
        let root = fixture.library("steam", 1);
        fs::rename(
            root.join("steamapps/appmanifest_1.acf"),
            root.join("steamapps/appmanifest_2.acf"),
        )
        .unwrap();
        let scan = scan_installations([root]);
        assert!(scan.games.is_empty());
        assert_eq!(scan.diagnostics.len(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn manifest_symlinks_and_directories_are_not_read() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let root = fixture.library("steam", 1);
        let steamapps = root.join("steamapps");
        symlink(
            steamapps.join("appmanifest_1.acf"),
            steamapps.join("appmanifest_2.acf"),
        )
        .unwrap();
        fs::create_dir(steamapps.join("appmanifest_3.acf")).unwrap();
        let scan = scan_installations([root]);
        assert_eq!(
            scan.games
                .iter()
                .map(|game| game.app_id)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(scan.diagnostics.len(), 2);
        assert_eq!(scan.diagnostics[0], scan.diagnostics[1]);
    }
    #[test]
    fn parses_valid_manifest_without_using_nested_fields() {
        let manifest = br#"// Steam metadata
            "AppState" {
                "appid" "570"
                "name" "Dota 2"
                "installdir" "dota 2 beta"
                "MountedDepots" { "appid" "999" "nested" { "value" "ok" } }
            }"#;
        assert_eq!(
            parse_app_manifest(manifest),
            Ok(SteamAppManifest {
                app_id: 570,
                name: "Dota 2".into(),
                install_dir: "dota 2 beta".into(),
            })
        );
    }

    #[test]
    fn diagnoses_missing_appid() {
        assert_eq!(
            parse_app_manifest(br#""AppState" { "name" "Game" "installdir" "Game" }"#),
            Err(ManifestDiagnostic::MissingField("appid"))
        );
    }

    #[test]
    fn requires_positive_decimal_u32_appid() {
        for id in ["", "0", "-1", "+1", "1.5", "abc", "4294967296"] {
            let manifest =
                format!(r#""AppState" {{ "appid" "{id}" "name" "Game" "installdir" "Game" }}"#);
            assert_eq!(
                parse_app_manifest(manifest.as_bytes()),
                Err(ManifestDiagnostic::InvalidField("appid"))
            );
        }
        let manifest = br#""AppState" { "appid" "4294967295" "name" "Game" "installdir" "Game" }"#;
        assert_eq!(
            parse_app_manifest(manifest).map(|game| game.app_id),
            Ok(u32::MAX)
        );
    }

    #[test]
    fn rejects_oversize_before_parsing() {
        assert_eq!(
            parse_app_manifest(&vec![b' '; MAX_MANIFEST_BYTES + 1]),
            Err(ManifestDiagnostic::InputTooLarge)
        );
        let mut manifest =
            br#""AppState" { "appid" "1" "name" "Game" "installdir" "Game" }"#.to_vec();
        manifest.resize(MAX_MANIFEST_BYTES, b' ');
        assert_eq!(parse_app_manifest(&manifest).map(|game| game.app_id), Ok(1));
    }

    #[test]
    fn rejects_malformed_quotes_objects_and_trailing_data() {
        for manifest in [
            r#""AppState" { "appid" "1 }"#,
            r#""AppState" { "appid" "1" "name" "Game" "installdir" "Game" "#,
            r#""AppState" { "appid" "1" "name" "Game" "installdir" "Game" } }"#,
            r#""AppState" { appid "1" }"#,
            r#""AppState" { "appid" "1" "name" "Game" "installdir" "Game" "extra" { "key" } }"#,
        ] {
            assert!(matches!(
                parse_app_manifest(manifest.as_bytes()),
                Err(ManifestDiagnostic::Malformed { .. })
            ));
        }
    }

    #[test]
    fn rejects_ambiguous_appid_and_unsafe_directory() {
        assert_eq!(
            parse_app_manifest(br#""AppState" { "appid" "1" "APPID" "2" }"#),
            Err(ManifestDiagnostic::DuplicateField("appid"))
        );
        for path in ["..", "../Game", "/Game", r"C:\Game", r"..\Game"] {
            let manifest =
                format!(r#""AppState" {{ "appid" "1" "name" "Game" "installdir" "{path}" }}"#);
            assert_eq!(
                parse_app_manifest(manifest.as_bytes()),
                Err(ManifestDiagnostic::InvalidField("installdir"))
            );
        }
    }

    #[test]
    fn decodes_escaped_quotes_and_preserves_unicode() {
        let manifest = r#""AppState" { "appid" "1" "name" "日本語 \"Game\"" "installdir" "Game" }"#;
        assert_eq!(
            parse_app_manifest(manifest.as_bytes()).map(|game| game.name),
            Ok("日本語 \"Game\"".into())
        );
    }

    #[test]
    fn diagnoses_invalid_utf8_and_excessive_nesting() {
        assert_eq!(
            parse_app_manifest(&[0xff]),
            Err(ManifestDiagnostic::InvalidUtf8)
        );
        let manifest = format!(
            "\"AppState\" {{ {}{} }}",
            "\"nested\" {".repeat(MAX_OBJECT_DEPTH),
            "}".repeat(MAX_OBJECT_DEPTH)
        );
        assert_eq!(
            parse_app_manifest(manifest.as_bytes()),
            Err(ManifestDiagnostic::NestingTooDeep)
        );
    }
}
