use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::{
    database::DatabaseState,
    image_format::ImageFormat,
    image_trim::webp_asset,
    network::{NetworkState, is_steam_asset_url},
    steam_details::{SteamDetails, cached_details},
};

const MAX_ASSET_BYTES: usize = 2 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 4096;
const CACHE_FILES: usize = 1024;
const FRESH_FOR: Duration = Duration::from_secs(72 * 60 * 60);
const ORPHAN_AGE: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Header,
    Capsule,
    Screenshot,
    Hero,
    Logo,
    LibraryCapsule,
    LibraryHeader,
    HeroBlur,
    ClientIcon,
}

impl AssetKind {
    fn library_filename(self, full: bool) -> Option<&'static str> {
        Some(match self {
            Self::Hero if full => "library_hero_2x.jpg",
            Self::Hero => "library_hero.jpg",
            Self::Logo if full => "logo_2x.png",
            Self::Logo => "logo.png",
            Self::LibraryCapsule if full => "library_capsule_2x.jpg",
            Self::LibraryCapsule => "library_capsule.jpg",
            Self::LibraryHeader if full => "library_header_2x.jpg",
            Self::LibraryHeader => "library_header.jpg",
            Self::HeroBlur => "library_hero_blur.jpg",
            Self::ClientIcon => "clienticon.ico",
            Self::Header | Self::Capsule | Self::Screenshot => return None,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetResult {
    #[serde(skip_serializing)]
    bytes: Vec<u8>,
    content_type: &'static str,
    stale: bool,
    refresh_after: u64,
    cache_warning: Option<String>,
}

impl AssetResult {
    pub(crate) fn into_response(self) -> Result<tauri::ipc::Response, String> {
        let metadata = serde_json::to_vec(&self).map_err(|error| error.to_string())?;
        crate::image_response::encode(metadata, self.bytes)
    }
}

#[derive(Deserialize, Serialize)]
struct CacheHeader {
    url: String,
    content_type: String,
    #[serde(default)]
    transform_version: u8,
}

struct CacheEntry {
    bytes: Vec<u8>,
    content_type: &'static str,
    stale: bool,
    refresh_after: u64,
}

#[derive(Clone)]
pub struct AssetCacheState {
    directory: Result<PathBuf, String>,
    lock: Arc<Mutex<Maintenance>>,
    pics_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Default)]
struct Maintenance {
    entries: Option<usize>,
    last_prune: Option<Instant>,
}

impl AssetCacheState {
    pub fn new(directory: Result<PathBuf, tauri::Error>) -> Self {
        Self {
            directory: directory
                .map(|path| path.join("steam-assets"))
                .map_err(|error| {
                    format!("Could not resolve the application cache directory: {error}")
                }),
            lock: Arc::new(Mutex::new(Maintenance::default())),
            pics_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn directory(&self) -> Result<&Path, String> {
        self.directory.as_deref().map_err(Clone::clone)
    }

    fn reset_app(&self, app_id: u32) -> Result<(), String> {
        let mut guard = self
            .lock
            .lock()
            .map_err(|_| "Image cache lock was poisoned")?;
        let directory = self.directory()?;
        let prefix = format!("{app_id}-");
        match fs::read_dir(directory) {
            Ok(entries) => {
                for entry in entries {
                    let entry =
                        entry.map_err(|error| format!("Could not read image cache: {error}"))?;
                    let path = entry.path();
                    if entry.file_name().to_string_lossy().starts_with(&prefix)
                        && path
                            .extension()
                            .is_some_and(|extension| extension == "asset")
                    {
                        fs::remove_file(path)
                            .map_err(|error| format!("Could not reset cached image: {error}"))?;
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Could not read image cache: {error}")),
        }
        match fs::remove_file(directory.join("pics").join(format!("{app_id}.json"))) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Could not reset Steam image metadata: {error}")),
        }
        guard.entries = None;
        Ok(())
    }

    pub(crate) fn maintain(&self) -> Result<(), String> {
        let mut guard = self
            .lock
            .lock()
            .map_err(|_| "Image cache lock was poisoned")?;
        let directory = self.directory()?;
        let entries = match prune(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(format!("Could not prune the image cache: {error}")),
        };
        guard.entries = Some(entries);
        guard.last_prune = Some(Instant::now());
        Ok(())
    }

    fn read(&self, key: &str, url: &str) -> Result<Option<CacheEntry>, String> {
        self.read_cached(key, Some(url))
    }

    fn read_cached(&self, key: &str, url: Option<&str>) -> Result<Option<CacheEntry>, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Image cache lock was poisoned.".to_owned())?;
        let directory = self.directory()?;
        let path = directory.join(format!("{key}.asset"));
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("Could not inspect a cached image: {error}")),
        };
        if !metadata.file_type().is_file() {
            return Err("A cached image is not a regular file.".to_owned());
        }
        if metadata.len() > (MAX_ASSET_BYTES + MAX_HEADER_BYTES) as u64 {
            return Err("A cached image exceeds the size limit.".to_owned());
        }
        let mut raw = Vec::new();
        fs::File::open(path)
            .map_err(|error| format!("Could not open a cached image: {error}"))?
            .take((MAX_ASSET_BYTES + MAX_HEADER_BYTES + 1) as u64)
            .read_to_end(&mut raw)
            .map_err(|error| format!("Could not read a cached image: {error}"))?;
        if raw.len() > MAX_ASSET_BYTES + MAX_HEADER_BYTES {
            return Err("A cached image exceeds the size limit.".to_owned());
        }
        let Some(split) = raw.iter().position(|byte| *byte == b'\n') else {
            return Err("A cached image has an invalid header.".to_owned());
        };
        if split > MAX_HEADER_BYTES {
            return Err("A cached image has an oversized header.".to_owned());
        }
        let header: CacheHeader = serde_json::from_slice(&raw[..split])
            .map_err(|_| "A cached image has an invalid header.".to_owned())?;
        if url.is_some_and(|url| header.url != url) || !matches!(header.transform_version, 1 | 2) {
            return Ok(None);
        }
        raw.drain(..=split);
        let bytes = raw;
        let format = ImageFormat::from_steam_bytes(&bytes)
            .ok_or_else(|| "A cached image has invalid content.".to_owned())?;
        if format.content_type() != header.content_type {
            return Err("A cached image has mismatched content type.".to_owned());
        }
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_none_or(|age| age >= FRESH_FOR)
            || header.transform_version != 2;
        Ok(Some(CacheEntry {
            bytes,
            content_type: format.content_type(),
            stale,
            refresh_after: if stale {
                0
            } else {
                metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map_or(0, |time| (time + FRESH_FOR).as_millis() as u64)
            },
        }))
    }

    fn write(
        &self,
        key: &str,
        url: &str,
        bytes: &[u8],
        content_type: &'static str,
    ) -> Result<(), String> {
        let mut guard = self
            .lock
            .lock()
            .map_err(|_| "Image cache lock was poisoned.".to_owned())?;
        let directory = self.directory()?;
        fs::create_dir_all(directory)
            .map_err(|error| format!("Could not create the image cache: {error}"))?;
        let header = serde_json::to_vec(&CacheHeader {
            url: url.to_owned(),
            transform_version: if content_type == "image/webp" { 2 } else { 1 },
            content_type: content_type.to_owned(),
        })
        .map_err(|error| format!("Could not encode the image cache header: {error}"))?;
        if header.len() > MAX_HEADER_BYTES || bytes.len() > MAX_ASSET_BYTES {
            return Err("Image cache entry exceeds the size limit.".to_owned());
        }
        let temporary = directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Could not write a cached image: {error}"))?;
        file.write_all(&header)
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.write_all(bytes))
            .map_err(|error| format!("Could not write a cached image: {error}"))?;
        drop(file);
        let destination = directory.join(format!("{key}.asset"));
        let replacing = match fs::symlink_metadata(&destination) {
            Ok(_) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => {
                return Err(format!(
                    "Could not inspect the image cache destination: {error}"
                ));
            }
        };
        if let Err(error) = fs::rename(&temporary, destination) {
            if let Err(cleanup) = fs::remove_file(&temporary) {
                eprintln!("Could not remove a temporary image cache file: {cleanup}");
            }
            return Err(format!("Could not save a cached image: {error}"));
        }
        guard.entries = guard
            .entries
            .map(|entries| entries + usize::from(!replacing));
        if guard.entries.is_none_or(|entries| entries > CACHE_FILES)
            || guard
                .last_prune
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(60))
        {
            guard.entries = Some(
                prune(directory)
                    .map_err(|error| format!("Could not prune the image cache: {error}"))?,
            );
            guard.last_prune = Some(Instant::now());
        }
        Ok(())
    }
}

fn prune(directory: &Path) -> io::Result<usize> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "tmp")
        {
            let modified = entry.metadata()?.modified()?;
            if SystemTime::now()
                .duration_since(modified)
                .is_ok_and(|age| age >= ORPHAN_AGE)
            {
                fs::remove_file(entry.path())?;
            }
        } else if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "asset")
        {
            files.push((entry.metadata()?.modified()?, entry.path()));
        }
    }
    files.sort_by_key(|(modified, _)| *modified);
    let retained = files.len().min(CACHE_FILES);
    let remove = files.len().saturating_sub(CACHE_FILES);
    for (_, path) in files.into_iter().take(remove) {
        fs::remove_file(path)?;
    }
    Ok(retained)
}

