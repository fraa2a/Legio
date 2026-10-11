use std::{ffi::OsString, fs, io::Read, path::Path};

#[cfg(any(windows, test))]
use rusqlite::{OptionalExtension, params};
#[cfg(any(windows, test))]
use serde::{Deserialize, Serialize};
#[cfg(any(windows, test))]
use serde_json::Value;
#[cfg(any(windows, test))]
use sha2::{Digest, Sha256};
#[cfg(any(windows, test))]
use std::collections::HashSet;

#[cfg(any(windows, test))]
use crate::database::{Database, Settings, database_error};

#[cfg(any(windows, test))]
const SOURCE_URL: &str = "https://source.taxphobia.top/store.json";
const MAX_EXPORT_BYTES: u64 = 64 * 1024 * 1024;

#[cfg(any(windows, test))]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Export {
    schema_version: u32,
    migration_id: String,
    games: Vec<Value>,
    preferences: Value,
    source_manifest: Value,
}

#[cfg(any(windows, test))]
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Receipt {
    migration_id: String,
    imported_games: usize,
    matched_games: usize,
    source_url: String,
    fingerprint: String,
}

#[cfg(any(windows, test))]
struct ConvertedGame {
    id: String,
    name: String,
    steam_app_id: Option<u32>,
    executable: String,
    root: String,
    duration: i64,
    last_played: Option<i64>,
}

#[cfg(any(windows, test))]
fn windows_path(value: &str) -> Result<String, String> {
    let value = value.replace('/', "\\");
    let bytes = value.as_bytes();
    let drive =
        bytes.len() > 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    let unc =
        value.starts_with("\\\\") && value[2..].split('\\').filter(|v| !v.is_empty()).count() >= 3;
    if (!drive && !unc)
        || value.chars().any(char::is_control)
        || value.split('\\').any(|part| matches!(part, "." | ".."))
        || value.contains(['"', '<', '>', '|', '?', '*'])
    {
        return Err(
            "Legacy paths must be absolute Windows paths without parent traversal".to_owned(),
        );
    }
    let mut normalized = if unc { "\\".to_owned() } else { String::new() };
    for part in value.split('\\').filter(|part| !part.is_empty()) {
        if !normalized.is_empty() {
            normalized.push('\\');
        }
        normalized.push_str(part);
    }
    Ok(normalized)
}

#[cfg(any(windows, test))]
fn steam_identity(game: &Value) -> Result<Option<u32>, String> {
    for key in ["steamAppId", "steam_app_id", "appId"] {
        if let Some(value) = game.get(key).filter(|v| !v.is_null()) {
            let number = value
                .as_u64()
                .or_else(|| value.as_str().and_then(|s| s.parse().ok()));
            return number
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0)
                .map(Some)
                .ok_or_else(|| "Invalid legacy Steam application identity".to_owned());
        }
    }
    let Some(cover) = game.get("cover").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Ok(url) = reqwest::Url::parse(cover) else {
        return Ok(None);
    };
    if url.scheme() != "https"
        || !matches!(
            url.host_str(),
            Some(
                "cdn.akamai.steamstatic.com"
                    | "cdn.cloudflare.steamstatic.com"
                    | "shared.fastly.steamstatic.com"
                    | "cdn.steamstatic.com"
            )
        )
    {
        return Ok(None);
    }
    let segments: Vec<_> = url.path_segments().into_iter().flatten().collect();
    Ok(segments
        .windows(2)
        .find(|parts| parts[0] == "apps")
        .and_then(|parts| parts[1].parse::<u32>().ok())
        .filter(|n| *n > 0))
}

