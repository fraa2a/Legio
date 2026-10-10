use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, TryLockError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use regex::Regex;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};

const MAX_LOG_BYTES: u64 = 256 * 1024;
const MAX_MESSAGE_BYTES: usize = 1024;
const MAX_STACK_BYTES: usize = 4096;
const REPORTS_PER_MINUTE: u32 = 50;
static APPLICATION_LOG: OnceLock<ApplicationLog> = OnceLock::new();

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogStatus {
    enabled: bool,
    directory: Option<PathBuf>,
    last_error: Option<String>,
    dropped_records: u64,
}

struct State {
    status: LogStatus,
    report_window: Instant,
    reports: u32,
    critical_reports: u32,
}

#[derive(Clone)]
pub(crate) struct ApplicationLog {
    state: Arc<Mutex<State>>,
    session: Arc<str>,
}

#[derive(Serialize)]
struct Record<'a> {
    timestamp_ms: u128,
    category: &'static str,
    session: &'a str,
    version: &'static str,
    platform: &'static str,
    level: &'a str,
    event: &'a str,
    message: Option<String>,
    stack: Option<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RendererEvent {
    RendererStart,
    RendererReady,
    RendererError,
    UnhandledRejection,
    Console,
    Navigation,
    CommandError,
    BootstrapError,
}

impl RendererEvent {
    fn is_critical(self) -> bool {
        matches!(
            self,
            Self::RendererError | Self::UnhandledRejection | Self::BootstrapError
        )
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::RendererStart => "renderer_start",
            Self::RendererReady => "renderer_ready",
            Self::RendererError => "renderer_error",
            Self::UnhandledRejection => "unhandled_rejection",
            Self::Console => "console",
            Self::Navigation => "navigation",
            Self::CommandError => "command_error",
            Self::BootstrapError => "bootstrap_error",
        }
    }
}

impl ApplicationLog {
    pub(crate) fn new(directory: PathBuf, enabled: bool) -> Self {
        Self::with_directory(Ok(directory), enabled)
    }

    fn with_directory(directory: Result<PathBuf, String>, enabled: bool) -> Self {
        let log = Self {
            state: Arc::new(Mutex::new(State {
                status: LogStatus {
                    enabled: false,
                    directory: directory.as_ref().ok().cloned(),
                    last_error: directory
                        .err()
                        .map(|_| "Could not locate the application log directory.".to_owned()),
                    dropped_records: 0,
                },
                report_window: Instant::now(),
                reports: 0,
                critical_reports: 0,
            })),
            session: uuid::Uuid::new_v4().to_string().into(),
        };
        log.set_enabled(enabled);
        log
    }

    pub(crate) fn status(&self) -> LogStatus {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .status
            .clone()
    }

