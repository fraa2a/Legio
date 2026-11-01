//! Packages Thunderstore lists that are not mods: mod managers, SDKs and other
//! tools whose archives must never be installed into a profile.
//!
//! Thunderstore exposes no per-package flag for this (the v1 listing, the
//! ecosystem schema and the community categories carry nothing usable), so
//! Legio reads the list r2modman maintains for the same purpose: one
//! `Owner-Name` per line, in `modExclusions.md` of the r2modmanPlus repository.

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant, SystemTime},
};

use super::{package_id::PackageName, thunderstore_http::ThunderstoreHttp};

const MAX_LIST_BYTES: usize = 256 * 1024;
// Keeps the SQL parameter count of a catalog query far below SQLite's limit.
const MAX_ENTRIES: usize = 5000;
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
// While GitHub cannot be reached, do not wait for a timeout on every sync.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(5 * 60);

/// Lowercase `Owner-Name` keys, as Thunderstore compares package names.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ExclusionList {
    names: HashSet<String>,
}

impl ExclusionList {
    pub(crate) fn contains(&self, package: &PackageName) -> bool {
        self.names.contains(&package.to_string().to_lowercase())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Sorted, so the same list always produces the same query.
    pub(crate) fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.names.iter().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// Reads one package per line. Lines that are not package names are
    /// ignored, and a list without any name is rejected as broken.
    fn parse(text: &str) -> Result<Self, String> {
        let names: HashSet<String> = text
            .lines()
            .map(str::trim)
            .filter_map(|line| line.parse::<PackageName>().ok())
            .map(|package| package.to_string().to_lowercase())
            .collect();
        if names.is_empty() {
            return Err("The mod exclusion list has no package names".to_owned());
        }
        if names.len() > MAX_ENTRIES {
            return Err("The mod exclusion list is larger than allowed".to_owned());
        }
        Ok(Self { names })
    }
}

/// The exclusion list, refreshed daily from GitHub and served from the disk
/// copy when it cannot be reached.
#[derive(Clone)]
pub(crate) struct ModExclusions {
    inner: Arc<Inner>,
}

struct Inner {
    url: String,
    cache_path: PathBuf,
    list: Mutex<Option<Arc<ExclusionList>>>,
    last_failure: Mutex<Option<Instant>>,
}

impl ModExclusions {
    pub(crate) fn new(cache_path: PathBuf, url: &str) -> Self {
        Self {
            inner: Arc::new(Inner {
                url: url.to_owned(),
                cache_path,
                list: Mutex::new(None),
                last_failure: Mutex::new(None),
            }),
        }
    }

    /// The last list fetched, or the disk copy of it, whatever its age; empty
    /// before the first fetch. Reads the disk copy once, so call it from a
    /// blocking thread.
    pub(crate) fn list(&self) -> Arc<ExclusionList> {
        let mut slot = self
            .inner
            .list
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(list) = slot.as_ref() {
            return list.clone();
        }
        match self.inner.read_cache() {
            Some(list) => {
                let list = Arc::new(list);
                *slot = Some(list.clone());
                list
            }
            None => Arc::new(ExclusionList::default()),
        }
    }

    /// Fetches the list unless the copy is less than a day old. A failure is
    /// logged and leaves the previous list in place, so browsing never waits
    /// on it.
    pub(crate) async fn refresh(&self, http: &ThunderstoreHttp) {
        let inner = &self.inner;
        if inner.is_fresh() || inner.recently_failed() {
            return;
        }
        match inner.fetch(http).await {
            Ok(list) => {
                *inner.list.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(list));
            }
            Err(error) => {
                *inner
                    .last_failure
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
                crate::application_log::failure("mod_exclusion_list_fetch", error);
            }
        }
    }
}

impl Inner {
    fn is_fresh(&self) -> bool {
        std::fs::metadata(&self.cache_path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age < MAX_AGE)
    }

    fn recently_failed(&self) -> bool {
        self.last_failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some_and(|failed| failed.elapsed() < RETRY_AFTER_FAILURE)
    }

