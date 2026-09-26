use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use serde::Serialize;
use uuid::Uuid;

use crate::database::{AppliedCompatibilityOptions, EffectiveCompatibilityConfig};
use crate::runner_discovery::{InstalledRunner, RunnerKind};

const MAX_STREAM_BYTES: u64 = 2 * 1024 * 1024;
const MAX_PROTON_LOG_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct CompatibilityLogState {
    directory: PathBuf,
    output_error: Arc<Mutex<Option<String>>>,
    truncated: Arc<AtomicBool>,
}

impl CompatibilityLogState {
    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn output_error(&self) -> Option<String> {
        self.output_error
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub(crate) fn truncated(&self) -> bool {
        self.truncated.load(Ordering::Relaxed)
    }

    fn report_error(&self, error: &io::Error, stream: &str) {
        let mut stored = self
            .output_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        stored.get_or_insert_with(|| {
            format!(
                "Could not write compatibility {stream} output ({:?})",
                error.kind()
            )
        });
    }
}

pub(crate) struct CompatibilityLog {
    state: CompatibilityLogState,
    stdout: Option<File>,
    stderr: Option<File>,
    exit_code: File,
    proton_log: Option<PathBuf>,
    readers: Vec<JoinHandle<()>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchReport<'a> {
    schema_version: u8,
    game_id: String,
    runner: &'a str,
    runner_version: &'a str,
    prefix_path: &'a Path,
    applied_options: &'a AppliedCompatibilityOptions,
    configured_environment_variables: Vec<&'a str>,
    debug_environment: Vec<(&'static str, String)>,
    stdout_log: &'static str,
    stderr_log: &'static str,
    runner_exit_code_log: &'static str,
}

impl CompatibilityLog {
    pub(crate) fn create(
        log_root: &Path,
        game_id: &str,
        prefix_path: &Path,
        runner: &InstalledRunner,
        config: &EffectiveCompatibilityConfig,
        applied_options: &AppliedCompatibilityOptions,
    ) -> Result<Self, String> {
        let game_id = Uuid::parse_str(game_id)
            .map_err(|_| "Could not create compatibility diagnostics: invalid game ID".to_owned())?
            .to_string();
        let root = resolve_directory(log_root)?;
        let directory = root.join(&game_id);
        ensure_log_directory(&directory)?;
        let directory = fs::canonicalize(&directory)
            .map_err(|error| format!("Could not resolve game log directory: {error}"))?;
        if !directory.starts_with(&root) {
            return Err(
                "Compatibility log directory escapes the application log folder".to_owned(),
            );
        }

        let stdout = open_log_file(&directory.join("stdout.log"))?;
        let stderr = open_log_file(&directory.join("stderr.log"))?;
        let exit_code = open_log_file(&directory.join("runner-exit-code.txt"))?;
        let manifest = open_log_file(&directory.join("launch.json"))?;
        let proton_log = matches!(runner.kind, RunnerKind::Proton | RunnerKind::GeProton)
            .then(|| directory.join("steam-480.log"));
        let report = LaunchReport {
            schema_version: 1,
            game_id,
            runner: &runner.name,
            runner_version: &runner.version,
            prefix_path,
            applied_options,
            configured_environment_variables: config
                .environment
                .keys()
                .map(String::as_str)
                .collect(),
            debug_environment: debug_environment(runner, config, &directory),
            stdout_log: "stdout.log",
            stderr_log: "stderr.log",
            runner_exit_code_log: "runner-exit-code.txt",
        };
        serde_json::to_writer_pretty(manifest, &report)
            .map_err(|error| format!("Could not write compatibility launch report: {error}"))?;

        Ok(Self {
            state: CompatibilityLogState {
                directory,
                output_error: Arc::new(Mutex::new(None)),
                truncated: Arc::new(AtomicBool::new(false)),
            },
            stdout: Some(stdout),
            stderr: Some(stderr),
            exit_code,
            proton_log,
            readers: Vec::new(),
        })
    }

