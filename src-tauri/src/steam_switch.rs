use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::{
    database::{DatabaseState, Game},
    steam_local, steam_process, steam_vdf,
};

const SWITCH_TIMEOUT: Duration = Duration::from_secs(60);
static LAUNCH_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LaunchStatus {
    NoOverride,
    AlreadyMatches,
    Mismatch,
    Unknown,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchInspection {
    pub status: LaunchStatus,
    pub steam_running: bool,
    pub current_account_name: Option<String>,
    pub target_account_name: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SteamLaunchResult {
    pub game_id: String,
    pub steam_app_id: u32,
}

pub(crate) fn inspect(app: &AppHandle, game_id: &str) -> Result<LaunchInspection, String> {
    let game = app.state::<DatabaseState>().database()?.game(game_id)?;
    inspect_game(&game)
}

pub(crate) fn launch(
    app: &AppHandle,
    game_id: &str,
    confirm_account_switch: bool,
) -> Result<SteamLaunchResult, String> {
    let _guard = LAUNCH_LOCK
        .lock()
        .map_err(|_| "Steam launch is unavailable after an earlier task failed".to_owned())?;
    let game = app.state::<DatabaseState>().database()?.game(game_id)?;
    let (steam_app_id, uri) = launch_uri(&game)?;
    let Some(target_id) = game.steam_account_id.as_deref() else {
        open_game(app, uri)?;
        return Ok(SteamLaunchResult {
            game_id: game.id,
            steam_app_id,
        });
    };

    let steam_root = steam_local::find_steam_root_for_game(&game)?;
    saved_account_name(&steam_root, target_id)?;
    let steam_running = steam_process::is_running()?;
    let current_id = if steam_running {
        steam_process::active_account_id(&steam_root)?
    } else {
        None
    };
    if steam_running && current_id.as_deref() == Some(target_id) {
        open_game(app, uri)?;
        return Ok(SteamLaunchResult {
            game_id: game.id,
            steam_app_id,
        });
    }
    if steam_running && !confirm_account_switch {
        return Err(
            "Confirm closing Steam before switching to this game's saved account.".to_owned(),
        );
    }

    steam_process::steam_executable(&steam_root)?;
    prepare_file_patches(&steam_root, target_id)?;
    #[cfg(windows)]
    validate_windows_registry()?;
    if steam_running {
        steam_process::request_shutdown_and_wait(&steam_root, SWITCH_TIMEOUT)?;
    }
    if steam_process::is_running()? {
        return Err("Steam started again before its account settings could be changed".to_owned());
    }
    let files = prepare_file_patches(&steam_root, target_id)?;
    apply_file_patches(&files)?;
    steam_process::launch_and_wait_selected_account(&steam_root, target_id, SWITCH_TIMEOUT)?;
    open_game(app, uri)?;
    Ok(SteamLaunchResult {
        game_id: game.id,
        steam_app_id,
    })
}

fn inspect_game(game: &Game) -> Result<LaunchInspection, String> {
    let steam_running = steam_process::is_running()?;
    let Some(target_id) = game.steam_account_id.as_deref() else {
        return Ok(LaunchInspection {
            status: LaunchStatus::NoOverride,
            steam_running,
            current_account_name: None,
            target_account_name: None,
        });
    };
    let steam_root = steam_local::find_steam_root_for_game(game)?;
    let loginusers = read_bounded_file(&steam_root.join("config/loginusers.vdf"))?;
    let users = steam_vdf::parse_loginusers(&loginusers)
        .map_err(|error| format!("Could not inspect saved Steam accounts: {error}"))?;
    let target_account = users
        .iter()
        .find(|user| user.steam_id == target_id)
        .ok_or_else(|| "The selected Steam account is no longer saved locally".to_owned())?;
    let target_account_name = target_account.persona_name.clone();
    let current_id = if steam_running {
        steam_process::active_account_id(&steam_root)?
    } else {
        None
    };
    let current_account_name = current_id.as_ref().and_then(|id| {
        users
            .iter()
            .find(|user| &user.steam_id == id)
            .and_then(|user| user.persona_name.clone())
    });
    let status = match current_id.as_deref() {
        Some(id) if id == target_id => LaunchStatus::AlreadyMatches,
        Some(_) => LaunchStatus::Mismatch,
        None => LaunchStatus::Unknown,
    };
    Ok(LaunchInspection {
        status,
        steam_running,
        current_account_name,
        target_account_name,
    })
}

fn prepare_file_patches(steam_root: &Path, target_id: &str) -> Result<Vec<FilePatch>, String> {
    let loginusers_path = steam_root.join("config/loginusers.vdf");
    let config_path = steam_root.join("config/config.vdf");
    let loginusers = read_bounded_file(&loginusers_path)?;
    #[cfg(target_os = "linux")]
    let users = steam_vdf::parse_loginusers(&loginusers)
        .map_err(|error| format!("Could not update saved Steam accounts: {error}"))?;
    let patched_loginusers = steam_vdf::patch_account_selection(&loginusers, target_id)
        .map_err(|error| format!("Could not update saved Steam accounts: {error}"))?;
    let config = read_bounded_file(&config_path)?;
    let patched_config = steam_vdf::patch_keyvalues_scalar(
        &config,
        &[
            "InstallConfigStore",
            "Software",
            "Valve",
            "Steam",
            "AlwaysShowUserChooser",
        ],
        "0",
    )
    .map_err(|error| format!("Could not update Steam's account chooser setting: {error}"))?;

    let mut files = vec![FilePatch {
        path: loginusers_path,
        original: loginusers,
        updated: patched_loginusers,
    }];
    files.push(FilePatch {
        path: config_path,
        original: config,
        updated: patched_config,
    });
    #[cfg(target_os = "linux")]
    {
        let account_name = users
            .iter()
            .find(|user| user.steam_id == target_id)
            .and_then(|user| user.account_name.as_deref())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| "The selected Steam account has no saved account name".to_owned())?;
        let registry_path = steam_local::steam_registry_path(steam_root)?;
        let registry = read_bounded_file(&registry_path)?;
        let patched_registry = steam_vdf::patch_keyvalues_scalar(
            &registry,
            &[
                "Registry",
                "HKCU",
                "Software",
                "Valve",
                "Steam",
                "AutoLoginUser",
            ],
            account_name,
        )
        .map_err(|error| format!("Could not update Steam's selected account: {error}"))?;
        files.push(FilePatch {
            path: registry_path,
            original: registry,
            updated: patched_registry,
        });
    }

    Ok(files)
}

fn saved_account_name(steam_root: &Path, target_id: &str) -> Result<String, String> {
    let loginusers = read_bounded_file(&steam_root.join("config/loginusers.vdf"))?;
    let users = steam_vdf::parse_loginusers(&loginusers)
        .map_err(|error| format!("Could not inspect saved Steam accounts: {error}"))?;
    users
        .iter()
        .find(|user| user.steam_id == target_id)
        .and_then(|user| user.account_name.clone())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "The selected Steam account is no longer saved locally".to_owned())
}