#[cfg(any(windows, test))]
fn convert_game(game: &Value) -> Result<ConvertedGame, String> {
    let id = game
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
        .ok_or("Invalid legacy game ID")?;
    let name = game
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 512 && !s.chars().any(char::is_control))
        .ok_or("Invalid legacy game name")?;
    let executable = windows_path(
        game.get("path")
            .and_then(Value::as_str)
            .ok_or("Legacy game has no executable path")?,
    )?;
    if !executable.to_lowercase().ends_with(".exe") {
        return Err("A legacy game path does not identify an executable. Select its .exe in TaxLauncher before migrating".to_owned());
    }
    let mut root = executable
        .rsplit_once('\\')
        .map(|(root, _)| root.to_owned())
        .ok_or("Invalid executable directory")?;
    if root.ends_with(':') {
        root.push('\\');
    }
    let minutes = game
        .get("playtime")
        .map_or(Some(0.0), Value::as_f64)
        .ok_or("Invalid legacy playtime")?;
    let milliseconds = minutes * 60_000.0;
    if !milliseconds.is_finite()
        || milliseconds < 0.0
        || milliseconds > 100.0 * 366.0 * 24.0 * 60.0 * 60_000.0
    {
        return Err("Invalid legacy playtime duration".to_owned());
    }
    let last_played = match game.get("lastPlayed").filter(|value| !value.is_null()) {
        None => None,
        Some(Value::String(value)) => Some(
            chrono::DateTime::parse_from_rfc3339(value)
                .map_err(|_| "Invalid legacy last-played date")?
                .timestamp_millis(),
        ),
        Some(value) => Some(
            value
                .as_i64()
                .filter(|n| *n >= 0)
                .ok_or("Invalid legacy last-played timestamp")?,
        ),
    };
    Ok(ConvertedGame {
        id: {
            let digest = Sha256::digest(format!("legio-taxlauncher-game:{id}").as_bytes());
            let mut bytes = [0_u8; 16];
            bytes.copy_from_slice(&digest[..16]);
            bytes[6] = (bytes[6] & 0x0f) | 0x80;
            bytes[8] = (bytes[8] & 0x3f) | 0x80;
            uuid::Uuid::from_bytes(bytes).to_string()
        },
        name: name.to_owned(),
        steam_app_id: steam_identity(game)?,
        executable,
        root,
        duration: milliseconds.round() as i64,
        last_played,
    })
}

#[cfg(any(windows, test))]
fn preference_bool(preferences: &Value, key: &str, fallback: bool) -> Result<bool, String> {
    match preferences.get(key) {
        None => Ok(fallback),
        Some(Value::Bool(value)) => Ok(*value),
        Some(Value::String(value)) if value == "true" || value == "false" => Ok(value == "true"),
        _ => Err(format!("Invalid legacy preference: {key}")),
    }
}

#[cfg(any(windows, test))]
fn convert_preferences(preferences: &Value) -> Result<Settings, String> {
    if !preferences.is_object() {
        return Err("Invalid legacy preferences".to_owned());
    }
    let mut settings = Settings {
        onboarding_completed: true,
        close_to_tray: preference_bool(preferences, "closeToTray", true)?,
        hide_on_game_start: preference_bool(preferences, "minimizeOnGameLaunch", true)?,
        launch_on_system_start: preference_bool(preferences, "autoStart", false)?,
        ..Settings::default()
    };
    settings.discord_presence.enabled = preference_bool(preferences, "discordRpcEnabled", true)?;
    if let Some(language) = preferences.get("appLanguage") {
        settings.language =
            serde_json::from_value(language.clone()).map_err(|_| "Invalid legacy language")?;
    }
    if let Some(path) = preferences
        .get("downloadPath")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        settings.download_path = Some(windows_path(path)?);
    }
    settings.theme = match preferences.get("selectedTheme").and_then(Value::as_str) {
        Some("dark") => crate::database::Theme::Dark,
        Some("light") => crate::database::Theme::Light,
        _ => crate::database::Theme::System,
    };
    Ok(settings)
}

