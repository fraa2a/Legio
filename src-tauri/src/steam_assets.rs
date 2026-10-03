use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::{
    database::DatabaseState,
    image_format::ImageFormat,
    image_trim::trim_png,
    network::{NetworkState, is_steam_asset_url},
    steam_details::{SteamDetails, cached_details},
};

const MAX_ASSET_BYTES: usize = 2 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 4096;
const CACHE_FILES: usize = 64;
const FRESH_FOR: Duration = Duration::from_secs(24 * 60 * 60);
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
    bytes: Vec<u8>,
    content_type: &'static str,
    stale: bool,
    cache_warning: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct CacheHeader {
    url: String,
    content_type: String,
}

struct CacheEntry {
    bytes: Vec<u8>,
    content_type: &'static str,
    stale: bool,
}

#[derive(Clone)]
pub struct AssetCacheState {
    directory: Result<PathBuf, String>,
    lock: Arc<Mutex<()>>,
    pics_lock: Arc<tokio::sync::Mutex<()>>,
}

impl AssetCacheState {
    pub fn new(directory: Result<PathBuf, tauri::Error>) -> Self {
        Self {
            directory: directory
                .map(|path| path.join("steam-assets"))
                .map_err(|error| {
                    format!("Could not resolve the application cache directory: {error}")
                }),
            lock: Arc::new(Mutex::new(())),
            pics_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn directory(&self) -> Result<&Path, String> {
        self.directory.as_deref().map_err(Clone::clone)
    }

    fn read(&self, key: &str, url: &str) -> Result<Option<CacheEntry>, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Image cache lock was poisoned.".to_owned())?;
        let directory = self.directory()?;
        match fs::metadata(directory) {
            Ok(_) => prune(directory)
                .map_err(|error| format!("Could not prune the image cache: {error}"))?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("Could not inspect the image cache: {error}")),
        }
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
        if header.url != url {
            return Ok(None);
        }
        let bytes = raw[split + 1..].to_vec();
        let format = ImageFormat::from_steam_bytes(&bytes)
            .ok_or_else(|| "A cached image has invalid content.".to_owned())?;
        if format.content_type() != header.content_type {
            return Err("A cached image has mismatched content type.".to_owned());
        }
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_none_or(|age| age >= FRESH_FOR);
        Ok(Some(CacheEntry {
            bytes,
            content_type: format.content_type(),
            stale,
        }))
    }

    fn write(
        &self,
        key: &str,
        url: &str,
        bytes: &[u8],
        content_type: &'static str,
    ) -> Result<(), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Image cache lock was poisoned.".to_owned())?;
        let directory = self.directory()?;
        fs::create_dir_all(directory)
            .map_err(|error| format!("Could not create the image cache: {error}"))?;
        let header = serde_json::to_vec(&CacheHeader {
            url: url.to_owned(),
            content_type: content_type.to_owned(),
        })
        .map_err(|error| format!("Could not encode the image cache header: {error}"))?;
        if header.len() > MAX_HEADER_BYTES || bytes.len() > MAX_ASSET_BYTES {
            return Err("Image cache entry exceeds the size limit.".to_owned());
        }
        let mut data = Vec::with_capacity(header.len() + 1 + bytes.len());
        data.extend_from_slice(&header);
        data.push(b'\n');
        data.extend_from_slice(bytes);
        let temporary = directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
        fs::write(&temporary, data)
            .map_err(|error| format!("Could not write a cached image: {error}"))?;
        let destination = directory.join(format!("{key}.asset"));
        if let Err(error) = fs::rename(&temporary, destination) {
            if let Err(cleanup) = fs::remove_file(&temporary) {
                eprintln!("Could not remove a temporary image cache file: {cleanup}");
            }
            return Err(format!("Could not save a cached image: {error}"));
        }
        prune(directory).map_err(|error| format!("Could not prune the image cache: {error}"))
    }
}

