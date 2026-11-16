//! itch reader.
//!
//! Locations: `~/.config/itch/db/butler.db`, the Flatpak
//! `~/.var/app/io.itch.itch/config/itch/db/butler.db`, and on Windows
//! `%APPDATA%/itch/db/butler.db`. SQLite, opened read-only and immutable.
//!
//! Tables used: `caves` (`game_id`, `install_location_id`,
//! `install_folder_name`, `custom_install_folder`, `verdict`), `games`
//! (`id`, `title`) and `install_locations` (`id`, `path`). The install folder
//! is `custom_install_folder`, else `<install_locations.path>/<install_folder_name>`.
//! `verdict` is JSON with `candidates: [{ path, flavor, depth }]`, relative to
//! the install folder. The executable is offered only when exactly one
//! candidate of the preferred flavor (or one shallowest) exists; otherwise the
//! install folder is returned without executable.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::fs_guard::{
    MAX_ROWS, clean_title, directory_title, existing_dir, file_within, open_database,
};
use super::{DetectedGame, GameKind, Launcher, LauncherOutcome, SourceError, conclude};

pub fn default_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(config) = dirs::config_dir() {
        roots.push(config.join("itch"));
    }
    if cfg!(target_os = "linux")
        && let Some(home) = dirs::home_dir()
    {
        roots.push(home.join(".var/app/io.itch.itch/config/itch"));
    }
    roots
}

pub fn scan(roots: &[PathBuf]) -> LauncherOutcome {
    let mut games = Vec::new();
    let mut problems = Vec::new();
    let mut seen = false;
    for root in roots {
        match read_root(root) {
            Ok(found) => {
                seen = true;
                games.extend(found);
            }
            Err(SourceError::Missing) => {}
            Err(SourceError::Unreadable(reason)) => {
                seen = true;
                problems.push(format!("butler.db: {reason}"));
            }
        }
    }
    conclude(seen, games, problems)
}

