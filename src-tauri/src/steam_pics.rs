use std::{
    collections::BTreeMap,
    future::Future,
    io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};
use steam_client::{AppsEvent, LogOnDetails, SteamClient, SteamEvent, utils::vdf::VdfValue};
use tokio::io::AsyncReadExt;

const CDN: &str = "https://shared.fastly.steamstatic.com/store_item_assets/steam/apps";
const COMMUNITY_ICON_CDN: &str =
    "https://shared.fastly.steamstatic.com/community_assets/images/apps";
const MAX_METADATA_BYTES: u64 = 16 * 1024;
const FRESH_FOR: Duration = Duration::from_secs(72 * 60 * 60);

#[derive(Deserialize, Serialize)]
pub(crate) struct PicsAssets(BTreeMap<String, String>);

impl PicsAssets {
    pub(crate) fn url(&self, app_id: u32, filename: &str) -> Option<String> {
        if filename == "clienticon.ico" {
            for (name, extension) in [("clienticon.ico", "ico"), ("clienticon.jpg", "jpg")] {
                if let Some(hash) = self.0.get(name).filter(|hash| valid_hash(hash)) {
                    return Some(format!("{COMMUNITY_ICON_CDN}/{app_id}/{hash}.{extension}"));
                }
            }
            return None;
        }
        let path = self.0.get(filename)?;
        valid_asset_path(path, filename).then(|| format!("{CDN}/{app_id}/{path}"))
    }

    fn from_vdf(info: &VdfValue) -> Self {
        let info = info.get("appinfo").unwrap_or(info);
        let common = info.get("common").unwrap_or(info);
        let mut paths = BTreeMap::new();
        if let Some(library) = common.get("library_assets_full") {
            for (key, filenames) in [
                ("library_logo", &["logo.png", "logo_2x.png"][..]),
                (
                    "library_capsule",
                    &[
                        "library_capsule.jpg",
                        "library_capsule_2x.jpg",
                        "library_600x900.jpg",
                        "library_600x900_2x.jpg",
                    ][..],
                ),
                (
                    "library_hero",
                    &["library_hero.jpg", "library_hero_2x.jpg"][..],
                ),
                ("library_hero_blur", &["library_hero_blur.jpg"][..]),
                (
                    "library_header",
                    &["library_header.jpg", "library_header_2x.jpg"][..],
                ),
            ] {
                for field in ["image", "image2x"] {
                    let Some(image) = library
                        .get(key)
                        .and_then(|asset| asset.get(field))
                        .and_then(|value| value.get_str("english"))
                    else {
                        continue;
                    };
                    let hash = image.split('/').next().unwrap_or_default();
                    if !valid_hash(hash) {
                        continue;
                    }
                    for filename in filenames {
                        if image == hash || valid_asset_path(image, filename) {
                            paths.insert((*filename).to_owned(), format!("{hash}/{filename}"));
                        }
                    }
                }
            }
        }
        for (key, filename) in [
            ("header_image", "header.jpg"),
            ("small_capsule", "capsule_231x87.jpg"),
        ] {
            if let Some(image) = common.get(key).and_then(|v| v.get_str("english"))
                && valid_asset_path(image, filename)
            {
                paths.insert(filename.to_owned(), image.to_owned());
            }
        }
        if let Some(hash) = common.get_str("clienticon").filter(|hash| valid_hash(hash)) {
            paths.insert("clienticon.ico".to_owned(), hash.to_owned());
        }
        if let Some(hash) = common.get_str("icon").filter(|hash| valid_hash(hash)) {
            paths.insert("clienticon.jpg".to_owned(), hash.to_owned());
        }
        Self(paths)
    }
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 40 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_asset_path(path: &str, filename: &str) -> bool {
    path.split_once('/')
        .is_some_and(|(hash, name)| valid_hash(hash) && name == filename)
}

async fn read_cache(path: &Path) -> Result<Option<(PicsAssets, bool)>, String> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not inspect Steam PICS cache: {error}")),
    };
    if !metadata.is_file() || metadata.len() > MAX_METADATA_BYTES {
        return Err("Invalid Steam PICS cache file.".to_owned());
    }
    let mut bytes = Vec::new();
    tokio::fs::File::open(path)
        .await
        .map_err(|error| format!("Could not open Steam PICS cache: {error}"))?
        .take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| format!("Could not read Steam PICS cache: {error}"))?;
    if bytes.len() as u64 > MAX_METADATA_BYTES {
        return Err("Steam PICS cache exceeds the size limit.".to_owned());
    }
    let assets = match serde_json::from_slice(&bytes) {
        Ok(assets) => assets,
        Err(error) => {
            if let Err(cleanup) = tokio::fs::remove_file(path).await {
                eprintln!("Could not remove an unreadable Steam PICS cache: {cleanup}");
            }
            return Err(format!("Invalid Steam PICS cache: {error}"));
        }
    };
    let stale = metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_none_or(|age| age >= FRESH_FOR);
    Ok(Some((assets, stale)))
}