fn selected_url(
    details: &SteamDetails,
    asset: AssetKind,
    index: Option<usize>,
    full: bool,
) -> Result<String, String> {
    let url = match (asset, index) {
        (AssetKind::Header, None) => details.assets.header.as_deref(),
        (AssetKind::Capsule, None) => details.assets.capsule.as_deref(),
        (AssetKind::Screenshot, Some(index)) => {
            details.assets.screenshots.get(index).and_then(|image| {
                let large = image.full.as_deref();
                if full && large.is_some() {
                    large
                } else {
                    image.thumbnail.as_deref().or(large)
                }
            })
        }
        _ => return Err("Invalid image selection.".to_owned()),
    }
    .ok_or_else(|| "This image is unavailable in cached Steam details.".to_owned())?;
    let parsed = reqwest::Url::parse(url).map_err(|_| "Cached image URL is invalid.".to_owned())?;
    if url.len() > 2048 || url.chars().any(char::is_control) || !is_steam_asset_url(&parsed) {
        return Err("Cached image URL is invalid.".to_owned());
    }
    Ok(url.to_owned())
}

static IMAGE_WORKERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

async fn optimized_asset_bytes(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let permit = IMAGE_WORKERS
        .acquire()
        .await
        .map_err(|error| error.to_string())?;
    let result = tauri::async_runtime::spawn_blocking(move || webp_asset(&bytes))
        .await
        .map_err(|error| format!("Artwork processing task failed: {error}"))?;
    drop(permit);
    result
}

