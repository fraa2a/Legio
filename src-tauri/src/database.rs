use std::{fs, path::Path, sync::Mutex};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const THEME_KEY: &str = "theme";
const SCHEMA_VERSION: i64 = 1;

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

    fn database(&self) -> Result<&Database, String> {
        self.database.as_ref().map_err(Clone::clone)
    }
}

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    fn open(data_dir: &Path) -> Result<Self, String> {
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
                    "SELECT id, steam_app_id, automatic_name, name_override
                     FROM games ORDER BY COALESCE(name_override, automatic_name), id",
                )
                .map_err(database_error)?;
            let rows = statement
                .query_map([], game_from_row)
                .map_err(database_error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
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
        let name = effective_name(&automatic_name, &name_override)?;

        self.with_connection(|connection| {
            let changed = connection
                .execute(
                    "UPDATE games
                     SET steam_app_id = ?2, automatic_name = ?3, name_override = ?4
                     WHERE id = ?1",
                    params![id, input.steam_app_id, automatic_name, name_override],
                )
                .map_err(database_error)?;
            if changed == 0 {
                return Err("game was not found".to_owned());
            }

            Ok(Game {
                id,
                steam_app_id: input.steam_app_id,
                automatic_name,
                name_override,
                name,
            })
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

    fn with_connection<T>(
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
    transaction.commit().map_err(database_error)
}

fn game_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Game> {
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
    })
}

fn required_name(value: String, field: &str) -> Result<String, String> {
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

fn database_error(error: rusqlite::Error) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory() -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("legio-database-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        directory
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
                    .execute_batch("PRAGMA user_version = 2")
                    .map_err(database_error)
            })
            .unwrap();
        drop(database);

        let error = match Database::open(&directory) {
            Ok(_) => panic!("an unsupported schema must not open"),
            Err(error) => error,
        };
        assert!(error.contains("newer than this Legio build supports"));

        let connection = Connection::open(directory.join("legio.sqlite3")).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
        fs::remove_dir_all(directory).unwrap();
    }
}
