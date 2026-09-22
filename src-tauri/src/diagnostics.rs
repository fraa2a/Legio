use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use crate::network::NetworkError;

const MAX_LOG_BYTES: u64 = 256 * 1024;
const QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    HydraSearch,
    SteamDetails,
    SteamConnectivity,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Success,
    Timeout,
    Transport,
    Http,
    TooLarge,
    Cancelled,
}

#[derive(Serialize)]
struct Record {
    timestamp_ms: u128,
    category: &'static str,
    operation: Operation,
    outcome: Outcome,
    elapsed_ms: u128,
    status: Option<u16>,
    response_bytes: Option<usize>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStatus {
    directory: Option<PathBuf>,
    last_error: Option<String>,
    dropped_records: u64,
    pub(crate) pending_records: usize,
}

#[derive(Clone)]
pub struct Diagnostics {
    sender: mpsc::SyncSender<Record>,
    status: Arc<Mutex<LogStatus>>,
}

impl Diagnostics {
    pub fn new(directory: Result<PathBuf, String>) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Record>(QUEUE_CAPACITY);
        let status = Arc::new(Mutex::new(LogStatus {
            directory: directory.as_ref().ok().cloned(),
            last_error: directory
                .as_ref()
                .err()
                .map(|_| "Could not locate the application log directory.".to_owned()),
            dropped_records: 0,
            pending_records: 0,
        }));
        if let Ok(directory) = directory {
            let worker_status = Arc::clone(&status);
            let spawn = std::thread::Builder::new().name("local-diagnostics".to_owned()).spawn(move || {
                if let Err(error) = prepare(&directory) {
                    let mut status = worker_status.lock().unwrap_or_else(|error| error.into_inner());
                    status.last_error = Some(format!("Could not initialize local logs ({:?}). Check directory permissions and free disk space.", error.kind()));
                }
                for record in receiver {
                    let result = append(&directory, &record);
                    let mut status = worker_status.lock().unwrap_or_else(|error| error.into_inner());
                    status.pending_records -= 1;
                    match result {
                        Ok(()) => status.last_error = None,
                        Err(error) => {
                            status.dropped_records += 1;
                            status.last_error = Some(format!("Could not write local logs ({:?}). Check directory permissions and free disk space.", error.kind()));
                        }
                    }
                }
            });
            if spawn.is_err() {
                status
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .last_error = Some(
                    "Could not start the local log writer. Restart Legio to retry.".to_owned(),
                );
            }
        }
        Self { sender, status }
    }

    pub fn status(&self) -> LogStatus {
        self.status
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn request(&self, operation: Operation) -> RequestLog<'_> {
        RequestLog {
            diagnostics: self,
            operation,
            started: Instant::now(),
            completed: false,
            status: None,
            response_bytes: None,
        }
    }

    fn record(&self, record: Record) {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        status.pending_records += 1;
        if let Err(error) = self.sender.try_send(record) {
            status.pending_records -= 1;
            status.dropped_records += 1;
            match error {
                mpsc::TrySendError::Full(_) => {
                    status.last_error =
                        Some("Local log queue is full; some diagnostics were dropped.".to_owned())
                }
                mpsc::TrySendError::Disconnected(_) => {
                    status.last_error.get_or_insert_with(|| {
                        "Local log writer is unavailable. Restart Legio to retry.".to_owned()
                    });
                }
            }
        }
    }
}

pub struct RequestLog<'a> {
    diagnostics: &'a Diagnostics,
    operation: Operation,
    started: Instant,
    completed: bool,
    status: Option<u16>,
    response_bytes: Option<usize>,
}

impl RequestLog<'_> {
    pub fn set_status(&mut self, status: u16) {
        self.status = Some(status);
    }

    pub fn set_response_bytes(&mut self, bytes: usize) {
        self.response_bytes = Some(bytes);
    }

    pub fn finish<T>(&mut self, result: &Result<T, NetworkError>) {
        let outcome = match result {
            Ok(_) => Outcome::Success,
            Err(NetworkError::Timeout) => Outcome::Timeout,
            Err(NetworkError::Transport { .. }) => Outcome::Transport,
            Err(NetworkError::Http { .. }) => Outcome::Http,
            Err(NetworkError::TooLarge) => Outcome::TooLarge,
        };
        self.emit(outcome);
        self.completed = true;
    }

    fn emit(&self, outcome: Outcome) {
        self.diagnostics.record(Record {
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            category: "network",
            operation: self.operation,
            outcome,
            elapsed_ms: self.started.elapsed().as_millis(),
            status: self.status,
            response_bytes: self.response_bytes,
        });
    }
}

impl Drop for RequestLog<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.emit(Outcome::Cancelled);
        }
    }
}

