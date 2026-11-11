use regex::Regex;
use reqwest::{
    Client, ClientBuilder, Method, Response, StatusCode, Url,
    header::{CONTENT_TYPE, HeaderMap, HeaderValue, LOCATION},
};
use serde::{Deserialize, Serialize};
use std::{sync::LazyLock, time::Duration};

static FUCKING_FAST_LINK: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r#"https://fuckingfast\.co/dl/[^\s"'<>\\]+"#).ok());
static MEDIAFIRE_LINK: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r#"https?://download[0-9]+\.mediafire\.com/[^\s"'<>\\]+"#).ok());
static ROOTZ_PAGE_TOKEN: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r#"pageToken[\\]*"\s*:\s*[\\]*"([^"\\]+)"#).ok());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Provider {
    Direct,
    Gofile,
    PixelDrain,
    Mediafire,
    FuckingFast,
    Rootz,
    OneFichier,
    Buzzheavier,
    Torrent,
    Unsupported,
}

pub(crate) struct ResolvedFile {
    pub url: String,
    pub name: Option<String>,
    pub headers: HeaderMap,
}
pub(crate) struct ResolvedDownload {
    pub provider: Provider,
    pub files: Vec<ResolvedFile>,
}

/// Plain HTTP is only acceptable when the archive hash is verified after the transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlainHttp {
    Allowed,
    Refused,
}

impl PlainHttp {
    pub(crate) fn for_hash(sha256: Option<&str>) -> Self {
        if sha256.is_some_and(|hash| !hash.is_empty()) {
            Self::Allowed
        } else {
            Self::Refused
        }
    }

    pub(crate) fn require_secure(self, url: &Url) -> Result<(), ResolveError> {
        if self == Self::Refused && url.scheme() != "https" {
            return Err(ResolveError::Insecure);
        }
        Ok(())
    }
}

/// Lets the download queue decide whether a failure waits for connectivity or tries another link.
#[derive(Debug)]
pub(crate) enum ResolveError {
    Network(String),
    Http(StatusCode),
    Link(String),
    /// The link needs plain HTTP and the job has no hash to verify the archive.
    Insecure,
    /// The host asks to try again later; `retry_after` is its stated wait, if any.
    RateLimited {
        message: String,
        retry_after: Option<Duration>,
    },
}

impl ResolveError {
    /// Failures that say nothing lasting about the link, so retrying later can succeed.
    pub(crate) fn is_transient(&self) -> bool {
        match self {
            Self::Network(_) | Self::RateLimited { .. } => true,
            Self::Http(status) => {
                status.is_server_error() || matches!(status.as_u16(), 408 | 425 | 429)
            }
            Self::Link(_) | Self::Insecure => false,
        }
    }
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(message) => write!(formatter, "Network: {message}"),
            Self::Http(status) => write!(formatter, "HTTP {}", status.as_u16()),
            Self::Insecure => {
                formatter.write_str("Download host does not offer a secure connection")
            }
            Self::Link(message) | Self::RateLimited { message, .. } => formatter.write_str(message),
        }
    }
}

impl From<String> for ResolveError {
    fn from(error: String) -> Self {
        Self::Link(error)
    }
}

impl From<&str> for ResolveError {
    fn from(error: &str) -> Self {
        Self::Link(error.to_owned())
    }
}

impl From<ResolveError> for String {
    fn from(error: ResolveError) -> Self {
        error.to_string()
    }
}

/// Seconds form of `Retry-After`; the date form is ignored.
pub(crate) fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
        .map(Duration::from_secs)
}

pub(crate) fn network_error(error: reqwest::Error) -> ResolveError {
    ResolveError::Network(error.without_url().to_string())
}