fn read_root(root: &Path) -> Result<Vec<DetectedGame>, SourceError> {
    let connection = open_database(&root.join("db/butler.db"))?;
    let unreadable = |_| SourceError::Unreadable("unsupported database layout".to_owned());
    let mut statement = connection
        .prepare(
            "SELECT c.game_id, g.title, c.install_folder_name, c.custom_install_folder, l.path, c.verdict \
             FROM caves c JOIN games g ON g.id = c.game_id \
             LEFT JOIN install_locations l ON l.id = c.install_location_id LIMIT ?1",
        )
        .map_err(unreadable)?;
    let rows = statement
        .query_map([MAX_ROWS as i64], |row| {
            Ok((
                row.get::<_, Option<i64>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(unreadable)?;
    let mut games = Vec::new();
    for row in rows {
        let (game_id, title, folder_name, custom, location, verdict) = row.map_err(unreadable)?;
        let install_dir = custom
            .filter(|value| !value.is_empty())
            .and_then(|value| existing_dir(&value))
            .or_else(|| {
                let folder = folder_name.filter(|name| {
                    !name.is_empty() && !name.contains(['/', '\\']) && name != ".." && name != "."
                })?;
                existing_dir(&Path::new(&location?).join(folder).to_string_lossy())
            });
        let Some(install_dir) = install_dir else {
            continue;
        };
        let Some(title) = title
            .as_deref()
            .and_then(clean_title)
            .or_else(|| directory_title(&install_dir))
        else {
            continue;
        };
        let (kind, executable) = verdict
            .as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .map_or_else(
                || (host_kind(), None),
                |verdict| pick_candidate(&verdict, &install_dir),
            );
        let mut game = DetectedGame::new(Launcher::Itch, title, &install_dir, kind);
        game.executable = executable.map(|path| path.to_string_lossy().into_owned());
        game.store_id = game_id.map(|id| id.to_string());
        games.push(game);
    }
    Ok(games)
}

fn host_kind() -> GameKind {
    if cfg!(windows) {
        GameKind::Windows
    } else {
        GameKind::Native
    }
}

fn flavor_kind(flavor: &str) -> Option<GameKind> {
    match flavor {
        "windows" => Some(GameKind::Windows),
        "linux" => Some(GameKind::Native),
        _ => None,
    }
}

fn pick_candidate(verdict: &Value, install_dir: &Path) -> (GameKind, Option<PathBuf>) {
    let candidates: Vec<(GameKind, &str, i64)> = verdict["candidates"]
        .as_array()
        .into_iter()
        .flatten()
        .take(MAX_ROWS)
        .filter_map(|candidate| {
            Some((
                flavor_kind(candidate["flavor"].as_str()?)?,
                candidate["path"].as_str()?,
                candidate["depth"].as_i64().unwrap_or(0),
            ))
        })
        .collect();
    let preferred = if candidates.iter().any(|(kind, ..)| *kind == host_kind()) {
        host_kind()
    } else if let Some((kind, ..)) = candidates.first() {
        *kind
    } else {
        return (host_kind(), None);
    };
    let matching: Vec<_> = candidates
        .iter()
        .filter(|(kind, ..)| *kind == preferred)
        .collect();
    let shallowest = matching.iter().map(|(_, _, depth)| *depth).min();
    let mut best = matching
        .iter()
        .filter(|(_, _, depth)| Some(*depth) == shallowest);
    let executable = match (best.next(), best.next()) {
        (Some((_, path, _)), None) => file_within(install_dir, path),
        _ => None,
    };
    (preferred, executable)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::Connection;

    use super::super::fs_guard::tests::temp;
    use super::*;

    fn database(root: &Path) -> Connection {
        fs::create_dir_all(root.join("db")).unwrap();
        let connection = Connection::open(root.join("db/butler.db")).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE games (id INTEGER PRIMARY KEY, title TEXT);
                 CREATE TABLE install_locations (id TEXT PRIMARY KEY, path TEXT);
                 CREATE TABLE caves (id TEXT PRIMARY KEY, game_id INTEGER, install_location_id TEXT, install_folder_name TEXT, custom_install_folder TEXT, verdict TEXT);",
            )
            .unwrap();
        connection
    }

    #[test]
    fn offers_executable_only_when_unambiguous() {
        let root = temp();
        let itch = root.join("itch");
        let apps = root.join("apps");
        for name in ["one", "two", "three", "four"] {
            fs::create_dir_all(apps.join(name)).unwrap();
        }
        for file in [
            "one/a.exe",
            "one/b.sh",
            "two/x.exe",
            "two/y.exe",
            "four/z.exe",
        ] {
            fs::write(apps.join(file), "MZ").unwrap();
        }
        let connection = database(&itch);
        connection
            .execute(
                "INSERT INTO install_locations VALUES ('loc', ?1)",
                [apps.to_str().unwrap()],
            )
            .unwrap();
        for (id, title) in [(1, "One"), (2, "Two"), (3, "Three"), (4, "Four")] {
            connection
                .execute("INSERT INTO games VALUES (?1, ?2)", (id, title))
                .unwrap();
        }
        let verdicts = [
            r#"{"candidates": [{"path": "a.exe", "flavor": "windows", "depth": 0}, {"path": "b.sh", "flavor": "linux", "depth": 0}]}"#,
            r#"{"candidates": [{"path": "x.exe", "flavor": "windows", "depth": 0}, {"path": "y.exe", "flavor": "windows", "depth": 0}]}"#,
            "not json",
            r#"{"candidates": [{"path": "../one/a.exe", "flavor": "windows", "depth": 0}]}"#,
        ];
        for (index, (name, verdict)) in ["one", "two", "three", "four"]
            .iter()
            .zip(verdicts)
            .enumerate()
        {
            connection
                .execute(
                    "INSERT INTO caves VALUES (?1, ?2, 'loc', ?3, NULL, ?4)",
                    (index.to_string(), index as i64 + 1, name, verdict),
                )
                .unwrap();
        }
        drop(connection);
        let LauncherOutcome::Found { games, .. } = scan(&[itch]) else {
            panic!("expected games");
        };
        assert_eq!(games.len(), 4);
        let one = games.iter().find(|game| game.title == "One").unwrap();
        let expected = if cfg!(windows) { "a.exe" } else { "b.sh" };
        assert!(one.executable.as_deref().unwrap().ends_with(expected));
        assert_eq!(one.store_id.as_deref(), Some("1"));
        for title in ["Two", "Three", "Four"] {
            assert_eq!(
                games
                    .iter()
                    .find(|game| game.title == title)
                    .unwrap()
                    .executable,
                None,
                "{title}"
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn skips_missing_and_hostile_folders_and_reports_bad_database() {
        let root = temp();
        let itch = root.join("itch");
        assert_eq!(
            scan(std::slice::from_ref(&itch)),
            LauncherOutcome::NotInstalled
        );
        let connection = database(&itch);
        connection
            .execute(
                "INSERT INTO install_locations VALUES ('loc', ?1)",
                [root.to_str().unwrap()],
            )
            .unwrap();
        connection
            .execute("INSERT INTO games VALUES (1, 'Gone')", [])
            .unwrap();
        connection
            .execute(
                "INSERT INTO caves VALUES ('a', 1, 'loc', 'missing', NULL, NULL)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO caves VALUES ('b', 1, 'loc', '..', NULL, NULL)",
                [],
            )
            .unwrap();
        drop(connection);
        let LauncherOutcome::Found { games, .. } = scan(std::slice::from_ref(&itch)) else {
            panic!("expected found");
        };
        assert!(games.is_empty());
        fs::write(
            itch.join("db/butler.db"),
            "garbage that is not sqlite at all",
        )
        .unwrap();
        assert!(matches!(scan(&[itch]), LauncherOutcome::Unreadable { .. }));
        fs::remove_dir_all(root).unwrap();
    }
}