async fn load_asset(
    cache: AssetCacheState,
    network: &NetworkState,
    key: String,
    url: String,
    refresh: bool,
) -> Result<AssetResult, String> {
    let reader = cache.clone();
    let read_key = key.clone();
    let read_url = url.clone();
    let read = tauri::async_runtime::spawn_blocking(move || reader.read(&read_key, &read_url))
        .await
        .map_err(|error| format!("Image cache read task failed: {error}"))?;
    let (previous, read_error) = match read {
        Ok(entry) => (entry, None),
        Err(error) => {
            eprintln!("Steam image cache read failed: {error}");
            (None, Some(error))
        }
    };
    if !refresh && previous.as_ref().is_some_and(|entry| !entry.stale) {
        let entry = previous.ok_or_else(|| "Image cache entry disappeared.".to_owned())?;
        return Ok(AssetResult {
            bytes: entry.bytes,
            content_type: entry.content_type,
            stale: false,
            refresh_after: entry.refresh_after,
            cache_warning: None,
        });
    }
    let fetched = network
        .steam_asset(&url)
        .await
        .map_err(|error| format!("Steam image request failed: {error:?}"))
        .and_then(|bytes| {
            ImageFormat::from_steam_bytes(&bytes)
                .map(|_| bytes)
                .ok_or_else(|| "Steam returned unsupported image content.".to_owned())
        });
    let fetched = match fetched {
        Ok(bytes) => optimized_asset_bytes(bytes).await,
        Err(error) => Err(error),
    };
    match fetched {
        Ok(bytes) => {
            let content_type = "image/webp";
            let writer = cache.clone();
            let write_key = key;
            let write_url = url;
            let (bytes, write_result) = tauri::async_runtime::spawn_blocking(move || {
                let result = writer.write(&write_key, &write_url, &bytes, content_type);
                (bytes, result)
            })
            .await
            .map_err(|error| format!("Image cache write task failed: {error}"))?;
            Ok(AssetResult {
                bytes,
                content_type,
                stale: false,
                refresh_after: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map_or(0, |time| (time + FRESH_FOR).as_millis() as u64),
                cache_warning: write_result.err().map(|error| {
                    format!("Image is visible, but could not be saved locally: {error}")
                }),
            })
        }
        Err(error) => previous
            .map(|entry| AssetResult {
                bytes: entry.bytes,
                content_type: entry.content_type,
                stale: true,
                refresh_after: 0,
                cache_warning: Some(error.clone()),
            })
            .ok_or_else(|| match read_error {
                Some(cache_error) => {
                    format!("{error} Cached image could not be used: {cache_error}")
                }
                None => error,
            }),
    }
}