fn host_is(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

pub(crate) fn classify(uri: &str) -> Result<Provider, String> {
    if !crate::hydra_source::valid_uri(uri) {
        return Err("Invalid download URI".into());
    }
    let url = Url::parse(uri).map_err(|_| "Invalid download URL")?;
    if url.scheme() == "magnet" || url.path().to_lowercase().ends_with(".torrent") {
        return Ok(Provider::Torrent);
    }
    if crate::hydra_source::paid_or_backend_only(uri) {
        return Ok(Provider::Unsupported);
    }
    let host = url.host_str().ok_or("Missing download host")?;
    Ok(
        if host_is(host, "filekeeper.net") || host_is(host, "akirabox.com") {
            Provider::Unsupported
        } else if host_is(host, "gofile.io") && url.path().starts_with("/d/") {
            Provider::Gofile
        } else if host_is(host, "pixeldrain.com") && url.path().starts_with("/u/") {
            Provider::PixelDrain
        } else if matches!(host, "mediafire.com" | "www.mediafire.com") {
            Provider::Mediafire
        } else if host_is(host, "fuckingfast.co") && !url.path().starts_with("/dl/") {
            Provider::FuckingFast
        } else if host_is(host, "rootz.so") && url.path().starts_with("/d/") {
            Provider::Rootz
        } else if crate::one_fichier::file_id(&url).is_some() {
            Provider::OneFichier
        } else if crate::buzzheavier::file_id(&url).is_some() {
            Provider::Buzzheavier
        } else {
            Provider::Direct
        },
    )
}

pub(crate) async fn get(
    client: &Client,
    uri: &str,
    headers: HeaderMap,
    plain_http: PlainHttp,
) -> Result<Response, ResolveError> {
    follow_redirects(client, Method::GET, uri, headers, plain_http).await
}

pub(crate) async fn head(
    client: &Client,
    uri: &str,
    headers: HeaderMap,
    plain_http: PlainHttp,
) -> Result<Response, ResolveError> {
    follow_redirects(client, Method::HEAD, uri, headers, plain_http).await
}

async fn follow_redirects(
    client: &Client,
    method: Method,
    uri: &str,
    mut headers: HeaderMap,
    plain_http: PlainHttp,
) -> Result<Response, ResolveError> {
    let mut url = Url::parse(uri).map_err(|_| "Invalid resolved URL")?;
    for hop in 0..=5 {
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("Invalid resolved URL".into());
        }
        plain_http.require_secure(&url)?;
        // IP literal hosts are never passed to the DNS resolver.
        if !crate::public_address::is_public_host(&url) {
            return Err("Download host is not a public internet address".into());
        }
        let response = client
            .request(method.clone(), url.clone())
            .headers(headers.clone())
            .send()
            .await
            .map_err(network_error)?;
        if response.status().is_redirection() {
            if hop == 5 {
                return Err("Download redirected too many times".into());
            }
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or("Redirect has no valid location")?;
            let next = url
                .join(location)
                .map_err(|_| "Invalid redirect location")?;
            reject_downgrade(&url, &next)?;
            if url.origin() != next.origin() {
                headers.remove("authorization");
                headers.remove("cookie");
            }
            url = next;
            continue;
        }
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(ResolveError::RateLimited {
                message: "The download host is limiting requests, try again later".into(),
                retry_after: retry_after(response.headers()),
            });
        }
        if !response.status().is_success() {
            return Err(ResolveError::Http(response.status()));
        }
        return Ok(response);
    }
    Err("Download redirect limit reached".into())
}

fn reject_downgrade(from: &Url, to: &Url) -> Result<(), ResolveError> {
    if from.scheme() == "https" && to.scheme() != "https" {
        return Err("Download redirected to an insecure connection".into());
    }
    Ok(())
}

pub(crate) async fn text(
    client: &Client,
    url: &str,
    headers: HeaderMap,
    plain_http: PlainHttp,
) -> Result<String, ResolveError> {
    body_text(get(client, url, headers, plain_http).await?).await
}

pub(crate) async fn body_text(mut response: Response) -> Result<String, ResolveError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("Host response exceeds the size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| "Host returned invalid text".into())
}

pub(crate) fn extract_link(provider: Provider, html: &str) -> Result<String, String> {
    let pattern = match provider {
        Provider::FuckingFast => &FUCKING_FAST_LINK,
        Provider::Mediafire => &MEDIAFIRE_LINK,
        _ => return Err("Unsupported page resolver".into()),
    };
    let link = pattern
        .as_ref()
        .ok_or("Download link pattern is invalid")?
        .find(html)
        .ok_or("No supported download link found on the host page")?
        .as_str()
        .replace("&amp;", "&");
    Ok(link)
}

