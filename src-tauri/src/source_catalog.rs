use crate::{
    hydra_source::{self, HydraSource, ImportDiagnostics},
    legio_source::{self, Download, Manifest, Release, SourceEntry},
    source_matching::{MatchResult, normalized_title},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceFormat {
    #[default]
    Legio,
    Hydra,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceCatalog {
    pub format: SourceFormat,
    pub name: Option<String>,
    pub manifest: Manifest,
    pub candidates: BTreeMap<String, Vec<String>>,
    pub diagnostics: ImportDiagnostics,
    pub raw_hydra: Option<HydraSource>,
    pub matches: HashMap<String, u32>,
    #[serde(default)]
    pub validated_sizes: BTreeMap<String, u64>,
}

impl SourceCatalog {
    pub(crate) fn native(mut manifest: Manifest) -> Self {
        manifest
            .verified
            .retain(|entry| crate::download_selection::supported(&entry.download.url));
        manifest
            .unverified
            .retain(|entry| crate::download_selection::supported(&entry.download.url));
        Self {
            format: SourceFormat::Legio,
            name: None,
            manifest,
            candidates: BTreeMap::new(),
            diagnostics: ImportDiagnostics::default(),
            raw_hydra: None,
            matches: HashMap::new(),
            validated_sizes: BTreeMap::new(),
        }
    }
}

pub(crate) fn parse_source(bytes: &[u8], games: &[(u32, String)]) -> Result<SourceCatalog, String> {
    if bytes.len() > hydra_source::MAX_BYTES {
        return Err("Source exceeds the size limit".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "Invalid source JSON")?;
    if value.get("schemaVersion").is_some() {
        return legio_source::parse_manifest(bytes)
            .map(SourceCatalog::native)
            .map_err(|error| error.to_string());
    }
    let raw = hydra_source::parse(bytes)?;
    Ok(from_hydra(raw, games, HashMap::new()))
}

pub(crate) fn from_hydra(
    mut raw: HydraSource,
    games: &[(u32, String)],
    mut matches: HashMap<String, u32>,
) -> SourceCatalog {
    for entry in &mut raw.downloads {
        entry
            .uris
            .retain(|uri| crate::download_selection::supported(uri));
    }
    raw.downloads.retain(|entry| !entry.uris.is_empty());
    let mut manifest = Manifest {
        schema_version: 2,
        support_game_updates: false,
        generated_at: String::new(),
        verified: Vec::new(),
        unverified: Vec::new(),
    };
    let mut diagnostics = raw.diagnostics.clone();
    diagnostics.unresolved = 0;
    let mut candidates = BTreeMap::new();
    let mut index: HashMap<String, Vec<u32>> = HashMap::new();
    for (id, name) in games {
        index.entry(normalized_title(name)).or_default().push(*id);
    }
    let titles: Vec<_> = raw
        .downloads
        .iter()
        .map(|entry| entry.title.clone())
        .collect();
    let text_matches = match crate::source_search::find_matches(&titles, games) {
        Ok(matches) => matches,
        Err(error) => {
            diagnostics.reasons.push(error);
            vec![MatchResult::Unresolved; titles.len()]
        }
    };
    for (position, entry) in raw.downloads.iter().enumerate() {
        let matched = matches
            .get(&entry.title)
            .copied()
            .map(MatchResult::Matched)
            .unwrap_or_else(|| {
                match index
                    .get(&normalized_title(&entry.title))
                    .map(Vec::as_slice)
                {
                    Some([id]) => MatchResult::Matched(*id),
                    Some(_) => MatchResult::Ambiguous,
                    None => text_matches[position],
                }
            });
        let MatchResult::Matched(app_id) = matched else {
            diagnostics.unresolved += 1;
            continue;
        };
        if app_id == 0 {
            diagnostics.unresolved += 1;
            continue;
        }
        matches.insert(entry.title.clone(), app_id);
        let mut stable_uris = entry.uris.clone();
        stable_uris.sort();
        let id = format!(
            "{:x}",
            Sha256::digest(
                format!(
                    "{}\0{}\0{}",
                    entry.title,
                    entry.upload_date.as_deref().unwrap_or(""),
                    stable_uris.join("\0")
                )
                .as_bytes()
            )
        );
        candidates.insert(id.clone(), entry.uris.clone());
        let published_at = entry
            .upload_date
            .as_ref()
            .filter(|date| chrono::DateTime::parse_from_rfc3339(date).is_ok())
            .cloned();
        manifest.unverified.push(SourceEntry {
            id: Some(id),
            source_id: None,
            steam_app_id: app_id,
            name: entry.title.clone(),
            release: Release {
                version: entry.title.clone(),
                published_at,
            },
            download: Download {
                url: entry.uris[0].clone(),
                sha256: None,
                size_bytes: None,
            },
        });
    }
    SourceCatalog {
        format: SourceFormat::Hydra,
        name: Some(raw.name.clone()),
        manifest,
        candidates,
        diagnostics,
        raw_hydra: Some(raw),
        matches,
        validated_sizes: BTreeMap::new(),
    }
}

pub(crate) fn encode_cache(source: &SourceCatalog) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(source).map_err(|error| error.to_string())?;
    if bytes.len() > 2 * hydra_source::MAX_BYTES {
        return Err("Source cache exceeds the size limit".into());
    }
    Ok(bytes)
}

pub(crate) fn decode_cache(bytes: &[u8]) -> Result<SourceCatalog, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "Invalid source cache JSON")?;
    if value.get("schemaVersion").is_some() {
        return legio_source::parse_manifest(bytes)
            .map(SourceCatalog::native)
            .map_err(|error| error.to_string());
    }
    let source: SourceCatalog = serde_json::from_value(value).map_err(|error| error.to_string())?;
    if source.format == SourceFormat::Hydra {
        let raw = source.raw_hydra.ok_or("Missing original Hydra records")?;
        let mut restored = from_hydra(raw, &[], source.matches);
        restored.validated_sizes = source.validated_sizes;
        restored.diagnostics.reasons = source.diagnostics.reasons;
        return Ok(restored);
    }
    let manifest_bytes = serde_json::to_vec(&source.manifest).map_err(|error| error.to_string())?;
    let mut restored = SourceCatalog::native(
        legio_source::parse_manifest(&manifest_bytes).map_err(|error| error.to_string())?,
    );
    restored.validated_sizes = source.validated_sizes;
    Ok(restored)
}

