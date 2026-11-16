//! Heroic Games Launcher reader.
//!
//! Config roots: `~/.config/heroic`, the Flatpak
//! `~/.var/app/com.heroicgameslauncher.hgl/config/heroic`, and on Windows
//! `%APPDATA%/heroic`. Files read inside a root (all JSON, all optional):
//!
//! - `legendaryConfig/legendary/installed.json`: object keyed by app name with
//!   `app_name`, `title`, `install_path`, `executable` (relative), `platform`
//!   and `is_dlc`.
//! - `gog_store/installed.json`: `{ "installed": [ { appName, install_path,
//!   platform, executable } ] }`. The executable is usually empty, so the
//!   primary play task of `<install_path>/goggame-<appName>.info` is used.
//! - `nile_config/nile/installed.json`: array of `{ id, path }` (Windows only,
//!   executable unknown).
//! - `sideload_apps/library.json`: `{ "games": [ { app_name, title,
//!   folder_name, is_installed, install: { executable, platform } } ] }`.
//! - `store_cache/{legendary,gog,nile}_library.json`: only to look up titles
//!   (`library` or `games` array of `{ app_name | id, title }`).
//! - `GamesConfig/<app>.json`: `{ "<app>": { winePrefix, wineVersion: { name } } }`.
//!
//! Nothing is written. Paths are validated before being offered.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::fs_guard::{
    MAX_ROWS, canonical_dir, clean_title, directory_title, existing_dir, existing_file,
    file_within, is_safe_name, read_json_within,
};
use super::{DetectedGame, GameKind, Launcher, LauncherOutcome, conclude};

pub fn default_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(config) = dirs::config_dir() {
        roots.push(config.join("heroic"));
    }
    if cfg!(target_os = "linux")
        && let Some(home) = dirs::home_dir()
    {
        roots.push(home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic"));
    }
    roots
}

pub fn scan(roots: &[PathBuf]) -> LauncherOutcome {
    let mut games = Vec::new();
    let mut problems = Vec::new();
    let mut seen = false;
    for root in roots {
        let Ok(root) = canonical_dir(root) else {
            continue;
        };
        seen = true;
        scan_root(&root, &mut games, &mut problems);
    }
    conclude(seen, games, problems)
}

