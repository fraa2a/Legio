//! Bottles reader (Linux only).
//!
//! Locations: `~/.local/share/bottles/bottles/<bottle>/bottle.yml` and the
//! Flatpak `~/.var/app/com.usebottles.bottles/data/bottles/bottles`. Bottle
//! folders that are symbolic links are skipped.
//!
//! From `bottle.yml` (tolerant scalar extractor, uncertain values skipped):
//! `Name`, `Runner` and `External_Programs`, a mapping of program id to
//! `name` and `path`; the older `name: path` form is accepted too. A `path`
//! is a host path, or a Windows path (`C:\...`) resolved under the bottle's
//! `drive_c`. The bottle folder is the Wine prefix. Every external program is
//! offered as a Windows game installed in the folder of its executable.

use std::path::{Path, PathBuf};

use super::fs_guard::{
    MAX_YAML_BYTES, clean_title, existing_dir, existing_file, file_within, read_text_within,
};
use super::yaml_scalars::YamlScalars;
use super::{DetectedGame, GameKind, Launcher, LauncherOutcome, SourceError, conclude};

const MAX_BOTTLES: usize = 200;
const MAX_PROGRAMS_PER_BOTTLE: usize = 500;

pub fn default_roots() -> Vec<PathBuf> {
    if !cfg!(target_os = "linux") {
        return Vec::new();
    }
    let mut roots = Vec::new();
    if let Some(data) = dirs::data_dir() {
        roots.push(data.join("bottles/bottles"));
    }
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".var/app/com.usebottles.bottles/data/bottles/bottles"));
    }
    roots
}

pub fn scan(roots: &[PathBuf]) -> LauncherOutcome {
    let mut games = Vec::new();
    let mut problems = Vec::new();
    let mut seen = false;
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        seen = true;
        for entry in entries.take(MAX_BOTTLES) {
            let Ok(entry) = entry else { continue };
            // Links could point outside the Bottles folder.
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            match read_bottle(&entry.path()) {
                Ok(found) => games.extend(found),
                Err(SourceError::Missing) => {}
                Err(SourceError::Unreadable(reason)) => {
                    problems.push(format!("{}: {reason}", entry.file_name().to_string_lossy()));
                }
            }
        }
    }
    conclude(seen, games, problems)
}

fn read_bottle(bottle: &Path) -> Result<Vec<DetectedGame>, SourceError> {
    let text = read_text_within(bottle, &bottle.join("bottle.yml"), MAX_YAML_BYTES)?;
    let yaml = YamlScalars::parse(&text);
    let prefix = existing_dir(&bottle.to_string_lossy());
    let runner = yaml
        .scalar(&["Runner"])
        .and_then(|value| clean_title(&value));
    let mut games = Vec::new();
    for id in yaml
        .children(&["External_Programs"])
        .into_iter()
        .take(MAX_PROGRAMS_PER_BOTTLE)
    {
        let path = ["External_Programs", id.as_str()];
        let (name, program) = match yaml.scalar(&[path[0], path[1], "path"]) {
            Some(program) => (yaml.scalar(&[path[0], path[1], "name"]), program),
            // Older files map the program name directly to its path.
            None => match yaml.scalar(&path) {
                Some(program) => (Some(id.clone()), program),
                None => continue,
            },
        };
        let Some(executable) = resolve_program(bottle, &program) else {
            continue;
        };
        let Some(install_dir) = executable.parent() else {
            continue;
        };
        let Some(title) = name
            .as_deref()
            .and_then(clean_title)
            .or_else(|| super::fs_guard::directory_title(install_dir))
        else {
            continue;
        };
        let mut game = DetectedGame::new(Launcher::Bottles, title, install_dir, GameKind::Windows);
        game.executable = Some(executable.to_string_lossy().into_owned());
        game.prefix = prefix
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        game.runner_hint = runner.clone();
        game.store_id = Some(id);
        games.push(game);
    }
    Ok(games)
}

