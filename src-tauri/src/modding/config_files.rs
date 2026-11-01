//! Config files inside a mod profile: listing, safe access and BepInEx `.cfg` editing.

use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use super::paths::validate_relative_path;
use super::profile_commands::run_modding;
use super::profiles::profile_for_game;

const MAX_LISTED_FILES: usize = 5000;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub(crate) const CONFIG_EXTENSIONS: [&str; 6] = ["cfg", "txt", "json", "yml", "yaml", "ini"];
const EXCLUDED_BEPINEX_FOLDERS: [&str; 4] = ["core", "plugins", "patchers", "cache"];
const EXCLUDED_FILE_NAMES: [&str; 2] = ["doorstop_config.ini", "manifest.json"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigFileEntry {
    pub(crate) relative_path: String,
    pub(crate) size: u64,
    pub(crate) modified: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModConfigFile {
    pub(crate) relative_path: String,
    pub(crate) text: String,
    pub(crate) bepinex: Option<BepInExConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BepInExConfig {
    pub(crate) sections: Vec<ConfigSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigSection {
    pub(crate) name: String,
    pub(crate) entries: Vec<ConfigEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigEntry {
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) description: Option<String>,
    pub(crate) setting_type: Option<String>,
    pub(crate) default_value: Option<String>,
    pub(crate) acceptable_values: Vec<String>,
    pub(crate) acceptable_range: Option<(String, String)>,
    /// 0-based index of the `key = value` line.
    pub(crate) line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigEdit {
    pub(crate) section: String,
    pub(crate) key: String,
    pub(crate) value: String,
    /// 0-based line of the entry, as listed in [`ConfigEntry::line`].
    pub(crate) line: usize,
}

pub(crate) fn list_config_files(profile_dir: &Path) -> Result<Vec<ConfigFileEntry>, String> {
    if !profile_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    let mut directories = vec![(profile_dir.to_path_buf(), String::new())];
    while let Some((directory, prefix)) = directories.pop() {
        let children = fs::read_dir(&directory)
            .map_err(|error| format!("Could not read mod profile folder: {error}"))?;
        for child in children {
            let child =
                child.map_err(|error| format!("Could not read mod profile folder: {error}"))?;
            let file_type = child
                .file_type()
                .map_err(|error| format!("Could not inspect mod profile file: {error}"))?;
            if file_type.is_symlink() {
                continue;
            }
            let name = child.file_name();
            let name = name
                .to_str()
                .ok_or("Mod profile contains a file name that is not UTF-8")?;
            let relative_path = if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix}/{name}")
            };
            if file_type.is_dir() {
                if !is_under_excluded_folder(&relative_path) {
                    directories.push((child.path(), relative_path));
                }
            } else if file_type.is_file() && is_listed_path(&relative_path) {
                if entries.len() == MAX_LISTED_FILES {
                    return Err(format!(
                        "Mod profile has more than {MAX_LISTED_FILES} config files"
                    ));
                }
                let metadata = child
                    .metadata()
                    .map_err(|error| format!("Could not inspect mod profile file: {error}"))?;
                entries.push(ConfigFileEntry {
                    relative_path,
                    size: metadata.len(),
                    modified: modified_seconds(&metadata)?,
                });
            }
        }
    }
    entries.sort_by(|left, right| {
        left.relative_path
            .to_lowercase()
            .cmp(&right.relative_path.to_lowercase())
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    Ok(entries)
}

pub(crate) fn read_config_text(profile_dir: &Path, relative_path: &str) -> Result<String, String> {
    let path = resolve_config_path(profile_dir, relative_path)?;
    let size = fs::metadata(&path)
        .map_err(|error| format!("Could not inspect config file: {error}"))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err("Config file is larger than 2 MiB".to_owned());
    }
    let bytes = fs::read(&path).map_err(|error| format!("Could not read config file: {error}"))?;
    let text = String::from_utf8(bytes)
        .map_err(|error| format!("Config file is not UTF-8 text: {error}"))?;
    Ok(text
        .strip_prefix('\u{feff}')
        .unwrap_or(text.as_str())
        .to_owned())
}

pub(crate) fn write_config_text(
    profile_dir: &Path,
    relative_path: &str,
    content: &str,
) -> Result<(), String> {
    let path = resolve_config_path(profile_dir, relative_path)?;
    if content.len() as u64 > MAX_FILE_BYTES {
        return Err("Config file would be larger than 2 MiB".to_owned());
    }
    crate::atomic_file::write_atomic(&path, content.as_bytes())
        .map_err(|error| format!("Could not write config file: {error}"))
}

pub(crate) fn read_mod_config(
    profile_dir: &Path,
    relative_path: &str,
) -> Result<ModConfigFile, String> {
    let text = read_config_text(profile_dir, relative_path)?;
    let bepinex = is_bepinex_cfg(relative_path).then(|| parse_bepinex_cfg(&text));
    Ok(ModConfigFile {
        relative_path: relative_path.to_owned(),
        text,
        bepinex,
    })
}

pub(crate) fn save_config_entries(
    profile_dir: &Path,
    relative_path: &str,
    edits: &[ConfigEdit],
) -> Result<(), String> {
    if !is_bepinex_cfg(relative_path) {
        return Err("Only .cfg files have editable entries".to_owned());
    }
    let text = read_config_text(profile_dir, relative_path)?;
    let updated = apply_cfg_edits(&text, edits)?;
    write_config_text(profile_dir, relative_path, &updated)
}

pub(crate) fn delete_config_file(profile_dir: &Path, relative_path: &str) -> Result<(), String> {
    let path = resolve_config_path(profile_dir, relative_path)?;
    fs::remove_file(path).map_err(|error| format!("Could not delete config file: {error}"))
}

/// Opens the file with the application the system associates with it.
pub(crate) fn open_config_file(profile_dir: &Path, relative_path: &str) -> Result<(), String> {
    let path = resolve_config_path(profile_dir, relative_path)?;
    crate::installed_folder::open_file(&path)
}

/// Resolves a listed, existing config file and checks that it stays inside the profile.
fn resolve_config_path(profile_dir: &Path, relative_path: &str) -> Result<PathBuf, String> {
    validate_relative_path(relative_path).map_err(|_| "Config file path is invalid".to_owned())?;
    if !is_listed_path(relative_path) {
        return Err("Config file type is not supported".to_owned());
    }
    let root = dunce::canonicalize(profile_dir)
        .map_err(|error| format!("Could not open mod profile: {error}"))?;
    let path = dunce::canonicalize(profile_dir.join(relative_path))
        .map_err(|error| format!("Could not open config file: {error}"))?;
    if !path.starts_with(&root) {
        return Err("Config file is outside the mod profile".to_owned());
    }
    if !path.is_file() {
        return Err("Config path is not a file".to_owned());
    }
    Ok(path)
}

/// True for paths the listing shows: allowed extension, not an excluded file or folder.
fn is_listed_path(relative_path: &str) -> bool {
    let name = relative_path.rsplit('/').next().unwrap_or(relative_path);
    !is_under_excluded_folder(relative_path) && is_config_name(name)
}

fn is_config_name(name: &str) -> bool {
    if EXCLUDED_FILE_NAMES
        .iter()
        .any(|excluded| name.eq_ignore_ascii_case(excluded))
    {
        return false;
    }
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            CONFIG_EXTENSIONS
                .iter()
                .any(|allowed| extension.eq_ignore_ascii_case(allowed))
        })
}

fn is_under_excluded_folder(relative_path: &str) -> bool {
    let mut parts = relative_path.split('/');
    parts
        .next()
        .is_some_and(|part| part.eq_ignore_ascii_case("BepInEx"))
        && parts.next().is_some_and(|folder| {
            EXCLUDED_BEPINEX_FOLDERS
                .iter()
                .any(|excluded| folder.eq_ignore_ascii_case(excluded))
        })
}

fn is_bepinex_cfg(relative_path: &str) -> bool {
    Path::new(relative_path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cfg"))
}

fn modified_seconds(metadata: &fs::Metadata) -> Result<i64, String> {
    let modified = metadata
        .modified()
        .map_err(|error| format!("Could not read config file time: {error}"))?;
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|age| i64::try_from(age.as_secs()).ok())
        .ok_or_else(|| "Config file time is outside the supported range".to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeyLine<'a> {
    key: &'a str,
    /// Byte index of the `=` in the line.
    equals: usize,
}

fn key_line(line: &str) -> Option<KeyLine<'_>> {
    let start = line.len() - line.trim_start().len();
    let rest = &line[start..];
    if rest.starts_with('#') {
        return None;
    }
    let equals = rest.find('=')?;
    let key = rest[..equals].trim_end();
    (!key.is_empty()).then_some(KeyLine {
        key,
        equals: start + equals,
    })
}

fn section_name(line: &str) -> Option<&str> {
    line.strip_prefix('[')?.strip_suffix(']')
}

pub(crate) fn parse_bepinex_cfg(text: &str) -> BepInExConfig {
    let mut config = BepInExConfig {
        sections: Vec::new(),
    };
    let mut pending = PendingEntry::default();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.starts_with('#') {
            pending.add_comment(line);
            continue;
        }
        // Comments only attach to the key line directly below them.
        let attached = std::mem::take(&mut pending);
        if let Some(name) = section_name(line) {
            config.sections.push(ConfigSection {
                name: name.to_owned(),
                entries: Vec::new(),
            });
            continue;
        }
        let (Some(section), Some(key)) = (config.sections.last_mut(), key_line(raw)) else {
            continue;
        };
        let value = raw[key.equals + 1..].trim();
        section
            .entries
            .push(attached.into_entry(key.key, value, index));
    }
    config
}

/// Replaces only the values of existing entries. Every other byte is kept.
/// Each edit names the line of its entry; the key and section found there
/// must match, otherwise the file changed since it was read.
pub(crate) fn apply_cfg_edits(text: &str, edits: &[ConfigEdit]) -> Result<String, String> {
    let config = parse_bepinex_cfg(text);
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_owned).collect();
    for edit in edits {
        if edit.value.contains(['\r', '\n']) {
            return Err("Config values cannot contain line breaks".to_owned());
        }
        let matches_edit = |section: &ConfigSection, entry: &ConfigEntry| {
            entry.line == edit.line && entry.key == edit.key && section.name == edit.section
        };
        if !config.sections.iter().any(|section| {
            section
                .entries
                .iter()
                .any(|entry| matches_edit(section, entry))
        }) {
            return Err("The config file changed on disk; reload it".to_owned());
        }
        let line = lines
            .get_mut(edit.line)
            .ok_or("The config file changed on disk; reload it")?;
        *line = replace_value(line, &edit.value)?;
    }
    Ok(lines.concat())
}

