use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;

const MAX_BYTES: u64 = 512 * 1024;

#[derive(Clone)]
pub(crate) struct LaunchJournal {
    directory: Result<PathBuf, String>,
    lock: Arc<Mutex<()>>,
}

#[derive(Serialize)]
struct Event<'a> {
    timestamp_ms: u128,
    game_id: &'a str,
    stage: &'a str,
    failed: bool,
    exit_code: Option<i32>,
}

impl LaunchJournal {
    pub(crate) fn new(directory: Result<PathBuf, String>) -> Self {
        Self {
            directory,
            lock: Arc::default(),
        }
    }

    pub(crate) fn record(&self, game_id: &str, stage: &str, failed: bool, exit_code: Option<i32>) {
        let result = (|| {
            let _lock = self
                .lock
                .lock()
                .map_err(|_| "Launch log lock is unavailable".to_owned())?;
            let directory = self.directory.as_ref().map_err(Clone::clone)?;
            append(
                directory,
                &Event {
                    timestamp_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis(),
                    game_id,
                    stage,
                    failed,
                    exit_code,
                },
            )
            .map_err(|error| format!("Could not write game launch journal: {error}"))
        })();
        if let Err(error) = result {
            crate::application_log::failure("write_launch_journal", error);
        }
    }
}

fn append(directory: &Path, event: &Event<'_>) -> std::io::Result<()> {
    fs::create_dir_all(directory)?;
    let path = directory.join("game-launches.jsonl");
    let previous = directory.join("game-launches.previous.jsonl");
    let mut line = serde_json::to_vec(event)?;
    line.push(b'\n');
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(std::io::Error::other(
                "Game launch log is not a regular file",
            ));
        }
        if metadata.len().saturating_add(line.len() as u64) > MAX_BYTES {
            match fs::remove_file(&previous) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            fs::rename(&path, previous)?;
        }
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(&line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_records_stages_and_exit_codes_with_bounded_retention() {
        let directory =
            std::env::temp_dir().join(format!("legio-launch-journal-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let current = directory.join("game-launches.jsonl");
        let previous = directory.join("game-launches.previous.jsonl");
        fs::write(&previous, b"obsolete generation").unwrap();
        for (stage, exit_code) in [("runner_exit", 127), ("second_exit", 126)] {
            let event = Event {
                timestamp_ms: 1,
                game_id: "game",
                stage,
                failed: true,
                exit_code: Some(exit_code),
            };
            let mut line = serde_json::to_vec(&event).unwrap();
            line.push(b'\n');
            let seed = line.repeat(MAX_BYTES as usize / line.len());
            fs::write(&current, &seed).unwrap();
            append(&directory, &event).unwrap();
            assert_eq!(fs::read(&previous).unwrap(), seed);
            assert_eq!(fs::read(&current).unwrap(), line);
            let record: serde_json::Value =
                serde_json::from_slice(&fs::read(&current).unwrap()).unwrap();
            assert_eq!(record["exit_code"], exit_code);
            assert_eq!(record["stage"], stage);
            for path in [&current, &previous] {
                assert!(fs::metadata(path).unwrap().len() <= MAX_BYTES);
            }
        }
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 2);
        fs::remove_dir_all(directory).unwrap();
    }
}
