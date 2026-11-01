//! Shared HTTP layer for Thunderstore: allowlisted hosts, bounded reads and
//! streamed file downloads with cancellation.

use std::{
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

use tokio::{io::AsyncWriteExt, sync::watch};

use crate::{
    diagnostics::{Diagnostics, Operation},
    network::{NetworkError, NetworkState},
};

const CONNECT_TIMEOUT_SECONDS: u64 = 15;
// Applies between body reads, so large downloads never hit a total deadline.
const READ_TIMEOUT_SECONDS: u64 = 30;
const API_REQUEST_TIMEOUT_SECONDS: u64 = 120;
const MAX_REDIRECTS: usize = 5;
const THUNDERSTORE_HOST: &str = "thunderstore.io";
const REDIRECT_REJECTED: &str = "Thunderstore sent a redirect that is not allowed";

/// Client for `thunderstore.io` and its CDN subdomains.
#[derive(Clone)]
pub(crate) struct ThunderstoreHttp {
    client: reqwest::Client,
    diagnostics: Diagnostics,
}

/// Why a download did not produce a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DownloadError {
    Cancelled,
    Failed(String),
}

impl fmt::Display for DownloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Download cancelled"),
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl From<DownloadError> for String {
    fn from(error: DownloadError) -> Self {
        error.to_string()
    }
}

