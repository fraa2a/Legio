use std::{collections::BTreeMap, fs, path::Path, sync::Mutex};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const THEME_KEY: &str = "theme";
const STEAM_LIBRARY_POLL_MINUTES_KEY: &str = "steam_library_poll_minutes";
const DOWNLOAD_BANDWIDTH_LIMIT_KEY: &str = "download_bandwidth_limit_bytes_per_second";
const DEFAULT_STEAM_LIBRARY_POLL_MINUTES: u32 = 30;
const MIN_STEAM_LIBRARY_POLL_MINUTES: u32 = 5;
const MAX_STEAM_LIBRARY_POLL_MINUTES: u32 = 120;
const SCHEMA_VERSION: i64 = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SteamLaunchConfig {
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct NativeLaunchConfig {
    pub arguments: Vec<String>,
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GraphicsRenderer {
    #[default]
    RunnerDefault,
    WineD3d,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WaylandMode {
    #[default]
    RunnerDefault,
    Disabled,
    Native,
}

/// Persisted defaults for Linux compatibility launches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CompatibilityDefaults {
    pub runner_path: Option<String>,
    /// Root directory where generated per-game prefixes are stored.
    pub prefix_root: Option<String>,
    pub arguments_before: Vec<String>,
    pub arguments_after: Vec<String>,
    pub working_directory: Option<String>,
    pub environment: BTreeMap<String, String>,
    pub dll_overrides: BTreeMap<String, String>,
    pub graphics_renderer: GraphicsRenderer,
    pub wayland: WaylandMode,
    pub debug_logging: bool,
}

/// Per-game compatibility values. `None` inherits the global default; an empty
/// string or argument list clears an inherited scalar or list. Nonempty
/// environment and DLL maps replace matching keys and retain other default
/// keys; an empty map clears the full inherited map.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GameCompatibilityOverrides {
    pub launch_via_steam: Option<bool>,
    pub runner_path: Option<String>,
    /// Exact prefix directory for this game, overriding the global prefix root.
    pub prefix_path: Option<String>,
    pub arguments_before: Option<Vec<String>>,
    pub arguments_after: Option<Vec<String>>,
    pub working_directory: Option<String>,
    pub environment: Option<BTreeMap<String, String>>,
    pub dll_overrides: Option<BTreeMap<String, String>>,
    pub graphics_renderer: Option<GraphicsRenderer>,
    pub wayland: Option<WaylandMode>,
    pub debug_logging: Option<bool>,
    pub online_fix: Option<bool>,
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
    pub graphics_renderer: GraphicsRenderer,
    pub wayland: WaylandMode,
    pub debug_logging: bool,
    pub online_fix: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedCompatibilityOptions {
    pub runner: String,
    pub version: String,
    pub launch_via_steam: bool,
    pub graphics_renderer: GraphicsRenderer,
    pub wayland: WaylandMode,
    pub debug_logging: bool,
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
    #[serde(default = "default_steam_library_poll_minutes")]
    pub steam_library_poll_minutes: u32,
    #[serde(default)]
    pub download_path: Option<String>,
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
    #[serde(default = "default_true")]
    pub hide_on_game_start: bool,
    #[serde(default)]
    pub launch_on_system_start: bool,
    #[serde(default)]
    pub launch_minimized: bool,
    #[serde(default)]
    pub launch_in_library: bool,
    #[serde(default = "default_true")]
    pub download_notifications: bool,
    #[serde(default = "default_true")]
    pub verify_verified_downloads: bool,
}

fn default_true() -> bool {
    true
}

fn default_steam_library_poll_minutes() -> u32 {
    DEFAULT_STEAM_LIBRARY_POLL_MINUTES
}

fn validate_steam_library_poll_minutes(minutes: u32) -> Result<(), String> {
    if (MIN_STEAM_LIBRARY_POLL_MINUTES..=MAX_STEAM_LIBRARY_POLL_MINUTES).contains(&minutes) {
        Ok(())
    } else {
        Err(format!(
            "Steam library poll interval must be between {MIN_STEAM_LIBRARY_POLL_MINUTES} and {MAX_STEAM_LIBRARY_POLL_MINUTES} minutes"
        ))
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            steam_library_poll_minutes: DEFAULT_STEAM_LIBRARY_POLL_MINUTES,
            download_path: None,
            close_to_tray: true,
            hide_on_game_start: true,
            launch_on_system_start: false,
            launch_minimized: false,
            launch_in_library: false,
            download_notifications: true,
            verify_verified_downloads: true,
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
    pub last_played_at: Option<i64>,
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
    database: Result<std::sync::Arc<Database>, String>,
}

impl DatabaseState {
    pub fn new(data_dir: Result<std::path::PathBuf, tauri::Error>) -> Self {
        let database = data_dir
            .map_err(|error| format!("could not resolve the application data directory: {error}"))
            .and_then(|data_dir| Database::open(&data_dir).map(std::sync::Arc::new));

        Self { database }
    }

    pub(crate) fn database(&self) -> Result<&Database, String> {
        self.database.as_deref().map_err(Clone::clone)
    }

    pub(crate) fn shared_database(&self) -> Result<std::sync::Arc<Database>, String> {
        self.database.as_ref().cloned().map_err(Clone::clone)
    }
}

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub(crate) fn storage_root(&self, data_dir: &Path) -> Result<std::path::PathBuf, String> {
        Ok(self
            .settings()?
            .download_path
            .map_or_else(|| data_dir.to_path_buf(), std::path::PathBuf::from))
    }

    pub fn steam_launch_config(&self, game_id: &str) -> Result<SteamLaunchConfig, String> {
        let game = self.game(game_id)?;
        if game.steam_install_path.is_none() {
            return Err("Steam launch settings require a Steam-managed game".to_owned());
        }
        self.with_connection(|connection| {
            let arguments: Option<String> = connection
                .query_row(
                    "SELECT arguments FROM game_steam_launch_config WHERE game_id = ?1",
                    [game.id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(database_error)?;
            arguments.map_or_else(
                || Ok(SteamLaunchConfig::default()),
                |arguments| {
                    serde_json::from_str(&arguments)
                        .map(|arguments| SteamLaunchConfig { arguments })
                        .map_err(|error| {
                            format!("stored Steam launch arguments are invalid: {error}")
                        })
                },
            )
        })
    }

    pub fn save_steam_launch_config(
        &self,
        game_id: &str,
        config: SteamLaunchConfig,
    ) -> Result<SteamLaunchConfig, String> {
        let game = self.game(game_id)?;
        if game.steam_install_path.is_none() {
            return Err("Steam launch settings require a Steam-managed game".to_owned());
        }
        if config
            .arguments
            .iter()
            .any(|argument| argument.contains('\0'))
        {
            return Err("Steam launch arguments cannot contain null characters".to_owned());
        }
        let arguments = encode_json(&config.arguments)?;
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO game_steam_launch_config (game_id, arguments) VALUES (?1, ?2)
                     ON CONFLICT(game_id) DO UPDATE SET arguments = excluded.arguments",
                    params![game.id, arguments],
                )
                .map_err(database_error)?;
            Ok(config)
        })
    }

    pub fn native_launch_config(&self, game_id: &str) -> Result<NativeLaunchConfig, String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| {
            let exists: bool = connection
                .query_row("SELECT EXISTS(SELECT 1 FROM games WHERE id = ?1)", [&game_id], |row| row.get(0))
                .map_err(database_error)?;
            if !exists {
                return Err("game was not found".to_owned());
            }
            let config: Option<(String, Option<String>)> = connection
                .query_row(
                    "SELECT arguments, working_directory FROM game_native_launch_config WHERE game_id = ?1",
                    [&game_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(database_error)?;
            config.map_or_else(
                || Ok(NativeLaunchConfig::default()),
                |(arguments, working_directory)| {
                    let arguments = serde_json::from_str(&arguments)
                        .map_err(|error| format!("stored native launch arguments are invalid: {error}"))?;
                    Ok(NativeLaunchConfig { arguments, working_directory })
                },
            )
        })
    }

    pub fn save_native_launch_config(
        &self,
        game_id: &str,
        config: NativeLaunchConfig,
    ) -> Result<NativeLaunchConfig, String> {
        let game_id = parse_game_id(game_id)?;
        if config
            .arguments
            .iter()
            .any(|argument| argument.contains('\0'))
            || config
                .working_directory
                .as_deref()
                .is_some_and(|path| path.contains('\0'))
        {
            return Err("Native launch settings cannot contain null characters".to_owned());
        }
        let arguments = encode_json(&config.arguments)?;
        self.with_connection(|connection| {
            let changed = connection.execute(
                "INSERT INTO game_native_launch_config (game_id, arguments, working_directory)
                 SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM games WHERE id = ?1)
                 ON CONFLICT(game_id) DO UPDATE SET arguments = excluded.arguments, working_directory = excluded.working_directory",
                params![game_id, arguments, config.working_directory],
            ).map_err(database_error)?;
            if changed == 0 { return Err("game was not found".to_owned()); }
            Ok(config)
        })
    }

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
            let stored_preferences: Option<String> = connection
                .query_row(
                    "SELECT value FROM settings WHERE key = 'app_preferences'",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .map_err(database_error)?;
            let stored_theme: Option<String> = connection
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [THEME_KEY],
                    |row| row.get(0),
                )
                .optional()
                .map_err(database_error)?;
            let stored_poll_minutes: Option<String> = connection
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [STEAM_LIBRARY_POLL_MINUTES_KEY],
                    |row| row.get(0),
                )
                .optional()
                .map_err(database_error)?;
            let theme = stored_theme.map_or(Ok(Theme::System), |value| Theme::parse(&value))?;
            let steam_library_poll_minutes =
                stored_poll_minutes.map_or(Ok(DEFAULT_STEAM_LIBRARY_POLL_MINUTES), |value| {
                    value.parse::<u32>().map_err(|error| {
                        format!("stored Steam library poll interval is invalid: {error}")
                    })
                })?;
            validate_steam_library_poll_minutes(steam_library_poll_minutes)?;
            let mut settings = stored_preferences.map_or_else(
                || Ok(Settings::default()),
                |value| {
                    serde_json::from_str::<Settings>(&value).map_err(|error| {
                        format!("stored application preferences are invalid: {error}")
                    })
                },
            )?;
            settings.theme = theme;
            settings.steam_library_poll_minutes = steam_library_poll_minutes;
            Ok(settings)
        })
    }

    pub fn save_settings(&self, settings: Settings) -> Result<Settings, String> {
        validate_steam_library_poll_minutes(settings.steam_library_poll_minutes)?;
        if settings.launch_minimized && !settings.launch_on_system_start {
            return Err("Launch minimized requires launch on system startup".to_owned());
        }
        if let Some(path) = &settings.download_path {
            let path = Path::new(path);
            if !path.is_absolute()
                || path.to_str().is_none()
                || path.to_string_lossy().chars().any(char::is_control)
            {
                return Err(
                    "Download path must be an absolute directory without control characters"
                        .to_owned(),
                );
            }
            fs::create_dir_all(path)
                .map_err(|error| format!("Could not create download directory: {error}"))?;
            let metadata = fs::symlink_metadata(path)
                .map_err(|error| format!("Could not inspect download directory: {error}"))?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("Download path is not a directory".to_owned());
            }
        }
        let preferences = encode_json(&settings)?;
        self.with_connection(|connection| {
            let transaction = connection.unchecked_transaction().map_err(database_error)?;
            let current: Option<String> = transaction.query_row(
                "SELECT value FROM settings WHERE key = 'app_preferences'", [], |row| row.get(0)
            ).optional().map_err(database_error)?;
            let old_path = current.map(|value| serde_json::from_str::<Settings>(&value)
                .map(|stored| stored.download_path)
                .map_err(|error| format!("stored application preferences are invalid: {error}")))
                .transpose()?.flatten();
            if old_path != settings.download_path {
                let active: bool = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM downloads WHERE status NOT IN ('installed', 'cancelled') OR (status = 'installed' AND staged_path IS NOT NULL))",
                    [], |row| row.get(0)
                ).map_err(database_error)?;
                if active { return Err("Finish or remove active downloads before changing the download directory".to_owned()); }
            }
            transaction
                .execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![THEME_KEY, settings.theme.as_str()],
                )
                .map_err(database_error)?;
            transaction
                .execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![
                        STEAM_LIBRARY_POLL_MINUTES_KEY,
                        settings.steam_library_poll_minutes.to_string()
                    ],
                )
                .map_err(database_error)?;
            transaction.execute(
                "INSERT INTO settings (key, value) VALUES ('app_preferences', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [preferences],
            ).map_err(database_error)?;
            transaction.commit().map_err(database_error)?;
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
        let graphics_renderer = encode_json(&defaults.graphics_renderer)?;
        let wayland = encode_json(&defaults.wayland)?;
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO compatibility_defaults
                        (id, runner_path, prefix_root, arguments_before,
                         arguments_after, working_directory, environment, dll_overrides,
                         graphics_renderer, wayland, debug_logging)
                     VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT(id) DO UPDATE SET
                        runner_path = excluded.runner_path,
                        prefix_root = excluded.prefix_root,
                        arguments_before = excluded.arguments_before,
                        arguments_after = excluded.arguments_after,
                        working_directory = excluded.working_directory,
                        environment = excluded.environment,
                        dll_overrides = excluded.dll_overrides,
                        graphics_renderer = excluded.graphics_renderer,
                        wayland = excluded.wayland,
                        debug_logging = excluded.debug_logging",
                    params![
                        defaults.runner_path,
                        defaults.prefix_root,
                        arguments_before,
                        arguments_after,
                        defaults.working_directory,
                        environment,
                        dll_overrides,
                        graphics_renderer,
                        wayland,
                        defaults.debug_logging
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
        #[cfg(not(target_os = "linux"))]
        if overrides.launch_via_steam == Some(true) {
            return Err("Launch via Steam is available on Linux only".to_owned());
        }
        let game_id = parse_game_id(game_id)?;
        if overrides.launch_via_steam == Some(true) {
            let game = self.game(&game_id)?;
            if game.steam_install_path.is_some() || game.steam_app_id.is_none() {
                return Err(
                    "Launch via Steam requires a manual game with a Steam App ID".to_owned(),
                );
            }
        }
        let arguments_before = encode_optional_json(&overrides.arguments_before)?;
        let arguments_after = encode_optional_json(&overrides.arguments_after)?;
        let environment = encode_optional_json(&overrides.environment)?;
        let dll_overrides = encode_optional_json(&overrides.dll_overrides)?;
        let graphics_renderer = encode_optional_json(&overrides.graphics_renderer)?;
        let wayland = encode_optional_json(&overrides.wayland)?;
        self.with_connection(|connection| {
            let changed = connection
                .execute(
                    "INSERT INTO game_compatibility_overrides
                        (game_id, runner_path, prefix_path, arguments_before,
                         arguments_after, working_directory, environment, dll_overrides,
                         graphics_renderer, wayland, debug_logging, launch_via_steam, online_fix)
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
                     WHERE EXISTS (SELECT 1 FROM games WHERE id = ?1)
                     ON CONFLICT(game_id) DO UPDATE SET
                        runner_path = excluded.runner_path,
                        prefix_path = excluded.prefix_path,
                        arguments_before = excluded.arguments_before,
                        arguments_after = excluded.arguments_after,
                        working_directory = excluded.working_directory,
                        environment = excluded.environment,
                        dll_overrides = excluded.dll_overrides,
                        graphics_renderer = excluded.graphics_renderer,
                        wayland = excluded.wayland,
                        debug_logging = excluded.debug_logging,
                        launch_via_steam = excluded.launch_via_steam,
                        online_fix = excluded.online_fix",
                    params![
                        game_id,
                        overrides.runner_path,
                        overrides.prefix_path,
                        arguments_before,
                        arguments_after,
                        overrides.working_directory,
                        environment,
                        dll_overrides,
                        graphics_renderer,
                        wayland,
                        overrides.debug_logging,
                        overrides.launch_via_steam,
                        overrides.online_fix
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
            ensure_unique_game_name(connection, game.steam_app_id, &game.name, None)?;
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
            ensure_unique_game_name(connection, input.steam_app_id, &name, Some(&id))?;
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
                         SUM(CASE WHEN s.id IS NOT NULL AND s.ended_at IS NULL THEN 1 ELSE 0 END),
                         MAX(s.started_at)
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
                        last_played_at: row.get(3)?,
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
        transaction.execute_batch(
            "CREATE TABLE downloads (
                id TEXT PRIMARY KEY NOT NULL,
                steam_app_id INTEGER NOT NULL CHECK (steam_app_id BETWEEN 1 AND 4294967295),
                name TEXT NOT NULL,
                release_version TEXT NOT NULL,
                url TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                etag TEXT,
                size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
                downloaded_bytes INTEGER NOT NULL DEFAULT 0 CHECK (downloaded_bytes >= 0 AND downloaded_bytes <= size_bytes),
                speed_bps INTEGER NOT NULL DEFAULT 0,
                eta_seconds INTEGER,
                status TEXT NOT NULL CHECK (status IN ('queued', 'downloading', 'paused', 'waiting', 'failed', 'downloaded', 'cancelled')),
                error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
             );
             CREATE INDEX downloads_status_idx ON downloads (status, created_at);
             PRAGMA user_version = 7;"
        ).map_err(database_error)?;
    }
    if version < 8 {
        transaction.execute_batch(
            "ALTER TABLE downloads ADD COLUMN staged_path TEXT;
             CREATE TABLE downloads_v8 (
                id TEXT PRIMARY KEY NOT NULL,
                steam_app_id INTEGER NOT NULL CHECK (steam_app_id BETWEEN 1 AND 4294967295),
                name TEXT NOT NULL,
                release_version TEXT NOT NULL,
                url TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                etag TEXT,
                size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
                downloaded_bytes INTEGER NOT NULL DEFAULT 0 CHECK (downloaded_bytes >= 0 AND downloaded_bytes <= size_bytes),
                speed_bps INTEGER NOT NULL DEFAULT 0,
                eta_seconds INTEGER,
                status TEXT NOT NULL CHECK (status IN ('queued', 'downloading', 'paused', 'waiting', 'failed', 'downloaded', 'staging', 'staged', 'cancelled')),
                error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                staged_path TEXT
             );
             INSERT INTO downloads_v8 SELECT * FROM downloads;
             DROP TABLE downloads;
             ALTER TABLE downloads_v8 RENAME TO downloads;
             CREATE INDEX downloads_status_idx ON downloads (status, created_at);
             PRAGMA user_version = 8;"
        ).map_err(database_error)?;
    }
    if version < 9 {
        transaction
            .execute_batch(
                "ALTER TABLE games ADD COLUMN executable_path TEXT;
                 CREATE UNIQUE INDEX games_executable_path_idx ON games (executable_path) WHERE executable_path IS NOT NULL;
                 PRAGMA user_version = 9;",
            )
            .map_err(database_error)?;
    }
    if version < 10 {
        transaction.execute_batch(
            "ALTER TABLE downloads ADD COLUMN final_path TEXT;
             ALTER TABLE downloads ADD COLUMN executable_relative TEXT;
             ALTER TABLE downloads ADD COLUMN install_token TEXT;
             CREATE TABLE downloads_v10 (
                id TEXT PRIMARY KEY NOT NULL,
                steam_app_id INTEGER NOT NULL CHECK (steam_app_id BETWEEN 1 AND 4294967295),
                name TEXT NOT NULL,
                release_version TEXT NOT NULL,
                url TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                etag TEXT,
                size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
                downloaded_bytes INTEGER NOT NULL DEFAULT 0 CHECK (downloaded_bytes >= 0 AND downloaded_bytes <= size_bytes),
                speed_bps INTEGER NOT NULL DEFAULT 0,
                eta_seconds INTEGER,
                status TEXT NOT NULL CHECK (status IN ('queued', 'downloading', 'paused', 'waiting', 'failed', 'downloaded', 'staging', 'staged', 'finalizing', 'installed', 'cancelled')),
                error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                staged_path TEXT,
                final_path TEXT,
                executable_relative TEXT,
                install_token TEXT
             );
             INSERT INTO downloads_v10 SELECT * FROM downloads;
             DROP TABLE downloads;
             ALTER TABLE downloads_v10 RENAME TO downloads;
             CREATE INDEX downloads_status_idx ON downloads (status, created_at);
             PRAGMA user_version = 10;"
        ).map_err(database_error)?;
    }
    if version < 11 {
        transaction
            .execute_batch(
                "CREATE TABLE compatibility_defaults (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    runner_path TEXT,
                    prefix_root TEXT,
                    arguments_before TEXT NOT NULL,
                    arguments_after TEXT NOT NULL,
                    working_directory TEXT,
                    environment TEXT NOT NULL,
                    dll_overrides TEXT NOT NULL
                 );
                 INSERT INTO compatibility_defaults
                    (id, arguments_before, arguments_after, environment, dll_overrides)
                 VALUES (1, '[]', '[]', '{}', '{}');
                 CREATE TABLE game_compatibility_overrides (
                    game_id TEXT PRIMARY KEY NOT NULL REFERENCES games(id) ON DELETE CASCADE,
                    runner_path TEXT,
                    prefix_path TEXT,
                    arguments_before TEXT,
                    arguments_after TEXT,
                    working_directory TEXT,
                    environment TEXT,
                    dll_overrides TEXT
                 );
                 PRAGMA user_version = 11;",
            )
            .map_err(database_error)?;
    }
    if version < 12 {
        transaction
            .execute_batch(
                "CREATE TABLE game_sessions (
                    id INTEGER PRIMARY KEY,
                    game_id TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
                    started_at INTEGER NOT NULL,
                    last_seen_at INTEGER NOT NULL,
                    ended_at INTEGER,
                    end_reason TEXT CHECK (end_reason IN ('finished', 'interrupted')),
                    CHECK (last_seen_at >= started_at),
                    CHECK (ended_at IS NULL OR ended_at >= started_at)
                 );
                 CREATE INDEX game_sessions_game_id_started_at_idx
                    ON game_sessions (game_id, started_at);
                 PRAGMA user_version = 12;",
            )
            .map_err(database_error)?;
    }
    if version < 13 {
        transaction
            .execute_batch(
                "CREATE TABLE game_native_launch_config (
                game_id TEXT PRIMARY KEY NOT NULL REFERENCES games(id) ON DELETE CASCADE,
                arguments TEXT NOT NULL DEFAULT '[]',
                working_directory TEXT
             );
             PRAGMA user_version = 13;",
            )
            .map_err(database_error)?;
    }
    if version < 14 {
        transaction
            .execute_batch(
                r#"ALTER TABLE compatibility_defaults ADD COLUMN steam_runtime TEXT NOT NULL DEFAULT '"runner_default"';
                   ALTER TABLE compatibility_defaults ADD COLUMN steam_overlay TEXT NOT NULL DEFAULT '"runner_default"';
                   ALTER TABLE compatibility_defaults ADD COLUMN graphics_renderer TEXT NOT NULL DEFAULT '"runner_default"';
                   ALTER TABLE compatibility_defaults ADD COLUMN wayland TEXT NOT NULL DEFAULT '"runner_default"';
                   ALTER TABLE game_compatibility_overrides ADD COLUMN steam_runtime TEXT;
                   ALTER TABLE game_compatibility_overrides ADD COLUMN steam_overlay TEXT;
                   ALTER TABLE game_compatibility_overrides ADD COLUMN graphics_renderer TEXT;
                   ALTER TABLE game_compatibility_overrides ADD COLUMN wayland TEXT;
                   PRAGMA user_version = 14;"#,
            )
            .map_err(database_error)?;
    }
    if version < 15 {
        transaction
            .execute_batch(
                "ALTER TABLE compatibility_defaults
                    ADD COLUMN debug_logging INTEGER NOT NULL DEFAULT 0
                    CHECK (debug_logging IN (0, 1));
                 ALTER TABLE game_compatibility_overrides
                    ADD COLUMN debug_logging INTEGER
                    CHECK (debug_logging IS NULL OR debug_logging IN (0, 1));
                 PRAGMA user_version = 15;",
            )
            .map_err(database_error)?;
    }
    if version < 16 {
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS game_steam_launch_config (
                    game_id TEXT PRIMARY KEY NOT NULL REFERENCES games(id) ON DELETE CASCADE,
                    arguments TEXT NOT NULL DEFAULT '[]'
                 );
                 CREATE TABLE steam_details_cache_v16 (
                    steam_app_id INTEGER PRIMARY KEY CHECK (steam_app_id BETWEEN 1 AND 4294967295),
                    details TEXT NOT NULL CHECK (length(CAST(details AS BLOB)) BETWEEN 1 AND 524288),
                    fetched_at INTEGER NOT NULL CHECK (fetched_at >= 0)
                 );
                 INSERT INTO steam_details_cache_v16 SELECT * FROM steam_details_cache;
                 DROP TABLE steam_details_cache;
                 ALTER TABLE steam_details_cache_v16 RENAME TO steam_details_cache;
                 PRAGMA user_version = 16;",
            )
            .map_err(database_error)?;
    }
    if version < 17 {
        let existing: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('game_compatibility_overrides') WHERE name = 'launch_via_steam'",
                [],
                |row| row.get(0),
            )
            .map_err(database_error)?;
        if existing == 0 {
            transaction.execute_batch(
                "ALTER TABLE game_compatibility_overrides ADD COLUMN launch_via_steam INTEGER CHECK (launch_via_steam IS NULL OR launch_via_steam IN (0, 1));"
            ).map_err(database_error)?;
        }
        transaction
            .execute_batch("PRAGMA user_version = 17;")
            .map_err(database_error)?;
    }
    if version < 18 {
        transaction
            .execute_batch(
                "ALTER TABLE compatibility_defaults DROP COLUMN steam_runtime;
                 ALTER TABLE compatibility_defaults DROP COLUMN steam_overlay;
                 ALTER TABLE game_compatibility_overrides DROP COLUMN steam_runtime;
                 ALTER TABLE game_compatibility_overrides DROP COLUMN steam_overlay;
                 PRAGMA user_version = 18;",
            )
            .map_err(database_error)?;
    }
    if version < 19 {
        let existing: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('game_compatibility_overrides') WHERE name = 'online_fix'",
                [],
                |row| row.get(0),
            )
            .map_err(database_error)?;
        if existing == 0 {
            transaction
                .execute_batch(
                    "ALTER TABLE game_compatibility_overrides
                        ADD COLUMN online_fix INTEGER
                        CHECK (online_fix IS NULL OR online_fix IN (0, 1));",
                )
                .map_err(database_error)?;
        }
        transaction
            .execute_batch("PRAGMA user_version = 19;")
            .map_err(database_error)?;
    }
    if version < 20 {
        let existing: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('downloads') WHERE name = 'source_verified'",
            [], |row| row.get(0),
        ).map_err(database_error)?;
        if existing == 0 {
            transaction.execute_batch(
                "ALTER TABLE downloads ADD COLUMN source_verified INTEGER NOT NULL DEFAULT 1 CHECK (source_verified IN (0, 1));"
            ).map_err(database_error)?;
        }
        let bytes: Option<Vec<u8>> = transaction
            .query_row(
                "SELECT manifest FROM legio_source_cache WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(database_error)?;
        if let Some(bytes) = bytes {
            match crate::legio_source::parse_manifest(&bytes) {
                Ok(manifest) => {
                    for entry in manifest.unverified {
                        transaction.execute(
                            "UPDATE downloads SET source_verified = 0 WHERE steam_app_id = ?1 AND url = ?2 AND release_version = ?3",
                            params![entry.steam_app_id, entry.download.url, entry.release.version],
                        ).map_err(database_error)?;
                    }
                }
                Err(error) => eprintln!("Could not classify existing download sources: {error}"),
            }
        }
        transaction
            .execute_batch("PRAGMA user_version = 20;")
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)
}

