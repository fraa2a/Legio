//! Local screenshots for the game page, read from the Steam screenshots folder of every
//! local Steam account and from one folder per game (Legio data directory by default).
//!
//! Folder contents are untrusted: only regular image files are listed, symbolic links are
//! never followed, the format is sniffed from the bytes and the count and file size are
//! capped. The frontend refers to a screenshot only by an opaque id, never by a path.

use std::{
    fs::{self, File, OpenOptions},
    io::{Cursor, Read},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::{database::DatabaseState, image_format::ImageFormat};

pub(crate) const PAGE_SIZE: usize = 12;
const MAX_LISTED: usize = 500;
const MAX_SCANNED_PER_FOLDER: usize = 5000;
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DIMENSION: u32 = 16384;
const THUMBNAIL_WIDTH: u32 = 480;
const THUMBNAIL_HEIGHT: u32 = 270;
const EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Source {
    Steam,
    Folder,
}

#[derive(Clone, Debug)]
pub(crate) struct Location {
    source: Source,
    // First part of screenshot ids; unique per location.
    prefix: String,
    directory: PathBuf,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScreenshotEntry {
    id: String,
    name: String,
    source: Source,
    modified_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScreenshotPage {
    entries: Vec<ScreenshotEntry>,
    total: usize,
    page_size: usize,
    folder: String,
    custom_folder: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageMetadata {
    content_type: &'static str,
}

struct Candidate {
    entry: ScreenshotEntry,
    path: PathBuf,
}

fn default_folder(app: &AppHandle, game_id: &str) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Could not locate app data: {error}"))?
        .join("game-screenshots")
        .join(game_id))
}

struct Folders {
    folder: PathBuf,
    custom: bool,
    locations: Vec<Location>,
}

fn folders(app: &AppHandle, game_id: &str) -> Result<Folders, String> {
    let state = app.state::<DatabaseState>();
    let database = state.database()?;
    let game = database.game(game_id)?;
    let (folder, custom) = match database.screenshot_folder(game_id)? {
        Some(path) => (PathBuf::from(path), true),
        None => (default_folder(app, &game.id)?, false),
    };
    let mut locations = Vec::new();
    if let Some(app_id) = game.steam_app_id {
        let roots = crate::steam_local::default_steam_roots().unwrap_or_default();
        locations.extend(steam_locations(&roots, app_id));
    }
    locations.push(Location {
        source: Source::Folder,
        prefix: "folder".to_owned(),
        directory: folder.clone(),
    });
    Ok(Folders {
        folder,
        custom,
        locations,
    })
}

/// `userdata/<account>/760/remote/<appid>/screenshots` under every existing Steam root.
fn steam_locations(roots: &[PathBuf], app_id: u32) -> Vec<Location> {
    let mut seen = Vec::new();
    let mut locations = Vec::new();
    for root in roots {
        let Ok(root) = dunce::canonicalize(root) else {
            continue;
        };
        if seen.contains(&root) {
            continue;
        }
        seen.push(root.clone());
        let Ok(accounts) = fs::read_dir(root.join("userdata")) else {
            continue;
        };
        for account in accounts.flatten() {
            let Some(name) = account.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_digit()) {
                continue;
            }
            let directory = account
                .path()
                .join(format!("760/remote/{app_id}/screenshots"));
            let prefix = format!("steam-{name}");
            if !locations
                .iter()
                .any(|known: &Location| known.prefix == prefix)
            {
                locations.push(Location {
                    source: Source::Steam,
                    prefix,
                    directory,
                });
            }
        }
    }
    locations
}

fn has_image_extension(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
}

fn open_regular(path: &Path) -> Result<File, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect the screenshot: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("The screenshot is not a regular file".to_owned());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options
        .open(path)
        .map_err(|error| format!("Could not open the screenshot: {error}"))?;
    let opened = file
        .metadata()
        .map_err(|error| format!("Could not inspect the screenshot: {error}"))?;
    if !opened.is_file() || opened.len() == 0 || opened.len() > MAX_IMAGE_BYTES {
        return Err("The screenshot is empty or exceeds the 32 MiB limit".to_owned());
    }
    Ok(file)
}

fn sniff(path: &Path) -> Option<ImageFormat> {
    let file = open_regular(path).ok()?;
    let mut header = [0; 12];
    let count = file.take(header.len() as u64).read(&mut header).ok()?;
    ImageFormat::from_bytes(&header[..count])
}

fn scan(location: &Location) -> Vec<Candidate> {
    let Ok(entries) = fs::read_dir(&location.directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .take(MAX_SCANNED_PER_FOLDER)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !has_image_extension(&name) {
                return None;
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).ok()?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() == 0
                || metadata.len() > MAX_IMAGE_BYTES
            {
                return None;
            }
            let modified_ms = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .and_then(|duration| i64::try_from(duration.as_millis()).ok())
                .unwrap_or(0);
            Some(Candidate {
                entry: ScreenshotEntry {
                    id: format!("{}/{name}", location.prefix),
                    name,
                    source: location.source,
                    modified_ms,
                },
                path,
            })
        })
        .collect()
}

