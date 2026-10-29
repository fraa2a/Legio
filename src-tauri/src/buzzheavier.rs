use crate::download_resolver::{
    Provider, ResolveError, ResolvedDownload, ResolvedFile, network_error,
};
use regex::Regex;
use reqwest::{
    Client, Response, StatusCode, Url,
    header::{ACCEPT, HeaderMap, HeaderValue, REFERER},
};
use std::time::Duration;

const SITE: &str = "https://buzzheavier.com";
const DOMAINS: [&str; 3] = ["buzzheavier.com", "bzzhr.co", "bzzhr.to"];
// Cloudflare challenges a share of the requests from clients that cannot run its script.
const ATTEMPTS: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_secs(1);

/// Id of a file page, `https://<domain>/<id>`. The other domains serve the same files.
pub(crate) fn file_id(url: &Url) -> Option<&str> {
    let host = url.host_str()?;
    let host = host.strip_prefix("www.").unwrap_or(host);
    if !DOMAINS.contains(&host) {
        return None;
    }
    let id = url.path().strip_prefix('/')?.trim_end_matches('/');
    ((8..=32).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .then_some(id)
}

/// Download servers are subdomains such as `ts.buzzheavier.com`; the bare domain is a web page.
fn is_download_url(link: &str) -> bool {
    Url::parse(link).is_ok_and(|url| {
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.host_str().is_some_and(|host| {
                DOMAINS.iter().any(|domain| {
                    host.strip_suffix(domain)
                        .is_some_and(|server| server.len() > 1 && server.ends_with('.'))
                })
            })
    })
}

fn is_challenge(headers: &HeaderMap) -> bool {
    headers
        .get("cf-mitigated")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("challenge"))
}

/// The page links its download button as `/<id>/download?t=<token>`, in markup and in script.
fn download_path(html: &str, id: &str) -> Result<String, String> {
    let pattern = Regex::new(&format!(
        r"/{}/download\?t=[A-Za-z0-9_.~%-]+",
        regex::escape(id)
    ))
    .map_err(|error| error.to_string())?;
    pattern
        .find(&html.replace("\\/", "/"))
        .map(|link| link.as_str().to_owned())
        .ok_or_else(|| "Buzzheavier did not offer a download link".into())
}

/// The download request answers with an `HX-Redirect` header instead of a redirect status.
fn redirect_target(headers: &HeaderMap) -> Result<String, String> {
    headers
        .get("hx-redirect")
        .and_then(|value| value.to_str().ok())
        .filter(|link| is_download_url(link))
        .map(str::to_owned)
        .ok_or_else(|| "Buzzheavier did not provide a download link".into())
}

async fn fetch(client: &Client, url: &str, headers: HeaderMap) -> Result<Response, ResolveError> {
    for attempt in 1..=ATTEMPTS {
        let response = client
            .get(url)
            .headers(headers.clone())
            .send()
            .await
            .map_err(network_error)?;
        if !is_challenge(response.headers()) {
            return match response.status() {
                StatusCode::NOT_FOUND => Err("The Buzzheavier file was not found".into()),
                StatusCode::TOO_MANY_REQUESTS => {
                    Err("Buzzheavier rate limit reached, try again later".into())
                }
                status if status.is_success() => Ok(response),
                status => Err(ResolveError::Http(status)),
            };
        }
        if attempt < ATTEMPTS {
            tokio::time::sleep(RETRY_DELAY).await;
        }
    }
    Err("Buzzheavier is asking for a browser check, try again later".into())
}

