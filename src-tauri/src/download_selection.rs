use crate::download_resolver::{self, Provider, ResolveError};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MirrorPlan {
    pub uris: Vec<String>,
    pub index: usize,
    pub failures: u8,
    pub expected_size: Option<u64>,
}

impl MirrorPlan {
    pub(crate) fn new(uris: Vec<String>, expected_size: Option<u64>) -> Self {
        Self {
            uris,
            index: 0,
            failures: 0,
            expected_size,
        }
    }

    pub(crate) fn current(&self) -> Option<&str> {
        self.uris.get(self.index).map(String::as_str)
    }

    pub(crate) fn fail(&mut self) -> bool {
        self.failures = self.failures.saturating_add(1);
        if self.failures > 3 {
            self.index += 1;
            self.failures = 0;
        }
        self.current().is_some()
    }

    pub(crate) fn restart(&mut self) {
        self.index = 0;
        self.failures = 0;
    }
}

pub(crate) fn native_cdn(uri: &str) -> bool {
    reqwest::Url::parse(uri)
        .is_ok_and(|url| url.scheme() == "https" && url.host_str() == Some("cdn.taxphobia.top"))
}

pub(crate) fn size_matches(expected: Option<u64>, actual: u64) -> bool {
    actual > 0
        && expected.is_none_or(|expected| {
            expected > 0 && u128::from(expected.abs_diff(actual)) * 100 <= u128::from(expected) * 3
        })
}

pub(crate) fn supported(uri: &str) -> bool {
    matches!(download_resolver::classify(uri), Ok(provider) if !matches!(provider, Provider::Torrent | Provider::Unsupported))
}

pub(crate) async fn inspect(client: &reqwest::Client, uri: &str) -> Result<u64, ResolveError> {
    let timeout = if download_resolver::classify(uri)? == Provider::Gofile {
        Duration::from_secs(125)
    } else {
        Duration::from_secs(20)
    };
    tokio::time::timeout(timeout, inspect_file(client, uri))
        .await
        .map_err(|_| ResolveError::Link("Download host lookup timed out".to_owned()))?
}

async fn inspect_file(client: &reqwest::Client, uri: &str) -> Result<u64, ResolveError> {
    let mut resolved = download_resolver::resolve(client, uri).await?;
    if resolved.files.len() != 1 {
        return Err("Only a single archive per download link is supported".into());
    }
    let file = resolved.files.pop().ok_or("Download contains no files")?;
    let _provider = resolved.provider;
    let name = file.name.or_else(|| {
        reqwest::Url::parse(&file.url)
            .ok()
            .map(|url| url.path().to_owned())
    });
    if name
        .is_some_and(|name| crate::archive_install::is_multipart_name(std::path::Path::new(&name)))
    {
        return Err("Multipart archive files are not supported".into());
    }
    let mut headers = file.headers;
    headers.insert(
        "range",
        reqwest::header::HeaderValue::from_static("bytes=0-7"),
    );
    let mut response = download_resolver::get(client, &file.url, headers).await?;
    if !matches!(
        response.status(),
        reqwest::StatusCode::OK | reqwest::StatusCode::PARTIAL_CONTENT
    ) {
        return Err(ResolveError::Http(response.status()));
    }
    if download_resolver::is_web_page(&response) {
        return Err("The download link returned a web page".into());
    }
    let size = if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        response
            .headers()
            .get("content-range")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("bytes 0-"))
            .and_then(|value| value.rsplit('/').next())
            .and_then(|value| value.parse::<u64>().ok())
    } else {
        response.content_length()
    }
    .filter(|size| *size >= 8)
    .ok_or("The download host did not provide an exact archive size")?;
    let mut magic = Vec::with_capacity(8);
    while magic.len() < 8 {
        let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| ResolveError::Link(error.without_url().to_string()))?
        else {
            break;
        };
        magic.extend_from_slice(&chunk[..chunk.len().min(8 - magic.len())]);
    }
    if !(magic.starts_with(b"PK\x03\x04")
        || magic.starts_with(b"PK\x05\x06")
        || magic.starts_with(b"7z\xbc\xaf\x27\x1c")
        || magic.starts_with(b"Rar!\x1a\x07\x00")
        || magic.starts_with(b"Rar!\x1a\x07\x01\x00"))
    {
        return Err("The link does not provide a supported ZIP, 7z or RAR archive".into());
    }
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_size_allows_exactly_three_percent_without_overflow() {
        assert!(size_matches(Some(1000), 970));
        assert!(size_matches(Some(1000), 1030));
        assert!(!size_matches(Some(1000), 969));
        assert!(!size_matches(Some(1000), 1031));
        assert!(!size_matches(Some(0), 1));
        assert!(!size_matches(None, 0));
        assert!(size_matches(None, 10));
        assert!(size_matches(Some(u64::MAX), u64::MAX));
        assert!(!size_matches(Some(1), u64::MAX));
    }
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn source_order_and_three_retries_survive_serialization() {
        let mut plan = MirrorPlan::new(
            vec![
                "https://example.test/first.zip".into(),
                "https://gofile.io/d/second".into(),
            ],
            None,
        );
        for _ in 0..3 {
            assert!(plan.fail());
            assert_eq!(plan.current(), Some("https://example.test/first.zip"));
        }
        let mut restored: MirrorPlan =
            serde_json::from_str(&serde_json::to_string(&plan).unwrap()).unwrap();
        assert!(restored.fail());
        assert_eq!(restored.current(), Some("https://gofile.io/d/second"));
        for _ in 0..3 {
            assert!(restored.fail());
        }
        assert!(!restored.fail());
        assert_eq!(restored.current(), None);
    }

    #[test]
    fn only_the_exact_native_cdn_uses_the_existing_transfer_path() {
        assert!(native_cdn("https://cdn.taxphobia.top/games/file.zip"));
        assert!(!native_cdn(
            "https://cdn.taxphobia.top.example.org/file.zip"
        ));
        assert!(!native_cdn(
            "https://other.example.org/cdn.taxphobia.top/file.zip"
        ));
    }

    async fn inspect_fixture(
        status: &str,
        content_type: &str,
        body: &[u8],
    ) -> Result<u64, ResolveError> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let uri = format!("http://{}/archive.zip", listener.local_addr().unwrap());
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let body = body.to_vec();
        let server = thread::spawn(move || {
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
            socket.write_all(response.as_bytes()).unwrap();
            socket.write_all(&body).unwrap();
        });
        let result = inspect(&crate::download_resolver::client().unwrap(), &uri).await;
        server.join().unwrap();
        result
    }

    #[tokio::test]
    async fn inspection_accepts_direct_archives_and_rejects_html_and_http_errors() {
        assert_eq!(
            inspect_fixture("200 OK", "application/octet-stream", b"PK\x03\x04abcdef")
                .await
                .unwrap(),
            10
        );
        assert!(
            inspect_fixture("200 OK", "text/html", b"<html>login</html>")
                .await
                .is_err()
        );
        assert!(
            inspect_fixture("200 OK", "application/octet-stream", b"<html>login</html>")
                .await
                .is_err()
        );
        assert!(
            inspect_fixture("503 Unavailable", "application/zip", b"PK\x03\x04abcdef")
                .await
                .is_err()
        );
    }
}
