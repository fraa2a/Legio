//! Steam's binary `shortcuts.vdf`: parsing, planning and a guarded write.
//!
//! The file is parsed into a tree that keeps every key and value as raw bytes, so
//! serializing an unchanged tree reproduces the input exactly. Value types Legio does
//! not understand make parsing fail, and a failed parse never leads to a write.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
const MAX_NODES: usize = 200_000;
const MAX_DEPTH: usize = 8;
const MAX_SHORTCUTS: usize = 10_000;
const KEPT_BACKUPS: usize = 5;
const MAP: u8 = 0x00;
const TEXT: u8 = 0x01;
const INT: u8 = 0x02;
const END: u8 = 0x08;
const TAG: &str = "Legio";
const LAUNCH_PREFIX: &str = "--launch-game=";
const FILE_NAME: &str = "shortcuts.vdf";
const BACKUP_PREFIX: &str = "shortcuts.vdf.legio-";
const BACKUP_SUFFIX: &str = ".bak";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShortcutsError {
    TooLarge,
    Malformed { offset: usize },
    UnsupportedType { offset: usize, kind: u8 },
    TooDeep,
    TooManyEntries,
    UnexpectedShape,
    AppIdCollision,
}

impl fmt::Display for ShortcutsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => formatter.write_str("shortcuts.vdf exceeds the size limit"),
            Self::Malformed { offset } => {
                write!(formatter, "shortcuts.vdf is malformed at byte {offset}")
            }
            Self::UnsupportedType { offset, kind } => write!(
                formatter,
                "shortcuts.vdf has an unsupported value type {kind} at byte {offset}"
            ),
            Self::TooDeep => formatter.write_str("shortcuts.vdf nesting is too deep"),
            Self::TooManyEntries => formatter.write_str("shortcuts.vdf has too many entries"),
            Self::UnexpectedShape => {
                formatter.write_str("shortcuts.vdf does not have the expected structure")
            }
            Self::AppIdCollision => {
                formatter.write_str("Could not find a free Steam shortcut ID for a game")
            }
        }
    }
}

impl std::error::Error for ShortcutsError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    key: Vec<u8>,
    value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value {
    Map(Vec<Entry>),
    Text(Vec<u8>),
    Int([u8; 4]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShortcutsFile {
    root: Vec<Entry>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
    nodes: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, ShortcutsError> {
        let byte = *self
            .bytes
            .get(self.offset)
            .ok_or(ShortcutsError::Malformed {
                offset: self.offset,
            })?;
        self.offset += 1;
        Ok(byte)
    }

    fn text(&mut self) -> Result<Vec<u8>, ShortcutsError> {
        let rest = &self.bytes[self.offset..];
        let length = rest
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(ShortcutsError::Malformed {
                offset: self.bytes.len(),
            })?;
        self.offset += length + 1;
        Ok(rest[..length].to_vec())
    }

    fn int(&mut self) -> Result<[u8; 4], ShortcutsError> {
        let end = self.offset + 4;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or(ShortcutsError::Malformed {
                offset: self.bytes.len(),
            })?;
        self.offset = end;
        Ok([slice[0], slice[1], slice[2], slice[3]])
    }

    fn map(&mut self, depth: usize) -> Result<Vec<Entry>, ShortcutsError> {
        if depth > MAX_DEPTH {
            return Err(ShortcutsError::TooDeep);
        }
        let mut entries = Vec::new();
        loop {
            let kind_offset = self.offset;
            let kind = self.byte()?;
            if kind == END {
                return Ok(entries);
            }
            if !matches!(kind, MAP | TEXT | INT) {
                return Err(ShortcutsError::UnsupportedType {
                    offset: kind_offset,
                    kind,
                });
            }
            self.nodes += 1;
            if self.nodes > MAX_NODES {
                return Err(ShortcutsError::TooManyEntries);
            }
            let key = self.text()?;
            let value = match kind {
                MAP => Value::Map(self.map(depth + 1)?),
                TEXT => Value::Text(self.text()?),
                _ => Value::Int(self.int()?),
            };
            entries.push(Entry { key, value });
        }
    }
}

fn write_map(entries: &[Entry], output: &mut Vec<u8>) {
    for entry in entries {
        output.push(match entry.value {
            Value::Map(_) => MAP,
            Value::Text(_) => TEXT,
            Value::Int(_) => INT,
        });
        output.extend_from_slice(&entry.key);
        output.push(0);
        match &entry.value {
            Value::Map(children) => write_map(children, output),
            Value::Text(text) => {
                output.extend_from_slice(text);
                output.push(0);
            }
            Value::Int(bytes) => output.extend_from_slice(bytes),
        }
    }
    output.push(END);
}

fn find<'a>(entries: &'a [Entry], name: &str) -> Option<&'a Entry> {
    entries
        .iter()
        .find(|entry| entry.key.eq_ignore_ascii_case(name.as_bytes()))
}

fn text_field(entries: &[Entry], name: &str) -> Option<String> {
    match &find(entries, name)?.value {
        Value::Text(text) => Some(String::from_utf8_lossy(text).into_owned()),
        _ => None,
    }
}