fn prune(directory: &Path) -> io::Result<()> {
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
    let remove = files.len().saturating_sub(CACHE_FILES);
    for (_, path) in files.into_iter().take(remove) {
        fs::remove_file(path)?;
    }
    Ok(())
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

fn trimmed_asset_bytes(bytes: Vec<u8>, content_type: &str) -> Vec<u8> {
    if content_type != "image/png" {
        return bytes;
    }
    match trim_png(&bytes) {
        Ok(trimmed) => trimmed,
        Err(error) => {
            eprintln!("Steam image trim failed: {error}");
            bytes
        }
    }
}

async fn load_asset(
    cache: AssetCacheState,
    network: &NetworkState,
    key: String,
    url: String,
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
    if previous.as_ref().is_some_and(|entry| !entry.stale) {
        let entry = previous.ok_or_else(|| "Image cache entry disappeared.".to_owned())?;
        return Ok(AssetResult {
            bytes: trimmed_asset_bytes(entry.bytes, entry.content_type),
            content_type: entry.content_type,
            stale: false,
            cache_warning: None,
        });
    }
    let fetched = network
        .steam_asset(&url)
        .await
        .map_err(|error| format!("Steam image request failed: {error:?}"))
        .and_then(|bytes| {
            ImageFormat::from_steam_bytes(&bytes)
                .map(|format| (bytes, format.content_type()))
                .ok_or_else(|| "Steam returned unsupported image content.".to_owned())
        });
    match fetched {
        Ok((bytes, content_type)) => {
            let bytes = trimmed_asset_bytes(bytes, content_type);
            let writer = cache.clone();
            let write_key = key;
            let write_url = url;
            let write_bytes = bytes.clone();
            let write_result = tauri::async_runtime::spawn_blocking(move || {
                writer.write(&write_key, &write_url, &write_bytes, content_type)
            })
            .await
            .map_err(|error| format!("Image cache write task failed: {error}"))
            .and_then(|result| result);
            Ok(AssetResult {
                bytes,
                content_type,
                stale: false,
                cache_warning: write_result.err().map(|error| {
                    format!("Image is visible, but could not be saved locally: {error}")
                }),
            })
        }
        Err(error) => previous
            .map(|entry| AssetResult {
                bytes: trimmed_asset_bytes(entry.bytes, entry.content_type),
                content_type: entry.content_type,
                stale: true,
                cache_warning: None,
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
        Ok(url) => load_asset(state, network, key, url).await,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    const JPEG: &[u8] = b"\xff\xd8\xffimage";

    fn cache() -> AssetCacheState {
        let directory =
            std::env::temp_dir().join(format!("legio-asset-test-{}", uuid::Uuid::new_v4()));
        AssetCacheState::new(Ok(directory))
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
    async fn store_assets_load_from_cache_while_pics_lookup_is_locked() {
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
                        "INSERT INTO steam_details_cache VALUES (400, ?1, 0)",
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
            (AssetKind::Header, None, "400-header"),
            (AssetKind::Capsule, None, "400-capsule"),
            (AssetKind::Screenshot, Some(0), "400-screenshot-0"),
        ] {
            cache.write(key, url, JPEG, "image/jpeg").unwrap();
            let image = tokio::time::timeout(
                Duration::from_secs(3),
                get_asset(app.handle().clone(), &network, 400, asset, index, false),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(image.bytes, JPEG);
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
        let first = load_asset(cache.clone(), &network, "4656000-logo".into(), url.clone())
            .await
            .unwrap();
        let second = cache.read("4656000-logo", &url).unwrap().unwrap();
        assert_eq!(first.bytes, second.bytes);
        assert_eq!(first.content_type, "image/png");
        assert!(!second.stale);
        fs::remove_dir_all(cache.directory().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn cache_miss_fetches_once_then_serves_local_bytes() {
        let cache = cache();
        let (url, server) = server(response(JPEG));
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
        ))
        .unwrap();
        server.join().unwrap();
        let second = tauri::async_runtime::block_on(load_asset(
            cache.clone(),
            &network,
            "400-header".into(),
            url,
        ))
        .unwrap();
        assert_eq!(first.bytes, JPEG);
        assert_eq!(second.bytes, JPEG);
        assert!(!second.stale);
        fs::remove_dir_all(cache.directory().unwrap()).unwrap();
    }

    #[test]
    fn stale_image_is_returned_when_refresh_has_invalid_content() {
        let cache = cache();
        let (url, server) = server(response(b"<html>not an image</html>"));
        cache.write("400-header", &url, JPEG, "image/jpeg").unwrap();
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
        ))
        .unwrap();
        server.join().unwrap();
        assert_eq!(result.bytes, JPEG);
        assert!(result.stale);
        fs::remove_dir_all(cache.directory().unwrap()).unwrap();
    }

    #[test]
    fn stale_image_is_returned_when_refresh_is_offline() {
        let cache = cache();
        let unavailable =
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let (url, server) = server(unavailable.to_vec());
        cache.write("400-header", &url, JPEG, "image/jpeg").unwrap();
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
        ))
        .unwrap();
        server.join().unwrap();
        assert_eq!(result.bytes, JPEG);
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
                url
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
    fn cache_is_limited_to_sixty_four_files() {
        let cache = cache();
        for index in 0..=CACHE_FILES {
            cache
                .write(
                    &format!("400-screenshot-{index}"),
                    "https://steamstatic.com/image.jpg",
                    JPEG,
                    "image/jpeg",
                )
                .unwrap();
        }
        let count = fs::read_dir(cache.directory().unwrap()).unwrap().count();
        assert_eq!(count, CACHE_FILES);
        fs::remove_dir_all(cache.directory().unwrap()).unwrap();
    }

    #[test]
    fn removes_orphan_temporary_files_before_cache_read() {
        let cache = cache();
        cache
            .write(
                "400-header",
                "https://steamstatic.com/image.jpg",
                JPEG,
                "image/jpeg",
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
                JPEG,
                "image/jpeg",
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
        cache.write("400-header", url, JPEG, "image/jpeg").unwrap();
        let directory = cache.directory().unwrap();
        std::os::unix::fs::symlink(
            directory.join("missing.asset"),
            directory.join("broken.asset"),
        )
        .unwrap();
        assert!(cache.read("400-header", url).unwrap().is_some());
        cache.write("400-capsule", url, JPEG, "image/jpeg").unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupt_or_oversized_cache_is_replaced_by_valid_network_image() {
        for invalid in [
            b"broken".to_vec(),
            vec![b'x'; MAX_ASSET_BYTES + MAX_HEADER_BYTES + 1],
        ] {
            let cache = cache();
            let (url, server) = server(response(JPEG));
            cache.write("400-header", &url, JPEG, "image/jpeg").unwrap();
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
            ))
            .unwrap();
            server.join().unwrap();
            assert_eq!(result.bytes, JPEG);
            assert!(cache.read("400-header", &url).unwrap().is_some());
            fs::remove_dir_all(cache.directory().unwrap()).unwrap();
        }
    }

    #[test]
    fn failed_refill_reports_both_network_and_cache_errors() {
        let cache = cache();
        let (url, server) = server(response(b"not an image"));
        cache.write("400-header", &url, JPEG, "image/jpeg").unwrap();
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
        let (url, server) = server(response(JPEG));
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
        ))
        .unwrap();
        server.join().unwrap();
        assert_eq!(result.bytes, JPEG);
        assert!(
            result
                .cache_warning
                .as_deref()
                .is_some_and(|message| message.contains("could not be saved locally"))
        );
        fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    }
}
