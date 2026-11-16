//! Cached on-disk size of installed games and per-library usage.
//!
//! Sizes are measured lazily, in the background, only when a view asks. Results
//! live in a small JSON file in the app cache folder, keyed by game ID, so they
//! survive restarts without touching the database schema.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::{
    database::{Database, DatabaseState},
    folder_size::{self, Measurement},
    game_library, installed_folder,
};

const CACHE_FILE: &str = "disk-usage.json";
const CACHE_VERSION: u32 = 1;
// Installs change in ways a folder timestamp does not always show, such as a
// file replaced deep in the tree, so an entry also expires.
const MAX_AGE_SECONDS: u64 = 7 * 24 * 60 * 60;
// Pause between games, so a full pass does not hammer the disk back to back.
const GAME_PAUSE: Duration = Duration::from_millis(50);
const CHANGED_EVENT: &str = "legio:disk-usage-changed";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    path: String,
    bytes: u64,
    partial: bool,
    /// Folder modification time in epoch seconds, the cheap invalidation signal.
    modified: Option<u64>,
    measured_at: u64,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    games: BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GameSize {
    game_id: String,
    bytes: u64,
    partial: bool,
    measured_at: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiskUsageSnapshot {
    games: Vec<GameSize>,
    computing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibraryUsage {
    library_id: String,
    total_bytes: Option<u64>,
    free_bytes: Option<u64>,
    /// Sum of the measured games. Games not measured yet are left out.
    legio_bytes: u64,
    /// Used space that does not belong to a measured game.
    other_bytes: Option<u64>,
    /// Some game is still unmeasured or only partly readable.
    estimated: bool,
}

pub(crate) struct DiskUsageState {
    cache_file: Option<PathBuf>,
    entries: Mutex<BTreeMap<String, Entry>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
    // Bumped by every invalidation so a walk that started earlier cannot store its result.
    generation: AtomicU64,
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn modified_seconds(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs())
}

fn is_fresh(entry: &Entry, path: &Path, modified: Option<u64>, now: u64) -> bool {
    Path::new(&entry.path) == path
        && entry.modified == modified
        && now.saturating_sub(entry.measured_at) <= MAX_AGE_SECONDS
}

/// Folder holding each game's files, for games Legio already knows about. Steam
/// games count only when their install folder is already recorded.
fn game_folders(database: &Database) -> Result<Vec<(String, PathBuf)>, String> {
    Ok(database
        .games()?
        .into_iter()
        .filter_map(|game| {
            let folder = game.steam_install_path.or(game.installation_root)?;
            Some((game.id, PathBuf::from(folder)))
        })
        .collect())
}

fn load_cache(file: &Path) -> BTreeMap<String, Entry> {
    let bytes = match fs::read(file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return BTreeMap::new(),
        Err(error) => {
            crate::application_log::failure("read_disk_usage_cache", error);
            return BTreeMap::new();
        }
    };
    match serde_json::from_slice::<CacheFile>(&bytes) {
        Ok(cache) if cache.version == CACHE_VERSION => cache.games,
        // A cache from another version or a damaged file is only a cache: measure again.
        Ok(_) => BTreeMap::new(),
        Err(error) => {
            crate::application_log::failure("parse_disk_usage_cache", error);
            BTreeMap::new()
        }
    }
}

impl DiskUsageState {
    pub(crate) fn new(cache_dir: Result<PathBuf, tauri::Error>) -> Self {
        let cache_file = cache_dir.ok().map(|directory| directory.join(CACHE_FILE));
        let entries = cache_file.as_deref().map(load_cache).unwrap_or_default();
        Self {
            cache_file,
            entries: Mutex::new(entries),
            cancel: Mutex::new(None),
            generation: AtomicU64::new(0),
        }
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn running(&self) -> std::sync::MutexGuard<'_, Option<Arc<AtomicBool>>> {
        self.cancel
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn persist(&self) {
        let Some(file) = &self.cache_file else {
            return;
        };
        let cache = CacheFile {
            version: CACHE_VERSION,
            games: self.entries().clone(),
        };
        let result = serde_json::to_vec(&cache)
            .map_err(std::io::Error::other)
            .and_then(|bytes| {
                if let Some(parent) = file.parent() {
                    fs::create_dir_all(parent)?;
                }
                crate::atomic_file::write_atomic(file, &bytes)
            });
        if let Err(error) = result {
            crate::application_log::failure("write_disk_usage_cache", error);
        }
    }

    fn fresh_entry(&self, id: &str, path: &Path, now: u64) -> Option<Entry> {
        let entry = self.entries().get(id)?.clone();
        is_fresh(&entry, path, modified_seconds(path), now).then_some(entry)
    }

    fn snapshot(&self, folders: &[(String, PathBuf)]) -> DiskUsageSnapshot {
        let now = now_seconds();
        DiskUsageSnapshot {
            games: folders
                .iter()
                .filter_map(|(id, path)| {
                    let entry = self.fresh_entry(id, path, now)?;
                    Some(GameSize {
                        game_id: id.clone(),
                        bytes: entry.bytes,
                        partial: entry.partial,
                        measured_at: entry.measured_at,
                    })
                })
                .collect(),
            computing: self.running().is_some(),
        }
    }

    fn store(&self, id: &str, path: &Path, measurement: Measurement, generation: u64) {
        if self.generation.load(Ordering::Acquire) != generation {
            return;
        }
        self.entries().insert(
            id.to_owned(),
            Entry {
                path: path.to_string_lossy().into_owned(),
                bytes: measurement.bytes,
                partial: measurement.partial,
                modified: modified_seconds(path),
                measured_at: now_seconds(),
            },
        );
    }

    fn begin(&self) -> Option<Arc<AtomicBool>> {
        let mut running = self.running();
        if running.is_some() {
            return None;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *running = Some(cancel.clone());
        Some(cancel)
    }

    fn finish(&self) {
        *self.running() = None;
    }

    fn cancel(&self) {
        if let Some(cancel) = self.running().as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    fn forget(&self, id: &str) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        if self.entries().remove(id).is_some() {
            self.persist();
        }
    }
}

fn publish<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = app.emit(CHANGED_EVENT, ()) {
        crate::application_log::failure("publish_disk_usage", error);
    }
}

/// Drops the cached size after an install, update, transfer or uninstall.
pub(crate) fn invalidate<R: Runtime>(app: &AppHandle<R>, game_id: &str) {
    if let Some(state) = app.try_state::<DiskUsageState>() {
        state.forget(game_id);
        publish(app);
    }
}

/// Measures every listed game that has no fresh size, one after the other.
fn run_pass(app: &AppHandle, folders: Vec<(String, PathBuf)>, cancel: &AtomicBool) {
    let state = app.state::<DiskUsageState>();
    let mut measured_any = false;
    for (id, path) in folders {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if state.fresh_entry(&id, &path, now_seconds()).is_some() || !path.is_dir() {
            continue;
        }
        let generation = state.generation.load(Ordering::Acquire);
        let Some(measurement) = folder_size::measure(&path, cancel) else {
            break;
        };
        state.store(&id, &path, measurement, generation);
        measured_any = true;
        publish(app);
        std::thread::sleep(GAME_PAUSE);
    }
    if measured_any {
        state.persist();
    }
    state.finish();
    publish(app);
}

/// Pure sum behind the library bar: `sizes` holds one `(bytes, partial)` per
/// game that has a measurement, `game_count` is how many games the library has.
fn library_usage(
    library_id: &str,
    space: Option<(u64, u64)>,
    sizes: &[(u64, bool)],
    game_count: usize,
) -> LibraryUsage {
    let legio_bytes = sizes
        .iter()
        .fold(0_u64, |sum, (bytes, _)| sum.saturating_add(*bytes));
    let (free_bytes, total_bytes) = match space {
        Some((free, total)) => (Some(free), Some(total)),
        None => (None, None),
    };
    LibraryUsage {
        library_id: library_id.to_owned(),
        total_bytes,
        free_bytes,
        legio_bytes,
        other_bytes: space
            .map(|(free, total)| total.saturating_sub(free).saturating_sub(legio_bytes)),
        estimated: sizes.len() < game_count || sizes.iter().any(|(_, partial)| *partial),
    }
}

#[tauri::command]
pub async fn get_disk_usage(app: AppHandle) -> Result<DiskUsageSnapshot, String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let folders = game_folders(app.state::<DatabaseState>().database()?)?;
        Ok(app.state::<DiskUsageState>().snapshot(&folders))
    })
    .await
    .map_err(|error| format!("Disk usage task failed: {error}"))?
}