fn scan_root(root: &Path, games: &mut Vec<DetectedGame>, problems: &mut Vec<String>) {
    let titles = store_titles(root, problems);
    let mut load = |relative: &str| match read_json_within(root, relative) {
        Ok(value) => value,
        Err(error) => {
            problems.push(error);
            None
        }
    };
    let legendary = load("legendaryConfig/legendary/installed.json");
    let gog = load("gog_store/installed.json");
    let nile = load("nile_config/nile/installed.json");
    let sideload = load("sideload_apps/library.json");

    let mut entries: Vec<Entry> = Vec::new();
    if let Some(Value::Object(map)) = &legendary {
        for (key, value) in map.iter().take(MAX_ROWS) {
            if value["is_dlc"].as_bool() == Some(true) {
                continue;
            }
            entries.push(Entry {
                app_name: value["app_name"].as_str().unwrap_or(key).to_owned(),
                title: value["title"].as_str().map(str::to_owned),
                install_path: value["install_path"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                executable: value["executable"].as_str().map(str::to_owned),
                platform: value["platform"].as_str().unwrap_or_default().to_owned(),
            });
        }
    }
    if let Some(list) = gog.as_ref().and_then(|value| value["installed"].as_array()) {
        for value in list.iter().take(MAX_ROWS) {
            if value["is_dlc"].as_bool() == Some(true) {
                continue;
            }
            let app_name = value["appName"].as_str().unwrap_or_default().to_owned();
            let install_path = value["install_path"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let executable = value["executable"]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .or_else(|| gog_primary_executable(&install_path, &app_name));
            entries.push(Entry {
                app_name,
                install_path,
                executable,
                platform: value["platform"].as_str().unwrap_or_default().to_owned(),
                ..Entry::default()
            });
        }
    }
    if let Some(list) = nile.as_ref().and_then(Value::as_array) {
        for value in list.iter().take(MAX_ROWS) {
            entries.push(Entry {
                app_name: value["id"].as_str().unwrap_or_default().to_owned(),
                install_path: value["path"].as_str().unwrap_or_default().to_owned(),
                platform: "windows".to_owned(),
                ..Entry::default()
            });
        }
    }
    if let Some(list) = sideload
        .as_ref()
        .and_then(|value| value["games"].as_array())
    {
        for value in list.iter().take(MAX_ROWS) {
            if value["is_installed"].as_bool() == Some(false) {
                continue;
            }
            entries.push(Entry {
                app_name: value["app_name"].as_str().unwrap_or_default().to_owned(),
                title: value["title"].as_str().map(str::to_owned),
                install_path: value["folder_name"].as_str().unwrap_or_default().to_owned(),
                executable: value["install"]["executable"].as_str().map(str::to_owned),
                platform: value["install"]["platform"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            });
        }
    }
    for entry in entries {
        if let Some(game) = build_game(root, entry, &titles) {
            games.push(game);
        }
    }
}

#[derive(Default)]
struct Entry {
    app_name: String,
    title: Option<String>,
    install_path: String,
    executable: Option<String>,
    platform: String,
}

fn kind_from_platform(platform: &str) -> Option<GameKind> {
    match platform.to_ascii_lowercase().as_str() {
        "windows" | "win" | "win32" | "win64" => Some(GameKind::Windows),
        "linux" => Some(GameKind::Native),
        _ => None,
    }
}

fn build_game(root: &Path, entry: Entry, titles: &HashMap<String, String>) -> Option<DetectedGame> {
    let kind = kind_from_platform(&entry.platform)?;
    let install_dir = existing_dir(&entry.install_path)?;
    let title = entry
        .title
        .as_deref()
        .and_then(clean_title)
        .or_else(|| titles.get(&entry.app_name).cloned())
        .or_else(|| directory_title(&install_dir))?;
    let executable = entry
        .executable
        .as_deref()
        .and_then(|value| existing_file(value).or_else(|| file_within(&install_dir, value)));
    let mut game = DetectedGame::new(Launcher::Heroic, title, &install_dir, kind);
    game.executable = executable.map(|path| path.to_string_lossy().into_owned());
    if is_safe_name(&entry.app_name) {
        game.store_id = Some(entry.app_name.clone());
        if kind == GameKind::Windows {
            apply_game_config(root, &entry.app_name, &mut game);
        }
    }
    Some(game)
}

fn apply_game_config(root: &Path, app_name: &str, game: &mut DetectedGame) {
    let Ok(Some(config)) = read_json_within(root, &format!("GamesConfig/{app_name}.json")) else {
        return;
    };
    let settings = &config[app_name];
    game.prefix = settings["winePrefix"]
        .as_str()
        .and_then(existing_dir)
        .map(|path| path.to_string_lossy().into_owned());
    game.runner_hint = settings["wineVersion"]["name"]
        .as_str()
        .and_then(clean_title);
}

fn gog_primary_executable(install_path: &str, app_name: &str) -> Option<String> {
    if !is_safe_name(app_name) {
        return None;
    }
    let install_dir = existing_dir(install_path)?;
    let info = file_within(&install_dir, &format!("goggame-{app_name}.info"))?;
    let text =
        super::fs_guard::read_text_within(&install_dir, &info, super::fs_guard::MAX_JSON_BYTES)
            .ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let tasks = value["playTasks"].as_array()?;
    let task = tasks
        .iter()
        .find(|task| task["isPrimary"].as_bool() == Some(true))?;
    task["path"].as_str().map(str::to_owned)
}

fn store_titles(root: &Path, problems: &mut Vec<String>) -> HashMap<String, String> {
    let mut titles = HashMap::new();
    for name in ["legendary", "gog", "nile"] {
        let relative = format!("store_cache/{name}_library.json");
        let value = match read_json_within(root, &relative) {
            Ok(Some(value)) => value,
            Ok(None) => continue,
            Err(error) => {
                problems.push(error);
                continue;
            }
        };
        let list = value
            .as_array()
            .or_else(|| value["library"].as_array())
            .or_else(|| value["games"].as_array());
        for item in list.into_iter().flatten().take(MAX_ROWS) {
            let id = item["app_name"].as_str().or_else(|| item["id"].as_str());
            if let (Some(id), Some(title)) = (id, item["title"].as_str().and_then(clean_title)) {
                titles.insert(id.to_owned(), title);
            }
        }
    }
    titles
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::fs_guard::tests::temp;
    use super::*;

    fn write(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn games(outcome: LauncherOutcome) -> Vec<DetectedGame> {
        match outcome {
            LauncherOutcome::Found { games, .. } => games,
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn reads_every_store_and_game_config() {
        let root = temp();
        let heroic = root.join("heroic");
        let install = |name: &str, files: &[&str]| {
            let dir = root.join(name);
            fs::create_dir_all(&dir).unwrap();
            for file in files {
                fs::write(dir.join(file), "MZ").unwrap();
            }
            dir.to_string_lossy().replace('\\', "/")
        };
        let prefix = root.join("prefix");
        fs::create_dir(&prefix).unwrap();
        let epic = install("epic", &["game.exe"]);
        let gog = install("gog", &["gog.exe"]);
        let amazon = install("amazon", &[]);
        let side = install("side", &["side.sh"]);
        write(
            &heroic,
            "legendaryConfig/legendary/installed.json",
            &format!(
                r#"{{"Sugar": {{"app_name": "Sugar", "title": "Epic Game", "install_path": "{epic}", "executable": "game.exe", "platform": "Windows", "is_dlc": false}},
                 "Dlc": {{"app_name": "Dlc", "title": "DLC", "install_path": "{epic}", "is_dlc": true}},
                 "Mac": {{"app_name": "Mac", "title": "Mac", "install_path": "{epic}", "platform": "Mac"}}}}"#
            ),
        );
        write(
            &heroic,
            "gog_store/installed.json",
            &format!(
                r#"{{"installed": [{{"appName": "123", "install_path": "{gog}", "executable": "", "platform": "windows"}}]}}"#
            ),
        );
        write(
            &root.join("gog"),
            "goggame-123.info",
            r#"{"playTasks": [{"path": "other.exe"}, {"isPrimary": true, "path": "gog.exe"}]}"#,
        );
        write(
            &heroic,
            "store_cache/gog_library.json",
            r#"{"games": [{"app_name": "123", "title": "GOG Game"}]}"#,
        );
        write(
            &heroic,
            "nile_config/nile/installed.json",
            &format!(r#"[{{"id": "amzn1", "path": "{amazon}"}}]"#),
        );
        write(
            &heroic,
            "sideload_apps/library.json",
            &format!(
                r#"{{"games": [{{"app_name": "side1", "title": "Side", "folder_name": "{side}", "is_installed": true, "install": {{"executable": "{side}/side.sh", "platform": "linux"}}}}]}}"#
            ),
        );
        write(
            &heroic,
            "GamesConfig/Sugar.json",
            &format!(
                r#"{{"Sugar": {{"winePrefix": "{}", "wineVersion": {{"name": "Wine-GE-8"}}}}, "version": "v0"}}"#,
                prefix.to_string_lossy().replace('\\', "/")
            ),
        );
        let found = games(scan(&[heroic]));
        let by_title = |title: &str| found.iter().find(|game| game.title == title).unwrap();
        assert_eq!(found.len(), 4);
        let epic = by_title("Epic Game");
        assert!(epic.executable.as_deref().unwrap().ends_with("game.exe"));
        assert_eq!(epic.kind, GameKind::Windows);
        assert_eq!(epic.runner_hint.as_deref(), Some("Wine-GE-8"));
        assert!(epic.prefix.is_some());
        assert!(
            by_title("GOG Game")
                .executable
                .as_deref()
                .unwrap()
                .ends_with("gog.exe")
        );
        assert_eq!(by_title("amazon").executable, None);
        assert_eq!(by_title("Side").kind, GameKind::Native);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_malformed_and_hostile_inputs() {
        let root = temp();
        assert_eq!(scan(&[root.join("none")]), LauncherOutcome::NotInstalled);
        let heroic = root.join("heroic");
        fs::create_dir(&heroic).unwrap();
        assert_eq!(games(scan(std::slice::from_ref(&heroic))), Vec::new());
        write(
            &heroic,
            "legendaryConfig/legendary/installed.json",
            "{ not json",
        );
        assert!(matches!(
            scan(std::slice::from_ref(&heroic)),
            LauncherOutcome::Unreadable { .. }
        ));
        // Missing install directory, traversal in the executable and a hostile app name.
        let install = root.join("game");
        fs::create_dir(&install).unwrap();
        fs::write(root.join("outside.exe"), "MZ").unwrap();
        let path = install.to_string_lossy().replace('\\', "/");
        write(
            &heroic,
            "legendaryConfig/legendary/installed.json",
            &format!(
                r#"{{"a": {{"app_name": "../../evil", "title": "Hostile", "install_path": "{path}", "executable": "../outside.exe", "platform": "Windows"}},
                 "b": {{"app_name": "b", "title": "Gone", "install_path": "/does/not/exist", "platform": "Windows"}}}}"#
            ),
        );
        let found = games(scan(std::slice::from_ref(&heroic)));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].executable, None);
        assert_eq!(found[0].store_id, None);
        // Oversized file.
        write(
            &heroic,
            "legendaryConfig/legendary/installed.json",
            &" ".repeat(17 * 1024 * 1024),
        );
        assert!(matches!(
            scan(&[heroic]),
            LauncherOutcome::Unreadable { .. }
        ));
        fs::remove_dir_all(root).unwrap();
    }
}
