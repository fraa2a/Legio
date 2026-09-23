use std::{fs, path::Path, sync::Mutex};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const THEME_KEY: &str = "theme";
const SCHEMA_VERSION: i64 = 7;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Dark,
    Light,
}

impl Theme {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "system" => Ok(Self::System),
            "dark" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            _ => Err(format!("stored theme is invalid: {value}")),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateGameInput {
    pub name: String,
    pub steam_app_id: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGameInput {
    pub id: String,
    pub steam_app_id: Option<u32>,
    pub automatic_name: Option<String>,
    pub name_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub steam_app_id: Option<u32>,
    pub automatic_name: Option<String>,
    pub name_override: Option<String>,
    pub name: String,
    pub steam_install_path: Option<String>,
    pub steam_account_id: Option<String>,
    pub executable_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountCheckStatus {
    NotRequired,
    MissingSavedAccount,
    Match,
    Mismatch,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountCheck {
    pub status: AccountCheckStatus,
    pub account_requirement_met: bool,
    pub selected_steam_id: Option<String>,
    pub message: Option<&'static str>,
}

pub struct DatabaseState {
    database: Result<Database, String>,
}

impl DatabaseState {
    pub fn new(data_dir: Result<std::path::PathBuf, tauri::Error>) -> Self {
        let database = data_dir
            .map_err(|error| format!("could not resolve the application data directory: {error}"))
            .and_then(|data_dir| Database::open(&data_dir));

        Self { database }
    }

    pub(crate) fn database(&self) -> Result<&Database, String> {
        self.database.as_ref().map_err(Clone::clone)
    }
}

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub(crate) fn open(data_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(data_dir)
            .map_err(|error| format!("could not create application data directory: {error}"))?;
        let connection = Connection::open(data_dir.join("legio.sqlite3"))
            .map_err(|error| format!("could not open the local database: {error}"))?;

        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(|error| format!("could not configure the local database: {error}"))?;
        migrate(&connection)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn settings(&self) -> Result<Settings, String> {
        self.with_connection(|connection| {
            let stored_theme: Option<String> = connection
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [THEME_KEY],
                    |row| row.get(0),
                )
                .optional()
                .map_err(database_error)?;

            stored_theme.map_or_else(
                || Ok(Settings::default()),
                |value| Theme::parse(&value).map(|theme| Settings { theme }),
            )
        })
    }

    pub fn save_settings(&self, settings: Settings) -> Result<Settings, String> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![THEME_KEY, settings.theme.as_str()],
                )
                .map_err(database_error)?;
            Ok(settings)
        })
    }

    pub fn games(&self) -> Result<Vec<Game>, String> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT id, steam_app_id, automatic_name, name_override, steam_install_path, steam_account_id, executable_path
                     FROM games ORDER BY COALESCE(name_override, automatic_name), id",
                )
                .map_err(database_error)?;
            let rows = statement
                .query_map([], game_from_row)
                .map_err(database_error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
        })
    }

    pub fn game(&self, id: &str) -> Result<Game, String> {
        let id = parse_game_id(id)?;
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id, steam_app_id, automatic_name, name_override, steam_install_path, steam_account_id, executable_path FROM games WHERE id = ?1",
                    [id],
                    game_from_row,
                )
                .optional()
                .map_err(database_error)?
                .ok_or_else(|| "game was not found".to_owned())
        })
    }

    pub fn create_game(&self, input: CreateGameInput) -> Result<Game, String> {
        let name = required_name(input.name, "name")?;
        let game = Game {
            id: Uuid::new_v4().to_string(),
            steam_app_id: input.steam_app_id,
            automatic_name: None,
            name_override: Some(name.clone()),
            name,
            steam_install_path: None,
            steam_account_id: None,
            executable_path: None,
        };

        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO games (id, steam_app_id, automatic_name, name_override)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        game.id,
                        game.steam_app_id,
                        game.automatic_name,
                        game.name_override
                    ],
                )
                .map_err(database_error)?;
            Ok(game)
        })
    }

    pub fn update_game(&self, input: UpdateGameInput) -> Result<Game, String> {
        let id = parse_game_id(&input.id)?;
        let automatic_name = optional_name(input.automatic_name, "automatic name")?;
        let name_override = optional_name(input.name_override, "name override")?;
        effective_name(&automatic_name, &name_override)?;

        self.with_connection(|connection| {
            connection
                .query_row(
                    "UPDATE games
                     SET steam_install_path = CASE WHEN steam_app_id IS ?2 THEN steam_install_path ELSE NULL END,
                         steam_account_id = CASE WHEN steam_app_id IS ?2 THEN steam_account_id ELSE NULL END,
                         steam_app_id = ?2, automatic_name = ?3, name_override = ?4
                     WHERE id = ?1
                     RETURNING id, steam_app_id, automatic_name, name_override, steam_install_path, steam_account_id, executable_path",
                    params![id, input.steam_app_id, automatic_name, name_override],
                    game_from_row,
                )
                .optional()
                .map_err(database_error)?
                .ok_or_else(|| "game was not found".to_owned())
        })
    }

    pub fn set_game_steam_account(
        &self,
        game_id: &str,
        steam_id: Option<&str>,
    ) -> Result<Game, String> {
        let game_id = parse_game_id(game_id)?;
        if let Some(steam_id) = steam_id {
            parse_steam_id(steam_id)?;
        }
        self.with_connection(|connection| {
            connection.query_row(
                "UPDATE games SET steam_account_id = ?2 WHERE id = ?1 AND (steam_app_id IS NOT NULL OR ?2 IS NULL)
                 RETURNING id, steam_app_id, automatic_name, name_override, steam_install_path, steam_account_id, executable_path",
                params![game_id, steam_id], game_from_row,
            ).optional().map_err(database_error)?.ok_or_else(|| "game was not found or has no Steam App ID".to_owned())
        })
    }

    pub fn check_game_steam_account(&self, game_id: &str) -> Result<AccountCheck, String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| {
            let selected_steam_id: Option<Option<String>> = connection
                .query_row(
                    "SELECT steam_account_id FROM games WHERE id = ?1",
                    [game_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(database_error)?;
            let selected_steam_id =
                selected_steam_id.ok_or_else(|| "game was not found".to_owned())?;
            // Saved Steam accounts do not establish the active client identity.
            Ok(account_preflight(selected_steam_id, None))
        })
    }

    pub fn remove_game(&self, id: &str) -> Result<(), String> {
        let id = parse_game_id(id)?;
        self.with_connection(|connection| {
            let changed = connection
                .execute("DELETE FROM games WHERE id = ?1", [id])
                .map_err(database_error)?;
            if changed == 0 {
                return Err("game was not found".to_owned());
            }
            Ok(())
        })
    }

    pub(crate) fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "local database lock was poisoned".to_owned())?;
        operation(&connection)
    }
}