    pub(crate) fn set_enabled(&self, enabled: bool) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let changed = state.status.enabled != enabled;
        state.status.enabled = enabled;
        if changed && enabled {
            state.report_window = Instant::now();
            state.reports = 0;
            state.critical_reports = 0;
            if let Some(directory) = &state.status.directory {
                let result = prepare(directory);
                update_status(&mut state.status, result);
            }
        }
    }

    pub(crate) fn report(
        &self,
        level: Level,
        event: RendererEvent,
        message: Option<&str>,
        stack: Option<&str>,
    ) -> Result<(), String> {
        if message.is_some_and(|message| message.len() > MAX_MESSAGE_BYTES)
            || stack.is_some_and(|stack| stack.len() > MAX_STACK_BYTES)
        {
            return Err("Application log report exceeds the size limit".to_owned());
        }
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if !state.status.enabled {
            return Ok(());
        }
        if state.report_window.elapsed() >= Duration::from_secs(60) {
            state.report_window = Instant::now();
            state.reports = 0;
            state.critical_reports = 0;
        }
        let reports = if event.is_critical() {
            &mut state.critical_reports
        } else {
            &mut state.reports
        };
        if *reports >= REPORTS_PER_MINUTE {
            state.status.dropped_records = state.status.dropped_records.saturating_add(1);
            return Err("Application log report rate limit reached".to_owned());
        }
        *reports += 1;
        self.write(
            &mut state,
            level.as_str(),
            event.as_str(),
            message,
            stack,
            false,
        );
        Ok(())
    }

    pub(crate) fn record(
        &self,
        level: &str,
        event: &str,
        message: Option<&str>,
        stack: Option<&str>,
        durable: bool,
    ) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        self.write(&mut state, level, event, message, stack, durable);
    }

    fn record_panic(&self, info: &std::panic::PanicHookInfo<'_>) {
        let mut state = match self.state.try_lock() {
            Ok(state) => state,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
            Err(TryLockError::WouldBlock) => return,
        };
        if !state.status.enabled {
            return;
        }
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("Rust panic");
        let location = info
            .location()
            .map(|location| {
                let filename = Path::new(location.file())
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                format!("{filename}:{}:{}", location.line(), location.column())
            })
            .unwrap_or_default();
        self.write(
            &mut state,
            "error",
            "panic",
            Some(payload),
            Some(&format!(
                "{location}\n{}",
                std::backtrace::Backtrace::force_capture()
            )),
            true,
        );
    }

    fn write(
        &self,
        state: &mut State,
        level: &str,
        event: &str,
        message: Option<&str>,
        stack: Option<&str>,
        durable: bool,
    ) {
        if !state.status.enabled {
            return;
        }
        let Some(directory) = &state.status.directory else {
            state.status.dropped_records = state.status.dropped_records.saturating_add(1);
            return;
        };
        let record = Record {
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            category: "application",
            session: &self.session,
            version: env!("CARGO_PKG_VERSION"),
            platform: std::env::consts::OS,
            level,
            event,
            message: message.map(|message| sanitize(message, MAX_MESSAGE_BYTES)),
            stack: stack.map(|stack| sanitize(stack, MAX_STACK_BYTES)),
        };
        let result = append(directory, &record, durable);
        update_status(&mut state.status, result);
    }
}

fn update_status(status: &mut LogStatus, result: io::Result<()>) {
    match result {
        Ok(()) => status.last_error = None,
        Err(error) => {
            status.dropped_records = status.dropped_records.saturating_add(1);
            status.last_error = Some(format!(
                "Could not write application logs ({:?}). Check directory permissions and free disk space.",
                error.kind()
            ));
        }
    }
}

fn prepare(directory: &Path) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    for name in ["app.jsonl", "app.previous.jsonl"] {
        let path = directory.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(io::Error::other("Application log is not a regular file"));
            }
            Ok(metadata) if metadata.len() > MAX_LOG_BYTES => fs::remove_file(path)?,
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn open_log(path: &Path) -> io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

fn append(directory: &Path, record: &Record<'_>, durable: bool) -> io::Result<()> {
    prepare(directory)?;
    let path = directory.join("app.jsonl");
    let mut line = serde_json::to_vec(record)?;
    line.push(b'\n');
    let mut file = open_log(&path)?;
    if file.metadata()?.len().saturating_add(line.len() as u64) > MAX_LOG_BYTES {
        drop(file);
        let previous = directory.join("app.previous.jsonl");
        match fs::remove_file(&previous) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(&path, previous)?;
        file = open_log(&path)?;
    }
    file.write_all(&line)?;
    if durable {
        file.sync_data()?;
    }
    Ok(())
}

fn redacted_location(text: &str) -> String {
    let text = text.trim_end_matches([')', ']', '}', ',', ';']);
    if let Some((prefix, column)) = text.rsplit_once(':')
        && let Some((_, line)) = prefix.rsplit_once(':')
        && !line.is_empty()
        && !column.is_empty()
        && line.bytes().all(|byte| byte.is_ascii_digit())
        && column.bytes().all(|byte| byte.is_ascii_digit())
        && line.parse::<u32>().is_ok()
        && column.parse::<u32>().is_ok()
    {
        return format!("[redacted]:{line}:{column}");
    }
    "[redacted]".to_owned()
}

