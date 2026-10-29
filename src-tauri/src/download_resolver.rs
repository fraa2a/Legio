use reqwest::{
    Client, ClientBuilder, Method, Response, Url,
    header::{CONTENT_TYPE, HeaderMap, HeaderValue, LOCATION},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Provider {
    Direct,
    Gofile,
    PixelDrain,
    Mediafire,
    FuckingFast,
    Rootz,
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
        } else {
            Provider::Direct
        },
    )
}

pub(crate) async fn get(
    client: &Client,
    uri: &str,
    headers: HeaderMap,
) -> Result<Response, String> {
    follow_redirects(client, Method::GET, uri, headers).await
}

pub(crate) async fn head(
    client: &Client,
    uri: &str,
    headers: HeaderMap,
) -> Result<Response, String> {
    follow_redirects(client, Method::HEAD, uri, headers).await
}

async fn follow_redirects(
    client: &Client,
    method: Method,
    uri: &str,
    mut headers: HeaderMap,
) -> Result<Response, String> {
    let mut url = Url::parse(uri).map_err(|_| "Invalid resolved URL")?;
    for hop in 0..=5 {
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("Invalid resolved URL".into());
        }
        // IP literal hosts are never passed to the DNS resolver.
        if !crate::public_address::is_public_host(&url) {
            return Err("Download host is not a public internet address".into());
        }
        let response = client
            .request(method.clone(), url.clone())
            .headers(headers.clone())
            .send()
            .await
            .map_err(|error| format!("Network: {}", error.without_url()))?;
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
            if url.scheme() == "https" && next.scheme() != "https" {
                return Err("Download redirected to an insecure connection".into());
            }
            if url.origin() != next.origin() {
                headers.remove("authorization");
                headers.remove("cookie");
            }
            url = next;
            continue;
        }
        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status().as_u16()));
        }
        return Ok(response);
    }
    Err("Download redirect limit reached".into())
}

async fn text(client: &Client, url: &str, headers: HeaderMap) -> Result<String, String> {
    let mut response = get(client, url, headers).await?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Network: {}", error.without_url()))?
    {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("Host response exceeds the size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| "Host returned invalid text".into())
}

pub(crate) fn extract_link(provider: Provider, html: &str) -> Result<String, String> {
    let pattern = match provider {
        Provider::FuckingFast => r#"https://fuckingfast\.co/dl/[^\s"'<>\\]+"#,
        Provider::Mediafire => r#"https?://download[0-9]+\.mediafire\.com/[^\s"'<>\\]+"#,
        _ => return Err("Unsupported page resolver".into()),
    };
    let regex = regex::Regex::new(pattern).map_err(|error| error.to_string())?;
    let link = regex
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
        .user_agent("Mozilla/5.0 Legio/0.2")
        .build()
        .map_err(|error| error.to_string())
}

pub(crate) async fn resolve(client: &Client, uri: &str) -> Result<ResolvedDownload, String> {
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
        Provider::Mediafire | Provider::FuckingFast => {
            extract_link(provider, &text(client, uri, HeaderMap::new()).await?)?
        }
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
            let html = text(client, uri, HeaderMap::new()).await?;
            let regex = regex::Regex::new(r#"pageToken[\\]*"\s*:\s*[\\]*"([^"\\]+)"#)
                .map_err(|e| e.to_string())?;
            let token = regex
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

async fn resolve_gofile(client: &Client, uri: &str) -> Result<ResolvedDownload, String> {
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