/// Newest first, at most `MAX_LISTED` valid images, returned as one page.
fn list(locations: &[Location], offset: usize) -> (Vec<ScreenshotEntry>, usize) {
    let mut candidates: Vec<Candidate> = locations.iter().flat_map(scan).collect();
    candidates.sort_by(|left, right| {
        right
            .entry
            .modified_ms
            .cmp(&left.entry.modified_ms)
            .then_with(|| left.entry.id.cmp(&right.entry.id))
    });
    let valid: Vec<ScreenshotEntry> = candidates
        .into_iter()
        .filter(|candidate| sniff(&candidate.path).is_some())
        .take(MAX_LISTED)
        .map(|candidate| candidate.entry)
        .collect();
    let total = valid.len();
    let page = valid.into_iter().skip(offset).take(PAGE_SIZE).collect();
    (page, total)
}

/// Maps an id from the frontend back to a file inside one of the known folders.
fn resolve(locations: &[Location], id: &str) -> Result<PathBuf, String> {
    let (prefix, name) = id
        .split_once('/')
        .ok_or_else(|| "Invalid screenshot id".to_owned())?;
    let location = locations
        .iter()
        .find(|location| location.prefix == prefix)
        .ok_or_else(|| "Unknown screenshot folder".to_owned())?;
    if name.is_empty() || name.contains(['/', '\\']) || name == ".." || !has_image_extension(name) {
        return Err("Invalid screenshot id".to_owned());
    }
    let path = location.directory.join(name);
    if sniff(&path).is_none() {
        return Err("The screenshot is missing or is not a supported image".to_owned());
    }
    Ok(path)
}

fn read_image(path: &Path) -> Result<(Vec<u8>, ImageFormat), String> {
    let file = open_regular(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read the screenshot: {error}"))?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("The screenshot exceeds the 32 MiB limit".to_owned());
    }
    let format = ImageFormat::from_bytes(&bytes)
        .ok_or_else(|| "The screenshot is not a supported PNG, JPEG, or WebP image".to_owned())?;
    Ok((bytes, format))
}

fn thumbnail(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|error| format!("Could not decode the screenshot: {error}"))?;
    let small = decoded
        .thumbnail(THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT)
        .to_rgb8();
    let mut output = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 80)
        .encode_image(&small)
        .map_err(|error| error.to_string())?;
    Ok(output)
}

pub(crate) fn list_page(
    app: &AppHandle,
    game_id: &str,
    offset: usize,
) -> Result<ScreenshotPage, String> {
    let folders = folders(app, game_id)?;
    let (entries, total) = list(&folders.locations, offset);
    Ok(ScreenshotPage {
        entries,
        total,
        page_size: PAGE_SIZE,
        folder: folders.folder.to_string_lossy().into_owned(),
        custom_folder: folders.custom,
    })
}

pub(crate) fn image_response(
    app: &AppHandle,
    game_id: &str,
    id: &str,
    small: bool,
) -> Result<tauri::ipc::Response, String> {
    let path = resolve(&folders(app, game_id)?.locations, id)?;
    let (bytes, format) = read_image(&path)?;
    let (bytes, content_type) = if small {
        (thumbnail(&bytes)?, "image/jpeg")
    } else {
        (bytes, format.content_type())
    };
    let metadata =
        serde_json::to_vec(&ImageMetadata { content_type }).map_err(|error| error.to_string())?;
    crate::image_response::encode(metadata, bytes)
}

/// Folder to show: the one holding the given screenshot, otherwise the per-game folder.
pub(crate) fn folder_to_open(
    app: &AppHandle,
    game_id: &str,
    id: Option<&str>,
) -> Result<PathBuf, String> {
    let folders = folders(app, game_id)?;
    if let Some(id) = id {
        let path = resolve(&folders.locations, id)?;
        return path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "The screenshot has no folder".to_owned());
    }
    fs::create_dir_all(&folders.folder)
        .map_err(|error| format!("Could not create the screenshots folder: {error}"))?;
    Ok(folders.folder)
}

pub(crate) fn screenshot_path(app: &AppHandle, game_id: &str, id: &str) -> Result<PathBuf, String> {
    resolve(&folders(app, game_id)?.locations, id)
}