pub(crate) async fn validate_links(
    mut catalog: SourceCatalog,
    client: &reqwest::Client,
    mut progress: impl FnMut(usize, usize) -> Result<(), String>,
) -> Result<SourceCatalog, String> {
    let mut links = BTreeMap::new();
    if let Some(raw) = &catalog.raw_hydra {
        for entry in &raw.downloads {
            for uri in &entry.uris {
                links.insert(uri.clone(), None);
            }
        }
    } else {
        for entry in catalog
            .manifest
            .verified
            .iter()
            .chain(&catalog.manifest.unverified)
        {
            links.insert(entry.download.url.clone(), entry.download.size_bytes);
        }
    }
    let total = links.len();
    let mut sizes = BTreeMap::new();
    let mut pending = links.into_iter();
    let mut tasks = tokio::task::JoinSet::new();
    let mut processed = 0;
    let mut reasons = Vec::new();
    loop {
        while tasks.len() < 4 {
            let Some((uri, expected)) = pending.next() else {
                break;
            };
            if crate::download_selection::native_cdn(&uri)
                && let Some(size) = expected
            {
                sizes.insert(uri, size);
                processed += 1;
                continue;
            }
            let client = client.clone();
            tasks.spawn(async move {
                let result = crate::download_selection::inspect(&client, &uri)
                    .await
                    .and_then(|size| {
                        if !crate::download_selection::size_matches(expected, size) {
                            Err("Download does not match the size published by the source".into())
                        } else {
                            Ok(size)
                        }
                    });
                (uri, result)
            });
        }
        let Some(result) = tasks.join_next().await else {
            break;
        };
        let (uri, result) =
            result.map_err(|error| format!("Download validation task failed: {error}"))?;
        match result {
            Ok(size) => {
                sizes.insert(uri, size);
            }
            Err(error) if reasons.len() < 20 => reasons.push(error),
            Err(_) => {}
        }
        processed += 1;
        progress(processed, total)?;
    }
    if let Some(mut raw) = catalog.raw_hydra.take() {
        for entry in &mut raw.downloads {
            let original = entry.uris.len();
            entry.uris.retain(|uri| sizes.contains_key(uri));
            raw.diagnostics.unsupported += original - entry.uris.len();
        }
        raw.downloads.retain(|entry| !entry.uris.is_empty());
        catalog = from_hydra(raw, &[], catalog.matches);
    } else {
        catalog
            .manifest
            .verified
            .retain(|entry| sizes.contains_key(&entry.download.url));
        catalog
            .manifest
            .unverified
            .retain(|entry| sizes.contains_key(&entry.download.url));
    }
    catalog.validated_sizes = sizes;
    catalog.diagnostics.reasons.extend(reasons);
    progress(total, total)?;
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn validated_links_hide_html_entries_preserve_json_order_and_survive_cache() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let first = format!("{base}/z-good.zip");
        let second = format!("{base}/a-good.zip");
        let page = format!("{base}/page.zip");
        let server = thread::spawn(move || {
            for _ in 0..3 {
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
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).unwrap();
                socket.write_all(body).unwrap();
            }
        });
        let bytes=serde_json::to_vec(&serde_json::json!({"name":"Publisher","downloads":[{"title":"Portal","uris":[first,second]},{"title":"Other","uris":[page]}]})).unwrap();
        let catalog =
            parse_source(&bytes, &[(400, "Portal".into()), (401, "Other".into())]).unwrap();
        let validated = validate_links(
            catalog,
            &crate::download_resolver::client().unwrap(),
            |_, _| Ok(()),
        )
        .await
        .unwrap();
        server.join().unwrap();
        assert_eq!(validated.manifest.unverified.len(), 1);
        assert_eq!(validated.manifest.unverified[0].steam_app_id, 400);
        let entry_id = validated.manifest.unverified[0].id.as_ref().unwrap();
        assert_eq!(validated.candidates[entry_id], vec![first, second]);
        let restored = decode_cache(&encode_cache(&validated).unwrap()).unwrap();
        assert_eq!(restored.manifest.unverified.len(), 1);
        assert_eq!(restored.candidates, validated.candidates);
        assert_eq!(restored.validated_sizes, validated.validated_sizes);
    }

    #[tokio::test]
    async fn native_cdn_entries_do_not_use_link_inspection() {
        let bytes=br#"{"schemaVersion":1,"generatedAt":"2026-10-17T00:00:00Z","verified":[],"unverified":[{"steamAppId":400,"name":"Portal","release":{"version":"1","publishedAt":"2026-10-17T00:00:00Z"},"download":{"url":"https://cdn.taxphobia.top/game.zip","sizeBytes":10}}]}"#;
        let catalog = parse_source(bytes, &[]).unwrap();
        let client = crate::download_resolver::client().unwrap();
        let validated = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            validate_links(catalog, &client, |_, _| Ok(())),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(validated.manifest.unverified.len(), 1);
        assert_eq!(
            validated.validated_sizes["https://cdn.taxphobia.top/game.zip"],
            10
        );
    }

    #[test]
    fn text_index_imports_release_words_and_preserves_exact_identity() {
        let bytes = br#"{"name":"Publisher","downloads":[{"title":"Machine Human Resource SteamGG Repack","uris":["https://example.test/a.zip"]},{"title":"Portal Stories Mel","uris":["https://example.test/b.zip"]}]}"#;
        let source = parse_source(
            bytes,
            &[
                (1, "Human Resource Machine".into()),
                (2, "Portal".into()),
                (3, "Portal Stories Mel".into()),
            ],
        )
        .unwrap();
        assert_eq!(source.manifest.unverified.len(), 2);
        assert_eq!(source.manifest.unverified[0].steam_app_id, 1);
        assert_eq!(source.manifest.unverified[1].steam_app_id, 3);
        assert!(source.manifest.verified.is_empty());
        let cached = decode_cache(&encode_cache(&source).unwrap()).unwrap();
        assert_eq!(cached.manifest.unverified[0].steam_app_id, 1);
    }

    #[test]
    fn steamgg_titles_match_without_changing_release_display_or_trust() {
        let title = "Beyond These Stars Free Download [v1.0.2244/Build-25972788]";
        let bytes = serde_json::to_vec(&serde_json::json!({
            "name": "SteamGG", "downloads": [{"title": title, "uris": ["https://gofile.io/d/example"]}]
        })).unwrap();
        let source = parse_source(&bytes, &[(123, "Beyond These Stars".into())]).unwrap();
        assert!(source.manifest.verified.is_empty());
        assert_eq!(source.diagnostics.unresolved, 0);
        assert_eq!(source.manifest.unverified[0].steam_app_id, 123);
        assert_eq!(source.manifest.unverified[0].name, title);
        let cached = decode_cache(&encode_cache(&source).unwrap()).unwrap();
        assert_eq!(cached.manifest.unverified[0].steam_app_id, 123);
        assert_eq!(cached.manifest.unverified[0].name, title);
    }

    #[test]
    fn cache_roundtrip_preserves_unverified_origin_and_unknown_metadata() {
        let bytes = br#"{"name":"SteamGG","downloads":[{"title":"Portal v1.0","uris":["https://gofile.io/d/test","https://example.test/file.zip"]}]}"#;
        let games = vec![(400, "Portal".to_owned())];
        let source = parse_source(bytes, &games).unwrap();
        assert!(source.manifest.verified.is_empty());
        assert!(!source.manifest.support_game_updates);
        assert_eq!(source.manifest.unverified.len(), 1);
        assert_eq!(source.manifest.unverified[0].download.size_bytes, None);
        assert_eq!(source.manifest.unverified[0].release.published_at, None);
        let cached = decode_cache(&encode_cache(&source).unwrap()).unwrap();
        assert_eq!(cached.name.as_deref(), Some("SteamGG"));
        assert_eq!(cached.candidates.values().next().unwrap().len(), 2);
        assert!(cached.manifest.verified.is_empty());
    }

    #[test]
    fn same_label_with_different_links_has_distinct_release_ids() {
        let source = parse_source(br#"{"name":"Publisher","downloads":[{"title":"Portal","uris":["https://example.test/a.zip"]},{"title":"Portal","uris":["https://example.test/b.zip"]}]}"#, &[(400, "Portal".into())]).unwrap();
        assert_ne!(
            source.manifest.unverified[0].id,
            source.manifest.unverified[1].id
        );
        assert_eq!(source.candidates.len(), 2);
    }

    #[test]
    fn malformed_native_documents_never_fall_back_to_hydra() {
        assert!(
            parse_source(br#"{"schemaVersion":99,"name":"Fake","downloads":[]}"#, &[]).is_err()
        );
    }
}
