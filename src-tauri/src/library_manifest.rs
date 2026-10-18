use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(crate) const FILE: &str = "library.ldf";
const LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Library {
    pub format: u32,
    pub library_id: String,
    pub games: Vec<Entry>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Entry {
    pub id: String,
    pub name: String,
    pub installation: Installation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_app_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<Update>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Installation {
    pub folder: String,
    pub executable: String,
    #[serde(default = "direct")]
    pub archive_layout: String,
}
fn direct() -> String {
    "direct".into()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Update {
    pub source_sha256: String,
    pub entry_id: String,
    pub installed_version: String,
}

pub(crate) fn checked(root: &Path, relative: &str, file: bool) -> Result<PathBuf, String> {
    let relative = crate::archive_install::safe_relative_path(relative)?;
    let mut path = root.to_path_buf();
    inspect(root, false)?;
    for component in relative.components() {
        path.push(component);
        inspect(&path, file && path == root.join(&relative))?;
    }
    Ok(path)
}

fn inspect(path: &Path, file: bool) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| format!("Could not inspect library path: {e}"))?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("Library path contains a reparse point".into());
        }
    }
    if metadata.file_type().is_symlink()
        || (file && !metadata.is_file())
        || (!file && !metadata.is_dir())
    {
        return Err("Library path must contain ordinary files and directories".into());
    }
    Ok(())
}

pub(crate) fn lock(root: &Path) -> Result<fs::File, String> {
    inspect(root, false)?;
    let path = root.join(".legio-library.lock");
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
    let file = options
        .open(&path)
        .map_err(|e| format!("Could not lock library: {e}"))?;
    inspect(&path, true)?;
    file.lock()
        .map_err(|e| format!("Could not lock library: {e}"))?;
    Ok(file)
}

pub(crate) fn read(root: &Path) -> Result<Option<Library>, String> {
    let path = root.join(FILE);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
        Ok(metadata) if metadata.len() > LIMIT => return Err("library.ldf exceeds 16 MiB".into()),
        Ok(_) => inspect(&path, true)?,
    }
    let mut text = String::new();
    fs::File::open(&path)
        .map_err(|e| e.to_string())?
        .take(LIMIT + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() as u64 > LIMIT {
        return Err("library.ldf exceeds 16 MiB".into());
    }
    let library: Library =
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON library.ldf: {e}"))?;
    validate(&library)?;
    Ok(Some(library))
}

pub(crate) fn validate(library: &Library) -> Result<(), String> {
    if library.format != 1 {
        return Err("Unsupported library.ldf format".into());
    }
    uuid::Uuid::parse_str(&library.library_id).map_err(|_| "Invalid library ID")?;
    let mut ids = HashSet::new();
    let mut folders = HashSet::new();
    for entry in &library.games {
        uuid::Uuid::parse_str(&entry.id).map_err(|_| "Invalid game ID in library")?;
        if !ids.insert(&entry.id) || !folders.insert(entry.installation.folder.to_lowercase()) {
            return Err("Duplicate game or directory in library".into());
        }
        crate::database::required_name(entry.name.clone(), "game name")?;
        let folder = crate::archive_install::safe_relative_path(&entry.installation.folder)?;
        if folder.components().count() != 1 || entry.installation.folder.starts_with('.') {
            return Err("Game folder must be one ordinary directory name".into());
        }
        crate::archive_install::safe_relative_path(&entry.installation.executable)?;
        if !["direct", "legacy"].contains(&entry.installation.archive_layout.as_str()) {
            return Err("Invalid archive layout".into());
        }
        if let Some(update) = &entry.update
            && (update.source_sha256.len() != 64
                || !update
                    .source_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || update.entry_id.is_empty()
                || update.entry_id.len() > 256
                || update.installed_version.trim().is_empty()
                || update.installed_version.len() > 128)
        {
            return Err("Invalid update metadata in library".into());
        }
    }
    Ok(())
}

pub(crate) fn save(root: &Path, library: &Library) -> Result<(), String> {
    validate(library)?;
    let mut content = serde_json::to_vec_pretty(library).map_err(|e| e.to_string())?;
    content.push(b'\n');
    if content.len() as u64 > LIMIT {
        return Err("Library manifest exceeds 16 MiB".into());
    }
    let temporary = root.join(format!(".library-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        output
            .write_all(&content)
            .and_then(|()| output.sync_all())
            .map_err(|e| e.to_string())?;
        drop(output);
        replace(&temporary, &root.join(FILE)).map_err(|e| e.to_string())?;
        crate::finalize_install::sync_directory(root)
    })();
    if let Err(error) = result {
        if let Err(cleanup) = fs::remove_file(&temporary)
            && cleanup.kind() != std::io::ErrorKind::NotFound
        {
            return Err(format!(
                "{error}; could not remove library temporary file: {cleanup}"
            ));
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
fn replace(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both buffers are terminated and live throughout the call.
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
