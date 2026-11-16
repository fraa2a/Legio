//! ProtonDB compatibility summaries for the Linux game page.
//!
//! The summary endpoint (`/api/v1/reports/summaries/<appid>.json`) is public but
//! unofficial: ProtonDB documents no contract for it, so the response is treated as
//! untrusted, every field is optional and a failure only hides the badge. Results are
//! cached as one JSON file per App ID in the app cache directory.

use std::{
    future::Future,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tauri::{Manager, Runtime};

use crate::database::now_seconds;
use crate::network::{NetworkError, NetworkState};

const SUMMARY_URL: &str = "https://www.protondb.com/api/v1/reports/summaries";
pub(crate) const MAX_SUMMARY_BYTES: usize = 64 * 1024;
const TTL_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Platinum,
    Gold,
    Silver,
    Bronze,
    Borked,
    Pending,
    Unknown,
}

impl Tier {
    fn from_remote(value: &str) -> Self {
        match value {
            "platinum" => Self::Platinum,
            "gold" => Self::Gold,
            "silver" => Self::Silver,
            "bronze" => Self::Bronze,
            "borked" => Self::Borked,
            "pending" => Self::Pending,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Strong,
    Good,
    Moderate,
    Low,
    Inadequate,
    Unknown,
}

impl Confidence {
    fn from_remote(value: &str) -> Self {
        match value {
            "strong" => Self::Strong,
            "good" => Self::Good,
            "moderate" => Self::Moderate,
            "low" => Self::Low,
            "inadequate" => Self::Inadequate,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub tier: Tier,
    pub confidence: Option<Confidence>,
    pub score: Option<f64>,
    pub total_reports: u64,
    pub trending_tier: Option<Tier>,
    pub best_reported_tier: Option<Tier>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteSummary {
    tier: Option<String>,
    confidence: Option<String>,
    score: Option<f64>,
    total: Option<u64>,
    trending_tier: Option<String>,
    best_reported_tier: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProtonDbResult {
    /// `None` when ProtonDB has no reports for the game.
    pub summary: Option<Summary>,
    pub cached_at: Option<i64>,
    pub stale: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheEntry {
    fetched_at: i64,
    summary: Option<Summary>,
}

pub(crate) fn summary_url(app_id: u32) -> String {
    format!("{SUMMARY_URL}/{app_id}.json")
}

fn parse(bytes: &[u8]) -> Result<Summary, String> {
    if bytes.len() > MAX_SUMMARY_BYTES {
        return Err("ProtonDB returned a response larger than the size limit.".to_owned());
    }
    let remote: RemoteSummary = serde_json::from_slice(bytes)
        .map_err(|error| format!("ProtonDB returned an unreadable summary: {error}"))?;
    let tier = |value: Option<String>| value.as_deref().map(Tier::from_remote);
    Ok(Summary {
        tier: tier(remote.tier).unwrap_or(Tier::Unknown),
        confidence: remote.confidence.as_deref().map(Confidence::from_remote),
        score: remote.score,
        total_reports: remote.total.unwrap_or(0),
        trending_tier: tier(remote.trending_tier),
        best_reported_tier: tier(remote.best_reported_tier),
    })
}

fn cache_path(directory: &Path, app_id: u32) -> PathBuf {
    directory.join(format!("{app_id}.json"))
}

async fn read_cache(directory: &Path, app_id: u32) -> Option<CacheEntry> {
    let bytes = match tokio::fs::read(cache_path(directory, app_id)).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            crate::application_log::failure("read_protondb_cache", error);
            return None;
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(entry) => Some(entry),
        Err(error) => {
            crate::application_log::failure("read_protondb_cache", error);
            None
        }
    }
}

async fn write_cache(directory: &Path, app_id: u32, entry: &CacheEntry) -> Result<(), String> {
    let bytes = serde_json::to_vec(entry).map_err(|error| error.to_string())?;
    tokio::fs::create_dir_all(directory)
        .await
        .map_err(|error| error.to_string())?;
    let path = cache_path(directory, app_id);
    tauri::async_runtime::spawn_blocking(move || crate::atomic_file::write_atomic(&path, &bytes))
        .await
        .map_err(std::io::Error::other)
        .and_then(|written| written)
        .map_err(|error| error.to_string())
}

fn is_fresh(entry: &CacheEntry, now: i64) -> bool {
    (0..TTL_SECONDS).contains(&(now - entry.fetched_at))
}

fn result(entry: CacheEntry, stale: bool) -> ProtonDbResult {
    ProtonDbResult {
        summary: entry.summary,
        cached_at: Some(entry.fetched_at),
        stale,
    }
}

/// Serves the cache while it is fresh, otherwise fetches and stores the summary.
/// A failed refresh falls back to the expired cache entry when there is one.
async fn load_summary<Fetch, Pending>(
    directory: &Path,
    app_id: u32,
    now: i64,
    fetch: Fetch,
) -> Result<ProtonDbResult, String>
where
    Fetch: FnOnce() -> Pending,
    Pending: Future<Output = Result<Vec<u8>, NetworkError>>,
{
    let cached = read_cache(directory, app_id).await;
    let cached = match cached {
        Some(entry) if is_fresh(&entry, now) => return Ok(result(entry, false)),
        other => other,
    };
    let fetched = match fetch().await {
        Ok(bytes) => parse(&bytes).map(Some),
        Err(NetworkError::Http { status: 404 }) => Ok(None),
        Err(error) => Err(format!("Could not reach ProtonDB: {error}")),
    };
    match fetched {
        Ok(summary) => {
            let entry = CacheEntry {
                fetched_at: now,
                summary,
            };
            if let Err(error) = write_cache(directory, app_id, &entry).await {
                crate::application_log::failure("write_protondb_cache", error);
            }
            Ok(result(entry, false))
        }
        Err(message) => {
            crate::application_log::failure("protondb_summary", &message);
            cached.map(|entry| result(entry, true)).ok_or(message)
        }
    }
}

pub async fn get_summary<R: Runtime>(
    app: &tauri::AppHandle<R>,
    network: &NetworkState,
    app_id: u32,
) -> Result<ProtonDbResult, String> {
    if app_id == 0 {
        return Err("Steam App ID must be a positive 32-bit integer.".to_owned());
    }
    let directory = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Could not resolve the cache directory: {error}"))?
        .join("protondb");
    load_summary(&directory, app_id, now_seconds()?, || {
        network.protondb_summary(app_id)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000_000_000;

    fn directory() -> PathBuf {
        std::env::temp_dir().join(format!("legio-protondb-{}", uuid::Uuid::new_v4()))
    }

    fn body(tier: &str) -> Vec<u8> {
        format!(
            r#"{{"bestReportedTier":"platinum","confidence":"strong","score":0.87,"tier":"{tier}","total":1234,"trendingTier":"gold"}}"#
        )
        .into_bytes()
    }

    fn run(
        directory: &Path,
        now: i64,
        response: Result<Vec<u8>, NetworkError>,
    ) -> Result<ProtonDbResult, String> {
        tauri::async_runtime::block_on(load_summary(directory, 620, now, || async { response }))
    }

    #[test]
    fn parses_every_tier() {
        for (name, tier) in [
            ("platinum", Tier::Platinum),
            ("gold", Tier::Gold),
            ("silver", Tier::Silver),
            ("bronze", Tier::Bronze),
            ("borked", Tier::Borked),
            ("pending", Tier::Pending),
        ] {
            assert_eq!(parse(&body(name)).unwrap().tier, tier, "{name}");
        }
    }

    #[test]
    fn parses_all_fields() {
        assert_eq!(
            parse(&body("gold")).unwrap(),
            Summary {
                tier: Tier::Gold,
                confidence: Some(Confidence::Strong),
                score: Some(0.87),
                total_reports: 1234,
                trending_tier: Some(Tier::Gold),
                best_reported_tier: Some(Tier::Platinum),
            }
        );
    }

    #[test]
    fn maps_unknown_and_missing_values() {
        let summary = parse(br#"{"tier":"diamond","confidence":"huge"}"#).unwrap();
        assert_eq!(summary.tier, Tier::Unknown);
        assert_eq!(summary.confidence, Some(Confidence::Unknown));
        let summary = parse(b"{}").unwrap();
        assert_eq!(summary.tier, Tier::Unknown);
        assert_eq!(summary.total_reports, 0);
        assert_eq!(summary.confidence, None);
    }

    #[test]
    fn rejects_malformed_and_oversized_bodies() {
        for bytes in [&b"not json"[..], b"[]", br#"{"tier":5}"#, b""] {
            assert!(parse(bytes).is_err());
        }
        let mut oversized = body("gold");
        oversized.resize(MAX_SUMMARY_BYTES + 1, b' ');
        assert!(parse(&oversized).is_err());
    }

    #[test]
    fn builds_the_summary_url_from_the_app_id() {
        assert_eq!(
            summary_url(620),
            "https://www.protondb.com/api/v1/reports/summaries/620.json"
        );
    }

    #[test]
    fn treats_404_as_no_reports_and_caches_it() {
        let directory = directory();
        let missing = run(&directory, NOW, Err(NetworkError::Http { status: 404 })).unwrap();
        assert_eq!(missing.summary, None);
        assert!(!missing.stale);
        let cached = run(&directory, NOW + 10, Err(NetworkError::Timeout)).unwrap();
        assert_eq!(cached.summary, None);
        assert_eq!(cached.cached_at, Some(NOW));
        assert!(!cached.stale);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn serves_a_fresh_cache_without_fetching() {
        let directory = directory();
        run(&directory, NOW, Ok(body("gold"))).unwrap();
        let cached = run(
            &directory,
            NOW + TTL_SECONDS - 1,
            Err(NetworkError::Timeout),
        )
        .unwrap();
        assert_eq!(cached.summary.unwrap().tier, Tier::Gold);
        assert!(!cached.stale);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn refreshes_after_the_ttl() {
        let directory = directory();
        run(&directory, NOW, Ok(body("gold"))).unwrap();
        let later = NOW + TTL_SECONDS;
        let refreshed = run(&directory, later, Ok(body("silver"))).unwrap();
        assert_eq!(refreshed.summary.unwrap().tier, Tier::Silver);
        assert_eq!(refreshed.cached_at, Some(later));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn serves_stale_data_when_the_refresh_fails() {
        let directory = directory();
        run(&directory, NOW, Ok(body("gold"))).unwrap();
        let later = NOW + TTL_SECONDS + 1;
        for failure in [
            Err(NetworkError::Timeout),
            Err(NetworkError::Http { status: 503 }),
            Ok(b"not json".to_vec()),
        ] {
            let stale = run(&directory, later, failure).unwrap();
            assert_eq!(stale.summary.unwrap().tier, Tier::Gold);
            assert_eq!(stale.cached_at, Some(NOW));
            assert!(stale.stale);
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn fails_without_a_cache_when_the_fetch_fails() {
        let directory = directory();
        assert!(run(&directory, NOW, Err(NetworkError::TooLarge)).is_err());
        assert!(run(&directory, NOW, Ok(b"not json".to_vec())).is_err());
    }

    #[test]
    fn ignores_a_corrupt_cache_file() {
        let directory = directory();
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(cache_path(&directory, 620), b"{broken").unwrap();
        let fetched = run(&directory, NOW, Ok(body("bronze"))).unwrap();
        assert_eq!(fetched.summary.unwrap().tier, Tier::Bronze);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
