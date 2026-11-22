use std::{
    future::Future,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::DateTime;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{
    database::{Database, DatabaseState},
    network::{NetworkError, NetworkState},
};

const MAX_RELEASES: usize = 10;
pub const URL: &str = "https://api.github.com/repos/fraa2a/Legio/releases?per_page=10";
pub const MAX_BYTES: usize = 1024 * 1024;
// Matches MAX_NOTES_CHARS in src/lib/stores/update-notes.ts.
const MAX_NOTES_CHARS: usize = 20_000;
const RELEASE_PAGE_PREFIX: &str = "https://github.com/fraa2a/Legio/releases/";
// The privacy page promises at most one automatic changelog request an hour.
const REFRESH_INTERVAL_SECS: i64 = 60 * 60;

/// The only fields read from a GitHub release; everything else is ignored.
#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    published_at: Option<String>,
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    version: String,
    published_at: String,
    notes: String,
    url: String,
    prerelease: bool,
}

#[derive(Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    releases: Vec<Release>,
    cached_at: Option<i64>,
    warning: Option<String>,
}

fn is_valid(release: &Release) -> bool {
    !release.version.is_empty()
        && release.version.len() <= 64
        && release
            .version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        && DateTime::parse_from_rfc3339(&release.published_at).is_ok()
        && release.notes.chars().count() <= MAX_NOTES_CHARS
        && release.url.starts_with(RELEASE_PAGE_PREFIX)
        && reqwest::Url::parse(&release.url).is_ok()
}

fn notes(body: Option<String>) -> String {
    body.unwrap_or_default()
        .replace("\r\n", "\n")
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .take(MAX_NOTES_CHARS)
        .collect()
}

/// Parses an untrusted GitHub releases response. Drafts and invalid entries are skipped.
pub(crate) fn parse(bytes: &[u8]) -> Result<Vec<Release>, String> {
    if bytes.len() > MAX_BYTES {
        return Err("The changelog exceeds 1 MiB.".to_owned());
    }
    let releases: Vec<GitHubRelease> = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid changelog JSON: {error}"))?;
    Ok(releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            Some(Release {
                version: release.tag_name.trim_start_matches('v').to_owned(),
                published_at: release.published_at?,
                notes: notes(release.body),
                url: release.html_url,
                prerelease: release.prerelease,
            })
        })
        .filter(is_valid)
        .take(MAX_RELEASES)
        .collect())
}

fn read_cache(database: &Database) -> Result<Snapshot, String> {
    let row: Option<String> = database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'changelog_cache'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())
    })?;
    let Some(raw) = row else {
        return Ok(Snapshot::default());
    };
    if raw.len() > MAX_BYTES {
        return Err("The cached changelog exceeds the size limit.".to_owned());
    }
    let snapshot: Snapshot =
        serde_json::from_str(&raw).map_err(|error| format!("Invalid cached changelog: {error}"))?;
    if !snapshot.releases.iter().all(is_valid) {
        return Err("Invalid cached changelog.".to_owned());
    }
    Ok(snapshot)
}

pub async fn cached(app: AppHandle) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        read_cache(app.state::<DatabaseState>().database()?)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn now_secs() -> Result<i64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .map_err(|error| error.to_string())
}

fn is_fresh(snapshot: &Snapshot, now: i64) -> bool {
    snapshot
        .cached_at
        .is_some_and(|cached_at| (0..REFRESH_INTERVAL_SECS).contains(&(now - cached_at)))
}

pub async fn refresh(
    app: AppHandle,
    state: &NetworkState,
    force: bool,
) -> Result<Snapshot, String> {
    let database = app.state::<DatabaseState>().shared_database()?;
    refresh_with(database, force, || state.changelog()).await
}