fn library_url(
    app_id: u32,
    filename: &str,
    assets: Option<&crate::steam_pics::PicsAssets>,
) -> Result<String, String> {
    let portrait = match filename {
        "library_capsule.jpg" => "library_600x900.jpg",
        "library_capsule_2x.jpg" => "library_600x900_2x.jpg",
        filename => filename,
    };
    if let Some(url) = assets.and_then(|assets| {
        assets
            .url(app_id, filename)
            .or_else(|| assets.url(app_id, portrait))
    }) {
        Ok(url)
    } else if filename == "clienticon.ico" {
        Err("Steam did not provide a client icon for this App ID.".to_owned())
    } else {
        Ok(format!(
            "https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/{portrait}"
        ))
    }
}

pub async fn get_asset<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    network: &NetworkState,
    app_id: u32,
    asset: AssetKind,
    index: Option<usize>,
    full: bool,
    refresh: bool,
) -> Result<AssetResult, String> {
    if app_id == 0 || (index.is_some() != matches!(asset, AssetKind::Screenshot)) {
        return Err("Invalid image selection.".to_owned());
    }
    let state = app.state::<AssetCacheState>().inner().clone();
    let filename = asset.library_filename(full);
    let suffix = if full { "-full" } else { "" };
    let kind = match asset {
        AssetKind::Header => "header",
        AssetKind::Capsule => "capsule",
        AssetKind::Hero => "hero",
        AssetKind::Logo => "logo",
        AssetKind::Screenshot => "screenshot",
        _ => filename.ok_or("Invalid image selection.")?,
    };
    let key = match index {
        Some(index) => format!("{app_id}-{kind}-{index}{suffix}"),
        None => format!("{app_id}-{kind}{suffix}"),
    };
    if !refresh {
        let reader = state.clone();
        let read_key = key.clone();
        let cached =
            tauri::async_runtime::spawn_blocking(move || reader.read_cached(&read_key, None))
                .await
                .map_err(|error| format!("Image cache read task failed: {error}"))?;
        match cached {
            Ok(Some(entry)) => {
                return Ok(AssetResult {
                    bytes: entry.bytes,
                    content_type: entry.content_type,
                    stale: entry.stale,
                    refresh_after: entry.refresh_after,
                    cache_warning: None,
                });
            }
            Ok(None) => {}
            Err(error) => eprintln!("Steam image cache read failed: {error}"),
        }
    }
    let (selected, metadata_warning) = if let Some(filename) = filename {
        let path = state
            .directory()
            .map(|directory| directory.join("pics").join(format!("{app_id}.json")));
        let (assets, warning) =
            crate::steam_pics::resolve(path, &state.pics_lock, crate::steam_pics::fetch(app_id))
                .await;
        if let Some(warning) = warning.as_deref() {
            eprintln!("Steam asset metadata lookup for {app_id}: {warning}");
        }
        (library_url(app_id, filename, assets.as_ref()), warning)
    } else {
        let app_for_lookup = app.clone();
        let url = tauri::async_runtime::spawn_blocking(move || {
            let state = app_for_lookup.state::<DatabaseState>();
            let database = state.database()?;
            let details = cached_details(database, app_id)
                .map_err(|error| error.message)?
                .ok_or_else(|| {
                    "No cached Steam details are available for this App ID.".to_owned()
                })?;
            selected_url(&details, asset, index, full)
        })
        .await
        .map_err(|error| format!("Image lookup task failed: {error}"))?;
        (url, None)
    };
    let loaded = match selected {
        Ok(url) => load_asset(state, network, key, url, refresh).await,
        Err(error) => Err(error),
    };
    let mut result = loaded.map_err(|error| match metadata_warning.as_deref() {
        Some(warning) => format!("{error} Steam asset metadata: {warning}"),
        None => error,
    })?;
    if let Some(warning) = metadata_warning {
        result.cache_warning = Some(match result.cache_warning {
            Some(existing) => format!("{existing} Steam asset metadata: {warning}"),
            None => format!("Steam asset metadata: {warning}"),
        });
    }
    Ok(result)
}