    async fn fetch(&self, http: &ThunderstoreHttp) -> Result<ExclusionList, String> {
        let bytes = http.get_exclusion_list(&self.url, MAX_LIST_BYTES).await?;
        let text = String::from_utf8(bytes)
            .map_err(|_| "The mod exclusion list is not valid text".to_owned())?;
        let list = ExclusionList::parse(&text)?;
        let path = self.cache_path.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            crate::atomic_file::write_atomic(&path, text.as_bytes())
        })
        .await
        .map_err(std::io::Error::other)
        .and_then(|written| written)
        .map_err(|error| format!("Could not save the mod exclusion list: {error}"))?;
        Ok(list)
    }

    fn read_cache(&self) -> Option<ExclusionList> {
        let text = match std::fs::read_to_string(&self.cache_path) {
            Ok(text) if text.len() <= MAX_LIST_BYTES => text,
            Ok(_) => return None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(error) => {
                crate::application_log::failure("mod_exclusion_list_cache", error);
                return None;
            }
        };
        match ExclusionList::parse(&text) {
            Ok(list) => Some(list),
            Err(error) => {
                crate::application_log::failure("mod_exclusion_list_cache", error);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        diagnostics::Diagnostics,
        modding::test_support::{Reply, TempDir, TestServer},
    };

    // Blank lines between entries, as in the published file.
    const LIST: &str =
        "ebkr-r2modman\n\nKesomannen-GaleModManager\r\n\n  Foo-Bar_Baz  \nnot a package\n# note\n";

    fn http() -> ThunderstoreHttp {
        ThunderstoreHttp::new(
            "0.1.0",
            Diagnostics::new(Err("test logs disabled".to_owned())),
        )
        .unwrap()
    }

    fn store(directory: &TempDir, server: &TestServer) -> ModExclusions {
        ModExclusions::new(
            directory.path().join("modding").join("exclusions.md"),
            &server.url("/list"),
        )
    }

    fn cache_path(directory: &TempDir) -> PathBuf {
        directory.path().join("modding").join("exclusions.md")
    }

    fn age_cache(directory: &TempDir, age: Duration) {
        let file = std::fs::File::options()
            .write(true)
            .open(cache_path(directory))
            .unwrap();
        file.set_modified(SystemTime::now() - age).unwrap();
    }

    fn package(name: &str) -> PackageName {
        name.parse().unwrap()
    }

    #[test]
    fn parses_one_package_per_line() {
        let list = ExclusionList::parse(LIST).unwrap();
        assert_eq!(
            list.names(),
            ["ebkr-r2modman", "foo-bar_baz", "kesomannen-galemodmanager"]
        );
        assert!(list.contains(&package("EBKR-R2Modman")));
        assert!(list.contains(&package("Foo-Bar_Baz")));
        assert!(!list.contains(&package("Good-Mod")));
    }

    #[test]
    fn rejects_a_list_without_packages() {
        for text in ["", "\n\n", "<html>blocked</html>"] {
            assert!(ExclusionList::parse(text).is_err(), "{text}");
        }
        let huge: String = (0..=MAX_ENTRIES).map(|n| format!("A-B{n}\n")).collect();
        assert!(ExclusionList::parse(&huge).is_err());
    }

    #[test]
    fn fetches_caches_and_does_not_refetch_while_fresh() {
        let directory = TempDir::new();
        let server = TestServer::start();
        server.set("/list", Reply::ok(LIST.as_bytes().to_vec()));
        let store = store(&directory, &server);
        assert!(store.list().is_empty());

        tauri::async_runtime::block_on(store.refresh(&http()));
        assert!(store.list().contains(&package("ebkr-r2modman")));
        assert_eq!(
            std::fs::read_to_string(cache_path(&directory)).unwrap(),
            LIST
        );

        tauri::async_runtime::block_on(store.refresh(&http()));
        assert_eq!(server.requests().len(), 1);
    }

    #[test]
    fn reads_the_disk_copy_without_the_network() {
        let directory = TempDir::new();
        let server = TestServer::start();
        std::fs::create_dir_all(directory.path().join("modding")).unwrap();
        std::fs::write(cache_path(&directory), LIST).unwrap();
        let store = store(&directory, &server);
        assert!(store.list().contains(&package("Kesomannen-GaleModManager")));
        tauri::async_runtime::block_on(store.refresh(&http()));
        assert!(server.requests().is_empty());
    }

    #[test]
    fn keeps_the_stale_copy_when_the_fetch_fails_or_is_broken() {
        let directory = TempDir::new();
        let server = TestServer::start();
        std::fs::create_dir_all(directory.path().join("modding")).unwrap();
        std::fs::write(cache_path(&directory), LIST).unwrap();
        for reply in [Reply::status(503), Reply::ok(b"<html></html>".to_vec())] {
            age_cache(&directory, MAX_AGE + Duration::from_secs(60));
            server.set("/list", reply);
            let store = store(&directory, &server);
            tauri::async_runtime::block_on(store.refresh(&http()));
            assert!(store.list().contains(&package("ebkr-r2modman")));
            assert_eq!(
                std::fs::read_to_string(cache_path(&directory)).unwrap(),
                LIST
            );

            let before = server.requests().len();
            tauri::async_runtime::block_on(store.refresh(&http()));
            assert_eq!(
                server.requests().len(),
                before,
                "failures are not retried at once"
            );
        }
    }

    #[test]
    fn replaces_a_stale_copy_with_the_new_list() {
        let directory = TempDir::new();
        let server = TestServer::start();
        server.set("/list", Reply::ok(b"New-Tool\n".to_vec()));
        std::fs::create_dir_all(directory.path().join("modding")).unwrap();
        std::fs::write(cache_path(&directory), LIST).unwrap();
        age_cache(&directory, MAX_AGE + Duration::from_secs(60));
        let store = store(&directory, &server);
        tauri::async_runtime::block_on(store.refresh(&http()));
        let list = store.list();
        assert!(list.contains(&package("New-Tool")));
        assert!(!list.contains(&package("ebkr-r2modman")));
    }
}
