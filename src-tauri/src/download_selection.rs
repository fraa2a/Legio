use crate::archive_parts::{PartScheme, find_part_set, looks_like_part};
use crate::download_resolver::{self, PlainHttp, Provider, ResolveError, ResolvedFile};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Saved plans without `format` predate multipart downloads and hold no parts.
const PLAN_FORMAT: u32 = 2;

const fn first_plan_format() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MirrorPlan {
    #[serde(default = "first_plan_format")]
    pub format: u32,
    pub uris: Vec<String>,
    pub index: usize,
    pub failures: u8,
    pub expected_size: Option<u64>,
    /// Parts of the multipart archive that `parts.uri` resolved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<PartLayout>,
}

/// One file of a multipart archive with its exact size on the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoredPart {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PartLayout {
    pub uri: String,
    pub scheme: PartScheme,
    pub parts: Vec<StoredPart>,
}

impl PartLayout {
    pub(crate) fn total(&self) -> u64 {
        self.parts
            .iter()
            .fold(0u64, |total, part| total.saturating_add(part.size))
    }

    /// One-based number of the part that holds byte `downloaded`.
    pub(crate) fn current(&self, downloaded: u64) -> usize {
        let mut end = 0u64;
        for (index, part) in self.parts.iter().enumerate() {
            end = end.saturating_add(part.size);
            if downloaded < end {
                return index + 1;
            }
        }
        self.parts.len()
    }

    /// Same files in the same order, so completed parts stay valid.
    pub(crate) fn same_files(&self, other: &Self) -> bool {
        self.scheme == other.scheme && self.parts == other.parts
    }
}

impl MirrorPlan {
    pub(crate) fn new(uris: Vec<String>, expected_size: Option<u64>) -> Self {
        Self {
            format: PLAN_FORMAT,
            uris,
            index: 0,
            failures: 0,
            expected_size,
            parts: None,
        }
    }

    /// Reads a saved plan and migrates older formats.
    pub(crate) fn load(stored: &str) -> Result<Self, String> {
        let plan: Self = serde_json::from_str(stored)
            .map_err(|error| format!("Invalid saved download links: {error}"))?;
        match plan.format {
            1 => Ok(Self {
                format: PLAN_FORMAT,
                parts: None,
                ..plan
            }),
            PLAN_FORMAT => Ok(plan),
            other => Err(format!("Saved download links use unknown format {other}")),
        }
    }

