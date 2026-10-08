use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::runner_discovery::InstalledRunner;
use crate::{steam_local, steam_vdf};

const MAX_METADATA_BYTES: u64 = 1024 * 1024;

pub(crate) fn prepare(
    runner: &InstalledRunner,
    steam_root: &Path,
    compatdata: &Path,
    wine_prefix: &Path,
) -> Result<Command, String> {
    let proton = Path::new(&runner.path).join("proton");
    if !is_executable(&proton) {
        return Err("Selected Proton entry point is unavailable".to_owned());
    }
    let runtime = match required_runtime(Path::new(&runner.path))? {
        Some(app_id) => Some(find_runtime(steam_root, app_id)?),
        None => None,
    };
    let proton_data = prepare_prefix_layout(compatdata, wine_prefix)?;
    let mut command = if let Some(runtime) = &runtime {
        let mut command = Command::new(runtime.join("run"));
        command.arg("--").arg(&proton);
        command
    } else {
        Command::new(&proton)
    };
    command.arg("run");
    crate::launch_environment::apply(&mut command);
    let mut mounts = vec![PathBuf::from(&runner.path), steam_root.to_path_buf()];
    if let Some(runtime) = runtime {
        mounts.push(runtime);
    }
    let mounts = std::env::join_paths(mounts)
        .map_err(|error| format!("Unsupported Steam runtime mount path: {error}"))?;
    command
        .env("STEAM_COMPAT_DATA_PATH", proton_data)
        .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", steam_root)
        .env("STEAM_COMPAT_TOOL_PATHS", &mounts)
        .env("STEAM_COMPAT_MOUNTS", mounts)
        .env("SteamGameId", "0")
        .env("SteamAppId", "0")
        .env_remove("UMU_ID");
    Ok(command)
}

fn required_runtime(runner: &Path) -> Result<Option<u32>, String> {
    let manifest = read_metadata(&runner.join("toolmanifest.vdf"))
        .map_err(|error| format!("Could not read Proton toolmanifest.vdf: {error}"))?;
    let value = match steam_vdf::keyvalues_scalar(&manifest, &["require_tool_appid"]) {
        Ok(value) => value,
        Err(steam_vdf::VdfError::ScalarNotFound) => return Ok(None),
        Err(error) => return Err(format!("Invalid Proton runtime manifest: {error}")),
    };
    if value == "0" {
        return Ok(None);
    }
    let app_id = value
        .parse::<u32>()
        .ok()
        .filter(|id| matches!(id, 1391110 | 1628350 | 4183110 | 4185400))
        .ok_or_else(|| format!("Unsupported Proton runtime requirement: {value}"))?;
    Ok(Some(app_id))
}

fn find_runtime(steam_root: &Path, app_id: u32) -> Result<PathBuf, String> {
    let mut libraries = vec![steam_root.to_path_buf()];
    match read_metadata(&steam_root.join("steamapps/libraryfolders.vdf")) {
        Ok(bytes) => libraries.extend(
            steam_local::parse_library_folders(&bytes)
                .map_err(|error| format!("Could not read Steam runtime libraries: {error}"))?,
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not read Steam runtime libraries: {error}")),
    }
    for library in libraries {
        let library = match fs::canonicalize(&library) {
            Ok(library) => library,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("Could not inspect Steam runtime library: {error}")),
        };
        let manifest_path = library
            .join("steamapps")
            .join(format!("appmanifest_{app_id}.acf"));
        let bytes = match read_metadata(&manifest_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("Could not read Steam runtime install: {error}")),
        };
        let manifest = steam_local::parse_app_manifest(&bytes)
            .map_err(|error| format!("Invalid Steam runtime install: {error}"))?;
        if manifest.app_id != app_id {
            return Err("Steam runtime manifest has an unexpected app ID".to_owned());
        }
        let directory = library.join("steamapps/common").join(manifest.install_dir);
        let directory = match fs::canonicalize(directory) {
            Ok(directory) => directory,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("Could not inspect Steam runtime: {error}")),
        };
        let run = directory.join("run");
        if !directory.starts_with(&library) {
            return Err("Steam runtime escapes its library".to_owned());
        }
        if is_executable(&run) {
            let resolved = fs::canonicalize(&run)
                .map_err(|error| format!("Could not resolve Steam runtime entry point: {error}"))?;
            if !resolved.starts_with(&directory) {
                return Err("Steam runtime entry point escapes its installation".to_owned());
            }
            return Ok(directory);
        }
    }
    Err(format!(
        "Install or repair Steam Linux Runtime (Steam app {app_id}) in Steam before launching with this Proton runner"
    ))
}