pub(crate) async fn reset(app: tauri::AppHandle, steam_app_id: u32) -> Result<(), String> {
    if steam_app_id == 0 {
        return Err("Invalid Steam App ID".to_owned());
    }
    let state = app.state::<AssetCacheState>().inner().clone();
    let _metadata = state.pics_lock.lock().await;
    let cache = state.clone();
    tauri::async_runtime::spawn_blocking(move || cache.reset_app(steam_app_id))
        .await
        .map_err(|error| format!("Image cache reset task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resetting_game_images_removes_only_the_selected_apps_assets_and_metadata() {
        let cache = cache();
        cache
            .write(
                "400-logo",
                "https://cdn.cloudflare.steamstatic.com/steam/apps/400/logo.png",
                PNG,
                "image/png",
            )
            .unwrap();
        cache
            .write(
                "400-header",
                "https://cdn.cloudflare.steamstatic.com/steam/apps/400/header.jpg",
                PNG,
                "image/png",
            )
            .unwrap();
        cache
            .write(
                "4000-logo",
                "https://cdn.cloudflare.steamstatic.com/steam/apps/4000/logo.png",
                PNG,
                "image/png",
            )
            .unwrap();
        let directory = cache.directory().unwrap();
        fs::create_dir_all(directory.join("pics")).unwrap();
        fs::write(directory.join("pics/400.json"), "{}").unwrap();
        fs::write(directory.join("pics/4000.json"), "{}").unwrap();
        cache.reset_app(400).unwrap();
        assert!(cache.read_cached("400-logo", None).unwrap().is_none());
        assert!(cache.read_cached("400-header", None).unwrap().is_none());
        assert!(cache.read_cached("4000-logo", None).unwrap().is_some());
        assert!(!directory.join("pics/400.json").exists());
        assert!(directory.join("pics/4000.json").exists());
        fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    }
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 2,
        0, 0, 0, 253, 212, 154, 115, 0, 0, 0, 22, 73, 68, 65, 84, 120, 156, 99, 148, 11, 232, 97,
        96, 96, 96, 98, 96, 96, 96, 96, 96, 0, 0, 11, 116, 0, 254, 223, 47, 23, 202, 0, 0, 0, 0,
        73, 69, 78, 68, 174, 66, 96, 130,
    ];

    fn cache() -> AssetCacheState {
        let directory =
            std::env::temp_dir().join(format!("legio-asset-test-{}", uuid::Uuid::new_v4()));
        AssetCacheState::new(Ok(directory))
    }

    #[test]
    fn cache_hits_do_not_run_directory_maintenance() {
        let cache = cache();
        cache
            .write(
                "400-logo",
                "https://example.invalid/logo.png",
                PNG,
                "image/png",
            )
            .unwrap();
        let orphan = cache.directory().unwrap().join("old.tmp");
        fs::write(&orphan, b"orphan").unwrap();
        fs::File::options()
            .write(true)
            .open(&orphan)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(SystemTime::now() - ORPHAN_AGE - Duration::from_secs(1)),
            )
            .unwrap();
        assert!(cache.read_cached("400-logo", None).unwrap().is_some());
        assert!(orphan.exists());
        cache.maintain().unwrap();
        assert!(!orphan.exists());
        fs::remove_dir_all(cache.directory().unwrap().parent().unwrap()).unwrap();
    }

    fn server(response: Vec<u8>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/asset", listener.local_addr().unwrap());
        let thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            stream.write_all(&response).unwrap();
        });
        (url, thread)
    }

    fn response(body: &[u8]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        response
    }

    #[tokio::test]
    async fn cached_artwork_bypasses_locked_metadata_lookup() {
        let cache = cache();
        let root = cache.directory().unwrap().parent().unwrap();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = format!("org.legio.test.{}", uuid::Uuid::new_v4());
        let app = tauri::test::mock_builder()
            .manage(DatabaseState::new(Ok(root.join("data"))))
            .manage(cache.clone())
            .build(context)
            .unwrap();
        let url =
            "https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/400/header.jpg";
        let details = serde_json::json!({
            "steamAppId": 400, "name": "Portal", "appType": "game",
            "developers": [], "publishers": [], "genres": [],
            "assets": { "header": url, "capsule": url, "background": null,
                "screenshots": [{ "thumbnail": url, "full": url }] }
        })
        .to_string();
        app.state::<DatabaseState>()
            .database()
            .unwrap()
            .with_connection(|connection| {
                connection
                    .execute(
                        "INSERT INTO steam_details_cache (steam_app_id, details, fetched_at) VALUES (400, ?1, 0)",
                        [details],
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let _guard = cache.pics_lock.lock().await;
        for (asset, index, key) in [
            (AssetKind::Hero, None, "400-hero"),
            (AssetKind::Header, None, "400-header"),
            (AssetKind::Capsule, None, "400-capsule"),
            (AssetKind::Screenshot, Some(0), "400-screenshot-0"),
        ] {
            cache.write(key, url, PNG, "image/png").unwrap();
            let image = tokio::time::timeout(
                Duration::from_secs(3),
                get_asset(
                    app.handle().clone(),
                    &network,
                    400,
                    asset,
                    index,
                    false,
                    false,
                ),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(image.bytes, PNG);
            assert!(image.cache_warning.is_none());
        }
        assert!(!cache.directory().unwrap().join("pics").exists());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn library_assets_preserve_legacy_urls_and_blurred_hero_selection() {
        let filename = AssetKind::HeroBlur.library_filename(false).unwrap();
        assert_eq!(filename, "library_hero_blur.jpg");
        assert_eq!(
            library_url(400, filename, None).unwrap(),
            "https://cdn.cloudflare.steamstatic.com/steam/apps/400/library_hero_blur.jpg"
        );
        assert_eq!(
            library_url(400, AssetKind::Logo.library_filename(true).unwrap(), None).unwrap(),
            "https://cdn.cloudflare.steamstatic.com/steam/apps/400/logo_2x.png"
        );
        assert!(library_url(400, "clienticon.ico", None).is_err());
    }

    #[test]
    fn library_capsules_use_legacy_portrait_filenames_without_pics() {
        for (full, filename) in [
            (false, "library_600x900.jpg"),
            (true, "library_600x900_2x.jpg"),
        ] {
            assert_eq!(
                library_url(
                    400,
                    AssetKind::LibraryCapsule.library_filename(full).unwrap(),
                    None,
                )
                .unwrap(),
                format!("https://cdn.cloudflare.steamstatic.com/steam/apps/400/{filename}")
            );
        }
    }

    #[tokio::test]
    #[ignore = "Requires Steam CM and CDN connectivity"]
    async fn live_modern_logo_download_is_cached() {
        let cache = cache();
        let assets = crate::steam_pics::fetch(4656000).await.unwrap();
        let url = assets.url(4656000, "logo.png").unwrap();
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let first = load_asset(
            cache.clone(),
            &network,
            "4656000-logo".into(),
            url.clone(),
            false,
        )
        .await
        .unwrap();
        let second = cache.read("4656000-logo", &url).unwrap().unwrap();
        assert_eq!(first.bytes, second.bytes);
        assert_eq!(first.content_type, "image/webp");
        assert!(!second.stale);
        fs::remove_dir_all(cache.directory().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn cache_miss_fetches_once_then_serves_local_bytes() {
        let cache = cache();
        let (url, server) = server(response(PNG));
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        assert!(cache.read("400-header", &url).unwrap().is_none());
        let first = tauri::async_runtime::block_on(load_asset(
            cache.clone(),
            &network,
            "400-header".into(),
            url.clone(),
            false,
        ))
        .unwrap();
        server.join().unwrap();
        let second = tauri::async_runtime::block_on(load_asset(
            cache.clone(),
            &network,
            "400-header".into(),
            url,
            false,
        ))
        .unwrap();
        assert_eq!(first.bytes, second.bytes);
        assert_eq!(first.content_type, "image/webp");
        assert!(image::load_from_memory(&first.bytes).is_ok());
        assert!(!second.stale);
        fs::remove_dir_all(cache.directory().unwrap()).unwrap();
    }

    #[test]
    fn stale_image_is_returned_when_refresh_has_invalid_content() {
        let cache = cache();
        let (url, server) = server(response(b"<html>not an image</html>"));
        cache.write("400-header", &url, PNG, "image/png").unwrap();
        let path = cache.directory().unwrap().join("400-header.asset");
        let old = SystemTime::now() - FRESH_FOR - Duration::from_secs(1);
        fs::File::open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let result = tauri::async_runtime::block_on(load_asset(
            cache.clone(),
            &network,
            "400-header".into(),
            url,
            false,
        ))
        .unwrap();
        server.join().unwrap();
        assert_eq!(result.bytes, PNG);
        assert!(result.stale);
        fs::remove_dir_all(cache.directory().unwrap()).unwrap();
    }

    #[test]
    fn stale_image_is_returned_when_refresh_is_offline() {
        let cache = cache();
        let unavailable =
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let (url, server) = server(unavailable.to_vec());
        cache.write("400-header", &url, PNG, "image/png").unwrap();
        let path = cache.directory().unwrap().join("400-header.asset");
        let old = SystemTime::now() - FRESH_FOR - Duration::from_secs(1);
        fs::File::open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();

        let result = tauri::async_runtime::block_on(load_asset(
            cache.clone(),
            &network,
            "400-header".into(),
            url,
            false,
        ))
        .unwrap();
        server.join().unwrap();
        assert_eq!(result.bytes, PNG);
        assert!(result.stale);
        fs::remove_dir_all(cache.directory().unwrap()).unwrap();
    }

    #[test]
    fn rejects_untrusted_url_and_invalid_selection() {
        let details: SteamDetails = serde_json::from_value(serde_json::json!({
            "steamAppId": 400, "name": "Portal", "appType": "game", "shortDescription": null,
            "developers": [], "publishers": [], "genres": [], "platforms": null, "releaseDate": null,
            "assets": {"header": "https://steamstatic.com.evil.test/image.jpg", "capsule": null,
                "background": null, "screenshots": []}
        })).unwrap();
        assert!(selected_url(&details, AssetKind::Header, None, false).is_err());
        assert!(selected_url(&details, AssetKind::Screenshot, Some(0), false).is_err());
        assert!(selected_url(&details, AssetKind::Capsule, Some(0), false).is_err());
    }

    #[test]
    fn full_screenshot_selection_prefers_the_large_image() {
        let details: SteamDetails = serde_json::from_value(serde_json::json!({
            "steamAppId": 400, "name": "Portal", "appType": "game", "shortDescription": null,
            "developers": [], "publishers": [], "genres": [], "platforms": null, "releaseDate": null,
            "assets": {"header": null, "capsule": null, "background": null,
                "screenshots": [{"thumbnail": "https://cdn.steamstatic.com/400/ss_small.jpg",
                    "full": "https://cdn.steamstatic.com/400/ss.jpg"},
                    {"thumbnail": "https://cdn.steamstatic.com/400/ss2_small.jpg"}]}
        })).unwrap();
        assert_eq!(
            selected_url(&details, AssetKind::Screenshot, Some(0), false).unwrap(),
            "https://cdn.steamstatic.com/400/ss_small.jpg"
        );
        assert_eq!(
            selected_url(&details, AssetKind::Screenshot, Some(0), true).unwrap(),
            "https://cdn.steamstatic.com/400/ss.jpg"
        );
        assert_eq!(
            selected_url(&details, AssetKind::Screenshot, Some(1), true).unwrap(),
            "https://cdn.steamstatic.com/400/ss2_small.jpg"
        );
    }

    #[test]
    fn rejects_remote_non_image_and_oversize() {
        let cache = cache();
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let (url, first_server) = server(response(b"<svg></svg>"));
        assert!(
            tauri::async_runtime::block_on(load_asset(
                cache.clone(),
                &network,
                "400-header".into(),
                url,
                false
            ))
            .is_err()
        );
        first_server.join().unwrap();

        let oversized = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_ASSET_BYTES + 1
        )
        .into_bytes();
        let (url, server) = server(oversized);
        assert_eq!(
            tauri::async_runtime::block_on(network.steam_asset(&url)).unwrap_err(),
            crate::network::NetworkError::TooLarge
        );
        server.join().unwrap();
    }

    #[test]
    fn webp_cache_survives_reopen_and_expires_after_72_hours() {
        let cache = cache();
        let bytes = webp_asset(PNG).unwrap();
        let url = "https://steamstatic.com/image.jpg";
        cache
            .write("400-header", url, &bytes, "image/webp")
            .unwrap();
        let reopened = AssetCacheState::new(Ok(cache
            .directory()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()));
        let path = cache.directory().unwrap().join("400-header.asset");
        for (hours, stale) in [(71, false), (73, true)] {
            fs::File::open(&path)
                .unwrap()
                .set_times(
                    fs::FileTimes::new()
                        .set_modified(SystemTime::now() - Duration::from_secs(hours * 60 * 60)),
                )
                .unwrap();
            let entry = reopened.read_cached("400-header", None).unwrap().unwrap();
            assert_eq!(entry.bytes, bytes);
            assert_eq!(entry.stale, stale);
        }
        fs::remove_dir_all(cache.directory().unwrap()).unwrap();
    }

    #[test]
    fn cache_is_limited_to_128_files() {
        let cache = cache();
        for index in 0..=CACHE_FILES {
            cache
                .write(
                    &format!("400-screenshot-{index}"),
                    "https://steamstatic.com/image.jpg",
                    PNG,
                    "image/png",
                )
                .unwrap();
        }
        let count = fs::read_dir(cache.directory().unwrap()).unwrap().count();
        assert_eq!(count, CACHE_FILES);
        fs::remove_dir_all(cache.directory().unwrap()).unwrap();
    }

    #[test]
    fn maintenance_removes_expired_orphans_without_pruning_cache_hits() {
        let cache = cache();
        cache
            .write(
                "400-header",
                "https://steamstatic.com/image.jpg",
                PNG,
                "image/png",
            )
            .unwrap();
        let orphan = cache.directory().unwrap().join("orphan.tmp");
        fs::write(&orphan, b"unfinished").unwrap();
        assert!(
            cache
                .read("400-header", "https://steamstatic.com/image.jpg")
                .unwrap()
                .is_some()
        );
        assert!(orphan.exists());
        let old = SystemTime::now() - ORPHAN_AGE - Duration::from_secs(1);
        fs::File::open(&orphan)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();
        assert!(
            cache
                .read("400-header", "https://steamstatic.com/image.jpg")
                .unwrap()
                .is_some()
        );
        assert!(orphan.exists());
        cache.maintain().unwrap();
        assert!(!orphan.exists());
        fs::remove_dir_all(cache.directory().unwrap()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_cache_entry() {
        let cache = cache();
        cache
            .write(
                "400-header",
                "https://steamstatic.com/image.jpg",
                PNG,
                "image/png",
            )
            .unwrap();
        std::os::unix::fs::symlink(
            cache.directory().unwrap().join("400-header.asset"),
            cache.directory().unwrap().join("400-capsule.asset"),
        )
        .unwrap();
        assert!(
            cache
                .read("400-capsule", "https://steamstatic.com/image.jpg")
                .is_err()
        );
        fs::remove_dir_all(cache.directory().unwrap()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn broken_symlink_does_not_hide_other_cached_images() {
        let cache = cache();
        let url = "https://steamstatic.com/image.jpg";
        cache.write("400-header", url, PNG, "image/png").unwrap();
        let directory = cache.directory().unwrap();
        std::os::unix::fs::symlink(
            directory.join("missing.asset"),
            directory.join("broken.asset"),
        )
        .unwrap();
        assert!(cache.read("400-header", url).unwrap().is_some());
        cache.write("400-capsule", url, PNG, "image/png").unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupt_or_oversized_cache_is_replaced_by_valid_network_image() {
        for invalid in [
            b"broken".to_vec(),
            vec![b'x'; MAX_ASSET_BYTES + MAX_HEADER_BYTES + 1],
        ] {
            let cache = cache();
            let (url, server) = server(response(PNG));
            cache.write("400-header", &url, PNG, "image/png").unwrap();
            fs::write(cache.directory().unwrap().join("400-header.asset"), invalid).unwrap();
            let network = NetworkState::new(
                "0.1.0",
                crate::diagnostics::Diagnostics::new(Err("test".into())),
            )
            .unwrap();
            let result = tauri::async_runtime::block_on(load_asset(
                cache.clone(),
                &network,
                "400-header".into(),
                url.clone(),
                false,
            ))
            .unwrap();
            server.join().unwrap();
            assert_eq!(result.content_type, "image/webp");
            assert!(cache.read("400-header", &url).unwrap().is_some());
            fs::remove_dir_all(cache.directory().unwrap()).unwrap();
        }
    }

    #[test]
    fn failed_refill_reports_both_network_and_cache_errors() {
        let cache = cache();
        let (url, server) = server(response(b"not an image"));
        cache.write("400-header", &url, PNG, "image/png").unwrap();
        fs::write(
            cache.directory().unwrap().join("400-header.asset"),
            b"broken",
        )
        .unwrap();
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let error = tauri::async_runtime::block_on(load_asset(
            cache.clone(),
            &network,
            "400-header".into(),
            url,
            false,
        ))
        .unwrap_err();
        server.join().unwrap();
        assert!(error.contains("unsupported image content"));
        assert!(error.contains("Cached image could not be used"));
        fs::remove_dir_all(cache.directory().unwrap()).unwrap();
    }

    #[test]
    fn cache_write_failure_returns_image_with_warning() {
        let cache = cache();
        let directory = cache.directory().unwrap();
        fs::create_dir_all(directory.parent().unwrap()).unwrap();
        fs::write(directory, b"not a directory").unwrap();
        let (url, server) = server(response(PNG));
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let result = tauri::async_runtime::block_on(load_asset(
            cache.clone(),
            &network,
            "400-header".into(),
            url,
            false,
        ))
        .unwrap();
        server.join().unwrap();
        assert_eq!(result.content_type, "image/webp");
        assert!(
            result
                .cache_warning
                .as_deref()
                .is_some_and(|message| message.contains("could not be saved locally"))
        );
        fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    }
}
