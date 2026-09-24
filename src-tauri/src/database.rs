Warning: truncated output (original token count: 17867)
Total output lines: 1843

use std::{collections::BTreeMap, fs, path::Path, sync::Mutex};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const THEME_KEY: &str = "theme";
const DOWNLOAD_BANDWIDTH_LIMIT_KEY: &str = "download_bandwidth_limit_bytes_per_second";
const SCHEMA_VERSION: i64 = 12;

/// Persisted defaults for Linux compatibility launches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityDefaults {
    pub runner_path: Option<String>,
    /// Root directory where generated per-game prefixes are stored.
    pub prefix_root: Option<String>,
    pub arguments_before: Vec<String>,
    pub arguments_after: Vec<String>,
    pub working_directory: Option<String>,
    pub environment: BTreeMap<String, String>,
    pub dll_overrides: BTreeMap<String, String>,
}

/// Per-game compatibility values. `None` inherits the global default; an empty
/// string or argument list clears an inherited scalar or list. Nonempty
/// environment and DLL maps replace matching keys and retain other default
/// keys; an empty map clears the full inherited map.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GameCompatibilityOverrides {
    pub runner_path: Option<String>,
    /// Exact prefix directory for this game, overriding the global prefix root.
    pub prefix_path: Option<String>,
    pub arguments_before: Option<Vec<String>>,
    pub arguments_after: Option<Vec<String>>,
    pub working_directory: Option<String>,
    pub environment: Option<BTreeMap<String, String>>,
    pub dll_overrides: Option<BTreeMap<String, String>>,
}

/// Compatibility values after applying a game's overrides to global defaults.
/// Prefixes use the global root unless `prefix_path` names a game-specific path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveCompatibilityConfig {
    pub runner_path: Option<String>,
    pub prefix_root: Option<String>,
    pub prefix_path: Option<String>,
    pub arguments_before: Vec<String>,
    pub arguments_after: Vec<String>,
    pub working_directory: Option<String>,
    pub environment: BTreeMap<String, String>,
    pub dll_overrides: BTreeMap<String, String>,
}

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
#[serde(rename_all = "camelCase")]
pub struct PlaytimeSummary {
    pub game_id: String,
    pub total_milliseconds: i64,
    pub active_sessions: u32,
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
        let database = Self {
            connection: Mutex::new(connection),
        };
        database.recover_open_game_sessions(now_milliseconds())?;
        Ok(database)
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