/// Stores or clears the per-game folder override.
pub(crate) fn set_folder(
    app: &AppHandle,
    game_id: &str,
    folder: Option<&str>,
) -> Result<(), String> {
    let normalized = match folder {
        Some(path) => {
            let path = Path::new(path);
            if !path.is_absolute() {
                return Err("The screenshots folder must be an absolute path".to_owned());
            }
            let canonical = dunce::canonicalize(path)
                .map_err(|error| format!("Could not use the screenshots folder: {error}"))?;
            if !canonical.is_dir() {
                return Err("The screenshots folder is not a directory".to_owned());
            }
            Some(canonical.to_string_lossy().into_owned())
        }
        None => None,
    };
    app.state::<DatabaseState>()
        .database()?
        .set_screenshot_folder(game_id, normalized.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n0000";
    const JPEG: &[u8] = b"\xff\xd8\xff\xe0000000000";

    fn directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!("legio-shots-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn location(directory: &Path) -> Location {
        Location {
            source: Source::Folder,
            prefix: "folder".to_owned(),
            directory: directory.to_owned(),
        }
    }

    fn write(directory: &Path, name: &str, bytes: &[u8], age_seconds: u64) {
        let path = directory.join(name);
        fs::write(&path, bytes).unwrap();
        let time = SystemTime::now() - Duration::from_secs(age_seconds);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(time)
            .unwrap();
    }

    fn names(entries: &[ScreenshotEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    #[test]
    fn lists_newest_first_and_filters_files() {
        let dir = directory();
        write(&dir, "old.png", PNG, 300);
        write(&dir, "new.JPG", JPEG, 10);
        write(&dir, "mid.jpeg", JPEG, 100);
        write(&dir, "notes.txt", PNG, 1);
        write(&dir, "fake.png", b"MZ not an image", 1);
        write(&dir, "empty.png", b"", 1);
        fs::create_dir(dir.join("thumbnails.png")).unwrap();
        let (entries, total) = list(&[location(&dir)], 0);
        assert_eq!(names(&entries), ["new.JPG", "mid.jpeg", "old.png"]);
        assert_eq!(total, 3);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn paginates_and_caps_the_listing() {
        let dir = directory();
        for index in 0..(MAX_LISTED + 5) {
            write(&dir, &format!("{index:04}.png"), PNG, index as u64);
        }
        let locations = [location(&dir)];
        let (first, total) = list(&locations, 0);
        assert_eq!(first.len(), PAGE_SIZE);
        assert_eq!(total, MAX_LISTED);
        assert_eq!(first[0].name, "0000.png");
        let (last, _) = list(&locations, MAX_LISTED - 3);
        assert_eq!(last.len(), 3);
        assert!(list(&locations, MAX_LISTED).0.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_neither_listed_nor_resolved() {
        let dir = directory();
        let outside = directory();
        write(&outside, "secret.png", PNG, 1);
        write(&dir, "real.png", PNG, 5);
        std::os::unix::fs::symlink(outside.join("secret.png"), dir.join("link.png")).unwrap();
        let locations = [location(&dir)];
        assert_eq!(names(&list(&locations, 0).0), ["real.png"]);
        assert!(resolve(&locations, "folder/link.png").is_err());
        assert!(resolve(&locations, "folder/real.png").is_ok());
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn ids_cannot_escape_the_folder() {
        let dir = directory();
        write(&dir, "a.png", PNG, 1);
        let locations = [location(&dir)];
        for id in [
            "folder/../a.png",
            "folder/..\\a.png",
            "folder//etc/passwd",
            "other/a.png",
            "a.png",
            "folder/",
            "folder/a.txt",
        ] {
            assert!(resolve(&locations, id).is_err(), "{id}");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finds_steam_screenshots_for_every_account() {
        let root = directory();
        for account in ["111", "222"] {
            let shots = root.join(format!("userdata/{account}/760/remote/620/screenshots"));
            fs::create_dir_all(&shots).unwrap();
            write(&shots, &format!("{account}.jpg"), JPEG, 5);
        }
        fs::create_dir_all(root.join("userdata/anonymous/760/remote/620/screenshots")).unwrap();
        let locations = steam_locations(&[root.clone(), root.clone()], 620);
        assert_eq!(locations.len(), 2);
        let (entries, total) = list(&locations, 0);
        assert_eq!(total, 2);
        assert!(entries.iter().all(|entry| entry.source == Source::Steam));
        assert!(
            steam_locations(std::slice::from_ref(&root), 999)
                .iter()
                .all(|l| !l.directory.exists())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn thumbnails_are_small_jpeg_images() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1920, 1080)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let output = thumbnail(png.get_ref()).unwrap();
        assert_eq!(ImageFormat::from_bytes(&output), Some(ImageFormat::Jpeg));
        let decoded = image::load_from_memory(&output).unwrap();
        assert!(decoded.width() <= THUMBNAIL_WIDTH && decoded.height() <= THUMBNAIL_HEIGHT);
        assert!(thumbnail(b"garbage").is_err());
    }
}