struct FilePatch {
    path: PathBuf,
    original: Vec<u8>,
    updated: Vec<u8>,
}

fn read_bounded_file(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect Steam configuration: {error}"))?;
    if !metadata.is_file() || metadata.len() > steam_vdf::MAX_LOGINUSERS_BYTES as u64 {
        return Err("Steam configuration is not a regular file within the size limit".to_owned());
    }
    fs::read(path).map_err(|error| format!("Could not read Steam configuration: {error}"))
}

fn create_backup(path: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let permissions = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect Steam configuration permissions: {error}"))?
        .permissions();
    let backup = backup_path(path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup)
        .map_err(|error| format!("Could not create Steam configuration backup: {error}"))?;
    if let Err(error) = file
        .set_permissions(permissions)
        .and_then(|()| file.write_all(bytes))
        .and_then(|()| file.sync_all())
    {
        drop(file);
        return Err(match fs::remove_file(&backup) {
            Ok(()) => format!("Could not write Steam configuration backup: {error}"),
            Err(cleanup_error) => format!(
                "Could not write Steam configuration backup: {error}; partial backup cleanup failed: {cleanup_error}"
            ),
        });
    }
    #[cfg(unix)]
    fs::File::open(path.parent().unwrap_or_else(|| Path::new(".")))
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Could not sync Steam configuration backup: {error}"))?;
    Ok(backup)
}

fn backup_path(path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!("{name}.legio-{stamp}.bak"))
}

fn apply_file_patches(files: &[FilePatch]) -> Result<(), String> {
    if steam_process::is_running()? {
        return Err("Steam is running; account settings were left untouched".to_owned());
    }
    for file in files {
        create_backup(&file.path, &file.original)?;
    }
    for (index, file) in files.iter().enumerate() {
        if let Err(error) = write_atomic(&file.path, &file.updated) {
            let rollback_files = &files[..=index];
            let rollback = restore_files(rollback_files);
            return Err(match rollback {
                Ok(()) => error,
                Err(rollback_error) => {
                    format!("{error}; file rollback also failed: {rollback_error}")
                }
            });
        }
    }
    #[cfg(windows)]
    if let Err(error) = set_windows_auto_login_from_files(files) {
        let rollback = restore_files(files);
        return Err(match rollback {
            Ok(()) => error,
            Err(rollback_error) => format!("{error}; file rollback also failed: {rollback_error}"),
        });
    }
    Ok(())
}

