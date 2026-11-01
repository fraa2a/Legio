//! Local SQLite copy of the Thunderstore package listings, one community per
//! game. Kept apart from the main Legio database because it is large,
//! disposable and rewritten wholesale on every sync.

use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use flate2::read::GzDecoder;
use rusqlite::{
    Connection, OptionalExtension, Transaction, params, params_from_iter, types::Value,
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use super::{
    external_url::parse_external_url,
    package_id::{PackageName, PackageVersionId, Version},
    thunderstore_http::ThunderstoreHttp,
};

const SCHEMA_VERSION: i64 = 2;
const STALE_AFTER_SECONDS: i64 = 60 * 60;
const MAX_INDEX_BYTES: usize = 2 * 1024 * 1024;
const MAX_INDEX_JSON_BYTES: usize = 8 * 1024 * 1024;
const MAX_CHUNKS: usize = 1024;
const MAX_CHUNK_BYTES: usize = 64 * 1024 * 1024;
const MAX_CHUNK_JSON_BYTES: usize = 256 * 1024 * 1024;
const DEFAULT_PAGE_SIZE: u32 = 30;
const MAX_PAGE_SIZE: u32 = 200;
const MAX_QUERY_TERMS: usize = 8;
const MAX_QUERY_CATEGORIES: usize = 64;
// Packages parsed ahead of the database writer.
const PIPELINE_DEPTH: usize = 2;

/// Packages of every synced community. Cheap to clone; all clones share one
/// read connection.
#[derive(Clone)]
pub(crate) struct PackageCache {
    inner: Arc<Inner>,
}

struct Inner {
    path: PathBuf,
    reader: Mutex<Option<Connection>>,
    sync_lock: tokio::sync::Mutex<()>,
}

/// State of a community's local copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogStatus {
    pub(crate) ecosystem_game_id: String,
    pub(crate) package_count: u64,
    /// Unix seconds of the last successful check against Thunderstore.
    pub(crate) synced_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PackageSort {
    #[default]
    LastUpdated,
    MostDownloads,
    TopRated,
    Newest,
    Alphabetical,
}

/// How `PackageQuery::categories` combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CategoryMode {
    /// In at least one of the categories.
    #[default]
    Any,
    /// In every one of the categories.
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct PackageQuery {
    /// Whitespace separated terms, all of which must match the package name,
    /// owner or description.
    pub(crate) text: String,
    /// Packages in these categories, matched according to `category_mode`;
    /// empty means any.
    pub(crate) categories: Vec<String>,
    pub(crate) category_mode: CategoryMode,
    /// Packages in none of these categories. Wins over `categories`.
    pub(crate) excluded_categories: Vec<String>,
    pub(crate) include_deprecated: bool,
    pub(crate) include_nsfw: bool,
    pub(crate) sort: PackageSort,
    pub(crate) offset: u32,
    /// Clamped to 1..=200.
    pub(crate) limit: u32,
}

impl Default for PackageQuery {
    fn default() -> Self {
        Self {
            text: String::new(),
            categories: Vec::new(),
            category_mode: CategoryMode::default(),
            excluded_categories: Vec::new(),
            include_deprecated: false,
            include_nsfw: false,
            sort: PackageSort::default(),
            offset: 0,
            limit: DEFAULT_PAGE_SIZE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackagePage {
    /// Matches across all pages.
    pub(crate) total: u64,
    pub(crate) items: Vec<PackageSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageSummary {
    pub(crate) full_name: String,
    pub(crate) owner: String,
    pub(crate) name: String,
    pub(crate) package_url: String,
    pub(crate) categories: Vec<String>,
    pub(crate) is_deprecated: bool,
    pub(crate) is_pinned: bool,
    pub(crate) has_nsfw_content: bool,
    pub(crate) rating_score: i64,
    pub(crate) date_created: String,
    pub(crate) date_updated: String,
    /// Sum over all versions.
    pub(crate) total_downloads: u64,
    pub(crate) latest_version: String,
    /// Description and icon of the latest version.
    pub(crate) description: String,
    pub(crate) icon: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageDetails {
    #[serde(flatten)]
    pub(crate) summary: PackageSummary,
    /// Web address the author asks donations to go to.
    pub(crate) donation_link: Option<String>,
    /// Newest first.
    pub(crate) versions: Vec<PackageVersionDetails>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageVersionDetails {
    pub(crate) full_name: String,
    pub(crate) version_number: String,
    pub(crate) description: String,
    pub(crate) icon: String,
    pub(crate) download_url: String,
    pub(crate) file_size: u64,
    /// `Owner-Name-x.y.z` strings as published.
    pub(crate) dependencies: Vec<String>,
    pub(crate) downloads: u64,
    pub(crate) date_created: String,
    /// The website the author set in the version manifest.
    pub(crate) website_url: Option<String>,
}

/// What the installer needs to fetch and resolve one package version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackageVersionRecord {
    pub(crate) id: PackageVersionId,
    pub(crate) download_url: String,
    pub(crate) file_size: u64,
    pub(crate) dependencies: Vec<PackageVersionId>,
    pub(crate) description: String,
    pub(crate) icon: String,
}

impl PackageCache {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            inner: Arc::new(Inner {
                path,
                reader: Mutex::new(None),
                sync_lock: tokio::sync::Mutex::new(()),
            }),
        }
    }

    /// Local copy state, or `None` before the first sync of `community`.
    pub(crate) fn status(&self, community: &str) -> Result<Option<CatalogStatus>, String> {
        self.with_reader(|connection| read_status(connection, community))
    }

    pub(crate) fn search(
        &self,
        community: &str,
        query: &PackageQuery,
    ) -> Result<PackagePage, String> {
        self.with_reader(|connection| search(connection, community, query))
    }

    pub(crate) fn package(
        &self,
        community: &str,
        name: &PackageName,
    ) -> Result<Option<PackageDetails>, String> {
        self.with_reader(|connection| package_details(connection, community, name))
    }

    pub(crate) fn version(
        &self,
        community: &str,
        id: &PackageVersionId,
    ) -> Result<Option<PackageVersionRecord>, String> {
        self.with_reader(|connection| {
            version_record(connection, community, &id.package, Some(&id.version))
        })
    }

    pub(crate) fn latest_version(
        &self,
        community: &str,
        name: &PackageName,
    ) -> Result<Option<PackageVersionRecord>, String> {
        self.with_reader(|connection| version_record(connection, community, name, None))
    }

    /// Category names in alphabetical order.
    pub(crate) fn categories(&self, community: &str) -> Result<Vec<String>, String> {
        self.with_reader(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT DISTINCT category FROM package_categories
                     WHERE community = ?1 ORDER BY category COLLATE NOCASE, category",
                )
                .map_err(database_error)?;
            statement
                .query_map([community], |row| row.get(0))
                .map_err(database_error)?
                .collect::<Result<_, _>>()
                .map_err(database_error)
        })
    }

    /// Brings the local copy of `community` up to date with the listing at
    /// `index_url`. Does nothing within an hour of the last check unless
    /// `force`. Chunks are only downloaded when the index changed, and the new
    /// listing replaces the old one in a single transaction, so a failure
    /// leaves the previous copy untouched.
    pub(crate) async fn sync(
        &self,
        http: &ThunderstoreHttp,
        community: &str,
        index_url: &str,
        force: bool,
        on_progress: impl Fn(u32, u32) + Sync,
    ) -> Result<CatalogStatus, String> {
        let _only_sync = self.inner.sync_lock.lock().await;
        let known = self
            .run_blocking({
                let community = community.to_owned();
                move |cache| cache.with_reader(|connection| read_state(connection, &community))
            })
            .await?;
        let now = unix_seconds();
        if !force
            && let Some(known) = &known
            && now.saturating_sub(known.synced_at) < STALE_AFTER_SECONDS
        {
            return Ok(known.status(community));
        }

        let compressed = http.get_bytes(index_url, MAX_INDEX_BYTES).await?;
        let hash = hex_digest(&compressed);
        if let Some(known) = &known
            && known.index_hash == hash
        {
            let community = community.to_owned();
            return self
                .run_blocking(move |cache| cache.touch_sync(&community, now))
                .await;
        }
        let chunk_urls = parse_index(compressed).await?;

        let (sender, receiver) = mpsc::channel(PIPELINE_DEPTH);
        let writer = {
            let path = self.inner.path.clone();
            let community = community.to_owned();
            let hash = hash.clone();
            tauri::async_runtime::spawn_blocking(move || {
                write_catalog(&path, &community, &hash, now, receiver)
            })
        };
        let fetched = fetch_chunks(http, &chunk_urls, &sender, &on_progress).await;
        let skipped = match fetched {
            Ok(skipped) => skipped,
            Err(failure) => {
                // Without a commit message the writer rolls back.
                drop(sender);
                let written = writer
                    .await
                    .map_err(|error| format!("Catalog write task failed: {error}"))?;
                return Err(match (failure, written) {
                    (ChunkError::WriterStopped, Err(error)) => error,
                    (ChunkError::WriterStopped, Ok(_)) => {
                        "The catalog writer stopped unexpectedly".to_owned()
                    }
                    (ChunkError::Fetch(error), _) => error,
                });
            }
        };
        let committed = sender.send(Message::Commit).await;
        drop(sender);
        let written = writer
            .await
            .map_err(|error| format!("Catalog write task failed: {error}"))?;
        if committed.is_err() && written.is_ok() {
            return Err("The catalog writer stopped unexpectedly".to_owned());
        }
        let package_count = written?;
        if skipped > 0 {
            crate::application_log::state_change(
                "mod_catalog_sync",
                &format!("{community}: skipped {skipped} entries that could not be read"),
            );
        }
        Ok(CatalogStatus {
            ecosystem_game_id: community.to_owned(),
            package_count,
            synced_at: now,
        })
    }

    fn touch_sync(&self, community: &str, now: i64) -> Result<CatalogStatus, String> {
        let connection = open_connection(&self.inner.path)?;
        connection
            .execute(
                "UPDATE sync_state SET synced_at = ?2 WHERE community = ?1",
                params![community, now],
            )
            .map_err(database_error)?;
        read_status(&connection, community)?
            .ok_or_else(|| "The catalog state disappeared during the update".to_owned())
    }

    pub(crate) async fn run_blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Self) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let cache = self.clone();
        tauri::async_runtime::spawn_blocking(move || work(&cache))
            .await
            .map_err(|error| format!("Catalog task failed: {error}"))?
    }

    fn with_reader<T>(
        &self,
        work: impl FnOnce(&Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut reader = self
            .inner
            .reader
            .lock()
            .map_err(|_| "The mod catalog lock was poisoned".to_owned())?;
        if reader.is_none() {
            *reader = Some(open_connection(&self.inner.path)?);
        }
        match reader.as_ref() {
            Some(connection) => work(connection),
            None => Err("The mod catalog is unavailable".to_owned()),
        }
    }
}

struct SyncState {
    index_hash: String,
    synced_at: i64,
    package_count: u64,
}

impl SyncState {
    fn status(&self, community: &str) -> CatalogStatus {
        CatalogStatus {
            ecosystem_game_id: community.to_owned(),
            package_count: self.package_count,
            synced_at: self.synced_at,
        }
    }
}

enum Message {
    Packages(Vec<ParsedPackage>),
    Commit,
}

enum ChunkError {
    Fetch(String),
    /// The receiving side is gone; its own result says why.
    WriterStopped,
}

async fn fetch_chunks(
    http: &ThunderstoreHttp,
    urls: &[String],
    sender: &mpsc::Sender<Message>,
    on_progress: &impl Fn(u32, u32),
) -> Result<usize, ChunkError> {
    let total = u32::try_from(urls.len()).unwrap_or(u32::MAX);
    let mut skipped = 0;
    on_progress(0, total);
    for (index, url) in urls.iter().enumerate() {
        let compressed = http
            .get_bytes(url, MAX_CHUNK_BYTES)
            .await
            .map_err(ChunkError::Fetch)?;
        let (packages, chunk_skipped) =
            tauri::async_runtime::spawn_blocking(move || parse_chunk(&compressed))
                .await
                .map_err(|error| ChunkError::Fetch(format!("Catalog parse task failed: {error}")))?
                .map_err(ChunkError::Fetch)?;
        skipped += chunk_skipped;
        sender
            .send(Message::Packages(packages))
            .await
            .map_err(|_| ChunkError::WriterStopped)?;
        on_progress(u32::try_from(index + 1).unwrap_or(u32::MAX), total);
    }
    Ok(skipped)
}

fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn database_error(error: rusqlite::Error) -> String {
    format!("Mod catalog database error: {error}")
}

fn gunzip_bounded(compressed: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let mut decoded = Vec::new();
    GzDecoder::new(compressed)
        .take(limit as u64 + 1)
        .read_to_end(&mut decoded)
        .map_err(|error| format!("The Thunderstore listing is not valid gzip data: {error}"))?;
    if decoded.len() > limit {
        return Err("The Thunderstore listing is larger than allowed".to_owned());
    }
    Ok(decoded)
}

/// The package-listing-index is a gzipped JSON array of chunk URLs.
async fn parse_index(compressed: Vec<u8>) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let json = gunzip_bounded(&compressed, MAX_INDEX_JSON_BYTES)?;
        let urls: Vec<String> = serde_json::from_slice(&json)
            .map_err(|error| format!("The Thunderstore package index is invalid: {error}"))?;
        if urls.is_empty() {
            return Err("The Thunderstore package index is empty".to_owned());
        }
        if urls.len() > MAX_CHUNKS {
            return Err("The Thunderstore package index lists too many parts".to_owned());
        }
        for url in &urls {
            let allowed = reqwest::Url::parse(url)
                .is_ok_and(|url| super::thunderstore_http::is_allowed_url(&url));
            if !allowed {
                return Err("The Thunderstore package index points outside Thunderstore".to_owned());
            }
        }
        Ok(urls)
    })
    .await
    .map_err(|error| format!("Catalog parse task failed: {error}"))?
}