async fn write_cache(path: &Path, assets: &PicsAssets) -> Result<(), String> {
    let directory = path.parent().ok_or("Invalid Steam PICS cache path.")?;
    tokio::fs::create_dir_all(directory)
        .await
        .map_err(|error| format!("Could not create Steam PICS cache: {error}"))?;
    let bytes = serde_json::to_vec(assets)
        .map_err(|error| format!("Could not encode Steam PICS cache: {error}"))?;
    let temporary = directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(|error| format!("Could not write Steam PICS cache: {error}"))?;
    if let Err(error) = tokio::fs::rename(&temporary, path).await {
        if let Err(cleanup) = tokio::fs::remove_file(&temporary).await {
            eprintln!("Could not remove temporary Steam PICS cache: {cleanup}");
        }
        return Err(format!("Could not save Steam PICS cache: {error}"));
    }
    Ok(())
}

pub(crate) async fn resolve(
    path: Result<PathBuf, String>,
    lock: &tokio::sync::Mutex<()>,
    fetched: impl Future<Output = Result<PicsAssets, String>>,
) -> (Option<PicsAssets>, Option<String>) {
    let (path, mut warning) = match path {
        Ok(path) => (Some(path), None),
        Err(error) => (None, Some(error)),
    };
    let mut cached = if let Some(path) = path.as_deref() {
        read_cache(path).await.unwrap_or_else(|error| {
            warning = Some(error);
            None
        })
    } else {
        None
    };
    if cached.as_ref().is_some_and(|(_, stale)| !stale) {
        return (cached.map(|(assets, _)| assets), warning);
    }

    // ponytail: serialize CM lookups; fresh cache reads bypass the lock.
    let _guard = lock.lock().await;
    if let Some(path) = path.as_deref() {
        match read_cache(path).await {
            Ok(Some(entry)) => cached = Some(entry),
            Ok(None) => {}
            Err(error) => warning = Some(error),
        }
    }
    if cached.as_ref().is_some_and(|(_, stale)| !stale) {
        return (cached.map(|(assets, _)| assets), warning);
    }
    match fetched.await {
        Ok(assets) => {
            if let Some(path) = path.as_deref()
                && let Err(error) = write_cache(path, &assets).await
            {
                warning = Some(error);
            }
            (Some(assets), warning)
        }
        Err(error) => {
            let error = match warning {
                Some(warning) => format!("{error} Steam PICS cache: {warning}"),
                None => error,
            };
            (cached.map(|(assets, _)| assets), Some(error))
        }
    }
}

