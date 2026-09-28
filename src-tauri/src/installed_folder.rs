use std::path::Path;
use std::process::Command;

use tauri::{AppHandle, Manager};

use crate::finalize_install;

// A file manager still running after this long has the folder open, so waiting
// any longer tells us nothing.
#[cfg(target_os = "linux")]
const OPEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
#[cfg(target_os = "linux")]
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);
#[cfg(target_os = "linux")]
const NO_ARGS: &[&str] = &[];

// Real file managers, tried in order when the registered default is unusable.
// Each takes the directory as a positional argument.
#[cfg(target_os = "linux")]
const FILE_MANAGERS: &[&str] = &[
    "nautilus", "nemo", "thunar", "dolphin", "pcmanfm", "caja", "xfe",
];

// xdg-mime reports the desktop id registered for directories. A terminal can
// be registered there, and a terminal is never what opening a folder means, so
// the registered default is not used in that case.
#[cfg(target_os = "linux")]
const TERMINAL_IDS: &[&str] = &[
    "terminal",
    "xterm",
    "console",
    "konsole",
    "kitty",
    "alacritty",
    "foot",
    "wezterm",
    "tilix",
    "guake",
    "terminator",
];

#[tauri::command]
pub fn open_installed_folder(app: AppHandle) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Could not locate app data: {error}"))?;
    let root = app
        .state::<crate::database::DatabaseState>()
        .database()?
        .storage_root(&data_dir)?;
    open_directory(&finalize_install::install_root(&root)?)
}

#[cfg(target_os = "linux")]
fn open_directory(path: &Path) -> Result<(), String> {
    let mut candidates: Vec<(&str, &[&str])> = Vec::new();
    if !default_handler_is_terminal() {
        candidates.push(("gio", &["open"]));
    }
    candidates.extend(FILE_MANAGERS.iter().map(|program| (*program, NO_ARGS)));

    let mut last = String::new();
    for (program, prefix) in candidates {
        match try_open(program, prefix, path) {
            Ok(()) => return Ok(()),
            Err(error) => last = error,
        }
    }
    Err(format!("Could not open the install folder: {last}"))
}

#[cfg(target_os = "linux")]
fn try_open(program: &str, prefix: &[&str], path: &Path) -> Result<(), String> {
    let mut request = Command::new(program);
    request.args(prefix).arg(path);
    let mut child = request
        .spawn()
        .map_err(|error| format!("{program} could not start: {error}"))?;
    let start = std::time::Instant::now();
    while start.elapsed() < OPEN_TIMEOUT {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("{program} exited with {status}")),
            Ok(None) => std::thread::sleep(POLL_INTERVAL),
            Err(error) => return Err(format!("{program} could not be waited for: {error}")),
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn default_handler_is_terminal() -> bool {
    let Ok(output) = Command::new("xdg-mime")
        .args(["query", "default", "inode/directory"])
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let handler = String::from_utf8_lossy(&output.stdout).to_lowercase();
    TERMINAL_IDS.iter().any(|name| handler.contains(name))
}

#[cfg(windows)]
fn open_directory(path: &Path) -> Result<(), String> {
    // explorer.exe reports a failure status even when it opens the folder, so
    // only a failed spawn is treated as an error.
    Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open the install folder: {error}"))
}

#[cfg(not(any(target_os = "linux", windows)))]
fn open_directory(_path: &Path) -> Result<(), String> {
    Err("Opening the install folder is unsupported on this platform".to_owned())
}