fn safe_filename(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= 255
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
        })
        && !name.ends_with(['.', ' '])
}

pub(crate) fn gofile_files(value: &serde_json::Value) -> Result<Vec<(String, String)>, String> {
    if value.get("canAccess").and_then(|v| v.as_bool()) == Some(false) {
        return Err("GoFile content is private, expired or password protected".into());
    }
    if value.get("type").and_then(|v| v.as_str()) == Some("folder") {
        let children = value
            .get("children")
            .and_then(|v| v.as_object())
            .ok_or("GoFile folder has no files")?;
        let mut files = Vec::new();
        for child in children.values() {
            files.extend(gofile_files(child)?);
            if files.len() > 1000 {
                return Err("GoFile folder has too many files".into());
            }
        }
        return if files.is_empty() {
            Err("GoFile folder is empty".into())
        } else {
            Ok(files)
        };
    }
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("GoFile file has no filename")?;
    let link = value
        .get("link")
        .and_then(|v| v.as_str())
        .ok_or("GoFile content requires a folder lookup")?;
    let url = Url::parse(link).map_err(|_| "Invalid GoFile file URL")?;
    if !safe_filename(name)
        || url.scheme() != "https"
        || !url.host_str().is_some_and(|h| host_is(h, "gofile.io"))
    {
        return Err("GoFile returned an invalid file".into());
    }
    Ok(vec![(name.to_owned(), link.to_owned())])
}

pub(crate) fn is_web_page(response: &Response) -> bool {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            let value = value.to_lowercase();
            value.contains("text/html")
                || value.contains("application/xhtml")
                || value.contains("application/json")
        })
}

/// Archive links come from untrusted sources: redirects are followed by hand and only
/// public addresses are reachable.
pub(crate) fn client_builder() -> ClientBuilder {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(crate::public_address::PublicResolver)
}

