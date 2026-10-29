use crate::database::{Database, database_error};
use rusqlite::{OptionalExtension, params};
use std::{collections::BTreeMap, sync::Arc};

const TTL_SECONDS: i64 = 24 * 60 * 60;
const CONCURRENCY: usize = 4;

pub(crate) type LinkResults = BTreeMap<String, Result<u64, String>>;

fn now() -> Result<i64, String> {
    Ok(crate::database::now_milliseconds()? / 1000)
}

fn is_fresh(checked_at: i64, current_time: i64) -> bool {
    checked_at <= current_time && current_time - checked_at < TTL_SECONDS
}

fn load_fresh(database: &Database, uris: &[String]) -> Result<LinkResults, String> {
    let current_time = now()?;
    database.with_connection(|connection| {
        let mut statement = connection
            .prepare("SELECT size_bytes, error, checked_at FROM link_checks WHERE url = ?1")
            .map_err(database_error)?;
        let mut found = LinkResults::new();
        for uri in uris {
            let row = statement
                .query_row([uri], |row| {
                    Ok((
                        row.get::<_, Option<i64>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })
                .optional()
                .map_err(database_error)?;
            let Some((size, error, checked_at)) = row else {
                continue;
            };
            if !is_fresh(checked_at, current_time) {
                continue;
            }
            let result = match size.and_then(|size| u64::try_from(size).ok()) {
                Some(size) => Ok(size),
                None => Err(error.unwrap_or_else(|| "The download link is unavailable".into())),
            };
            found.insert(uri.clone(), result);
        }
        Ok(found)
    })
}

fn store(database: &Database, results: &LinkResults) -> Result<(), String> {
    let current_time = now()?;
    database.with_connection(|connection| {
        for (uri, result) in results {
            let (size, error) = match result {
                Ok(size) => (i64::try_from(*size).ok(), None),
                Err(error) => (None, Some(error.as_str())),
            };
            connection
                .execute(
                    "INSERT INTO link_checks (url, size_bytes, error, checked_at) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(url) DO UPDATE SET size_bytes = ?2, error = ?3, checked_at = ?4",
                    params![uri, size, error, current_time],
                )
                .map_err(database_error)?;
        }
        Ok(())
    })
}

pub(crate) fn cached_size(database: &Database, uri: &str) -> Result<Option<u64>, String> {
    Ok(load_fresh(database, &[uri.to_owned()])?
        .remove(uri)
        .and_then(Result::ok))
}

/// Reachable download links of every cached release of a game, with exact sizes.
pub(crate) async fn check_game(
    app: tauri::AppHandle,
    steam_app_id: u32,
) -> Result<BTreeMap<String, u64>, String> {
    use tauri::Manager;
    let database = app
        .state::<crate::database::DatabaseState>()
        .shared_database()?;
    let read_database = database.clone();
    let links = tauri::async_runtime::spawn_blocking(move || {
        let mut links = BTreeMap::new();
        for row in crate::legio_source_cache::cached_rows(&read_database)?.iter() {
            for entry in row.manifest.verified.iter().chain(&row.manifest.unverified) {
                if entry.steam_app_id != steam_app_id {
                    continue;
                }
                let uris = entry
                    .id
                    .as_ref()
                    .and_then(|id| row.catalog.candidates.get(id))
                    .cloned()
                    .unwrap_or_else(|| vec![entry.download.url.clone()]);
                for uri in uris {
                    links.insert(uri, entry.download.size_bytes);
                }
            }
        }
        Ok::<_, String>(links.into_iter().collect::<Vec<_>>())
    })
    .await
    .map_err(|error| format!("Source cache task failed: {error}"))??;
    let results = check_links(database, &crate::download_resolver::client()?, links).await?;
    Ok(results
        .into_iter()
        .filter_map(|(uri, result)| result.ok().map(|size| (uri, size)))
        .collect())
}

#[cfg(test)]
pub(crate) fn store_for_test(database: &Database, uri: &str, size: u64) {
    store(database, &LinkResults::from([(uri.to_owned(), Ok(size))])).unwrap();
}

fn check_size(expected: Option<u64>, result: Result<u64, String>) -> Result<u64, String> {
    result.and_then(|size| {
        if crate::download_selection::size_matches(expected, size) {
            Ok(size)
        } else {
            Err("Download does not match the size published by the source".into())
        }
    })
}

/// Results are cached for 24 hours, failures included. Links on the native CDN
/// with a published size are trusted without a request.
pub(crate) async fn check_links(
    database: Arc<Database>,
    client: &reqwest::Client,
    links: Vec<(String, Option<u64>)>,
) -> Result<LinkResults, String> {
    let uris: Vec<String> = links.iter().map(|(uri, _)| uri.clone()).collect();
    let read_database = database.clone();
    let cached = tauri::async_runtime::spawn_blocking(move || load_fresh(&read_database, &uris))
        .await
        .map_err(|error| format!("Link cache task failed: {error}"))??;
    let mut results = LinkResults::new();
    let mut fresh = LinkResults::new();
    let mut pending = Vec::new();
    for (uri, expected) in links {
        if let (true, Some(size)) = (crate::download_selection::native_cdn(&uri), expected) {
            results.insert(uri, Ok(size));
        } else if let Some(result) = cached.get(&uri) {
            results.insert(uri, check_size(expected, result.clone()));
        } else {
            pending.push((uri, expected));
        }
    }
    let mut pending = pending.into_iter();
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        while tasks.len() < CONCURRENCY {
            let Some((uri, expected)) = pending.next() else {
                break;
            };
            let client = client.clone();
            tasks.spawn(async move {
                let result = crate::download_selection::inspect(&client, &uri)
                    .await
                    .map_err(|error| error.to_string());
                (uri, expected, result)
            });
        }
        let Some(joined) = tasks.join_next().await else {
            break;
        };
        let (uri, expected, result) =
            joined.map_err(|error| format!("Link check task failed: {error}"))?;
        fresh.insert(uri.clone(), result.clone());
        results.insert(uri, check_size(expected, result));
    }
    if !fresh.is_empty() {
        tauri::async_runtime::spawn_blocking(move || store(&database, &fresh))
            .await
            .map_err(|error| format!("Link cache task failed: {error}"))??;
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> (std::path::PathBuf, Database) {
        let directory = std::env::temp_dir().join(format!("legio-link-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let database = Database::open(&directory).unwrap();
        (directory, database)
    }

    #[test]
    fn freshness_expires_after_a_day_and_rejects_future_timestamps() {
        assert!(is_fresh(100, 100));
        assert!(is_fresh(100, 100 + TTL_SECONDS - 1));
        assert!(!is_fresh(100, 100 + TTL_SECONDS));
        assert!(!is_fresh(101, 100));
    }

    #[test]
    fn stores_successes_and_failures_for_reuse() {
        let (directory, database) = database();
        let mut results = LinkResults::new();
        results.insert("https://example.test/a.zip".into(), Ok(123));
        results.insert("https://example.test/b.zip".into(), Err("HTTP 404".into()));
        store(&database, &results).unwrap();
        let loaded = load_fresh(
            &database,
            &[
                "https://example.test/a.zip".into(),
                "https://example.test/b.zip".into(),
                "https://example.test/c.zip".into(),
            ],
        )
        .unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded["https://example.test/a.zip"], Ok(123));
        assert_eq!(loaded["https://example.test/b.zip"], Err("HTTP 404".into()));
        assert_eq!(
            cached_size(&database, "https://example.test/a.zip").unwrap(),
            Some(123)
        );
        assert_eq!(
            cached_size(&database, "https://example.test/b.zip").unwrap(),
            None
        );
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn native_cdn_links_with_a_size_skip_the_network() {
        let (directory, database) = database();
        let client = crate::download_resolver::client().unwrap();
        let uri = "https://cdn.taxphobia.top/game.zip".to_owned();
        let results = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            check_links(Arc::new(database), &client, vec![(uri.clone(), Some(10))]),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(results[&uri], Ok(10));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn rejects_pages_and_caches_the_outcome() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        let (directory, database) = database();
        let database = Arc::new(database);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let good = format!("{base}/good.zip");
        let page = format!("{base}/page.zip");
        // Exactly one request per link: the second check must be served from the cache.
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let request = String::from_utf8(request).unwrap();
                let (kind, body): (&str, &[u8]) = if request.starts_with("GET /page") {
                    ("text/html", b"<html>landing page</html>")
                } else {
                    ("application/zip", b"PK\x03\x04abcdef")
                };
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).unwrap();
                socket.write_all(body).unwrap();
            }
        });
        let client = crate::download_resolver::client().unwrap();
        let links = vec![(good.clone(), None), (page.clone(), None)];
        let first = check_links(database.clone(), &client, links.clone())
            .await
            .unwrap();
        server.join().unwrap();
        assert_eq!(first[&good], Ok(10));
        assert!(first[&page].is_err());
        let second = check_links(database, &client, links).await.unwrap();
        assert_eq!(second, first);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
