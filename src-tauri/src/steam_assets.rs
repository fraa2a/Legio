use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant, SystemTime},
};

use crate::{
    artwork_publication::{
        ArtworkPublication, PublicationMode, PublicationOutcome, PublicationResult,
    },
    artwork_scheduler::{ArtworkScheduler, WorkError, WorkHandle, WorkOrigin},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
const FRESH_FOR: Duration = Duration::from_secs(5 * 24 * 60 * 60);
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
            Self::HeroBlur if full => "library_hero_2x.jpg",
            Self::HeroBlur => "library_hero.jpg",
            Self::ClientIcon => "clienticon.ico",
            Self::Header | Self::Capsule | Self::Screenshot => return None,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetResult {
    #[serde(skip_serializing)]
    bytes: Vec<u8>,
    content_type: &'static str,
    stale: bool,
    refresh_after: u64,
    cache_warning: Option<String>,
    replacement_token: Option<String>,
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

type AssetFetch = tokio::sync::Mutex<Option<(String, Result<AssetResult, String>)>>;
type MetadataLookup = Arc<tokio::sync::Mutex<crate::steam_pics::Lookup>>;

#[derive(Clone)]
pub struct AssetCacheState {
    directory: Result<PathBuf, String>,
    pub(crate) scheduler: ArtworkScheduler,
    publication: ArtworkPublication,
    failures: Arc<Mutex<BTreeMap<String, Instant>>>,
    lock: Arc<Mutex<Maintenance>>,
    pics_locks: Arc<Mutex<BTreeMap<u32, MetadataLookup>>>,
    asset_locks: Arc<Mutex<BTreeMap<String, Weak<AssetFetch>>>>,
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
            scheduler: ArtworkScheduler::default(),
            publication: ArtworkPublication::default(),
            failures: Arc::default(),
            lock: Arc::new(Mutex::new(Maintenance::default())),
            pics_locks: Arc::new(Mutex::new(BTreeMap::new())),
            asset_locks: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    fn metadata_lock(&self, app_id: u32) -> Result<MetadataLookup, String> {
        let mut locks = self
            .pics_locks
            .lock()
            .map_err(|_| "Steam metadata lock was poisoned")?;
        if locks.len() >= CACHE_FILES {
            locks.retain(|_, lock| Arc::strong_count(lock) > 1);
        }
        Ok(locks.entry(app_id).or_default().clone())
    }

    fn asset_lock(&self, key: &str) -> Result<Arc<AssetFetch>, String> {
        let mut locks = self
            .asset_locks
            .lock()
            .map_err(|_| "Artwork request lock was poisoned")?;
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(key).and_then(Weak::upgrade) {
            return Ok(lock);
        }
        let lock = Arc::new(AssetFetch::new(None));
        locks.insert(key.to_owned(), Arc::downgrade(&lock));
        Ok(lock)
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
        self.publication.cleanup_orphans(directory)?;
        let entries = match prune_protected(directory, &self.publication.protected_destinations()?)
        {
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
        let stale = artwork_expired(
            metadata
                .modified()
                .ok()
                .and_then(|modified| SystemTime::now().duration_since(modified).ok()),
        ) || header.transform_version != 2;
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

    fn publish(
        &self,
        key: &str,
        url: &str,
        bytes: &[u8],
        content_type: &'static str,
        mode: PublicationMode,
    ) -> Result<Option<String>, String> {
        if mode == PublicationMode::NativeOnly {
            self.write(key, url, bytes, content_type)?;
            return Ok(None);
        }
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Image cache lock was poisoned")?;
        let directory = self.directory()?;
        fs::create_dir_all(directory)
            .map_err(|error| format!("Could not create artwork staging: {error}"))?;
        let header = serde_json::to_vec(&CacheHeader {
            url: url.to_owned(),
            content_type: content_type.to_owned(),
            transform_version: 2,
        })
        .map_err(|error| error.to_string())?;
        if header.len() > MAX_HEADER_BYTES || bytes.len() > MAX_ASSET_BYTES {
            return Err("Image cache entry exceeds the size limit".into());
        }
        let stage = directory.join(format!("{}.stage", uuid::Uuid::new_v4()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)
            .map_err(|error| format!("Could not stage artwork: {error}"))?;
        let result = file
            .write_all(&header)
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.write_all(bytes))
            .and_then(|()| file.sync_data());
        drop(file);
        if let Err(error) = result {
            let _ = fs::remove_file(&stage);
            return Err(format!("Could not stage artwork: {error}"));
        }
        match self
            .publication
            .register(stage.clone(), directory.join(format!("{key}.asset")))
        {
            Ok(token) => Ok(Some(token)),
            Err(error) => {
                if let Err(cleanup) = fs::remove_file(stage) {
                    crate::application_log::failure("remove_staged_artwork", cleanup);
                }
                Err(error)
            }
        }
    }
    pub(crate) fn finish(
        &self,
        token: &str,
        outcome: PublicationOutcome,
    ) -> Result<PublicationResult, String> {
        let mut guard = self
            .lock
            .lock()
            .map_err(|_| "Image cache lock was poisoned")?;
        let result = self.publication.finish(token, outcome)?;
        guard.entries = None;
        drop(guard);
        if outcome != PublicationOutcome::Rejected {
            self.maintain()?;
        }
        Ok(result)
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
                crate::application_log::failure("remove_temporary_image_cache", cleanup);
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
                prune_protected(directory, &self.publication.protected_destinations()?)
                    .map_err(|error| format!("Could not prune the image cache: {error}"))?,
            );
            guard.last_prune = Some(Instant::now());
        }
        Ok(())
    }
}

fn prune_protected(
    directory: &Path,
    protected: &std::collections::HashSet<PathBuf>,
) -> io::Result<usize> {
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
    for (_, path) in files
        .into_iter()
        .filter(|(_, path)| !protected.contains(path))
        .take(remove)
    {
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

async fn optimized_asset_bytes(
    bytes: Vec<u8>,
    hero: Option<(u32, bool)>,
    work: &AssetWork,
) -> Result<Vec<u8>, String> {
    let permit = work
        .scheduler
        .acquire_conversion(&work.handle)
        .await
        .map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        match hero {
            Some((width, blurred)) => crate::image_trim::hero_webp(&bytes, width, blurred),
            None => webp_asset(&bytes),
        }
    })
    .await
    .map_err(|error| format!("Artwork processing task failed: {error}"))?
}

struct HeroSource {
    width: u32,
    fallback_url: Option<String>,
}

#[cfg(test)]
async fn load_asset(
    cache: AssetCacheState,
    network: &NetworkState,
    key: String,
    url: String,
    refresh: bool,
    hero: Option<HeroSource>,
) -> Result<AssetResult, String> {
    let work = AssetWork::new(
        &cache.scheduler,
        1,
        AssetDemand {
            request_id: uuid::Uuid::new_v4().to_string(),
            origin: WorkOrigin::Visible,
            publication: PublicationMode::NativeOnly,
        },
    )
    .map_err(|error| error.to_string())?;
    load_asset_scheduled(cache, network, key, url, refresh, hero, &work).await
}

async fn load_asset_scheduled(
    cache: AssetCacheState,
    network: &NetworkState,
    key: String,
    url: String,
    refresh: bool,
    hero: Option<HeroSource>,
    work: &AssetWork,
) -> Result<AssetResult, String> {
    let request_lock = cache.asset_lock(&key)?;
    let mut request = request_lock.lock().await;
    if let Some((requested_url, result)) = request.as_ref()
        && requested_url == &url
    {
        return result.clone();
    }
    let requested_url = url.clone();
    let result = load_asset_inner(cache, network, key, url, refresh, hero, work).await;
    if result
        .as_ref()
        .map_or_else(|error| error != "Artwork request cancelled", |_| true)
    {
        *request = Some((requested_url, result.clone()));
    }
    result
}

async fn load_asset_inner(
    cache: AssetCacheState,
    network: &NetworkState,
    key: String,
    mut url: String,
    refresh: bool,
    hero: Option<HeroSource>,
    work: &AssetWork,
) -> Result<AssetResult, String> {
    let reader = cache.clone();
    let read_key = key.clone();
    let read_url = url.clone();
    let fallback_url = hero.as_ref().and_then(|hero| hero.fallback_url.clone());
    let read = tauri::async_runtime::spawn_blocking(move || {
        let primary = reader.read(&read_key, &read_url)?;
        match (primary, fallback_url) {
            (None, Some(fallback)) => reader.read(&read_key, &fallback),
            (entry, _) => Ok(entry),
        }
    })
    .await
    .map_err(|error| format!("Image cache read task failed: {error}"))?;
    let (previous, read_error) = match read {
        Ok(entry) => (entry, None),
        Err(error) => {
            crate::application_log::failure("read_image_cache", &error);
            (None, Some(error))
        }
    };
    if !refresh && previous.as_ref().is_some_and(|entry| !entry.stale) {
        let entry = previous.ok_or_else(|| "Image cache entry disappeared.".to_owned())?;
        return Ok(AssetResult {
            replacement_token: None,
            bytes: entry.bytes,
            content_type: entry.content_type,
            stale: false,
            refresh_after: entry.refresh_after,
            cache_warning: None,
        });
    }
    let primary_url = url.clone();
    let fetched = work
        .scheduler
        .run_network(&work.handle, || async {
            match network.steam_asset(&primary_url).await {
                Err(crate::network::NetworkError::Http { status: 404 }) => {
                    match hero.as_ref().and_then(|hero| hero.fallback_url.as_ref()) {
                        Some(fallback) => network
                            .steam_asset(fallback)
                            .await
                            .map(|bytes| (bytes, Some(fallback.clone()))),
                        None => Err(crate::network::NetworkError::Http { status: 404 }),
                    }
                }
                result => result.map(|bytes| (bytes, None)),
            }
            .map_err(|error| format!("Steam image request failed: {error:?}"))
        })
        .await;
    let fetched = match fetched {
        Ok((bytes, fallback)) => {
            if let Some(fallback) = fallback {
                url = fallback;
            }
            Ok(bytes)
        }
        Err(error) => Err(error.to_string()),
    };
    let fetched = fetched.and_then(|bytes| {
        ImageFormat::from_steam_bytes(&bytes)
            .map(|_| bytes)
            .ok_or_else(|| "Steam returned unsupported image content.".to_owned())
    });
    let fetched = match fetched {
        Ok(bytes) => optimized_asset_bytes(bytes, hero.map(|hero| (hero.width, false)), work).await,
        Err(error) => Err(error),
    };
    match fetched {
        Ok(bytes) => {
            let content_type = "image/webp";
            let writer = cache.clone();
            let write_key = key;
            let write_url = url;
            let mode = work.publication;
            let (bytes, write_result) = tauri::async_runtime::spawn_blocking(move || {
                let result = writer.publish(&write_key, &write_url, &bytes, content_type, mode);
                (bytes, result)
            })
            .await
            .map_err(|error| format!("Image cache write task failed: {error}"))?;
            Ok(AssetResult {
                replacement_token: write_result.as_ref().ok().cloned().flatten(),
                bytes,
                content_type,
                stale: false,
                refresh_after: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map_or(0, |time| (time + FRESH_FOR).as_millis() as u64),
                cache_warning: write_result.as_ref().err().map(|error| {
                    format!("Image is visible, but could not be saved locally: {error}")
                }),
            })
        }
        Err(error) if error == "Artwork request cancelled" => Err(error),
        Err(error) => previous
            .map(|entry| AssetResult {
                replacement_token: None,
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

#[allow(clippy::too_many_arguments)]
async fn load_legacy_preview(
    cache: AssetCacheState,
    network: &NetworkState,
    key: String,
    url: String,
    refresh: bool,
    hero: Option<HeroSource>,
    metadata_path: Result<PathBuf, String>,
    work: &AssetWork,
) -> Result<Option<AssetResult>, String> {
    let Ok(path) = metadata_path else {
        return Ok(None);
    };
    match crate::steam_pics::read_cache(&path).await {
        Ok(None) => {}
        Ok(Some(_)) | Err(_) => return Ok(None),
    }
    match load_asset_scheduled(cache, network, key, url, refresh, hero, work).await {
        Ok(asset) => Ok(Some(asset)),
        Err(_) => Ok(None),
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
            .or_else(|| {
                (filename == "library_hero_2x.jpg")
                    .then(|| assets.url(app_id, "library_hero.jpg"))
                    .flatten()
            })
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

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetPolicy {
    CacheOrFetch,
    RefreshIfExpired,
    ForceRefresh,
}
pub struct AssetRequest {
    pub app_id: u32,
    pub asset: AssetKind,
    pub index: Option<usize>,
    pub full: bool,
    pub policy: AssetPolicy,
    pub screenshot_identity: Option<String>,
}
pub struct AssetDemand {
    pub request_id: String,
    pub origin: WorkOrigin,
    pub publication: PublicationMode,
}
struct AssetWork {
    policy: AssetPolicy,
    scheduler: ArtworkScheduler,
    handle: WorkHandle,
    publication: PublicationMode,
}
impl AssetWork {
    fn new(
        scheduler: &ArtworkScheduler,
        app_id: u32,
        demand: AssetDemand,
    ) -> Result<Self, WorkError> {
        Ok(Self {
            policy: AssetPolicy::CacheOrFetch,
            scheduler: scheduler.clone(),
            handle: scheduler.begin(demand.request_id, app_id, demand.origin)?,
            publication: demand.publication,
        })
    }
    fn native_only(&self) -> Self {
        Self {
            policy: self.policy,
            scheduler: self.scheduler.clone(),
            handle: self.handle.clone(),
            publication: PublicationMode::NativeOnly,
        }
    }
}
fn artwork_expired(age: Option<Duration>) -> bool {
    age.is_none_or(|age| age >= FRESH_FOR)
}
fn refresh_artwork(policy: AssetPolicy, stale: bool) -> bool {
    policy == AssetPolicy::ForceRefresh || (policy == AssetPolicy::RefreshIfExpired && stale)
}
fn asset_key(
    app_id: u32,
    asset: AssetKind,
    index: Option<usize>,
    full: bool,
    width: u32,
    identity: Option<&str>,
) -> Result<String, String> {
    let adaptive = matches!(asset, AssetKind::Hero | AssetKind::HeroBlur);
    let suffix = if adaptive {
        format!(
            "-display-v{}-{width}",
            if matches!(asset, AssetKind::HeroBlur) {
                2
            } else {
                1
            }
        )
    } else if full {
        "-full".into()
    } else {
        String::new()
    };
    let kind = match asset {
        AssetKind::Header => "header",
        AssetKind::Capsule => "capsule",
        AssetKind::Hero => "hero",
        AssetKind::HeroBlur => "hero-blur",
        AssetKind::Logo => "logo",
        AssetKind::Screenshot => "screenshot",
        AssetKind::ClientIcon => "clienticon-alpha",
        _ => asset
            .library_filename(full)
            .ok_or("Invalid image selection")?,
    };
    Ok(if let Some(identity) = identity {
        format!(
            "{app_id}-screenshot-{:x}{suffix}",
            Sha256::digest(identity.as_bytes())
        )
    } else {
        match index {
            Some(index) => format!("{app_id}-{kind}-{index}{suffix}"),
            None => format!("{app_id}-{kind}{suffix}"),
        }
    })
}
fn cached_result(entry: CacheEntry) -> AssetResult {
    AssetResult {
        bytes: entry.bytes,
        content_type: entry.content_type,
        stale: entry.stale,
        refresh_after: entry.refresh_after,
        cache_warning: None,
        replacement_token: None,
    }
}
fn selected_url_with_identity(
    details: &SteamDetails,
    asset: AssetKind,
    index: Option<usize>,
    full: bool,
    identity: Option<&str>,
) -> Result<String, String> {
    if matches!(asset, AssetKind::Screenshot)
        && let Some(identity) = identity
    {
        let parsed = reqwest::Url::parse(identity)
            .map_err(|_| "Screenshot identity is invalid".to_owned())?;
        if identity.len() > 2048
            || identity.chars().any(char::is_control)
            || !is_steam_asset_url(&parsed)
        {
            return Err("Screenshot identity is invalid".to_owned());
        }
        return Ok(identity.to_owned());
    }
    selected_url(details, asset, index, full)
}
pub async fn get_asset_for_request<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    network: &NetworkState,
    request: AssetRequest,
    demand: AssetDemand,
) -> Result<AssetResult, WorkError> {
    if request.app_id == 0
        || (request.index.is_some() != matches!(request.asset, AssetKind::Screenshot))
        || (request.screenshot_identity.is_some()
            && !matches!(request.asset, AssetKind::Screenshot))
    {
        return Err(WorkError::Failed("Invalid image selection".into()));
    }
    let cache = app.state::<AssetCacheState>().inner().clone();
    let mut work = AssetWork::new(&cache.scheduler, request.app_id, demand)?;
    work.policy = request.policy;
    let width = crate::artwork_display::width(&app);
    let full = if matches!(request.asset, AssetKind::Hero | AssetKind::HeroBlur) {
        width > 1920
    } else {
        request.full
    };
    let identity = if matches!(request.asset, AssetKind::Screenshot) {
        let app = app.clone();
        let app_id = request.app_id;
        let index = request.index;
        let requested_identity = request.screenshot_identity.clone();
        let identity = tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<DatabaseState>();
            let database = state.database()?;
            let details = cached_details(database, app_id)
                .map_err(|error| error.message)?
                .ok_or("No cached Steam details are available for this App ID")?;
            if let Some(identity) = requested_identity {
                if !(0..details.assets.screenshots.len()).any(|candidate| {
                    selected_url(&details, AssetKind::Screenshot, Some(candidate), full)
                        .is_ok_and(|url| url == identity)
                }) {
                    return Err("Screenshot identity no longer matches cached metadata".to_owned());
                }
                Ok(identity)
            } else {
                selected_url(&details, AssetKind::Screenshot, index, full)
            }
        })
        .await
        .map_err(|error| WorkError::Failed(error.to_string()))?
        .map_err(WorkError::Failed)?;
        if request
            .screenshot_identity
            .as_ref()
            .is_some_and(|value| value != &identity)
        {
            return Err(WorkError::Failed(
                "Screenshot identity no longer matches cached metadata".into(),
            ));
        }
        Some(identity)
    } else {
        None
    };
    let key = asset_key(
        request.app_id,
        request.asset,
        request.index,
        full,
        width,
        identity.as_deref(),
    )
    .map_err(WorkError::Failed)?;
    let previous = {
        let cache = cache.clone();
        let key = key.clone();
        let identity = identity.clone();
        let legacy = asset_key(
            request.app_id,
            request.asset,
            request.index,
            full,
            width,
            None,
        )
        .map_err(WorkError::Failed)?;
        tauri::async_runtime::spawn_blocking(move || {
            match cache.read_cached(&key, identity.as_deref()) {
                Ok(None) if identity.is_some() => cache.read_cached(&legacy, identity.as_deref()),
                result => result,
            }
        })
        .await
        .map_err(|error| WorkError::Failed(error.to_string()))?
    };
    let previous = match previous {
        Ok(entry) => entry,
        Err(error) => {
            crate::application_log::failure("read_image_cache", error);
            None
        }
    };
    if let Some(entry) = previous.as_ref()
        && !refresh_artwork(request.policy, entry.stale)
    {
        return Ok(cached_result(previous.ok_or_else(|| {
            WorkError::Failed("Image cache entry disappeared".into())
        })?));
    }
    if request.policy != AssetPolicy::ForceRefresh && previous.is_some() {
        let failures = cache
            .failures
            .lock()
            .map_err(|_| WorkError::Failed("Artwork retry state unavailable".into()))?;
        if failures
            .get(&key)
            .is_some_and(|retry| *retry > Instant::now())
        {
            return Ok(cached_result(previous.ok_or_else(|| {
                WorkError::Failed("Image cache entry disappeared".into())
            })?));
        }
    }
    let result = Box::pin(get_asset_core(
        app,
        network,
        request.app_id,
        request.asset,
        request.index,
        full,
        previous.is_some() || request.policy == AssetPolicy::ForceRefresh,
        identity.as_deref(),
        &work,
    ))
    .await;
    if work.handle.is_cancelled() {
        return Err(WorkError::Cancelled);
    }
    let failed = !result.as_ref().is_ok_and(|result| !result.stale);
    if failed {
        work.handle.mark_failed();
    }
    let mut failures = cache
        .failures
        .lock()
        .map_err(|_| WorkError::Failed("Artwork retry state unavailable".into()))?;
    failures.retain(|_, retry| *retry > Instant::now());
    if failed {
        if failures.len() >= CACHE_FILES {
            failures.clear();
        }
        failures.insert(key, Instant::now() + Duration::from_secs(300));
    } else {
        failures.remove(&key);
    }
    match result {
        Ok(result) => Ok(result),
        Err(error) if error == "Artwork request cancelled" => Err(WorkError::Cancelled),
        Err(error) => match previous {
            Some(entry) => {
                let mut result = cached_result(entry);
                result.stale = true;
                result.cache_warning = Some(error);
                Ok(result)
            }
            None => Err(WorkError::Failed(error)),
        },
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
    get_asset_for_request(
        app,
        network,
        AssetRequest {
            app_id,
            asset,
            index,
            full,
            policy: if refresh {
                AssetPolicy::ForceRefresh
            } else {
                AssetPolicy::CacheOrFetch
            },
            screenshot_identity: None,
        },
        AssetDemand {
            request_id: uuid::Uuid::new_v4().to_string(),
            origin: WorkOrigin::Prefetch,
            publication: PublicationMode::NativeOnly,
        },
    )
    .await
    .map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
async fn get_asset_core<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    network: &NetworkState,
    app_id: u32,
    asset: AssetKind,
    index: Option<usize>,
    full: bool,
    refresh: bool,
    screenshot_identity: Option<&str>,
    work: &AssetWork,
) -> Result<AssetResult, String> {
    if app_id == 0 || (index.is_some() != matches!(asset, AssetKind::Screenshot)) {
        return Err("Invalid image selection.".to_owned());
    }
    let state = app.state::<AssetCacheState>().inner().clone();
    let display_width = crate::artwork_display::width(&app);
    let adaptive_hero = matches!(asset, AssetKind::Hero | AssetKind::HeroBlur);
    let full = if adaptive_hero {
        display_width > 1920
    } else {
        full
    };
    let filename = asset.library_filename(full);
    let key = asset_key(
        app_id,
        asset,
        index,
        full,
        display_width,
        screenshot_identity,
    )?;
    let blur_lock = matches!(asset, AssetKind::HeroBlur)
        .then(|| state.asset_lock(&key))
        .transpose()?;
    let (mut blur_request, refresh) = match blur_lock.as_ref() {
        Some(lock) => match lock.try_lock() {
            Ok(guard) => (Some(guard), refresh),
            Err(_) => (Some(lock.lock().await), false),
        },
        None => (None, refresh),
    };
    if let Some(request) = blur_request.as_ref()
        && let Some((_, result)) = request.as_ref()
    {
        return result.clone();
    }
    if !refresh {
        let reader = state.clone();
        let read_key = key.clone();
        let cached =
            tauri::async_runtime::spawn_blocking(move || reader.read_cached(&read_key, None))
                .await
                .map_err(|error| format!("Image cache read task failed: {error}"))?;
        match cached {
            Ok(Some(entry)) if !refresh_artwork(work.policy, entry.stale) => {
                return Ok(AssetResult {
                    replacement_token: None,
                    bytes: entry.bytes,
                    content_type: entry.content_type,
                    stale: entry.stale,
                    refresh_after: entry.refresh_after,
                    cache_warning: None,
                });
            }
            Ok(_) => {}
            Err(error) => crate::application_log::failure("read_image_cache", error),
        }
    }
    if matches!(asset, AssetKind::HeroBlur) {
        let dependency_work = work.native_only();
        // The sharp cache is shared with banners; cards persist their own resized blur.
        let hero = Box::pin(get_asset_core(
            app.clone(),
            network,
            app_id,
            AssetKind::Hero,
            None,
            full,
            work.policy == AssetPolicy::ForceRefresh,
            None,
            &dependency_work,
        ))
        .await;
        let (source, source_name) = match hero {
            Ok(hero) => (hero, "derived:library_hero"),
            Err(hero_error) => {
                let header = Box::pin(get_asset_core(
                    app,
                    network,
                    app_id,
                    AssetKind::Header,
                    None,
                    false,
                    work.policy == AssetPolicy::ForceRefresh,
                    None,
                    &dependency_work,
                ))
                .await
                .map_err(|error| format!("{hero_error} Header fallback: {error}"))?;
                (header, "derived:header")
            }
        };
        let bytes = optimized_asset_bytes(source.bytes, Some((display_width, true)), work).await?;
        let stale = source.stale;
        let mode = work.publication;
        let (bytes, written) = tauri::async_runtime::spawn_blocking(move || {
            let written = if stale {
                Ok(None)
            } else {
                state.publish(&key, source_name, &bytes, "image/webp", mode)
            };
            (bytes, written)
        })
        .await
        .map_err(|error| format!("Card image cache task failed: {error}"))?;
        let mut warning = source.cache_warning;
        let replacement_token = written.as_ref().ok().cloned().flatten();
        if let Err(error) = written {
            warning = Some(match warning {
                Some(existing) => format!("{existing} Card image cache: {error}"),
                None => format!("Card image cache: {error}"),
            });
        }
        let result = AssetResult {
            replacement_token,
            bytes,
            content_type: "image/webp",
            stale,
            refresh_after: source.refresh_after,
            cache_warning: warning,
        };
        if let Some(request) = blur_request.as_mut() {
            **request = Some((source_name.to_owned(), Ok(result.clone())));
        }
        return Ok(result);
    }
    if let Some(filename) = filename
        && !matches!(asset, AssetKind::ClientIcon)
    {
        let path = state
            .directory()
            .map(|directory| directory.join("pics").join(format!("{app_id}.json")));
        let fallback_url = if matches!(asset, AssetKind::Hero) && full {
            Some(library_url(app_id, "library_hero.jpg", None)?)
        } else {
            None
        };
        if let Some(result) = load_legacy_preview(
            state.clone(),
            network,
            key.clone(),
            library_url(app_id, filename, None)?,
            refresh,
            adaptive_hero.then_some(HeroSource {
                width: display_width,
                fallback_url,
            }),
            path,
            work,
        )
        .await?
        {
            return Ok(result);
        }
    }
    let (selected, metadata_warning, hero_fallback) = if let Some(filename) = filename {
        let path = state
            .directory()
            .map(|directory| directory.join("pics").join(format!("{app_id}.json")));
        let metadata_lock = state.metadata_lock(app_id)?;
        let (assets, warning) = crate::steam_pics::resolve(path, &metadata_lock, async {
            work.scheduler
                .run_metadata(&work.handle, || crate::steam_pics::fetch(app_id))
                .await
                .map_err(|error| error.to_string())
        })
        .await;
        if let Some(warning) = warning.as_deref() {
            crate::application_log::failure("asset_metadata_lookup", warning);
        }
        let fallback = if matches!(asset, AssetKind::Hero) && full {
            Some(library_url(app_id, "library_hero.jpg", assets.as_ref())?)
        } else {
            None
        };
        (
            library_url(app_id, filename, assets.as_ref()),
            warning,
            fallback,
        )
    } else {
        let app_for_lookup = app.clone();
        let (details, language) = tauri::async_runtime::spawn_blocking(move || {
            let state = app_for_lookup.state::<DatabaseState>();
            let database = state.database()?;
            let language = database.settings()?.language.resolve();
            Ok::<_, String>((
                cached_details(database, app_id).map_err(|error| error.message)?,
                language,
            ))
        })
        .await
        .map_err(|error| format!("Image lookup task failed: {error}"))??;
        let details = match details {
            Some(details) => details,
            None => Box::pin(crate::steam_details::get_details_localized_with_policy(
                app.clone(),
                network,
                app_id,
                crate::steam_details::DetailsPolicy::CacheOrFetch,
                language,
                &work.handle,
            ))
            .await
            .map_err(|error| error.message)?
            .details
            .ok_or("Steam artwork metadata is unavailable")?,
        };
        let url = selected_url_with_identity(&details, asset, index, full, screenshot_identity);
        (url, None, None)
    };
    let loaded = match selected {
        Ok(url) => {
            load_asset_scheduled(
                state,
                network,
                key,
                url,
                refresh,
                adaptive_hero.then_some(HeroSource {
                    width: display_width,
                    fallback_url: hero_fallback,
                }),
                work,
            )
            .await
        }
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
    let metadata_lock = state.metadata_lock(steam_app_id)?;
    let mut metadata = metadata_lock.lock().await;
    *metadata = crate::steam_pics::Lookup::default();
    let cache = state.clone();
    tauri::async_runtime::spawn_blocking(move || cache.reset_app(steam_app_id))
        .await
        .map_err(|error| format!("Image cache reset task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn nested_blur_does_not_hold_network_or_conversion_capacity() {
        let cache = cache();
        let root = cache.directory().unwrap().parent().unwrap().to_path_buf();
        let sharp = crate::image_trim::webp_asset(PNG).unwrap();
        cache
            .write("400-hero-display-v1-1920", "test", &sharp, "image/webp")
            .unwrap();
        let scheduler = cache.scheduler.clone();
        let ordinary = AssetWork::new(
            &scheduler,
            1,
            AssetDemand {
                request_id: "ordinary-codec".into(),
                origin: WorkOrigin::Visible,
                publication: PublicationMode::NativeOnly,
            },
        )
        .unwrap();
        let held_codec = scheduler
            .acquire_conversion(&ordinary.handle)
            .await
            .unwrap();
        let started = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut transfers = Vec::new();
        for id in 1..=4 {
            let work = AssetWork::new(
                &scheduler,
                id,
                AssetDemand {
                    request_id: format!("ordinary-network-{id}"),
                    origin: WorkOrigin::Visible,
                    publication: PublicationMode::NativeOnly,
                },
            )
            .unwrap();
            let count = started.clone();
            transfers.push(tokio::spawn(async move {
                work.scheduler
                    .run_network(&work.handle, || async {
                        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        std::future::pending::<Result<(), String>>().await
                    })
                    .await
            }));
        }
        while started.load(std::sync::atomic::Ordering::SeqCst) < 4 {
            tokio::task::yield_now().await;
        }
        scheduler.select_game(Some(400)).unwrap();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = format!("org.legio.test.{}", uuid::Uuid::new_v4());
        let app = tauri::test::mock_builder()
            .manage(cache.clone())
            .build(context)
            .unwrap();
        let network = NetworkState::new(
            "0.2.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let image = tokio::time::timeout(
            Duration::from_secs(5),
            get_asset(
                app.handle().clone(),
                &network,
                400,
                AssetKind::HeroBlur,
                None,
                false,
                false,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(ImageFormat::from_steam_bytes(&image.bytes).is_some());
        assert_eq!(started.load(std::sync::atomic::Ordering::SeqCst), 4);
        for id in 1..=4 {
            scheduler.cancel(&format!("ordinary-network-{id}")).unwrap();
        }
        for transfer in transfers {
            assert!(matches!(transfer.await.unwrap(), Err(WorkError::Cancelled)));
        }
        drop(held_codec);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cached_artwork_ignores_expiry_outside_game_open() {
        assert!(!refresh_artwork(AssetPolicy::CacheOrFetch, true));
        assert!(refresh_artwork(AssetPolicy::RefreshIfExpired, true));
    }
    #[test]
    fn page_open_refreshes_at_five_day_boundary() {
        assert!(!artwork_expired(Some(Duration::from_secs(431_999))));
        assert!(artwork_expired(Some(Duration::from_secs(432_000))));
        assert!(artwork_expired(None));
    }
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
    async fn unrelated_games_do_not_wait_for_another_games_metadata() {
        let cache = cache();
        let directory = cache.directory().unwrap().join("pics");
        let busy_lock = cache.metadata_lock(400).unwrap();
        let _busy_game = busy_lock.lock().await;
        let other_lock = cache.metadata_lock(401).unwrap();
        let lookup =
            crate::steam_pics::resolve(Ok(directory.join("401.json")), &other_lock, async {
                Ok(serde_json::from_str::<crate::steam_pics::PicsAssets>("{}").unwrap())
            });
        let result = tokio::time::timeout(Duration::from_millis(100), lookup).await;
        assert!(
            result.is_ok(),
            "An unrelated app must not wait for a busy app lookup"
        );
        fs::remove_dir_all(cache.directory().unwrap().parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn existing_hashed_metadata_keeps_priority_over_legacy_artwork() {
        let cache = cache();
        let path = cache.directory().unwrap().join("pics/400.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"library_hero.jpg":"94e9d990ddd19610b268faf629865e17dcda0bb8/library_hero.jpg"}"#,
        )
        .unwrap();
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let work = AssetWork::new(
            &cache.scheduler,
            400,
            AssetDemand {
                request_id: uuid::Uuid::new_v4().to_string(),
                origin: WorkOrigin::Prefetch,
                publication: PublicationMode::NativeOnly,
            },
        )
        .unwrap();
        let result = load_legacy_preview(
            cache.clone(),
            &network,
            "400-hero-display-v1-1920".into(),
            "http://127.0.0.1:1/should-not-load".into(),
            false,
            None,
            Ok(path),
            &work,
        )
        .await
        .unwrap();
        assert!(result.is_none());
        assert!(
            cache
                .read_cached("400-hero-display-v1-1920", None)
                .unwrap()
                .is_none()
        );
        fs::remove_dir_all(cache.directory().unwrap().parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn cold_legacy_artwork_is_available_without_a_steam_metadata_connection() {
        let cache = cache();
        let path = cache.directory().unwrap().join("pics/400.json");
        let (url, server) = server(response(PNG));
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let work = AssetWork::new(
            &cache.scheduler,
            400,
            AssetDemand {
                request_id: uuid::Uuid::new_v4().to_string(),
                origin: WorkOrigin::Prefetch,
                publication: PublicationMode::NativeOnly,
            },
        )
        .unwrap();
        let result = load_legacy_preview(
            cache.clone(),
            &network,
            "400-hero-display-v1-1920".into(),
            url,
            false,
            Some(HeroSource {
                width: 1920,
                fallback_url: None,
            }),
            Ok(path.clone()),
            &work,
        )
        .await
        .unwrap();
        assert!(
            result.is_some(),
            "Available CDN artwork must precede a Steam connection"
        );
        server.join().unwrap();
        assert!(image::load_from_memory(&result.unwrap().bytes).is_ok());
        assert!(!path.exists());
        fs::remove_dir_all(cache.directory().unwrap().parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn different_source_urls_do_not_share_a_failed_download_result() {
        let cache = cache();
        let (missing, missing_server) = server(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        );
        let (available, available_server) = server(response(PNG));
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let (failed, loaded) = tokio::join!(
            load_asset(
                cache.clone(),
                &network,
                "400-hero".into(),
                missing,
                false,
                None
            ),
            load_asset(
                cache.clone(),
                &network,
                "400-hero".into(),
                available,
                false,
                None
            ),
        );
        assert!(failed.is_err());
        assert!(
            loaded.is_ok(),
            "A failed legacy source must not suppress a new hashed source"
        );
        missing_server.join().unwrap();
        available_server.join().unwrap();
        fs::remove_dir_all(cache.directory().unwrap().parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn concurrent_sharp_hero_requests_share_one_download() {
        let cache = cache();
        let (url, server) = server(response(PNG));
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let requests = (0..3)
            .map(|_| {
                load_asset(
                    cache.clone(),
                    &network,
                    "400-hero-display-v1-1920".into(),
                    url.clone(),
                    false,
                    Some(HeroSource {
                        width: 1920,
                        fallback_url: None,
                    }),
                )
            })
            .collect::<Vec<_>>();
        let mut requests = requests.into_iter();
        let (first, second, third) = tokio::join!(
            requests.next().unwrap(),
            requests.next().unwrap(),
            requests.next().unwrap()
        );
        server.join().unwrap();
        let first = first.unwrap();
        assert_eq!(second.unwrap().bytes, first.bytes);
        assert_eq!(third.unwrap().bytes, first.bytes);
        fs::remove_dir_all(cache.directory().unwrap().parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn concurrent_heroes_share_download_even_when_cache_is_unwritable() {
        let cache = cache();
        fs::create_dir_all(cache.directory().unwrap().parent().unwrap()).unwrap();
        fs::write(cache.directory().unwrap(), b"not a directory").unwrap();
        let (url, server) = server(response(PNG));
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let requests = (0..3)
            .map(|_| {
                load_asset(
                    cache.clone(),
                    &network,
                    "400-hero-display-v1-1920".into(),
                    url.clone(),
                    false,
                    Some(HeroSource {
                        width: 1920,
                        fallback_url: None,
                    }),
                )
            })
            .collect::<Vec<_>>();
        let mut requests = requests.into_iter();
        let (first, second, third) = tokio::join!(
            requests.next().unwrap(),
            requests.next().unwrap(),
            requests.next().unwrap()
        );
        server.join().unwrap();
        let first = first.unwrap();
        assert_eq!(second.unwrap().bytes, first.bytes);
        assert_eq!(third.unwrap().bytes, first.bytes);
        fs::remove_dir_all(cache.directory().unwrap().parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn concurrent_hero_refreshes_share_one_download() {
        let cache = cache();
        let (url, server) = server(response(PNG));
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let requests = (0..3)
            .map(|_| {
                load_asset(
                    cache.clone(),
                    &network,
                    "400-hero-display-v1-1920".into(),
                    url.clone(),
                    true,
                    Some(HeroSource {
                        width: 1920,
                        fallback_url: None,
                    }),
                )
            })
            .collect::<Vec<_>>();
        let mut requests = requests.into_iter();
        let (first, second, third) = tokio::join!(
            requests.next().unwrap(),
            requests.next().unwrap(),
            requests.next().unwrap()
        );
        server.join().unwrap();
        let first = first.unwrap();
        for result in [second.unwrap(), third.unwrap()] {
            assert!(
                !result.stale,
                "A duplicate refresh must reuse the completed result"
            );
            assert!(result.cache_warning.is_none());
            assert_eq!(result.bytes, first.bytes);
        }
        fs::remove_dir_all(cache.directory().unwrap().parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn card_blur_is_persisted_and_reused_without_processing_the_sharp_source() {
        let cache = cache();
        let root = cache.directory().unwrap().parent().unwrap().to_path_buf();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = format!("org.legio.test.{}", uuid::Uuid::new_v4());
        let app = tauri::test::mock_builder()
            .manage(cache.clone())
            .build(context)
            .unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1920,
            620,
            image::Rgba([20, 40, 60, 255]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        let sharp = crate::image_trim::hero_webp(png.get_ref(), 1920, false).unwrap();
        cache
            .write("400-hero-display-v1-1920", "test", &sharp, "image/webp")
            .unwrap();
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        cache
            .write(
                "400-hero-blur-display-v1-1920",
                "derived:library_hero",
                PNG,
                "image/png",
            )
            .unwrap();
        let metadata_lock = cache.metadata_lock(400).unwrap();
        let _guard = metadata_lock.lock().await;
        let first = get_asset(
            app.handle().clone(),
            &network,
            400,
            AssetKind::HeroBlur,
            None,
            false,
            false,
        )
        .await
        .unwrap();
        let decoded = image::load_from_memory(&first.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1248, 403));
        assert_eq!(
            cache
                .read_cached("400-hero-blur-display-v2-1920", None)
                .unwrap()
                .unwrap()
                .bytes,
            first.bytes
        );
        // An invalid sharp image would fail if the card were processed again.
        cache
            .write("400-hero-display-v1-1920", "test", b"invalid", "image/webp")
            .unwrap();
        let second = get_asset(
            app.handle().clone(),
            &network,
            400,
            AssetKind::HeroBlur,
            None,
            true,
            false,
        )
        .await
        .unwrap();
        assert_eq!(first.bytes, second.bytes);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn missing_hero_uses_a_blurred_header_without_modifying_the_sharp_header() {
        let cache = cache();
        let root = cache.directory().unwrap().parent().unwrap().to_path_buf();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = format!("org.legio.test.{}", uuid::Uuid::new_v4());
        let app = tauri::test::mock_builder()
            .manage(cache.clone())
            .build(context)
            .unwrap();
        let source =
            image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(460, 215, |x, _| {
                let value = if x < 230 { 0 } else { 255 };
                image::Rgba([value, value, value, 255])
            }));
        let mut png = std::io::Cursor::new(Vec::new());
        source.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let sharp = crate::image_trim::webp_asset(png.get_ref()).unwrap();
        cache
            .write("400-header", "test", &sharp, "image/webp")
            .unwrap();
        let pics = cache.directory().unwrap().join("pics");
        fs::create_dir_all(&pics).unwrap();
        fs::write(pics.join("400.json"), b"invalid").unwrap();
        let locks = cache.pics_locks.clone();
        let _ = std::thread::spawn(move || {
            let _guard = locks.lock().unwrap();
            panic!("Force a failed hero lookup without making a network request");
        })
        .join();
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let blurred = get_asset(
            app.handle().clone(),
            &network,
            400,
            AssetKind::HeroBlur,
            None,
            false,
            false,
        )
        .await
        .unwrap();
        let pixels = image::load_from_memory(&blurred.bytes).unwrap().to_rgba8();
        assert_eq!(pixels.width(), 299);
        assert!(pixels.get_pixel(145, 60)[0] > 10);
        assert_eq!(
            cache
                .read_cached("400-header", None)
                .unwrap()
                .unwrap()
                .bytes,
            sharp
        );
        assert_eq!(
            cache
                .read_cached("400-hero-blur-display-v2-1920", None)
                .unwrap()
                .unwrap()
                .bytes,
            blurred.bytes
        );
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn missing_header_waits_for_corrupt_metadata_recovery() {
        let cache = cache();
        let root = cache.directory().unwrap().parent().unwrap().to_path_buf();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = format!("org.legio.test.{}", uuid::Uuid::new_v4());
        let app = tauri::test::mock_builder()
            .manage(DatabaseState::new(Ok(root.join("data"))))
            .manage(cache.scheduler.clone())
            .manage(crate::steam_details::DetailsFetchState::default())
            .manage(cache.clone())
            .build(context)
            .unwrap();
        let database = app.state::<DatabaseState>();
        database.database().unwrap().with_connection(|connection| connection.execute("INSERT INTO steam_details_cache (steam_app_id,details,fetched_at) VALUES (400,'broken',0)", []).map_err(|error| error.to_string())).unwrap();
        let details_state = app.state::<crate::steam_details::DetailsFetchState>();
        let lock = crate::steam_details::hold_metadata_for_test(&details_state, 400);
        let guard = lock.lock().await;
        let network = NetworkState::new(
            "0.2.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let loading = get_asset(
            app.handle().clone(),
            &network,
            400,
            AssetKind::Header,
            None,
            false,
            false,
        );
        tokio::pin!(loading);
        assert!(
            tokio::time::timeout(Duration::from_millis(150), &mut loading)
                .await
                .is_err(),
            "corrupt metadata must reach the existing recovery lock instead of failing the artwork"
        );
        let url = "https://cdn.steamstatic.com/400/header.jpg";
        let repaired = serde_json::json!({"steamAppId":400,"name":"Portal","appType":"game","developers":[],"publishers":[],"genres":[],"detailedDescription":"Ready","systemRequirements":{},"assets":{"header":url,"capsule":null,"background":null,"screenshots":[]}}).to_string();
        database
            .database()
            .unwrap()
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE steam_details_cache SET details=?1,fetched_at=?2",
                        rusqlite::params![
                            repaired,
                            SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap()
                                .as_secs() as i64
                        ],
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        cache.write("400-header", url, PNG, "image/png").unwrap();
        drop(guard);
        let image = tokio::time::timeout(Duration::from_secs(3), loading)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(image.bytes, PNG);
        drop(app);
        fs::remove_dir_all(root).unwrap();
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
        cache.scheduler.select_game(Some(400)).unwrap();
        let capacity_work = cache
            .scheduler
            .begin("saturated-cache-test".into(), 400, WorkOrigin::Visible)
            .unwrap();
        let mut network_capacity = Vec::new();
        for _ in 0..5 {
            network_capacity.push(
                cache
                    .scheduler
                    .acquire_for_test(&capacity_work, crate::artwork_scheduler::WorkStage::Network)
                    .await
                    .unwrap(),
            );
        }
        let mut conversion_capacity = Vec::new();
        for _ in 0..2 {
            conversion_capacity.push(
                cache
                    .scheduler
                    .acquire_conversion(&capacity_work)
                    .await
                    .unwrap(),
            );
        }
        let metadata_lock = cache.metadata_lock(400).unwrap();
        let _guard = metadata_lock.lock().await;
        for (asset, index, key) in [
            (AssetKind::Hero, None, "400-hero-display-v1-1920"),
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
    fn blurred_heroes_use_the_sharp_source_and_preserve_legacy_urls() {
        let filename = AssetKind::HeroBlur.library_filename(false).unwrap();
        assert_eq!(filename, "library_hero.jpg");
        assert_eq!(
            library_url(400, filename, None).unwrap(),
            "https://cdn.cloudflare.steamstatic.com/steam/apps/400/library_hero.jpg"
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
            None,
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
    fn missing_double_size_hero_falls_back_and_caches_the_available_source() {
        let cache = cache();
        let (missing, missing_server) = server(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        );
        let (fallback, fallback_server) = server(response(PNG));
        let network = NetworkState::new(
            "0.1.0",
            crate::diagnostics::Diagnostics::new(Err("test".into())),
        )
        .unwrap();
        let first = tauri::async_runtime::block_on(load_asset(
            cache.clone(),
            &network,
            "400-hero-display-v1-3840".into(),
            missing.clone(),
            false,
            Some(HeroSource {
                width: 3840,
                fallback_url: Some(fallback.clone()),
            }),
        ))
        .unwrap();
        missing_server.join().unwrap();
        fallback_server.join().unwrap();
        assert_eq!(image::load_from_memory(&first.bytes).unwrap().width(), 2);
        assert_eq!(
            cache
                .read("400-hero-display-v1-3840", &fallback)
                .unwrap()
                .unwrap()
                .bytes,
            first.bytes
        );
        let cached = tauri::async_runtime::block_on(load_asset(
            cache.clone(),
            &network,
            "400-hero-display-v1-3840".into(),
            missing,
            false,
            Some(HeroSource {
                width: 3840,
                fallback_url: Some(fallback),
            }),
        ))
        .unwrap();
        assert_eq!(cached.bytes, first.bytes);
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
            None,
        ))
        .unwrap();
        server.join().unwrap();
        let second = tauri::async_runtime::block_on(load_asset(
            cache.clone(),
            &network,
            "400-header".into(),
            url,
            false,
            None,
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
            None,
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
            None,
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
    fn screenshot_download_keeps_validated_identity_after_metadata_reordering() {
        let mut details: SteamDetails = serde_json::from_value(serde_json::json!({
            "steamAppId":400,"name":"Portal","appType":"game","developers":[],"publishers":[],"genres":[],
            "assets":{"header":null,"capsule":null,"background":null,"screenshots":[
                {"thumbnail":"https://cdn.steamstatic.com/400/a.jpg"},
                {"thumbnail":"https://cdn.steamstatic.com/400/b.jpg"}]}
        })).unwrap();
        let validated = selected_url(&details, AssetKind::Screenshot, Some(0), false).unwrap();
        details.assets.screenshots.swap(0, 1);
        let selected = selected_url_with_identity(
            &details,
            AssetKind::Screenshot,
            Some(0),
            false,
            Some(&validated),
        )
        .unwrap();
        assert_eq!(selected, validated);
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
                false,
                None,
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
    fn webp_cache_survives_reopen_and_expires_after_five_days() {
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
        for (hours, stale) in [(119, false), (121, true)] {
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
        let recent = cache.directory().unwrap().join("recent.tmp");
        fs::write(&recent, b"recent unfinished").unwrap();
        let orphan = cache.directory().unwrap().join("orphan.tmp");
        fs::write(&orphan, b"unfinished").unwrap();
        assert!(cache.read_cached("400-header", None).unwrap().is_some());
        assert!(orphan.exists());
        let old = SystemTime::now() - ORPHAN_AGE - Duration::from_secs(1);
        fs::File::open(&orphan)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();
        assert!(cache.read_cached("400-header", None).unwrap().is_some());
        assert!(orphan.exists());
        cache.maintain().unwrap();
        assert!(!orphan.exists());
        assert!(recent.exists());
        assert!(cache.read_cached("400-header", None).unwrap().is_some());
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
                None,
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
            None,
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
            None,
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