fn sanitize(text: &str, limit: usize) -> String {
    static QUOTED_CONTENT: OnceLock<Regex> = OnceLock::new();
    static PATH_FILTERS: OnceLock<Vec<Regex>> = OnceLock::new();
    static FILTERS: OnceLock<Vec<Regex>> = OnceLock::new();
    let quoted_content = QUOTED_CONTENT.get_or_init(|| {
        Regex::new(r#"\"[^\"\n]*\"|'[^'\n]*'|`[^`\n]*`"#)
            .expect("constant application log quoted-content pattern")
    });
    let path_filters = PATH_FILTERS.get_or_init(|| {
        [
            r#"(?i)\b[a-z][a-z0-9+.-]*://[^\s)\]\}<>\"']+"#,
            r#"(?i)\b[a-z]:[\\/][^\r\n]*|\\\\[^\r\n]*"#,
            r#"(?:~?/|\./|\.\./)[^\r\n]*"#,
        ]
        .into_iter()
        .map(|pattern| Regex::new(pattern).expect("constant application log path pattern"))
        .collect()
    });
    let filters = FILTERS.get_or_init(|| [
        r"(?i)\b(?:set-)?cookie\s*:[^\r\n]*",
        r"(?i)\b(?:bearer|basic)\s+[^\s,;]+",
        r#"(?i)\b(?:password|passwd|pwd|token|access[_-]?token|refresh[_-]?token|authorization|cookie|secret|api[_-]?key)\s*[=:]\s*(?:\"[^\"]*\"|'[^']*'|[^\s,;]+)"#,
        r"(?i)\b(?:steam[ _-]*(?:app[ _-]*)?id|app[ _-]*id|game[ _-]*id|download[ _-]*id|account[ _-]*id)\s*(?:[:=]\s*)?[a-z0-9_-]+",
        r"[\w.+-]+@[\w.-]+\.[a-zA-Z]{2,}",
        r"\b(?:[a-fA-F0-9]{32,}|[0-9]{15,}|[a-fA-F0-9]{8}(?:-[a-fA-F0-9]{4}){3}-[a-fA-F0-9]{12})\b",
        r"\beyJ[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+){1,2}\b",
    ].into_iter().map(|pattern| Regex::new(pattern).expect("constant application log redaction pattern")).collect());
    let mut result = text
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .collect::<String>();
    result = quoted_content
        .replace_all(&result, "[redacted]")
        .into_owned();
    for filter in path_filters {
        result = filter
            .replace_all(&result, |capture: &regex::Captures<'_>| {
                redacted_location(&capture[0])
            })
            .into_owned();
    }
    for filter in filters {
        result = filter.replace_all(&result, "[redacted]").into_owned();
    }
    if result.len() > limit {
        let mut boundary = limit;
        while !result.is_char_boundary(boundary) {
            boundary -= 1;
        }
        result.truncate(boundary);
    }
    result
}

fn saved_enabled(data_directory: &Path) -> bool {
    let path = data_directory.join("legio.sqlite3");
    if !path.is_file() {
        return false;
    }
    let read = || -> rusqlite::Result<Option<String>> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'app_preferences'",
                [],
                |row| row.get(0),
            )
            .optional()
    };
    read()
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(&value).ok())
        .and_then(|value| {
            value
                .get("applicationLoggingEnabled")
                .and_then(serde_json::Value::as_bool)
        })
        .unwrap_or(false)
}

pub(crate) fn initialize(config: &tauri::Config) -> ApplicationLog {
    let data_directory = dirs::data_dir().map(|directory| directory.join(&config.identifier));
    let log_directory =
        dirs::data_local_dir().map(|directory| directory.join(&config.identifier).join("logs"));
    initialize_at(data_directory.as_deref(), log_directory)
}