#[cfg(any(windows, test))]
pub(crate) fn import(database: &Database, input: &Export) -> Result<Receipt, String> {
    if input.schema_version != 1
        || uuid::Uuid::parse_str(&input.migration_id).is_err()
        || input.games.len() > 20_000
    {
        return Err("Unsupported or invalid TaxLauncher export".to_owned());
    }
    let games = input
        .games
        .iter()
        .map(convert_game)
        .collect::<Result<Vec<_>, _>>()?;
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    for game in &games {
        if !ids.insert(&game.id) || !paths.insert(game.executable.to_lowercase()) {
            return Err("Duplicate legacy game ID or executable".to_owned());
        }
    }
    let settings = serde_json::to_string(&convert_preferences(&input.preferences)?)
        .map_err(|e| e.to_string())?;
    let manifest = serde_json::to_vec(&input.source_manifest).map_err(|e| e.to_string())?;
    crate::legio_source::parse_manifest(&manifest)
        .map_err(|error| format!("Invalid migration source: {error}"))?;
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(input).map_err(|e| e.to_string())?)
    );
    let marker = format!("taxlauncher_import:{}", input.migration_id);
    let now = crate::database::now_milliseconds()?;
    database.with_connection(|connection| {
        let transaction = connection.unchecked_transaction().map_err(database_error)?;
        let prior: Option<String> = transaction.query_row("SELECT value FROM settings WHERE key=?1", [&marker], |r| r.get(0)).optional().map_err(database_error)?;
        if let Some(prior) = prior {
            let receipt: Receipt = serde_json::from_str(&prior).map_err(|e| e.to_string())?;
            return if receipt.fingerprint == fingerprint { Ok(receipt) } else { Err("The export changed after this migration was imported".to_owned()) };
        }
        let existing: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM settings WHERE key IN ('app_preferences','theme','steam_library_poll_minutes')) OR EXISTS(SELECT 1 FROM games)", [], |r|r.get(0)).map_err(database_error)?;
        let mut receipt = Receipt { migration_id: input.migration_id.clone(), imported_games: 0, matched_games: 0, source_url: SOURCE_URL.to_owned(), fingerprint };
        for game in games {
            let matched: Option<String> = transaction.query_row("SELECT id FROM games WHERE LOWER(REPLACE(executable_path,'/','\\'))=LOWER(?1)", [&game.executable], |r|r.get(0)).optional().map_err(database_error)?;
            let id = if let Some(id) = matched {receipt.matched_games += 1; id} else {
                transaction.execute("INSERT INTO games (id,steam_app_id,automatic_name,name_override,executable_path,installation_root) VALUES (?1,?2,?3,?3,?4,?5)",params![game.id, game.steam_app_id, game.name, game.executable, game.root]).map_err(database_error)?;
                receipt.imported_games += 1; game.id
            };
            if game.duration > 0 || game.last_played.is_some() {
                let start = game.last_played.unwrap_or(now.saturating_sub(game.duration));
                let end = start.checked_add(game.duration).ok_or("Legacy playtime timestamp overflow")?;
                transaction.execute("INSERT INTO game_sessions (game_id,started_at,last_seen_at,ended_at,end_reason) VALUES (?1,?2,?3,?3,'finished')",params![id,start,end]).map_err(database_error)?;
            }
        }
        if !existing {
            transaction.execute("INSERT INTO settings (key,value) VALUES ('app_preferences',?1)", [&settings]).map_err(database_error)?;
            let theme = convert_preferences(&input.preferences)?.theme;
            let theme = match theme {crate::database::Theme::Dark=>"dark",crate::database::Theme::Light=>"light",crate::database::Theme::System=>"system"};
            transaction.execute("INSERT INTO settings (key,value) VALUES ('theme',?1)",[theme]).map_err(database_error)?;
        }
        transaction.execute("INSERT INTO download_sources (id,url,manifest,fetched_at) VALUES (?1,?2,?3,0) ON CONFLICT(url) DO NOTHING",params![uuid::Uuid::new_v4().to_string(),SOURCE_URL,manifest]).map_err(database_error)?;
        transaction.execute("INSERT INTO settings (key,value) VALUES (?1,?2)", params![marker,serde_json::to_string(&receipt).map_err(|e|e.to_string())?]).map_err(database_error)?;
        transaction.commit().map_err(database_error)?;
        Ok(receipt)
    })
}

