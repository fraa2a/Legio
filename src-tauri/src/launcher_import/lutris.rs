//! Lutris reader (Linux only).
//!
//! Locations: data `~/.local/share/lutris/pga.db` and config
//! `~/.config/lutris/games/<configpath>.yml`, or the Flatpak equivalents under
//! `~/.var/app/net.lutris.Lutris/{data,config}/lutris`.
//!
//! `pga.db` is SQLite, opened read-only and immutable. Table `games` columns
//! used: `name`, `slug`, `runner`, `directory`, `installed`, `configpath`.
//! Only the `wine` (Windows) and `linux` (native) runners are offered. The
//! per-game YAML supplies `game.exe`, `game.prefix` and `wine.version` through
//! the tolerant scalar extractor; an uncertain value is skipped.

use std::path::{Path, PathBuf};

use super::fs_guard::{
    MAX_ROWS, MAX_YAML_BYTES, clean_title, existing_dir, existing_file, is_safe_name,
    open_database, read_text_within,
};
use super::yaml_scalars::YamlScalars;
use super::{DetectedGame, GameKind, Launcher, LauncherOutcome, SourceError, conclude};

pub struct LutrisRoot {
    pub data: PathBuf,
    pub config: PathBuf,
}

pub fn default_roots() -> Vec<LutrisRoot> {
    if !cfg!(target_os = "linux") {
        return Vec::new();
    }
    let mut roots = Vec::new();
    if let (Some(data), Some(config)) = (dirs::data_dir(), dirs::config_dir()) {
        roots.push(LutrisRoot {
            data: data.join("lutris"),
            config: config.join("lutris"),
        });
    }
    if let Some(home) = dirs::home_dir() {
        let flatpak = home.join(".var/app/net.lutris.Lutris");
        roots.push(LutrisRoot {
            data: flatpak.join("data/lutris"),
            config: flatpak.join("config/lutris"),
        });
    }
    roots
}

pub fn scan(roots: &[LutrisRoot]) -> LauncherOutcome {
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
                problems.push(format!("pga.db: {reason}"));
            }
        }
    }
    conclude(seen, games, problems)
}