impl ThunderstoreHttp {
    pub(crate) fn new(version: &str, diagnostics: Diagnostics) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECONDS))
            .read_timeout(Duration::from_secs(READ_TIMEOUT_SECONDS))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    attempt.error(REDIRECT_REJECTED)
                } else if is_allowed_url(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.error(REDIRECT_REJECTED)
                }
            }))
            // Tests talk to a local plain HTTP server.
            .https_only(!cfg!(test))
            .user_agent(format!("Legio/{version}"))
            .build()
            .map_err(|error| format!("could not initialize the Thunderstore client: {error}"))?;
        Ok(Self {
            client,
            diagnostics,
        })
    }

    /// GET `url` into memory, failing when the body exceeds `max_bytes`.
    /// Retries once after a transport failure.
    pub(crate) async fn get_bytes(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
        require_allowed(url).map_err(|error| describe(&error))?;
        let first = self.get_bytes_once(url, max_bytes).await;
        let result = if matches!(first, Err(NetworkError::Transport { .. })) {
            self.get_bytes_once(url, max_bytes).await
        } else {
            first
        };
        result.map_err(|error| describe(&error))
    }

    /// One GET attempt that keeps the failure type, for callers that react to
    /// the status code or retry on their own schedule.
    pub(crate) async fn try_get_bytes(
        &self,
        url: &str,
        max_bytes: usize,
    ) -> Result<Vec<u8>, NetworkError> {
        require_allowed(url)?;
        self.get_bytes_once(url, max_bytes).await
    }

    /// One POST attempt with the given body and content type.
    pub(crate) async fn try_post_bytes(
        &self,
        url: &str,
        content_type: &'static str,
        body: Vec<u8>,
        max_bytes: usize,
    ) -> Result<Vec<u8>, NetworkError> {
        require_allowed(url)?;
        let mut log = self.diagnostics.request(Operation::Thunderstore);
        let result = async {
            let response = self
                .client
                .post(url)
                .header(reqwest::header::CONTENT_TYPE, content_type)
                .body(body)
                .timeout(Duration::from_secs(API_REQUEST_TIMEOUT_SECONDS))
                .send()
                .await
                .map_err(network_error)?;
            NetworkState::read_bounded_to(response, max_bytes, &mut log).await
        }
        .await;
        log.finish(&result);
        result
    }

    async fn get_bytes_once(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, NetworkError> {
        let mut log = self.diagnostics.request(Operation::Thunderstore);
        let result = async {
            let response = self
                .client
                .get(url)
                .timeout(Duration::from_secs(API_REQUEST_TIMEOUT_SECONDS))
                .send()
                .await
                .map_err(network_error)?;
            NetworkState::read_bounded_to(response, max_bytes, &mut log).await
        }
        .await;
        log.finish(&result);
        result
    }

    /// Streams `url` to a temporary file next to `dest`, then renames it over
    /// `dest`. Nothing is left at `dest` when the download fails or is
    /// cancelled. `cancel` stops the transfer once its value turns true.
    pub(crate) async fn download_to_file(
        &self,
        url: &str,
        dest: &Path,
        max_bytes: u64,
        mut on_progress: impl FnMut(u64, Option<u64>),
        mut cancel: watch::Receiver<bool>,
    ) -> Result<(), DownloadError> {
        require_allowed(url).map_err(|error| DownloadError::Failed(describe(&error)))?;
        let partial = partial_path(dest)?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| local_failure("create the download folder", &error))?;
        }
        let mut log = self.diagnostics.request(Operation::Thunderstore);
        let result = self
            .stream_to(
                url,
                &partial,
                dest,
                max_bytes,
                &mut on_progress,
                &mut cancel,
                &mut log,
            )
            .await;
        let outcome = match result {
            Ok(()) => {
                log.finish::<()>(&Ok(()));
                Ok(())
            }
            // Dropping the log without finishing records the cancellation.
            Err(Transfer::Cancelled) => Err(DownloadError::Cancelled),
            Err(Transfer::Network(error)) => {
                let message = describe(&error);
                log.finish::<()>(&Err(error));
                Err(DownloadError::Failed(message))
            }
            Err(Transfer::Local(message)) => {
                log.finish::<()>(&Err(NetworkError::Transport {
                    message: message.clone(),
                }));
                Err(DownloadError::Failed(message))
            }
        };
        if outcome.is_err() {
            remove_partial(&partial).await;
        }
        outcome
    }

    #[allow(clippy::too_many_arguments)]
    async fn stream_to(
        &self,
        url: &str,
        partial: &Path,
        dest: &Path,
        max_bytes: u64,
        on_progress: &mut impl FnMut(u64, Option<u64>),
        cancel: &mut watch::Receiver<bool>,
        log: &mut crate::diagnostics::RequestLog<'_>,
    ) -> Result<(), Transfer> {
        let mut response = until_cancelled(cancel, self.client.get(url).send())
            .await?
            .map_err(|error| Transfer::Network(network_error(error)))?;
        log.set_status(response.status().as_u16());
        if !response.status().is_success() {
            return Err(Transfer::Network(NetworkError::Http {
                status: response.status().as_u16(),
            }));
        }
        let total = response.content_length();
        if total.is_some_and(|length| length > max_bytes) {
            return Err(Transfer::Network(NetworkError::TooLarge));
        }
        let mut file = tokio::fs::File::create(partial)
            .await
            .map_err(|error| Transfer::Local(describe_local("create the download file", &error)))?;
        let mut received = 0_u64;
        on_progress(received, total);
        while let Some(chunk) = until_cancelled(cancel, response.chunk())
            .await?
            .map_err(|error| Transfer::Network(network_error(error)))?
        {
            received = received.saturating_add(chunk.len() as u64);
            if received > max_bytes {
                return Err(Transfer::Network(NetworkError::TooLarge));
            }
            file.write_all(&chunk)
                .await
                .map_err(|error| Transfer::Local(describe_local("write the download", &error)))?;
            log.set_response_bytes(usize::try_from(received).unwrap_or(usize::MAX));
            on_progress(received, total);
        }
        if total.is_some_and(|length| length != received) {
            return Err(Transfer::Network(NetworkError::Transport {
                message: "the connection closed before the download finished".to_owned(),
            }));
        }
        file.sync_all()
            .await
            .map_err(|error| Transfer::Local(describe_local("save the download", &error)))?;
        drop(file);
        if *cancel.borrow() {
            return Err(Transfer::Cancelled);
        }
        tokio::fs::rename(partial, dest)
            .await
            .map_err(|error| Transfer::Local(describe_local("finish the download", &error)))
    }
}