#[cfg(any(windows, test))]
fn complete_import(
    database: &Database,
    input: &Export,
    register: impl FnOnce(bool) -> Result<(), String>,
) -> Result<Receipt, String> {
    let receipt = import(database, input)?;
    register(database.settings()?.launch_on_system_start)?;
    Ok(receipt)
}

fn bounded_read(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|e| format!("Could not read migration input: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_EXPORT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_EXPORT_BYTES {
        return Err("Migration input is too large".to_owned());
    }
    Ok(bytes)
}

pub fn run_cli(args: &[OsString]) -> Option<Result<(), String>> {
    match args.get(1).and_then(|s| s.to_str()) {
        Some("--validate-source-manifest") => Some((|| {
            if args.len() != 3 {
                return Err("Expected a source manifest path".to_owned());
            }
            crate::legio_source::parse_manifest(&bounded_read(Path::new(&args[2]))?)
                .map_err(|e| e.to_string())?;
            Ok(())
        })()),
        Some("--import-taxlauncher") => {
            let result = import_cli(args);
            if let Err(error) = &result
                && args.len() == 5
                && args[3] == "--import-report"
            {
                let persist = (|| -> Result<(), String> {
                    use std::io::Write;
                    let mut report = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(Path::new(&args[4]))
                        .map_err(|e| e.to_string())?;
                    let bytes = serde_json::to_vec_pretty(&serde_json::json!({"error": error}))
                        .map_err(|e| e.to_string())?;
                    report
                        .write_all(&bytes)
                        .and_then(|_| report.sync_all())
                        .map_err(|e| e.to_string())
                })();
                if let Err(write_error) = persist {
                    eprintln!("Could not persist migration failure: {write_error}");
                }
            }
            Some(result)
        }
        _ => None,
    }
}