fn replace_value(line: &str, value: &str) -> Result<String, String> {
    let content = line.trim_end_matches(['\r', '\n']);
    let equals = key_line(content)
        .ok_or("Config entry line is invalid")?
        .equals;
    let after = &content[equals + 1..];
    let value_start = equals + 1 + (after.len() - after.trim_start().len());
    let value_end = (equals + 1 + after.trim_end().len()).max(value_start);
    let separator = if after.trim().is_empty() && value_start == equals + 1 && !value.is_empty() {
        " "
    } else {
        ""
    };
    Ok(format!(
        "{}{separator}{value}{}",
        &line[..value_start],
        &line[value_end..]
    ))
}

fn parse_range(text: &str) -> Option<(String, String)> {
    let (from, to) = text.strip_prefix("From ")?.split_once(" To ")?;
    Some((from.trim().to_owned(), to.trim().to_owned()))
}

#[derive(Debug, Default)]
struct PendingEntry {
    description: Vec<String>,
    setting_type: Option<String>,
    default_value: Option<String>,
    acceptable_values: Vec<String>,
    acceptable_range: Option<(String, String)>,
}

impl PendingEntry {
    /// `line` is a trimmed line starting with `#`.
    fn add_comment(&mut self, line: &str) {
        if let Some(text) = line.strip_prefix("##") {
            self.description.push(text.trim().to_owned());
        } else if let Some(text) = line.strip_prefix('#') {
            let text = text.trim();
            if let Some(value) = text.strip_prefix("Setting type:") {
                self.setting_type = Some(value.trim().to_owned());
            } else if let Some(value) = text.strip_prefix("Default value:") {
                self.default_value = Some(value.trim().to_owned());
            } else if let Some(value) = text.strip_prefix("Acceptable values:") {
                self.acceptable_values = value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .collect();
            } else if let Some(value) = text.strip_prefix("Acceptable value range:") {
                self.acceptable_range = parse_range(value.trim());
            }
        }
    }