enum Transfer {
    Cancelled,
    Network(NetworkError),
    Local(String),
}

/// Thunderstore web and CDN addresses: HTTPS on the default port, no
/// credentials, host `thunderstore.io` or one of its subdomains.
pub(crate) fn is_allowed_url(url: &reqwest::Url) -> bool {
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    #[cfg(test)]
    if url.scheme() == "http" && url.host_str() == Some("127.0.0.1") {
        return true;
    }
    url.scheme() == "https"
        && url.port().is_none()
        && url.host_str().is_some_and(|host| {
            host == THUNDERSTORE_HOST
                || host
                    .strip_suffix(THUNDERSTORE_HOST)
                    .is_some_and(|prefix| prefix.ends_with('.'))
        })
}

fn require_allowed(url: &str) -> Result<(), NetworkError> {
    let rejected = || NetworkError::Transport {
        message: "the address is not a Thunderstore address".to_owned(),
    };
    let url = reqwest::Url::parse(url).map_err(|_| rejected())?;
    if is_allowed_url(&url) {
        Ok(())
    } else {
        Err(rejected())
    }
}

fn network_error(error: reqwest::Error) -> NetworkError {
    if error.is_redirect() {
        NetworkError::Transport {
            message: REDIRECT_REJECTED.to_owned(),
        }
    } else {
        NetworkError::from(error)
    }
}

pub(crate) fn describe(error: &NetworkError) -> String {
    match error {
        NetworkError::Timeout => "Thunderstore did not respond in time".to_owned(),
        NetworkError::Transport { message } => format!("Could not reach Thunderstore: {message}"),
        NetworkError::Http { status } => format!("Thunderstore answered with HTTP {status}"),
        NetworkError::TooLarge => "The Thunderstore response is larger than allowed".to_owned(),
    }
}

fn describe_local(action: &str, error: &std::io::Error) -> String {
    format!("Could not {action}: {error}")
}

fn local_failure(action: &str, error: &std::io::Error) -> DownloadError {
    DownloadError::Failed(describe_local(action, error))
}

fn partial_path(dest: &Path) -> Result<PathBuf, DownloadError> {
    let mut name = dest
        .file_name()
        .ok_or_else(|| DownloadError::Failed("The download destination has no file name".into()))?
        .to_owned();
    name.push(".part");
    Ok(dest.with_file_name(name))
}

async fn remove_partial(partial: &Path) {
    match tokio::fs::remove_file(partial).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => crate::application_log::failure("remove_partial_mod_download", error),
    }
}

async fn until_cancelled<T>(
    cancel: &mut watch::Receiver<bool>,
    future: impl std::future::Future<Output = T>,
) -> Result<T, Transfer> {
    if *cancel.borrow_and_update() {
        return Err(Transfer::Cancelled);
    }
    tokio::select! {
        biased;
        () = cancellation(cancel) => Err(Transfer::Cancelled),
        value = future => Ok(value),
    }
}

