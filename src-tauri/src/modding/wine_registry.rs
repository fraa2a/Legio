//! Wine `user.reg` DLL override patching for Steam games run by Proton.
//!
//! Steam accepts only arguments for a game, not environment variables, so the
//! mod loader proxy DLL must be marked native in the prefix registry instead.

// Only Linux patches a Proton prefix; the code stays compiled and tested everywhere.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const DLL_OVERRIDES_SECTION: &str = "[Software\\\\Wine\\\\DllOverrides]";
const UNIX_TO_FILETIME_SECONDS: u64 = 11_644_473_600;
const BACKUP_NAME: &str = "user.reg.legio.bak";
const TEMPORARY_NAME: &str = "user.reg.legio.tmp";

/// Sets `name=value` in the DLL overrides section, creating the section when
/// missing. Returns `None` when the registry already holds the value.
pub(super) fn ensure_dll_override(
    registry: &str,
    name: &str,
    value: &str,
    now_seconds: u64,
) -> Option<String> {
    let entry = format!("\"{name}\"=\"{value}\"");
    let mut lines: Vec<&str> = registry.split('\n').collect();
    let Some(header) = lines.iter().position(|line| is_section_header(line)) else {
        return Some(append_section(registry, &entry, now_seconds));
    };
    let end = lines[header + 1..]
        .iter()
        .position(|line| line.trim().is_empty() || line.starts_with('['))
        .map_or(lines.len(), |offset| header + 1 + offset);
    let existing = (header + 1..end).find(|&index| key_matches(lines[index], name));
    if let Some(index) = existing {
        if lines[index] == entry {
            return None;
        }
        lines[index] = &entry;
    } else {
        lines.insert(end, &entry);
    }
    Some(lines.join("\n"))
}

/// Applies [`ensure_dll_override`] to `user_reg`. The original file is kept
/// once as a backup before the first change.
pub(super) fn patch_user_reg(
    user_reg: &Path,
    name: &str,
    value: &str,
    now_seconds: u64,
) -> Result<(), String> {
    let content = fs::read_to_string(user_reg).map_err(|error| {
        format!(
            "Could not read the Wine registry {}: {error}",
            user_reg.display()
        )
    })?;
    let Some(patched) = ensure_dll_override(&content, name, value, now_seconds) else {
        return Ok(());
    };
    let directory = user_reg
        .parent()
        .ok_or_else(|| "Wine registry path has no parent directory".to_owned())?;
    let backup = directory.join(BACKUP_NAME);
    if !backup.exists() {
        fs::copy(user_reg, &backup)
            .map_err(|error| format!("Could not back up the Wine registry: {error}"))?;
    }
    write_atomic(user_reg, directory.join(TEMPORARY_NAME), patched.as_bytes())
}