    pub(crate) fn state(&self) -> CompatibilityLogState {
        self.state.clone()
    }

    pub(crate) fn capture_output(&mut self, child: &mut Child) -> Result<(), String> {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Compatibility runner stdout was not piped".to_owned())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Compatibility runner stderr was not piped".to_owned())?;
        self.readers.push(spawn_reader(
            stdout,
            self.stdout
                .take()
                .ok_or_else(|| "Compatibility stdout log is unavailable".to_owned())?,
            self.state.clone(),
            "stdout",
        )?);
        self.readers.push(spawn_reader(
            stderr,
            self.stderr
                .take()
                .ok_or_else(|| "Compatibility stderr log is unavailable".to_owned())?,
            self.state.clone(),
            "stderr",
        )?);
        Ok(())
    }

    pub(crate) fn record_exit_code(&mut self, code: i32) {
        if let Err(error) = self
            .exit_code
            .set_len(0)
            .and_then(|()| self.exit_code.seek(SeekFrom::Start(0)).map(|_| ()))
            .and_then(|()| writeln!(self.exit_code, "{code}"))
            .and_then(|()| self.exit_code.flush())
        {
            self.state.report_error(&error, "runner exit code");
        }
    }

    pub(crate) fn cap_runner_log(&self) {
        let Some(path) = &self.proton_log else {
            return;
        };
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
            Err(error) => {
                self.state.report_error(&error, "Proton");
                return;
            }
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                if metadata.len() <= MAX_PROTON_LOG_BYTES {
                    return;
                }
            }
            Ok(_) => {
                self.state.report_error(
                    &io::Error::new(io::ErrorKind::InvalidData, "log is not a regular file"),
                    "Proton",
                );
                return;
            }
        }
        match OpenOptions::new()
            .write(true)
            .open(path)
            .and_then(|file| file.set_len(MAX_PROTON_LOG_BYTES))
        {
            Ok(()) => self.state.truncated.store(true, Ordering::Relaxed),
            Err(error) => self.state.report_error(&error, "Proton"),
        }
    }

    #[cfg(test)]
    fn wait_for_readers(&mut self) {
        for reader in self.readers.drain(..) {
            reader.join().expect("output reader thread should finish");
        }
    }
}

pub(crate) fn directory(log_root: &Path) -> Result<PathBuf, String> {
    resolve_directory(&log_root.join("compatibility"))
}

fn resolve_directory(path: &Path) -> Result<PathBuf, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err("Compatibility log directory is not a regular directory".to_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(|error| {
                format!("Could not create compatibility log directory: {error}")
            })?;
        }
        Err(error) => {
            return Err(format!(
                "Could not inspect compatibility log directory: {error}"
            ));
        }
    }
    let root = fs::canonicalize(path)
        .map_err(|error| format!("Could not resolve compatibility log directory: {error}"))?;
    ensure_log_directory(&root)?;
    Ok(root)
}

fn ensure_log_directory(directory: &Path) -> Result<(), String> {
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err("Compatibility game log path is not a regular directory".to_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(directory).map_err(|error| {
                format!("Could not create compatibility game log directory: {error}")
            })?;
        }
        Err(error) => {
            return Err(format!(
                "Could not inspect compatibility game log directory: {error}"
            ));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).map_err(|error| {
            format!("Could not restrict compatibility log permissions: {error}")
        })?;
    }
    Ok(())
}

fn open_log_file(path: &Path) -> Result<File, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err("Compatibility log path is not a regular file".to_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not inspect compatibility log file: {error}")),
    }
    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(path)
        .map_err(|error| format!("Could not create compatibility log file: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| {
                format!("Could not restrict compatibility log permissions: {error}")
            })?;
    }
    Ok(file)
}

