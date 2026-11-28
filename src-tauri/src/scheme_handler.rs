//! Linux URL scheme registration. The deep-link plugin starts host tools with
//! the AppImage's bundled library paths, which crash them, so Legio writes the
//! plugin's handler file itself and runs those tools with the host environment.

use std::path::Path;
use std::process::Command;

use tauri::{AppHandle, Manager};

const AUR_DESKTOP_FILE: &str = "legio-launcher.desktop";

/// The desktop file that opens Legio: the package wrapper on the AUR, else a
/// handler file named like the plugin's so earlier registrations stay valid.
pub(crate) fn desktop_file() -> Result<String, String> {
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

pub(crate) fn xdg_mime() -> Command {
    let mut command = Command::new("xdg-mime");
    crate::launch_environment::apply(&mut command);
    command
}

/// Makes Legio the default handler of `scheme`.
pub(crate) fn register(app: &AppHandle, scheme: &str) -> Result<(), String> {
    let desktop_file = desktop_file()?;
    let mime = format!("x-scheme-handler/{scheme}");
    // The AUR package wrapper keeps the AUR updater settings, so it is used as is.
    if !crate::commands::is_aur_package() {
        let directory = applications_directory(app)?;
        let path = directory.join(&desktop_file);
        let existing = read_existing(&path)?;
        let mut types = existing.as_deref().map(mime_types).unwrap_or_default();
        if !types.contains(&mime.as_str()) {
            types.push(&mime);
        }
        let entry = desktop_entry(&display_name(app, &desktop_file), &exec_line(app)?, &types);
        if existing.as_deref() != Some(entry.as_str()) {
            std::fs::write(&path, entry)
                .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
        }
        update_database(&directory)?;
    }
    run(xdg_mime().args(["default", &desktop_file, &mime]))
}

/// Removes `scheme` from Legio's handler file. Returns whether it was listed.
/// The AUR package's desktop file is never changed.
pub(crate) fn remove(app: &AppHandle, scheme: &str) -> Result<bool, String> {
    if crate::commands::is_aur_package() {
        return Ok(false);
    }
    let desktop_file = desktop_file()?;
    let mime = format!("x-scheme-handler/{scheme}");
    let directory = applications_directory(app)?;
    let path = directory.join(&desktop_file);
    let Some(existing) = read_existing(&path)? else {
        return Ok(false);
    };
    let types = mime_types(&existing);
    if !types.contains(&mime.as_str()) {
        return Ok(false);
    }
    let types: Vec<&str> = types.into_iter().filter(|value| *value != mime).collect();
    let entry = desktop_entry(&display_name(app, &desktop_file), &exec_line(app)?, &types);
    std::fs::write(&path, entry)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    update_database(&directory)?;
    Ok(true)
}

fn applications_directory(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let directory = app
        .path()
        .data_dir()
        .map_err(|error| format!("Could not find the applications folder: {error}"))?
        .join("applications");
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create {}: {error}", directory.display()))?;
    Ok(directory)
}

fn read_existing(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Could not read {}: {error}", path.display())),
    }
}

fn display_name(app: &AppHandle, desktop_file: &str) -> String {
    app.config()
        .product_name
        .clone()
        .unwrap_or_else(|| desktop_file.to_owned())
}

fn exec_line(app: &AppHandle) -> Result<String, String> {
    let program = match app.env().appimage {
        Some(appimage) => appimage,
        None => tauri::utils::platform::current_exe()
            .map_err(|error| format!("Could not find Legio executable: {error}"))?
            .into_os_string(),
    };
    Ok(format!(
        "{} %u",
        quoted_argument(&program.to_string_lossy())?
    ))
}

/// `value` as a quoted desktop entry `Exec` argument.
fn quoted_argument(value: &str) -> Result<String, String> {
    if value.chars().any(char::is_control) {
        return Err("Legio's path contains control characters".to_owned());
    }
    let mut quoted = String::from("\"");
    for character in value.chars() {
        match character {
            // Escaped for the quoting rule, then the backslash again for the string rule.
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(character);
            }
            '\\' => quoted.push_str("\\\\\\\\"),
            '%' => quoted.push_str("%%"),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    Ok(quoted)
}

fn mime_types(entry: &str) -> Vec<&str> {
    entry
        .lines()
        .find_map(|line| line.strip_prefix("MimeType="))
        .map(|value| value.split(';').filter(|value| !value.is_empty()).collect())
        .unwrap_or_default()
}

fn desktop_entry(name: &str, exec: &str, mime_types: &[&str]) -> String {
    let types: String = mime_types.iter().map(|value| format!("{value};")).collect();
    format!(
        "[Desktop Entry]\nType=Application\nName={name}\nExec={exec}\nTerminal=false\nMimeType={types}\nNoDisplay=true\n"
    )
}

fn update_database(directory: &Path) -> Result<(), String> {
    let mut command = Command::new("update-desktop-database");
    command.arg(directory);
    crate::launch_environment::apply(&mut command);
    match command.status() {
        // Not every distro ships it, and the cache it refreshes is only an optimization.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not start update-desktop-database: {error}")),
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("update-desktop-database failed: {status}")),
    }
}

fn run(command: &mut Command) -> Result<(), String> {
    let name = command.get_program().to_string_lossy().into_owned();
    let status = command
        .status()
        .map_err(|error| format!("Could not start {name}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name} failed: {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_plugin_entries() {
        // Written by the deep-link plugin, which leaves no trailing separator.
        let plugin = "[Desktop Entry]\nType=Application\nName=Legio Launcher\nExec=\"/x\" %u\nTerminal=false\nMimeType=x-scheme-handler/legio;x-scheme-handler/ror2mm\nNoDisplay=true\n";
        let types = mime_types(plugin);
        assert_eq!(types, ["x-scheme-handler/legio", "x-scheme-handler/ror2mm"]);
        assert_eq!(
            desktop_entry("Legio Launcher", "\"/x\" %u", &types),
            "[Desktop Entry]\nType=Application\nName=Legio Launcher\nExec=\"/x\" %u\nTerminal=false\nMimeType=x-scheme-handler/legio;x-scheme-handler/ror2mm;\nNoDisplay=true\n"
        );
        assert!(mime_types("[Desktop Entry]\n").is_empty());
    }

    #[test]
    fn quotes_exec_paths() {
        assert_eq!(
            quoted_argument("/home/a b/Legio.AppImage").unwrap(),
            "\"/home/a b/Legio.AppImage\""
        );
        assert_eq!(
            quoted_argument("/a$\"`\\100%").unwrap(),
            "\"/a\\\\$\\\\\"\\\\`\\\\\\\\100%%\""
        );
        assert!(quoted_argument("/a\nExec=x").is_err());
    }
}
