use rusqlite::{Transaction, TransactionBehavior, params};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    database::{Database, DatabaseState, database_error, required_name},
    steam_local::{self, SteamScan},
};

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamImportResult {
    pub detected: usize,
    // Counts refer to library rows, including existing duplicate Steam App IDs.
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub diagnostics: Vec<String>,
}

pub fn import_default_installations(state: &DatabaseState) -> Result<SteamImportResult, String> {
    let database = state.database()?;
    import_scan(database, steam_local::scan_default_installations())
}

fn import_scan(database: &Database, scan: SteamScan) -> Result<SteamImportResult, String> {
    database.with_connection(|connection| {
        let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)
            .map_err(database_error)?;
        let mut result = SteamImportResult {
            detected: scan.games.len(),
            inserted: 0,
            updated: 0,
            unchanged: 0,
            removed: 0,
            diagnostics: scan.diagnostics,
        };
        {
            let mut remove = transaction
                .prepare("DELETE FROM games WHERE steam_app_id = ?1 AND automatic_name = ?2 AND steam_install_path = ?3 AND name_override IS NULL")
                .map_err(database_error)?;
            for app in scan.excluded_non_games {
                if let Ok(name) = required_name(app.name, "automatic name") {
                    result.removed += remove.execute(params![app.app_id, name, app.install_path]).map_err(database_error)?;
                }
            }
            let mut count = transaction
                .prepare("SELECT COUNT(*) FROM games WHERE steam_app_id = ?1")
                .map_err(database_error)?;
            let mut insert = transaction
                .prepare(
                    "INSERT INTO games (id, steam_app_id, automatic_name, steam_install_path)
                     VALUES (?1, ?2, ?3, ?4)",
                )
                .map_err(database_error)?;
            let mut update = transaction
                .prepare(
                    "UPDATE games SET automatic_name = ?2, steam_install_path = ?3
                     WHERE steam_app_id = ?1
                       AND (automatic_name IS NOT ?2 OR steam_install_path IS NOT ?3)",
                )
                .map_err(database_error)?;
            for game in scan.games {
                let name = required_name(game.name, "automatic name")?;
                let existing: i64 = count
                    .query_row([game.app_id], |row| row.get(0))
                    .map_err(database_error)?;
                let existing = usize::try_from(existing)
                    .map_err(|_| "local database game count exceeds platform limits".to_owned())?;
                if existing == 0 {
                    insert
                        .execute(params![
                            Uuid::new_v4().to_string(),
                            game.app_id,
                            name,
                            game.install_path
                        ])
                        .map_err(database_error)?;
                    result.inserted += 1;
                } else {
                    let changed = update
                        .execute(params![game.app_id, name, game.install_path])
                        .map_err(database_error)?;
                    result.updated += changed;
                    result.unchanged += existing - changed;
                }
            }
        }
        transaction.commit().map_err(database_error)?;
        Ok(result)
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use crate::database::{CreateGameInput, UpdateGameInput};

    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let directory =
                std::env::temp_dir().join(format!("legio-steam-import-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&directory).unwrap();
            Self(directory)
        }

        fn install(&self, app_id: u32, name: &str, directory: &str) {
            let steamapps = self.0.join("steam/steamapps");
            fs::create_dir_all(steamapps.join("common").join(directory)).unwrap();
            fs::write(
                steamapps.join(format!("appmanifest_{app_id}.acf")),
                format!(
                    r#""AppState" {{ "appid" "{app_id}" "name" "{name}" "installdir" "{directory}" }}"#
                ),
            )
            .unwrap();
            let mut ids = fs::read_dir(&steamapps)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .filter_map(|name| {
                    name.to_str().and_then(|name| {
                        name.strip_prefix("appmanifest_")
                            .and_then(|name| name.strip_suffix(".acf"))
                            .and_then(|id| id.parse::<u32>().ok())
                    })
                })
                .collect::<Vec<_>>();
            ids.sort_unstable();
            fs::create_dir_all(self.0.join("steam/appcache")).unwrap();
            fs::write(
                self.0.join("steam/appcache/appinfo.vdf"),
                steam_local::appinfo_fixture(
                    &ids.iter().map(|id| (*id, "Game")).collect::<Vec<_>>(),
                ),
            )
            .unwrap();
        }

        fn scan(&self) -> SteamScan {
            steam_local::scan_installations([self.0.join("steam")])
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn imports_persistently_and_refreshes_without_replacing_ids_or_overrides() {
        let fixture = Fixture::new();
        fixture.install(400, "Portal", "Portal");
        fixture.install(570, "Dota 2", "dota 2 beta");
        let database = Database::open(&fixture.0).unwrap();
        let manual = database
            .create_game(CreateGameInput {
                name: "My Portal".to_owned(),
                steam_app_id: Some(400),
            })
            .unwrap();
        let unrelated = database
            .create_game(CreateGameInput {
                name: "Unrelated".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        let first = import_scan(&database, fixture.scan()).unwrap();
        assert_eq!(
            (
                first.detected,
                first.inserted,
                first.updated,
                first.unchanged
            ),
            (2, 1, 1, 0)
        );
        let initial = database.games().unwrap();
        let portal = initial.iter().find(|game| game.id == manual.id).unwrap();
        assert_eq!(portal.name, "My Portal");
        assert_eq!(portal.automatic_name.as_deref(), Some("Portal"));
        assert_eq!(
            portal.steam_install_path.as_deref(),
            fixture
                .0
                .join("steam/steamapps/common/Portal")
                .canonicalize()
                .unwrap()
                .to_str()
        );
        assert!(initial.contains(&unrelated));
        drop(database);
        let database = Database::open(&fixture.0).unwrap();
        assert_eq!(database.games().unwrap(), initial);
        let repeated = import_scan(&database, fixture.scan()).unwrap();
        assert_eq!(
            (repeated.inserted, repeated.updated, repeated.unchanged),
            (0, 0, 2)
        );
        assert_eq!(database.games().unwrap(), initial);

        fixture.install(400, "Portal refreshed", "Portal moved");
        let refreshed = import_scan(&database, fixture.scan()).unwrap();
        assert_eq!(
            (refreshed.inserted, refreshed.updated, refreshed.unchanged),
            (0, 1, 1)
        );
        let games = database.games().unwrap();
        let portal = games.iter().find(|game| game.id == manual.id).unwrap();
        assert_eq!(portal.name, "My Portal");
        assert_eq!(portal.automatic_name.as_deref(), Some("Portal refreshed"));
        assert!(
            std::path::Path::new(portal.steam_install_path.as_deref().unwrap())
                .ends_with("Portal moved")
        );
        assert!(
            games.contains(
                initial
                    .iter()
                    .find(|game| game.steam_app_id == Some(570))
                    .unwrap()
            )
        );

        let renamed = database
            .update_game(UpdateGameInput {
                id: portal.id.clone(),
                steam_app_id: portal.steam_app_id,
                automatic_name: portal.automatic_name.clone(),
                name_override: Some("Another name".to_owned()),
            })
            .unwrap();
        assert_eq!(renamed.steam_install_path, portal.steam_install_path);
        let reassigned = database
            .update_game(UpdateGameInput {
                id: renamed.id,
                steam_app_id: Some(999),
                automatic_name: renamed.automatic_name,
                name_override: renamed.name_override,
            })
            .unwrap();
        assert_eq!(reassigned.steam_install_path, None);
    }

    #[test]
    fn removes_only_unchanged_automatic_rows_for_non_games() {
        let fixture = Fixture::new();
        fixture.install(1, "Game", "Game");
        fixture.install(2, "Compatibility", "Compatibility");
        fixture.install(3, "Runtime", "Runtime");
        let database = Database::open(&fixture.0).unwrap();
        assert_eq!(import_scan(&database, fixture.scan()).unwrap().inserted, 3);
        let edited = database
            .games()
            .unwrap()
            .into_iter()
            .find(|game| game.steam_app_id == Some(3))
            .unwrap();
        database
            .update_game(UpdateGameInput {
                id: edited.id.clone(),
                steam_app_id: edited.steam_app_id,
                automatic_name: edited.automatic_name,
                name_override: Some("Keep my edited row".to_owned()),
            })
            .unwrap();
        let manual = database
            .create_game(CreateGameInput {
                name: "My tool entry".to_owned(),
                steam_app_id: Some(2),
            })
            .unwrap();
        fs::write(
            fixture.0.join("steam/appcache/appinfo.vdf"),
            steam_local::appinfo_fixture(&[(1, "Game"), (2, "Tool"), (3, "Tool")]),
        )
        .unwrap();
        let result = import_scan(&database, fixture.scan()).unwrap();
        assert_eq!((result.detected, result.removed), (1, 1));
        let games = database.games().unwrap();
        assert_eq!(games.len(), 3);
        assert!(games.iter().any(|game| game.id == manual.id));
        assert!(
            games
                .iter()
                .any(|game| game.id == edited.id && game.name == "Keep my edited row")
        );
        assert!(games.iter().any(|game| game.steam_app_id == Some(1)));
    }

    #[test]
    fn refreshes_all_duplicate_matches_without_merging_manual_rows() {
        let fixture = Fixture::new();
        fixture.install(400, "Portal", "Portal");
        let database = Database::open(&fixture.0).unwrap();
        let originals: Vec<_> = ["First copy", "Second copy"]
            .into_iter()
            .map(|name| {
                database
                    .create_game(CreateGameInput {
                        name: name.to_owned(),
                        steam_app_id: Some(400),
                    })
                    .unwrap()
            })
            .collect();
        let result = import_scan(&database, fixture.scan()).unwrap();
        assert_eq!(
            (result.detected, result.inserted, result.updated),
            (1, 0, 2)
        );
        let games = database.games().unwrap();
        assert_eq!(games.len(), 2);
        for original in originals {
            let game = games.iter().find(|game| game.id == original.id).unwrap();
            assert_eq!(game.name, original.name);
            assert_eq!(game.name_override, original.name_override);
            assert_eq!(game.automatic_name.as_deref(), Some("Portal"));
        }
    }

    #[test]
    fn database_failure_rolls_back_both_inserts_and_updates() {
        let fixture = Fixture::new();
        fixture.install(1, "New game", "New");
        fixture.install(2, "Refreshed game", "Refreshed");
        fixture.install(3, "Rejected game", "Rejected");
        let database = Database::open(&fixture.0).unwrap();
        database
            .create_game(CreateGameInput {
                name: "Manual name".to_owned(),
                steam_app_id: Some(2),
            })
            .unwrap();
        let original = database.games().unwrap();
        database
            .with_connection(|connection| {
                connection
                    .execute_batch(
                        "CREATE TRIGGER reject_import BEFORE INSERT ON games
                 WHEN NEW.steam_app_id = 3 BEGIN SELECT RAISE(ABORT, 'rejected import'); END;",
                    )
                    .map_err(database_error)
            })
            .unwrap();
        assert!(import_scan(&database, fixture.scan()).is_err());
        assert_eq!(database.games().unwrap(), original);
        drop(database);
        assert_eq!(
            Database::open(&fixture.0).unwrap().games().unwrap(),
            original
        );
    }

    #[test]
    fn empty_and_partial_scans_preserve_existing_library_and_report_diagnostics() {
        let fixture = Fixture::new();
        let database = Database::open(&fixture.0).unwrap();
        let manual = database
            .create_game(CreateGameInput {
                name: "Still in library".to_owned(),
                steam_app_id: Some(999),
            })
            .unwrap();
        let empty = import_scan(&database, fixture.scan()).unwrap();
        assert_eq!(
            (
                empty.detected,
                empty.inserted,
                empty.updated,
                empty.unchanged
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(database.games().unwrap(), vec![manual.clone()]);
        fixture.install(400, "Portal", "Portal");
        fs::write(
            fixture.0.join("steam/steamapps/appmanifest_2.acf"),
            "invalid",
        )
        .unwrap();
        let scan = fixture.scan();
        let diagnostics = scan.diagnostics.clone();
        assert_eq!(diagnostics.len(), 1);
        let partial = import_scan(&database, scan).unwrap();
        assert_eq!((partial.detected, partial.inserted), (1, 1));
        assert_eq!(partial.diagnostics, diagnostics);
        assert!(database.games().unwrap().contains(&manual));
    }

    #[cfg(unix)]
    #[test]
    fn never_persists_an_installation_that_escapes_its_library() {
        let fixture = Fixture::new();
        fixture.install(400, "Portal", "Portal");
        let outside = fixture.0.join("outside");
        fs::create_dir(&outside).unwrap();
        let installed = fixture.0.join("steam/steamapps/common/Portal");
        fs::remove_dir(&installed).unwrap();
        std::os::unix::fs::symlink(outside, installed).unwrap();
        let database = Database::open(&fixture.0).unwrap();
        let result = import_scan(&database, fixture.scan()).unwrap();
        assert_eq!((result.detected, result.inserted), (0, 0));
        assert_eq!(result.diagnostics.len(), 1);
        assert!(database.games().unwrap().is_empty());
    }
}
