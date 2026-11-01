//! Claiming and releasing the `ror2mm` scheme for Legio. Releasing removes
//! only what Legio wrote: another manager's registration, including a
//! machine-wide one, is never touched.

use serde::Serialize;
use tauri::AppHandle;

use super::mod_links::SCHEME;
use crate::application_log;

/// Who opens `ror2mm://` links on this system.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum LinkHandler {
    Legio,
    Other,
    Nobody,
}

/// Development builds must not take the scheme from the developer's own manager.
fn can_register() -> bool {
    !tauri::is_dev() && !cfg!(debug_assertions)
}

fn classify(registered: Option<&str>, own: &str) -> LinkHandler {
    match registered {
        None => LinkHandler::Nobody,
        Some(value) if same_handler(value, own) => LinkHandler::Legio,
        Some(_) => LinkHandler::Other,
    }
}

// Windows commands contain file paths, which are case-insensitive there.
fn same_handler(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

/// Claims or releases the scheme. Releasing a scheme that another program
/// handles is not an error: it leaves that registration alone.
pub(crate) fn set_registered(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if !can_register() {
        return Ok(());
    }
    if enabled {
        let before = match handler(app) {
            Ok(handler) => Some(handler),
            Err(error) => {
                application_log::failure("check_mod_links", error);
                None
            }
        };
        register(app)?;
        if before != Some(LinkHandler::Legio) {
            application_log::state_change("mod_links_registered", "Legio opens ror2mm links");
        }
        return Ok(());
    }
    if release(app)? {
        application_log::state_change("mod_links_released", "Legio no longer opens ror2mm links");
    } else {
        application_log::state_change(
            "mod_links_release_skipped",
            "ror2mm links are not registered to Legio, left unchanged",
        );
    }
    Ok(())
}

/// Who opens `ror2mm://` links right now.
pub(crate) fn handler(app: &AppHandle) -> Result<LinkHandler, String> {
    platform::handler(app)
}

fn register(app: &AppHandle) -> Result<(), String> {
    platform::register(app)
}

/// Removes Legio's registration. Returns whether anything was removed.
fn release(app: &AppHandle) -> Result<bool, String> {
    platform::release(app)
}

#[tauri::command]
pub async fn get_mod_link_handler(app: AppHandle) -> Result<LinkHandler, String> {
    tauri::async_runtime::spawn_blocking(move || handler(&app))
        .await
        .map_err(|error| format!("Mod link check failed: {error}"))?
}

#[cfg(windows)]
mod platform {
    use tauri::AppHandle;
    use tauri_plugin_deep_link::DeepLinkExt;
    use winreg::{
        RegKey,
        enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER},
    };

    use super::{LinkHandler, SCHEME, classify};

    const USER_CLASSES: &str = "Software\\Classes\\";

    /// The command the plugin writes when it registers the scheme.
    fn own_command() -> Result<String, String> {
        let exe = tauri::utils::platform::current_exe()
            .map_err(|error| format!("Could not find Legio executable: {error}"))?;
        Ok(format!("\"{}\" \"%1\"", dunce::simplified(&exe).display()))
    }

    fn read_command(root: &RegKey, prefix: &str) -> Result<Option<String>, String> {
        let path = format!("{prefix}{SCHEME}\\shell\\open\\command");
        let key = match root.open_subkey(path) {
            Ok(key) => key,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("Could not read Thunderstore links: {error}")),
        };
        match key.get_value::<String, _>("") {
            Ok(command) => Ok(Some(command)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("Could not read Thunderstore links: {error}")),
        }
    }

    pub(super) fn handler(_app: &AppHandle) -> Result<LinkHandler, String> {
        let merged = read_command(&RegKey::predef(HKEY_CLASSES_ROOT), "")?;
        Ok(classify(merged.as_deref(), &own_command()?))
    }

    pub(super) fn register(app: &AppHandle) -> Result<(), String> {
        app.deep_link()
            .register(SCHEME)
            .map_err(|error| format!("Could not register Thunderstore links: {error}"))
    }

    // The plugin's own unregister also deletes the machine-wide key.
    pub(super) fn release(_app: &AppHandle) -> Result<bool, String> {
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let registered = read_command(&user, USER_CLASSES)?;
        if classify(registered.as_deref(), &own_command()?) != LinkHandler::Legio {
            return Ok(false);
        }
        match user.delete_subkey_all(format!("{USER_CLASSES}{SCHEME}")) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!("Could not release Thunderstore links: {error}")),
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::process::Command;

    use tauri::{AppHandle, Manager};
    use tauri_plugin_deep_link::DeepLinkExt;

    use super::{LinkHandler, SCHEME, classify};

    const AUR_DESKTOP_FILE: &str = "legio-launcher.desktop";

    fn mime_type() -> String {
        format!("x-scheme-handler/{SCHEME}")
    }

    /// The desktop file that opens Legio: the package wrapper on the AUR,
    /// else the one the plugin writes.
    fn own_desktop_file() -> Result<String, String> {
        if crate::commands::is_aur_package() {
            return Ok(AUR_DESKTOP_FILE.to_owned());
        }
        let exe = tauri::utils::platform::current_exe()
            .map_err(|error| format!("Could not find Legio executable: {error}"))?;
        let name = exe
            .file_name()
            .ok_or_else(|| "Legio executable has no file name".to_owned())?;
        Ok(format!("{}-handler.desktop", name.to_string_lossy()))
    }

    pub(super) fn handler(_app: &AppHandle) -> Result<LinkHandler, String> {
        let output = Command::new("xdg-mime")
            .args(["query", "default", &mime_type()])
            .output()
            .map_err(|error| format!("Could not read Thunderstore links: {error}"))?;
        let default = String::from_utf8_lossy(&output.stdout);
        let default = Some(default.trim()).filter(|name| !name.is_empty());
        Ok(classify(default, &own_desktop_file()?))
    }

    pub(super) fn register(app: &AppHandle) -> Result<(), String> {
        if crate::commands::is_aur_package() {
            // Like the legio:// handler, the package wrapper keeps the AUR updater settings.
            let status = Command::new("xdg-mime")
                .args(["default", AUR_DESKTOP_FILE, &mime_type()])
                .status()
                .map_err(|error| {
                    format!("Could not register packaged Thunderstore links: {error}")
                })?;
            return if status.success() {
                Ok(())
            } else {
                Err(format!(
                    "Could not register packaged Thunderstore links: {status}"
                ))
            };
        }
        app.deep_link()
            .register(SCHEME)
            .map_err(|error| format!("Could not register Thunderstore links: {error}"))
    }

    pub(super) fn release(app: &AppHandle) -> Result<bool, String> {
        if handler(app)? != LinkHandler::Legio {
            return Ok(false);
        }
        if !crate::commands::is_aur_package() {
            app.deep_link()
                .unregister(SCHEME)
                .map_err(|error| format!("Could not release Thunderstore links: {error}"))?;
            return Ok(true);
        }
        let path = app
            .path()
            .config_dir()
            .map_err(|error| format!("Could not release Thunderstore links: {error}"))?
            .join("mimeapps.list");
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(format!("Could not release Thunderstore links: {error}")),
        };
        let Some(updated) = super::without_default(&contents, &mime_type(), AUR_DESKTOP_FILE)
        else {
            return Ok(false);
        };
        std::fs::write(&path, updated)
            .map_err(|error| format!("Could not release Thunderstore links: {error}"))?;
        Ok(true)
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use tauri::AppHandle;

    use super::LinkHandler;

    const UNSUPPORTED: &str = "Thunderstore links are not supported on this platform";

    pub(super) fn handler(_app: &AppHandle) -> Result<LinkHandler, String> {
        Err(UNSUPPORTED.to_owned())
    }

    pub(super) fn register(_app: &AppHandle) -> Result<(), String> {
        Err(UNSUPPORTED.to_owned())
    }

    pub(super) fn release(_app: &AppHandle) -> Result<bool, String> {
        Err(UNSUPPORTED.to_owned())
    }
}

