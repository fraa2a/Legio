use std::env;
#[cfg(target_os = "linux")]
use std::{fs, path::PathBuf};

pub(crate) fn set_enabled(enabled: bool, development: bool) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let config = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .ok_or_else(|| "Could not find the user configuration directory".to_owned())?;
        if !config.is_absolute() {
            return Err("User configuration directory is not absolute".to_owned());
        }
        let directory = config.join("autostart");
        let path = directory.join(if development {
            "legio-dev.desktop"
        } else {
            "legio.desktop"
        });
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err("Startup entry is not a regular file".to_owned());
                }
                let existing = fs::read_to_string(&path)
                    .map_err(|error| format!("Could not inspect startup entry: {error}"))?;
                if !existing
                    .lines()
                    .any(|line| line == "X-Legio-Autostart=true")
                {
                    return Err("Refusing to replace a startup entry not owned by Legio".to_owned());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Could not inspect startup entry: {error}")),
        }
        if enabled {
            fs::create_dir_all(&directory)
                .map_err(|error| format!("Could not create startup directory: {error}"))?;
            let launcher = env::var_os("APPIMAGE").map(PathBuf::from).unwrap_or(
                env::current_exe()
                    .map_err(|error| format!("Could not find Legio executable: {error}"))?,
            );
            let launcher = fs::canonicalize(launcher)
                .map_err(|error| format!("Could not inspect Legio executable: {error}"))?;
            if !launcher.is_file() {
                return Err("Legio executable is not a file".to_owned());
            }
            let launcher = launcher
                .to_str()
                .ok_or_else(|| "Legio executable path is not UTF-8".to_owned())?;
            if launcher.chars().any(char::is_control) {
                return Err("Legio executable path contains control characters".to_owned());
            }
            let launcher = crate::desktop_shortcuts::quote_exec_argument(launcher)?;
            let entry = format!(
                "[Desktop Entry]\nType=Application\nName=Legio\nExec={launcher} --minimized\nX-Legio-Autostart=true\n"
            );
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let temporary =
                directory.join(format!(".legio-autostart-{}.tmp", uuid::Uuid::new_v4()));
            let result = (|| {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&temporary)
                    .map_err(|error| format!("Could not create startup entry: {error}"))?;
                file.write_all(entry.as_bytes())
                    .and_then(|()| file.sync_all())
                    .map_err(|error| format!("Could not write startup entry: {error}"))?;
                fs::rename(&temporary, &path)
                    .map_err(|error| format!("Could not enable system startup: {error}"))?;
                crate::finalize_install::sync_directory(&directory)
            })();
            if let Err(error) = result {
                if let Err(cleanup) = fs::remove_file(&temporary)
                    && cleanup.kind() != std::io::ErrorKind::NotFound
                {
                    return Err(format!("{error}; startup cleanup failed: {cleanup}"));
                }
                return Err(error);
            }
        } else if path.exists() {
            fs::remove_file(path)
                .map_err(|error| format!("Could not disable system startup: {error}"))?;
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let current = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = current
            .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
            .map_err(|error| format!("Could not open Windows startup settings: {error}"))?;
        if enabled {
            let launcher = env::current_exe()
                .map_err(|error| format!("Could not find Legio executable: {error}"))?;
            let launcher = launcher
                .to_str()
                .ok_or_else(|| "Legio executable path is not UTF-8".to_owned())?;
            key.set_value(
                if development { "LegioDev" } else { "Legio" },
                &format!("\"{launcher}\" --minimized"),
            )
            .map_err(|error| format!("Could not enable system startup: {error}"))
        } else {
            match key.delete_value(if development { "LegioDev" } else { "Legio" }) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!("Could not disable system startup: {error}")),
            }
        }
    }
}
