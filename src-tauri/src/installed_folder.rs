use std::path::Path;
use std::process::Command;

use serde::Serialize;
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledFolderInfo {
    directory: String,
    free_bytes: u64,
    total_bytes: u64,
}

#[tauri::command]
pub fn get_installed_folder_info(app: AppHandle) -> Result<InstalledFolderInfo, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Could not locate app data: {error}"))?;
    let state = app.state::<crate::database::DatabaseState>();
    let database = state.database()?;
    let root = database.storage_root(&data_dir)?;
    let directory = finalize_install::install_root(database, &root)?;
    let (free_bytes, total_bytes) = disk_space(&directory)?;
    Ok(InstalledFolderInfo {
        directory: directory.to_string_lossy().into_owned(),
        free_bytes,
        total_bytes,
    })
}

#[cfg(target_os = "linux")]
fn disk_space(path: &Path) -> Result<(u64, u64), String> {
    use std::os::unix::ffi::OsStrExt;

    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| "Install directory path contains an interior NUL byte".to_owned())?;
    // SAFETY: statvfs contains only integer fields; zero is valid for each field.
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: c_path is NUL-terminated and stats remains writable for this call.
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stats) } != 0 {
        return Err(format!(
            "Could not read free disk space: {}",
            std::io::Error::last_os_error()
        ));
    }
    let block = block_count(stats.f_frsize);
    Ok((
        block_count(stats.f_bavail).saturating_mul(block),
        block_count(stats.f_blocks).saturating_mul(block),
    ))
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
fn block_count(value: libc::c_ulong) -> u64 {
    value
}

#[cfg(all(target_os = "linux", not(target_pointer_width = "64")))]
fn block_count(value: libc::c_ulong) -> u64 {
    u64::from(value)
}

#[cfg(windows)]
fn disk_space(path: &Path) -> Result<(u64, u64), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let directory: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0_u64;
    let mut total = 0_u64;
    let mut total_free = 0_u64;
    // SAFETY: directory is NUL-terminated and all output pointers live through the call.
    let result = unsafe {
        GetDiskFreeSpaceExW(
            directory.as_ptr(),
            &mut available,
            &mut total,
            &mut total_free,
        )
    };
    if result == 0 {
        return Err(format!(
            "Could not read free disk space: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok((available, total))
}

#[tauri::command]
pub fn open_installed_folder(app: AppHandle) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Could not locate app data: {error}"))?;
    let state = app.state::<crate::database::DatabaseState>();
    let database = state.database()?;
    let root = database.storage_root(&data_dir)?;
    open_directory(&finalize_install::install_root(database, &root)?)
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