fn migrate(connection: &Connection) -> Result<(), String> {
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(database_error)?;
    if version > SCHEMA_VERSION {
        return Err(format!(
            "local database schema version {version} is newer than this Legio build supports"
        ));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }

    let transaction = connection.unchecked_transaction().map_err(database_error)?;
    if version == 0 {
        transaction
            .execute_batch(
                "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE games (
                   id TEXT PRIMARY KEY NOT NULL,
                   steam_app_id INTEGER,
                   automatic_name TEXT,
                   name_override TEXT,
                   CHECK (steam_app_id IS NULL OR steam_app_id > 0)
                 );
                 CREATE INDEX games_steam_app_id_idx ON games (steam_app_id);
                 PRAGMA user_version = 1;",
            )
            .map_err(database_error)?;
    }
    if version < 2 {
        transaction
            .execute_batch(
                "CREATE TABLE catalog_games (
                steam_app_id INTEGER PRIMARY KEY CHECK (steam_app_id BETWEEN 1 AND 4294967295),
                name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 512),
                search_name TEXT NOT NULL,
                fetched_at INTEGER NOT NULL
             );
             CREATE TABLE catalog_cache (
                provider TEXT PRIMARY KEY CHECK (provider = 'hydra'),
                query TEXT NOT NULL,
                fetched_at INTEGER NOT NULL,
                remote_count INTEGER NOT NULL CHECK (remote_count >= 0)
             );
             PRAGMA user_version = 2;",
            )
            .map_err(database_error)?;
    }
    if version < 3 {
        transaction
            .execute_batch(
                "ALTER TABLE games ADD COLUMN steam_install_path TEXT;
                 PRAGMA user_version = 3;",
            )
            .map_err(database_error)?;
    }
    if version < 4 {
        transaction
            .execute_batch(
                "CREATE TABLE steam_details_cache (
                steam_app_id INTEGER PRIMARY KEY CHECK (steam_app_id BETWEEN 1 AND 4294967295),
                details TEXT NOT NULL CHECK (length(CAST(details AS BLOB)) BETWEEN 1 AND 65536),
                fetched_at INTEGER NOT NULL CHECK (fetched_at >= 0)
            );
            PRAGMA user_version = 4;",
            )
            .map_err(database_error)?;
    }
    if version < 5 {
        transaction
            .execute_batch(
                "ALTER TABLE games ADD COLUMN steam_account_id TEXT;
             PRAGMA user_version = 5;",
            )
            .map_err(database_error)?;
    }
    if version < 6 {
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS legio_source_cache (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    manifest BLOB NOT NULL CHECK (length(manifest) BETWEEN 1 AND 2097152),
                    fetched_at INTEGER NOT NULL CHECK (fetched_at >= 0)
                );
                PRAGMA user_version = 6;",
            )
            .map_err(database_error)?;
    }
    if version < 7 {
        transaction
            .execute_batch(
                "ALTER TABLE games ADD COLUMN executable_path TEXT;
                 CREATE UNIQUE INDEX games_executable_path_idx ON games (executable_path) WHERE executable_path IS NOT NULL;
                 PRAGMA user_version = 7;",
            )
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)
}