fn write_atomic(path: &Path, temporary: PathBuf, bytes: &[u8]) -> Result<(), String> {
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)
            .map_err(|error| format!("Could not create the temporary Wine registry: {error}"))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("Could not write the temporary Wine registry: {error}"))?;
        fs::rename(&temporary, path)
            .map_err(|error| format!("Could not replace the Wine registry: {error}"))
    })();
    if result.is_err() {
        // A leftover temporary file is truncated on the next attempt.
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn is_section_header(line: &str) -> bool {
    line.strip_prefix(DLL_OVERRIDES_SECTION)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
}

fn key_matches(line: &str, name: &str) -> bool {
    line.strip_prefix('"')
        .and_then(|rest| rest.split_once("\"="))
        .is_some_and(|(key, _)| key.eq_ignore_ascii_case(name))
}

// Wine writes "[section] <unix seconds>" and "#time=<hex FILETIME>" per section.
fn append_section(registry: &str, entry: &str, now_seconds: u64) -> String {
    let filetime = (now_seconds + UNIX_TO_FILETIME_SECONDS) * 10_000_000;
    let mut patched = registry.to_owned();
    if !patched.is_empty() && !patched.ends_with('\n') {
        patched.push('\n');
    }
    if !patched.is_empty() && !patched.ends_with("\n\n") {
        patched.push('\n');
    }
    patched.push_str(&format!(
        "{DLL_OVERRIDES_SECTION} {now_seconds}\n#time={filetime:x}\n{entry}\n\n"
    ));
    patched
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRY: &str = "WINE REGISTRY Version 2\n;; All keys relative to \\\\User\\\\S-1\n\n[Software\\\\Wine\\\\DllOverrides] 1700000000\n#time=1d9a000000000000\n\"*d3d11\"=\"native\"\n\n[Software\\\\Wine\\\\Other] 1700000001\n#time=1\n\"a\"=\"b\"\n\n";

    #[test]
    fn creates_the_section_with_a_wine_timestamp() {
        let registry =
            "WINE REGISTRY Version 2\n\n[Software\\\\Wine\\\\Other] 1\n#time=1\n\"a\"=\"b\"\n";
        let patched = ensure_dll_override(registry, "winhttp", "native,builtin", 0).unwrap();
        assert_eq!(
            patched,
            format!(
                "{registry}\n[Software\\\\Wine\\\\DllOverrides] 0\n#time=19db1ded53e8000\n\"winhttp\"=\"native,builtin\"\n\n"
            )
        );
    }

    #[test]
    fn creates_the_section_in_an_empty_registry() {
        let patched = ensure_dll_override("", "winhttp", "native,builtin", 0).unwrap();
        assert!(patched.starts_with(DLL_OVERRIDES_SECTION));
    }

    #[test]
    fn adds_a_value_to_an_existing_section() {
        let patched = ensure_dll_override(REGISTRY, "winhttp", "native,builtin", 5).unwrap();
        assert_eq!(
            patched,
            REGISTRY.replace(
                "\"*d3d11\"=\"native\"\n",
                "\"*d3d11\"=\"native\"\n\"winhttp\"=\"native,builtin\"\n"
            )
        );
    }

    #[test]
    fn replaces_a_different_value() {
        let registry = REGISTRY.replace(
            "\"*d3d11\"=\"native\"",
            "\"WinHTTP\"=\"builtin\"\n\"*d3d11\"=\"native\"",
        );
        let patched = ensure_dll_override(&registry, "winhttp", "native,builtin", 5).unwrap();
        assert!(patched.contains("\"winhttp\"=\"native,builtin\"\n\"*d3d11\""));
        assert!(!patched.contains("builtin\"\n\"*d3d11\"=\"native\"\n\"winhttp"));
        assert_eq!(patched.matches("winhttp").count(), 1);
    }

    #[test]
    fn is_idempotent() {
        let patched = ensure_dll_override(REGISTRY, "winhttp", "native,builtin", 5).unwrap();
        assert_eq!(
            ensure_dll_override(&patched, "winhttp", "native,builtin", 9),
            None
        );
        let created = ensure_dll_override("", "winhttp", "native,builtin", 5).unwrap();
        assert_eq!(
            ensure_dll_override(&created, "winhttp", "native,builtin", 9),
            None
        );
    }

    #[test]
    fn handles_a_section_at_the_end_without_a_blank_line() {
        let registry = "[Software\\\\Wine\\\\DllOverrides] 1\n#time=1\n\"a\"=\"b\"";
        let patched = ensure_dll_override(registry, "winhttp", "native,builtin", 5).unwrap();
        assert_eq!(
            patched,
            "[Software\\\\Wine\\\\DllOverrides] 1\n#time=1\n\"a\"=\"b\"\n\"winhttp\"=\"native,builtin\""
        );
    }

    #[test]
    fn patches_the_file_and_keeps_the_first_backup() {
        let directory = std::env::temp_dir().join(format!("legio-reg-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let user_reg = directory.join("user.reg");
        fs::write(&user_reg, REGISTRY).unwrap();

        patch_user_reg(&user_reg, "winhttp", "native,builtin", 5).unwrap();
        let patched = fs::read_to_string(&user_reg).unwrap();
        assert!(patched.contains("\"winhttp\"=\"native,builtin\""));
        assert_eq!(
            fs::read_to_string(directory.join(BACKUP_NAME)).unwrap(),
            REGISTRY
        );
        assert!(!directory.join(TEMPORARY_NAME).exists());

        patch_user_reg(&user_reg, "winhttp", "native,builtin", 6).unwrap();
        assert_eq!(fs::read_to_string(&user_reg).unwrap(), patched);

        patch_user_reg(&user_reg, "winhttp", "builtin", 7).unwrap();
        assert_eq!(
            fs::read_to_string(directory.join(BACKUP_NAME)).unwrap(),
            REGISTRY
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reports_a_missing_registry() {
        let missing = std::env::temp_dir().join(format!("legio-reg-{}", uuid::Uuid::new_v4()));
        assert!(patch_user_reg(&missing.join("user.reg"), "winhttp", "n", 0).is_err());
    }
}
