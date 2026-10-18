use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub(crate) const MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportDiagnostics {
    pub rejected: usize,
    pub unsupported: usize,
    pub unresolved: usize,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HydraDownload {
    pub title: String,
    pub uris: Vec<String>,
    #[serde(default)]
    pub upload_date: Option<String>,
    #[serde(default)]
    pub file_size: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct HydraSource {
    pub name: String,
    pub downloads: Vec<HydraDownload>,
    #[serde(default)]
    pub diagnostics: ImportDiagnostics,
}

pub(crate) fn valid_uri(uri: &str) -> bool {
    if uri.len() > 8192 || uri.chars().any(char::is_control) {
        return false;
    }
    let Ok(url) = reqwest::Url::parse(uri) else {
        return false;
    };
    match url.scheme() {
        "http" | "https" => {
            url.host_str().is_some() && url.username().is_empty() && url.password().is_none()
        }
        "magnet" => url.query_pairs().any(|(key, value)| {
            key == "xt" && (value.starts_with("urn:btih:") || value.starts_with("urn:btmh:"))
        }),
        _ => false,
    }
}

pub(crate) fn paid_or_backend_only(uri: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(uri) else {
        return false;
    };
    url.host_str().is_some_and(|host| {
        [
            "real-debrid.com",
            "alldebrid.com",
            "premiumize.me",
            "torbox.app",
            "vikingfile.com",
            "datanodes.to",
        ]
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
    })
}

pub(crate) fn parse(bytes: &[u8]) -> Result<HydraSource, String> {
    if bytes.len() > MAX_BYTES {
        return Err("Hydra source exceeds the 32 MiB limit".into());
    }
    let document: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "Invalid source JSON")?;
    if document.get("schemaVersion").is_some() {
        return Err("Native manifests must use the Legio parser".into());
    }
    let name = document
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|name| {
            !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
        })
        .ok_or("Hydra source requires a nonempty name of at most 128 bytes")?
        .trim()
        .to_owned();
    let records = document
        .get("downloads")
        .and_then(|v| v.as_array())
        .ok_or("Hydra source requires a downloads array")?;
    if records.len() > 100_000 {
        return Err("Hydra source exceeds 100000 records".into());
    }
    let mut source = HydraSource {
        name,
        downloads: Vec::new(),
        diagnostics: ImportDiagnostics::default(),
    };
    let mut seen = HashSet::new();
    for (index, record) in records.iter().enumerate() {
        let result = serde_json::from_value::<HydraDownload>(record.clone());
        let Ok(mut entry) = result else {
            source.diagnostics.rejected += 1;
            source
                .diagnostics
                .reasons
                .push(format!("Record {index}: invalid title or URI list"));
            continue;
        };
        entry.title = entry.title.split_whitespace().collect::<Vec<_>>().join(" ");
        let valid = !entry.title.trim().is_empty()
            && entry.title.len() <= 512
            && !entry.title.chars().any(char::is_control)
            && !entry.uris.is_empty()
            && entry.uris.len() <= 256;
        if !valid {
            source.diagnostics.rejected += 1;
            source.diagnostics.reasons.push(format!(
                "Record {index}: invalid title or unsupported URI scheme"
            ));
            continue;
        }
        if !entry.uris.iter().any(|uri| valid_uri(uri)) {
            source.diagnostics.rejected += 1;
            continue;
        }
        let mut links = HashSet::new();
        let original_count = entry.uris.len();
        entry
            .uris
            .retain(|uri| crate::download_selection::supported(uri) && links.insert(uri.clone()));
        source.diagnostics.unsupported += original_count - entry.uris.len();
        if entry.uris.is_empty() {
            continue;
        }
        let mut stable_uris = entry.uris.clone();
        stable_uris.sort();
        if seen.insert((entry.title.clone(), stable_uris)) {
            source.downloads.push(entry);
        }
    }
    source.diagnostics.reasons.truncate(200);
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_release_page_whitespace_but_rejects_control_characters() {
        let source = parse(br#"{"name":"SteamGG","downloads":[{"title":"Scritchy Scratchy Free Download (Build\t22395563)","uris":["https://example.test/game.zip"]},{"title":"Bad\u0007Title","uris":["https://example.test/game.zip"]}]}"#).unwrap();
        assert_eq!(source.downloads.len(), 1);
        assert_eq!(
            source.downloads[0].title,
            "Scritchy Scratchy Free Download (Build 22395563)"
        );
        assert_eq!(source.diagnostics.rejected, 1);
    }

    #[test]
    fn parses_named_source_and_keeps_mirrors_without_claiming_trust() {
        let source = parse(br#"{"name":"SteamGG","downloads":[{"title":"Portal v1.0 [SteamGG]","uris":["https://gofile.io/d/example","https://pixeldrain.com/u/example"],"uploadDate":"2026-10-17T00:00:00Z","fileSize":"1.2 GB","verified":true}]}"#).unwrap();
        assert_eq!(source.name, "SteamGG");
        assert_eq!(source.downloads.len(), 1);
        assert_eq!(source.downloads[0].uris.len(), 2);
        assert_eq!(source.downloads[0].file_size.as_deref(), Some("1.2 GB"));
    }

    #[test]
    fn partial_import_reports_invalid_records_without_discarding_valid_ones() {
        let source = parse(br#"{"name":"Public","downloads":[{"title":"Portal","uris":["https://example.test/file.zip"]},{"title":"Invalid","uris":["file:///etc/passwd"]},{"title":"Paid","uris":["https://real-debrid.com/d/test"]}]}"#).unwrap();
        assert_eq!(source.downloads.len(), 1);
        assert_eq!(source.diagnostics.rejected, 1);
        assert_eq!(source.diagnostics.unsupported, 1);
    }

    #[test]
    fn rejects_bad_shape_and_duplicate_or_unbounded_content() {
        assert!(parse(br#"{"name":"","downloads":[]}"#).is_err());
        assert!(parse(br#"{"schemaVersion":1,"downloads":[]}"#).is_err());
        assert!(parse(br#"{"name":"Public","downloads":"wrong"}"#).is_err());
    }

    #[test]
    fn unsupported_only_entries_are_omitted_and_valid_mirrors_keep_json_order() {
        let source = parse(br#"{"name":"SteamGG","downloads":[{"title":"Hidden","uris":["https://datanodes.to/a","https://vikingfile.com/f/a","https://filekeeper.net/a/game.zip","https://akirabox.com/a/file"]},{"title":"Visible","uris":["magnet:?xt=urn:btih:abc","https://example.test/first.zip","https://gofile.io/d/second","file:///wrong"]}]}"#).unwrap();
        assert_eq!(source.downloads.len(), 1);
        assert_eq!(source.downloads[0].title, "Visible");
        assert_eq!(
            source.downloads[0].uris,
            vec![
                "https://example.test/first.zip",
                "https://gofile.io/d/second"
            ]
        );
    }
}