fn read_root(root: &LutrisRoot) -> Result<Vec<DetectedGame>, SourceError> {
    let connection = open_database(&root.data.join("pga.db"))?;
    let unreadable = |_| SourceError::Unreadable("unsupported database layout".to_owned());
    let mut statement = connection
        .prepare(
            "SELECT name, slug, runner, directory, configpath FROM games \
             WHERE installed = 1 AND runner IN ('wine', 'linux') LIMIT ?1",
        )
        .map_err(unreadable)?;
    let rows = statement
        .query_map([MAX_ROWS as i64], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(unreadable)?;
    let mut games = Vec::new();
    for row in rows {
        let (name, slug, runner, directory, configpath) = row.map_err(unreadable)?;
        let kind = match runner.as_deref() {
            Some("wine") => GameKind::Windows,
            _ => GameKind::Native,
        };
        let yaml = configpath
            .as_deref()
            .filter(|value| is_safe_name(value))
            .and_then(|value| {
                let path = root.config.join("games").join(format!("{value}.yml"));
                read_text_within(&root.config, &path, MAX_YAML_BYTES).ok()
            })
            .map(|text| YamlScalars::parse(&text));
        let directory = directory.as_deref().and_then(existing_dir);
        let executable = yaml
            .as_ref()
            .and_then(|yaml| yaml.scalar(&["game", "exe"]))
            .and_then(|value| resolve_executable(&value, directory.as_deref()));
        let Some(install_dir) = directory.or_else(|| {
            executable
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
        }) else {
            continue;
        };
        let Some(title) = name.as_deref().and_then(clean_title) else {
            continue;
        };
        let mut game = DetectedGame::new(Launcher::Lutris, title, &install_dir, kind);
        game.executable = executable.map(|path| path.to_string_lossy().into_owned());
        game.store_id = slug.filter(|value| is_safe_name(value));
        if kind == GameKind::Windows {
            game.prefix = yaml
                .as_ref()
                .and_then(|yaml| yaml.scalar(&["game", "prefix"]))
                .and_then(|value| existing_dir(&value))
                .map(|path| path.to_string_lossy().into_owned());
            game.runner_hint = yaml
                .as_ref()
                .and_then(|yaml| yaml.scalar(&["wine", "version"]))
                .and_then(|value| clean_title(&value));
        }
        games.push(game);
    }
    Ok(games)
}

/// Absolute paths are used as they are; relative ones are joined to the game directory.
fn resolve_executable(value: &str, directory: Option<&Path>) -> Option<PathBuf> {
    if Path::new(value).is_absolute() {
        return existing_file(value);
    }
    super::fs_guard::file_within(directory?, value)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::Connection;

    use super::super::fs_guard::tests::temp;
    use super::*;

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = temp();
            fs::create_dir_all(root.join("data")).unwrap();
            fs::create_dir_all(root.join("config/games")).unwrap();
            let connection = Connection::open(root.join("data/pga.db")).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE games (id INTEGER PRIMARY KEY, name TEXT, slug TEXT, runner TEXT, directory TEXT, installed INTEGER, configpath TEXT);",
                )
                .unwrap();
            Self { root }
        }

        fn add(&self, name: &str, runner: &str, directory: &str, installed: i32, configpath: &str) {
            Connection::open(self.root.join("data/pga.db"))
                .unwrap()
                .execute(
                    "INSERT INTO games (name, slug, runner, directory, installed, configpath) VALUES (?1, ?1, ?2, ?3, ?4, ?5)",
                    (name, runner, directory, installed, configpath),
                )
                .unwrap();
        }

        fn roots(&self) -> Vec<LutrisRoot> {
            vec![LutrisRoot {
                data: self.root.join("data"),
                config: self.root.join("config"),
            }]
        }
    }

    #[test]
    fn reads_installed_wine_and_linux_games() {
        let fixture = Fixture::new();
        let root = &fixture.root;
        let wine_dir = root.join("wine-game");
        let native_dir = root.join("native");
        let prefix = root.join("prefix");
        for dir in [&wine_dir, &native_dir, &prefix] {
            fs::create_dir(dir).unwrap();
        }
        fs::write(wine_dir.join("game.exe"), "MZ").unwrap();
        fs::write(native_dir.join("run"), "").unwrap();
        fixture.add(
            "wine-game",
            "wine",
            wine_dir.to_str().unwrap(),
            1,
            "wine-game-1",
        );
        fixture.add(
            "native",
            "linux",
            native_dir.to_str().unwrap(),
            1,
            "native-1",
        );
        fixture.add(
            "removed",
            "wine",
            wine_dir.to_str().unwrap(),
            0,
            "removed-1",
        );
        fixture.add("emulated", "dosbox", wine_dir.to_str().unwrap(), 1, "dos-1");
        fs::write(
            root.join("config/games/wine-game-1.yml"),
            format!(
                "game:\n  exe: {}/game.exe\n  prefix: {}\nwine:\n  version: lutris-8\n",
                wine_dir.display(),
                prefix.display()
            ),
        )
        .unwrap();
        fs::write(
            root.join("config/games/native-1.yml"),
            "game:\n  exe: run\n",
        )
        .unwrap();
        let LauncherOutcome::Found { games, .. } = scan(&fixture.roots()) else {
            panic!("expected games");
        };
        assert_eq!(games.len(), 2);
        let wine = &games[0];
        assert_eq!(wine.kind, GameKind::Windows);
        assert!(wine.executable.as_deref().unwrap().ends_with("game.exe"));
        assert!(wine.prefix.is_some());
        assert_eq!(wine.runner_hint.as_deref(), Some("lutris-8"));
        assert!(games[1].executable.as_deref().unwrap().ends_with("run"));
        fs::remove_dir_all(fixture.root).unwrap();
    }

    #[test]
    fn tolerates_hostile_yaml_and_configpath() {
        let fixture = Fixture::new();
        let dir = fixture.root.join("game");
        fs::create_dir(&dir).unwrap();
        fixture.add("odd", "wine", dir.to_str().unwrap(), 1, "../../etc/passwd");
        fixture.add("anchors", "wine", dir.to_str().unwrap(), 1, "anchors");
        fixture.add("huge", "wine", dir.to_str().unwrap(), 1, "huge");
        fs::write(
            fixture.root.join("config/games/anchors.yml"),
            "game:\n  exe: &a /bin/sh\n",
        )
        .unwrap();
        fs::write(
            fixture.root.join("config/games/huge.yml"),
            " ".repeat(300 * 1024),
        )
        .unwrap();
        let LauncherOutcome::Found { games, .. } = scan(&fixture.roots()) else {
            panic!("expected games");
        };
        assert_eq!(games.len(), 3);
        assert!(
            games
                .iter()
                .all(|game| game.executable.is_none() && game.prefix.is_none())
        );
        fs::remove_dir_all(fixture.root).unwrap();
    }

    #[test]
    fn reports_missing_and_unreadable_databases() {
        let root = temp();
        let roots = vec![LutrisRoot {
            data: root.join("data"),
            config: root.join("config"),
        }];
        assert_eq!(scan(&roots), LauncherOutcome::NotInstalled);
        fs::create_dir(root.join("data")).unwrap();
        fs::write(
            root.join("data/pga.db"),
            "this is not a database file at all",
        )
        .unwrap();
        assert!(matches!(scan(&roots), LauncherOutcome::Unreadable { .. }));
        fs::remove_dir_all(root).unwrap();
    }
}