fn int_field(entries: &[Entry], name: &str) -> Option<u32> {
    match find(entries, name)?.value {
        Value::Int(bytes) => Some(u32::from_le_bytes(bytes)),
        _ => None,
    }
}

// Replaces or appends a field. A same-named field of another type is replaced too.
fn set_field(entries: &mut Vec<Entry>, name: &str, value: Value) {
    match entries
        .iter_mut()
        .find(|entry| entry.key.eq_ignore_ascii_case(name.as_bytes()))
    {
        Some(entry) => entry.value = value,
        None => entries.push(Entry {
            key: name.as_bytes().to_vec(),
            value,
        }),
    }
}

fn text(value: &str) -> Value {
    Value::Text(value.as_bytes().to_vec())
}

fn int(value: u32) -> Value {
    Value::Int(value.to_le_bytes())
}

impl ShortcutsFile {
    pub(crate) fn empty() -> Self {
        Self { root: Vec::new() }
    }

    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ShortcutsError> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(ShortcutsError::TooLarge);
        }
        if bytes.is_empty() {
            return Ok(Self::empty());
        }
        let mut reader = Reader {
            bytes,
            offset: 0,
            nodes: 0,
        };
        let root = reader.map(0)?;
        if reader.offset != bytes.len() {
            return Err(ShortcutsError::Malformed {
                offset: reader.offset,
            });
        }
        let file = Self { root };
        let matches = file
            .root
            .iter()
            .filter(|entry| entry.key.eq_ignore_ascii_case(b"shortcuts"))
            .count();
        if matches > 1 {
            return Err(ShortcutsError::UnexpectedShape);
        }
        if let Some(entry) = find(&file.root, "shortcuts") {
            match &entry.value {
                Value::Map(shortcuts) if shortcuts.len() <= MAX_SHORTCUTS => {}
                Value::Map(_) => return Err(ShortcutsError::TooManyEntries),
                _ => return Err(ShortcutsError::UnexpectedShape),
            }
        }
        Ok(file)
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut output = Vec::new();
        write_map(&self.root, &mut output);
        output
    }

    fn shortcuts(&self) -> &[Entry] {
        match find(&self.root, "shortcuts") {
            Some(Entry {
                value: Value::Map(shortcuts),
                ..
            }) => shortcuts,
            _ => &[],
        }
    }

    fn shortcuts_mut(&mut self) -> &mut Vec<Entry> {
        let position = self
            .root
            .iter()
            .position(|entry| entry.key.eq_ignore_ascii_case(b"shortcuts"));
        let position = position.unwrap_or_else(|| {
            self.root.push(Entry {
                key: b"shortcuts".to_vec(),
                value: Value::Map(Vec::new()),
            });
            self.root.len() - 1
        });
        match &mut self.root[position].value {
            Value::Map(shortcuts) => shortcuts,
            // parse() refuses a non-map `shortcuts` value.
            _ => unreachable!("shortcuts is always a map"),
        }
    }

    /// Entries created by Legio, by their game id.
    pub(crate) fn owned_game_ids(&self, launcher: &str) -> Vec<String> {
        self.shortcuts()
            .iter()
            .filter_map(|entry| match &entry.value {
                Value::Map(fields) => owned_game_id(fields, launcher),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn shortcut_count(&self) -> usize {
        self.shortcuts().len()
    }
}

pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn quoted(value: &str) -> String {
    format!("\"{value}\"")
}

fn unquoted(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

/// The shortcut app id Steam clients use: CRC32 of the quoted exe and the name, with
/// the top bit set.
pub(crate) fn shortcut_app_id(quoted_exe: &str, name: &str) -> u32 {
    crc32(format!("{quoted_exe}{name}").as_bytes()) | 0x8000_0000
}

fn launch_options(game_id: &str) -> String {
    format!("{LAUNCH_PREFIX}{game_id}")
}

fn launch_game_id(options: &str) -> Option<String> {
    let id = options.strip_prefix(LAUNCH_PREFIX)?;
    let canonical = uuid::Uuid::parse_str(id).ok()?.to_string();
    (canonical == id).then_some(canonical)
}

fn has_tag(fields: &[Entry]) -> bool {
    match find(fields, "tags") {
        Some(Entry {
            value: Value::Map(tags),
            ..
        }) => tags
            .iter()
            .any(|tag| matches!(&tag.value, Value::Text(value) if value == TAG.as_bytes())),
        _ => false,
    }
}

// A shortcut belongs to Legio when it launches a game through `--launch-game=<uuid>`
// and either carries the Legio tag or runs the current Legio executable.
fn owned_game_id(fields: &[Entry], launcher: &str) -> Option<String> {
    let id = launch_game_id(&text_field(fields, "LaunchOptions")?)?;
    let runs_launcher =
        text_field(fields, "Exe").is_some_and(|exe| unquoted(&exe) == unquoted(launcher));
    (has_tag(fields) || runs_launcher).then_some(id)
}

fn ensure_tag(fields: &mut Vec<Entry>) {
    if has_tag(fields) {
        return;
    }
    let position = fields
        .iter()
        .position(|entry| entry.key.eq_ignore_ascii_case(b"tags"))
        .unwrap_or_else(|| {
            fields.push(Entry {
                key: b"tags".to_vec(),
                value: Value::Map(Vec::new()),
            });
            fields.len() - 1
        });
    if !matches!(fields[position].value, Value::Map(_)) {
        fields[position].value = Value::Map(Vec::new());
    }
    if let Value::Map(tags) = &mut fields[position].value {
        let next = next_index(tags);
        tags.push(Entry {
            key: next.to_string().into_bytes(),
            value: text(TAG),
        });
    }
}

fn next_index(entries: &[Entry]) -> u64 {
    entries
        .iter()
        .filter_map(|entry| std::str::from_utf8(&entry.key).ok()?.parse::<u64>().ok())
        .max()
        .map_or(0, |highest| highest + 1)
}

pub(crate) struct ExportContext<'a> {
    pub launcher: &'a str,
    pub grid_dir: &'a Path,
}

impl ExportContext<'_> {
    fn quoted_exe(&self) -> String {
        quoted(self.launcher)
    }

    fn start_dir(&self) -> String {
        let parent = Path::new(self.launcher)
            .parent()
            .and_then(Path::to_str)
            .unwrap_or_default();
        quoted(parent)
    }

    fn icon_path(&self, app_id: u32) -> Option<String> {
        self.grid_dir
            .join(format!("{app_id}_icon.png"))
            .to_str()
            .map(str::to_owned)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportGame {
    pub game_id: String,
    pub name: String,
    pub has_icon: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action {
    Add,
    Update,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Change {
    pub game_id: String,
    pub name: String,
    pub app_id: u32,
    pub action: Action,
    has_icon: bool,
    entry_key: Option<Vec<u8>>,
}

fn used_app_ids(file: &ShortcutsFile) -> Vec<u32> {
    file.shortcuts()
        .iter()
        .filter_map(|entry| match &entry.value {
            Value::Map(fields) => int_field(fields, "appid"),
            _ => None,
        })
        .collect()
}

fn free_app_id(
    context: &ExportContext<'_>,
    game: &ExportGame,
    used: &[u32],
) -> Result<u32, ShortcutsError> {
    let exe = context.quoted_exe();
    let base = shortcut_app_id(&exe, &game.name);
    if !used.contains(&base) {
        return Ok(base);
    }
    let salted = shortcut_app_id(&exe, &format!("{} {}", game.name, game.game_id));
    if used.contains(&salted) {
        return Err(ShortcutsError::AppIdCollision);
    }
    Ok(salted)
}

fn new_entry_fields(context: &ExportContext<'_>, game: &ExportGame, app_id: u32) -> Vec<Entry> {
    let entry = |key: &str, value: Value| Entry {
        key: key.as_bytes().to_vec(),
        value,
    };
    let icon = if game.has_icon {
        context.icon_path(app_id).unwrap_or_default()
    } else {
        String::new()
    };
    vec![
        entry("appid", int(app_id)),
        entry("AppName", text(&game.name)),
        entry("Exe", text(&context.quoted_exe())),
        entry("StartDir", text(&context.start_dir())),
        entry("icon", text(&icon)),
        entry("ShortcutPath", text("")),
        entry("LaunchOptions", text(&launch_options(&game.game_id))),
        entry("IsHidden", int(0)),
        entry("AllowDesktopConfig", int(1)),
        entry("AllowOverlay", int(1)),
        entry("OpenVR", int(0)),
        entry("Devkit", int(0)),
        entry("DevkitGameID", text("")),
        entry("DevkitOverrideAppID", int(0)),
        entry("LastPlayTime", int(0)),
        entry("FlatpakAppID", text("")),
        entry(
            "tags",
            Value::Map(vec![Entry {
                key: b"0".to_vec(),
                value: text(TAG),
            }]),
        ),
    ]
}

// Brings the fields Legio manages up to date and leaves all other fields alone.
fn update_fields(
    fields: &mut Vec<Entry>,
    context: &ExportContext<'_>,
    game: &ExportGame,
    app_id: u32,
) {
    if int_field(fields, "appid").is_none() {
        set_field(fields, "appid", int(app_id));
    }
    set_field(fields, "AppName", text(&game.name));
    set_field(fields, "Exe", text(&context.quoted_exe()));
    set_field(fields, "StartDir", text(&context.start_dir()));
    set_field(
        fields,
        "LaunchOptions",
        text(&launch_options(&game.game_id)),
    );
    if game.has_icon {
        let icon = context.icon_path(app_id).unwrap_or_default();
        set_field(fields, "icon", text(&icon));
    }
    ensure_tag(fields);
}

/// Decides, for every game, whether a shortcut has to be added or updated. Entries
/// Legio did not create are never matched.
pub(crate) fn plan_export(
    file: &ShortcutsFile,
    context: &ExportContext<'_>,
    games: &[ExportGame],
) -> Result<Vec<Change>, ShortcutsError> {
    let mut used = used_app_ids(file);
    let mut changes = Vec::with_capacity(games.len());
    for game in games {
        let existing = file.shortcuts().iter().find(|entry| match &entry.value {
            Value::Map(fields) => {
                owned_game_id(fields, context.launcher).as_deref() == Some(game.game_id.as_str())
            }
            _ => false,
        });
        let change = match existing {
            Some(Entry {
                key,
                value: Value::Map(fields),
            }) => {
                let app_id = match int_field(fields, "appid") {
                    Some(app_id) => app_id,
                    None => {
                        let app_id = free_app_id(context, game, &used)?;
                        used.push(app_id);
                        app_id
                    }
                };
                let mut updated = fields.clone();
                update_fields(&mut updated, context, game, app_id);
                Change {
                    game_id: game.game_id.clone(),
                    name: game.name.clone(),
                    app_id,
                    action: if updated == *fields {
                        Action::Unchanged
                    } else {
                        Action::Update
                    },
                    has_icon: game.has_icon,
                    entry_key: Some(key.clone()),
                }
            }
            _ => {
                let app_id = free_app_id(context, game, &used)?;
                used.push(app_id);
                Change {
                    game_id: game.game_id.clone(),
                    name: game.name.clone(),
                    app_id,
                    action: Action::Add,
                    has_icon: game.has_icon,
                    entry_key: None,
                }
            }
        };
        changes.push(change);
    }
    Ok(changes)
}

pub(crate) fn apply_export(
    file: &mut ShortcutsFile,
    context: &ExportContext<'_>,
    changes: &[Change],
) {
    let shortcuts = file.shortcuts_mut();
    let mut next = next_index(shortcuts);
    for change in changes {
        let game = ExportGame {
            game_id: change.game_id.clone(),
            name: change.name.clone(),
            has_icon: change.has_icon,
        };
        match (&change.action, &change.entry_key) {
            (Action::Add, _) => {
                shortcuts.push(Entry {
                    key: next.to_string().into_bytes(),
                    value: Value::Map(new_entry_fields(context, &game, change.app_id)),
                });
                next += 1;
            }
            (Action::Update, Some(key)) => {
                if let Some(Entry {
                    value: Value::Map(fields),
                    ..
                }) = shortcuts.iter_mut().find(|entry| entry.key == *key)
                {
                    update_fields(fields, context, &game, change.app_id);
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Removal {
    pub game_id: String,
    pub name: String,
    entry_key: Vec<u8>,
}

/// Legio's own entries, limited to `game_ids` when given.
pub(crate) fn plan_removal(
    file: &ShortcutsFile,
    launcher: &str,
    game_ids: Option<&[String]>,
) -> Vec<Removal> {
    file.shortcuts()
        .iter()
        .filter_map(|entry| {
            let Value::Map(fields) = &entry.value else {
                return None;
            };
            let game_id = owned_game_id(fields, launcher)?;
            if game_ids.is_some_and(|ids| !ids.contains(&game_id)) {
                return None;
            }
            Some(Removal {
                game_id,
                name: text_field(fields, "AppName").unwrap_or_default(),
                entry_key: entry.key.clone(),
            })
        })
        .collect()
}

pub(crate) fn apply_removal(file: &mut ShortcutsFile, removals: &[Removal]) {
    file.shortcuts_mut().retain(|entry| {
        !removals
            .iter()
            .any(|removal| removal.entry_key == entry.key)
    });
}

/// `<root>/userdata/<account>/config` for a numeric account id, never through a symlink.
pub(crate) fn config_directory(steam_root: &Path, account_id: u32) -> Result<PathBuf, String> {
    let account = account_id.to_string();
    let mut directory = steam_root.to_path_buf();
    for part in ["userdata", account.as_str()] {
        directory.push(part);
        require_real_directory(&directory, true)?;
    }
    directory.push("config");
    require_real_directory(&directory, false)?;
    Ok(directory)
}

fn require_real_directory(path: &Path, must_exist: bool) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err("A Steam userdata folder is not a regular folder".to_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound && !must_exist => Ok(()),
        Err(error) => Err(format!(
            "Could not inspect the Steam userdata folder: {error}"
        )),
    }
}

pub(crate) fn grid_directory(config: &Path) -> PathBuf {
    config.join("grid")
}

/// Creates Steam's artwork folder for non-Steam shortcuts, never through a symlink.
pub(crate) fn create_grid_directory(config: &Path) -> Result<PathBuf, String> {
    let grid = grid_directory(config);
    fs::create_dir_all(&grid)
        .map_err(|error| format!("Could not create the Steam artwork folder: {error}"))?;
    require_real_directory(&grid, true)?;
    Ok(grid)
}

/// Writes `bytes` only if no file exists at `path`. Returns whether it was written.
pub(crate) fn write_new_file(path: &Path, bytes: &[u8]) -> io::Result<bool> {
    use std::io::Write;

    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(error),
    };
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(true)
}

/// Reads and parses the account's shortcuts. A missing file is an empty one.
pub(crate) fn read_shortcuts(config: &Path) -> Result<(ShortcutsFile, Option<Vec<u8>>), String> {
    let path = config.join(FILE_NAME);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok((ShortcutsFile::empty(), None));
        }
        Err(error) => return Err(format!("Could not inspect shortcuts.vdf: {error}")),
    };
    if !metadata.file_type().is_file() {
        return Err("shortcuts.vdf is not a regular file".to_owned());
    }
    if metadata.len() > MAX_FILE_BYTES as u64 {
        return Err(ShortcutsError::TooLarge.to_string());
    }
    let bytes =
        fs::read(&path).map_err(|error| format!("Could not read shortcuts.vdf: {error}"))?;
    let file = ShortcutsFile::parse(&bytes).map_err(|error| error.to_string())?;
    Ok((file, Some(bytes)))
}

fn backup_name() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    format!("{BACKUP_PREFIX}{millis:020}{BACKUP_SUFFIX}")
}

fn prune_backups(config: &Path) {
    let Ok(entries) = fs::read_dir(config) else {
        return;
    };
    let mut backups: Vec<String> = entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.starts_with(BACKUP_PREFIX) && name.ends_with(BACKUP_SUFFIX))
        .collect();
    backups.sort();
    let surplus = backups.len().saturating_sub(KEPT_BACKUPS);
    for name in backups.into_iter().take(surplus) {
        if let Err(error) = fs::remove_file(config.join(&name))
            && error.kind() != io::ErrorKind::NotFound
        {
            crate::application_log::failure("prune_steam_shortcuts_backup", error);
        }
    }
}

/// Edits the account's shortcuts and writes them back. Refuses while Steam runs,
/// keeps a timestamped backup of the previous file, writes atomically and checks the
/// result by reading it again. Nothing is written when `edit` changes nothing.
pub(crate) fn modify_shortcuts<T>(
    config: &Path,
    steam_is_running: &dyn Fn() -> Result<bool, String>,
    edit: impl FnOnce(&mut ShortcutsFile) -> Result<T, ShortcutsError>,
) -> Result<T, String> {
    if steam_is_running()? {
        return Err(
            "Steam is running. Close Steam completely, then try again: it would overwrite the change when it exits."
                .to_owned(),
        );
    }
    let (mut file, original) = read_shortcuts(config)?;
    let result = edit(&mut file).map_err(|error| error.to_string())?;
    let bytes = file.to_bytes();
    let unchanged = match &original {
        Some(original) => *original == bytes,
        None => file.shortcut_count() == 0,
    };
    if unchanged {
        return Ok(result);
    }
    fs::create_dir_all(config)
        .map_err(|error| format!("Could not create the Steam config folder: {error}"))?;
    require_real_directory(config, true)?;
    if let Some(original) = &original {
        crate::atomic_file::write_atomic(&config.join(backup_name()), original)
            .map_err(|error| format!("Could not back up shortcuts.vdf: {error}"))?;
    }
    let path = config.join(FILE_NAME);
    crate::atomic_file::write_atomic(&path, &bytes)
        .map_err(|error| format!("Could not write shortcuts.vdf: {error}"))?;
    let (verified, _) = read_shortcuts(config)?;
    if verified.to_bytes() != bytes {
        return Err(
            "shortcuts.vdf did not read back as written. Restore the latest backup next to it."
                .to_owned(),
        );
    }
    prune_backups(config);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const LAUNCHER: &str = "/opt/legio/legio-launcher";
    const ID_ONE: &str = "00000000-0000-4000-8000-000000000001";
    const ID_TWO: &str = "00000000-0000-4000-8000-000000000002";

    fn temp_dir() -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("legio-shortcuts-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn context(grid: &Path) -> ExportContext<'_> {
        ExportContext {
            launcher: LAUNCHER,
            grid_dir: grid,
        }
    }

    fn game(id: &str, name: &str) -> ExportGame {
        ExportGame {
            game_id: id.to_owned(),
            name: name.to_owned(),
            has_icon: false,
        }
    }

    fn cstr(output: &mut Vec<u8>, value: &str) {
        output.extend_from_slice(value.as_bytes());
        output.push(0);
    }

    fn text_entry(output: &mut Vec<u8>, key: &str, value: &str) {
        output.push(TEXT);
        cstr(output, key);
        cstr(output, value);
    }

    fn int_entry(output: &mut Vec<u8>, key: &str, value: u32) {
        output.push(INT);
        cstr(output, key);
        output.extend_from_slice(&value.to_le_bytes());
    }

    // A foreign entry with lower-case Steam 2020 keys and an unknown key.
    fn foreign_lowercase(output: &mut Vec<u8>, index: &str) {
        output.push(MAP);
        cstr(output, index);
        int_entry(output, "appid", 0x8123_4567);
        text_entry(output, "appname", "Foreign Game");
        text_entry(output, "exe", "\"/usr/bin/foreign\"");
        text_entry(output, "StartDir", "\"/usr/bin\"");
        text_entry(output, "LaunchOptions", "--fullscreen");
        int_entry(output, "UnknownFlag", 7);
        output.push(MAP);
        cstr(output, "tags");
        text_entry(output, "0", "favorite");
        output.push(END);
        output.push(END);
    }

    fn file_with(entries: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        let mut bytes = vec![MAP];
        cstr(&mut bytes, "shortcuts");
        entries(&mut bytes);
        bytes.push(END);
        bytes.push(END);
        bytes
    }

    fn not_running() -> Result<bool, String> {
        Ok(false)
    }

    #[test]
    fn crc32_matches_the_reference_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn app_id_is_deterministic_with_the_top_bit_set() {
        let first = shortcut_app_id("\"/opt/legio\"", "Game");
        assert_eq!(first, shortcut_app_id("\"/opt/legio\"", "Game"));
        assert_ne!(first, shortcut_app_id("\"/opt/legio\"", "Other"));
        assert!(first & 0x8000_0000 != 0);
    }

    #[test]
    fn round_trip_keeps_both_key_casings_and_unknown_keys() {
        let bytes = file_with(|output| {
            foreign_lowercase(output, "0");
            output.push(MAP);
            cstr(output, "1");
            int_entry(output, "AppId", 5);
            text_entry(output, "AppName", "Upper");
            output.push(END);
        });
        let file = ShortcutsFile::parse(&bytes).unwrap();
        assert_eq!(file.to_bytes(), bytes);
        assert_eq!(file.shortcut_count(), 2);
    }

    #[test]
    fn parse_refuses_malformed_input() {
        let valid = file_with(|output| foreign_lowercase(output, "0"));
        for length in 1..valid.len() - 1 {
            assert!(
                ShortcutsFile::parse(&valid[..length]).is_err(),
                "truncated at {length}"
            );
        }
        let mut trailing = valid.clone();
        trailing.push(0);
        assert!(ShortcutsFile::parse(&trailing).is_err());
        let mut unknown = vec![MAP];
        cstr(&mut unknown, "shortcuts");
        unknown.push(0x07);
        unknown.extend_from_slice(b"key\0\0\0\0\0\0\0\0\0");
        unknown.extend_from_slice(&[END, END]);
        assert!(matches!(
            ShortcutsFile::parse(&unknown),
            Err(ShortcutsError::UnsupportedType { kind: 7, .. })
        ));
        let mut wrong_shape = vec![TEXT];
        cstr(&mut wrong_shape, "shortcuts");
        cstr(&mut wrong_shape, "x");
        wrong_shape.push(END);
        assert_eq!(
            ShortcutsFile::parse(&wrong_shape),
            Err(ShortcutsError::UnexpectedShape)
        );
    }

    #[test]
    fn parse_enforces_size_depth_and_entry_limits() {
        assert_eq!(
            ShortcutsFile::parse(&vec![0; MAX_FILE_BYTES + 1]),
            Err(ShortcutsError::TooLarge)
        );
        let mut deep = Vec::new();
        for _ in 0..=MAX_DEPTH + 1 {
            deep.push(MAP);
            cstr(&mut deep, "a");
        }
        deep.extend(std::iter::repeat_n(END, MAX_DEPTH + 3));
        assert_eq!(ShortcutsFile::parse(&deep), Err(ShortcutsError::TooDeep));
        let many = file_with(|output| {
            for index in 0..=MAX_SHORTCUTS {
                output.push(MAP);
                cstr(output, &index.to_string());
                output.push(END);
            }
        });
        assert_eq!(
            ShortcutsFile::parse(&many),
            Err(ShortcutsError::TooManyEntries)
        );
        assert_eq!(ShortcutsFile::parse(&[]), Ok(ShortcutsFile::empty()));
    }

    #[test]
    fn plan_adds_and_keeps_foreign_entries_byte_for_byte() {
        let grid = Path::new("/grid");
        let bytes = file_with(|output| foreign_lowercase(output, "0"));
        let mut file = ShortcutsFile::parse(&bytes).unwrap();
        let games = [game(ID_ONE, "Alpha"), game(ID_TWO, "Beta")];
        let changes = plan_export(&file, &context(grid), &games).unwrap();
        assert!(changes.iter().all(|change| change.action == Action::Add));
        apply_export(&mut file, &context(grid), &changes);
        let written = file.to_bytes();
        let reparsed = ShortcutsFile::parse(&written).unwrap();
        assert_eq!(reparsed.shortcut_count(), 3);
        let foreign = file_with(|output| foreign_lowercase(output, "0"));
        // The foreign entry is the unchanged prefix of the shortcuts map.
        let prefix = &foreign[..foreign.len() - 2];
        assert!(written.starts_with(prefix));
        let owned = reparsed.owned_game_ids(LAUNCHER);
        assert_eq!(owned, [ID_ONE, ID_TWO]);
        let entry = &reparsed.shortcuts()[1];
        let Value::Map(fields) = &entry.value else {
            panic!("entry is a map");
        };
        assert_eq!(entry.key, b"1");
        assert_eq!(
            text_field(fields, "Exe").unwrap(),
            "\"/opt/legio/legio-launcher\""
        );
        assert_eq!(text_field(fields, "StartDir").unwrap(), "\"/opt/legio\"");
        assert_eq!(
            text_field(fields, "LaunchOptions").unwrap(),
            format!("--launch-game={ID_ONE}")
        );
        assert!(has_tag(fields));
        assert_eq!(
            int_field(fields, "appid").unwrap(),
            shortcut_app_id("\"/opt/legio/legio-launcher\"", "Alpha")
        );
    }

    #[test]
    fn plan_updates_owned_entries_only_when_needed() {
        let grid = Path::new("/grid");
        let mut file = ShortcutsFile::empty();
        let games = [game(ID_ONE, "Alpha")];
        let changes = plan_export(&file, &context(grid), &games).unwrap();
        apply_export(&mut file, &context(grid), &changes);
        let again = plan_export(&file, &context(grid), &games).unwrap();
        assert_eq!(again[0].action, Action::Unchanged);
        assert_eq!(again[0].app_id, changes[0].app_id);

        let renamed = [game(ID_ONE, "Alpha Remastered")];
        let update = plan_export(&file, &context(grid), &renamed).unwrap();
        assert_eq!(update[0].action, Action::Update);
        // The id of an existing entry stays, so its artwork keeps matching.
        assert_eq!(update[0].app_id, changes[0].app_id);
        apply_export(&mut file, &context(grid), &update);
        assert_eq!(file.shortcut_count(), 1);
        let Value::Map(fields) = &file.shortcuts()[0].value else {
            panic!("entry is a map");
        };
        assert_eq!(text_field(fields, "AppName").unwrap(), "Alpha Remastered");
    }

    #[test]
    fn update_preserves_unknown_fields_of_an_owned_entry() {
        let grid = Path::new("/grid");
        let bytes = file_with(|output| {
            output.push(MAP);
            cstr(output, "0");
            int_entry(output, "AppId", 9);
            text_entry(output, "appname", "Old");
            text_entry(output, "Exe", "\"/old/legio\"");
            text_entry(output, "LaunchOptions", &launch_options(ID_ONE));
            int_entry(output, "UnknownFlag", 3);
            output.push(MAP);
            cstr(output, "tags");
            text_entry(output, "0", "Legio");
            output.push(END);
            output.push(END);
        });
        let mut file = ShortcutsFile::parse(&bytes).unwrap();
        let changes = plan_export(&file, &context(grid), &[game(ID_ONE, "New")]).unwrap();
        assert_eq!(changes[0].action, Action::Update);
        assert_eq!(changes[0].app_id, 9);
        apply_export(&mut file, &context(grid), &changes);
        let Value::Map(fields) = &file.shortcuts()[0].value else {
            panic!("entry is a map");
        };
        assert_eq!(int_field(fields, "UnknownFlag"), Some(3));
        assert_eq!(text_field(fields, "AppName").unwrap(), "New");
        assert!(fields.iter().any(|field| field.key == b"appname"));
        assert!(!fields.iter().any(|field| field.key == b"AppName"));
        assert_eq!(
            text_field(fields, "Exe").unwrap(),
            "\"/opt/legio/legio-launcher\""
        );
    }

    #[test]
    fn foreign_entries_with_similar_launch_options_are_not_owned() {
        let grid = Path::new("/grid");
        let bytes = file_with(|output| {
            output.push(MAP);
            cstr(output, "0");
            text_entry(output, "AppName", "Other launcher");
            text_entry(output, "Exe", "\"/usr/bin/other\"");
            text_entry(output, "LaunchOptions", &launch_options(ID_ONE));
            output.push(END);
        });
        let file = ShortcutsFile::parse(&bytes).unwrap();
        assert!(file.owned_game_ids(LAUNCHER).is_empty());
        let changes = plan_export(&file, &context(grid), &[game(ID_ONE, "Alpha")]).unwrap();
        assert_eq!(changes[0].action, Action::Add);
        assert!(plan_removal(&file, LAUNCHER, None).is_empty());
    }

    #[test]
    fn removal_takes_only_owned_entries() {
        let grid = Path::new("/grid");
        let bytes = file_with(|output| foreign_lowercase(output, "0"));
        let mut file = ShortcutsFile::parse(&bytes).unwrap();
        let games = [game(ID_ONE, "Alpha"), game(ID_TWO, "Beta")];
        let changes = plan_export(&file, &context(grid), &games).unwrap();
        apply_export(&mut file, &context(grid), &changes);

        let selected = [ID_ONE.to_owned()];
        let removals = plan_removal(&file, LAUNCHER, Some(&selected));
        assert_eq!(removals.len(), 1);
        apply_removal(&mut file, &removals);
        assert_eq!(file.owned_game_ids(LAUNCHER), [ID_TWO]);

        let all = plan_removal(&file, LAUNCHER, None);
        apply_removal(&mut file, &all);
        assert_eq!(file.to_bytes(), bytes);
    }

    #[test]
    fn colliding_app_ids_get_a_distinct_id() {
        let grid = Path::new("/grid");
        let games = [game(ID_ONE, "Same"), game(ID_TWO, "Same")];
        let changes = plan_export(&ShortcutsFile::empty(), &context(grid), &games).unwrap();
        assert_ne!(changes[0].app_id, changes[1].app_id);
    }

    #[test]
    fn icon_path_points_into_the_grid_folder() {
        let grid = Path::new("/grid");
        let mut with_icon = game(ID_ONE, "Alpha");
        with_icon.has_icon = true;
        let mut file = ShortcutsFile::empty();
        let changes = plan_export(&file, &context(grid), &[with_icon]).unwrap();
        apply_export(&mut file, &context(grid), &changes);
        let Value::Map(fields) = &file.shortcuts()[0].value else {
            panic!("entry is a map");
        };
        assert_eq!(
            text_field(fields, "icon").unwrap(),
            format!("/grid/{}_icon.png", changes[0].app_id)
        );
    }

    fn export_to(
        config: &Path,
        steam_running: &dyn Fn() -> Result<bool, String>,
    ) -> Result<usize, String> {
        let grid = config.join("grid");
        modify_shortcuts(config, steam_running, |file| {
            let changes = plan_export(file, &context(&grid), &[game(ID_ONE, "Alpha")])?;
            apply_export(file, &context(&grid), &changes);
            Ok(changes.len())
        })
    }

    #[test]
    fn modify_creates_the_file_and_verifies_it() {
        let root = temp_dir();
        let config = root.join("config");
        assert_eq!(export_to(&config, &not_running), Ok(1));
        let (file, bytes) = read_shortcuts(&config).unwrap();
        assert!(bytes.is_some());
        assert_eq!(file.owned_game_ids(LAUNCHER), [ID_ONE]);
        assert_eq!(fs::read_dir(&config).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn modify_backs_up_the_previous_file_and_prunes_old_backups() {
        let root = temp_dir();
        let config = root.join("config");
        fs::create_dir_all(&config).unwrap();
        let original = file_with(|output| foreign_lowercase(output, "0"));
        fs::write(config.join(FILE_NAME), &original).unwrap();
        for index in 0..KEPT_BACKUPS + 2 {
            fs::write(
                config.join(format!("{BACKUP_PREFIX}{index:020}{BACKUP_SUFFIX}")),
                b"old",
            )
            .unwrap();
        }
        export_to(&config, &not_running).unwrap();
        let backups: Vec<_> = fs::read_dir(&config)
            .unwrap()
            .filter_map(|entry| entry.unwrap().file_name().into_string().ok())
            .filter(|name| name.starts_with(BACKUP_PREFIX))
            .collect();
        assert_eq!(backups.len(), KEPT_BACKUPS);
        let newest = backups.iter().max().unwrap();
        assert_eq!(fs::read(config.join(newest)).unwrap(), original);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn modify_writes_nothing_when_nothing_changes() {
        let root = temp_dir();
        let config = root.join("config");
        export_to(&config, &not_running).unwrap();
        let before = fs::read(config.join(FILE_NAME)).unwrap();
        export_to(&config, &not_running).unwrap();
        assert_eq!(fs::read(config.join(FILE_NAME)).unwrap(), before);
        assert_eq!(fs::read_dir(&config).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn modify_refuses_while_steam_runs() {
        let root = temp_dir();
        let config = root.join("config");
        let checked = Cell::new(false);
        let running = || {
            checked.set(true);
            Ok(true)
        };
        let error = export_to(&config, &running).unwrap_err();
        assert!(checked.get());
        assert!(error.contains("Close Steam"));
        assert!(!config.exists());
        assert!(export_to(&config, &|| Err("probe failed".to_owned())).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn modify_leaves_an_unreadable_file_untouched() {
        let root = temp_dir();
        let config = root.join("config");
        fs::create_dir_all(&config).unwrap();
        let broken = b"\x00shortcuts\x00\x07nope".to_vec();
        fs::write(config.join(FILE_NAME), &broken).unwrap();
        assert!(export_to(&config, &not_running).is_err());
        assert_eq!(fs::read(config.join(FILE_NAME)).unwrap(), broken);
        assert_eq!(fs::read_dir(&config).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_inside_userdata_are_refused() {
        let root = temp_dir();
        let outside = temp_dir();
        fs::create_dir_all(root.join("userdata")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("userdata/42")).unwrap();
        assert!(config_directory(&root, 42).is_err());

        fs::remove_file(root.join("userdata/42")).unwrap();
        fs::create_dir_all(root.join("userdata/42")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("userdata/42/config")).unwrap();
        assert!(config_directory(&root, 42).is_err());

        fs::remove_file(root.join("userdata/42/config")).unwrap();
        fs::create_dir_all(root.join("userdata/42/config")).unwrap();
        let config = config_directory(&root, 42).unwrap();
        std::os::unix::fs::symlink(outside.join("target"), config.join(FILE_NAME)).unwrap();
        assert!(read_shortcuts(&config).is_err());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