fn open_connection(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create the mod catalog folder: {error}"))?;
    }
    let connection = Connection::open(path)
        .map_err(|error| format!("Could not open the mod catalog: {error}"))?;
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = 5000;",
        )
        .map_err(database_error)?;
    prepare_schema(&connection)?;
    Ok(connection)
}

// The catalog can always be downloaded again, so a new layout starts empty.
fn prepare_schema(connection: &Connection) -> Result<(), String> {
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(database_error)?;
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    connection
        .execute_batch(&format!(
            "BEGIN;
             DROP TABLE IF EXISTS package_versions;
             DROP TABLE IF EXISTS package_categories;
             DROP TABLE IF EXISTS packages;
             DROP TABLE IF EXISTS sync_state;
             CREATE TABLE sync_state (
                 community TEXT PRIMARY KEY,
                 index_hash TEXT NOT NULL,
                 synced_at INTEGER NOT NULL,
                 package_count INTEGER NOT NULL
             ) WITHOUT ROWID;
             CREATE TABLE packages (
                 community TEXT NOT NULL,
                 full_name TEXT NOT NULL,
                 owner TEXT NOT NULL,
                 name TEXT NOT NULL,
                 package_url TEXT NOT NULL,
                 is_deprecated INTEGER NOT NULL,
                 is_pinned INTEGER NOT NULL,
                 has_nsfw_content INTEGER NOT NULL,
                 rating_score INTEGER NOT NULL,
                 date_created TEXT NOT NULL,
                 date_updated TEXT NOT NULL,
                 total_downloads INTEGER NOT NULL,
                 latest_version TEXT NOT NULL,
                 description TEXT NOT NULL,
                 icon TEXT NOT NULL,
                 donation_link TEXT NOT NULL,
                 search_text TEXT NOT NULL,
                 PRIMARY KEY (community, full_name)
             ) WITHOUT ROWID;
             CREATE INDEX packages_by_updated ON packages (community, date_updated);
             CREATE INDEX packages_by_created ON packages (community, date_created);
             CREATE INDEX packages_by_downloads ON packages (community, total_downloads);
             CREATE INDEX packages_by_rating ON packages (community, rating_score);
             CREATE INDEX packages_by_name ON packages (community, name COLLATE NOCASE);
             CREATE TABLE package_categories (
                 community TEXT NOT NULL,
                 full_name TEXT NOT NULL,
                 category TEXT NOT NULL,
                 PRIMARY KEY (community, category, full_name)
             ) WITHOUT ROWID;
             CREATE TABLE package_versions (
                 community TEXT NOT NULL,
                 package_full_name TEXT NOT NULL,
                 version_number TEXT NOT NULL,
                 major INTEGER NOT NULL,
                 minor INTEGER NOT NULL,
                 patch INTEGER NOT NULL,
                 description TEXT NOT NULL,
                 icon TEXT NOT NULL,
                 download_url TEXT NOT NULL,
                 file_size INTEGER NOT NULL,
                 dependencies TEXT NOT NULL,
                 downloads INTEGER NOT NULL,
                 date_created TEXT NOT NULL,
                 website_url TEXT NOT NULL,
                 PRIMARY KEY (community, package_full_name, version_number)
             ) WITHOUT ROWID;
             PRAGMA user_version = {SCHEMA_VERSION};
             COMMIT;"
        ))
        .map_err(database_error)
}