fn restore_files(files: &[FilePatch]) -> Result<(), String> {
    let mut errors = Vec::new();
    for file in files.iter().rev() {
        if let Err(error) = write_atomic(&file.path, &file.original) {
            errors.push(error);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect Steam configuration: {error}"))?;
    if !metadata.is_file() {
        return Err("Steam configuration is not a regular file".to_owned());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "Steam configuration path has no parent".to_owned())?;
    let temporary = backup_path(path).with_extension("tmp");
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Could not create temporary Steam configuration: {error}"))?;
        file.set_permissions(metadata.permissions())
            .map_err(|error| {
                format!("Could not preserve Steam configuration permissions: {error}")
            })?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("Could not write temporary Steam configuration: {error}"))?;
        fs::rename(&temporary, path)
            .map_err(|error| format!("Could not replace Steam configuration: {error}"))?;
        #[cfg(unix)]
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("Could not sync Steam configuration directory: {error}"))?;
        Ok(())
    })();
    if let Err(error) = result {
        return Err(match fs::remove_file(&temporary) {
            Ok(()) => error,
            Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => error,
            Err(cleanup_error) => {
                format!("{error}; temporary file cleanup failed: {cleanup_error}")
            }
        });
    }
    Ok(())
}

fn launch_uri(game: &Game) -> Result<(u32, String), String> {
    let app_id = game
        .steam_app_id
        .filter(|id| *id > 0)
        .ok_or_else(|| "This game has no valid Steam App ID".to_owned())?;
    let path = game
        .steam_install_path
        .as_deref()
        .ok_or_else(|| "This game has no detected Steam installation".to_owned())?;
    if !Path::new(path).is_dir() {
        return Err(
            "Steam installation directory is missing. Rescan Steam games and retry.".to_owned(),
        );
    }
    Ok((app_id, format!("steam://run/{app_id}")))
}

fn open_game(app: &AppHandle, uri: String) -> Result<(), String> {
    app.opener()
        .open_url(uri, None::<&str>)
        .map_err(|error| format!("Could not ask Steam to launch the game: {error}"))?;
    Ok(())
}

#[cfg(windows)]
fn set_windows_auto_login(account_name: &str) -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    let key = current_user
        .open_subkey_with_flags("Software\\Valve\\Steam", KEY_READ | KEY_WRITE)
        .map_err(|error| format!("Could not update Steam's account selection: {error}"))?;
    let old_auto_login = match key.get_raw_value("AutoLoginUser") {
        Ok(value) => Some(value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("Could not read Steam's account selection: {error}")),
    };
    key.set_value("AutoLoginUser", &account_name)
        .map_err(|error| format!("Could not update Steam's account selection: {error}"))?;
    if let Err(error) = key.set_value("RememberPassword", &1u32) {
        let rollback = restore_registry_value(&key, "AutoLoginUser", old_auto_login.as_ref());
        return Err(match rollback {
            Ok(()) => format!("Could not update Steam's remembered-login setting: {error}"),
            Err(rollback_error) => format!(
                "Could not update Steam's remembered-login setting: {error}; registry rollback also failed: {rollback_error}"
            ),
        });
    }
    Ok(())
}

#[cfg(windows)]
fn validate_windows_registry() -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};

    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags("Software\\Valve\\Steam", KEY_WRITE)
        .map(|_| ())
        .map_err(|error| format!("Could not update Steam's account selection: {error}"))
}

#[cfg(windows)]
fn set_windows_auto_login_from_files(files: &[FilePatch]) -> Result<(), String> {
    let loginusers = files
        .iter()
        .find(|file| file.path.ends_with("loginusers.vdf"))
        .ok_or_else(|| "Steam loginusers.vdf patch is missing".to_owned())?;
    let users = steam_vdf::parse_loginusers(&loginusers.updated)
        .map_err(|error| format!("Could not update saved Steam accounts: {error}"))?;
    let account_name = users
        .iter()
        .find(|user| user.auto_login.as_deref() == Some("1"))
        .and_then(|user| user.account_name.as_deref())
        .ok_or_else(|| "Selected Steam account has no saved account name".to_owned())?;
    set_windows_auto_login(account_name)
}

#[cfg(windows)]
fn restore_registry_value(
    key: &winreg::RegKey,
    name: &str,
    value: Option<&winreg::RegValue>,
) -> Result<(), String> {
    if let Some(value) = value {
        key.set_raw_value(name, value)
            .map_err(|error| format!("Could not restore registry value {name}: {error}"))
    } else {
        match key.delete_value(name) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Could not remove registry value {name}: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_and_rollback_restore_the_original_file() {
        let root = std::env::temp_dir().join(format!(
            "legio-steam-switch-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("loginusers.vdf");
        let original = b"original";
        fs::write(&path, original).unwrap();
        let backup = create_backup(&path, original).unwrap();
        assert_eq!(fs::read(&backup).unwrap(), original);

        let patch = FilePatch {
            path: path.clone(),
            original: original.to_vec(),
            updated: b"selected account".to_vec(),
        };
        write_atomic(&path, &patch.updated).unwrap();
        restore_files(&[patch]).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);

        fs::remove_dir_all(root).unwrap();
    }
}