fn prepare_prefix_layout(compatdata: &Path, wine_prefix: &Path) -> Result<PathBuf, String> {
    if compatdata != wine_prefix {
        // umu keeps Proton metadata alongside a self-aliased Wine prefix.
        match fs::canonicalize(wine_prefix.join("pfx")) {
            Ok(existing) if existing == wine_prefix => return Ok(wine_prefix.to_path_buf()),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Could not inspect existing umu prefix layout: {error}"
                ));
            }
        }
    }
    let nested = compatdata.join("pfx");
    match fs::symlink_metadata(&nested) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound && compatdata == wine_prefix => {
            // Proton expects pfx even when the existing Wine prefix is at the data root.
            symlink(".", &nested)
                .map_err(|error| format!("Could not alias the existing Proton prefix: {error}"))?;
        }
        Err(error) => return Err(format!("Could not inspect Proton prefix layout: {error}")),
    }
    let resolved = fs::canonicalize(nested)
        .map_err(|error| format!("Could not resolve Proton prefix layout: {error}"))?;
    if resolved != wine_prefix {
        return Err("Proton pfx does not point to the selected Wine prefix".to_owned());
    }
    Ok(compatdata.to_path_buf())
}

fn read_metadata(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_METADATA_BYTES {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_METADATA_BYTES {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    Ok(bytes)
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner_discovery::RunnerKind;

    fn executable(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn fixture() -> (PathBuf, InstalledRunner, PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("legio-proton-runtime-{}", uuid::Uuid::new_v4()));
        let runner_path = root.join("Proton Tools/GE-Proton10-33");
        executable(
            &runner_path.join("proton"),
            "#!/bin/sh\nprintf '%s\\n' \"$STEAM_COMPAT_CLIENT_INSTALL_PATH\" \"$STEAM_COMPAT_DATA_PATH\" \"${UMU_ID-unset}\" \"$@\"\n",
        );
        fs::write(
            runner_path.join("toolmanifest.vdf"),
            b"\"manifest\" { \"require_tool_appid\" \"1628350\" }",
        )
        .unwrap();
        let steam = root.join("Steam Client");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        let compatdata = root.join("prefix");
        fs::create_dir_all(compatdata.join("pfx/drive_c")).unwrap();
        fs::write(compatdata.join("pfx/drive_c/save.dat"), b"saved game").unwrap();
        let runner = InstalledRunner {
            kind: RunnerKind::GeProton,
            name: "GE-Proton10-33".to_owned(),
            version: "custom build version".to_owned(),
            path: runner_path.to_string_lossy().into_owned(),
        };
        (root, runner, steam, compatdata)
    }

    fn install_runtime(library: &Path, app_id: u32, directory: &str) -> PathBuf {
        let runtime = library.join("steamapps/common").join(directory);
        executable(
            &runtime.join("run"),
            "#!/bin/sh\n[ \"$1\" = -- ] || exit 23\nshift\nexec \"$@\"\n",
        );
        fs::write(library.join(format!("steamapps/appmanifest_{app_id}.acf")), format!("\"AppState\" {{ \"appid\" \"{app_id}\" \"name\" \"Steam Linux Runtime\" \"installdir\" \"{directory}\" }}")).unwrap();
        runtime
    }

    #[test]
    fn runs_proton_through_the_declared_runtime_with_steam_context_and_exact_arguments() {
        let (root, runner, steam, data) = fixture();
        let runtime = install_runtime(&steam, 1628350, "Steam Runtime Custom Name");
        let mut command = prepare(&runner, &steam, &data, &data.join("pfx")).unwrap();
        assert_eq!(command.get_program(), runtime.join("run"));
        command.args([
            "before",
            "/Games/My Game/game.exe",
            "two words",
            "$(untouched)",
        ]);
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            [
                steam.to_str().unwrap(),
                data.to_str().unwrap(),
                "unset",
                "run",
                "before",
                "/Games/My Game/game.exe",
                "two words",
                "$(untouched)"
            ]
        );
        assert_eq!(
            fs::read(data.join("pfx/drive_c/save.dat")).unwrap(),
            b"saved game"
        );
        assert!(!data.join("pfx/drive_c/Program Files (x86)/Steam").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_requirement_comes_from_manifest_instead_of_runner_version() {
        let (root, runner, steam, data) = fixture();
        fs::write(
            Path::new(&runner.path).join("toolmanifest.vdf"),
            b"\"manifest\" { \"require_tool_appid\" \"4183110\" }",
        )
        .unwrap();
        install_runtime(&steam, 1628350, "sniper");
        let expected = install_runtime(&steam, 4183110, "SteamLinuxRuntime_4");
        let command = prepare(&runner, &steam, &data, &data.join("pfx")).unwrap();
        assert_eq!(command.get_program(), expected.join("run"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_can_be_installed_in_another_steam_library() {
        let (root, runner, steam, data) = fixture();
        let library = root.join("Another Library");
        let runtime = install_runtime(&library, 1628350, "sniper");
        fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} }}",
                root.join("disconnected").display(),
                library.display()
            ),
        )
        .unwrap();
        let command = prepare(&runner, &steam, &data, &data.join("pfx")).unwrap();
        assert_eq!(command.get_program(), runtime.join("run"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unavailable_required_runtime_reports_installation_instruction() {
        let (root, runner, steam, data) = fixture();
        let error = prepare(&runner, &steam, &data, &data.join("pfx")).unwrap_err();
        assert!(error.contains("Install or repair Steam Linux Runtime"));
        assert!(error.contains("1628350"));
        assert_eq!(
            fs::read(data.join("pfx/drive_c/save.dat")).unwrap(),
            b"saved game"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_root_prefix_gets_an_alias_without_moving_saves() {
        let (root, runner, steam, data) = fixture();
        install_runtime(&steam, 1628350, "sniper");
        let prefix = data.join("pfx");
        prepare(&runner, &steam, &prefix, &prefix).unwrap();
        assert_eq!(fs::read_link(prefix.join("pfx")).unwrap(), Path::new("."));
        assert_eq!(
            fs::read(prefix.join("drive_c/save.dat")).unwrap(),
            b"saved game"
        );
        prepare(&runner, &steam, &prefix, &prefix).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_umu_prefix_keeps_its_proton_metadata_directory() {
        let (root, runner, steam, data) = fixture();
        install_runtime(&steam, 1628350, "sniper");
        let prefix = data.join("pfx");
        symlink(".", prefix.join("pfx")).unwrap();
        fs::write(prefix.join("version"), b"existing prefix version").unwrap();
        fs::write(prefix.join("tracked_files"), b"existing tracked files").unwrap();
        let command = prepare(&runner, &steam, &data, &prefix).unwrap();
        assert!(command.get_envs().any(
            |(key, value)| key == "STEAM_COMPAT_DATA_PATH" && value == Some(prefix.as_os_str())
        ));
        assert_eq!(
            fs::read(prefix.join("version")).unwrap(),
            b"existing prefix version"
        );
        assert_eq!(
            fs::read(prefix.join("tracked_files")).unwrap(),
            b"existing tracked files"
        );
        assert_eq!(
            fs::read(prefix.join("drive_c/save.dat")).unwrap(),
            b"saved game"
        );
        assert!(!data.join("version").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn escaping_or_conflicting_prefix_is_rejected_without_replacing_it() {
        let (root, runner, steam, data) = fixture();
        install_runtime(&steam, 1628350, "sniper");
        let outside = root.join("other-prefix");
        fs::create_dir(&outside).unwrap();
        assert!(
            prepare(&runner, &steam, &data, &outside)
                .unwrap_err()
                .contains("does not point")
        );
        assert_eq!(
            fs::read(data.join("pfx/drive_c/save.dat")).unwrap(),
            b"saved game"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn host_runtime_manifest_launches_proton_directly() {
        let (root, runner, steam, data) = fixture();
        fs::write(
            Path::new(&runner.path).join("toolmanifest.vdf"),
            b"\"manifest\" { \"require_tool_appid\" \"0\" }",
        )
        .unwrap();
        let command = prepare(&runner, &steam, &data, &data.join("pfx")).unwrap();
        assert_eq!(
            command.get_program(),
            Path::new(&runner.path).join("proton")
        );
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["run"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_duplicate_or_unknown_runtime_requirements_are_rejected() {
        let (root, runner, _, _) = fixture();
        for manifest in [
            "\"manifest\" { \"require_tool_appid\" \"1628350\"",
            "\"manifest\" { \"require_tool_appid\" \"1628350\" \"require_tool_appid\" \"4183110\" }",
            "\"manifest\" { \"require_tool_appid\" \"999\" }",
        ] {
            fs::write(Path::new(&runner.path).join("toolmanifest.vdf"), manifest).unwrap();
            assert!(required_runtime(Path::new(&runner.path)).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_entry_point_cannot_escape_its_installation() {
        let (root, _, steam, _) = fixture();
        let runtime = install_runtime(&steam, 1628350, "sniper");
        fs::remove_file(runtime.join("run")).unwrap();
        symlink("/bin/sh", runtime.join("run")).unwrap();
        assert!(
            find_runtime(&steam, 1628350)
                .unwrap_err()
                .contains("escapes")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
