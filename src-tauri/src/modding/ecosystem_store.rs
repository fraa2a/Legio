//! Fetches, caches and holds the Thunderstore ecosystem schema.

use std::{
    path::PathBuf,
    sync::{Arc, PoisonError},
    time::{Duration, Instant, SystemTime},
};

use tokio::sync::Mutex;

use super::{ecosystem::EcosystemSchema, thunderstore_http::ThunderstoreHttp};

pub(crate) const SCHEMA_URL: &str = "https://thunderstore.io/api/experimental/schema/dev/latest/";
const MAX_SCHEMA_BYTES: usize = 16 * 1024 * 1024;
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
// While offline, do not wait for a connection timeout on every call.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(5 * 60);

/// Lazily loaded ecosystem schema, refreshed daily and served from the disk
/// cache when Thunderstore cannot be reached.
#[derive(Clone)]
pub(crate) struct EcosystemStore {
    inner: Arc<Inner>,
}

struct Inner {
    http: ThunderstoreHttp,
    url: String,
    cache_path: PathBuf,
    state: Mutex<State>,
    // Kept apart from `state`, which stays locked for the length of a fetch.
    latest: std::sync::Mutex<Option<Arc<EcosystemSchema>>>,
}

#[derive(Default)]
struct State {
    loaded: Option<Loaded>,
    last_failure: Option<(Instant, String)>,
}

struct Loaded {
    schema: Arc<EcosystemSchema>,
    fetched_at: SystemTime,
}

impl Loaded {
    fn is_fresh(&self) -> bool {
        self.fetched_at.elapsed().is_ok_and(|age| age < MAX_AGE)
    }
}

impl EcosystemStore {
    /// A schema for work that only needs the install rules: the loaded one
    /// or the disk cache, whatever its age, without touching the network.
    /// Fetches only when nothing is cached.
    pub(crate) async fn cached_schema(&self) -> Result<Arc<EcosystemSchema>, String> {
        if let Some(schema) = self.inner.remembered() {
            return Ok(schema);
        }
        if let Some(loaded) = self.inner.read_cache().await {
            self.inner.remember(&loaded.schema);
            return Ok(loaded.schema);
        }
        self.schema().await
    }

    pub(crate) fn new(http: ThunderstoreHttp, cache_path: PathBuf, url: &str) -> Self {
        Self {
            inner: Arc::new(Inner {
                http,
                url: url.to_owned(),
                cache_path,
                state: Mutex::new(State::default()),
                latest: std::sync::Mutex::new(None),
            }),
        }
    }

    /// The current schema. Errors only when it is neither cached nor fetchable.
    pub(crate) async fn schema(&self) -> Result<Arc<EcosystemSchema>, String> {
        let inner = &self.inner;
        let mut state = inner.state.lock().await;
        if let Some(loaded) = state.loaded.as_ref().filter(|loaded| loaded.is_fresh()) {
            return Ok(loaded.schema.clone());
        }
        if state.loaded.is_none() {
            state.loaded = inner.read_cache().await;
            if let Some(loaded) = &state.loaded {
                inner.remember(&loaded.schema);
            }
            if let Some(loaded) = state.loaded.as_ref().filter(|loaded| loaded.is_fresh()) {
                return Ok(loaded.schema.clone());
            }
        }
        let recent_failure = state
            .last_failure
            .as_ref()
            .filter(|(failed, _)| failed.elapsed() < RETRY_AFTER_FAILURE)
            .map(|(_, error)| error.clone());
        let recently_failed = recent_failure.is_some();
        let fetched = match recent_failure {
            Some(error) => Err(error),
            None => inner.fetch().await,
        };
        match fetched {
            Ok(loaded) => {
                state.last_failure = None;
                let schema = loaded.schema.clone();
                inner.remember(&schema);
                state.loaded = Some(loaded);
                Ok(schema)
            }
            Err(error) => {
                if !recently_failed {
                    state.last_failure = Some((Instant::now(), error.clone()));
                    crate::application_log::failure("ecosystem_schema_fetch", &error);
                }
                match state.loaded.as_ref() {
                    Some(loaded) => Ok(loaded.schema.clone()),
                    None => Err(format!(
                        "Could not load the Thunderstore game list and none is cached: {error}"
                    )),
                }
            }
        }
    }
}

impl Inner {
    fn remember(&self, schema: &Arc<EcosystemSchema>) {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = Some(schema.clone());
    }

