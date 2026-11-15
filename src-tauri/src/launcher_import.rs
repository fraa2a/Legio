//! Read-only detection of games installed through other launchers (Heroic,
//! Lutris, itch, Bottles) and their import into the library.
//!
//! Everything read from other launchers is untrusted. Nothing is ever written
//! to their folders: configuration files are read with size caps, SQLite
//! databases are opened read-only and immutable. Detected games are imported
//! through the manual import path, so executables get the same validation and
//! duplicate detection as a game added by hand.

mod bottles;
mod fs_guard;
mod heroic;
mod itch;
mod lutris;
mod yaml_scalars;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::database::{Database, Game, GameCompatibilityOverrides};
use crate::manual_import::{self, ManualImportInput};

/// Upper bound on games offered per launcher.
const MAX_GAMES: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Launcher {
    Heroic,
    Lutris,
    Itch,
    Bottles,
}

impl Launcher {
    const ALL: [Launcher; 4] = [Self::Heroic, Self::Lutris, Self::Itch, Self::Bottles];

    fn slug(self) -> &'static str {
        match self {
            Self::Heroic => "heroic",
            Self::Lutris => "lutris",
            Self::Itch => "itch",
            Self::Bottles => "bottles",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GameKind {
    Native,
    Windows,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedGame {
    /// Stable identifier built from the launcher, store id and paths.
    pub key: String,
    pub launcher: Launcher,
    pub title: String,
    pub install_dir: String,
    pub executable: Option<String>,
    pub kind: GameKind,
    /// Existing Wine prefix directory of the game, when the launcher records one.
    pub prefix: Option<String>,
    /// Runner name recorded by the launcher. Informational only.
    pub runner_hint: Option<String>,
    pub store_id: Option<String>,
}

impl DetectedGame {
    fn new(launcher: Launcher, title: String, install_dir: &Path, kind: GameKind) -> Self {
        Self {
            key: String::new(),
            launcher,
            title,
            install_dir: install_dir.to_string_lossy().into_owned(),
            executable: None,
            kind,
            prefix: None,
            runner_hint: None,
            store_id: None,
        }
    }

    fn seal(mut self) -> Self {
        self.key = format!(
            "{}|{}|{}",
            self.launcher.slug(),
            self.store_id.as_deref().unwrap_or(&self.title),
            self.executable.as_deref().unwrap_or(&self.install_dir)
        );
        self
    }
}

/// Why a launcher source could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceError {
    Missing,
    Unreadable(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum LauncherOutcome {
    NotInstalled,
    Unreadable {
        reason: String,
    },
    Found {
        games: Vec<DetectedGame>,
        warnings: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LauncherReport {
    pub launcher: Launcher,
    #[serde(flatten)]
    pub outcome: LauncherOutcome,
}

/// `seen_source` is true when a launcher folder or database exists at all.
fn conclude(seen_source: bool, games: Vec<DetectedGame>, problems: Vec<String>) -> LauncherOutcome {
    if !seen_source {
        return LauncherOutcome::NotInstalled;
    }
    if games.is_empty() && !problems.is_empty() {
        return LauncherOutcome::Unreadable {
            reason: problems.join("; "),
        };
    }
    let mut seen = std::collections::HashSet::new();
    let games = games
        .into_iter()
        .map(DetectedGame::seal)
        .filter(|game| seen.insert(game.key.clone()))
        .take(MAX_GAMES)
        .collect();
    LauncherOutcome::Found {
        games,
        warnings: problems,
    }
}

fn scan_launcher(launcher: Launcher) -> LauncherOutcome {
    match launcher {
        Launcher::Heroic => heroic::scan(&heroic::default_roots()),
        Launcher::Lutris => lutris::scan(&lutris::default_roots()),
        Launcher::Itch => itch::scan(&itch::default_roots()),
        Launcher::Bottles => bottles::scan(&bottles::default_roots()),
    }
}

/// Scans every launcher independently; a failure in one never affects the others.
pub fn scan_all() -> Vec<LauncherReport> {
    Launcher::ALL
        .into_iter()
        .map(|launcher| {
            let outcome = scan_launcher(launcher);
            if let LauncherOutcome::Unreadable { reason } = &outcome {
                crate::application_log::failure("scan_launcher", format!("{launcher:?}: {reason}"));
            }
            LauncherReport { launcher, outcome }
        })
        .collect()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSelection {
    pub launcher: Launcher,
    pub key: String,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ImportOutcome {
    Imported { warning: Option<String> },
    Duplicate,
    Failed { reason: String },
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    pub launcher: Launcher,
    pub key: String,
    pub title: String,
    #[serde(flatten)]
    pub outcome: ImportOutcome,
}

/// Imports the selected games, matching them against a fresh scan so only
/// paths found in launcher manifests can be imported.
pub fn import_selected(
    database: &Database,
    selection: Vec<ImportSelection>,
    after_import: impl FnMut(&Game) -> Option<String>,
) -> Result<Vec<ImportItem>, String> {
    let mut detected = Vec::new();
    for launcher in Launcher::ALL {
        if selection.iter().any(|item| item.launcher == launcher)
            && let LauncherOutcome::Found { games, .. } = scan_launcher(launcher)
        {
            detected.extend(games);
        }
    }
    import_detected(database, &detected, selection, after_import)
}

fn import_detected(
    database: &Database,
    detected: &[DetectedGame],
    selection: Vec<ImportSelection>,
    mut after_import: impl FnMut(&Game) -> Option<String>,
) -> Result<Vec<ImportItem>, String> {
    let mut library = database.games()?;
    let mut items = Vec::new();
    for item in selection {
        let game = detected
            .iter()
            .find(|game| game.launcher == item.launcher && game.key == item.key);
        let (title, outcome) = match game {
            None => (
                item.key.clone(),
                ImportOutcome::Failed {
                    reason: "The game is no longer detected in the launcher".to_owned(),
                },
            ),
            Some(game) => (
                game.title.clone(),
                import_one(database, game, &mut library, &mut after_import),
            ),
        };
        items.push(ImportItem {
            launcher: item.launcher,
            key: item.key,
            title,
            outcome,
        });
    }
    Ok(items)
}

fn import_one(
    database: &Database,
    game: &DetectedGame,
    library: &mut Vec<Game>,
    after_import: &mut impl FnMut(&Game) -> Option<String>,
) -> ImportOutcome {
    let Some(executable) = game.executable.as_deref() else {
        return ImportOutcome::Failed {
            reason: "The launcher does not record the executable".to_owned(),
        };
    };
    if is_in_library(library, Path::new(&game.install_dir), Path::new(executable)) {
        return ImportOutcome::Duplicate;
    }
    let imported = match manual_import::import_into(
        database,
        ManualImportInput {
            executable_path: executable.to_owned(),
            name: Some(game.title.clone()),
        },
    ) {
        Ok(imported) => imported,
        Err(reason) if reason == "executable is already in the library" => {
            return ImportOutcome::Duplicate;
        }
        Err(reason) => return ImportOutcome::Failed { reason },
    };
    library.push(imported.clone());
    let mut warnings = Vec::new();
    if cfg!(target_os = "linux")
        && game.kind == GameKind::Windows
        && let Some(prefix) = &game.prefix
        && let Err(error) = database.save_game_compatibility_overrides(
            &imported.id,
            GameCompatibilityOverrides {
                prefix_path: Some(prefix.clone()),
                ..GameCompatibilityOverrides::default()
            },
        )
    {
        warnings.push(format!("The Wine prefix was not applied: {error}"));
    }
    warnings.extend(after_import(&imported));
    ImportOutcome::Imported {
        warning: (!warnings.is_empty()).then(|| warnings.join(" ")),
    }
}

/// A game is a duplicate when the same executable, or an install directory
/// already used by another entry (including Steam installs), is in the library.
fn is_in_library(library: &[Game], install_dir: &Path, executable: &Path) -> bool {
    let canonical = |path: &Path| dunce::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    let install_dir = canonical(install_dir);
    let executable = canonical(executable);
    library.iter().any(|game| {
        let same = |value: &Option<String>| {
            value
                .as_deref()
                .is_some_and(|value| canonical(Path::new(value)) == install_dir)
        };
        let contains = |value: &Option<String>| {
            value
                .as_deref()
                .is_some_and(|value| executable.starts_with(canonical(Path::new(value))))
        };
        game.executable_path
            .as_deref()
            .is_some_and(|value| canonical(Path::new(value)) == executable)
            || same(&game.steam_install_path)
            || same(&game.installation_root)
            || contains(&game.steam_install_path)
            || contains(&game.installation_root)
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::fs_guard::tests::temp;
    use super::*;

    fn detected(root: &Path, name: &str) -> DetectedGame {
        let directory = root.join(name);
        fs::create_dir_all(&directory).unwrap();
        let executable = directory.join("game.exe");
        fs::write(&executable, b"MZ").unwrap();
        let mut game = DetectedGame::new(
            Launcher::Heroic,
            name.to_owned(),
            &directory,
            GameKind::Windows,
        );
        game.executable = Some(executable.to_string_lossy().into_owned());
        game.seal()
    }

    fn select(game: &DetectedGame) -> ImportSelection {
        ImportSelection {
            launcher: game.launcher,
            key: game.key.clone(),
        }
    }

    #[test]
    fn imports_once_and_skips_duplicates() {
        let root = temp();
        let database = Database::open(&root).unwrap();
        let first = detected(&root, "First");
        let missing_exe = {
            let mut game = detected(&root, "Second");
            game.executable = None;
            game.seal()
        };
        let all = [first.clone(), missing_exe.clone()];
        let run = |selection: Vec<ImportSelection>| {
            import_detected(&database, &all, selection, |_| None).unwrap()
        };
        let items = run(vec![select(&first), select(&missing_exe)]);
        assert!(matches!(items[0].outcome, ImportOutcome::Imported { .. }));
        assert!(matches!(items[1].outcome, ImportOutcome::Failed { .. }));
        let games = database.games().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "First");
        assert_eq!(
            run(vec![select(&first)])[0].outcome,
            ImportOutcome::Duplicate
        );
        assert_eq!(database.games().unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn skips_install_directory_already_imported_from_steam() {
        let root = temp();
        let game = detected(&root, "Portal");
        let steam = Game {
            id: "id".to_owned(),
            steam_app_id: Some(400),
            automatic_name: Some("Portal".to_owned()),
            name_override: None,
            name: "Portal".to_owned(),
            steam_install_path: Some(game.install_dir.clone()),
            steam_account_id: None,
            executable_path: None,
            installation_root: None,
        };
        assert!(is_in_library(
            &[steam],
            Path::new(&game.install_dir),
            Path::new(game.executable.as_deref().unwrap())
        ));
        assert!(!is_in_library(
            &[],
            Path::new(&game.install_dir),
            Path::new(game.executable.as_deref().unwrap())
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn applies_detected_prefix_to_windows_games() {
        let root = temp();
        let database = Database::open(&root).unwrap();
        let prefix = root.join("prefix");
        fs::create_dir(&prefix).unwrap();
        let mut game = detected(&root, "Wine game");
        game.prefix = Some(prefix.to_string_lossy().into_owned());
        import_detected(&database, &[game.clone()], vec![select(&game)], |_| None).unwrap();
        let id = database.games().unwrap().remove(0).id;
        let overrides = database.game_compatibility_overrides(&id).unwrap();
        let expected = dunce::canonicalize(&prefix).unwrap();
        assert_eq!(overrides.prefix_path.as_deref(), expected.to_str());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unknown_selection_fails_and_missing_sources_mean_not_installed() {
        let root = temp();
        let database = Database::open(&root).unwrap();
        let selection = vec![ImportSelection {
            launcher: Launcher::Itch,
            key: "x".to_owned(),
        }];
        let items = import_detected(&database, &[], selection, |_| None).unwrap();
        assert!(matches!(items[0].outcome, ImportOutcome::Failed { .. }));
        assert_eq!(
            conclude(false, Vec::new(), Vec::new()),
            LauncherOutcome::NotInstalled
        );
        assert!(matches!(
            conclude(true, Vec::new(), vec!["bad".to_owned()]),
            LauncherOutcome::Unreadable { .. }
        ));
        fs::remove_dir_all(root).unwrap();
    }
}