fn debug_environment(
    runner: &InstalledRunner,
    config: &EffectiveCompatibilityConfig,
    directory: &Path,
) -> Vec<(&'static str, String)> {
    match runner.kind {
        RunnerKind::Proton | RunnerKind::GeProton => {
            let mut variables = vec![
                ("PROTON_LOG", "1".to_owned()),
                ("PROTON_LOG_DIR", directory.to_string_lossy().into_owned()),
                ("SteamGameId", "480".to_owned()),
            ];
            if config.environment.contains_key("WINEDEBUG") {
                variables.push(("WINEDEBUG", "configured_by_user".to_owned()));
            }
            variables
        }
        RunnerKind::Wine => {
            let value = if config.environment.contains_key("WINEDEBUG") {
                "configured_by_user".to_owned()
            } else {
                "+timestamp,+pid,+tid,+seh".to_owned()
            };
            vec![("WINEDEBUG", value)]
        }
    }
}

fn spawn_reader<R: Read + Send + 'static>(
    mut source: R,
    mut target: File,
    state: CompatibilityLogState,
    stream: &'static str,
) -> Result<JoinHandle<()>, String> {
    thread::Builder::new()
        .name(format!("legio-compat-{stream}"))
        .spawn(move || {
            let mut saved = 0_u64;
            let mut buffer = [0_u8; 8192];
            loop {
                let read = match source.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => read,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        state.report_error(&error, stream);
                        break;
                    }
                };
                let writable = (MAX_STREAM_BYTES - saved).min(read as u64) as usize;
                if writable < read {
                    state.truncated.store(true, Ordering::Relaxed);
                }
                if writable > 0 {
                    if let Err(error) = target.write_all(&buffer[..writable]) {
                        state.report_error(&error, stream);
                        loop {
                            match source.read(&mut buffer) {
                                Ok(0) => break,
                                Ok(_) => {}
                                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                                Err(error) => {
                                    state.report_error(&error, stream);
                                    break;
                                }
                            }
                        }
                        break;
                    }
                    saved += writable as u64;
                }
            }
            if let Err(error) = target.flush() {
                state.report_error(&error, stream);
            }
        })
        .map_err(|error| format!("Could not start compatibility {stream} log reader: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{GraphicsRenderer, SteamOverlayMode, SteamRuntimeMode, WaylandMode};
    use std::process::Stdio;

    fn test_log(root: &Path, config: &EffectiveCompatibilityConfig) -> CompatibilityLog {
        let runner = InstalledRunner {
            kind: RunnerKind::GeProton,
            name: "GE-Proton-test".to_owned(),
            version: "test".to_owned(),
            path: "/unused/GE-Proton-test".to_owned(),
        };
        let options = AppliedCompatibilityOptions {
            runner: runner.name.clone(),
            version: runner.version.clone(),
            steam_runtime: SteamRuntimeMode::RunnerDefault,
            steam_overlay: SteamOverlayMode::RunnerDefault,
            graphics_renderer: GraphicsRenderer::RunnerDefault,
            wayland: WaylandMode::RunnerDefault,
            debug_logging: true,
        };
        CompatibilityLog::create(
            root,
            "00000000-0000-0000-0000-000000000001",
            Path::new("/tmp/prefix"),
            &runner,
            config,
            &options,
        )
        .unwrap()
    }

    #[test]
    fn captures_runner_output_and_records_exit_code() {
        let root = std::env::temp_dir().join(format!("legio-compat-log-{}", Uuid::new_v4()));
        let config = EffectiveCompatibilityConfig::default();
        let mut logs = test_log(&root, &config);
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "printf runner-out; printf runner-err >&2"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        logs.capture_output(&mut child).unwrap();
        let status = child.wait().unwrap();
        logs.record_exit_code(status.code().unwrap());
        logs.wait_for_readers();
        let directory = logs.state().directory().to_path_buf();
        assert_eq!(
            fs::read(directory.join("stdout.log")).unwrap(),
            b"runner-out"
        );
        assert_eq!(
            fs::read(directory.join("stderr.log")).unwrap(),
            b"runner-err"
        );
        assert_eq!(
            fs::read_to_string(directory.join("runner-exit-code.txt")).unwrap(),
            "0\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn caps_captured_output_and_keeps_draining() {
        let root = std::env::temp_dir().join(format!("legio-compat-cap-{}", Uuid::new_v4()));
        let config = EffectiveCompatibilityConfig::default();
        let logs = test_log(&root, &config);
        let directory = logs.state().directory().to_path_buf();
        let target = File::create(directory.join("bounded.log")).unwrap();
        let state = logs.state();
        let reader = spawn_reader(
            io::Cursor::new(vec![b'x'; MAX_STREAM_BYTES as usize + 1]),
            target,
            state.clone(),
            "stdout",
        )
        .unwrap();
        reader.join().unwrap();
        assert_eq!(
            fs::metadata(directory.join("bounded.log")).unwrap().len(),
            MAX_STREAM_BYTES
        );
        assert!(state.truncated());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn caps_proton_log_growth() {
        let root = std::env::temp_dir().join(format!("legio-proton-cap-{}", Uuid::new_v4()));
        let config = EffectiveCompatibilityConfig::default();
        let logs = test_log(&root, &config);
        let path = logs.state().directory().join("steam-480.log");
        fs::write(&path, vec![b'x'; MAX_PROTON_LOG_BYTES as usize + 1]).unwrap();
        logs.cap_runner_log();
        assert_eq!(fs::metadata(path).unwrap().len(), MAX_PROTON_LOG_BYTES);
        assert!(logs.state().truncated());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn launch_report_omits_custom_environment_values() {
        let root = std::env::temp_dir().join(format!("legio-compat-report-{}", Uuid::new_v4()));
        let mut config = EffectiveCompatibilityConfig::default();
        config.environment.insert(
            "ACCESS_TOKEN".to_owned(),
            "do-not-write-this-value".to_owned(),
        );
        config.environment.insert(
            "WINEDEBUG".to_owned(),
            "also-do-not-write-this-value".to_owned(),
        );
        let logs = test_log(&root, &config);
        let report = fs::read_to_string(logs.state().directory().join("launch.json")).unwrap();
        assert!(report.contains("ACCESS_TOKEN"));
        assert!(report.contains("configured_by_user"));
        assert!(!report.contains("do-not-write-this-value"));
        assert!(!report.contains("also-do-not-write-this-value"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_invalid_game_id_and_unwritable_log_roots() {
        let root = std::env::temp_dir().join(format!("legio-compat-invalid-{}", Uuid::new_v4()));
        let config = EffectiveCompatibilityConfig::default();
        let runner = InstalledRunner {
            kind: RunnerKind::Wine,
            name: "Wine".to_owned(),
            version: "test".to_owned(),
            path: "/unused/wine".to_owned(),
        };
        let options = AppliedCompatibilityOptions {
            runner: "Wine".to_owned(),
            version: "test".to_owned(),
            steam_runtime: SteamRuntimeMode::RunnerDefault,
            steam_overlay: SteamOverlayMode::RunnerDefault,
            graphics_renderer: GraphicsRenderer::RunnerDefault,
            wayland: WaylandMode::RunnerDefault,
            debug_logging: true,
        };
        assert!(
            CompatibilityLog::create(
                &root,
                "../../outside",
                Path::new("/tmp/prefix"),
                &runner,
                &config,
                &options,
            )
            .is_err()
        );

        fs::write(&root, b"file instead of directory").unwrap();
        let error = match CompatibilityLog::create(
            &root,
            "00000000-0000-0000-0000-000000000001",
            Path::new("/tmp/prefix"),
            &runner,
            &config,
            &options,
        ) {
            Ok(_) => panic!("file log root should fail"),
            Err(error) => error,
        };
        assert!(error.contains("Compatibility log directory is not a regular directory"));
        fs::remove_file(root).unwrap();
    }
}
