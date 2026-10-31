//! Profile exchange in the r2modman `.r2z` format: a zip holding `export.r2x`
//! (YAML mod list) plus the profile's config files.

use std::{
    collections::HashSet,
    fs,
    io::{self, Cursor, Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::package_id::{PackageName, PackageVersionId, Version};

const MANIFEST_ENTRY: &str = "export.r2x";
const CONFIG_ENTRY_DIR: &str = "config";
const SUPPORTED_EXTENSIONS: [&str; 6] = ["cfg", "txt", "json", "yml", "yaml", "ini"];
const BLOCKED_EXTENSIONS: [&str; 4] = ["dll", "exe", "bat", "reg"];
const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;
const MAX_TOTAL_BYTES: u64 = 128 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_COMPRESSION_RATIO: u64 = 200;
const MAX_MODS: usize = 5_000;
const MAX_PROFILE_NAME_CHARS: usize = 64;

/// A mod to write into the export, as tracked by the profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportedMod {
    pub(crate) package: PackageName,
    pub(crate) version: Version,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedMod {
    pub(crate) id: PackageVersionId,
    pub(crate) enabled: bool,
}

/// A config file from an archive, already mapped to a profile-relative path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedFile {
    pub(crate) relative_path: String,
    pub(crate) contents: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProfileImport {
    pub(crate) profile_name: String,
    pub(crate) mods: Vec<ImportedMod>,
    pub(crate) files: Vec<ImportedFile>,
    /// Archive entries ignored because their type is not an allowed config file.
    pub(crate) skipped: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestOut<'a> {
    profile_name: &'a str,
    mods: Vec<ManifestModOut>,
}

#[derive(Serialize)]
struct ManifestModOut {
    name: String,
    version: Version,
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestIn {
    profile_name: String,
    mods: Vec<ManifestModIn>,
}

#[derive(Deserialize)]
struct ManifestModIn {
    name: String,
    version: Version,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

/// Builds the `.r2z` bytes for a profile.
pub(crate) fn export_profile(
    profile_dir: &Path,
    profile_name: &str,
    mods: &[ExportedMod],
) -> Result<Vec<u8>, String> {
    let manifest = ManifestOut {
        profile_name,
        mods: mods
            .iter()
            .map(|entry| ManifestModOut {
                name: entry.package.to_string(),
                version: entry.version,
                enabled: entry.enabled,
            })
            .collect(),
    };
    let manifest = serde_yaml_ng::to_string(&manifest)
        .map_err(|error| format!("Could not write the profile mod list: {error}"))?;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    add_entry(&mut writer, MANIFEST_ENTRY, manifest.as_bytes(), options)?;
    for (entry_name, path) in collect_export_files(profile_dir)? {
        let contents = fs::read(&path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        add_entry(&mut writer, &entry_name, &contents, options)?;
    }
    writer
        .finish()
        .map(Cursor::into_inner)
        .map_err(|error| format!("Could not finish the profile archive: {error}"))
}

/// Exports a profile to a user-chosen file.
pub(crate) fn export_profile_to_file(
    profile_dir: &Path,
    profile_name: &str,
    mods: &[ExportedMod],
    destination: &Path,
) -> Result<(), String> {
    let bytes = export_profile(profile_dir, profile_name, mods)?;
    fs::write(destination, bytes)
        .map_err(|error| format!("Could not write {}: {error}", destination.display()))
}

fn add_entry(
    writer: &mut ZipWriter<Cursor<Vec<u8>>>,
    name: &str,
    contents: &[u8],
    options: SimpleFileOptions,
) -> Result<(), String> {
    writer
        .start_file(name, options)
        .map_err(|error| format!("Could not add {name} to the profile archive: {error}"))?;
    writer
        .write_all(contents)
        .map_err(|error| format!("Could not add {name} to the profile archive: {error}"))
}

/// Mirrors r2modman: config files and a few text-like files, never binaries,
/// loader runtimes or mod manifests.
fn collect_export_files(profile_dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut found = Vec::new();
    let mut pending = vec![(profile_dir.to_path_buf(), Vec::<String>::new())];
    while let Some((directory, relative)) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound && relative.is_empty() => {
                return Ok(found);
            }
            Err(error) => return Err(format!("Could not read {}: {error}", directory.display())),
        };
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("Could not read {}: {error}", directory.display()))?;
            let file_type = entry.file_type().map_err(|error| {
                format!("Could not inspect {}: {error}", entry.path().display())
            })?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let mut child = relative.clone();
            child.push(name);
            if file_type.is_dir() {
                if !is_excluded_directory(&child) {
                    pending.push((entry.path(), child));
                }
            } else if file_type.is_file()
                && let Some(entry_name) = export_entry_name(&child)
            {
                found.push((entry_name, entry.path()));
            }
        }
    }
    found.sort();
    Ok(found)
}

fn lower(parts: &[String]) -> Vec<String> {
    parts.iter().map(|part| part.to_ascii_lowercase()).collect()
}

fn is_excluded_directory(parts: &[String]) -> bool {
    let parts = lower(parts);
    match parts.as_slice() {
        [root] => matches!(root.as_str(), "dotnet" | "_state" | "melonloader"),
        [root, child] => root == "gdweave" && matches!(child.as_str(), "core" | "mods"),
        _ => false,
    }
}

fn export_entry_name(parts: &[String]) -> Option<String> {
    let lowered = lower(parts);
    let file_name = lowered.last()?;
    if !has_supported_extension(file_name) || file_name.ends_with("mods.yml") {
        return None;
    }
    if lowered.len() == 4 && lowered[0] == "bepinex" && lowered[1] == "plugins" {
        if file_name == "manifest.json" {
            return None;
        }
    }
    if lowered.len() >= 3 && lowered[0] == "bepinex" && lowered[1] == "config" {
        return Some(format!("{CONFIG_ENTRY_DIR}/{}", parts[2..].join("/")));
    }
    Some(parts.join("/"))
}

fn has_supported_extension(file_name: &str) -> bool {
    extension(file_name).is_some_and(|ext| SUPPORTED_EXTENSIONS.contains(&ext.as_str()))
}

fn extension(file_name: &str) -> Option<String> {
    file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
}

/// Reads and validates an untrusted `.r2z` from disk.
pub(crate) fn read_r2z_file(path: &Path) -> Result<ProfileImport, String> {
    let length = fs::metadata(path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?
        .len();
    if length > MAX_ARCHIVE_BYTES as u64 {
        return Err("The profile file is too large".to_owned());
    }
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    read_r2z(&bytes)
}

/// Reads and validates an untrusted `.r2z`.
pub(crate) fn read_r2z(bytes: &[u8]) -> Result<ProfileImport, String> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err("The profile file is too large".to_owned());
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("The profile file is not a valid archive: {error}"))?;
    if archive.len() > MAX_ENTRIES {
        return Err("The profile file has too many entries".to_owned());
    }

    let mut manifest = None;
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("The profile file has an unreadable entry: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let raw_name = entry
            .name()
            .map_err(|_| "The profile file has an entry with an invalid name".to_owned())?
            .into_owned();
        let size = entry.size();
        total = total.saturating_add(size);
        if total > MAX_TOTAL_BYTES {
            return Err("The profile file expands to too much data".to_owned());
        }
        if size > MAX_COMPRESSION_RATIO.saturating_mul(entry.compressed_size().max(1)) {
            return Err("The profile file has a suspicious compression ratio".to_owned());
        }
        let name = normalize_entry_path(&raw_name)?;
        let lowered = name.to_ascii_lowercase();
        if lowered == MANIFEST_ENTRY {
            if size > MAX_MANIFEST_BYTES {
                return Err("The profile mod list is too large".to_owned());
            }
            manifest = Some(read_entry(&mut entry, size, &raw_name)?);
            continue;
        }
        if lowered == "mods.yml" {
            continue;
        }
        let file_name = lowered.rsplit('/').next().unwrap_or_default();
        let blocked =
            extension(file_name).is_some_and(|ext| BLOCKED_EXTENSIONS.contains(&ext.as_str()));
        if blocked || !has_supported_extension(file_name) {
            skipped.push(name);
            continue;
        }
        let relative_path = map_to_profile_path(&name);
        if !seen.insert(relative_path.to_ascii_lowercase()) {
            return Err(format!("The profile file has a duplicate entry: {name}"));
        }
        let contents = read_entry(&mut entry, size, &raw_name)?;
        files.push(ImportedFile {
            relative_path,
            contents,
        });
    }

    let manifest = manifest.ok_or_else(|| "The profile file has no export.r2x".to_owned())?;
    let manifest: ManifestIn = serde_yaml_ng::from_slice(&manifest)
        .map_err(|error| format!("The profile mod list is invalid: {error}"))?;
    if manifest.mods.len() > MAX_MODS {
        return Err("The profile lists too many mods".to_owned());
    }
    let mods = manifest
        .mods
        .into_iter()
        .map(|entry| {
            Ok(ImportedMod {
                id: PackageVersionId::new(entry.name.parse()?, entry.version),
                enabled: entry.enabled,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ProfileImport {
        profile_name: sanitize_profile_name(&manifest.profile_name)?,
        mods,
        files,
        skipped,
    })
}

/// Reads exactly the declared size and fails if the entry holds more.
fn read_entry(entry: &mut impl Read, declared: u64, name: &str) -> Result<Vec<u8>, String> {
    let mut contents = Vec::new();
    entry
        .take(declared.saturating_add(1))
        .read_to_end(&mut contents)
        .map_err(|error| format!("Could not read {name} from the profile file: {error}"))?;
    if contents.len() as u64 > declared {
        return Err(format!("{name} is larger than the archive declares"));
    }
    Ok(contents)
}

/// Converts an archive entry name to a safe relative path with `/` separators.
fn normalize_entry_path(raw: &str) -> Result<String, String> {
    let invalid = || format!("The profile file has an unsafe path: {raw}");
    let normalized = raw.replace('\\', "/");
    if normalized.starts_with('/') {
        return Err(invalid());
    }
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.contains(':')
            || part.chars().any(char::is_control)
            || part.ends_with(' ')
            || part.ends_with('.')
        {
            return Err(invalid());
        }
        parts.push(part);
    }
    Ok(parts.join("/"))
}

fn map_to_profile_path(name: &str) -> String {
    match name.split_once('/') {
        Some((first, rest)) if first.eq_ignore_ascii_case(CONFIG_ENTRY_DIR) => {
            format!("BepInEx/config/{rest}")
        }
        _ => name.to_owned(),
    }
}

/// Makes an archive-provided name usable as a Legio profile name.
fn sanitize_profile_name(raw: &str) -> Result<String, String> {
    let cleaned: String = raw
        .chars()
        .map(|character| {
            if character == '/' || character == '\\' || character.is_control() {
                '_'
            } else {
                character
            }
        })
        .take(MAX_PROFILE_NAME_CHARS)
        .collect();
    let cleaned = cleaned.trim().to_owned();
    if cleaned.is_empty() {
        return Err("The profile has no name".to_owned());
    }
    Ok(cleaned)
}

/// Writes the imported config files into `profile_dir` (a staging directory
/// for new profiles). Paths were validated when the archive was read.
pub(crate) fn apply_import_files(import: &ProfileImport, profile_dir: &Path) -> Result<(), String> {
    for file in &import.files {
        let target = profile_dir.join(&file.relative_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
        }
        fs::write(&target, &file.contents)
            .map_err(|error| format!("Could not write {}: {error}", target.display()))?;
    }
    Ok(())
}

/// Moves a fully built staging directory into place. The target must not exist.
pub(crate) fn promote_staged_profile(staging_dir: &Path, profile_dir: &Path) -> Result<(), String> {
    if profile_dir.exists() {
        return Err("The profile directory already exists".to_owned());
    }
    if let Some(parent) = profile_dir.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }
    fs::rename(staging_dir, profile_dir)
        .map_err(|error| format!("Could not move the imported profile into place: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("legio-exchange-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, contents) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    const MANIFEST: &[u8] = b"profileName: Test\nmods: []\n";

    fn entry_names(bytes: &[u8]) -> Vec<String> {
        let archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut names: Vec<String> = archive
            .file_names()
            .map(|name| name.unwrap().into_owned())
            .collect();
        names.sort();
        names
    }

    fn sample_mod(enabled: bool) -> ExportedMod {
        ExportedMod {
            package: "RiskofThunder-R2API_Core".parse().unwrap(),
            version: Version::new(5, 1, 4),
            enabled,
        }
    }

    #[test]
    fn exports_expected_files() {
        let root = temp_dir();
        write(&root, "BepInEx/config/a.cfg", "a");
        write(&root, "BepInEx/config/sub/b.json", "b");
        write(&root, "BepInEx/config/lib.dll", "binary");
        write(&root, "BepInEx/plugins/Mod/manifest.json", "m");
        write(&root, "BepInEx/plugins/Mod/settings.json", "s");
        write(&root, "BepInEx/plugins/Mod/Mod.dll", "binary");
        write(&root, "BepInEx/core/core.json", "c");
        write(&root, "doorstop_config.ini", "d");
        write(&root, "mods.yml", "x");
        write(&root, "dotnet/runtime.json", "x");
        write(&root, "_state/state.json", "x");
        write(&root, "MelonLoader/ml.cfg", "x");
        write(&root, "GDWeave/core/a.json", "x");
        write(&root, "GDWeave/configs/b.json", "b");
        write(&root, "run.exe", "x");

        let bytes = export_profile(&root, "My profile", &[sample_mod(true)]).unwrap();
        assert_eq!(
            entry_names(&bytes),
            [
                "BepInEx/core/core.json",
                "BepInEx/plugins/Mod/settings.json",
                "GDWeave/configs/b.json",
                "config/a.cfg",
                "config/sub/b.json",
                "doorstop_config.ini",
                "export.r2x",
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exports_missing_profile_directory_as_empty() {
        let root = temp_dir().join("missing");
        let bytes = export_profile(&root, "Empty", &[]).unwrap();
        assert_eq!(entry_names(&bytes), ["export.r2x"]);
    }

    #[test]
    fn round_trips_through_import() {
        let source = temp_dir();
        write(&source, "BepInEx/config/a.cfg", "[x]\nkey = 1\n");
        write(&source, "BepInEx/plugins/Mod/settings.json", "{}");
        let mods = [sample_mod(true), sample_mod(false)];
        let bytes = export_profile(&source, "Round trip", &mods[..1]).unwrap();
        let disabled = export_profile(&source, "Round trip", &mods[1..]).unwrap();

        let import = read_r2z(&bytes).unwrap();
        assert_eq!(import.profile_name, "Round trip");
        assert_eq!(import.mods.len(), 1);
        assert_eq!(
            import.mods[0].id.to_string(),
            "RiskofThunder-R2API_Core-5.1.4"
        );
        assert!(import.mods[0].enabled);
        assert!(!read_r2z(&disabled).unwrap().mods[0].enabled);

        let target = temp_dir();
        apply_import_files(&import, &target).unwrap();
        assert_eq!(
            fs::read_to_string(target.join("BepInEx/config/a.cfg")).unwrap(),
            "[x]\nkey = 1\n"
        );
        assert_eq!(
            fs::read_to_string(target.join("BepInEx/plugins/Mod/settings.json")).unwrap(),
            "{}"
        );
        fs::remove_dir_all(source).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn writes_r2modman_manifest_shape() {
        let bytes = export_profile(&temp_dir(), "P", &[sample_mod(true)]).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut text = String::new();
        archive
            .by_name("export.r2x")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
        assert_eq!(value["profileName"], "P");
        assert_eq!(value["mods"][0]["name"], "RiskofThunder-R2API_Core");
        assert_eq!(value["mods"][0]["version"]["major"], 5);
        assert_eq!(value["mods"][0]["version"]["minor"], 1);
        assert_eq!(value["mods"][0]["version"]["patch"], 4);
        assert_eq!(value["mods"][0]["enabled"], true);
    }

    #[test]
    fn reads_r2modman_fixture() {
        // js-yaml output of r2modman, CRLF config path and a missing `enabled`.
        let manifest = b"profileName: Friends Pack\nmods:\n  - name: BepInEx-BepInExPack\n    version:\n      major: 5\n      minor: 4\n      patch: 2100\n    enabled: true\n  - name: tristanmcpherson-MoreCompany\n    version:\n      major: 1\n      minor: 7\n      patch: 2\n    enabled: false\n  - name: Owner-NoFlag\n    version:\n      major: 0\n      minor: 0\n      patch: 1\n";
        let bytes = zip_of(&[
            ("export.r2x", manifest),
            ("config\\BepInEx.cfg", b"[Logging]"),
            ("config/sub\\Other.cfg", b"x"),
            ("BepInEx/plugins/Mod/data.txt", b"t"),
            ("mods.yml", b"ignored"),
        ]);
        let import = read_r2z(&bytes).unwrap();
        assert_eq!(import.profile_name, "Friends Pack");
        let mods: Vec<_> = import
            .mods
            .iter()
            .map(|entry| (entry.id.to_string(), entry.enabled))
            .collect();
        assert_eq!(
            mods,
            [
                ("BepInEx-BepInExPack-5.4.2100".to_owned(), true),
                ("tristanmcpherson-MoreCompany-1.7.2".to_owned(), false),
                ("Owner-NoFlag-0.0.1".to_owned(), true),
            ]
        );
        let mut paths: Vec<_> = import
            .files
            .iter()
            .map(|f| f.relative_path.as_str())
            .collect();
        paths.sort();
        assert_eq!(
            paths,
            [
                "BepInEx/config/BepInEx.cfg",
                "BepInEx/config/sub/Other.cfg",
                "BepInEx/plugins/Mod/data.txt",
            ]
        );
        assert!(import.skipped.is_empty());
    }

    #[test]
    fn rejects_unsafe_paths() {
        for name in [
            "../evil.cfg",
            "config/../../evil.cfg",
            "config\\..\\evil.cfg",
            "/abs/evil.cfg",
            "\\abs\\evil.cfg",
            "C:/evil.cfg",
            "C:\\evil.cfg",
            "config//a.cfg",
            "config/a.cfg.",
            "config/a\u{0}.cfg",
        ] {
            let bytes = zip_of(&[("export.r2x", MANIFEST), (name, b"x")]);
            assert!(read_r2z(&bytes).is_err(), "{name}");
        }
    }

    #[test]
    fn skips_blocked_and_unsupported_files() {
        let bytes = zip_of(&[
            ("export.r2x", MANIFEST),
            ("config/a.cfg", b"ok"),
            ("config/evil.dll", b"x"),
            ("config/evil.EXE", b"x"),
            ("run.bat", b"x"),
            ("import.reg", b"x"),
            ("config/script.ps1", b"x"),
            ("noextension", b"x"),
        ]);
        let import = read_r2z(&bytes).unwrap();
        assert_eq!(import.files.len(), 1);
        assert_eq!(import.skipped.len(), 6);
    }

    #[test]
    fn rejects_duplicates_and_missing_manifest() {
        let duplicates = zip_of(&[
            ("export.r2x", MANIFEST),
            ("config/a.cfg", b"1"),
            ("CONFIG/A.cfg", b"2"),
        ]);
        assert!(read_r2z(&duplicates).is_err());
        assert!(read_r2z(&zip_of(&[("config/a.cfg", b"1")])).is_err());
        assert!(read_r2z(b"not a zip").is_err());
    }

    #[test]
    fn rejects_invalid_manifests() {
        for manifest in [
            "mods: []\n",
            "profileName: P\n",
            "profileName: P\nmods: nope\n",
            "profileName: P\nmods:\n  - name: Bad\n    version: {major: 1, minor: 0, patch: 0}\n",
            "profileName: P\nmods:\n  - name: A-B\n    version: {major: -1, minor: 0, patch: 0}\n",
            "profileName: '  '\nmods: []\n",
            ": : :",
        ] {
            let bytes = zip_of(&[("export.r2x", manifest.as_bytes())]);
            assert!(read_r2z(&bytes).is_err(), "{manifest}");
        }
    }

    #[test]
    fn sanitizes_profile_names() {
        let bytes = zip_of(&[(
            "export.r2x",
            format!("profileName: \"  a/b\\\\c{}\"\nmods: []\n", "x".repeat(100)).as_bytes(),
        )]);
        let name = read_r2z(&bytes).unwrap().profile_name;
        assert!(name.starts_with("a_b_cx"));
        assert_eq!(name.chars().count(), MAX_PROFILE_NAME_CHARS - 2);
    }

    #[test]
    fn rejects_too_many_entries() {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("export.r2x", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(MANIFEST).unwrap();
        for index in 0..=MAX_ENTRIES {
            writer
                .start_file(format!("config/{index}.cfg"), SimpleFileOptions::default())
                .unwrap();
        }
        let bytes = writer.finish().unwrap().into_inner();
        assert!(read_r2z(&bytes).unwrap_err().contains("too many entries"));
    }

    #[test]
    fn rejects_high_compression_ratio() {
        let zeros = vec![0_u8; 8 * 1024 * 1024];
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer.start_file("export.r2x", options).unwrap();
        writer.write_all(MANIFEST).unwrap();
        writer.start_file("config/big.cfg", options).unwrap();
        writer.write_all(&zeros).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert!(read_r2z(&bytes).unwrap_err().contains("compression ratio"));
    }

    #[test]
    fn rejects_too_much_total_data() {
        // Sparse noise deflates to roughly 50:1, under the per-entry ratio cap.
        let mut state = 0x2545_f491_u32;
        let mut chunk = vec![0_u8; 1024 * 1024];
        for index in (0..chunk.len()).step_by(50) {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            chunk[index] = state as u8;
        }
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer.start_file("export.r2x", options).unwrap();
        writer.write_all(MANIFEST).unwrap();
        for index in 0..=(MAX_TOTAL_BYTES / chunk.len() as u64) {
            writer
                .start_file(format!("config/{index}.cfg"), options)
                .unwrap();
            writer.write_all(&chunk).unwrap();
        }
        let bytes = writer.finish().unwrap().into_inner();
        assert!(bytes.len() <= MAX_ARCHIVE_BYTES);
        assert!(read_r2z(&bytes).unwrap_err().contains("too much data"));
    }

    #[test]
    fn promotes_staged_profile_once() {
        let root = temp_dir();
        let staging = root.join("staging");
        let target = root.join("profiles/p1");
        fs::create_dir_all(&staging).unwrap();
        write(&staging, "a.cfg", "a");
        promote_staged_profile(&staging, &target).unwrap();
        assert!(target.join("a.cfg").is_file());
        fs::create_dir_all(&staging).unwrap();
        assert!(promote_staged_profile(&staging, &target).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exports_to_file() {
        let root = temp_dir();
        let destination = root.join("out.r2z");
        export_profile_to_file(&root.join("none"), "P", &[], &destination).unwrap();
        assert_eq!(read_r2z_file(&destination).unwrap().profile_name, "P");
        fs::remove_dir_all(root).unwrap();
    }
}