#[cfg(windows)]
fn import_cli(args: &[OsString]) -> Result<(), String> {
    if args.len() != 5 || args[3] != "--import-report" {
        return Err("Expected export and report paths".to_owned());
    }
    let input: Export = serde_json::from_slice(&bounded_read(Path::new(&args[2]))?)
        .map_err(|e| format!("Invalid migration export: {e}"))?;
    let config = tauri::generate_context!();
    let data_dir = dirs::data_dir()
        .ok_or("Could not resolve the production app-data directory")?
        .join(&config.config().identifier);
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let file = data_dir.join("legio.sqlite3");
    if file.exists() {
        let backup = data_dir.join(format!(
            "legio-before-taxlauncher-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = rusqlite::Connection::open(&file).map_err(database_error)?;
        connection
            .execute(
                "VACUUM INTO ?1",
                [backup.to_str().ok_or("Invalid database backup path")?],
            )
            .map_err(database_error)?;
    }
    let database = Database::open(&data_dir)?;
    let receipt = complete_import(&database, &input, |enabled| {
        crate::startup::set_enabled(enabled, false)
    })?;
    let report = Path::new(&args[4]);
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report)
        .map_err(|e| format!("Could not create migration receipt: {e}"))?;
    use std::io::Write;
    output
        .write_all(&serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    output.sync_all().map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn import_cli(_args: &[OsString]) -> Result<(), String> {
    Err("TaxLauncher replacement is supported on Windows only".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{CreateGameInput, Theme};
    use serde_json::json;
    use std::path::PathBuf;

    fn fixture() -> (Database, PathBuf, Export) {
        let root = std::env::temp_dir().join(format!("legio-tax-import-{}", uuid::Uuid::new_v4()));
        let db = Database::open(&root).unwrap();
        let input = serde_json::from_value(json!({
            "schemaVersion": 1, "migrationId": "fbbe5c0d-3d93-4f83-9b65-361ce916f83b",
            "games": [
                {"id":"1711000000000", "name":"Portal personalizzato", "path":"C:\\Games\\Portal\\portal.exe", "cover":"https://cdn.akamai.steamstatic.com/steam/apps/400/library_600x900.jpg", "playtime":12.5,"lastPlayed":"2026-10-11T12:00:00Z"},
                {"id":"42", "name":"Manuale", "path":"D:\\Giochi\\manuale.exe", "playtime":0}
            ],
            "preferences":{"appLanguage":"it", "closeToTray":false, "minimizeOnGameLaunch":false, "autoStart":"true", "discordRpcEnabled":"false", "downloadPath":"D:\\Downloads"},
            "sourceManifest":{"schemaVersion":1,"generatedAt":"2026-10-11T12:00:00Z","verified":[],"unverified":[]}
        })).unwrap();
        (db, root, input)
    }

    fn cleanup(db: Database, root: PathBuf) {
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_registration_failure_blocks_completion_and_retry_registers_without_duplicates() {
        let (db, root, input) = fixture();
        assert!(
            complete_import(&db, &input, |enabled| {
                assert!(enabled);
                Err("test registration failure".to_owned())
            })
            .is_err()
        );
        let mut registered = false;
        complete_import(&db, &input, |enabled| {
            registered = enabled;
            Ok(())
        })
        .unwrap();
        assert!(registered);
        assert_eq!(db.games().unwrap().len(), 2);
        assert_eq!(
            db.playtime_summaries(1_800_000_000_000)
                .unwrap()
                .iter()
                .map(|s| s.total_milliseconds)
                .sum::<i64>(),
            750000
        );
        cleanup(db, root);
    }

    #[test]
    fn imported_games_work_with_normal_lookup_edit_launch_config_and_delete_apis() {
        let (db, root, input) = fixture();
        import(&db, &input).unwrap();
        let game = db.games().unwrap().remove(0);
        assert_eq!(db.game(&game.id).unwrap(), game);
        db.save_native_launch_config(&game.id, crate::database::NativeLaunchConfig::default())
            .unwrap();
        db.update_game(crate::database::UpdateGameInput {
            id: game.id.clone(),
            steam_app_id: game.steam_app_id,
            automatic_name: game.automatic_name,
            name_override: Some("Edited".to_owned()),
        })
        .unwrap();
        db.remove_game(&game.id).unwrap();
        cleanup(db, root);
    }

    #[test]
    fn converts_library_minutes_preferences_and_source_in_native_schema() {
        let (db, root, input) = fixture();
        let receipt = import(&db, &input).unwrap();
        assert_eq!(receipt.imported_games, 2);
        let games = db.games().unwrap();
        assert_eq!(games.len(), 2);
        let portal = games
            .iter()
            .find(|g| g.name == "Portal personalizzato")
            .unwrap();
        assert_eq!(portal.steam_app_id, Some(400));
        assert!(
            portal
                .executable_path
                .as_ref()
                .unwrap()
                .ends_with("portal.exe")
        );
        assert_eq!(
            games
                .iter()
                .find(|g| g.name == "Manuale")
                .unwrap()
                .steam_app_id,
            None
        );
        let settings = db.settings().unwrap();
        assert!(!settings.close_to_tray);
        assert!(!settings.hide_on_game_start);
        assert!(!settings.discord_presence.enabled);
        assert!(settings.launch_on_system_start);
        assert!(!settings.application_logging_enabled && !settings.diagnostics_enabled);
        assert!(settings.onboarding_completed);
        let summary = db.playtime_summaries(1_800_000_000_000).unwrap();
        assert_eq!(
            summary
                .iter()
                .find(|s| s.game_id == portal.id)
                .unwrap()
                .total_milliseconds,
            750000
        );
        assert_eq!(
            db.with_connection(|c| c
                .query_row("SELECT url FROM download_sources", [], |r| r
                    .get::<_, String>(0))
                .map_err(crate::database::database_error))
                .unwrap(),
            "https://source.taxphobia.top/store.json"
        );
        cleanup(db, root);
    }

    #[test]
    fn retry_does_not_duplicate_games_or_sessions_and_rejects_changed_export() {
        let (db, root, mut input) = fixture();
        let receipt = import(&db, &input).unwrap();
        assert_eq!(import(&db, &input).unwrap(), receipt);
        assert_eq!(db.games().unwrap().len(), 2);
        assert_eq!(
            db.playtime_summaries(1_800_000_000_000)
                .unwrap()
                .iter()
                .map(|s| s.total_milliseconds)
                .sum::<i64>(),
            750000
        );
        input.games[0]["playtime"] = json!(999);
        assert!(import(&db, &input).is_err());
        cleanup(db, root);
    }

    #[test]
    fn existing_legio_preferences_and_case_insensitive_executable_are_preserved() {
        let (db, root, input) = fixture();
        let existing = db
            .create_game(CreateGameInput {
                name: "Existing".into(),
                steam_app_id: None,
            })
            .unwrap();
        db.with_connection(|c| {
            c.execute(
                "UPDATE games SET executable_path=?1 WHERE id=?2",
                rusqlite::params!["c:\\games\\portal\\PORTAL.EXE", existing.id],
            )
            .map(|_| ())
            .map_err(crate::database::database_error)
        })
        .unwrap();
        let mut settings = db.settings().unwrap();
        settings.theme = Theme::Light;
        db.save_settings(settings.clone()).unwrap();
        let receipt = import(&db, &input).unwrap();
        assert_eq!(receipt.matched_games, 1);
        assert_eq!(db.games().unwrap().len(), 2);
        assert_eq!(db.game(&existing.id).unwrap().name, "Existing");
        assert_eq!(db.settings().unwrap(), settings);
        cleanup(db, root);
    }

    #[test]
    fn invalid_late_game_or_source_leaves_all_import_tables_unchanged() {
        let (db, root, mut input) = fixture();
        input.games[1]["name"] = json!("");
        assert!(import(&db, &input).is_err());
        assert!(db.games().unwrap().is_empty());
        input.games[1]["name"] = json!("Manuale");
        input.source_manifest["schemaVersion"] = json!(99);
        assert!(import(&db, &input).is_err());
        assert!(db.games().unwrap().is_empty());
        cleanup(db, root);
    }

    #[test]
    fn database_failure_rolls_back_games_sessions_preferences_source_and_receipt() {
        let (db, root, input) = fixture();
        db.with_connection(|c|c.execute_batch("CREATE TRIGGER fail_source BEFORE INSERT ON download_sources BEGIN SELECT RAISE(ABORT,'test source failure'); END;").map_err(crate::database::database_error)).unwrap();
        assert!(import(&db, &input).is_err());
        db.with_connection(|c| {
            for table in ["games", "game_sessions", "settings", "download_sources"] {
                let count: i64 = c
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                    .map_err(crate::database::database_error)?;
                assert_eq!(count, 0);
            }
            Ok(())
        })
        .unwrap();
        cleanup(db, root);
    }

    #[test]
    fn rejects_invalid_paths_negative_playtime_duplicate_ids_and_spoofed_cover_hosts() {
        let (db, root, mut input) = fixture();
        input.games[0]["cover"] =
            json!("https://steamstatic.com.evil.invalid/steam/apps/99/header.jpg");
        import(&db, &input).unwrap();
        assert_eq!(
            db.games()
                .unwrap()
                .iter()
                .find(|g| g.name == "Portal personalizzato")
                .unwrap()
                .steam_app_id,
            None
        );
        cleanup(db, root);
        for (field, value) in [
            ("path", json!("..\\bad.exe")),
            ("playtime", json!(-1)),
            ("id", json!("42")),
        ] {
            let (db, root, mut input) = fixture();
            input.games[0][field] = value;
            assert!(import(&db, &input).is_err(), "{field}");
            assert!(db.games().unwrap().is_empty());
            cleanup(db, root);
        }
    }
}