    fn into_entry(self, key: &str, value: &str, line: usize) -> ConfigEntry {
        let description = self.description.join("\n").trim().to_owned();
        ConfigEntry {
            key: key.to_owned(),
            value: value.to_owned(),
            description: (!description.is_empty()).then_some(description),
            setting_type: self.setting_type,
            default_value: self.default_value,
            acceptable_values: self.acceptable_values,
            acceptable_range: self.acceptable_range,
            line,
        }
    }
}

#[tauri::command]
pub async fn list_mod_config_files(
    app: AppHandle,
    game_id: String,
    profile_id: String,
) -> Result<Vec<ConfigFileEntry>, String> {
    run_modding(app, move |_, database, paths, _| {
        let (_, directory) = profile_for_game(database, paths, &game_id, &profile_id)?;
        list_config_files(&directory)
    })
    .await
}

#[tauri::command]
pub async fn read_mod_config_file(
    app: AppHandle,
    game_id: String,
    profile_id: String,
    relative_path: String,
) -> Result<ModConfigFile, String> {
    run_modding(app, move |_, database, paths, _| {
        let (_, directory) = profile_for_game(database, paths, &game_id, &profile_id)?;
        read_mod_config(&directory, &relative_path)
    })
    .await
}

#[tauri::command]
pub async fn save_mod_config_text(
    app: AppHandle,
    game_id: String,
    profile_id: String,
    relative_path: String,
    text: String,
) -> Result<(), String> {
    run_modding(app, move |_, database, paths, _| {
        let (_, directory) = profile_for_game(database, paths, &game_id, &profile_id)?;
        write_config_text(&directory, &relative_path, &text)
    })
    .await
}