pub(crate) fn client() -> Result<Client, String> {
    client_builder()
        .connect_timeout(Duration::from_secs(5))
        .read_timeout(Duration::from_secs(15))
        .user_agent(concat!("Mozilla/5.0 Legio/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())
}

pub(crate) async fn resolve(
    client: &Client,
    uri: &str,
    plain_http: PlainHttp,
) -> Result<ResolvedDownload, ResolveError> {
    let provider = classify(uri)?;
    let mut headers = HeaderMap::new();
    let url = match provider {
        Provider::Direct => uri.to_owned(),
        Provider::PixelDrain => {
            let parsed = Url::parse(uri).map_err(|_| "Invalid PixelDrain URL")?;
            let id = parsed
                .path_segments()
                .and_then(|mut parts| parts.nth(1))
                .filter(|id| !id.is_empty() && id.bytes().all(|c| c.is_ascii_alphanumeric()))
                .ok_or("Invalid PixelDrain file ID")?;
            format!("https://pixeldrain.com/api/file/{id}?download")
        }
        Provider::Mediafire | Provider::FuckingFast => extract_link(
            provider,
            &text(client, uri, HeaderMap::new(), plain_http).await?,
        )?,
        Provider::Rootz => {
            let parsed = Url::parse(uri).map_err(|_| "Invalid Rootz URL")?;
            let id = parsed
                .path_segments()
                .and_then(|mut parts| parts.nth(1))
                .filter(|id| {
                    id.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                })
                .ok_or("Invalid Rootz file ID")?;
            let html = text(client, uri, HeaderMap::new(), plain_http).await?;
            let token = ROOTZ_PAGE_TOKEN
                .as_ref()
                .ok_or("Rootz page token pattern is invalid")?
                .captures(&html)
                .and_then(|c| c.get(1))
                .ok_or("Rootz page token is unavailable")?
                .as_str();
            headers.insert(
                "x-page-token",
                HeaderValue::from_str(token).map_err(|_| "Invalid Rootz page token")?,
            );
            headers.insert(
                "referer",
                HeaderValue::from_str(uri).map_err(|_| "Invalid Rootz page URL")?,
            );
            let json: serde_json::Value = serde_json::from_str(
                &text(
                    client,
                    &format!("https://www.rootz.so/api/files/download-by-short?shortId={id}"),
                    headers.clone(),
                    PlainHttp::Refused,
                )
                .await?,
            )
            .map_err(|_| "Invalid Rootz API response")?;
            let data = json.get("data").ok_or("Rootz file is unavailable")?;
            if json.get("success").and_then(|v| v.as_bool()) != Some(true)
                || data.get("downloadAllowed").and_then(|v| v.as_bool()) == Some(false)
                || data.get("passwordProtected").and_then(|v| v.as_bool()) == Some(true)
            {
                return Err("Rootz download is not available".into());
            }
            let file = data
                .get("fileId")
                .and_then(|v| v.as_str())
                .filter(|id| {
                    !id.is_empty()
                        && id
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                })
                .ok_or("Rootz returned an invalid file ID")?;
            format!("https://www.rootz.so/api/files/proxy-download/{file}")
        }
        Provider::Gofile => return resolve_gofile(client, uri).await,
        Provider::OneFichier => return crate::one_fichier::resolve(client, uri).await,
        Provider::Buzzheavier => return crate::buzzheavier::resolve(client, uri).await,
        Provider::Torrent => return Err("Torrent engine is not available in this build".into()),
        Provider::Unsupported => {
            return Err(
                "This host requires an unsupported account or Hydra backend service".into(),
            );
        }
    };
    Ok(ResolvedDownload {
        provider,
        files: vec![ResolvedFile {
            url,
            name: None,
            headers,
        }],
    })
}

async fn resolve_gofile(client: &Client, uri: &str) -> Result<ResolvedDownload, ResolveError> {
    let parsed = Url::parse(uri).map_err(|_| "Invalid GoFile URL")?;
    let id = parsed
        .path_segments()
        .and_then(|mut parts| parts.nth(1))
        .filter(|id| {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        })
        .ok_or("Invalid GoFile content ID")?;
    let password = parsed
        .query_pairs()
        .find(|(key, _)| key == "password")
        .map(|(_, value)| value.into_owned());
    crate::gofile::resolve(client, id, password.as_deref()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serves one canned response per connection and counts the requests received.
    fn serve(responses: Vec<String>) -> (String, std::thread::JoinHandle<usize>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            let mut served = 0;
            for response in responses {
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(_) if std::time::Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => return served,
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                socket.write_all(response.as_bytes()).unwrap();
                served += 1;
            }
            served
        });
        (base, server)
    }

    const OK: &str = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nPK";

    #[tokio::test]
    async fn plain_http_with_a_hash_is_allowed() {
        let (base, server) = serve(vec![OK.to_owned()]);
        let client = client().unwrap();
        let hash = PlainHttp::for_hash(Some("abc"));
        assert!(get(&client, &base, HeaderMap::new(), hash).await.is_ok());
        assert_eq!(server.join().unwrap(), 1);
    }

    #[tokio::test]
    async fn plain_http_without_a_hash_is_refused_before_any_request() {
        let (base, server) = serve(vec![OK.to_owned()]);
        let client = client().unwrap();
        for hash in [None, Some("")] {
            let policy = PlainHttp::for_hash(hash);
            for result in [
                get(&client, &base, HeaderMap::new(), policy).await,
                head(&client, &base, HeaderMap::new(), policy).await,
            ] {
                assert!(matches!(result, Err(ResolveError::Insecure)));
            }
            assert!(matches!(
                text(&client, &base, HeaderMap::new(), policy).await,
                Err(ResolveError::Insecure)
            ));
        }
        assert_eq!(server.join().unwrap(), 0);
    }

    #[tokio::test]
    async fn plain_http_landing_page_is_refused_without_a_hash() {
        let client = client().unwrap();
        for uri in [
            "http://www.mediafire.com/file/abc/game.zip",
            "http://fuckingfast.co/abc",
            "http://rootz.so/d/abc",
        ] {
            let result = resolve(&client, uri, PlainHttp::Refused).await;
            assert!(matches!(result, Err(ResolveError::Insecure)), "{uri}");
        }
    }

    #[test]
    fn https_needs_no_hash() {
        let url = Url::parse("https://example.org/game.zip").unwrap();
        assert!(PlainHttp::Refused.require_secure(&url).is_ok());
        assert!(PlainHttp::for_hash(Some("abc")) == PlainHttp::Allowed);
    }

    #[test]
    fn https_to_http_redirect_is_always_rejected() {
        let secure = Url::parse("https://example.org/a").unwrap();
        let plain = Url::parse("http://example.org/b").unwrap();
        assert!(reject_downgrade(&secure, &plain).is_err());
        assert!(reject_downgrade(&secure, &secure).is_ok());
        assert!(reject_downgrade(&plain, &plain).is_ok());
    }

    #[test]
    fn classifies_hosts_without_substring_spoofing() {
        assert_eq!(
            classify("https://gofile.io/d/test").unwrap(),
            Provider::Gofile
        );
        assert_eq!(
            classify("https://gofile.io.attacker.test/d/test").unwrap(),
            Provider::Direct
        );
        assert_eq!(
            classify("https://pixeldrain.com/u/test").unwrap(),
            Provider::PixelDrain
        );
        assert_eq!(
            classify("https://download1.mediafire.com/file").unwrap(),
            Provider::Direct
        );
        assert_eq!(
            classify("https://real-debrid.com/d/test").unwrap(),
            Provider::Unsupported
        );
        assert_eq!(
            classify("https://1fichier.com/?abc123").unwrap(),
            Provider::OneFichier
        );
        assert_eq!(
            classify("https://a-15.1fichier.com/c1281161327").unwrap(),
            Provider::Direct
        );
        assert_eq!(
            classify("https://1fichier.com.attacker.test/?abc123").unwrap(),
            Provider::Direct
        );
        for domain in ["buzzheavier.com", "bzzhr.co", "bzzhr.to"] {
            assert_eq!(
                classify(&format!("https://{domain}/nq3n8ffrrryv")).unwrap(),
                Provider::Buzzheavier,
                "{domain}"
            );
        }
        assert_eq!(
            classify("https://ts.buzzheavier.com/d/nq3n8ffrrryv?v=token").unwrap(),
            Provider::Direct
        );
        assert_eq!(
            classify("https://buzzheavier.com.attacker.test/nq3n8ffrrryv").unwrap(),
            Provider::Direct
        );
        assert!(classify("file:///etc/passwd").is_err());
    }

    #[test]
    fn extracts_only_expected_hoster_downloads() {
        assert_eq!(
            extract_link(
                Provider::FuckingFast,
                r#"window.open("https://fuckingfast.co/dl/test")"#
            )
            .unwrap(),
            "https://fuckingfast.co/dl/test"
        );
        assert_eq!(
            extract_link(
                Provider::Mediafire,
                r#"<a href="https://download123.mediafire.com/path/file.zip">Download</a>"#
            )
            .unwrap(),
            "https://download123.mediafire.com/path/file.zip"
        );
        assert!(
            extract_link(
                Provider::Mediafire,
                r#"<a href="https://download123.mediafire.com.attacker.test/file">Download</a>"#
            )
            .is_err()
        );
    }

    #[test]
    fn gofile_file_resolution_requires_safe_names_and_hosts() {
        let url = "https://store1.gofile.io/download/game.zip";
        let file = serde_json::json!({"name": "game.zip", "link": url});
        assert_eq!(
            gofile_files(&file).unwrap(),
            vec![("game.zip".to_owned(), url.to_owned())]
        );
        for (key, value) in [
            ("name", serde_json::json!("../escape")),
            (
                "link",
                serde_json::json!("https://gofile.io.attacker.test/game.zip"),
            ),
            (
                "link",
                serde_json::json!("http://store1.gofile.io/game.zip"),
            ),
            ("canAccess", serde_json::json!(false)),
        ] {
            let mut invalid = file.clone();
            invalid[key] = value;
            assert!(gofile_files(&invalid).is_err(), "{key}: {invalid}");
        }
    }
}
