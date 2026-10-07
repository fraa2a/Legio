use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;

pub(crate) fn host_path(value: &OsStr) -> OsString {
    match std::env::var_os("APPDIR") {
        Some(directory) => without_bundle_paths(value, Path::new(&directory)),
        None => value.to_owned(),
    }
}

pub(crate) fn apply(command: &mut Command) {
    let Some(directory) = std::env::var_os("APPDIR") else {
        return;
    };
    apply_from(command, Path::new(&directory), std::env::vars_os());
}

fn without_bundle_paths(value: &OsStr, directory: &Path) -> OsString {
    std::env::join_paths(std::env::split_paths(value).filter(|path| !path.starts_with(directory)))
        .unwrap_or_default()
}

fn apply_from(
    command: &mut Command,
    directory: &Path,
    environment: impl IntoIterator<Item = (OsString, OsString)>,
) {
    for (key, value) in environment {
        if !matches!(
            key.to_str(),
            Some(
                "PATH"
                    | "LD_LIBRARY_PATH"
                    | "PYTHONPATH"
                    | "PYTHONHOME"
                    | "GIO_MODULE_DIR"
                    | "GDK_PIXBUF_MODULE_FILE"
                    | "GST_PLUGIN_PATH"
                    | "GST_PLUGIN_SYSTEM_PATH"
                    | "XDG_DATA_DIRS"
            )
        ) {
            continue;
        }
        let host = without_bundle_paths(&value, directory);
        if host != value {
            if host.is_empty() {
                command.env_remove(&key);
            } else {
                command.env(&key, host);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_paths_are_removed_while_host_paths_and_unrelated_settings_survive() {
        let mut command = Command::new("/bin/sh");
        apply_from(
            &mut command,
            Path::new("/tmp/.mount_Legio"),
            [
                ("LD_LIBRARY_PATH", "/tmp/.mount_Legio/usr/lib:/host/lib"),
                (
                    "PATH",
                    "/tmp/.mount_Legio/usr/bin:/usr/bin:/home/user/.local/bin",
                ),
                ("PYTHONHOME", "/tmp/.mount_Legio/usr"),
                ("GIO_MODULE_DIR", "/tmp/.mount_Legio/usr/lib/gio/modules"),
                ("WINEPREFIX", "/games/prefix"),
                ("DISPLAY", ":0"),
            ]
            .map(|(key, value)| (key.into(), value.into())),
        );
        let environment = command
            .get_envs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            environment[OsStr::new("LD_LIBRARY_PATH")],
            Some(OsStr::new("/host/lib"))
        );
        assert_eq!(
            environment[OsStr::new("PATH")],
            Some(OsStr::new("/usr/bin:/home/user/.local/bin"))
        );
        assert_eq!(environment[OsStr::new("PYTHONHOME")], None);
        assert_eq!(environment[OsStr::new("GIO_MODULE_DIR")], None);
        assert!(!environment.contains_key(OsStr::new("WINEPREFIX")));
        assert!(!environment.contains_key(OsStr::new("DISPLAY")));
        assert_eq!(
            without_bundle_paths(
                OsStr::new("/tmp/.mount_Legio-extra/lib"),
                Path::new("/tmp/.mount_Legio")
            ),
            "/tmp/.mount_Legio-extra/lib"
        );
        command
            .arg("-c")
            .arg("test \"$LD_LIBRARY_PATH\" = /host/lib && test -z \"$PYTHONHOME\"");
        assert!(command.status().unwrap().success());
    }
}