fn initialize_at(data_directory: Option<&Path>, log_directory: Option<PathBuf>) -> ApplicationLog {
    APPLICATION_LOG
        .get_or_init(|| {
            let enabled = data_directory.is_some_and(saved_enabled);
            let log = match log_directory {
                Some(directory) => ApplicationLog::new(directory, enabled),
                None => ApplicationLog::with_directory(
                    Err("Application log directory unavailable".to_owned()),
                    enabled,
                ),
            };
            let panic_log = log.clone();
            let previous_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                panic_log.record_panic(info);
                previous_hook(info);
            }));
            log
        })
        .clone()
}

pub(crate) fn failure(context: &'static str, error: impl std::fmt::Display) {
    if let Some(log) = APPLICATION_LOG.get() {
        log.record(
            "error",
            "rust_failure",
            Some(&format!("{context}: {error}")),
            None,
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn directory() -> PathBuf {
        std::env::temp_dir().join(format!("legio-app-log-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn opt_out_creates_nothing_and_disabling_stops_writes() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), false);
        log.record("error", "panic", Some("failure"), None, true);
        assert!(!directory.exists());
        log.set_enabled(true);
        log.record("info", "startup", None, None, false);
        let before = fs::read(directory.join("app.jsonl")).unwrap();
        log.set_enabled(false);
        log.record("error", "panic", Some("failure"), None, true);
        assert_eq!(fs::read(directory.join("app.jsonl")).unwrap(), before);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn quoted_private_content_is_removed_before_greedy_path_redaction() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), true);
        for message in [
            "Could not open \"PRIVATE_USER /home/alice/My Private Game/config.json\"",
            "Could not open 'PRIVATE_USER C:\\Users\\alice\\My Private Game\\config.json'",
            "Could not open `PRIVATE_USER https://private.example/config.json`",
        ] {
            log.report(
                Level::Error,
                RendererEvent::CommandError,
                Some(message),
                None,
            )
            .unwrap();
        }
        let text = fs::read_to_string(directory.join("app.jsonl")).unwrap();
        assert!(
            !text.contains("PRIVATE_USER"),
            "private quoted prefix: {text}"
        );
        assert!(!text.contains("alice"));
        assert!(!text.contains("private.example"));
        assert!(!text.contains("My Private Game"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn records_redact_credentials_urls_paths_and_quoted_private_content() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), true);
        for (message, private) in [
            ("password=secret", "secret"),
            ("password=\"super secret\"", "super secret"),
            ("token:abc", "abc"),
            ("Bearer jwtsecret", "jwtsecret"),
            (
                "Authorization: Bearer privatecredential",
                "privatecredential",
            ),
            (
                "Authorization: Basic cHJpdmF0ZXVzZXI6cGFzc3dvcmQ=",
                "cHJpdmF0ZXVzZXI6cGFzc3dvcmQ=",
            ),
            (
                "https://user:pass@example.com/private?token=secret",
                "example.com",
            ),
            ("/home/alice/Games/Private Game", "Private Game"),
            ("C:\\Users\\Alice\\Games\\Private Game", "Private Game"),
            ("'Private game'", "Private game"),
            ("alice@example.com", "alice@example.com"),
            ("Steam account 76561198000000001", "76561198000000001"),
        ] {
            log.record("error", "renderer_error", Some(message), None, false);
            let text = fs::read_to_string(directory.join("app.jsonl")).unwrap();
            let record: serde_json::Value =
                serde_json::from_str(text.lines().last().unwrap()).unwrap();
            assert_eq!(record["event"], "renderer_error");
            let sanitized = record["message"].as_str().unwrap();
            assert!(
                !sanitized.contains(private),
                "leaked {private}: {sanitized}"
            );
            assert!(sanitized.contains("redacted"));
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn concurrent_writes_retain_complete_bounded_records_and_previous_file() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), true);
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let log = log.clone();
                std::thread::spawn(move || {
                    for _ in 0..300 {
                        log.record(
                            "error",
                            "rust_failure",
                            Some(&"failure ".repeat(128)),
                            None,
                            false,
                        );
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert!(directory.join("app.previous.jsonl").exists());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 2);
        for name in ["app.jsonl", "app.previous.jsonl"] {
            let path = directory.join(name);
            assert!(fs::metadata(&path).unwrap().len() <= 256 * 1024);
            for line in fs::read_to_string(path).unwrap().lines() {
                let value: serde_json::Value = serde_json::from_str(line).unwrap();
                assert_eq!(value["event"], "rust_failure");
            }
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn renderer_reports_redact_contextual_game_and_download_identifiers() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), true);
        log.report(
            Level::Error,
            RendererEvent::Console,
            Some("Steam App ID 620; game ID private-game; download ID private-download"),
            None,
        )
        .unwrap();
        let text = fs::read_to_string(directory.join("app.jsonl")).unwrap();
        let record: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        let message = record["message"].as_str().unwrap();
        for private in ["620", "private-game", "private-download"] {
            assert!(!message.contains(private), "leaked {private}");
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cookie_headers_redact_every_value_on_the_line() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), true);
        for header in ["Cookie", "Set-Cookie"] {
            log.record(
                "error",
                "renderer_error",
                Some(&format!(
                    "{header}: session_id=privatefirst; auth_session=privatesecond"
                )),
                None,
                false,
            );
            let text = fs::read_to_string(directory.join("app.jsonl")).unwrap();
            let record: serde_json::Value =
                serde_json::from_str(text.lines().last().unwrap()).unwrap();
            let message = record["message"].as_str().unwrap();
            assert!(!message.contains("privatefirst"));
            assert!(!message.contains("privatesecond"));
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn private_stack_urls_preserve_numeric_locations() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), true);
        log.record("error", "unhandled_rejection", Some("failure"), Some("at load (https://private.example/app-secret.js?token=private:123:456)\nat read (file:///home/alice/private.js:78:9)"), false);
        let text = fs::read_to_string(directory.join("app.jsonl")).unwrap();
        let record: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        let stack = record["stack"].as_str().unwrap();
        assert!(stack.contains("[redacted]:123:456"), "{stack}");
        assert!(stack.contains("[redacted]:78:9"), "{stack}");
        for private in ["private", "alice", "example"] {
            assert!(!stack.contains(private));
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn durable_panic_record_is_readable_without_a_background_flush() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), true);
        log.record("error", "panic", Some("panic failure"), None, true);
        let text = fs::read_to_string(directory.join("app.jsonl")).unwrap();
        let value: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(value["event"], "panic");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn renderer_reports_reject_oversized_input_and_limit_each_budget() {
        let directory = directory();
        let log = ApplicationLog::new(directory.clone(), true);
        assert!(
            log.report(
                Level::Error,
                RendererEvent::RendererError,
                Some(&"x".repeat(1025)),
                None
            )
            .is_err()
        );
        assert!(
            log.report(
                Level::Error,
                RendererEvent::RendererError,
                None,
                Some(&"x".repeat(4097))
            )
            .is_err()
        );
        assert!(!directory.join("app.jsonl").exists());
        for _ in 0..50 {
            log.report(Level::Warn, RendererEvent::Console, Some("warning"), None)
                .unwrap();
        }
        assert!(
            log.report(Level::Warn, RendererEvent::Console, None, None)
                .is_err()
        );
        for _ in 0..50 {
            log.report(
                Level::Error,
                RendererEvent::RendererError,
                Some("critical failure"),
                None,
            )
            .unwrap();
        }
        assert!(
            log.report(Level::Error, RendererEvent::UnhandledRejection, None, None)
                .is_err()
        );
        assert_eq!(log.status().dropped_records, 2);
        let text = fs::read_to_string(directory.join("app.jsonl")).unwrap();
        assert_eq!(text.lines().count(), 100);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn log_write_failures_remain_visible_without_private_path_details() {
        let directory = directory();
        fs::write(&directory, "not a directory").unwrap();
        let log = ApplicationLog::new(directory.clone(), true);
        log.record("error", "rust_failure", Some("failure"), None, false);
        let status = log.status();
        assert!(status.dropped_records > 0);
        let error = status.last_error.unwrap();
        assert!(error.contains("Could not write application logs"));
        assert!(!error.contains(directory.to_str().unwrap()));
        fs::remove_file(directory).unwrap();
    }

    #[test]
    fn startup_opt_in_reads_existing_preferences_without_migration_or_creation() {
        let directory = directory();
        assert!(!saved_enabled(&directory));
        assert!(!directory.exists());
        let database = crate::database::Database::open(&directory).unwrap();
        assert!(!saved_enabled(&directory));
        database
            .save_settings(crate::database::Settings {
                application_logging_enabled: true,
                ..crate::database::Settings::default()
            })
            .unwrap();
        drop(database);
        let path = directory.join("legio.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "user_version", 1_000_000)
            .unwrap();
        drop(connection);
        let before = fs::read(&path).unwrap();
        assert!(saved_enabled(&directory));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(crate::database::Database::open(&directory).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn preexisting_oversized_logs_are_removed_before_writing() {
        let directory = directory();
        fs::create_dir_all(&directory).unwrap();
        for name in ["app.jsonl", "app.previous.jsonl"] {
            fs::write(directory.join(name), vec![b'x'; 256 * 1024 + 1]).unwrap();
        }
        let log = ApplicationLog::new(directory.clone(), true);
        log.record("info", "startup", None, None, false);
        let text = fs::read_to_string(directory.join("app.jsonl")).unwrap();
        let record: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(record["event"], "startup");
        assert!(!directory.join("app.previous.jsonl").exists());
        fs::remove_dir_all(directory).unwrap();
    }

    fn panic_subprocess(enabled: bool, locked: bool) -> PathBuf {
        let directory = directory();
        let database = crate::database::Database::open(&directory).unwrap();
        database
            .save_settings(crate::database::Settings {
                application_logging_enabled: enabled,
                ..crate::database::Settings::default()
            })
            .unwrap();
        drop(database);
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "application_log::tests::panic_hook_child",
                "--nocapture",
            ])
            .env("LEGIO_PANIC_TEST_DIRECTORY", &directory)
            .env("LEGIO_PANIC_TEST_LOCKED", if locked { "1" } else { "0" })
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(exit) = child.try_wait().unwrap() {
                assert!(!exit.success());
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("panic hook deadlocked while the log writer was locked");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        directory
    }

    #[test]
    fn panic_hook_child() {
        let Some(directory) = std::env::var_os("LEGIO_PANIC_TEST_DIRECTORY") else {
            return;
        };
        let directory = PathBuf::from(directory);
        let log = initialize_at(Some(&directory), Some(directory.join("logs")));
        if std::env::var("LEGIO_PANIC_TEST_LOCKED").as_deref() == Ok("1") {
            let _locked = log.state.lock().unwrap();
            panic!("simulated locked-writer panic");
        }
        panic!("simulated panic token=privatecredential");
    }

    #[test]
    fn actual_panic_hook_obeys_saved_opt_in_and_persists_redacted_details() {
        let directory = panic_subprocess(true, false);
        let text = fs::read_to_string(directory.join("logs/app.jsonl")).unwrap();
        let record: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(record["event"], "panic");
        assert!(!text.contains("privatecredential"));
        assert!(
            record["stack"]
                .as_str()
                .unwrap()
                .contains("application_log.rs:")
        );
        fs::remove_dir_all(directory).unwrap();
        let directory = panic_subprocess(false, false);
        assert!(!directory.join("logs").exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn panic_hook_does_not_deadlock_when_writer_is_locked() {
        let directory = panic_subprocess(true, true);
        fs::remove_dir_all(directory).unwrap();
    }
}
