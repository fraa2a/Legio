//! A zip with the information needed to report a problem. Everything in it is
//! chosen by an allowlist, so the user can read `README.txt` and know what is shared.

use std::{
    fmt::Write as _,
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

use regex::Regex;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::{
    application_log::ApplicationLog,
    database::{Database, DatabaseState, Settings},
    diagnostics::Diagnostics,
    modding::support_report::{read_tail_limited, redact_home},
};

// Per file: the newest lines up to this size are kept.
const MAX_LOG_BYTES: u64 = 256 * 1024;
const USER_PLACEHOLDER: &str = "<user>";
const APPLICATION_LOGS: [&str; 2] = ["app.txt", "app.previous.txt"];
const NETWORK_LOGS: [&str; 2] = ["network.jsonl", "network.previous.jsonl"];

struct LogSource {
    title: &'static str,
    enabled: bool,
    directory: Option<PathBuf>,
    names: [&'static str; 2],
}

struct Inputs {
    system: String,
    settings: Result<Settings, String>,
    database: Result<Value, String>,
    logs: Vec<LogSource>,
    home: PathBuf,
}

/// Hides the home folder and the user name, which can also appear on their own.
fn redact_private(text: &str, home: &Path) -> String {
    let text = redact_home(text, home);
    let Some(user) = home
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.chars().count() >= 3)
    else {
        return text;
    };
    match Regex::new(&format!("(?i){}", regex::escape(user))) {
        Ok(pattern) => pattern.replace_all(&text, USER_PLACEHOLDER).into_owned(),
        Err(_) => text,
    }
}

/// Only the fields listed here are shared. Paths, accounts and the appearance
/// (which can name a background file) are never copied.
fn settings_json(settings: &Settings) -> Value {
    json!({
        "theme": settings.theme,
        "language": settings.language,
        "steamLibraryPollMinutes": settings.steam_library_poll_minutes,
        "syncSteamGames": settings.sync_steam_games,
        "customDownloadPathSet": settings.download_path.is_some(),
        "closeToTray": settings.close_to_tray,
        "hideOnGameStart": settings.hide_on_game_start,
        "launchOnSystemStart": settings.launch_on_system_start,
        "launchMinimized": settings.launch_minimized,
        "launchInLibrary": settings.launch_in_library,
        "sidebarCollapsed": settings.sidebar_collapsed,
        "libraryPortrait": settings.library_portrait,
        "automaticGameUpdates": settings.automatic_game_updates,
        "downloadWhilePlaying": settings.download_while_playing,
        "downloadNotifications": settings.download_notifications,
        "verifyVerifiedDownloads": settings.verify_verified_downloads,
        "diagnosticsEnabled": settings.diagnostics_enabled,
        "applicationLoggingEnabled": settings.application_logging_enabled,
        "deferExtractionWhilePlaying": settings.defer_extraction_while_playing,
        "thunderstoreLinksEnabled": settings.thunderstore_links_enabled,
    })
}

/// The schema version and the number of rows per table, never the rows.
fn database_summary(database: &Database) -> Result<Value, String> {
    database.with_connection(|connection| {
        let error =
            |error: rusqlite::Error| format!("could not read the database summary: {error}");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(error)?;
        let mut statement = connection
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .map_err(error)?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        let mut rows = serde_json::Map::new();
        for name in names {
            let count: i64 = connection
                .query_row(
                    &format!("SELECT COUNT(*) FROM \"{}\"", name.replace('"', "\"\"")),
                    [],
                    |row| row.get(0),
                )
                .map_err(error)?;
            rows.insert(name, count.into());
        }
        Ok(json!({ "schemaVersion": version, "rowCounts": rows }))
    })
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

/// The newest lines of each existing log, with the user's identity removed.
/// Records are already redacted when written; this is a second pass.
fn collect_logs(source: &LogSource, home: &Path) -> (Vec<(String, String)>, String) {
    let files = source
        .directory
        .iter()
        .flat_map(|directory| {
            source
                .names
                .iter()
                .map(|name| (*name, directory.join(name)))
        })
        .filter(|(_, path)| is_regular_file(path))
        .map(|(name, path)| {
            let text = read_tail_limited(&path, MAX_LOG_BYTES)
                .unwrap_or_else(|error| format!("Could not read this log: {error}\n"));
            (format!("logs/{name}"), redact_private(&text, home))
        })
        .collect::<Vec<_>>();
    let note = match (source.enabled, files.is_empty()) {
        (true, true) => format!(
            "{}: enabled, but nothing has been recorded yet. Reproduce the problem and create the bundle again.",
            source.title
        ),
        (false, true) => format!(
            "{}: disabled, so there is nothing to include. Enable it in Settings, restart Legio, reproduce the problem and create the bundle again.",
            source.title
        ),
        (true, false) => format!("{}: enabled, newest lines included.", source.title),
        (false, false) => format!(
            "{}: disabled now; the newest lines recorded earlier are included.",
            source.title
        ),
    };
    (files, note)
}

fn readme(entries: &[String], log_notes: &[String]) -> String {
    let mut text = String::from(
        "Legio support bundle\n\nThis archive was created on request and was not sent anywhere. Attach it to a bug report only after checking it.\n\nFiles:\n",
    );
    for entry in entries {
        let _ = writeln!(text, "- {entry}");
    }
    text.push_str("\nLogs:\n");
    for note in log_notes {
        let _ = writeln!(text, "- {note}");
    }
    let _ = write!(
        text,
        "\nLogs keep at most the newest {} KiB per file. Paths, URLs, tokens, identifiers and the user name are redacted.\nNot included: game names, library content, download sources, account names, Steam ids, file paths and database rows.\n",
        MAX_LOG_BYTES / 1024
    );
    text
}

fn build_bundle(inputs: &Inputs) -> Result<Vec<u8>, String> {
    let home = inputs.home.as_path();
    let mut files = vec![(
        "system.txt".to_owned(),
        redact_private(&inputs.system, home),
        "app version, build and system information",
    )];
    let settings = match &inputs.settings {
        Ok(settings) => serde_json::to_string_pretty(&settings_json(settings))
            .map_err(|error| error.to_string())?,
        Err(error) => format!(
            "Settings are unavailable: {}\n",
            redact_private(error, home)
        ),
    };
    files.push((
        "settings.json".to_owned(),
        settings,
        "non-sensitive settings from a fixed list",
    ));
    let database = match &inputs.database {
        Ok(summary) => serde_json::to_string_pretty(summary).map_err(|error| error.to_string())?,
        Err(error) => format!(
            "The database is unavailable: {}\n",
            redact_private(error, home)
        ),
    };
    files.push((
        "database.json".to_owned(),
        database,
        "database schema version and row counts per table",
    ));
    let mut log_notes = Vec::new();
    for source in &inputs.logs {
        let (logs, note) = collect_logs(source, home);
        log_notes.push(note);
        for (name, text) in logs {
            files.push((name, text, "log, newest lines only"));
        }
    }
    let entries = ["README.txt - this list".to_owned()]
        .into_iter()
        .chain(
            files
                .iter()
                .map(|(name, _, purpose)| format!("{name} - {purpose}")),
        )
        .collect::<Vec<_>>();
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut add = |name: &str, text: &str| -> Result<(), String> {
        writer
            .start_file(name, options)
            .map_err(|error| error.to_string())?;
        writer
            .write_all(text.as_bytes())
            .map_err(|error| error.to_string())
    };
    add("README.txt", &readme(&entries, &log_notes))
        .map_err(|error| format!("could not write the support bundle: {error}"))?;
    for (name, text, _) in &files {
        add(name, text).map_err(|error| format!("could not write the support bundle: {error}"))?;
    }
    writer
        .finish()
        .map(Cursor::into_inner)
        .map_err(|error| format!("could not write the support bundle: {error}"))
}

fn linux_distribution() -> Option<String> {
    let text = fs::read_to_string("/etc/os-release").ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .map(|name| name.trim_matches('"').to_owned())
}

fn system_report(app: &AppHandle) -> String {
    let package = app.package_info();
    let mut lines = vec![
        format!("Legio: {}", package.version),
        format!(
            "Build: {}",
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
        ),
        format!("Identifier: {}", app.config().identifier),
        format!("OS: {}", std::env::consts::OS),
        format!("Architecture: {}", std::env::consts::ARCH),
        format!(
            "System tray available: {}",
            app.state::<crate::TrayAvailable>().0
        ),
        format!(
            "Created: {}",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        ),
    ];
    if cfg!(target_os = "linux") {
        let state = |name: &str| {
            if std::env::var_os(name).is_some() {
                "set"
            } else {
                "unset"
            }
        };
        lines.push(format!(
            "Distribution: {}",
            linux_distribution().unwrap_or_else(|| "unknown".to_owned())
        ));
        lines.push(format!(
            "Desktop: {}",
            crate::commands::desktop_environment().unwrap_or_else(|| "unknown".to_owned())
        ));
        lines.push(format!(
            "Session type: {}",
            std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".to_owned())
        ));
        lines.push(format!(
            "Display backend: Wayland socket {}, X11 display {}",
            state("WAYLAND_DISPLAY"),
            state("DISPLAY")
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn collect_inputs(app: &AppHandle) -> Inputs {
    let state = app.state::<DatabaseState>();
    let application = app.state::<ApplicationLog>().status();
    let network = app.state::<Diagnostics>().status();
    Inputs {
        system: system_report(app),
        settings: state.database().and_then(Database::settings),
        database: state.database().and_then(database_summary),
        logs: vec![
            LogSource {
                title: "Application log",
                enabled: application.enabled,
                directory: application.directory,
                names: APPLICATION_LOGS,
            },
            LogSource {
                title: "Network log",
                enabled: network.enabled,
                directory: network.directory,
                names: NETWORK_LOGS,
            },
        ],
        home: dirs::home_dir().unwrap_or_default(),
    }
}

#[tauri::command]
pub async fn create_support_bundle(app: AppHandle, path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&path);
        if !path.is_absolute() {
            return Err("Support bundle paths must be absolute".to_owned());
        }
        let bundle = build_bundle(&collect_inputs(&app))?;
        crate::atomic_file::write_atomic(path, &bundle)
            .map_err(|error| format!("Could not write the support bundle: {error}"))?;
        crate::application_log::state_change("support_bundle", "Support bundle created");
        Ok(())
    })
    .await
    .map_err(|error| format!("Support bundle task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use zip::ZipArchive;

    use super::*;

    struct Temporary(PathBuf);

    impl Temporary {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("legio-bundle-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Temporary {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn inputs(logs: Vec<LogSource>) -> Inputs {
        Inputs {
            system: "Legio: 1.0.0\n".to_owned(),
            settings: Ok(Settings::default()),
            database: Ok(json!({ "schemaVersion": 39, "rowCounts": { "games": 2 } })),
            logs,
            home: PathBuf::from("/home/sentinel_user"),
        }
    }

    fn entries(bytes: &[u8]) -> Vec<(String, String)> {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        (0..archive.len())
            .map(|index| {
                let mut file = archive.by_index(index).unwrap();
                let mut text = String::new();
                file.read_to_string(&mut text).unwrap();
                (file.name().unwrap().to_string(), text)
            })
            .collect()
    }

    fn entry<'a>(entries: &'a [(String, String)], name: &str) -> &'a str {
        &entries.iter().find(|(entry, _)| entry == name).unwrap().1
    }

    fn application_source(directory: &Path, enabled: bool) -> LogSource {
        LogSource {
            title: "Application log",
            enabled,
            directory: Some(directory.to_path_buf()),
            names: APPLICATION_LOGS,
        }
    }

    #[test]
    fn settings_outside_the_allowlist_never_reach_the_bundle() {
        let settings = Settings {
            download_path: Some("/mnt/SENTINEL_PATH/games".to_owned()),
            store_sort: "SENTINEL_SORT".to_owned(),
            ..Settings::default()
        };
        let mut input = inputs(Vec::new());
        input.settings = Ok(settings);
        let bytes = build_bundle(&input).unwrap();
        let entries = entries(&bytes);
        for (name, text) in &entries {
            assert!(!text.contains("SENTINEL"), "{name}");
        }
        assert!(entry(&entries, "settings.json").contains("\"customDownloadPathSet\": true"));
    }

    #[test]
    fn home_folder_and_user_name_are_redacted() {
        let home = Path::new("/home/sentinel_user");
        let text = redact_private("at /home/sentinel_user/x by Sentinel_User", home);
        assert_eq!(text, "at ~/x by <user>");
        assert_eq!(redact_private("/home/me/x", Path::new("/home/me")), "~/x");
    }

    #[test]
    fn logs_are_redacted_a_second_time() {
        let directory = Temporary::new();
        fs::write(
            directory.0.join("app.txt"),
            "line from /home/sentinel_user/cache for sentinel_user\n",
        )
        .unwrap();
        let bytes = build_bundle(&inputs(vec![application_source(&directory.0, true)])).unwrap();
        let entries = entries(&bytes);
        let log = entry(&entries, "logs/app.txt");
        assert!(!log.contains("sentinel_user"), "{log}");
        assert!(log.contains("<user>"));
    }

    #[test]
    fn logs_keep_the_newest_lines_within_the_cap() {
        let directory = Temporary::new();
        let count = (MAX_LOG_BYTES as usize / 100) * 2;
        let text = (0..count)
            .map(|index| format!("{index:08} {}\n", "x".repeat(90)))
            .collect::<String>();
        fs::write(directory.0.join("app.txt"), &text).unwrap();
        let bytes = build_bundle(&inputs(vec![application_source(&directory.0, true)])).unwrap();
        let entries = entries(&bytes);
        let log = entry(&entries, "logs/app.txt");
        assert!(log.len() as u64 <= MAX_LOG_BYTES);
        assert!(text.ends_with(log));
        assert!(log.starts_with(|first: char| first.is_ascii_digit()));
        assert!(!log.contains("00000000 "));
    }

    #[test]
    fn disabled_logging_without_files_is_explained() {
        let directory = Temporary::new();
        let bytes = build_bundle(&inputs(vec![application_source(&directory.0, false)])).unwrap();
        let entries = entries(&bytes);
        assert!(entries.iter().all(|(name, _)| !name.starts_with("logs/")));
        let readme = entry(&entries, "README.txt");
        assert!(readme.contains("Application log: disabled"));
        assert!(readme.contains("Enable it in Settings"));
    }

    #[test]
    fn unavailable_database_and_settings_are_recorded() {
        let mut input = inputs(Vec::new());
        input.settings = Err("failed in /home/sentinel_user/data".to_owned());
        input.database = Err("cannot open /home/sentinel_user/data".to_owned());
        let bytes = build_bundle(&input).unwrap();
        let entries = entries(&bytes);
        assert!(entry(&entries, "database.json").contains("cannot open ~/data"));
        assert!(entry(&entries, "settings.json").contains("failed in ~/data"));
    }

    #[test]
    fn readme_lists_every_file() {
        let bytes = build_bundle(&inputs(Vec::new())).unwrap();
        let entries = entries(&bytes);
        let readme = entry(&entries, "README.txt");
        for (name, _) in &entries {
            assert!(readme.contains(name.as_str()), "{name}");
        }
    }

    #[test]
    fn database_summary_has_counts_only() {
        let directory = Temporary::new();
        let database = Database::open(&directory.0).unwrap();
        let summary = database_summary(&database).unwrap();
        assert!(summary["schemaVersion"].as_i64().unwrap() > 0);
        assert_eq!(summary["rowCounts"]["games"], 0);
    }
}
