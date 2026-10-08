use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::Path;

const OVERLAY_DLL: &str = "GameOverlayRenderer64.dll";
const MAX_DLL_BYTES: u64 = 128 * 1024 * 1024;

pub(crate) fn prepare_prefix(steam_root: &Path, wine_prefix: &Path) -> Result<bool, String> {
    let prefix = fs::canonicalize(wine_prefix)
        .map_err(|error| format!("Could not inspect Steam overlay prefix: {error}"))?;
    let destination = prefix
        .join("drive_c/Program Files (x86)/Steam")
        .join(OVERLAY_DLL);
    match fs::symlink_metadata(&destination) {
        Ok(_) => {
            validate_dll(&destination)?;
            return Ok(false);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not inspect Steam overlay DLL: {error}")),
    }

    let steam_root = fs::canonicalize(steam_root)
        .map_err(|error| format!("Could not inspect Steam installation: {error}"))?;
    let source = steam_root.join("legacycompat").join(OVERLAY_DLL);
    let source = fs::canonicalize(&source).map_err(|error| {
        format!(
            "Steam Windows overlay DLL is unavailable at {}. Update or repair Steam: {error}",
            source.display()
        )
    })?;
    if !source.starts_with(&steam_root) {
        return Err("Steam overlay DLL escapes the Steam installation".to_owned());
    }
    validate_dll(&source)?;

    let mut directory = prefix.clone();
    for component in ["drive_c", "Program Files (x86)", "Steam"] {
        directory.push(component);
        match fs::create_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("Could not create Steam overlay directory: {error}")),
        }
        let resolved = fs::canonicalize(&directory)
            .map_err(|error| format!("Could not inspect Steam overlay directory: {error}"))?;
        if !resolved.starts_with(&prefix) || !resolved.is_dir() {
            return Err("Steam overlay directory escapes the Wine prefix".to_owned());
        }
    }

    let temporary = directory.join(format!(".legio-overlay-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let input = File::open(&source)
            .map_err(|error| format!("Could not read Steam overlay DLL: {error}"))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Could not prepare Steam overlay DLL: {error}"))?;
        let bytes = io::copy(&mut input.take(MAX_DLL_BYTES + 1), &mut output)
            .map_err(|error| format!("Could not copy Steam overlay DLL: {error}"))?;
        if bytes > MAX_DLL_BYTES {
            return Err("Steam overlay DLL exceeds the supported size".to_owned());
        }
        validate_dll(&temporary)?;
        output
            .sync_all()
            .map_err(|error| format!("Could not sync Steam overlay DLL: {error}"))?;
        match fs::hard_link(&temporary, &destination) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                validate_dll(&destination)?;
                Ok(false)
            }
            Err(error) => Err(format!(
                "Could not install Steam overlay DLL in the prefix: {error}"
            )),
        }
    })();
    if let Err(error) = fs::remove_file(&temporary)
        && error.kind() != io::ErrorKind::NotFound
    {
        return Err(format!(
            "Could not remove temporary Steam overlay DLL: {error}"
        ));
    }
    result
}

fn validate_dll(path: &Path) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not inspect Steam overlay DLL: {error}"))?;
    if !metadata.is_file() || metadata.len() > MAX_DLL_BYTES {
        return Err("Steam overlay DLL is not a supported regular file".to_owned());
    }
    let mut file = File::open(path).map_err(|error| {
        format!(
            "Could not read Steam overlay DLL at {}: {error}",
            path.display()
        )
    })?;
    let mut signature = [0; 2];
    file.read_exact(&mut signature)
        .map_err(|error| format!("Could not read Steam overlay DLL header: {error}"))?;
    if &signature != b"MZ" {
        return Err(format!(
            "Steam overlay DLL at {} is not a Windows DLL",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn fixture() -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("legio-steam-overlay-{}", uuid::Uuid::new_v4()));
        let steam = root.join("Steam");
        let prefix = root.join("prefix/pfx");
        fs::create_dir_all(steam.join("legacycompat")).unwrap();
        fs::create_dir_all(&prefix).unwrap();
        fs::write(steam.join("legacycompat").join(OVERLAY_DLL), b"MZ overlay").unwrap();
        (root, steam, prefix)
    }

    #[test]
    fn missing_overlay_is_copied_into_the_proton_steam_directory() {
        let (root, steam, prefix) = fixture();
        assert!(prepare_prefix(&steam, &prefix).unwrap());
        assert_eq!(
            fs::read(
                prefix
                    .join("drive_c/Program Files (x86)/Steam")
                    .join(OVERLAY_DLL)
            )
            .unwrap(),
            b"MZ overlay"
        );
        assert!(!prepare_prefix(&steam, &prefix).unwrap());
        assert_eq!(
            fs::read_dir(prefix.join("drive_c/Program Files (x86)/Steam"))
                .unwrap()
                .count(),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_overlay_and_proton_symlinks_are_preserved() {
        let (root, steam, prefix) = fixture();
        let directory = prefix.join("drive_c/Program Files (x86)/Steam");
        fs::create_dir_all(&directory).unwrap();
        let provided = root.join("runner-overlay.dll");
        fs::write(&provided, b"MZ runner version").unwrap();
        symlink(&provided, directory.join(OVERLAY_DLL)).unwrap();
        fs::remove_dir_all(&steam).unwrap();
        assert!(!prepare_prefix(&steam, &prefix).unwrap());
        assert!(
            fs::symlink_metadata(directory.join(OVERLAY_DLL))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(provided).unwrap(), b"MZ runner version");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_steam_overlay_reports_its_location_without_changing_the_prefix() {
        let (root, steam, prefix) = fixture();
        fs::remove_file(steam.join("legacycompat").join(OVERLAY_DLL)).unwrap();
        let error = prepare_prefix(&steam, &prefix).unwrap_err();
        assert!(error.contains("legacycompat/GameOverlayRenderer64.dll"));
        assert!(!prefix.join("drive_c").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_overlay_is_rejected_without_changing_the_prefix() {
        let (root, steam, prefix) = fixture();
        fs::write(steam.join("legacycompat").join(OVERLAY_DLL), b"ELF library").unwrap();
        assert!(
            prepare_prefix(&steam, &prefix)
                .unwrap_err()
                .contains("not a Windows DLL")
        );
        assert!(!prefix.join("drive_c").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_overlay_is_rejected_before_creating_prefix_directories() {
        let (root, steam, prefix) = fixture();
        OpenOptions::new()
            .write(true)
            .open(steam.join("legacycompat").join(OVERLAY_DLL))
            .unwrap()
            .set_len(MAX_DLL_BYTES + 1)
            .unwrap();
        assert!(
            prepare_prefix(&steam, &prefix)
                .unwrap_err()
                .contains("not a supported regular file")
        );
        assert!(!prefix.join("drive_c").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prefix_directory_symlink_cannot_redirect_overlay_writes() {
        let (root, steam, prefix) = fixture();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, prefix.join("drive_c")).unwrap();
        assert!(
            prepare_prefix(&steam, &prefix)
                .unwrap_err()
                .contains("escapes")
        );
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_symlink_cannot_import_an_unrelated_dll() {
        let (root, steam, prefix) = fixture();
        let outside = root.join("outside.dll");
        fs::write(&outside, b"MZ unrelated").unwrap();
        let source = steam.join("legacycompat").join(OVERLAY_DLL);
        fs::remove_file(&source).unwrap();
        symlink(outside, source).unwrap();
        assert!(
            prepare_prefix(&steam, &prefix)
                .unwrap_err()
                .contains("escapes")
        );
        assert!(!prefix.join("drive_c").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