pub(crate) async fn fetch(app_id: u32) -> Result<PicsAssets, String> {
    let mut client = SteamClient::new(Default::default());
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        client
            .log_on(LogOnDetails {
                anonymous: true,
                ..Default::default()
            })
            .await
            .map_err(|error| format!("Steam anonymous login failed: {error}"))?;
        client
            .get_product_info(vec![app_id])
            .await
            .map_err(|error| format!("Steam PICS request failed: {error}"))?;
        loop {
            if let Some(SteamEvent::Apps(AppsEvent::ProductInfoResponse {
                apps,
                unknown_apps,
                ..
            })) = client
                .poll_event()
                .await
                .map_err(|error| format!("Steam PICS response failed: {error}"))?
            {
                if let Some(app) = apps.get(&app_id) {
                    let info = app
                        .app_info
                        .as_ref()
                        .ok_or("Steam PICS returned no app metadata.")?;
                    return Ok(PicsAssets::from_vdf(info));
                }
                if unknown_apps.contains(&app_id) {
                    return Err("Steam PICS could not find this App ID.".to_owned());
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "Steam PICS request timed out.".to_owned())
    .and_then(|result| result);
    if client.is_logged_in() {
        match tokio::time::timeout(Duration::from_secs(3), client.log_off()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("Steam anonymous logout failed: {error}"),
            Err(_) => eprintln!("Steam anonymous logout timed out."),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use steam_client::utils::vdf::parse_vdf;

    #[test]
    fn hashed_library_assets_and_community_icon_are_validated() {
        let hash = "94e9d990ddd19610b268faf629865e17dcda0bb8";
        let info = parse_vdf(&format!(r#""appinfo" {{ "common" {{ "library_assets_full" {{ "library_logo" {{ "image" {{ "english" "{hash}/logo.png" }} "image2x" {{ "english" "{hash}/logo_2x.png" }} }} "library_hero" {{ "image" {{ "english" "../../evil" }} }} }} "clienticon" "{hash}" }} }}"#)).unwrap();
        let assets = PicsAssets::from_vdf(&info);
        assert_eq!(
            assets.url(4656000, "logo_2x.png"),
            Some(format!("{CDN}/4656000/{hash}/logo_2x.png"))
        );
        assert!(assets.url(4656000, "library_hero.jpg").is_none());
        assert_eq!(
            assets.url(4656000, "clienticon.ico"),
            Some(format!("{COMMUNITY_ICON_CDN}/4656000/{hash}.ico"))
        );
        assert_eq!(
            PicsAssets(BTreeMap::from([("clienticon.jpg".into(), hash.into(),)]))
                .url(4656000, "clienticon.ico"),
            Some(format!("{COMMUNITY_ICON_CDN}/4656000/{hash}.jpg"))
        );
        assert!(
            PicsAssets(BTreeMap::from([(
                "logo.png".into(),
                "../../logo.png".into()
            )]))
            .url(1, "logo.png")
            .is_none()
        );
    }

    const HASH: &str = "94e9d990ddd19610b268faf629865e17dcda0bb8";

    #[test]
    fn client_icon_prefers_alpha_capable_ico_over_jpeg() {
        let jpeg_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let info = parse_vdf(&format!(
            r#""appinfo" {{ "common" {{ "clienticon" "{HASH}" "icon" "{jpeg_hash}" }} }}"#
        ))
        .unwrap();
        assert_eq!(
            PicsAssets::from_vdf(&info).url(400, "clienticon.ico"),
            Some(format!("{COMMUNITY_ICON_CDN}/400/{HASH}.ico"))
        );
    }

    fn logo_assets(hash: &str) -> PicsAssets {
        PicsAssets(BTreeMap::from([(
            "logo.png".into(),
            format!("{hash}/logo.png"),
        )]))
    }

    fn cache_path() -> PathBuf {
        std::env::temp_dir()
            .join(format!("legio-pics-test-{}", uuid::Uuid::new_v4()))
            .join("4656000.json")
    }

    async fn expire_cache(path: &Path) {
        write_cache(path, &logo_assets(HASH)).await.unwrap();
        std::fs::File::open(path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(SystemTime::now() - FRESH_FOR - Duration::from_secs(1)),
            )
            .unwrap();
    }

    #[tokio::test]
    async fn cache_miss_fetches_and_persists_metadata() {
        let path = cache_path();
        let lock = tokio::sync::Mutex::new(());
        let (assets, warning) =
            resolve(Ok(path.clone()), &lock, async { Ok(logo_assets(HASH)) }).await;
        let (saved, stale) = read_cache(&path).await.unwrap().unwrap();
        assert_eq!(
            saved.url(4656000, "logo.png"),
            assets.unwrap().url(4656000, "logo.png")
        );
        assert!(!stale);
        assert!(warning.is_none());
        tokio::fs::remove_dir_all(path.parent().unwrap())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn fresh_metadata_bypasses_network_and_lookup_lock() {
        let path = cache_path();
        write_cache(&path, &logo_assets(HASH)).await.unwrap();
        let lock = tokio::sync::Mutex::new(());
        let _guard = lock.lock().await;
        let (assets, warning) = tokio::time::timeout(
            Duration::from_secs(3),
            resolve(Ok(path.clone()), &lock, async {
                panic!("Fresh metadata must not connect to Steam")
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            assets.unwrap().url(4656000, "logo.png"),
            logo_assets(HASH).url(4656000, "logo.png")
        );
        assert!(warning.is_none());
        tokio::fs::remove_dir_all(path.parent().unwrap())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn expired_metadata_refreshes_and_saves_the_new_hash() {
        let path = cache_path();
        expire_cache(&path).await;
        let lock = tokio::sync::Mutex::new(());
        let hash = "b".repeat(40);
        let (assets, warning) =
            resolve(Ok(path.clone()), &lock, async { Ok(logo_assets(&hash)) }).await;
        let expected = logo_assets(&hash).url(4656000, "logo.png");
        assert_eq!(assets.unwrap().url(4656000, "logo.png"), expected);
        let (saved, stale) = read_cache(&path).await.unwrap().unwrap();
        assert_eq!(saved.url(4656000, "logo.png"), expected);
        assert!(!stale);
        assert!(warning.is_none());
        tokio::fs::remove_dir_all(path.parent().unwrap())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn expired_metadata_is_preserved_when_steam_is_offline() {
        let path = cache_path();
        expire_cache(&path).await;
        let lock = tokio::sync::Mutex::new(());
        let (assets, warning) = resolve(Ok(path.clone()), &lock, async {
            Err("Steam is offline".into())
        })
        .await;
        assert_eq!(
            assets.unwrap().url(4656000, "logo.png"),
            logo_assets(HASH).url(4656000, "logo.png")
        );
        assert_eq!(warning.as_deref(), Some("Steam is offline"));
        assert!(read_cache(&path).await.unwrap().unwrap().1);
        tokio::fs::remove_dir_all(path.parent().unwrap())
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires a live anonymous Steam CM connection"]
    async fn live_pics_resolves_modern_assets() {
        let assets = fetch(4656000).await.unwrap();
        for filename in [
            "logo.png",
            "logo_2x.png",
            "library_capsule.jpg",
            "library_capsule_2x.jpg",
            "library_hero.jpg",
            "library_hero_2x.jpg",
            "library_hero_blur.jpg",
            "library_header.jpg",
            "library_header_2x.jpg",
            "header.jpg",
            "capsule_231x87.jpg",
            "clienticon.jpg",
        ] {
            assert!(
                assets.url(4656000, filename).is_some(),
                "Missing {filename}"
            );
        }
    }
}