    /// Loads the persistent download bandwidth limit. Zero means unlimited.
    pub fn download_bandwidth_limit(&self) -> Result<u64, String> {
        self.with_connection(|connection| {
            let value: Option<String> = connection
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [DOWNLOAD_BANDWIDTH_LIMIT_KEY],
                    |row| row.get(0),
                )
                .optional()
                .map_err(database_error)?;
            let Some(value) = value else {
                return Ok(0);
            };
            let parsed = value
                .parse::<u64>()
                .map_err(|_| "stored download bandwidth limit is invalid".to_owned())?;
            if parsed > i64::MAX as u64 {
                return Err("stored download bandwidth limit is too large".to_owned());
            }
            Ok(parsed)
        })
    }

    /// Stores the global download bandwidth limit. Zero means unlimited.
    pub fn save_download_bandwidth_limit(&self, bytes_per_second: u64) -> Result<(), String> {
        if bytes_per_second > i64::MAX as u64 {
            return Err("Bandwidth limit is too large".to_owned());
        }
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![DOWNLOAD_BANDWIDTH_LIMIT_KEY, bytes_per_second.to_string()],
                )
                .map_err(database_error)?;
            Ok(())
        })
    }

    /// Loads the global compatibility defaults, or their empty defaults on a new database.
    pub fn compatibility_defaults(&self) -> Result<CompatibilityDefaults, String> {
        self.with_connection(load_compatibility_defaults)
    }

    /// Resolves global defaults and per-game overrides in one database read.
    pub fn effective_compatibility_config(
        &self,
        game_id: &str,
    ) -> Result<EffectiveCompatibilityConfig, String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| {
            let defaults = load_compatibility_defaults(connection)?;
            let overrides = load_game_compatibility_overrides(connection, &game_id)?;
            Ok(merge_compatibility_config(defaults, overrides))
        })
    }

    /// Saves the global compatibility defaults.
    pub fn save_compatibility_defaults(
        &self,
        defaults: CompatibilityDefaults,
    ) -> Result<CompatibilityDefaults, String> {
        let arguments_before = encode_json(&defaults.arguments_before)?;
        let arguments_after = encode_json(&defaults.arguments_after)?;
        let environment = encode_json(&defaults.environment)?;
        let dll_overrides = encode_json(&defaults.dll_overrides)?;
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO compatibility_defaults
                        (id, runner_path, prefix_root, arguments_before,
                         arguments_after, working_directory, environment, dll_overrides)
                     VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(id) DO UPDATE SET
                        runner_path = excluded.runner_path,
                        prefix_root = excluded.prefix_root,
                        arguments_before = excluded.arguments_before,
                        arguments_after = excluded.arguments_after,
                        working_directory = excluded.working_directory,
                        environment = excluded.environment,
                        dll_overrides = excluded.dll_overrides",
                    params![
                        defaults.runner_path,
                        defaults.prefix_root,
                        arguments_before,
                        arguments_after,
                        defaults.working_directory,
                        environment,
                        dll_overrides
                    ],
                )
                .map_err(database_error)?;
            Ok(defaults)
        })
    }

    /// Loads a game's compatibility overrides. Missing fields inherit global defaults.
    pub fn game_compatibility_overrides(
        &self,
        game_id: &str,
    ) -> Result<GameCompatibilityOverrides, String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| load_game_compatibility_overrides(connection, &game_id))
    }

    /// Replaces a game's compatibility overrides. `None` fields inherit defaults;
    /// empty collections are retained and explicitly clear inherited collections.
    pub fn save_game_compatibility_overrides(
        &self,
        game_id: &str,
        overrides: GameCompatibilityOverrides,
    ) -> Result<GameCompatibilityOverrides, String> {
        let game_id = parse_game_id(game_id)?;
        let arguments_before = encode_optional_json(&overrides.arguments_before)?;
        let arguments_after = encode_optional_json(&overrides.arguments_after)?;
        let environment = encode_optional_json(&overrides.environment)?;
        let dll_overrides = encode_optional_json(&overrides.dll_overrides)?;
        self.with_connection(|connection| {
            let changed = connection
                .execute(
                    "INSERT INTO game_compatibility_overrides
                        (game_id, runner_path, prefix_path, arguments_before,
                         arguments_after, working_directory, environment, dll_overrides)
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8
                     WHERE EXISTS (SELECT 1 FROM games WHERE id = ?1)
                     ON CONFLICT(game_id) DO UPDATE SET
                        runner_path = excluded.runner_path,
                        prefix_path = excluded.prefix_path,
                        arguments_before = excluded.arguments_before,
                        arguments_after = excluded.arguments_after,
                        working_directory = excluded.working_directory,
                        environment = excluded.environment,
                        dll_overrides = excluded.dll_overrides",
                    params![
                        game_id,
                        overrides.runner_path,
                        overrides.prefix_path,
                        arguments_before,
                        arguments_after,
                        overrides.working_directory,
                        environment,
                        dll_overrides
                    ],
                )
                .map_err(database_error)?;
            if changed == 0 {
                return Err("game was not found".to_owned());
            }
            Ok(overrides)
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

    pub(crate) fn start_game_session(&self, game_id: &str, now: i64) -> Result<(), String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO game_sessions (game_id, started_at, last_seen_at)
                     VALUES (?1, ?2, ?2)",
                    params![game_id, now],
                )
                .map_err(database_error)?;
            Ok(())
        })
    }

    pub(crate) fn heartbeat_game_session(&self, game_id: &str, now: i64) -> Result<(), String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE game_sessions SET last_seen_at = ?2
                     WHERE game_id = ?1 AND ended_at IS NULL",
                    params![game_id, now],
                )
                .map_err(database_error)?;
            Ok(())
        })
    }

    pub(crate) fn end_game_session(&self, game_id: &str, now: i64) -> Result<(), String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE game_sessions SET ended_at = ?2, last_seen_at = ?2,
                         end_reason = 'finished'
                     WHERE game_id = ?1 AND ended_at IS NULL",
                    params![game_id, now],
                )
                .map_err(database_error)?;
            Ok(())
        })
    }

    pub(crate) fn playtime_summaries(&self, now: i64) -> Result<Vec<PlaytimeSummary>, String> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT g.id,
                         COALESCE(SUM(CASE WHEN s.ended_at IS NULL
                             THEN MAX(0, ?1 - s.started_at)
                             ELSE MAX(0, s.ended_at - s.started_at) END), 0),
                         SUM(CASE WHEN s.ended_at IS NULL THEN 1 ELSE 0 END)
                     FROM games g LEFT JOIN game_sessions s ON s.game_id = g.id
                     GROUP BY g.id ORDER BY g.id",
                )
                .map_err(database_error)?;
            let rows = statement
                .query_map([now], |row| {
                    Ok(PlaytimeSummary {
                        game_id: row.get(0)?,
                        total_milliseconds: row.get(1)?,
                        active_sessions: row.get::<_, Option<u32>>(2)?.unwrap_or(0),
                    })
                })
                .map_err(database_error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
        })
    }

    fn recover_open_game_sessions(&self, now: i64) -> Result<(), String> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE game_sessions SET ended_at = last_seen_at,
                         end_reason = 'interrupted'
                     WHERE ended_at IS NULL AND last_seen_at <= ?1",
                    [now],
                )
                .map_err(database_error)?;
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
   …5867 tokens truncated…= connection
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
    fn download_bandwidth_limit_defaults_to_unlimited_and_survives_reopen() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        assert_eq!(database.download_bandwidth_limit().unwrap(), 0);

        database.save_download_bandwidth_limit(1_500_000).unwrap();
        assert_eq!(database.download_bandwidth_limit().unwrap(), 1_500_000);
        assert!(
            database
                .save_download_bandwidth_limit(i64::MAX as u64 + 1)
                .is_err()
        );

        drop(database);
        let reopened = Database::open(&directory).unwrap();
        assert_eq!(reopened.download_bandwidth_limit().unwrap(), 1_500_000);
        reopened.save_download_bandwidth_limit(0).unwrap();
        assert_eq!(reopened.download_bandwidth_limit().unwrap(), 0);
        drop(reopened);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn playtime_summaries_include_active_and_finished_sessions_per_game() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        let first = database
            .create_game(CreateGameInput {
                name: "First game".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        let second = database
            .create_game(CreateGameInput {
                name: "Second game".to_owned(),
                steam_app_id: None,
            })
            .unwrap();

        database.start_game_session(&first.id, 1_000).unwrap();
        database.heartbeat_game_session(&first.id, 2_000).unwrap();
        database.end_game_session(&first.id, 3_000).unwrap();
        database.start_game_session(&first.id, 4_000).unwrap();
        database.start_game_session(&second.id, 4_500).unwrap();

        let summaries = database.playtime_summaries(10_000).unwrap();
        let first_summary = summaries
            .iter()
            .find(|item| item.game_id == first.id)
            .unwrap();
        let second_summary = summaries
            .iter()
            .find(|item| item.game_id == second.id)
            .unwrap();
        assert_eq!(first_summary.total_milliseconds, 8_000);
        assert_eq!(first_summary.active_sessions, 1);
        assert_eq!(second_summary.total_milliseconds, 5_500);
        assert_eq!(second_summary.active_sessions, 1);

        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reopening_recovers_open_sessions_at_the_last_heartbeat() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        let game = database
            .create_game(CreateGameInput {
                name: "Interrupted game".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        database.start_game_session(&game.id, 1_000).unwrap();
        database.heartbeat_game_session(&game.id, 2_500).unwrap();
        drop(database);

        let reopened = Database::open(&directory).unwrap();
        let summaries = reopened.playtime_summaries(10_000).unwrap();
        assert_eq!(summaries[0].total_milliseconds, 1_500);
        assert_eq!(summaries[0].active_sessions, 0);
        let reason: String = reopened
            .with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT end_reason FROM game_sessions WHERE game_id = ?1",
                        [&game.id],
                        |row| row.get(0),
                    )
                    .map_err(database_error)
            })
            .unwrap();
        assert_eq!(reason, "interrupted");
        drop(reopened);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compatibility_settings_persist_and_empty_game_values_clear_inherited_collections() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        let game = database
            .create_game(CreateGameInput {
                name: "Compatibility test".to_owned(),
                steam_app_id: None,
            })
            .unwrap();

        let mut environment = BTreeMap::new();
        environment.insert("WINEDEBUG".to_owned(), "-all".to_owned());
        let defaults = CompatibilityDefaults {
            runner_path: Some("/usr/bin/wine".to_owned()),
            prefix_root: Some("/prefixes".to_owned()),
            arguments_before: vec!["-windowed".to_owned()],
            arguments_after: Vec::new(),
            working_directory: Some("/games/default".to_owned()),
            environment,
            dll_overrides: BTreeMap::new(),
        };
        assert_eq!(
            database
                .save_compatibility_defaults(defaults.clone())
                .unwrap(),
            defaults
        );

        let overrides = GameCompatibilityOverrides {
            arguments_before: Some(Vec::new()),
            environment: Some(BTreeMap::new()),
            ..GameCompatibilityOverrides::default()
        };
        assert_eq!(
            database
                .save_game_compatibility_overrides(&game.id, overrides.clone())
                .unwrap(),
            overrides
        );
        drop(database);

        let reopened = Database::open(&directory).unwrap();
        assert_eq!(reopened.compatibility_defaults().unwrap(), defaults);
        let stored = reopened.game_compatibility_overrides(&game.id).unwrap();
        assert_eq!(stored, overrides);
        assert_eq!(stored.runner_path, None);
        assert_eq!(stored.arguments_before, Some(Vec::new()));
        assert_eq!(stored.environment, Some(BTreeMap::new()));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compatibility_override_save_rejects_unknown_game() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        assert_eq!(
            database
                .save_game_compatibility_overrides(
                    "00000000-0000-0000-0000-000000000001",
                    GameCompatibilityOverrides::default(),
                )
                .unwrap_err(),
            "game was not found"
        );
        assert_eq!(
            database
                .game_compatibility_overrides("00000000-0000-0000-0000-000000000001")
                .unwrap_err(),
            "game was not found"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn effective_compatibility_config_merges_defaults_and_game_values() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        let game = database
            .create_game(CreateGameInput {
                name: "Effective config".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        let mut default_environment = BTreeMap::new();
        default_environment.insert("A".to_owned(), "default-a".to_owned());
        default_environment.insert("B".to_owned(), "default-b".to_owned());
        let mut default_dlls = BTreeMap::new();
        default_dlls.insert("d3d11".to_owned(), "native".to_owned());
        database
            .save_compatibility_defaults(CompatibilityDefaults {
                runner_path: Some("/usr/bin/wine".to_owned()),
                prefix_root: Some("/prefixes".to_owned()),
                arguments_before: vec!["default-before".to_owned()],
                arguments_after: vec!["default-after".to_owned()],
                working_directory: Some("/games/default".to_owned()),
                environment: default_environment,
                dll_overrides: default_dlls,
            })
            .unwrap();
        let mut environment = BTreeMap::new();
        environment.insert("B".to_owned(), "game-b".to_owned());
        environment.insert("C".to_owned(), "game-c".to_owned());
        let mut dlls = BTreeMap::new();
        dlls.insert("d3d11".to_owned(), "builtin".to_owned());
        dlls.insert("dxgi".to_owned(), "native".to_owned());
        database
            .save_game_compatibility_overrides(
                &game.id,
                GameCompatibilityOverrides {
                    runner_path: Some("/opt/wine-custom".to_owned()),
                    prefix_path: Some("/prefixes/custom".to_owned()),
                    arguments_before: Some(vec!["game-before".to_owned()]),
                    working_directory: Some(String::new()),
                    environment: Some(environment),
                    dll_overrides: Some(dlls),
                    ..GameCompatibilityOverrides::default()
                },
            )
            .unwrap();

        let effective = database.effective_compatibility_config(&game.id).unwrap();
        assert_eq!(effective.runner_path.as_deref(), Some("/opt/wine-custom"));
        assert_eq!(effective.prefix_root.as_deref(), Some("/prefixes"));
        assert_eq!(effective.prefix_path.as_deref(), Some("/prefixes/custom"));
        assert_eq!(effective.arguments_before, vec!["game-before"]);
        assert_eq!(effective.arguments_after, vec!["default-after"]);
        assert_eq!(effective.working_directory, None);
        assert_eq!(
            effective.environment.get("A").map(String::as_str),
            Some("default-a")
        );
        assert_eq!(
            effective.environment.get("B").map(String::as_str),
            Some("game-b")
        );
        assert_eq!(
            effective.environment.get("C").map(String::as_str),
            Some("game-c")
        );
        assert_eq!(
            effective.dll_overrides.get("d3d11").map(String::as_str),
            Some("builtin")
        );
        assert_eq!(
            effective.dll_overrides.get("dxgi").map(String::as_str),
            Some("native")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn effective_compatibility_config_empty_values_clear_defaults() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        let game = database
            .create_game(CreateGameInput {
                name: "Clear config".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        let mut default_environment = BTreeMap::new();
        default_environment.insert("A".to_owned(), "default".to_owned());
        let mut default_dlls = BTreeMap::new();
        default_dlls.insert("d3d11".to_owned(), "native".to_owned());
        database
            .save_compatibility_defaults(CompatibilityDefaults {
                runner_path: Some("/usr/bin/wine".to_owned()),
                prefix_root: Some("/prefixes".to_owned()),
                arguments_before: vec!["default".to_owned()],
                arguments_after: vec!["default".to_owned()],
                working_directory: Some("/games/default".to_owned()),
                environment: default_environment,
                dll_overrides: default_dlls,
            })
            .unwrap();
        database
            .save_game_compatibility_overrides(
                &game.id,
                GameCompatibilityOverrides {
                    runner_path: Some(String::new()),
                    prefix_path: Some(String::new()),
                    arguments_before: Some(Vec::new()),
                    arguments_after: Some(Vec::new()),
                    working_directory: Some(String::new()),
                    environment: Some(BTreeMap::new()),
                    dll_overrides: Some(BTreeMap::new()),
                },
            )
            .unwrap();

        assert_eq!(
            database.effective_compatibility_config(&game.id).unwrap(),
            EffectiveCompatibilityConfig {
                runner_path: None,
                prefix_root: Some("/prefixes".to_owned()),
                prefix_path: None,
                arguments_before: Vec::new(),
                arguments_after: Vec::new(),
                working_directory: None,
                environment: BTreeMap::new(),
                dll_overrides: BTreeMap::new(),
            }
        );
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