pub(crate) fn game_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Game> {
    let steam_app_id = row
        .get::<_, Option<i64>>(1)?
        .map(|value| {
            u32::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(1, value))
        })
        .transpose()?;
    let automatic_name: Option<String> = row.get(2)?;
    let name_override: Option<String> = row.get(3)?;
    let name = name_override
        .clone()
        .or_else(|| automatic_name.clone())
        .ok_or(rusqlite::Error::InvalidQuery)?;

    Ok(Game {
        id: row.get(0)?,
        steam_app_id,
        automatic_name,
        name_override,
        name,
        steam_install_path: row.get(4)?,
        steam_account_id: row.get(5)?,
        executable_path: row.get(6)?,
    })
}

pub(crate) fn required_name(value: String, field: &str) -> Result<String, String> {
    optional_name(Some(value), field)?.ok_or_else(|| format!("{field} is required"))
}

fn optional_name(value: Option<String>, field: &str) -> Result<Option<String>, String> {
    value
        .map(|value| {
            let value = value.trim().to_owned();
            if value.is_empty() {
                return Err(format!("{field} cannot be blank"));
            }
            if value.chars().count() > 200 {
                return Err(format!("{field} cannot exceed 200 characters"));
            }
            Ok(value)
        })
        .transpose()
}

fn effective_name(
    automatic_name: &Option<String>,
    name_override: &Option<String>,
) -> Result<String, String> {
    name_override
        .clone()
        .or_else(|| automatic_name.clone())
        .ok_or_else(|| "a game needs an automatic name or a name override".to_owned())
}

fn parse_game_id(value: &str) -> Result<String, String> {
    Uuid::parse_str(value)
        .map(|id| id.to_string())
        .map_err(|_| "game id is invalid".to_owned())
}

fn parse_steam_id(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0 && id.to_string() == value)
        .ok_or_else(|| "Steam ID is invalid".to_owned())
}

pub(crate) fn account_preflight(
    selected_steam_id: Option<String>,
    active_steam_id: Option<&str>,
) -> AccountCheck {
    let (status, message) = match (selected_steam_id.as_deref(), active_steam_id) {
        (None, _) => (AccountCheckStatus::NotRequired, None),
        (Some(selected), Some(active)) if selected == active => (AccountCheckStatus::Match, None),
        (Some(_), Some(_)) => (
            AccountCheckStatus::Mismatch,
            Some("Steam is signed into a different account. Change accounts in Steam and retry."),
        ),
        (Some(_), None) => (
            AccountCheckStatus::Unknown,
            Some(
                "Legio cannot verify which Steam account is active. Account-specific launch is unavailable.",
            ),
        ),
    };
    AccountCheck {
        account_requirement_met: matches!(
            status,
            AccountCheckStatus::NotRequired | AccountCheckStatus::Match
        ),
        selected_steam_id,
        status,
        message,
    }
}

pub(crate) fn database_error(error: rusqlite::Error) -> String {
    format!("local database error: {error}")
}