/// Starts measuring in the background and returns at once. With `game_id` only
/// that game is measured. Does nothing while a pass is already running.
#[tauri::command]
pub async fn refresh_disk_usage(app: AppHandle, game_id: Option<String>) -> Result<(), String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut folders = game_folders(app.state::<DatabaseState>().database()?)?;
        if let Some(id) = game_id {
            folders.retain(|(candidate, _)| *candidate == id);
        }
        let Some(cancel) = app.state::<DiskUsageState>().begin() else {
            return Ok(());
        };
        let worker = app.clone();
        tauri::async_runtime::spawn_blocking(move || run_pass(&worker, folders, &cancel));
        publish(&app);
        Ok(())
    })
    .await
    .map_err(|error| format!("Disk usage task failed: {error}"))?
}

#[tauri::command]
pub fn cancel_disk_usage(app: AppHandle) {
    app.state::<DiskUsageState>().cancel();
}

#[tauri::command]
pub async fn get_library_usage(app: AppHandle) -> Result<Vec<LibraryUsage>, String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DatabaseState>();
        let database = state.database()?;
        let folders = game_folders(database)?;
        let snapshot = app.state::<DiskUsageState>().snapshot(&folders);
        let mut usages = Vec::new();
        for library in game_library::libraries(database)? {
            let root = PathBuf::from(&library.root);
            let games = game_library::library_games(database, &root)?;
            let sizes: Vec<(u64, bool)> = games
                .iter()
                .filter_map(|game| {
                    snapshot
                        .games
                        .iter()
                        .find(|size| size.game_id == game.id)
                        .map(|size| (size.bytes, size.partial))
                })
                .collect();
            let space = if root.is_dir() {
                installed_folder::disk_space(&root).ok()
            } else {
                None
            };
            usages.push(library_usage(&library.id, space, &sizes, games.len()));
        }
        Ok(usages)
    })
    .await
    .map_err(|error| format!("Library usage task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, modified: Option<u64>, measured_at: u64) -> Entry {
        Entry {
            path: path.to_owned(),
            bytes: 10,
            partial: false,
            modified,
            measured_at,
        }
    }

    #[test]
    fn entry_is_fresh_when_nothing_changed() {
        let cached = entry("/games/a", Some(5), 1_000);
        assert!(is_fresh(&cached, Path::new("/games/a"), Some(5), 1_500));
    }

    #[test]
    fn entry_expires_on_path_mtime_or_age() {
        let cached = entry("/games/a", Some(5), 1_000);
        assert!(!is_fresh(&cached, Path::new("/games/b"), Some(5), 1_500));
        assert!(!is_fresh(&cached, Path::new("/games/a"), Some(6), 1_500));
        assert!(!is_fresh(&cached, Path::new("/games/a"), None, 1_500));
        assert!(is_fresh(
            &cached,
            Path::new("/games/a"),
            Some(5),
            1_000 + MAX_AGE_SECONDS
        ));
        assert!(!is_fresh(
            &cached,
            Path::new("/games/a"),
            Some(5),
            1_001 + MAX_AGE_SECONDS
        ));
    }

    #[test]
    fn forgetting_a_game_blocks_an_older_walk() {
        let state = DiskUsageState::new(Err(tauri::Error::UnknownPath));
        let directory = std::env::temp_dir();
        let measurement = Measurement {
            bytes: 1,
            partial: false,
        };
        let generation = state.generation.load(Ordering::Acquire);
        state.store("game", &directory, measurement, generation);
        assert_eq!(state.entries().len(), 1);
        state.forget("game");
        assert!(state.entries().is_empty());
        state.store("game", &directory, measurement, generation);
        assert!(state.entries().is_empty());
    }

    #[test]
    fn cache_file_round_trips_and_ignores_damage() {
        let directory =
            std::env::temp_dir().join(format!("legio-disk-usage-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let state = DiskUsageState::new(Ok(directory.clone()));
        state
            .entries()
            .insert("game".to_owned(), entry("/games/a", Some(5), 7));
        state.persist();
        let reloaded = DiskUsageState::new(Ok(directory.clone()));
        assert_eq!(
            reloaded.entries().get("game"),
            Some(&entry("/games/a", Some(5), 7))
        );
        fs::write(directory.join(CACHE_FILE), b"not json").unwrap();
        assert!(
            DiskUsageState::new(Ok(directory.clone()))
                .entries()
                .is_empty()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn only_one_pass_runs_and_cancel_reaches_it() {
        let state = DiskUsageState::new(Err(tauri::Error::UnknownPath));
        let cancel = state.begin().unwrap();
        assert!(state.begin().is_none());
        state.cancel();
        assert!(cancel.load(Ordering::Relaxed));
        state.finish();
        assert!(state.begin().is_some());
    }

    #[test]
    fn library_usage_splits_used_space() {
        let usage = library_usage("lib", Some((300, 1_000)), &[(200, false), (100, false)], 2);
        assert_eq!(usage.legio_bytes, 300);
        assert_eq!(usage.other_bytes, Some(400));
        assert_eq!(usage.free_bytes, Some(300));
        assert!(!usage.estimated);
    }

    #[test]
    fn library_usage_is_estimated_when_incomplete() {
        assert!(library_usage("lib", Some((0, 10)), &[(1, false)], 2).estimated);
        assert!(library_usage("lib", Some((0, 10)), &[(1, true)], 1).estimated);
    }

    #[test]
    fn library_usage_never_underflows() {
        let usage = library_usage("lib", Some((900, 1_000)), &[(500, false)], 1);
        assert_eq!(usage.other_bytes, Some(0));
        let unreachable = library_usage("lib", None, &[(500, false)], 1);
        assert_eq!(unreachable.other_bytes, None);
        assert_eq!(unreachable.total_bytes, None);
    }
}