    fn remembered(&self) -> Option<Arc<EcosystemSchema>> {
        self.latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    async fn fetch(&self) -> Result<Loaded, String> {
        let bytes = self.http.get_bytes(&self.url, MAX_SCHEMA_BYTES).await?;
        let (bytes, schema) = parse(bytes).await?;
        self.write_cache(&bytes).await?;
        Ok(Loaded {
            schema: Arc::new(schema),
            fetched_at: SystemTime::now(),
        })
    }

    async fn read_cache(&self) -> Option<Loaded> {
        let read = async {
            let metadata = tokio::fs::metadata(&self.cache_path).await?;
            if metadata.len() > MAX_SCHEMA_BYTES as u64 {
                return Ok(None);
            }
            let fetched_at = metadata.modified()?;
            let bytes = tokio::fs::read(&self.cache_path).await?;
            Ok::<_, std::io::Error>(Some((bytes, fetched_at)))
        };
        let (bytes, fetched_at) = match read.await {
            Ok(Some(cached)) => cached,
            Ok(None) => return None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(error) => {
                crate::application_log::failure("ecosystem_schema_cache", error);
                return None;
            }
        };
        match parse(bytes).await {
            Ok((_, schema)) => Some(Loaded {
                schema: Arc::new(schema),
                fetched_at,
            }),
            Err(error) => {
                crate::application_log::failure("ecosystem_schema_cache", error);
                None
            }
        }
    }

    async fn write_cache(&self, bytes: &[u8]) -> Result<(), String> {
        let write_error = |error: std::io::Error| format!("Could not save the game list: {error}");
        if let Some(parent) = self.cache_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(write_error)?;
        }
        let path = self.cache_path.clone();
        let bytes = bytes.to_vec();
        tauri::async_runtime::spawn_blocking(move || {
            crate::atomic_file::write_atomic(&path, &bytes)
        })
        .await
        .map_err(std::io::Error::other)
        .and_then(|written| written)
        .map_err(write_error)
    }
}