pub fn get_settings(state: &DatabaseState) -> Result<Settings, String> {
    state.database()?.settings()
}
pub fn save_settings(state: &DatabaseState, settings: Settings) -> Result<Settings, String> {
    state.database()?.save_settings(settings)
}
pub fn list_games(state: &DatabaseState) -> Result<Vec<Game>, String> {
    state.database()?.games()
}
pub fn create_game(state: &DatabaseState, input: CreateGameInput) -> Result<Game, String> {
    state.database()?.create_game(input)
}
pub fn update_game(state: &DatabaseState, input: UpdateGameInput) -> Result<Game, String> {
    state.database()?.update_game(input)
}
pub fn remove_game(state: &DatabaseState, id: &str) -> Result<(), String> {
    state.database()?.remove_game(id)
}
pub fn set_game_steam_account(
    state: &DatabaseState,
    game_id: &str,
    steam_id: Option<&str>,
) -> Result<Game, String> {
    state.database()?.set_game_steam_account(game_id, steam_id)
}
pub fn check_game_steam_account(
    state: &DatabaseState,
    game_id: &str,
) -> Result<AccountCheck, String> {
    state.database()?.check_game_steam_account(game_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_v1_without_replacing_user_data() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL); CREATE TABLE games (id TEXT PRIMARY KEY, steam_app_id INTEGER, automatic_name TEXT, name_override TEXT); INSERT INTO settings VALUES ('theme', 'light'); INSERT INTO games VALUES ('manual', 400, 'Portal', 'My Portal'); PRAGMA user_version = 1;").unwrap();
        migrate(&connection).unwrap();
        migrate(&connection).unwrap();
        let name: String = connection
            .query_row(
                "SELECT name_override FROM games WHERE id = 'manual'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "My Portal");
        let theme: String = connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'theme'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(theme, "light");
        connection
            .execute(
                "INSERT INTO catalog_games VALUES (400, 'Portal', 'portal', 100)",
                [],
            )
            .unwrap();
    }

    #[test]
    fn migrates_v2_preserving_duplicate_games_settings_and_catalog() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE games (id TEXT PRIMARY KEY NOT NULL, steam_app_id INTEGER, automatic_name TEXT, name_override TEXT);
             CREATE INDEX games_steam_app_id_idx ON games (steam_app_id);
             CREATE TABLE catalog_games (steam_app_id INTEGER PRIMARY KEY, name TEXT NOT NULL, search_name TEXT NOT NULL, fetched_at INTEGER NOT NULL);
             CREATE TABLE catalog_cache (provider TEXT PRIMARY KEY, query TEXT NOT NULL, fetched_at INTEGER NOT NULL, remote_count INTEGER NOT NULL);
             INSERT INTO settings VALUES ('theme', 'dark');
             INSERT INTO games VALUES ('first', 400, 'Portal', 'First copy'), ('second', 400, 'Portal', 'Second copy');
             INSERT INTO catalog_games VALUES (400, 'Portal', 'portal', 100);
             INSERT INTO catalog_cache VALUES ('hydra', 'portal', 100, 1);
             PRAGMA user_version = 2;"
        ).unwrap();
        migrate(&connection).unwrap();
        migrate(&connection).unwrap();
        let catalog_name: String = connection
            .query_row(
                "SELECT name FROM catalog_games WHERE steam_app_id = 400",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(catalog_name, "Portal");
        let cached_query: String = connection
            .query_row(
                "SELECT query FROM catalog_cache WHERE provider = 'hydra'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cached_query, "portal");
        let database = Database {
            connection: Mutex::new(connection),
        };
        assert_eq!(database.settings().unwrap().theme, Theme::Dark);
        assert_eq!(
            database.games().unwrap(),
            vec![
                Game {
                    id: "first".to_owned(),
                    steam_app_id: Some(400),
                    automatic_name: Some("Portal".to_owned()),
                    name_override: Some("First copy".to_owned()),
                    name: "First copy".to_owned(),
                    steam_install_path: None,
                    steam_account_id: None,
                    executable_path: None
                },
                Game {
                    id: "second".to_owned(),
                    steam_app_id: Some(400),
                    automatic_name: Some("Portal".to_owned()),
                    name_override: Some("Second copy".to_owned()),
                    name: "Second copy".to_owned(),
                    steam_install_path: None,
                    steam_account_id: None,
                    executable_path: None
                },
            ]
        );
    }

    fn temporary_directory() -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("legio-database-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn game_lookup_rejects_invalid_and_missing_ids() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        assert_eq!(database.game("invalid").unwrap_err(), "game id is invalid");
        assert_eq!(
            database
                .game("9717c6f1-3d4b-49b5-903b-c07d898c333c")
                .unwrap_err(),
            "game was not found"
        );
        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn persists_settings_and_enriched_manual_games_after_reopen() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        assert_eq!(database.settings().unwrap(), Settings::default());
        database
            .save_settings(Settings {
                theme: Theme::Light,
            })
            .unwrap();
        let game = database
            .create_game(CreateGameInput {
                name: "My manual title".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        let enriched = database
            .update_game(UpdateGameInput {
                id: game.id.clone(),
                steam_app_id: Some(480),
                automatic_name: Some("Spacewar".to_owned()),
                name_override: game.name_override.clone(),
            })
            .unwrap();
        assert_eq!(enriched.name, "My manual title");

        drop(database);
        let reopened = Database::open(&directory).unwrap();
        assert_eq!(
            reopened.settings().unwrap(),
            Settings {
                theme: Theme::Light
            }
        );
        assert_eq!(reopened.games().unwrap(), vec![enriched]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_updates_without_an_effective_name() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        let game = database
            .create_game(CreateGameInput {
                name: "Manual".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        let error = database
            .update_game(UpdateGameInput {
                id: game.id,
                steam_app_id: None,
                automatic_name: None,
                name_override: None,
            })
            .unwrap_err();
        assert_eq!(error, "a game needs an automatic name or a name override");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn account_selection_persists_and_app_id_change_clears_it() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        let game = database
            .create_game(CreateGameInput {
                name: "Test game".to_owned(),
                steam_app_id: Some(400),
            })
            .unwrap();
        assert_eq!(
            database.check_game_steam_account(&game.id).unwrap().status,
            AccountCheckStatus::NotRequired
        );
        let selected = database
            .set_game_steam_account(&game.id, Some("76561198000000001"))
            .unwrap();
        assert_eq!(
            selected.steam_account_id.as_deref(),
            Some("76561198000000001")
        );
        let check = database.check_game_steam_account(&game.id).unwrap();
        assert_eq!(check.status, AccountCheckStatus::Unknown);
        assert!(!check.account_requirement_met);
        assert_eq!(
            check.selected_steam_id.as_deref(),
            Some("76561198000000001")
        );
        assert_eq!(
            database
                .update_game(UpdateGameInput {
                    id: game.id.clone(),
                    steam_app_id: Some(400),
                    automatic_name: None,
                    name_override: Some("Renamed".to_owned()),
                })
                .unwrap()
                .steam_account_id,
            selected.steam_account_id
        );
        drop(database);
        let reopened = Database::open(&directory).unwrap();
        assert_eq!(
            reopened.games().unwrap()[0].steam_account_id.as_deref(),
            Some("76561198000000001")
        );
        assert_eq!(
            reopened
                .update_game(UpdateGameInput {
                    id: game.id,
                    steam_app_id: Some(401),
                    automatic_name: None,
                    name_override: Some("Renamed".to_owned()),
                })
                .unwrap()
                .steam_account_id,
            None
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn account_selection_rejects_invalid_ids_and_games_without_steam_app_id() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        let game = database
            .create_game(CreateGameInput {
                name: "Test game".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        for id in ["0", "01", "18446744073709551616", "x"] {
            assert_eq!(
                database
                    .set_game_steam_account(&game.id, Some(id))
                    .unwrap_err(),
                "Steam ID is invalid"
            );
        }
        assert_eq!(
            database
                .set_game_steam_account(&game.id, Some("76561198000000001"))
                .unwrap_err(),
            "game was not found or has no Steam App ID"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn account_preflight_requires_verified_match_for_selected_account() {
        let selected = Some("76561198000000001".to_owned());
        let matched = account_preflight(selected.clone(), Some("76561198000000001"));
        assert_eq!(matched.status, AccountCheckStatus::Match);
        assert!(matched.account_requirement_met);
        let mismatched = account_preflight(selected.clone(), Some("76561198000000002"));
        assert_eq!(mismatched.status, AccountCheckStatus::Mismatch);
        assert!(!mismatched.account_requirement_met);
        assert!(mismatched.message.unwrap().contains("Change accounts"));
        let unknown = account_preflight(selected, None);
        assert_eq!(unknown.status, AccountCheckStatus::Unknown);
        assert!(!unknown.account_requirement_met);
        let no_requirement = account_preflight(None, None);
        assert_eq!(no_requirement.status, AccountCheckStatus::NotRequired);
        assert!(no_requirement.account_requirement_met);
    }

    #[test]
    fn preserves_an_unsupported_newer_schema_for_recovery() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        database
            .save_settings(Settings {
                theme: Theme::Light,
            })
            .unwrap();
        database
            .with_connection(|connection| {
                connection
                    .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
                    .map_err(database_error)
            })
            .unwrap();
        drop(database);

        assert!(Database::open(&directory).is_err());

        let connection = Connection::open(directory.join("legio.sqlite3")).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION + 1);
        let theme: String = connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'theme'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(theme, "light");
        fs::remove_dir_all(directory).unwrap();
    }
}