pub(crate) fn now_milliseconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn load_compatibility_defaults(connection: &Connection) -> Result<CompatibilityDefaults, String> {
    let (
        runner_path,
        prefix_root,
        arguments_before,
        arguments_after,
        working_directory,
        environment,
        dll_overrides,
        graphics_renderer,
        wayland,
        debug_logging,
    ) = connection
        .query_row(
            "SELECT runner_path, prefix_root, arguments_before,
                    arguments_after, working_directory, environment, dll_overrides,
                    graphics_renderer, wayland, debug_logging
             FROM compatibility_defaults WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, bool>(9)?,
                ))
            },
        )
        .map_err(database_error)?;
    Ok(CompatibilityDefaults {
        runner_path,
        prefix_root,
        arguments_before: decode_json(&arguments_before)?,
        arguments_after: decode_json(&arguments_after)?,
        working_directory,
        environment: decode_json(&environment)?,
        dll_overrides: decode_json(&dll_overrides)?,
        graphics_renderer: decode_json(&graphics_renderer)?,
        wayland: decode_json(&wayland)?,
        debug_logging,
    })
}

fn load_game_compatibility_overrides(
    connection: &Connection,
    game_id: &str,
) -> Result<GameCompatibilityOverrides, String> {
    let stored = connection
        .query_row(
            "SELECT o.runner_path, o.prefix_path, o.arguments_before,
                    o.arguments_after, o.working_directory, o.environment, o.dll_overrides,
                    o.graphics_renderer, o.wayland,
                    o.debug_logging, o.launch_via_steam, o.online_fix
             FROM games AS g
             LEFT JOIN game_compatibility_overrides AS o ON o.game_id = g.id
             WHERE g.id = ?1",
            [game_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<bool>>(9)?,
                    row.get::<_, Option<bool>>(10)?,
                    row.get::<_, Option<bool>>(11)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    let Some((
        runner_path,
        prefix_path,
        arguments_before,
        arguments_after,
        working_directory,
        environment,
        dll_overrides,
        graphics_renderer,
        wayland,
        debug_logging,
        launch_via_steam,
        online_fix,
    )) = stored
    else {
        return Err("game was not found".to_owned());
    };
    Ok(GameCompatibilityOverrides {
        runner_path,
        prefix_path,
        arguments_before: decode_optional_json(arguments_before)?,
        arguments_after: decode_optional_json(arguments_after)?,
        working_directory,
        environment: decode_optional_json(environment)?,
        dll_overrides: decode_optional_json(dll_overrides)?,
        graphics_renderer: decode_optional_json(graphics_renderer)?,
        wayland: decode_optional_json(wayland)?,
        debug_logging,
        launch_via_steam,
        online_fix,
    })
}

fn merge_compatibility_config(
    defaults: CompatibilityDefaults,
    overrides: GameCompatibilityOverrides,
) -> EffectiveCompatibilityConfig {
    let mut environment = defaults.environment;
    if let Some(override_values) = overrides.environment {
        if override_values.is_empty() {
            environment.clear();
        } else {
            environment.extend(override_values);
        }
    }

    let mut dll_overrides = defaults.dll_overrides;
    if let Some(override_values) = overrides.dll_overrides {
        if override_values.is_empty() {
            dll_overrides.clear();
        } else {
            dll_overrides.extend(override_values);
        }
    }

    EffectiveCompatibilityConfig {
        runner_path: overrides
            .runner_path
            .or(defaults.runner_path)
            .filter(|value| !value.is_empty()),
        prefix_root: defaults.prefix_root.filter(|value| !value.is_empty()),
        prefix_path: overrides.prefix_path.filter(|value| !value.is_empty()),
        arguments_before: overrides
            .arguments_before
            .unwrap_or(defaults.arguments_before),
        arguments_after: overrides
            .arguments_after
            .unwrap_or(defaults.arguments_after),
        working_directory: overrides
            .working_directory
            .filter(|value| !value.is_empty()),
        environment,
        dll_overrides,
        graphics_renderer: overrides
            .graphics_renderer
            .unwrap_or(defaults.graphics_renderer),
        wayland: overrides.wayland.unwrap_or(defaults.wayland),
        debug_logging: overrides.debug_logging.unwrap_or(defaults.debug_logging),
        online_fix: overrides.online_fix.unwrap_or(false),
    }
}

fn encode_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value)
        .map_err(|error| format!("could not encode compatibility setting: {error}"))
}