/// `contents` of a `mimeapps.list` without the default-handler line of
/// `mime` that points at `desktop_file`, or `None` if there is no such line.
#[cfg(any(target_os = "linux", test))]
fn without_default(contents: &str, mime: &str, desktop_file: &str) -> Option<String> {
    let line = format!("{mime}={desktop_file}");
    let mut in_defaults = false;
    let mut removed = false;
    let mut updated = String::with_capacity(contents.len());
    for raw in contents.split_inclusive('\n') {
        let trimmed = raw.trim();
        if trimmed.starts_with('[') {
            in_defaults = trimmed == "[Default Applications]";
        } else if in_defaults && trimmed == line {
            removed = true;
            continue;
        }
        updated.push_str(raw);
    }
    removed.then_some(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_the_registered_command() {
        let own = "\"C:\\Legio\\legio.exe\" \"%1\"";
        assert_eq!(classify(Some(own), own), LinkHandler::Legio);
        assert_eq!(
            classify(Some("\"C:\\r2modman\\r2modman.exe\" \"%1\""), own),
            LinkHandler::Other
        );
        assert_eq!(classify(Some(""), own), LinkHandler::Other);
        assert_eq!(classify(None, own), LinkHandler::Nobody);
    }

    #[test]
    fn serializes_camel_case() {
        let value =
            serde_json::to_value([LinkHandler::Legio, LinkHandler::Other, LinkHandler::Nobody])
                .unwrap();
        assert_eq!(value, serde_json::json!(["legio", "other", "nobody"]));
    }

    const MIME: &str = "x-scheme-handler/ror2mm";
    const LAUNCHER: &str = "legio-launcher.desktop";

    #[test]
    fn removes_only_the_matching_default_line() {
        let contents = "[Added Associations]\nx-scheme-handler/ror2mm=legio-launcher.desktop;\n\n[Default Applications]\nx-scheme-handler/legio=legio-launcher.desktop\nx-scheme-handler/ror2mm=legio-launcher.desktop\ntext/plain=a.desktop\n";
        assert_eq!(
            without_default(contents, MIME, LAUNCHER).unwrap(),
            "[Added Associations]\nx-scheme-handler/ror2mm=legio-launcher.desktop;\n\n[Default Applications]\nx-scheme-handler/legio=legio-launcher.desktop\ntext/plain=a.desktop\n"
        );
    }

    #[test]
    fn keeps_other_default_handlers() {
        let other = "[Default Applications]\nx-scheme-handler/ror2mm=r2modman.desktop\n";
        assert_eq!(without_default(other, MIME, LAUNCHER), None);
        let wrong_section =
            "[Added Associations]\nx-scheme-handler/ror2mm=legio-launcher.desktop\n";
        assert_eq!(without_default(wrong_section, MIME, LAUNCHER), None);
        assert_eq!(without_default("", MIME, LAUNCHER), None);
    }
}
