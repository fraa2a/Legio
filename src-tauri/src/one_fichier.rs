use crate::download_resolver::{
    Provider, ResolveError, ResolvedDownload, ResolvedFile, network_error,
};
use regex::Regex;
use reqwest::{
    Client, StatusCode, Url,
    header::{CONTENT_TYPE, HeaderMap, HeaderValue, REFERER},
};
use std::path::Path;

const FILE_PAGE: &str = "https://1fichier.com/";
const LINK_CHECKER: &str = "https://1fichier.com/check_links.pl";

/// Id of a file page, `https://1fichier.com/?<id>` with optional `&` parameters.
pub(crate) fn file_id(url: &Url) -> Option<&str> {
    if !matches!(url.host_str()?, "1fichier.com" | "www.1fichier.com") || url.path() != "/" {
        return None;
    }
    let id = url.query()?.split('&').next()?;
    (!id.is_empty() && id.len() <= 64 && id.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .then_some(id)
}

fn page_url(id: &str) -> String {
    format!("{FILE_PAGE}?{id}")
}

struct FileInfo {
    name: String,
    size: u64,
}

/// The link checker answers `<url>;<name>;<size>` per link, or `<url>;;;NOT FOUND`.
fn parse_file_info(report: &str, id: &str) -> Result<FileInfo, String> {
    let prefix = format!("{};", page_url(id));
    let line = report
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .ok_or("1fichier returned no information for the link")?;
    let (name, size) = line
        .rsplit_once(';')
        .ok_or("1fichier returned an invalid file report")?;
    let Ok(size) = size.trim().parse::<u64>() else {
        return Err(if size.trim() == "NOT FOUND" {
            "The 1fichier file was not found".into()
        } else {
            "1fichier could not provide the file information".into()
        });
    };
    if name.is_empty() {
        return Err("1fichier returned an invalid file report".into());
    }
    Ok(FileInfo {
        name: name.to_owned(),
        size,
    })
}

fn has_archive_extension(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["zip", "7z", "rar"]
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

/// Reads size and name from the public link checker. Requesting the download
/// form would use up the single free download that guests get per hour.
pub(crate) async fn inspect(client: &Client, uri: &str) -> Result<u64, ResolveError> {
    let url = Url::parse(uri).map_err(|_| "Invalid 1fichier URL")?;
    let id = file_id(&url).ok_or("Invalid 1fichier link")?;
    let mut checker = Url::parse(LINK_CHECKER).map_err(|error| error.to_string())?;
    checker
        .query_pairs_mut()
        .append_pair("links[]", &page_url(id));
    let report = crate::download_resolver::text(client, checker.as_str(), HeaderMap::new()).await?;
    let info = parse_file_info(&report, id)?;
    if crate::archive_install::is_multipart_name(Path::new(&info.name)) {
        return Err("Multipart archive files are not supported".into());
    }
    if !has_archive_extension(&info.name) {
        return Err("The link does not provide a supported ZIP, 7z or RAR archive".into());
    }
    if info.size < 8 {
        return Err("The download host did not provide an exact archive size".into());
    }
    Ok(info.size)
}

fn is_download_host(link: &str) -> bool {
    Url::parse(link).is_ok_and(|url| {
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url
                .host_str()
                .is_some_and(|host| host.ends_with(".1fichier.com"))
    })
}

/// The form answers with a one-time link on a download server, or with a notice
/// when the guest has to wait for the next free download.
fn parse_download_page(html: &str) -> Result<String, String> {
    let anchor = Regex::new(r"(?is)<a\s[^>]*>").map_err(|error| error.to_string())?;
    let href = Regex::new(r#"(?i)\bhref="([^"]+)""#).map_err(|error| error.to_string())?;
    let class = Regex::new(r#"(?i)\bclass="([^"]*)""#).map_err(|error| error.to_string())?;
    for tag in anchor.find_iter(html).map(|tag| tag.as_str()) {
        let is_offer = class
            .captures(tag)
            .is_some_and(|found| found[1].split_whitespace().any(|name| name == "ok"));
        if is_offer
            && let Some(link) = href
                .captures(tag)
                .map(|found| found[1].replace("&amp;", "&"))
            && is_download_host(&link)
        {
            return Ok(link);
        }
    }
    Err(refusal(html)?)
}

fn refusal(html: &str) -> Result<String, String> {
    let notice = Regex::new(r"(?is)notice-alert.*?</div>").map_err(|error| error.to_string())?;
    let tags = Regex::new(r"<[^>]*>").map_err(|error| error.to_string())?;
    let wait = Regex::new(r"(?i)\bwait\s+([0-9]{1,4})\s+(second|minute|hour)")
        .map_err(|error| error.to_string())?;
    let Some(notice) = notice.find(html) else {
        return Ok("1fichier did not offer a download link".into());
    };
    let text = tags.replace_all(notice.as_str(), " ");
    Ok(match wait.captures(&text) {
        Some(wait) => {
            let (count, unit) = (&wait[1], wait[2].to_lowercase());
            let plural = if count == "1" { "" } else { "s" };
            format!("1fichier requires a wait of {count} {unit}{plural} between free downloads")
        }
        None => "1fichier is limiting free downloads right now, try again later".into(),
    })
}

/// Submits the free download form. The returned link works once, so the caller
/// must request it immediately before transferring.
pub(crate) async fn resolve(client: &Client, uri: &str) -> Result<ResolvedDownload, ResolveError> {
    let url = Url::parse(uri).map_err(|_| "Invalid 1fichier URL")?;
    let id = file_id(&url).ok_or("Invalid 1fichier link")?;
    let page = page_url(id);
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    headers.insert(
        REFERER,
        HeaderValue::from_str(&page).map_err(|_| "Invalid 1fichier page URL")?,
    );
    let response = client
        .post(page)
        .headers(headers)
        .send()
        .await
        .map_err(network_error)?;
    match response.status() {
        StatusCode::NOT_FOUND => return Err("The 1fichier file was not found".into()),
        status if !status.is_success() => return Err(ResolveError::Http(status)),
        _ => {}
    }
    let html = crate::download_resolver::body_text(response).await?;
    Ok(ResolvedDownload {
        provider: Provider::OneFichier,
        files: vec![ResolvedFile {
            url: parse_download_page(&html)?,
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

    #[test]
    fn recognizes_only_file_pages_on_the_main_domain() {
        assert_eq!(
            id_of("https://1fichier.com/?a0x6p6serz33hs8r32ix").as_deref(),
            Some("a0x6p6serz33hs8r32ix")
        );
        assert_eq!(
            id_of("https://www.1fichier.com/?abc123&af=77").as_deref(),
            Some("abc123")
        );
        for uri in [
            "https://1fichier.com/",
            "https://1fichier.com/?",
            "https://1fichier.com/?id=abc",
            "https://1fichier.com/dir/abc123",
            "https://a-15.1fichier.com/c1281161327",
            "https://1fichier.com.attacker.test/?abc123",
            "https://attacker.test/1fichier.com/?abc123",
        ] {
            assert_eq!(id_of(uri), None, "{uri}");
        }
    }

    #[test]
    fn reads_size_and_name_from_the_link_checker() {
        let report = "https://1fichier.com/?abc123;My; Game.part.rar;17754376134\n";
        let info = parse_file_info(report, "abc123").unwrap();
        assert_eq!(info.name, "My; Game.part.rar");
        assert_eq!(info.size, 17_754_376_134);
        assert_eq!(
            parse_file_info("https://1fichier.com/?abc123;;;NOT FOUND\n", "abc123").err(),
            Some("The 1fichier file was not found".to_owned())
        );
        assert!(parse_file_info("https://1fichier.com/?abc123;;;BAD LINK", "abc123").is_err());
        assert!(parse_file_info("https://1fichier.com/?other;a.zip;10", "abc123").is_err());
        assert!(parse_file_info("https://1fichier.com/?abc123;;10", "abc123").is_err());
    }

    #[test]
    fn accepts_only_zip_7z_and_rar_names() {
        assert!(has_archive_extension("Game.rar"));
        assert!(has_archive_extension("Game.ZIP"));
        assert!(has_archive_extension("Game.7z"));
        assert!(!has_archive_extension("Game.exe"));
        assert!(!has_archive_extension("zip"));
    }

    #[test]
    fn extracts_the_one_time_link_from_the_form_answer() {
        let html = r#"<a href="https://1fichier.com/cgu.html">Terms</a>
            <div class="alc"><a href="https://a-15.1fichier.com/c1281161327" style="display:inline-block;float:none" class="ok btn-general btn-orange">Start your download</a></div>"#;
        assert_eq!(
            parse_download_page(html).unwrap(),
            "https://a-15.1fichier.com/c1281161327"
        );
        for html in [
            r#"<a href="https://a-15.1fichier.com/c1" class="btn-general">Other</a>"#,
            r#"<a href="https://evil.test/c1" class="ok">Start</a>"#,
            r#"<a href="https://1fichier.com.evil.test/c1" class="ok">Start</a>"#,
            r#"<a href="http://a-15.1fichier.com/c1" class="ok">Start</a>"#,
        ] {
            assert_eq!(
                parse_download_page(html).unwrap_err(),
                "1fichier did not offer a download link",
                "{html}"
            );
        }
    }

    #[test]
    fn reports_the_wait_between_free_downloads() {
        let html = r#"<form id="f1"><div class="notice notice-alert">
            <b>Warning!</b>
        <br>Without subscription, you must wait between downloads.
        <br>You must wait 60 minutes...
    </div><button id="dlw" disabled="disabled">Free download</button></form>"#;
        assert_eq!(
            parse_download_page(html).unwrap_err(),
            "1fichier requires a wait of 60 minutes between free downloads"
        );
        let single = html.replace("60 minutes", "1 minute");
        assert_eq!(
            parse_download_page(&single).unwrap_err(),
            "1fichier requires a wait of 1 minute between free downloads"
        );
        let unknown = html.replace("You must wait 60 minutes...", "Try later.");
        assert_eq!(
            parse_download_page(&unknown).unwrap_err(),
            "1fichier is limiting free downloads right now, try again later"
        );
    }
}