fn prepare(directory: &Path) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    for name in ["network.jsonl", "network.previous.jsonl"] {
        let path = directory.join(name);
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() > MAX_LOG_BYTES => fs::remove_file(path)?,
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    open_log(&directory.join("network.jsonl"))?;
    Ok(())
}

fn open_log(path: &Path) -> io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn append(directory: &Path, record: &Record) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let path = directory.join("network.jsonl");
    let mut line = serde_json::to_vec(record)?;
    line.push(b'\n');
    let mut file = open_log(&path)?;
    let length = file.metadata()?.len();
    if length > MAX_LOG_BYTES {
        drop(file);
        fs::remove_file(&path)?;
        file = open_log(&path)?;
    } else if length.saturating_add(line.len() as u64) > MAX_LOG_BYTES {
        drop(file);
        let previous = directory.join("network.previous.jsonl");
        match fs::remove_file(&previous) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(&path, previous)?;
        file = open_log(&path)?;
    }
    file.write_all(&line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_saturation_reports_dropped_record_without_waiting() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let diagnostics = Diagnostics {
            sender,
            status: Arc::new(Mutex::new(LogStatus {
                directory: None,
                last_error: None,
                dropped_records: 0,
                pending_records: 0,
            })),
        };
        drop(diagnostics.request(Operation::HydraSearch));
        drop(diagnostics.request(Operation::HydraSearch));
        let status = diagnostics.status();
        assert_eq!(status.pending_records, 1);
        assert_eq!(status.dropped_records, 1);
        assert!(status.last_error.unwrap().contains("queue is full"));
    }

    #[test]
    fn missing_log_directory_keeps_initial_failure_visible() {
        let diagnostics = Diagnostics::new(Err("private path".to_owned()));
        drop(diagnostics.request(Operation::SteamConnectivity));
        let status = diagnostics.status();
        assert!(status.directory.is_none());
        assert_eq!(status.dropped_records, 1);
        assert_eq!(
            status.last_error.as_deref(),
            Some("Could not locate the application log directory.")
        );
    }

    #[test]
    fn rotates_complete_records_with_bounded_retention() {
        let directory = std::env::temp_dir().join(format!("legio-logs-{}", uuid::Uuid::new_v4()));
        let record = Record {
            timestamp_ms: 1,
            category: "network",
            operation: Operation::HydraSearch,
            outcome: Outcome::Timeout,
            elapsed_ms: 10,
            status: None,
            response_bytes: None,
        };
        for _ in 0..4000 {
            append(&directory, &record).unwrap();
        }
        let entries = fs::read_dir(&directory)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 2);
        for entry in entries {
            assert!(entry.metadata().unwrap().len() <= MAX_LOG_BYTES);
            for line in fs::read_to_string(entry.path()).unwrap().lines() {
                let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
                assert_eq!(parsed["outcome"], "timeout");
            }
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn removes_preexisting_oversized_logs() {
        let directory = std::env::temp_dir().join(format!("legio-logs-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        for name in ["network.jsonl", "network.previous.jsonl"] {
            fs::write(directory.join(name), vec![b'x'; MAX_LOG_BYTES as usize + 1]).unwrap();
        }
        prepare(&directory).unwrap();
        assert_eq!(
            fs::metadata(directory.join("network.jsonl")).unwrap().len(),
            0
        );
        assert!(!directory.join("network.previous.jsonl").exists());
        fs::write(
            directory.join("network.jsonl"),
            vec![b'x'; MAX_LOG_BYTES as usize + 1],
        )
        .unwrap();
        let record = Record {
            timestamp_ms: 1,
            category: "network",
            operation: Operation::HydraSearch,
            outcome: Outcome::Success,
            elapsed_ms: 1,
            status: Some(200),
            response_bytes: Some(2),
        };
        append(&directory, &record).unwrap();
        assert!(fs::metadata(directory.join("network.jsonl")).unwrap().len() < MAX_LOG_BYTES);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reports_unwritable_logs_without_panicking_or_blocking_requests() {
        let path = std::env::temp_dir().join(format!("legio-logs-{}", uuid::Uuid::new_v4()));
        fs::write(&path, b"not a directory").unwrap();
        let diagnostics = Diagnostics::new(Ok(path.clone()));
        drop(diagnostics.request(Operation::SteamDetails));
        wait_for_logs(&diagnostics);
        let status = diagnostics.status();
        assert_eq!(status.dropped_records, 1);
        assert!(
            status
                .last_error
                .unwrap()
                .contains("Could not write local logs")
        );
        drop(diagnostics);
        fs::remove_file(path).unwrap();
    }

    pub(crate) fn wait_for_logs(diagnostics: &Diagnostics) {
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        while diagnostics.status().pending_records != 0 {
            assert!(Instant::now() < deadline, "log writer did not finish");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
