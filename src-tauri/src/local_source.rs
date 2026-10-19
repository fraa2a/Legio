use std::{fs::File, io::Read, path::Path};

pub(crate) fn location(path: &str) -> Result<String, String> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err("Select an absolute path to a local JSON source".into());
    }
    let path = path
        .canonicalize()
        .map_err(|error| format!("Could not open local source: {error}"))?;
    reqwest::Url::from_file_path(path)
        .map(String::from)
        .map_err(|_| "Invalid local source path".into())
}

pub(crate) fn file_path(location: &str) -> Result<std::path::PathBuf, String> {
    let url = reqwest::Url::parse(location).map_err(|_| "Invalid local source location")?;
    if url.query().is_some() || url.fragment().is_some() || url.host_str().is_some() {
        return Err("Local sources must be files on this computer".into());
    }
    url.to_file_path()
        .map_err(|_| "Invalid local source path".into())
}

pub(crate) fn display_name(location: &str) -> Option<String> {
    file_path(location)
        .ok()?
        .file_name()?
        .to_str()
        .map(str::to_owned)
}

pub(crate) fn read(location: &str) -> Result<Vec<u8>, String> {
    let path = file_path(location)?;
    let mut options = File::options();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|error| format!("Could not read local source: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Could not inspect local source: {error}"))?;
    if !metadata.is_file() {
        return Err("Local source must be a regular file".into());
    }
    let limit = crate::hydra_source::MAX_BYTES;
    if metadata.len() > limit as u64 {
        return Err("Local source exceeds the size limit".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read local source: {error}"))?;
    if bytes.len() > limit {
        return Err("Local source exceeds the size limit".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_paths_round_trip_and_refresh_reads_disk_changes() {
        let directory = std::env::temp_dir().join(format!("legio-local-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("sorgente à #1.json");
        let first = br#"{"name":"Local","downloads":[]}"#;
        std::fs::write(&path, first).unwrap();
        let url = location(path.to_str().unwrap()).unwrap();
        assert_eq!(display_name(&url).as_deref(), Some("sorgente à #1.json"));
        assert_eq!(read(&url).unwrap(), first);
        assert!(crate::source_catalog::parse_source(&read(&url).unwrap(), &[]).is_ok());
        assert!(crate::legio_source::validate_manifest_url(&url).is_err());
        std::fs::write(&path, b"updated").unwrap();
        assert_eq!(read(&url).unwrap(), b"updated");
        std::fs::remove_file(&path).unwrap();
        assert!(read(&url).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_directories_large_files_and_nonlocal_locations() {
        let directory = std::env::temp_dir().join(format!("legio-local-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        assert!(read(&location(directory.to_str().unwrap()).unwrap()).is_err());
        let path = directory.join("large.json");
        File::create(&path)
            .unwrap()
            .set_len(crate::hydra_source::MAX_BYTES as u64 + 1)
            .unwrap();
        assert!(read(&location(path.to_str().unwrap()).unwrap()).is_err());
        assert!(location("relative.json").is_err());
        for url in [
            "https://example.com/source.json",
            "file://server/share/source.json",
            "file:///tmp/a.json?x=1",
        ] {
            assert!(read(url).is_err());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