fn decode_json<T: for<'de> Deserialize<'de>>(value: &str) -> Result<T, String> {
    serde_json::from_str(value)
        .map_err(|error| format!("stored compatibility setting is invalid: {error}"))
}

fn encode_optional_json<T: Serialize>(value: &Option<T>) -> Result<Option<String>, String> {
    value.as_ref().map(encode_json).transpose()
}

fn decode_optional_json<T: for<'de> Deserialize<'de>>(
    value: Option<String>,
) -> Result<Option<T>, String> {
    value.map(|value| decode_json(&value)).transpose()
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

fn ensure_unique_game_name(
    connection: &Connection,
    steam_app_id: Option<u32>,
    name: &str,
    excluded_id: Option<&str>,
) -> Result<(), String> {
    let Some(steam_app_id) = steam_app_id else {
        return Ok(());
    };
    let duplicate = connection
        .query_row(
            "SELECT 1 FROM games
             WHERE steam_app_id = ?1 AND COALESCE(name_override, automatic_name) = ?2 COLLATE NOCASE
               AND (?3 IS NULL OR id != ?3)
             LIMIT 1",
            params![steam_app_id, name, excluded_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(database_error)?;
    if duplicate.is_some() {
        return Err(
            "Il nome è già usato per questo gioco Steam. Scegli un nome diverso.".to_owned(),
        );
    }
    Ok(())
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
    fn migrates_v9_staged_download_for_finalization() {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        connection.execute_batch("INSERT INTO downloads (id, steam_app_id, name, release_version, url, sha256, size_bytes, status, created_at, updated_at, staged_path) VALUES ('job', 42, 'Game', '1', 'https://example.test', 'hash', 4, 'staged', 1, 1, '/stage'); ALTER TABLE downloads DROP COLUMN source_verified; ALTER TABLE downloads DROP COLUMN install_token; ALTER TABLE downloads DROP COLUMN executable_relative; ALTER TABLE downloads DROP COLUMN final_path; DROP TABLE game_native_launch_config; DROP TABLE game_sessions; DROP TABLE game_compatibility_overrides; DROP TABLE compatibility_defaults; PRAGMA user_version = 9;").unwrap();
        migrate(&connection).unwrap();
        let row: (String, String, Option<String>) = connection
            .query_row(
                "SELECT status, staged_path, final_path FROM downloads WHERE id = 'job'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, ("staged".to_owned(), "/stage".to_owned(), None));
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn migration_11_adds_compatibility_storage_without_changing_existing_games() {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        connection
            .execute_batch(
                "DROP TABLE game_compatibility_overrides;
                 DROP TABLE game_native_launch_config; DROP TABLE game_sessions;
                 DROP TABLE compatibility_defaults;
                 INSERT INTO games (id, name_override) VALUES ('00000000-0000-0000-0000-000000000001', 'Existing');
                 PRAGMA user_version = 10;",
            )
            .unwrap();
        migrate(&connection).unwrap();
        let game_name: String = connection
            .query_row(
                "SELECT name_override FROM games WHERE id = '00000000-0000-0000-0000-000000000001'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(game_name, "Existing");
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn migration_adds_typed_options_and_removes_steam_fragments() {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO games (id, name_override) VALUES (?1, 'Existing')",
                ["00000000-0000-0000-0000-000000000001"],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE compatibility_defaults SET arguments_before = '[\"--keep\"]'
                 WHERE id = 1",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO game_compatibility_overrides (game_id, arguments_before)
                 VALUES (?1, '[\"--game\"]')",
                ["00000000-0000-0000-0000-000000000001"],
            )
            .unwrap();
        connection
            .execute_batch(
                "ALTER TABLE compatibility_defaults DROP COLUMN graphics_renderer;
                 ALTER TABLE compatibility_defaults DROP COLUMN wayland;
                 ALTER TABLE compatibility_defaults DROP COLUMN debug_logging;
                 ALTER TABLE game_compatibility_overrides DROP COLUMN graphics_renderer;
                 ALTER TABLE game_compatibility_overrides DROP COLUMN wayland;
                 ALTER TABLE game_compatibility_overrides DROP COLUMN debug_logging;
                 PRAGMA user_version = 13;",
            )
            .unwrap();

        migrate(&connection).unwrap();
        let defaults: (String, String, String) = connection
            .query_row(
                "SELECT arguments_before, graphics_renderer, wayland
                 FROM compatibility_defaults WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let overrides: (String, Option<String>) = connection
            .query_row(
                "SELECT arguments_before, graphics_renderer FROM game_compatibility_overrides
                 WHERE game_id = ?1",
                ["00000000-0000-0000-0000-000000000001"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            defaults,
            (
                "[\"--keep\"]".to_owned(),
                "\"runner_default\"".to_owned(),
                "\"runner_default\"".to_owned(),
            )
        );
        assert_eq!(overrides, ("[\"--game\"]".to_owned(), None));
        let steam_columns: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('compatibility_defaults')
                 WHERE name IN ('steam_runtime', 'steam_overlay')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(steam_columns, 0);
        let debug_defaults: bool = connection
            .query_row(
                "SELECT debug_logging FROM compatibility_defaults WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let debug_override: Option<bool> = connection
            .query_row(
                "SELECT debug_logging FROM game_compatibility_overrides WHERE game_id = ?1",
                ["00000000-0000-0000-0000-000000000001"],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!debug_defaults);
        assert_eq!(debug_override, None);
    }

    #[test]
    fn typed_compatibility_enums_reject_unknown_values() {
        assert!(serde_json::from_str::<GraphicsRenderer>("\"vulkan\"").is_err());
        assert!(serde_json::from_str::<WaylandMode>("\"enabled\"").is_err());
    }

    #[test]
    fn migrates_v8_staged_download_database_without_losing_games() {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO games (id, name_override) VALUES ('manual', 'Manual game');
             ALTER TABLE downloads DROP COLUMN source_verified; ALTER TABLE downloads DROP COLUMN install_token;
             ALTER TABLE downloads DROP COLUMN executable_relative;
             ALTER TABLE downloads DROP COLUMN final_path;
             DROP TABLE game_compatibility_overrides;
             DROP TABLE game_native_launch_config; DROP TABLE game_sessions;
             DROP TABLE compatibility_defaults;
             DROP INDEX games_executable_path_idx;
             ALTER TABLE games DROP COLUMN executable_path;
             PRAGMA user_version = 8;",
            )
            .unwrap();
        migrate(&connection).unwrap();
        let name: String = connection
            .query_row(
                "SELECT name_override FROM games WHERE id = 'manual'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let downloads_exists: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'downloads'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((name.as_str(), downloads_exists), ("Manual game", 1));
    }

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
    fn rejects_duplicate_name_for_the_same_steam_app_id() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        let first = database
            .create_game(CreateGameInput {
                name: "Portal".to_owned(),
                steam_app_id: Some(400),
            })
            .unwrap();
        let error = database
            .create_game(CreateGameInput {
                name: "portal".to_owned(),
                steam_app_id: Some(400),
            })
            .unwrap_err();
        assert!(error.contains("nome è già usato"));

        let second = database
            .create_game(CreateGameInput {
                name: "Portal".to_owned(),
                steam_app_id: Some(401),
            })
            .unwrap();
        let error = database
            .update_game(UpdateGameInput {
                id: second.id,
                steam_app_id: Some(400),
                automatic_name: None,
                name_override: Some("Portal".to_owned()),
            })
            .unwrap_err();
        assert!(error.contains("nome è già usato"));
        assert_eq!(database.game(&first.id).unwrap().name, "Portal");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn shared_app_id_keeps_names_settings_and_playtime_separate() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        let first = database
            .create_game(CreateGameInput {
                name: "Original".to_owned(),
                steam_app_id: Some(400),
            })
            .unwrap();
        let second = database
            .create_game(CreateGameInput {
                name: "Moddato".to_owned(),
                steam_app_id: Some(400),
            })
            .unwrap();
        database
            .save_game_compatibility_overrides(
                &first.id,
                GameCompatibilityOverrides {
                    prefix_path: Some("/games/original-prefix".to_owned()),
                    ..Default::default()
                },
            )
            .unwrap();
        database
            .save_game_compatibility_overrides(
                &second.id,
                GameCompatibilityOverrides {
                    prefix_path: Some("/games/modded-prefix".to_owned()),
                    ..Default::default()
                },
            )
            .unwrap();
        database.start_game_session(&second.id, 1_000).unwrap();
        database.end_game_session(&second.id, 3_000).unwrap();

        assert_eq!(database.games().unwrap().len(), 2);
        assert_eq!(database.game(&first.id).unwrap().name, "Original");
        assert_eq!(database.game(&second.id).unwrap().name, "Moddato");
        assert_eq!(
            database
                .game_compatibility_overrides(&first.id)
                .unwrap()
                .prefix_path
                .as_deref(),
            Some("/games/original-prefix")
        );
        assert_eq!(
            database
                .game_compatibility_overrides(&second.id)
                .unwrap()
                .prefix_path
                .as_deref(),
            Some("/games/modded-prefix")
        );
        let summaries = database.playtime_summaries(4_000).unwrap();
        assert_eq!(
            summaries
                .iter()
                .find(|summary| summary.game_id == first.id)
                .unwrap()
                .last_played_at,
            None
        );
        assert_eq!(
            summaries
                .iter()
                .find(|summary| summary.game_id == second.id)
                .unwrap()
                .last_played_at,
            Some(1_000)
        );

        drop(database);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn native_launch_config_persists_and_rejects_null_arguments() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        assert_eq!(
            database
                .native_launch_config(&Uuid::new_v4().to_string())
                .unwrap_err(),
            "game was not found"
        );
        let game = database
            .create_game(CreateGameInput {
                name: "Native test".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        assert_eq!(
            database.native_launch_config(&game.id).unwrap(),
            NativeLaunchConfig::default()
        );
        let config = NativeLaunchConfig {
            arguments: vec!["--profile".to_owned(), "Player One".to_owned()],
            working_directory: Some("C:\\Games\\Test".to_owned()),
        };
        database
            .save_native_launch_config(&game.id, config.clone())
            .unwrap();
        assert!(
            database
                .save_native_launch_config(
                    &game.id,
                    NativeLaunchConfig {
                        arguments: vec!["bad\0argument".to_owned()],
                        working_directory: None,
                    }
                )
                .is_err()
        );
        drop(database);
        let database = Database::open(&directory).unwrap();
        assert_eq!(database.native_launch_config(&game.id).unwrap(), config);
        drop(database);
        fs::remove_dir_all(directory).unwrap();
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
    fn verified_hash_check_defaults_on_and_persists_when_disabled() {
        let mut preferences = serde_json::to_value(Settings::default()).unwrap();
        preferences
            .as_object_mut()
            .unwrap()
            .remove("verifyVerifiedDownloads");
        assert!(
            serde_json::from_value::<Settings>(preferences)
                .unwrap()
                .verify_verified_downloads
        );
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        assert!(database.settings().unwrap().verify_verified_downloads);
        database
            .save_settings(Settings {
                verify_verified_downloads: false,
                ..Settings::default()
            })
            .unwrap();
        drop(database);
        let database = Database::open(&directory).unwrap();
        assert!(!database.settings().unwrap().verify_verified_downloads);
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
                steam_library_poll_minutes: 15,
                ..Settings::default()
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
                theme: Theme::Light,
                steam_library_poll_minutes: 15,
                ..Settings::default()
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
        let untouched = database
            .create_game(CreateGameInput {
                name: "Untouched game".to_owned(),
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
        let untouched_summary = summaries
            .iter()
            .find(|item| item.game_id == untouched.id)
            .unwrap();
        assert_eq!(first_summary.total_milliseconds, 8_000);
        assert_eq!(first_summary.active_sessions, 1);
        assert_eq!(first_summary.last_played_at, Some(4_000));
        assert_eq!(second_summary.total_milliseconds, 5_500);
        assert_eq!(second_summary.active_sessions, 1);
        assert_eq!(second_summary.last_played_at, Some(4_500));
        assert_eq!(untouched_summary.total_milliseconds, 0);
        assert_eq!(untouched_summary.active_sessions, 0);
        assert_eq!(untouched_summary.last_played_at, None);

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
            runner_path: Some("/opt/Proton 10.0".to_owned()),
            prefix_root: Some("/prefixes".to_owned()),
            arguments_before: vec!["-windowed".to_owned()],
            arguments_after: Vec::new(),
            working_directory: Some("/games/default".to_owned()),
            environment,
            dll_overrides: BTreeMap::new(),
            graphics_renderer: GraphicsRenderer::WineD3d,
            wayland: WaylandMode::Native,
            debug_logging: true,
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
            graphics_renderer: Some(GraphicsRenderer::RunnerDefault),
            wayland: Some(WaylandMode::Disabled),
            debug_logging: Some(false),
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
        assert_eq!(
            stored.graphics_renderer,
            Some(GraphicsRenderer::RunnerDefault)
        );
        assert_eq!(stored.wayland, Some(WaylandMode::Disabled));
        assert_eq!(stored.debug_logging, Some(false));
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
                runner_path: Some("/opt/Proton 10.0".to_owned()),
                prefix_root: Some("/prefixes".to_owned()),
                arguments_before: vec!["default-before".to_owned()],
                arguments_after: vec!["default-after".to_owned()],
                working_directory: Some("/games/default".to_owned()),
                environment: default_environment,
                dll_overrides: default_dlls,
                graphics_renderer: GraphicsRenderer::WineD3d,
                wayland: WaylandMode::Native,
                debug_logging: true,
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
                    runner_path: Some("/opt/Proton custom".to_owned()),
                    prefix_path: Some("/prefixes/custom".to_owned()),
                    arguments_before: Some(vec!["game-before".to_owned()]),
                    working_directory: Some(String::new()),
                    environment: Some(environment),
                    dll_overrides: Some(dlls),
                    wayland: Some(WaylandMode::Disabled),
                    debug_logging: Some(false),
                    ..GameCompatibilityOverrides::default()
                },
            )
            .unwrap();

        let effective = database.effective_compatibility_config(&game.id).unwrap();
        assert_eq!(effective.runner_path.as_deref(), Some("/opt/Proton custom"));
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
        assert_eq!(effective.graphics_renderer, GraphicsRenderer::WineD3d);
        assert_eq!(effective.wayland, WaylandMode::Disabled);
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
                ..CompatibilityDefaults::default()
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
                    ..GameCompatibilityOverrides::default()
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
                graphics_renderer: GraphicsRenderer::RunnerDefault,
                wayland: WaylandMode::RunnerDefault,
                debug_logging: false,
                online_fix: false,
            }
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn typed_compatibility_options_inherit_override_and_reset() {
        let directory = temporary_directory();
        let database = Database::open(&directory).unwrap();
        let game = database
            .create_game(CreateGameInput {
                name: "Typed options".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        database
            .save_compatibility_defaults(CompatibilityDefaults {
                graphics_renderer: GraphicsRenderer::WineD3d,
                wayland: WaylandMode::Native,
                debug_logging: true,
                ..CompatibilityDefaults::default()
            })
            .unwrap();
        assert_eq!(
            database.effective_compatibility_config(&game.id).unwrap(),
            EffectiveCompatibilityConfig {
                graphics_renderer: GraphicsRenderer::WineD3d,
                wayland: WaylandMode::Native,
                debug_logging: true,
                ..EffectiveCompatibilityConfig::default()
            }
        );
        assert!(
            database
                .effective_compatibility_config(&game.id)
                .unwrap()
                .debug_logging
        );

        database
            .save_game_compatibility_overrides(
                &game.id,
                GameCompatibilityOverrides {
                    wayland: Some(WaylandMode::Disabled),
                    debug_logging: Some(false),
                    online_fix: Some(true),
                    ..GameCompatibilityOverrides::default()
                },
            )
            .unwrap();
        let overridden = database.effective_compatibility_config(&game.id).unwrap();
        assert_eq!(overridden.graphics_renderer, GraphicsRenderer::WineD3d);
        assert_eq!(overridden.wayland, WaylandMode::Disabled);
        assert!(!overridden.debug_logging);
        assert!(overridden.online_fix);
        assert_eq!(
            database
                .game_compatibility_overrides(&game.id)
                .unwrap()
                .online_fix,
            Some(true)
        );

        database
            .save_game_compatibility_overrides(&game.id, GameCompatibilityOverrides::default())
            .unwrap();
        let reset = database.effective_compatibility_config(&game.id).unwrap();
        assert_eq!(reset.graphics_renderer, GraphicsRenderer::WineD3d);
        assert_eq!(reset.wayland, WaylandMode::Native);
        assert!(reset.debug_logging);
        assert!(!reset.online_fix);
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
                steam_library_poll_minutes: 15,
                ..Settings::default()
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