// A dropped sender means nobody can cancel any more, so it never resolves.
async fn cancellation(cancel: &mut watch::Receiver<bool>) {
    loop {
        if *cancel.borrow_and_update() {
            return;
        }
        if cancel.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modding::test_support::{Reply, TempDir, TestServer};

    fn http() -> ThunderstoreHttp {
        ThunderstoreHttp::new(
            "0.1.0",
            Diagnostics::new(Err("test logs disabled".to_owned())),
        )
        .unwrap()
    }

    fn never_cancelled() -> (watch::Sender<bool>, watch::Receiver<bool>) {
        watch::channel(false)
    }

    fn parse(url: &str) -> reqwest::Url {
        reqwest::Url::parse(url).unwrap()
    }

    #[test]
    fn allows_only_https_thunderstore_hosts() {
        for url in [
            "https://thunderstore.io/c/lethal-company/api/v1/package-listing-index/",
            "https://gcdn.thunderstore.io/live/repository/packages/a.zip",
            "https://hcdn-1.hcdn.thunderstore.io/x",
        ] {
            assert!(is_allowed_url(&parse(url)), "{url}");
        }
        for url in [
            "http://thunderstore.io/x",
            "https://thunderstore.io.evil.test/x",
            "https://evilthunderstore.io/x",
            "https://thunderstore.io:8443/x",
            "https://user@thunderstore.io/x",
            "https://user:pass@thunderstore.io/x",
            "https://example.com/thunderstore.io",
            "ftp://thunderstore.io/x",
            "https://127.0.0.1/x",
            "http://localhost/x",
        ] {
            assert!(!is_allowed_url(&parse(url)), "{url}");
        }
    }

    #[test]
    fn reads_a_body_and_sends_the_legio_user_agent() {
        let server = TestServer::start();
        server.set("/a", Reply::ok(b"hello".to_vec()));
        let body =
            tauri::async_runtime::block_on(http().get_bytes(&server.url("/a"), 100)).unwrap();
        assert_eq!(body, b"hello");
        assert!(server.headers("/a")[0].contains("user-agent: legio/0.1.0"));
    }

    #[test]
    fn rejects_addresses_outside_the_allowlist_before_connecting() {
        let error = tauri::async_runtime::block_on(http().get_bytes("https://example.com/", 100))
            .unwrap_err();
        assert!(error.contains("not a Thunderstore address"), "{error}");
        let error = tauri::async_runtime::block_on(http().get_bytes("not a url", 100)).unwrap_err();
        assert!(error.contains("not a Thunderstore address"), "{error}");
    }

    #[test]
    fn enforces_the_size_cap_on_declared_and_streamed_bodies() {
        let server = TestServer::start();
        server.set("/declared", Reply::ok(vec![b'x'; 2000]));
        server.set("/streamed", Reply::chunked(vec![b'x'; 2000]));
        for path in ["/declared", "/streamed"] {
            let error = tauri::async_runtime::block_on(http().get_bytes(&server.url(path), 1000))
                .unwrap_err();
            assert!(error.contains("larger than allowed"), "{path}: {error}");
        }
        let body = tauri::async_runtime::block_on(http().get_bytes(&server.url("/declared"), 2000))
            .unwrap();
        assert_eq!(body.len(), 2000);
    }

    #[test]
    fn reports_http_failures_without_retrying() {
        let server = TestServer::start();
        server.set("/missing", Reply::status(404));
        let error = tauri::async_runtime::block_on(http().get_bytes(&server.url("/missing"), 100))
            .unwrap_err();
        assert!(error.contains("HTTP 404"), "{error}");
        assert_eq!(server.requests().len(), 1);
    }

    #[test]
    fn follows_redirects_within_the_allowlist() {
        let server = TestServer::start();
        server.set("/start", Reply::redirect(&server.url("/end")));
        server.set("/end", Reply::ok(b"done".to_vec()));
        let body =
            tauri::async_runtime::block_on(http().get_bytes(&server.url("/start"), 100)).unwrap();
        assert_eq!(body, b"done");
    }

    #[test]
    fn rejects_redirects_to_other_hosts() {
        let server = TestServer::start();
        server.set(
            "/start",
            Reply::redirect("https://evil.example.com/payload"),
        );
        let error = tauri::async_runtime::block_on(http().get_bytes(&server.url("/start"), 100))
            .unwrap_err();
        assert!(error.contains("not allowed"), "{error}");

        let directory = TempDir::new();
        let dest = directory.path().join("package.zip");
        let (_keep, cancel) = never_cancelled();
        let error = tauri::async_runtime::block_on(http().download_to_file(
            &server.url("/start"),
            &dest,
            100,
            |_, _| {},
            cancel,
        ))
        .unwrap_err();
        assert!(error.to_string().contains("not allowed"), "{error}");
        assert!(!dest.exists());
    }

    #[test]
    fn stops_after_too_many_redirects() {
        let server = TestServer::start();
        server.set("/loop", Reply::redirect(&server.url("/loop")));
        let error = tauri::async_runtime::block_on(http().get_bytes(&server.url("/loop"), 100))
            .unwrap_err();
        assert!(error.contains("not allowed"), "{error}");
        assert!(server.requests().len() <= 2 * (MAX_REDIRECTS + 1));
    }

    #[test]
    fn downloads_to_a_file_with_progress() {
        let server = TestServer::start();
        server.set("/file", Reply::ok(vec![7; 5000]));
        let directory = TempDir::new();
        let dest = directory.path().join("nested").join("package.zip");
        let (_keep, cancel) = never_cancelled();
        let mut progress = Vec::new();
        tauri::async_runtime::block_on(http().download_to_file(
            &server.url("/file"),
            &dest,
            5000,
            |received, total| progress.push((received, total)),
            cancel,
        ))
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), vec![7; 5000]);
        assert!(!directory.path().join("nested/package.zip.part").exists());
        assert_eq!(progress.first(), Some(&(0, Some(5000))));
        assert_eq!(progress.last(), Some(&(5000, Some(5000))));
    }

    #[test]
    fn download_enforces_max_bytes_and_cleans_up() {
        let server = TestServer::start();
        server.set("/declared", Reply::ok(vec![1; 2000]));
        server.set("/streamed", Reply::chunked(vec![1; 2000]));
        let directory = TempDir::new();
        for path in ["/declared", "/streamed"] {
            let dest = directory.path().join("package.zip");
            std::fs::write(&dest, b"previous").unwrap();
            let (_keep, cancel) = never_cancelled();
            let error = tauri::async_runtime::block_on(http().download_to_file(
                &server.url(path),
                &dest,
                1000,
                |_, _| {},
                cancel,
            ))
            .unwrap_err();
            assert!(error.to_string().contains("larger than allowed"), "{error}");
            assert_eq!(std::fs::read(&dest).unwrap(), b"previous");
            assert!(!directory.path().join("package.zip.part").exists());
        }
    }

    #[test]
    fn download_reports_http_errors() {
        let server = TestServer::start();
        server.set("/gone", Reply::status(500));
        let directory = TempDir::new();
        let dest = directory.path().join("package.zip");
        let (_keep, cancel) = never_cancelled();
        let error = tauri::async_runtime::block_on(http().download_to_file(
            &server.url("/gone"),
            &dest,
            100,
            |_, _| {},
            cancel,
        ))
        .unwrap_err();
        assert!(error.to_string().contains("HTTP 500"), "{error}");
        assert!(!dest.exists());
    }

    #[test]
    fn download_can_be_cancelled() {
        let server = TestServer::start();
        server.set("/slow", Reply::stall_after(vec![1; 100]));
        let directory = TempDir::new();
        let dest = directory.path().join("package.zip");
        let (sender, cancel) = watch::channel(false);
        let error = tauri::async_runtime::block_on(http().download_to_file(
            &server.url("/slow"),
            &dest,
            5000,
            |received, _| {
                if received > 0 {
                    sender.send_replace(true);
                }
            },
            cancel,
        ))
        .unwrap_err();
        assert_eq!(error, DownloadError::Cancelled);
        assert!(!dest.exists());
        assert!(!directory.path().join("package.zip.part").exists());
    }

    #[test]
    fn download_without_a_live_canceller_still_completes() {
        let server = TestServer::start();
        server.set("/file", Reply::ok(b"data".to_vec()));
        let directory = TempDir::new();
        let dest = directory.path().join("package.zip");
        let (sender, cancel) = watch::channel(false);
        drop(sender);
        tauri::async_runtime::block_on(http().download_to_file(
            &server.url("/file"),
            &dest,
            100,
            |_, _| {},
            cancel,
        ))
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"data");
    }
}