async fn parse(bytes: Vec<u8>) -> Result<(Vec<u8>, EcosystemSchema), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let schema: EcosystemSchema = serde_json::from_slice(&bytes)
            .map_err(|error| format!("The Thunderstore game list is invalid: {error}"))?;
        if schema.games.is_empty() {
            return Err("The Thunderstore game list is empty".to_owned());
        }
        Ok((bytes, schema))
    })
    .await
    .map_err(|error| format!("Game list task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        diagnostics::Diagnostics,
        modding::test_support::{Reply, TempDir, TestServer},
    };

    const FIXTURE: &str = include_str!("../../test-fixtures/modding/ecosystem-schema.json");

    fn store(directory: &TempDir, server: &TestServer) -> EcosystemStore {
        let http = ThunderstoreHttp::new(
            "0.1.0",
            Diagnostics::new(Err("test logs disabled".to_owned())),
        )
        .unwrap();
        EcosystemStore::new(
            http,
            directory.path().join("modding").join("ecosystem.json"),
            &server.url("/schema"),
        )
    }

    fn cache_path(directory: &TempDir) -> PathBuf {
        directory.path().join("modding").join("ecosystem.json")
    }

    fn age_cache(directory: &TempDir, age: Duration) {
        let file = std::fs::File::options()
            .write(true)
            .open(cache_path(directory))
            .unwrap();
        file.set_modified(SystemTime::now() - age).unwrap();
    }

    #[test]
    fn fetches_and_caches_the_schema() {
        let directory = TempDir::new();
        let server = TestServer::start();
        server.set("/schema", Reply::ok(FIXTURE.as_bytes().to_vec()));
        let store = store(&directory, &server);
        let schema = tauri::async_runtime::block_on(store.schema()).unwrap();
        assert!(schema.games.contains_key("lethal-company"));
        assert_eq!(
            std::fs::read_to_string(cache_path(&directory)).unwrap(),
            FIXTURE
        );

        tauri::async_runtime::block_on(store.schema()).unwrap();
        assert_eq!(
            server.requests().len(),
            1,
            "a fresh schema is not refetched"
        );
    }

    #[test]
    fn uses_a_fresh_disk_cache_without_the_network() {
        let directory = TempDir::new();
        let server = TestServer::start();
        std::fs::create_dir_all(directory.path().join("modding")).unwrap();
        std::fs::write(cache_path(&directory), FIXTURE).unwrap();
        let store = store(&directory, &server);
        let schema = tauri::async_runtime::block_on(store.schema()).unwrap();
        assert_eq!(schema.games.len(), 3);
        assert!(server.requests().is_empty());
    }

    #[test]
    fn refreshes_a_stale_cache() {
        let directory = TempDir::new();
        let server = TestServer::start();
        let newer = FIXTURE.replace("\"riskofrain2\"", "\"riskofrain2-renamed\"");
        server.set("/schema", Reply::ok(newer.into_bytes()));
        std::fs::create_dir_all(directory.path().join("modding")).unwrap();
        std::fs::write(cache_path(&directory), FIXTURE).unwrap();
        age_cache(&directory, MAX_AGE + Duration::from_secs(60));
        let schema = tauri::async_runtime::block_on(store(&directory, &server).schema()).unwrap();
        assert!(schema.games.contains_key("riskofrain2-renamed"));
        assert!(
            std::fs::read_to_string(cache_path(&directory))
                .unwrap()
                .contains("riskofrain2-renamed")
        );
    }

    #[test]
    fn falls_back_to_a_stale_cache_when_the_fetch_fails() {
        let directory = TempDir::new();
        let server = TestServer::start();
        server.set("/schema", Reply::status(503));
        std::fs::create_dir_all(directory.path().join("modding")).unwrap();
        std::fs::write(cache_path(&directory), FIXTURE).unwrap();
        age_cache(&directory, MAX_AGE + Duration::from_secs(60));
        let store = store(&directory, &server);
        let schema = tauri::async_runtime::block_on(store.schema()).unwrap();
        assert!(schema.games.contains_key("lethal-company"));
        assert_eq!(server.requests().len(), 1);

        tauri::async_runtime::block_on(store.schema()).unwrap();
        assert_eq!(
            server.requests().len(),
            1,
            "failures are not retried at once"
        );
    }

    #[test]
    fn cached_schema_never_waits_for_the_network_when_a_cache_exists() {
        let directory = TempDir::new();
        let server = TestServer::start();
        std::fs::create_dir_all(directory.path().join("modding")).unwrap();
        std::fs::write(cache_path(&directory), FIXTURE).unwrap();
        age_cache(&directory, MAX_AGE + Duration::from_secs(60));
        let store = store(&directory, &server);
        for _ in 0..2 {
            let schema = tauri::async_runtime::block_on(store.cached_schema()).unwrap();
            assert_eq!(schema.games.len(), 3);
        }
        assert!(server.requests().is_empty());
    }

    #[test]
    fn cached_schema_fetches_when_nothing_is_cached() {
        let directory = TempDir::new();
        let server = TestServer::start();
        server.set("/schema", Reply::ok(FIXTURE.as_bytes().to_vec()));
        let store = store(&directory, &server);
        let schema = tauri::async_runtime::block_on(store.cached_schema()).unwrap();
        assert_eq!(schema.games.len(), 3);
        assert_eq!(server.requests().len(), 1);
        tauri::async_runtime::block_on(store.cached_schema()).unwrap();
        assert_eq!(server.requests().len(), 1);
    }

    #[test]
    fn errors_when_there_is_no_cache_and_no_network() {
        let directory = TempDir::new();
        let server = TestServer::start();
        server.set("/schema", Reply::status(503));
        let error = tauri::async_runtime::block_on(store(&directory, &server).schema())
            .err()
            .unwrap();
        assert!(error.contains("none is cached"), "{error}");
    }

    #[test]
    fn keeps_the_cache_when_the_fetched_schema_is_invalid() {
        let directory = TempDir::new();
        let server = TestServer::start();
        for body in ["not json", r#"{"games":{}}"#] {
            server.set("/schema", Reply::ok(body.as_bytes().to_vec()));
            std::fs::create_dir_all(directory.path().join("modding")).unwrap();
            std::fs::write(cache_path(&directory), FIXTURE).unwrap();
            age_cache(&directory, MAX_AGE + Duration::from_secs(60));
            let schema =
                tauri::async_runtime::block_on(store(&directory, &server).schema()).unwrap();
            assert_eq!(schema.games.len(), 3);
            assert_eq!(
                std::fs::read_to_string(cache_path(&directory)).unwrap(),
                FIXTURE
            );
        }
    }

    #[test]
    fn ignores_a_corrupt_cache() {
        let directory = TempDir::new();
        let server = TestServer::start();
        server.set("/schema", Reply::ok(FIXTURE.as_bytes().to_vec()));
        std::fs::create_dir_all(directory.path().join("modding")).unwrap();
        std::fs::write(cache_path(&directory), "{broken").unwrap();
        let schema = tauri::async_runtime::block_on(store(&directory, &server).schema()).unwrap();
        assert_eq!(schema.games.len(), 3);
    }

    #[test]
    fn rejects_an_oversized_schema() {
        let directory = TempDir::new();
        let server = TestServer::start();
        server.set("/schema", Reply::chunked(vec![b' '; MAX_SCHEMA_BYTES + 1]));
        let error = tauri::async_runtime::block_on(store(&directory, &server).schema())
            .err()
            .unwrap();
        assert!(error.contains("larger than allowed"), "{error}");
    }
}