fn read_state(connection: &Connection, community: &str) -> Result<Option<SyncState>, String> {
    connection
        .query_row(
            "SELECT index_hash, synced_at, package_count FROM sync_state WHERE community = ?1",
            [community],
            |row| {
                Ok(SyncState {
                    index_hash: row.get(0)?,
                    synced_at: row.get(1)?,
                    package_count: to_unsigned(row.get(2)?),
                })
            },
        )
        .optional()
        .map_err(database_error)
}

fn read_status(connection: &Connection, community: &str) -> Result<Option<CatalogStatus>, String> {
    Ok(read_state(connection, community)?.map(|state| state.status(community)))
}

fn to_unsigned(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn to_signed(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// One package as stored: identity and versions already validated.
struct ParsedPackage {
    name: PackageName,
    package_url: String,
    categories: Vec<String>,
    is_deprecated: bool,
    is_pinned: bool,
    has_nsfw_content: bool,
    rating_score: i64,
    date_created: String,
    date_updated: String,
    donation_link: String,
    versions: Vec<ParsedVersion>,
}

struct ParsedVersion {
    version: Version,
    description: String,
    icon: String,
    download_url: String,
    file_size: u64,
    dependencies: Vec<String>,
    downloads: u64,
    date_created: String,
    website_url: String,
}

#[derive(Deserialize)]
struct RawPackage {
    full_name: String,
    owner: String,
    name: String,
    #[serde(default)]
    package_url: String,
    #[serde(default)]
    date_created: String,
    #[serde(default)]
    date_updated: String,
    #[serde(default)]
    rating_score: i64,
    #[serde(default)]
    is_pinned: bool,
    #[serde(default)]
    is_deprecated: bool,
    #[serde(default)]
    has_nsfw_content: bool,
    #[serde(default)]
    donation_link: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    versions: Vec<RawVersion>,
}

#[derive(Deserialize)]
struct RawVersion {
    version_number: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    download_url: String,
    #[serde(default)]
    file_size: u64,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    date_created: String,
    #[serde(default)]
    website_url: Option<String>,
    #[serde(default = "default_active")]
    is_active: bool,
}

fn default_active() -> bool {
    true
}

/// Author links reach the UI and the system browser, so anything that is not
/// a plain web address is dropped while the catalog is read.
fn web_address(value: Option<String>) -> String {
    value
        .and_then(|value| parse_external_url(&value))
        .map(String::from)
        .unwrap_or_default()
}

/// A chunk is a gzipped JSON array of packages. Entries that cannot be read
/// are skipped and counted rather than failing the whole sync.
fn parse_chunk(compressed: &[u8]) -> Result<(Vec<ParsedPackage>, usize), String> {
    let json = gunzip_bounded(compressed, MAX_CHUNK_JSON_BYTES)?;
    let entries: Vec<Box<RawValue>> = serde_json::from_slice(&json)
        .map_err(|error| format!("A Thunderstore package list is invalid: {error}"))?;
    let mut packages = Vec::with_capacity(entries.len());
    let mut skipped = 0;
    for entry in entries {
        match serde_json::from_str::<RawPackage>(entry.get())
            .ok()
            .and_then(parse_package)
        {
            Some(package) => packages.push(package),
            None => skipped += 1,
        }
    }
    Ok((packages, skipped))
}

fn parse_package(raw: RawPackage) -> Option<ParsedPackage> {
    let name = PackageName::new(&raw.owner, &raw.name).ok()?;
    if raw.full_name != name.to_string() {
        return None;
    }
    let versions: Vec<ParsedVersion> = raw
        .versions
        .into_iter()
        .filter(|version| version.is_active)
        .filter_map(|version| {
            Some(ParsedVersion {
                version: version.version_number.parse().ok()?,
                description: version.description.unwrap_or_default(),
                icon: version.icon.unwrap_or_default(),
                download_url: version.download_url,
                file_size: version.file_size,
                dependencies: version.dependencies,
                downloads: version.downloads,
                date_created: version.date_created,
                website_url: web_address(version.website_url),
            })
        })
        .collect();
    if versions.is_empty() {
        return None;
    }
    Some(ParsedPackage {
        name,
        package_url: raw.package_url,
        categories: raw.categories,
        is_deprecated: raw.is_deprecated,
        is_pinned: raw.is_pinned,
        has_nsfw_content: raw.has_nsfw_content,
        rating_score: raw.rating_score,
        date_created: raw.date_created,
        date_updated: raw.date_updated,
        donation_link: web_address(raw.donation_link),
        versions,
    })
}

/// Replaces the community's rows with the packages received, committing only
/// when `Message::Commit` arrives. Returns the stored package count.
fn write_catalog(
    path: &Path,
    community: &str,
    index_hash: &str,
    synced_at: i64,
    mut receiver: mpsc::Receiver<Message>,
) -> Result<u64, String> {
    let mut connection = open_connection(path)?;
    let transaction = connection.transaction().map_err(database_error)?;
    for table in ["package_versions", "package_categories", "packages"] {
        transaction
            .execute(
                &format!("DELETE FROM {table} WHERE community = ?1"),
                [community],
            )
            .map_err(database_error)?;
    }
    loop {
        match receiver.blocking_recv() {
            Some(Message::Packages(packages)) => {
                insert_packages(&transaction, community, &packages)?;
            }
            Some(Message::Commit) => break,
            None => return Err("The catalog update was interrupted".to_owned()),
        }
    }
    // A package listed in two parts is stored once.
    let count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM packages WHERE community = ?1",
            [community],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    transaction
        .execute(
            "INSERT OR REPLACE INTO sync_state (community, index_hash, synced_at, package_count)
             VALUES (?1, ?2, ?3, ?4)",
            params![community, index_hash, synced_at, count],
        )
        .map_err(database_error)?;
    transaction.commit().map_err(database_error)?;
    Ok(to_unsigned(count))
}

fn insert_packages(
    transaction: &Transaction<'_>,
    community: &str,
    packages: &[ParsedPackage],
) -> Result<(), String> {
    let mut insert_package = transaction
        .prepare_cached(
            "INSERT OR REPLACE INTO packages (
                 community, full_name, owner, name, package_url, is_deprecated, is_pinned,
                 has_nsfw_content, rating_score, date_created, date_updated, total_downloads,
                 latest_version, description, icon, donation_link, search_text
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        )
        .map_err(database_error)?;
    let mut insert_category = transaction
        .prepare_cached(
            "INSERT OR REPLACE INTO package_categories (community, full_name, category)
             VALUES (?1, ?2, ?3)",
        )
        .map_err(database_error)?;
    let mut insert_version = transaction
        .prepare_cached(
            "INSERT OR REPLACE INTO package_versions (
                 community, package_full_name, version_number, major, minor, patch, description,
                 icon, download_url, file_size, dependencies, downloads, date_created, website_url
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )
        .map_err(database_error)?;
    for package in packages {
        let full_name = package.name.to_string();
        // Versions are never empty here; the latest decides the summary fields.
        let Some(latest) = package
            .versions
            .iter()
            .max_by_key(|version| version.version)
        else {
            continue;
        };
        let total_downloads = package
            .versions
            .iter()
            .fold(0_u64, |sum, version| sum.saturating_add(version.downloads));
        let search_text = format!(
            "{} {} {}",
            package.name.name.replace('_', " "),
            package.name.owner,
            latest.description
        )
        .to_lowercase();
        insert_package
            .execute(params![
                community,
                full_name,
                package.name.owner,
                package.name.name,
                package.package_url,
                package.is_deprecated,
                package.is_pinned,
                package.has_nsfw_content,
                package.rating_score,
                package.date_created,
                package.date_updated,
                to_signed(total_downloads),
                latest.version.to_string(),
                latest.description,
                latest.icon,
                package.donation_link,
                search_text,
            ])
            .map_err(database_error)?;
        for category in &package.categories {
            insert_category
                .execute(params![community, full_name, category])
                .map_err(database_error)?;
        }
        for version in &package.versions {
            let dependencies = serde_json::to_string(&version.dependencies)
                .map_err(|error| format!("Could not store package dependencies: {error}"))?;
            insert_version
                .execute(params![
                    community,
                    full_name,
                    version.version.to_string(),
                    version.version.major,
                    version.version.minor,
                    version.version.patch,
                    version.description,
                    version.icon,
                    version.download_url,
                    to_signed(version.file_size),
                    dependencies,
                    to_signed(version.downloads),
                    version.date_created,
                    version.website_url,
                ])
                .map_err(database_error)?;
        }
    }
    Ok(())
}

const SUMMARY_COLUMNS: &str = "p.full_name, p.owner, p.name, p.package_url, p.is_deprecated,
    p.is_pinned, p.has_nsfw_content, p.rating_score, p.date_created, p.date_updated,
    p.total_downloads, p.latest_version, p.description, p.icon";

fn summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PackageSummary> {
    Ok(PackageSummary {
        full_name: row.get(0)?,
        owner: row.get(1)?,
        name: row.get(2)?,
        package_url: row.get(3)?,
        categories: Vec::new(),
        is_deprecated: row.get(4)?,
        is_pinned: row.get(5)?,
        has_nsfw_content: row.get(6)?,
        rating_score: row.get(7)?,
        date_created: row.get(8)?,
        date_updated: row.get(9)?,
        total_downloads: to_unsigned(row.get(10)?),
        latest_version: row.get(11)?,
        description: row.get(12)?,
        icon: row.get(13)?,
    })
}

fn package_categories(
    connection: &Connection,
    community: &str,
    full_name: &str,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare_cached(
            "SELECT category FROM package_categories
             WHERE community = ?1 AND full_name = ?2 ORDER BY category COLLATE NOCASE, category",
        )
        .map_err(database_error)?;
    statement
        .query_map(params![community, full_name], |row| row.get(0))
        .map_err(database_error)?
        .collect::<Result<_, _>>()
        .map_err(database_error)
}

fn escape_like(term: &str) -> String {
    let mut escaped = String::with_capacity(term.len() + 2);
    for character in term.chars() {
        if matches!(character, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn distinct_categories(categories: &[String]) -> Vec<String> {
    let mut distinct: Vec<String> = Vec::new();
    for category in categories {
        if distinct.len() == MAX_QUERY_CATEGORIES {
            break;
        }
        if !distinct.contains(category) {
            distinct.push(category.clone());
        }
    }
    distinct
}

/// `quantifier` is `EXISTS` or `NOT EXISTS`: whether the package has any of `categories`.
fn push_category_condition(
    conditions: &mut String,
    values: &mut Vec<Value>,
    quantifier: &str,
    categories: &[String],
) {
    let placeholders = vec!["?"; categories.len()].join(", ");
    conditions.push_str(&format!(
        " AND {quantifier} (SELECT 1 FROM package_categories c WHERE c.community = p.community
          AND c.full_name = p.full_name AND c.category IN ({placeholders}))"
    ));
    values.extend(categories.iter().cloned().map(Value::Text));
}

fn search(
    connection: &Connection,
    community: &str,
    query: &PackageQuery,
) -> Result<PackagePage, String> {
    let mut conditions = String::from("p.community = ?");
    let mut values = vec![Value::Text(community.to_owned())];
    for term in query
        .text
        .to_lowercase()
        .split_whitespace()
        .take(MAX_QUERY_TERMS)
    {
        conditions.push_str(" AND p.search_text LIKE ? ESCAPE '\\'");
        values.push(Value::Text(format!("%{}%", escape_like(term))));
    }
    if !query.include_deprecated {
        conditions.push_str(" AND p.is_deprecated = 0");
    }
    if !query.include_nsfw {
        conditions.push_str(" AND p.has_nsfw_content = 0");
    }
    let included = distinct_categories(&query.categories);
    let excluded = distinct_categories(&query.excluded_categories);
    // `All` needs one membership test per category; `Any` a single test over the set.
    let membership_groups: Vec<&[String]> = if included.is_empty() {
        Vec::new()
    } else if query.category_mode == CategoryMode::All {
        included.chunks(1).collect()
    } else {
        vec![included.as_slice()]
    };
    for group in membership_groups {
        push_category_condition(&mut conditions, &mut values, "EXISTS", group);
    }
    if !excluded.is_empty() {
        push_category_condition(&mut conditions, &mut values, "NOT EXISTS", &excluded);
    }

    let total: i64 = connection
        .query_row(
            &format!("SELECT COUNT(*) FROM packages p WHERE {conditions}"),
            params_from_iter(values.iter()),
            |row| row.get(0),
        )
        .map_err(database_error)?;

    // Both keys share one direction so the sort indexes can serve the scan.
    let order = match query.sort {
        PackageSort::LastUpdated => "p.date_updated DESC, p.full_name DESC",
        PackageSort::MostDownloads => "p.total_downloads DESC, p.full_name DESC",
        PackageSort::TopRated => "p.rating_score DESC, p.full_name DESC",
        PackageSort::Newest => "p.date_created DESC, p.full_name DESC",
        PackageSort::Alphabetical => "p.name COLLATE NOCASE ASC, p.full_name ASC",
    };
    let limit = query.limit.clamp(1, MAX_PAGE_SIZE);
    let mut page_values = values;
    page_values.push(Value::Integer(i64::from(limit)));
    page_values.push(Value::Integer(i64::from(query.offset)));
    let mut statement = connection
        .prepare(&format!(
            "SELECT {SUMMARY_COLUMNS} FROM packages p WHERE {conditions}
             ORDER BY {order} LIMIT ? OFFSET ?"
        ))
        .map_err(database_error)?;
    let mut items: Vec<PackageSummary> = statement
        .query_map(params_from_iter(page_values.iter()), summary_from_row)
        .map_err(database_error)?
        .collect::<Result<_, _>>()
        .map_err(database_error)?;
    for item in &mut items {
        item.categories = package_categories(connection, community, &item.full_name)?;
    }
    Ok(PackagePage {
        total: to_unsigned(total),
        items,
    })
}

fn package_details(
    connection: &Connection,
    community: &str,
    name: &PackageName,
) -> Result<Option<PackageDetails>, String> {
    let full_name = name.to_string();
    let summary = connection
        .query_row(
            &format!("SELECT {SUMMARY_COLUMNS} FROM packages p WHERE p.community = ?1 AND p.full_name = ?2"),
            params![community, full_name],
            summary_from_row,
        )
        .optional()
        .map_err(database_error)?;
    let Some(mut summary) = summary else {
        return Ok(None);
    };
    summary.categories = package_categories(connection, community, &full_name)?;
    let donation_link: String = connection
        .query_row(
            "SELECT donation_link FROM packages WHERE community = ?1 AND full_name = ?2",
            params![community, full_name],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    let mut statement = connection
        .prepare(
            "SELECT version_number, description, icon, download_url, file_size, dependencies,
                    downloads, date_created, website_url
             FROM package_versions WHERE community = ?1 AND package_full_name = ?2
             ORDER BY major DESC, minor DESC, patch DESC",
        )
        .map_err(database_error)?;
    let versions = statement
        .query_map(params![community, full_name], |row| {
            let version_number: String = row.get(0)?;
            let dependencies: String = row.get(5)?;
            Ok(PackageVersionDetails {
                full_name: format!("{full_name}-{version_number}"),
                version_number,
                description: row.get(1)?,
                icon: row.get(2)?,
                download_url: row.get(3)?,
                file_size: to_unsigned(row.get(4)?),
                dependencies: serde_json::from_str(&dependencies).unwrap_or_default(),
                downloads: to_unsigned(row.get(6)?),
                date_created: row.get(7)?,
                website_url: non_empty(row.get(8)?),
            })
        })
        .map_err(database_error)?
        .collect::<Result<_, _>>()
        .map_err(database_error)?;
    Ok(Some(PackageDetails {
        summary,
        donation_link: non_empty(donation_link),
        versions,
    }))
}

/// A specific version, or the highest one when `version` is `None`.
fn version_record(
    connection: &Connection,
    community: &str,
    name: &PackageName,
    version: Option<&Version>,
) -> Result<Option<PackageVersionRecord>, String> {
    let (filter, mut values) = match version {
        Some(version) => (
            "AND version_number = ?3",
            vec![Value::Text(version.to_string())],
        ),
        None => ("", Vec::new()),
    };
    values.insert(0, Value::Text(name.to_string()));
    values.insert(0, Value::Text(community.to_owned()));
    let row = connection
        .query_row(
            &format!(
                "SELECT version_number, description, icon, download_url, file_size, dependencies
                 FROM package_versions WHERE community = ?1 AND package_full_name = ?2 {filter}
                 ORDER BY major DESC, minor DESC, patch DESC LIMIT 1"
            ),
            params_from_iter(values.iter()),
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()
        .map_err(database_error)?;
    let Some((version_number, description, icon, download_url, file_size, dependencies)) = row
    else {
        return Ok(None);
    };
    let version: Version = version_number.parse()?;
    let dependencies: Vec<String> = serde_json::from_str(&dependencies)
        .map_err(|error| format!("Stored dependencies are invalid: {error}"))?;
    let dependencies = dependencies
        .iter()
        .filter_map(|dependency| match dependency.parse() {
            Ok(id) => Some(id),
            Err(error) => {
                crate::application_log::state_change(
                    "mod_dependency_skipped",
                    &format!("{name}-{version}: {error}"),
                );
                None
            }
        })
        .collect();
    Ok(Some(PackageVersionRecord {
        id: PackageVersionId::new(name.clone(), version),
        download_url,
        file_size: to_unsigned(file_size),
        dependencies,
        description,
        icon,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        diagnostics::Diagnostics,
        modding::test_support::{Reply, TempDir, TestServer},
    };
    use flate2::{Compression, write::GzEncoder};
    use serde_json::{Value as Json, json};
    use std::io::Write;

    const COMMUNITY: &str = "lethal-company";

    fn gzip(value: &Json) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(value.to_string().as_bytes()).unwrap();
        encoder.finish().unwrap()
    }

    fn version(package: &str, number: &str, downloads: u64, dependencies: &[&str]) -> Json {
        json!({
            "name": package,
            "full_name": format!("x-{package}-{number}"),
            "description": format!("{package} {number} description"),
            "icon": format!("https://gcdn.thunderstore.io/{package}-{number}.png"),
            "version_number": number,
            "dependencies": dependencies,
            "download_url": format!("https://thunderstore.io/package/download/Owner/{package}/{number}/"),
            "downloads": downloads,
            "date_created": "2025-01-01T00:00:00.000000+00:00",
            "website_url": "",
            "is_active": true,
            "uuid4": "00000000-0000-4000-8000-000000000000",
            "file_size": 1000
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn package(
        owner: &str,
        name: &str,
        updated: &str,
        rating: i64,
        categories: &[&str],
        flags: (bool, bool),
        versions: Vec<Json>,
    ) -> Json {
        json!({
            "name": name,
            "full_name": format!("{owner}-{name}"),
            "owner": owner,
            "package_url": format!("https://thunderstore.io/c/lethal-company/p/{owner}/{name}/"),
            "donation_link": null,
            "date_created": format!("2024-{}", &updated[5..]),
            "date_updated": updated,
            "uuid4": "00000000-0000-4000-8000-000000000001",
            "rating_score": rating,
            "is_pinned": false,
            "is_deprecated": flags.0,
            "has_nsfw_content": flags.1,
            "categories": categories,
            "versions": versions
        })
    }

    fn sample_chunks() -> (Json, Json) {
        let first = json!([
            package(
                "Alice",
                "Big_Mod",
                "2025-03-01T00:00:00+00:00",
                10,
                &["Mods", "Tools"],
                (false, false),
                vec![
                    version(
                        "Big_Mod",
                        "1.2.0",
                        300,
                        &["BepInEx-BepInExPack-5.4.2100", "bad dependency"]
                    ),
                    version("Big_Mod", "1.10.0", 200, &[]),
                    version("Big_Mod", "1.9.0", 100, &[]),
                ]
            ),
            package(
                "Bob",
                "Small_Mod",
                "2025-05-01T00:00:00+00:00",
                50,
                &["Mods"],
                (false, false),
                vec![version("Small_Mod", "0.1.0", 5000, &[]),]
            ),
            package(
                "Carol",
                "Old_Mod",
                "2024-01-01T00:00:00+00:00",
                5,
                &["Mods"],
                (true, false),
                vec![version("Old_Mod", "1.0.0", 1, &[]),]
            ),
        ]);
        let second = json!([
            package(
                "Dave",
                "Adult_Mod",
                "2025-04-01T00:00:00+00:00",
                30,
                &["Cosmetics"],
                (false, true),
                vec![version("Adult_Mod", "2.0.0", 700, &[]),]
            ),
            package(
                "Erin",
                "zebra_tool",
                "2025-02-01T00:00:00+00:00",
                20,
                &["Tools"],
                (false, false),
                vec![version("zebra_tool", "3.0.1", 10, &[]),]
            ),
        ]);
        (first, second)
    }

    struct Fixture {
        _directory: TempDir,
        server: TestServer,
        cache: PackageCache,
        http: ThunderstoreHttp,
        index_url: String,
        chunks: [String; 2],
    }

    impl Fixture {
        fn new() -> Self {
            let directory = TempDir::new();
            let server = TestServer::start();
            let chunks = [server.url("/chunk1"), server.url("/chunk2")];
            let (first, second) = sample_chunks();
            server.set("/chunk1", Reply::ok(gzip(&first)));
            server.set("/chunk2", Reply::ok(gzip(&second)));
            server.set("/index", Reply::ok(gzip(&json!(chunks))));
            let cache =
                PackageCache::new(directory.path().join("modding").join("packages.sqlite3"));
            let http = ThunderstoreHttp::new(
                "0.1.0",
                Diagnostics::new(Err("test logs disabled".to_owned())),
            )
            .unwrap();
            let index_url = server.url("/index");
            Self {
                _directory: directory,
                server,
                cache,
                http,
                index_url,
                chunks,
            }
        }

        fn sync(&self, force: bool) -> Result<CatalogStatus, String> {
            tauri::async_runtime::block_on(self.cache.sync(
                &self.http,
                COMMUNITY,
                &self.index_url,
                force,
                |_, _| {},
            ))
        }

        fn synced() -> Self {
            let fixture = Self::new();
            fixture.sync(false).unwrap();
            fixture
        }

        fn names(&self, query: &PackageQuery) -> Vec<String> {
            self.cache
                .search(COMMUNITY, query)
                .unwrap()
                .items
                .into_iter()
                .map(|item| item.full_name)
                .collect()
        }

        fn set_synced_at(&self, synced_at: i64) {
            let connection = open_connection(&self.cache.inner.path).unwrap();
            connection
                .execute("UPDATE sync_state SET synced_at = ?1", [synced_at])
                .unwrap();
        }
    }

    fn query() -> PackageQuery {
        PackageQuery::default()
    }

    #[test]
    fn syncs_a_community_and_reports_progress() {
        let fixture = Fixture::new();
        let progress = Mutex::new(Vec::new());
        let status = tauri::async_runtime::block_on(fixture.cache.sync(
            &fixture.http,
            COMMUNITY,
            &fixture.index_url,
            false,
            |done, total| progress.lock().unwrap().push((done, total)),
        ))
        .unwrap();
        assert_eq!(status.package_count, 5);
        assert_eq!(status.ecosystem_game_id, COMMUNITY);
        assert_eq!(*progress.lock().unwrap(), [(0, 2), (1, 2), (2, 2)]);
        assert_eq!(fixture.cache.status(COMMUNITY).unwrap(), Some(status));
        assert_eq!(fixture.cache.status("other").unwrap(), None);
    }

    #[test]
    fn skips_chunks_when_the_index_is_unchanged() {
        let fixture = Fixture::synced();
        fixture.server.clear_requests();
        fixture.set_synced_at(0);
        let status = fixture.sync(false).unwrap();
        assert_eq!(status.package_count, 5);
        assert!(status.synced_at > 0, "the check time is refreshed");
        assert_eq!(fixture.server.requests(), ["/index"]);
    }

    #[test]
    fn does_nothing_while_fresh_unless_forced() {
        let fixture = Fixture::synced();
        fixture.server.clear_requests();
        fixture.sync(false).unwrap();
        assert!(fixture.server.requests().is_empty());
        fixture.sync(true).unwrap();
        assert_eq!(fixture.server.requests(), ["/index"]);
    }

    #[test]
    fn resyncs_and_prunes_when_the_index_changes() {
        let fixture = Fixture::synced();
        assert!(
            fixture
                .cache
                .package(COMMUNITY, &"Bob-Small_Mod".parse().unwrap())
                .unwrap()
                .is_some()
        );
        let replacement = json!([package(
            "Alice",
            "Big_Mod",
            "2025-06-01T00:00:00+00:00",
            11,
            &["Mods"],
            (false, false),
            vec![version("Big_Mod", "2.0.0", 1, &[]),]
        )]);
        fixture.server.set("/chunk1", Reply::ok(gzip(&replacement)));
        fixture
            .server
            .set("/index", Reply::ok(gzip(&json!([fixture.chunks[0]]))));
        let status = fixture.sync(true).unwrap();
        assert_eq!(status.package_count, 1);
        assert_eq!(fixture.names(&query()), ["Alice-Big_Mod"]);
        assert!(
            fixture
                .cache
                .package(COMMUNITY, &"Bob-Small_Mod".parse().unwrap())
                .unwrap()
                .is_none()
        );
        assert_eq!(fixture.cache.categories(COMMUNITY).unwrap(), ["Mods"]);
        let big = fixture
            .cache
            .package(COMMUNITY, &"Alice-Big_Mod".parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(big.versions.len(), 1);
    }

    #[test]
    fn a_failed_chunk_commits_nothing() {
        let fixture = Fixture::synced();
        let before = fixture.names(&query());
        let replacement = json!([package(
            "Zed",
            "New_Mod",
            "2025-06-01T00:00:00+00:00",
            1,
            &["Mods"],
            (false, false),
            vec![version("New_Mod", "1.0.0", 1, &[]),]
        )]);
        // Chunk URLs differ from the synced ones; the first part would already be written.
        let urls = [fixture.server.url("/chunk3"), fixture.server.url("/chunk4")];
        fixture.server.set("/chunk3", Reply::ok(gzip(&replacement)));
        fixture.server.set("/chunk4", Reply::status(500));
        fixture.server.set("/index", Reply::ok(gzip(&json!(urls))));
        let error = fixture.sync(true).unwrap_err();
        assert!(error.contains("HTTP 500"), "{error}");
        assert_eq!(fixture.names(&query()), before);
        let zed = "Zed-New_Mod".parse().unwrap();
        assert!(fixture.cache.package(COMMUNITY, &zed).unwrap().is_none());
        assert_eq!(
            fixture
                .cache
                .status(COMMUNITY)
                .unwrap()
                .unwrap()
                .package_count,
            5
        );

        // The failed attempt did not record the new index hash.
        fixture.server.set("/chunk4", Reply::ok(gzip(&json!([]))));
        fixture.server.clear_requests();
        assert_eq!(fixture.sync(true).unwrap().package_count, 1);
        assert!(fixture.server.requests().contains(&"/chunk3".to_owned()));
        assert!(fixture.cache.package(COMMUNITY, &zed).unwrap().is_some());
    }

    #[test]
    fn rejects_bad_indexes_and_chunk_urls() {
        let fixture = Fixture::new();
        let cases = [
            (Reply::ok(b"not gzip".to_vec()), "gzip"),
            (Reply::ok(gzip(&json!({"content": []}))), "invalid"),
            (Reply::ok(gzip(&json!([]))), "empty"),
            (
                Reply::ok(gzip(&json!(["https://evil.example.com/chunk"]))),
                "outside Thunderstore",
            ),
        ];
        for (reply, expected) in cases {
            fixture.server.set("/index", reply);
            let error = fixture.sync(true).unwrap_err();
            assert!(error.contains(expected), "{expected}: {error}");
        }
        assert_eq!(fixture.cache.status(COMMUNITY).unwrap(), None);
    }

    #[test]
    fn caps_decompressed_size() {
        let big = json!(["x".repeat(2048)]);
        let compressed = gzip(&big);
        assert!(compressed.len() < 512, "highly compressible");
        let error = gunzip_bounded(&compressed, 1024).unwrap_err();
        assert!(error.contains("larger than allowed"), "{error}");
        assert!(gunzip_bounded(&compressed, 4096).is_ok());
    }

    #[test]
    fn skips_unreadable_packages_but_keeps_the_rest() {
        let fixture = Fixture::new();
        let chunk = json!([
            package("Good", "Mod", "2025-01-01T00:00:00+00:00", 1, &[], (false, false), vec![version("Mod", "1.0.0", 1, &[])]),
            {"full_name": 5},
            package("No", "Versions", "2025-01-01T00:00:00+00:00", 1, &[], (false, false), vec![]),
            package("Bad", "Version", "2025-01-01T00:00:00+00:00", 1, &[], (false, false), vec![version("Version", "latest", 1, &[])]),
            {"name": "Mod", "full_name": "Other-Mod", "owner": "Good", "versions": []},
        ]);
        fixture.server.set("/chunk1", Reply::ok(gzip(&chunk)));
        fixture
            .server
            .set("/index", Reply::ok(gzip(&json!([fixture.chunks[0]]))));
        assert_eq!(fixture.sync(true).unwrap().package_count, 1);
        assert_eq!(fixture.names(&query()), ["Good-Mod"]);
    }

    #[test]
    fn hides_deprecated_and_nsfw_by_default() {
        let fixture = Fixture::synced();
        let all = fixture.names(&PackageQuery {
            include_deprecated: true,
            include_nsfw: true,
            ..query()
        });
        assert_eq!(all.len(), 5);
        let default = fixture.names(&query());
        assert!(!default.contains(&"Carol-Old_Mod".to_owned()));
        assert!(!default.contains(&"Dave-Adult_Mod".to_owned()));
        assert_eq!(default.len(), 3);
        assert_eq!(fixture.cache.search(COMMUNITY, &query()).unwrap().total, 3);
        let only_deprecated = fixture.names(&PackageQuery {
            include_deprecated: true,
            ..query()
        });
        assert_eq!(only_deprecated.len(), 4);
        assert!(only_deprecated.contains(&"Carol-Old_Mod".to_owned()));
        let with_nsfw = fixture.names(&PackageQuery {
            include_nsfw: true,
            ..query()
        });
        assert!(with_nsfw.contains(&"Dave-Adult_Mod".to_owned()));
    }

    #[test]
    fn searches_name_owner_and_description() {
        let fixture = Fixture::synced();
        let find = |text: &str| {
            fixture.names(&PackageQuery {
                text: text.to_owned(),
                ..query()
            })
        };
        assert_eq!(find("big mod"), ["Alice-Big_Mod"]);
        assert_eq!(find("BIG"), ["Alice-Big_Mod"]);
        assert_eq!(find("bob"), ["Bob-Small_Mod"]);
        assert_eq!(find("alice big"), ["Alice-Big_Mod"]);
        assert_eq!(find("zebra"), ["Erin-zebra_tool"]);
        assert_eq!(find("1.10.0"), ["Alice-Big_Mod"], "latest description");
        assert_eq!(find("  small   mod "), ["Bob-Small_Mod"]);
        assert!(find("big zebra").is_empty(), "every term must match");
        assert!(find("nothing like this").is_empty());
        assert!(find("%").is_empty(), "wildcards are literal");
    }

    #[test]
    fn filters_by_category() {
        let fixture = Fixture::synced();
        let filtered = |categories: &[&str]| {
            fixture.names(&PackageQuery {
                categories: categories.iter().map(|c| (*c).to_owned()).collect(),
                sort: PackageSort::Alphabetical,
                ..query()
            })
        };
        assert_eq!(filtered(&["Tools"]), ["Alice-Big_Mod", "Erin-zebra_tool"]);
        assert_eq!(filtered(&["Mods"]), ["Alice-Big_Mod", "Bob-Small_Mod"]);
        assert_eq!(filtered(&["Tools", "Mods"]).len(), 3);
        assert!(filtered(&["Cosmetics"]).is_empty(), "NSFW stays hidden");
        assert!(filtered(&["Unknown"]).is_empty());
        let big = fixture
            .cache
            .search(
                COMMUNITY,
                &PackageQuery {
                    text: "big".into(),
                    ..query()
                },
            )
            .unwrap();
        assert_eq!(big.items[0].categories, ["Mods", "Tools"]);
        assert_eq!(
            fixture.cache.categories(COMMUNITY).unwrap(),
            ["Cosmetics", "Mods", "Tools"]
        );
    }

    #[test]
    fn matches_all_categories_or_any() {
        let fixture = Fixture::synced();
        let names = |mode, categories: &[&str]| {
            fixture.names(&PackageQuery {
                categories: categories.iter().map(|c| (*c).to_owned()).collect(),
                category_mode: mode,
                sort: PackageSort::Alphabetical,
                ..query()
            })
        };
        assert_eq!(
            names(CategoryMode::All, &["Mods", "Tools"]),
            ["Alice-Big_Mod"]
        );
        assert_eq!(names(CategoryMode::All, &["Mods", "Mods"]).len(), 2);
        assert_eq!(names(CategoryMode::All, &["Tools"]).len(), 2);
        assert!(names(CategoryMode::All, &["Mods", "Unknown"]).is_empty());
        assert!(names(CategoryMode::All, &["Mods", "Cosmetics"]).is_empty());
        assert_eq!(names(CategoryMode::Any, &["Mods", "Tools"]).len(), 3);
        assert_eq!(names(CategoryMode::All, &[]).len(), 3, "no categories");
    }

    #[test]
    fn excludes_categories() {
        let fixture = Fixture::synced();
        let names = |categories: &[&str], excluded: &[&str]| {
            fixture.names(&PackageQuery {
                categories: categories.iter().map(|c| (*c).to_owned()).collect(),
                excluded_categories: excluded.iter().map(|c| (*c).to_owned()).collect(),
                sort: PackageSort::Alphabetical,
                ..query()
            })
        };
        assert_eq!(names(&[], &["Tools"]), ["Bob-Small_Mod"]);
        assert_eq!(names(&[], &["Tools", "Mods"]), Vec::<String>::new());
        assert_eq!(names(&["Mods"], &["Tools"]), ["Bob-Small_Mod"]);
        assert!(names(&["Tools"], &["Tools"]).is_empty(), "exclusion wins");
        assert_eq!(names(&[], &["Unknown"]).len(), 3);
        let with_nsfw = fixture.names(&PackageQuery {
            excluded_categories: vec!["Cosmetics".to_owned()],
            include_nsfw: true,
            include_deprecated: true,
            ..query()
        });
        assert_eq!(with_nsfw.len(), 4);
        assert!(!with_nsfw.contains(&"Dave-Adult_Mod".to_owned()));
    }

    #[test]
    fn reads_category_filters_from_the_frontend() {
        let parsed: PackageQuery = serde_json::from_value(json!({
            "categories": ["A", "B"], "categoryMode": "all", "excludedCategories": ["C"]
        }))
        .unwrap();
        assert_eq!(parsed.category_mode, CategoryMode::All);
        assert_eq!(parsed.excluded_categories, ["C"]);
        let defaults: PackageQuery = serde_json::from_value(json!({})).unwrap();
        assert_eq!(defaults.category_mode, CategoryMode::Any);
        assert!(defaults.excluded_categories.is_empty());
    }

    #[test]
    fn stores_donation_and_website_links() {
        let fixture = Fixture::new();
        let mut with_links = package(
            "Alice",
            "Linked",
            "2025-01-01T00:00:00+00:00",
            1,
            &[],
            (false, false),
            vec![
                version("Linked", "2.0.0", 1, &[]),
                version("Linked", "1.0.0", 1, &[]),
                version("Linked", "0.5.0", 1, &[]),
            ],
        );
        with_links["donation_link"] = json!("https://ko-fi.com/alice");
        with_links["versions"][0]["website_url"] = json!("https://example.com/linked");
        with_links["versions"][1]["website_url"] = json!("javascript:alert(1)");
        with_links["versions"][2]["website_url"] = json!(null);
        let mut bad_donation = package(
            "Bob",
            "Plain",
            "2025-01-01T00:00:00+00:00",
            1,
            &[],
            (false, false),
            vec![version("Plain", "1.0.0", 1, &[])],
        );
        bad_donation["donation_link"] = json!("file:///C:/Windows");
        fixture.server.set(
            "/chunk1",
            Reply::ok(gzip(&json!([with_links, bad_donation]))),
        );
        fixture
            .server
            .set("/index", Reply::ok(gzip(&json!([fixture.chunks[0]]))));
        fixture.sync(true).unwrap();

        let linked = fixture
            .cache
            .package(COMMUNITY, &"Alice-Linked".parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            linked.donation_link.as_deref(),
            Some("https://ko-fi.com/alice")
        );
        let websites: Vec<_> = linked
            .versions
            .iter()
            .map(|version| version.website_url.as_deref())
            .collect();
        assert_eq!(websites, [Some("https://example.com/linked"), None, None]);
        let plain = fixture
            .cache
            .package(COMMUNITY, &"Bob-Plain".parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(plain.donation_link, None);
        assert_eq!(plain.versions[0].website_url, None);
        let value = serde_json::to_value(&linked).unwrap();
        assert_eq!(value["donationLink"], "https://ko-fi.com/alice");
        assert_eq!(
            value["versions"][0]["websiteUrl"],
            "https://example.com/linked"
        );
    }

    #[test]
    fn sorts_every_way() {
        let fixture = Fixture::synced();
        let sorted = |sort| fixture.names(&PackageQuery { sort, ..query() });
        assert_eq!(
            sorted(PackageSort::LastUpdated),
            ["Bob-Small_Mod", "Alice-Big_Mod", "Erin-zebra_tool"]
        );
        assert_eq!(
            sorted(PackageSort::MostDownloads),
            ["Bob-Small_Mod", "Alice-Big_Mod", "Erin-zebra_tool"]
        );
        assert_eq!(
            sorted(PackageSort::TopRated),
            ["Bob-Small_Mod", "Erin-zebra_tool", "Alice-Big_Mod"]
        );
        assert_eq!(
            sorted(PackageSort::Newest),
            ["Bob-Small_Mod", "Alice-Big_Mod", "Erin-zebra_tool"]
        );
        assert_eq!(
            sorted(PackageSort::Alphabetical),
            ["Alice-Big_Mod", "Bob-Small_Mod", "Erin-zebra_tool"]
        );
    }

    #[test]
    fn paginates_with_totals() {
        let fixture = Fixture::synced();
        let page = |offset, limit| {
            fixture
                .cache
                .search(
                    COMMUNITY,
                    &PackageQuery {
                        offset,
                        limit,
                        sort: PackageSort::Alphabetical,
                        ..query()
                    },
                )
                .unwrap()
        };
        let first = page(0, 2);
        assert_eq!(first.total, 3);
        assert_eq!(first.items.len(), 2);
        let second = page(2, 2);
        assert_eq!(second.total, 3);
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.items[0].full_name, "Erin-zebra_tool");
        assert!(page(10, 2).items.is_empty());
        assert_eq!(page(0, 0).items.len(), 1, "limit is at least one");
        assert_eq!(page(0, 100_000).items.len(), 3, "limit is capped");
    }

    #[test]
    fn summarizes_packages() {
        let fixture = Fixture::synced();
        let page = fixture
            .cache
            .search(
                COMMUNITY,
                &PackageQuery {
                    text: "big".into(),
                    ..query()
                },
            )
            .unwrap();
        let summary = &page.items[0];
        assert_eq!(summary.owner, "Alice");
        assert_eq!(summary.name, "Big_Mod");
        assert_eq!(summary.total_downloads, 600);
        assert_eq!(summary.latest_version, "1.10.0");
        assert_eq!(summary.description, "Big_Mod 1.10.0 description");
        assert_eq!(summary.rating_score, 10);
        assert_eq!(
            summary.package_url,
            "https://thunderstore.io/c/lethal-company/p/Alice/Big_Mod/"
        );
        assert!(!summary.is_deprecated && !summary.is_pinned && !summary.has_nsfw_content);
    }

    #[test]
    fn returns_package_details_with_versions_newest_first() {
        let fixture = Fixture::synced();
        let details = fixture
            .cache
            .package(COMMUNITY, &"Alice-Big_Mod".parse().unwrap())
            .unwrap()
            .unwrap();
        let numbers: Vec<_> = details
            .versions
            .iter()
            .map(|v| v.version_number.as_str())
            .collect();
        assert_eq!(numbers, ["1.10.0", "1.9.0", "1.2.0"]);
        assert_eq!(details.versions[2].full_name, "Alice-Big_Mod-1.2.0");
        assert_eq!(
            details.versions[2].dependencies,
            ["BepInEx-BepInExPack-5.4.2100", "bad dependency"]
        );
        assert_eq!(details.versions[2].downloads, 300);
        assert_eq!(details.versions[0].file_size, 1000);
        assert_eq!(details.summary.categories, ["Mods", "Tools"]);
        assert!(
            fixture
                .cache
                .package(COMMUNITY, &"Nobody-Nothing".parse().unwrap())
                .unwrap()
                .is_none()
        );
        assert!(
            fixture
                .cache
                .package("other", &"Alice-Big_Mod".parse().unwrap())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn resolves_versions_and_skips_unparsable_dependencies() {
        let fixture = Fixture::synced();
        let id: PackageVersionId = "Alice-Big_Mod-1.2.0".parse().unwrap();
        let record = fixture.cache.version(COMMUNITY, &id).unwrap().unwrap();
        assert_eq!(record.id, id);
        assert_eq!(record.file_size, 1000);
        assert_eq!(
            record.download_url,
            "https://thunderstore.io/package/download/Owner/Big_Mod/1.2.0/"
        );
        assert_eq!(record.description, "Big_Mod 1.2.0 description");
        assert_eq!(
            record.icon,
            "https://gcdn.thunderstore.io/Big_Mod-1.2.0.png"
        );
        assert_eq!(
            record.dependencies,
            ["BepInEx-BepInExPack-5.4.2100"
                .parse::<PackageVersionId>()
                .unwrap()]
        );
        assert!(
            fixture
                .cache
                .version(COMMUNITY, &"Alice-Big_Mod-9.9.9".parse().unwrap())
                .unwrap()
                .is_none()
        );

        let latest = fixture
            .cache
            .latest_version(COMMUNITY, &"Alice-Big_Mod".parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(latest.id.to_string(), "Alice-Big_Mod-1.10.0");
        assert!(latest.dependencies.is_empty());
        assert!(
            fixture
                .cache
                .latest_version(COMMUNITY, &"Nobody-Nothing".parse().unwrap())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn keeps_communities_separate() {
        let fixture = Fixture::synced();
        fixture
            .server
            .set("/index2", Reply::ok(gzip(&json!([fixture.chunks[1]]))));
        let other = tauri::async_runtime::block_on(fixture.cache.sync(
            &fixture.http,
            "riskofrain2",
            &fixture.server.url("/index2"),
            false,
            |_, _| {},
        ))
        .unwrap();
        assert_eq!(other.package_count, 2);
        assert_eq!(fixture.names(&query()).len(), 3);
        let other_names: Vec<_> = fixture
            .cache
            .search("riskofrain2", &query())
            .unwrap()
            .items
            .into_iter()
            .map(|item| item.full_name)
            .collect();
        assert_eq!(other_names, ["Erin-zebra_tool"]);
    }

    #[test]
    fn rebuilds_the_database_when_the_layout_version_differs() {
        let fixture = Fixture::synced();
        let connection = open_connection(&fixture.cache.inner.path).unwrap();
        connection.execute_batch("PRAGMA user_version = 0").unwrap();
        drop(connection);
        let fresh = PackageCache::new(fixture.cache.inner.path.clone());
        assert_eq!(fresh.status(COMMUNITY).unwrap(), None);
    }

    #[test]
    fn serializes_queries_and_pages_for_the_frontend() {
        let parsed: PackageQuery = serde_json::from_value(json!({
            "text": "x", "sort": "mostDownloads", "includeNsfw": true, "limit": 10
        }))
        .unwrap();
        assert_eq!(parsed.sort, PackageSort::MostDownloads);
        assert!(parsed.include_nsfw && !parsed.include_deprecated);
        assert_eq!(parsed.limit, 10);
        let defaults: PackageQuery = serde_json::from_value(json!({})).unwrap();
        assert_eq!(defaults, query());

        let fixture = Fixture::synced();
        let page = fixture.cache.search(COMMUNITY, &query()).unwrap();
        let value = serde_json::to_value(&page).unwrap();
        assert_eq!(value["total"], 3);
        assert!(value["items"][0]["fullName"].is_string());
        assert!(value["items"][0]["totalDownloads"].is_number());
        let details = fixture
            .cache
            .package(COMMUNITY, &"Alice-Big_Mod".parse().unwrap())
            .unwrap()
            .unwrap();
        let value = serde_json::to_value(&details).unwrap();
        assert_eq!(value["fullName"], "Alice-Big_Mod");
        assert_eq!(value["versions"][0]["versionNumber"], "1.10.0");
    }
}