    pub(crate) fn save(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|error| error.to_string())
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

/// Pause before retrying a link whose host asked to wait. Without a stated wait
/// the host gets a short default, and long waits are cut so the job stays responsive.
pub(crate) fn rate_limit_delay(retry_after: Option<Duration>) -> Duration {
    retry_after
        .unwrap_or(Duration::from_secs(30))
        .clamp(Duration::from_secs(1), Duration::from_secs(300))
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

pub(crate) async fn inspect(
    client: &reqwest::Client,
    uri: &str,
    plain_http: PlainHttp,
) -> Result<u64, ResolveError> {
    let timeout = if download_resolver::classify(uri)? == Provider::Gofile {
        Duration::from_secs(125)
    } else {
        Duration::from_secs(20)
    };
    tokio::time::timeout(timeout, inspect_file(client, uri, plain_http))
        .await
        .map_err(|_| ResolveError::Network("Download host lookup timed out".to_owned()))?
}

/// What one download link holds once resolved.
pub(crate) enum Layout {
    Single(ResolvedFile),
    /// Every volume of one multipart archive, in volume order.
    Parts(PartScheme, Vec<ResolvedFile>),
}

/// The host's name for a file, or the last segment of its URL.
pub(crate) fn file_name(file: &ResolvedFile) -> String {
    file.name.clone().unwrap_or_else(|| {
        reqwest::Url::parse(&file.url)
            .ok()
            .and_then(|url| {
                url.path_segments()
                    .and_then(|mut segments| segments.next_back().map(str::to_owned))
            })
            .unwrap_or_default()
    })
}

fn is_archive_name(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["zip", "7z", "rar"]
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

/// A lone file must not be a volume of a larger archive. Several files must hold one
/// complete multipart set; without any part the first file is the download, as before.
pub(crate) fn layout(mut files: Vec<ResolvedFile>) -> Result<Layout, String> {
    let names: Vec<String> = files.iter().map(file_name).collect();
    match files.len() {
        0 => return Err("Download contains no files".into()),
        1 if looks_like_part(&names[0]) => {
            return Err(format!(
                "The link provides only {} of a multipart archive; every part must be in the same link",
                names[0]
            ));
        }
        1 => return Ok(Layout::Single(files.remove(0))),
        _ => {}
    }
    let references: Vec<&str> = names.iter().map(String::as_str).collect();
    let Some(set) = find_part_set(&references).map_err(|error| error.to_string())? else {
        return Ok(Layout::Single(files.remove(0)));
    };
    if names
        .iter()
        .enumerate()
        .any(|(index, name)| !set.order.contains(&index) && is_archive_name(name))
    {
        return Err(
            "The link holds a multipart archive and other archives, so the one to install is unclear"
                .into(),
        );
    }
    let mut slots: Vec<Option<ResolvedFile>> = files.into_iter().map(Some).collect();
    let parts = set
        .order
        .iter()
        .map(|index| {
            slots[*index]
                .take()
                .ok_or("The multipart archive repeats a file")
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Layout::Parts(set.scheme, parts))
}

fn is_supported_magic(magic: &[u8]) -> bool {
    magic.starts_with(b"PK\x03\x04")
        || magic.starts_with(b"PK\x05\x06")
        || magic.starts_with(b"7z\xbc\xaf\x27\x1c")
        || magic.starts_with(b"Rar!\x1a\x07\x00")
        || magic.starts_with(b"Rar!\x1a\x07\x01\x00")
}

fn matches_scheme(scheme: PartScheme, magic: &[u8]) -> bool {
    match scheme {
        PartScheme::RarVolumes | PartScheme::RarLegacyVolumes => magic.starts_with(b"Rar!"),
        PartScheme::SevenZipSplit => magic.starts_with(b"7z\xbc\xaf\x27\x1c"),
        PartScheme::ZipSplit => magic.starts_with(b"PK"),
    }
}

async fn inspect_file(
    client: &reqwest::Client,
    uri: &str,
    plain_http: PlainHttp,
) -> Result<u64, ResolveError> {
    if download_resolver::classify(uri)? == Provider::OneFichier {
        return crate::one_fichier::inspect(client, uri).await;
    }
    let resolved = download_resolver::resolve(client, uri, plain_http).await?;
    match layout(resolved.files)? {
        Layout::Single(file) => {
            let (size, magic) = probe(client, &file, plain_http, 8).await?;
            if !is_supported_magic(&magic) {
                return Err("The link does not provide a supported ZIP, 7z or RAR archive".into());
            }
            Ok(size)
        }
        Layout::Parts(scheme, parts) => {
            let sizes = probe_parts(client, scheme, &parts, plain_http).await?;
            Ok(sizes.iter().map(|part| part.size).sum())
        }
    }
}

/// Exact sizes of every part. The first part must start like its format.
pub(crate) async fn probe_parts(
    client: &reqwest::Client,
    scheme: PartScheme,
    parts: &[ResolvedFile],
    plain_http: PlainHttp,
) -> Result<Vec<StoredPart>, ResolveError> {
    let mut sizes = Vec::with_capacity(parts.len());
    for (index, part) in parts.iter().enumerate() {
        let minimum = if index == 0 { 8 } else { 1 };
        let (size, magic) = probe(client, part, plain_http, minimum).await?;
        if index == 0 && !matches_scheme(scheme, &magic) {
            return Err(
                "The first archive part does not contain the format its name announces".into(),
            );
        }
        sizes.push(StoredPart {
            name: file_name(part),
            size,
        });
    }
    Ok(sizes)
}

/// Exact size and first bytes of a file, from an eight-byte range request.
async fn probe(
    client: &reqwest::Client,
    file: &ResolvedFile,
    plain_http: PlainHttp,
    minimum: u64,
) -> Result<(u64, Vec<u8>), ResolveError> {
    let mut headers = file.headers.clone();
    headers.insert(
        "range",
        reqwest::header::HeaderValue::from_static("bytes=0-7"),
    );
    let mut response = download_resolver::get(client, &file.url, headers, plain_http).await?;
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
    .filter(|size| *size >= minimum)
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
    Ok((size, magic))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_delay_respects_the_host_within_bounds() {
        assert_eq!(rate_limit_delay(None), Duration::from_secs(30));
        assert_eq!(
            rate_limit_delay(Some(Duration::from_secs(90))),
            Duration::from_secs(90)
        );
        assert_eq!(
            rate_limit_delay(Some(Duration::ZERO)),
            Duration::from_secs(1)
        );
        assert_eq!(
            rate_limit_delay(Some(Duration::from_secs(3600))),
            Duration::from_secs(300)
        );
    }

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
        let result = inspect(
            &crate::download_resolver::client().unwrap(),
            &uri,
            PlainHttp::Allowed,
        )
        .await;
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

    #[tokio::test]
    async fn a_link_to_a_single_part_is_rejected_by_name() {
        let error = inspect(
            &crate::download_resolver::client().unwrap(),
            "https://example.org/files/Game.part1.rar",
            PlainHttp::Refused,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("only Game.part1.rar"), "{error}");
    }

    fn files(names: &[&str]) -> Vec<ResolvedFile> {
        names
            .iter()
            .map(|name| ResolvedFile {
                url: format!("https://example.org/{name}"),
                name: Some((*name).to_owned()),
                headers: reqwest::header::HeaderMap::new(),
            })
            .collect()
    }

    fn layout_names(names: &[&str]) -> Result<(Option<PartScheme>, Vec<String>), String> {
        Ok(match layout(files(names))? {
            Layout::Single(file) => (None, vec![file_name(&file)]),
            Layout::Parts(scheme, parts) => (Some(scheme), parts.iter().map(file_name).collect()),
        })
    }

    #[test]
    fn layout_orders_a_complete_set_and_ignores_notes() {
        assert_eq!(
            layout_names(&["Game.part2.rar", "readme.txt", "Game.part1.rar"]).unwrap(),
            (
                Some(PartScheme::RarVolumes),
                vec!["Game.part1.rar".into(), "Game.part2.rar".into()]
            )
        );
    }

    #[test]
    fn layout_keeps_the_first_file_without_parts() {
        assert_eq!(
            layout_names(&["game.zip", "readme.txt"]).unwrap(),
            (None, vec!["game.zip".into()])
        );
        assert_eq!(
            layout_names(&["game.7z"]).unwrap(),
            (None, vec!["game.7z".into()])
        );
    }

    #[test]
    fn layout_rejects_incomplete_ambiguous_and_lone_parts() {
        assert!(
            layout_names(&["g.part1.rar", "g.part3.rar"])
                .unwrap_err()
                .contains("g.part2.rar is missing")
        );
        assert!(
            layout_names(&["g.part1.rar", "g.part2.rar", "crack.zip"])
                .unwrap_err()
                .contains("unclear")
        );
        assert!(
            layout_names(&["g.7z.001"])
                .unwrap_err()
                .contains("only g.7z.001")
        );
        assert!(
            layout_names(&["g.z01", "g.zip"])
                .unwrap_err()
                .contains("not supported")
        );
        assert!(layout_names(&[]).is_err());
    }

    #[test]
    fn file_names_fall_back_to_the_last_url_segment() {
        let file = ResolvedFile {
            url: "https://example.org/a/b/Game.7z.001?token=1".into(),
            name: None,
            headers: reqwest::header::HeaderMap::new(),
        };
        assert_eq!(file_name(&file), "Game.7z.001");
    }

    #[test]
    fn plans_saved_before_multipart_support_still_load() {
        // Exactly what the previous version stored in `downloads.mirror_plan`.
        let stored = r#"{"uris":["https://gofile.io/d/a","https://example.test/b.zip"],"index":1,"failures":2,"expected_size":10}"#;
        let plan = MirrorPlan::load(stored).unwrap();
        assert_eq!(plan.format, PLAN_FORMAT);
        assert_eq!(plan.current(), Some("https://example.test/b.zip"));
        assert_eq!(plan.failures, 2);
        assert_eq!(plan.expected_size, Some(10));
        assert_eq!(plan.parts, None);
        let saved: serde_json::Value = serde_json::from_str(&plan.save().unwrap()).unwrap();
        assert_eq!(saved["format"], 2);
        assert!(
            MirrorPlan::load(
                r#"{"format":9,"uris":[],"index":0,"failures":0,"expected_size":null}"#
            )
            .is_err()
        );
    }

    #[test]
    fn part_layout_reports_the_current_part() {
        let layout = PartLayout {
            uri: "https://gofile.io/d/a".into(),
            scheme: PartScheme::RarVolumes,
            parts: [("a.part1.rar", 10), ("a.part2.rar", 10), ("a.part3.rar", 5)]
                .map(|(name, size)| StoredPart {
                    name: name.into(),
                    size,
                })
                .to_vec(),
        };
        assert_eq!(layout.total(), 25);
        assert_eq!(layout.current(0), 1);
        assert_eq!(layout.current(9), 1);
        assert_eq!(layout.current(10), 2);
        assert_eq!(layout.current(24), 3);
        assert_eq!(layout.current(25), 3);
        let mut plan = MirrorPlan::new(vec![layout.uri.clone()], Some(25));
        plan.parts = Some(layout.clone());
        assert_eq!(
            MirrorPlan::load(&plan.save().unwrap()).unwrap().parts,
            Some(layout)
        );
    }

    #[tokio::test]
    async fn probing_parts_reads_each_size_and_checks_the_first_format() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for body in [b"Rar!\x1a\x07\x01\x00rest".as_slice(), b"abc".as_slice()] {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                assert!(
                    String::from_utf8(request)
                        .unwrap()
                        .to_lowercase()
                        .contains("range: bytes=0-7")
                );
                let total = body.len() + 100;
                let shown = &body[..body.len().min(8)];
                socket.write_all(format!("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-{}/{total}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", shown.len() - 1, shown.len()).as_bytes()).unwrap();
                socket.write_all(shown).unwrap();
            }
        });
        let parts: Vec<ResolvedFile> = ["g.part1.rar", "g.part2.rar"]
            .iter()
            .map(|name| ResolvedFile {
                url: format!("{base}/{name}"),
                name: Some((*name).to_owned()),
                headers: reqwest::header::HeaderMap::new(),
            })
            .collect();
        let client = crate::download_resolver::client().unwrap();
        let sizes = probe_parts(&client, PartScheme::RarVolumes, &parts, PlainHttp::Allowed)
            .await
            .unwrap();
        server.join().unwrap();
        assert_eq!(
            sizes,
            vec![
                StoredPart {
                    name: "g.part1.rar".into(),
                    size: 112
                },
                StoredPart {
                    name: "g.part2.rar".into(),
                    size: 103
                }
            ]
        );
        assert!(!matches_scheme(
            PartScheme::SevenZipSplit,
            b"Rar!\x1a\x07\x01\x00"
        ));
    }
}