/// Host paths are used as they are; `C:\...` paths map to the bottle's `drive_c`.
fn resolve_program(bottle: &Path, value: &str) -> Option<PathBuf> {
    let bytes = value.as_bytes();
    if bytes.len() > 3 && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/') {
        if !bytes[0].eq_ignore_ascii_case(&b'c') {
            return None;
        }
        return file_within(&bottle.join("drive_c"), &value[3..]);
    }
    existing_file(value)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::fs_guard::tests::temp;
    use super::*;

    #[test]
    fn lists_external_programs_with_bottle_as_prefix() {
        let root = temp();
        let bottles = root.join("bottles");
        let bottle = bottles.join("Gaming");
        fs::create_dir_all(bottle.join("drive_c/Games")).unwrap();
        fs::write(bottle.join("drive_c/Games/inside.exe"), "MZ").unwrap();
        let host = root.join("host");
        fs::create_dir(&host).unwrap();
        fs::write(host.join("outside.exe"), "MZ").unwrap();
        fs::write(
            bottle.join("bottle.yml"),
            format!(
                "Name: Gaming\nRunner: soda-9.0\nExternal_Programs:\n  id1:\n    name: Inside\n    path: C:\\Games\\inside.exe\n  id2:\n    name: Outside\n    path: {}/outside.exe\n  id3:\n    name: Escape\n    path: C:\\..\\..\\host\\outside.exe\n  id4:\n    name: Missing\n    path: /nope/none.exe\n  Legacy: {}/outside.exe\n",
                host.display(),
                host.display()
            ),
        )
        .unwrap();
        let LauncherOutcome::Found { games, .. } = scan(&[bottles]) else {
            panic!("expected games");
        };
        let titles: Vec<_> = games.iter().map(|game| game.title.as_str()).collect();
        assert_eq!(titles, ["Inside", "Outside", "Legacy"]);
        assert!(games.iter().all(|game| game.kind == GameKind::Windows));
        let expected = dunce::canonicalize(&bottle).unwrap();
        assert_eq!(games[0].prefix.as_deref(), expected.to_str());
        assert_eq!(games[0].runner_hint.as_deref(), Some("soda-9.0"));
        assert!(games[0].install_dir.ends_with("Games"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_malformed_oversized_and_linked_bottles() {
        let root = temp();
        let bottles = root.join("bottles");
        assert_eq!(
            scan(std::slice::from_ref(&bottles)),
            LauncherOutcome::NotInstalled
        );
        fs::create_dir_all(bottles.join("empty")).unwrap();
        assert!(matches!(
            scan(std::slice::from_ref(&bottles)),
            LauncherOutcome::Found { .. }
        ));
        fs::create_dir_all(bottles.join("garbage")).unwrap();
        fs::write(bottles.join("garbage/bottle.yml"), "\u{1}\u{2} :: {{{").unwrap();
        fs::create_dir_all(bottles.join("big")).unwrap();
        fs::write(bottles.join("big/bottle.yml"), " ".repeat(300 * 1024)).unwrap();
        assert!(matches!(
            scan(std::slice::from_ref(&bottles)),
            LauncherOutcome::Unreadable { .. }
        ));
        fs::remove_dir_all(bottles.join("big")).unwrap();
        let LauncherOutcome::Found { games, warnings } = scan(std::slice::from_ref(&bottles))
        else {
            panic!("expected found");
        };
        assert!(games.is_empty() && warnings.is_empty());
        #[cfg(unix)]
        {
            let outside = root.join("outside");
            fs::create_dir(&outside).unwrap();
            fs::write(
                outside.join("bottle.yml"),
                "External_Programs:\n  Legacy: /bin/sh\n",
            )
            .unwrap();
            std::os::unix::fs::symlink(&outside, bottles.join("linked")).unwrap();
            let LauncherOutcome::Found { games, .. } = scan(&[bottles]) else {
                panic!("expected found");
            };
            assert!(games.is_empty());
        }
        fs::remove_dir_all(root).unwrap();
    }
}
