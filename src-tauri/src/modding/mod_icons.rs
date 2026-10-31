//! Thunderstore package icons fetched by the backend and cached on disk,
//! because the webview only loads local images.

use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};

use super::{
    paths::ModdingPaths,
    thunderstore_http::{ThunderstoreHttp, is_allowed_url},
};
use crate::image_format::ImageFormat;

// Thunderstore icons are 256x256 PNGs, usually well under 100 KiB.
const MAX_ICON_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ModIcon {
    content_type: &'static str,
}

#[tauri::command]
pub async fn get_mod_icon(
    app: AppHandle,
    http: State<'_, ThunderstoreHttp>,
    url: String,
) -> Result<tauri::ipc::Response, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))?;
    let directory = ModdingPaths::new(&data_dir).icon_cache_dir();
    let (bytes, format) = load_icon(&http, &directory, &url).await?;
    let metadata = serde_json::to_vec(&ModIcon {
        content_type: format.content_type(),
    })
    .map_err(|error| error.to_string())?;
    crate::image_response::encode(metadata, bytes)
}

/// Returns the cached icon for `url`, downloading and caching it on a miss.
async fn load_icon(
    http: &ThunderstoreHttp,
    directory: &Path,
    url: &str,
) -> Result<(Vec<u8>, ImageFormat), String> {
    let parsed = reqwest::Url::parse(url)
        .ok()
        .filter(is_allowed_url)
        .ok_or_else(|| "The mod icon address is not a Thunderstore address".to_owned())?;
    let key = format!("{:x}", Sha256::digest(parsed.as_str().as_bytes()));
    if let Some(cached) = read_cached(directory, &key).await {
        return Ok(cached);
    }
    let bytes = http.get_bytes(parsed.as_str(), MAX_ICON_BYTES).await?;
    let format = ImageFormat::from_bytes(&bytes)
        .ok_or_else(|| "Thunderstore sent a mod icon that is not a supported image".to_owned())?;
    if let Err(error) = write_cached(directory, &key, format, &bytes).await {
        crate::application_log::failure("cache_mod_icon", error);
    }
    Ok((bytes, format))
}

async fn read_cached(directory: &Path, key: &str) -> Option<(Vec<u8>, ImageFormat)> {
    for format in ImageFormat::ALL {
        let path = cached_path(directory, key, format);
        match tokio::fs::read(&path).await {
            // A file that does not hold the expected image is refetched.
            Ok(bytes) if ImageFormat::from_bytes(&bytes) == Some(format) => {
                return Some((bytes, format));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => crate::application_log::failure("read_mod_icon", error),
        }
    }
    None
}

async fn write_cached(
    directory: &Path,
    key: &str,
    format: ImageFormat,
    bytes: &[u8],
) -> std::io::Result<()> {
    tokio::fs::create_dir_all(directory).await?;
    let destination = cached_path(directory, key, format);
    // A unique partial name keeps concurrent fetches of one icon apart.
    let partial = directory.join(format!("{key}.{}.part", uuid::Uuid::new_v4()));
    tokio::fs::write(&partial, bytes).await?;
    if let Err(error) = tokio::fs::rename(&partial, &destination).await {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err(error);
    }
    Ok(())
}

fn cached_path(directory: &Path, key: &str, format: ImageFormat) -> PathBuf {
    directory.join(format!("{key}.{}", format.extension()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        diagnostics::Diagnostics,
        modding::test_support::{Reply, TempDir, TestServer},
    };

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nicon";

    fn http() -> ThunderstoreHttp {
        ThunderstoreHttp::new(
            "0.1.0",
            Diagnostics::new(Err("test logs disabled".to_owned())),
        )
        .unwrap()
    }

    #[test]
    fn caches_icons_after_the_first_fetch() {
        let server = TestServer::start();
        server.set("/icon.png", Reply::ok(PNG.to_vec()));
        let directory = TempDir::new();
        let url = server.url("/icon.png");
        let http = http();
        for _ in 0..2 {
            let (bytes, format) =
                tauri::async_runtime::block_on(load_icon(&http, &directory, &url)).unwrap();
            assert_eq!(bytes, PNG);
            assert_eq!(format, ImageFormat::Png);
        }
        assert_eq!(server.requests().len(), 1);
        let files: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with(".png"), "{files:?}");
    }

    #[test]
    fn rejects_other_hosts_and_non_images() {
        let directory = TempDir::new();
        let error = tauri::async_runtime::block_on(load_icon(
            &http(),
            &directory,
            "https://example.com/icon.png",
        ))
        .unwrap_err();
        assert!(error.contains("not a Thunderstore address"), "{error}");

        let server = TestServer::start();
        server.set("/page", Reply::ok(b"<html></html>".to_vec()));
        let error =
            tauri::async_runtime::block_on(load_icon(&http(), &directory, &server.url("/page")))
                .unwrap_err();
        assert!(error.contains("not a supported image"), "{error}");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