pub(crate) async fn resolve(client: &Client, uri: &str) -> Result<ResolvedDownload, ResolveError> {
    let url = Url::parse(uri).map_err(|_| "Invalid Buzzheavier URL")?;
    let id = file_id(&url).ok_or("Invalid Buzzheavier link")?;
    // Without a referrer the website sends the visitor elsewhere.
    let page = format!("{SITE}/{id}");
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("text/html"));
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://buzzheavier.com/"),
    );
    let html = crate::download_resolver::body_text(fetch(client, &page, headers).await?).await?;
    let path = download_path(&html, id)?;
    let mut headers = HeaderMap::new();
    headers.insert("hx-request", HeaderValue::from_static("true"));
    headers.insert(
        REFERER,
        HeaderValue::from_str(&page).map_err(|_| "Invalid Buzzheavier page URL")?,
    );
    let response = fetch(client, &format!("{SITE}{path}"), headers).await?;
    Ok(ResolvedDownload {
        provider: Provider::Buzzheavier,
        files: vec![ResolvedFile {
            url: redirect_target(response.headers())?,
            name: None,
            headers: HeaderMap::new(),
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id_of(uri: &str) -> Option<String> {
        file_id(&Url::parse(uri).unwrap()).map(str::to_owned)
    }

    fn headers(name: &'static str, value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(name, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn recognizes_file_pages_on_all_three_domains() {
        for domain in [
            "buzzheavier.com",
            "bzzhr.co",
            "bzzhr.to",
            "www.buzzheavier.com",
        ] {
            assert_eq!(
                id_of(&format!("https://{domain}/nq3n8ffrrryv")).as_deref(),
                Some("nq3n8ffrrryv"),
                "{domain}"
            );
        }
        assert_eq!(
            id_of("https://bzzhr.to/nq3n8ffrrryv/").as_deref(),
            Some("nq3n8ffrrryv")
        );
        for uri in [
            "https://buzzheavier.com/",
            "https://buzzheavier.com/short",
            "https://buzzheavier.com/nq3n8ffrrryv/download",
            "https://ts.buzzheavier.com/d/nq3n8ffrrryv",
            "https://ts.bzzhr.to/d/nq3n8ffrrryv",
            "https://buzzheavier.com.attacker.test/nq3n8ffrrryv",
            "https://attacker.test/buzzheavier.com/nq3n8ffrrryv",
        ] {
            assert_eq!(id_of(uri), None, "{uri}");
        }
    }

    #[test]
    fn finds_the_download_link_of_the_requested_file_only() {
        // Trimmed from a file page: markup link, mirror link and the script copy.
        let html = r#"<a class="download-btn" hx-get="/nq3n8ffrrryv/download?t=MTc5.CByV-_k4" rel="noopener">Download File</a>
            <a class="copy" onclick="copyDownloadLink('\/nq3n8ffrrryv\/download?t=MTc5.CByV-_k4')">Copy</a>
            <a hx-get="/nq3n8ffrrryv/download?t=MTc5.CByV-_k4&amp;alt=true">Mirror</a>"#;
        assert_eq!(
            download_path(html, "nq3n8ffrrryv").unwrap(),
            "/nq3n8ffrrryv/download?t=MTc5.CByV-_k4"
        );
        let script_only = r#"onclick="copyDownloadLink('\/nq3n8ffrrryv\/download?t=abc.def')""#;
        assert_eq!(
            download_path(script_only, "nq3n8ffrrryv").unwrap(),
            "/nq3n8ffrrryv/download?t=abc.def"
        );
        assert!(download_path(html, "otherfile123").is_err());
        assert!(
            download_path(
                "<p>Whatever lived here has returned to the void.</p>",
                "nq3n8ffrrryv"
            )
            .is_err()
        );
    }

    #[test]
    fn accepts_only_download_servers_from_the_hx_redirect_header() {
        let accepted = [
            "https://ts.buzzheavier.com/d/nq3n8ffrrryv?v=aEKqNb8_bNno",
            "https://ts.bzzhr.to/d/nq3n8ffrrryv?v=abc",
        ];
        for link in accepted {
            assert_eq!(
                redirect_target(&headers("hx-redirect", link)).unwrap(),
                link
            );
        }
        for link in [
            "https://buzzheavier.com/nq3n8ffrrryv",
            "https://steamrip.com",
            "http://ts.buzzheavier.com/d/nq3n8ffrrryv",
            "https://ts.buzzheavier.com.attacker.test/d/nq3n8ffrrryv",
            "https://evilbuzzheavier.com/d/nq3n8ffrrryv",
            "https://user:pass@ts.buzzheavier.com/d/nq3n8ffrrryv",
        ] {
            assert!(
                redirect_target(&headers("hx-redirect", link)).is_err(),
                "{link}"
            );
        }
        assert!(redirect_target(&HeaderMap::new()).is_err());
    }

    #[test]
    fn detects_the_cloudflare_challenge() {
        assert!(is_challenge(&headers("cf-mitigated", "challenge")));
        assert!(!is_challenge(&headers("cf-mitigated", "other")));
        assert!(!is_challenge(&HeaderMap::new()));
    }
}