#[tauri::command]
pub async fn save_mod_config_entries(
    app: AppHandle,
    game_id: String,
    profile_id: String,
    relative_path: String,
    edits: Vec<ConfigEdit>,
) -> Result<(), String> {
    run_modding(app, move |_, database, paths, _| {
        let (_, directory) = profile_for_game(database, paths, &game_id, &profile_id)?;
        save_config_entries(&directory, &relative_path, &edits)
    })
    .await
}

#[tauri::command]
pub async fn delete_mod_config_file(
    app: AppHandle,
    game_id: String,
    profile_id: String,
    relative_path: String,
) -> Result<(), String> {
    run_modding(app, move |_, database, paths, _| {
        let (_, directory) = profile_for_game(database, paths, &game_id, &profile_id)?;
        delete_config_file(&directory, &relative_path)?;
        crate::application_log::state_change("mod_config_deleted", &relative_path);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn open_mod_config_file(
    app: AppHandle,
    game_id: String,
    profile_id: String,
    relative_path: String,
) -> Result<(), String> {
    run_modding(app, move |_, database, paths, _| {
        let (_, directory) = profile_for_game(database, paths, &game_id, &profile_id)?;
        open_config_file(&directory, &relative_path)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    const EXAMPLE: &str = "## Settings file was created by plugin X v1.0\n\
## Plugin GUID: x.y\n\
\n\
[General]\n\
\n\
## Enables the thing\n\
# Setting type: Boolean\n\
# Default value: true\n\
Enabled = true\n\
\n\
## Pick a mode\n\
# Setting type: Mode\n\
# Default value: Fast\n\
# Acceptable values: Fast, Slow\n\
Mode = Fast\n\
\n\
[Audio]\n\
## Volume\n\
# Setting type: Int32\n\
# Default value: 50\n\
# Acceptable value range: From 0 To 100\n\
Volume = 50\n";

    /// Removes its folder when dropped, including after a failed assertion.
    struct TempProfile(PathBuf);

    impl TempProfile {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("legio-modding-config-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, relative_path: &str, bytes: &[u8]) {
            let path = self.0.join(relative_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
    }

    impl Drop for TempProfile {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn deletes_only_listed_files_inside_the_profile() {
        let profile = TempProfile::new();
        profile.write("BepInEx/config/a.cfg", b"x");
        profile.write("BepInEx/plugins/Mod.dll", b"x");
        let outside = profile.0.with_extension("outside.cfg");
        fs::write(&outside, b"x").unwrap();
        let outside_name = format!("../{}", outside.file_name().unwrap().to_str().unwrap());

        for path in [
            "BepInEx/plugins/Mod.dll",
            "BepInEx/config",
            "../x.cfg",
            "missing.cfg",
            &outside_name,
        ] {
            assert!(delete_config_file(&profile.0, path).is_err(), "{path}");
        }
        assert!(profile.0.join("BepInEx/plugins/Mod.dll").is_file());
        assert!(outside.is_file());
        fs::remove_file(&outside).unwrap();

        delete_config_file(&profile.0, "BepInEx/config/a.cfg").unwrap();
        assert!(!profile.0.join("BepInEx/config/a.cfg").exists());
        assert!(profile.0.join("BepInEx/config").is_dir());
    }

    #[test]
    fn open_refuses_unsafe_paths_before_launching() {
        let profile = TempProfile::new();
        profile.write("BepInEx/plugins/Mod.dll", b"x");
        for path in [
            "../a.cfg",
            r"a\b.cfg",
            "BepInEx/plugins/Mod.dll",
            "missing.cfg",
        ] {
            assert!(open_config_file(&profile.0, path).is_err(), "{path}");
        }
    }

    fn edit(section: &str, key: &str, value: &str, line: usize) -> ConfigEdit {
        ConfigEdit {
            section: section.to_owned(),
            key: key.to_owned(),
            value: value.to_owned(),
            line,
        }
    }

    #[test]
    fn parses_bepinex_entries() {
        let config = parse_bepinex_cfg(EXAMPLE);
        let names: Vec<_> = config.sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["General", "Audio"]);

        let enabled = &config.sections[0].entries[0];
        assert_eq!(enabled.key, "Enabled");
        assert_eq!(enabled.value, "true");
        assert_eq!(enabled.description.as_deref(), Some("Enables the thing"));
        assert_eq!(enabled.setting_type.as_deref(), Some("Boolean"));
        assert_eq!(enabled.default_value.as_deref(), Some("true"));
        assert!(enabled.acceptable_values.is_empty());
        assert_eq!(enabled.acceptable_range, None);
        assert_eq!(enabled.line, 8);

        let mode = &config.sections[0].entries[1];
        assert_eq!(mode.description.as_deref(), Some("Pick a mode"));
        assert_eq!(mode.acceptable_values, ["Fast", "Slow"]);
        assert_eq!(mode.line, 14);

        let volume = &config.sections[1].entries[0];
        assert_eq!(volume.setting_type.as_deref(), Some("Int32"));
        assert_eq!(
            volume.acceptable_range,
            Some(("0".to_owned(), "100".to_owned()))
        );
        assert_eq!(volume.line, 21);
    }

    #[test]
    fn keeps_header_comments_out_of_entries() {
        let config = parse_bepinex_cfg("## Header\n[A]\nKey = 1\n");
        assert_eq!(config.sections[0].entries[0].description, None);
    }

    #[test]
    fn applies_edits_preserving_bytes() {
        let crlf = EXAMPLE.replace('\n', "\r\n");
        let updated = apply_cfg_edits(
            &crlf,
            &[
                edit("General", "Enabled", "false", 8),
                edit("Audio", "Volume", "75", 21),
            ],
        )
        .unwrap();
        assert_eq!(
            updated,
            crlf.replace("Enabled = true", "Enabled = false")
                .replace("Volume = 50", "Volume = 75")
        );
    }

    #[test]
    fn keeps_original_spacing_around_equals() {
        let updated = apply_cfg_edits(
            "[A]\r\nKey=old   \r\nOther = x\r\n",
            &[edit("A", "Key", "new", 1)],
        )
        .unwrap();
        assert_eq!(updated, "[A]\r\nKey=new   \r\nOther = x\r\n");
    }

    #[test]
    fn rejects_stale_edits_and_line_breaks() {
        let stale = "The config file changed on disk; reload it";
        for wrong in [
            edit("General", "Missing", "1", 8),
            edit("Missing", "Enabled", "false", 8),
            edit("General", "Enabled", "false", 7),
            edit("General", "Enabled", "false", 999),
            edit("Audio", "Enabled", "false", 8),
        ] {
            assert_eq!(apply_cfg_edits(EXAMPLE, &[wrong]).unwrap_err(), stale);
        }
        assert!(apply_cfg_edits(EXAMPLE, &[edit("General", "Enabled", "a\nb", 8)]).is_err());
        assert!(apply_cfg_edits(EXAMPLE, &[edit("General", "Enabled", "a\rb", 8)]).is_err());
    }

    #[test]
    fn edits_the_named_line_among_duplicate_keys_and_sections() {
        let text = "[A]\nKey = 1\nKey = 2\n[A]\nKey = 3\n";
        let config = parse_bepinex_cfg(text);
        let lines: Vec<_> = config
            .sections
            .iter()
            .flat_map(|section| section.entries.iter().map(|entry| entry.line))
            .collect();
        assert_eq!(lines, [1, 2, 4]);

        assert_eq!(
            apply_cfg_edits(text, &[edit("A", "Key", "x", 2)]).unwrap(),
            "[A]\nKey = 1\nKey = x\n[A]\nKey = 3\n"
        );
        assert_eq!(
            apply_cfg_edits(text, &[edit("A", "Key", "y", 4)]).unwrap(),
            "[A]\nKey = 1\nKey = 2\n[A]\nKey = y\n"
        );
        assert_eq!(
            apply_cfg_edits(text, &[edit("A", "Key", "p", 1), edit("A", "Key", "q", 4)]).unwrap(),
            "[A]\nKey = p\nKey = 2\n[A]\nKey = q\n"
        );
    }

    #[test]
    fn rejects_unsafe_paths() {
        let profile = TempProfile::new();
        profile.write("config/a.cfg", b"x=1\n");
        profile.write("config/a.exe", b"");
        for path in [
            "",
            "/config/a.cfg",
            "\\config\\a.cfg",
            "config\\a.cfg",
            "config/../a.cfg",
            "../config/a.cfg",
            "./config/a.cfg",
            "config/./a.cfg",
            "config//a.cfg",
            "C:/config/a.cfg",
            "config/a.cfg:stream",
            "config/a.exe",
            "config/missing.cfg",
            "BepInEx/core/a.cfg",
            "doorstop_config.ini",
        ] {
            assert!(resolve_config_path(&profile.0, path).is_err(), "{path}");
        }
        assert!(resolve_config_path(&profile.0, "config/a.cfg").is_ok());
    }

    #[test]
    fn lists_profile_config_files() {
        let profile = TempProfile::new();
        profile.write("alpha.txt", b"a");
        profile.write("config/a.cfg", b"x=1\n");
        profile.write("BepInEx/config/b.JSON", b"{}");
        profile.write("BepInEx/core/c.cfg", b"");
        profile.write("bepinex/CORE/c2.cfg", b"");
        profile.write("BepInEx/plugins/d.txt", b"");
        profile.write("BepInEx/patchers/e.yml", b"");
        profile.write("BepInEx/cache/f.yaml", b"");
        profile.write("doorstop_config.ini", b"");
        profile.write("MANIFEST.json", b"");
        profile.write("readme.md", b"");
        profile.write("plugin.dll", b"");

        let entries = list_config_files(&profile.0).unwrap();
        let paths: Vec<_> = entries.iter().map(|e| e.relative_path.as_str()).collect();
        assert_eq!(
            paths,
            ["alpha.txt", "BepInEx/config/b.JSON", "config/a.cfg"]
        );
        assert_eq!(entries[2].size, 4);
        assert!(entries[2].modified > 0);
    }

    #[test]
    fn lists_nothing_for_missing_profile() {
        let profile = TempProfile::new();
        let missing = profile.0.join("missing");
        assert!(list_config_files(&missing).unwrap().is_empty());
    }

    #[test]
    fn caps_listed_files() {
        let profile = TempProfile::new();
        for index in 0..=MAX_LISTED_FILES {
            profile.write(&format!("many/{index:05}.cfg"), b"");
        }
        assert!(list_config_files(&profile.0).is_err());
        fs::remove_file(profile.0.join("many/00000.cfg")).unwrap();
        assert_eq!(
            list_config_files(&profile.0).unwrap().len(),
            MAX_LISTED_FILES
        );
    }

    #[test]
    fn caps_file_size() {
        let profile = TempProfile::new();
        let limit = usize::try_from(MAX_FILE_BYTES).unwrap();
        profile.write("config/full.cfg", &vec![b'a'; limit]);
        profile.write("config/over.cfg", &vec![b'a'; limit + 1]);
        assert_eq!(
            read_config_text(&profile.0, "config/full.cfg")
                .unwrap()
                .len(),
            limit
        );
        assert!(read_config_text(&profile.0, "config/over.cfg").is_err());
        assert!(write_config_text(&profile.0, "config/full.cfg", &"b".repeat(limit + 1)).is_err());
    }

    #[test]
    fn strips_bom_and_replaces_existing_files() {
        let profile = TempProfile::new();
        profile.write("config/bom.cfg", b"\xEF\xBB\xBFa = b\n");
        assert_eq!(
            read_config_text(&profile.0, "config/bom.cfg").unwrap(),
            "a = b\n"
        );
        write_config_text(&profile.0, "config/bom.cfg", "x = 1\n").unwrap();
        assert_eq!(
            read_config_text(&profile.0, "config/bom.cfg").unwrap(),
            "x = 1\n"
        );
        let names: Vec<_> = fs::read_dir(profile.0.join("config"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1);
    }

    #[test]
    fn saves_entries_only_for_cfg_files() {
        let profile = TempProfile::new();
        profile.write("config/a.cfg", b"[A]\nKey = 1\n");
        profile.write("config/a.txt", b"[A]\nKey = 1\n");
        save_config_entries(&profile.0, "config/a.cfg", &[edit("A", "Key", "2", 1)]).unwrap();
        assert_eq!(
            read_config_text(&profile.0, "config/a.cfg").unwrap(),
            "[A]\nKey = 2\n"
        );
        assert!(save_config_entries(&profile.0, "config/a.txt", &[]).is_err());
    }
}