/// Calls `fetch` only while the automatic update check is on and, unless `force`, the cache is
/// older than an hour; otherwise returns the cache untouched.
async fn refresh_with<F, Fut>(
    database: Arc<Database>,
    force: bool,
    fetch: F,
) -> Result<Snapshot, String>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Vec<u8>, NetworkError>>,
{
    let checked = database.clone();
    let cache = tauri::async_runtime::spawn_blocking(move || {
        let enabled = checked.settings()?.automatic_update_check;
        let cache = read_cache(&checked);
        let fresh = match &cache {
            Ok(cache) => !force && is_fresh(cache, now_secs()?),
            Err(_) => false,
        };
        if !enabled || fresh {
            return cache.map(Some);
        }
        Ok(None)
    })
    .await
    .map_err(|error| error.to_string())??;
    if let Some(cache) = cache {
        return Ok(cache);
    }
    let fetched = fetch()
        .await
        .map_err(|error| format!("Could not fetch the changelog: {error}"));
    tauri::async_runtime::spawn_blocking(move || {
        let database = database.as_ref();
        let fresh = fetched.and_then(|bytes| parse(&bytes)).and_then(|releases| {
            let snapshot = Snapshot {
                releases,
                cached_at: Some(now_secs()?),
                warning: None,
            };
            let raw = serde_json::to_string(&snapshot).map_err(|error| error.to_string())?;
            database.with_connection(|connection| {
                connection
                    .execute(
                        "INSERT INTO settings (key, value) VALUES ('changelog_cache', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                        [raw],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })?;
            Ok(snapshot)
        });
        match fresh {
            Ok(snapshot) => Ok(snapshot),
            Err(error) => {
                let mut snapshot = read_cache(database)?;
                snapshot.warning = Some(error);
                Ok(snapshot)
            }
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, draft: bool) -> serde_json::Value {
        serde_json::json!({
            "tag_name": tag, "name": "Legio", "draft": draft, "prerelease": false,
            "published_at": if draft { serde_json::Value::Null } else { "2026-10-04T12:00:00Z".into() },
            "body": "Fixes\r\n- one", "html_url": format!("https://github.com/fraa2a/Legio/releases/tag/{tag}"),
            "assets": [], "author": {"login": "someone"}
        })
    }

    fn bytes(value: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&value).unwrap()
    }

    #[test]
    fn skips_drafts() {
        let releases = parse(&bytes(serde_json::json!([
            release("v1.1.0", true),
            release("v1.0.0", false)
        ])))
        .unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].version, "1.0.0");
        assert_eq!(releases[0].notes, "Fixes\n- one");
    }

    #[test]
    fn rejects_malformed_json() {
        for invalid in [
            &b"not json"[..],
            br#"{"tag_name":"v1"}"#,
            br#"[{"tag_name":1}]"#,
        ] {
            assert!(parse(invalid).is_err());
        }
        assert!(parse(&vec![b' '; MAX_BYTES + 1]).is_err());
    }

    #[test]
    fn skips_untrusted_links_and_tags() {
        let mut link = release("v1.0.0", false);
        link["html_url"] = "https://example.com/releases/tag/v1.0.0".into();
        let mut tag = release("v1.0.0", false);
        tag["tag_name"] = "<script>".into();
        assert!(
            parse(&bytes(serde_json::json!([link, tag])))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn truncates_long_notes() {
        let mut long = release("v1.0.0", false);
        long["body"] = "a".repeat(MAX_NOTES_CHARS + 10).into();
        let releases = parse(&bytes(serde_json::json!([long]))).unwrap();
        assert_eq!(releases[0].notes.chars().count(), MAX_NOTES_CHARS);
    }

    #[test]
    fn failure_keeps_cache() {
        let root = std::env::temp_dir().join(format!("legio-changelog-{}", uuid::Uuid::new_v4()));
        let database = DatabaseState::new(Ok(root.clone()))
            .shared_database()
            .unwrap();
        let body = bytes(serde_json::json!([release("v1.0.0", false)]));
        let requests = std::sync::atomic::AtomicUsize::new(0);
        let count = || requests.load(std::sync::atomic::Ordering::SeqCst);
        let fetch = || async {
            requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(body.clone())
        };
        let fresh =
            tauri::async_runtime::block_on(refresh_with(database.clone(), false, fetch)).unwrap();
        assert_eq!(fresh.releases.len(), 1);
        // A recent cache is reused unless the refresh is forced.
        let reused =
            tauri::async_runtime::block_on(refresh_with(database.clone(), false, fetch)).unwrap();
        assert_eq!(reused, fresh);
        assert_eq!(count(), 1);
        let failed =
            tauri::async_runtime::block_on(refresh_with(database.clone(), true, || async {
                Err(NetworkError::Timeout)
            }))
            .unwrap();
        assert_eq!(failed.releases, fresh.releases);
        assert!(failed.warning.is_some());
        // With the automatic update check off, only the cache is returned.
        let mut settings = database.settings().unwrap();
        settings.automatic_update_check = false;
        database.save_settings(settings).unwrap();
        let cached = tauri::async_runtime::block_on(refresh_with(database, true, fetch)).unwrap();
        assert_eq!(cached.releases, fresh.releases);
        assert_eq!(count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
