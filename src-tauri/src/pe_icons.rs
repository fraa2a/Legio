use std::{fs, io::Cursor, path::Path};

const MAX_FRAME_DIMENSION: u16 = 256;

pub(crate) fn extract_png(executable: &Path) -> Result<Vec<u8>, String> {
    if !executable.is_absolute() {
        return Err("Windows executable path must be absolute".to_owned());
    }
    if !executable
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err("Selected file is not a Windows executable".to_owned());
    }
    let metadata = fs::symlink_metadata(executable)
        .map_err(|error| format!("Could not inspect Windows executable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Selected Windows executable is not a regular file".to_owned());
    }

    let extractor = icoextract_rs::IconExtractor::from_path(executable)
        .map_err(|error| format!("Could not read embedded game icon: {error}"))?;
    let icon = extractor
        .icon_by_index(0)
        .map_err(|error| format!("Could not read embedded game icon: {error}"))?;
    let frame = icon
        .images()
        .iter()
        .filter(|frame| {
            frame.info().width() <= MAX_FRAME_DIMENSION
                && frame.info().height() <= MAX_FRAME_DIMENSION
        })
        .max_by_key(|frame| {
            (
                u32::from(frame.info().width()) * u32::from(frame.info().height()),
                frame.info().bit_count(),
            )
        })
        .ok_or_else(|| "Embedded game icon has no supported image frames".to_owned())?;
    let icon = icoextract_rs::ExtractedIcon::new(icon.resource_id().clone(), vec![frame.clone()]);
    let ico = icon
        .to_ico_bytes()
        .map_err(|error| format!("Could not encode embedded game icon: {error}"))?;
    let decoded = image::load_from_memory_with_format(&ico, image::ImageFormat::Ico)
        .map_err(|error| format!("Could not decode embedded game icon: {error}"))?;
    let thumbnail = decoded.thumbnail(
        u32::from(MAX_FRAME_DIMENSION),
        u32::from(MAX_FRAME_DIMENSION),
    );
    let mut output = Cursor::new(Vec::new());
    thumbnail
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| format!("Could not encode embedded game icon as PNG: {error}"))?;
    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    fn test_dir() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("legio-pe-icon-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn rejects_relative_executable_paths() {
        let error = extract_png(Path::new("game.exe")).unwrap_err();
        assert!(error.contains("must be absolute"));
    }

    #[test]
    fn rejects_non_executable_extensions() {
        let root = test_dir();
        let source = root.join("game.dll");
        fs::write(&source, b"MZ").unwrap();

        let error = extract_png(&source).unwrap_err();

        assert!(error.contains("not a Windows executable"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_malformed_executables_with_a_diagnostic() {
        let root = test_dir();
        let source = root.join("game.exe");
        fs::write(&source, b"MZ").unwrap();

        let error = extract_png(&source).unwrap_err();

        assert!(error.contains("Could not read embedded game icon"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn refuses_symlinked_executables() {
        let root = test_dir();
        let target = root.join("target.exe");
        let source = root.join("game.exe");
        fs::write(&target, b"MZ").unwrap();
        symlink(&target, &source).unwrap();

        let error = extract_png(&source).unwrap_err();

        assert!(error.contains("not a regular file"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires an installed Windows game with embedded PE icons"]
    fn extracts_installed_windows_game_icon() {
        let path = std::env::var_os("LEGIO_TEST_WINDOWS_GAME_EXE")
            .map(std::path::PathBuf::from)
            .expect("set LEGIO_TEST_WINDOWS_GAME_EXE to a Windows game executable");
        let bytes = extract_png(&path).unwrap();
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(bytes.len() <= 2 * 1024 * 1024);
        let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .expect("extracted game icon must decode as PNG");
        assert!(decoded.width() > 0 && decoded.height() > 0);
        assert!(decoded.width() <= u32::from(MAX_FRAME_DIMENSION));
        assert!(decoded.height() <= u32::from(MAX_FRAME_DIMENSION));
    }
}
