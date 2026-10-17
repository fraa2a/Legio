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
}

impl SourceCatalog {
    pub(crate) fn native(manifest: Manifest) -> Self {
        Self {
            format: SourceFormat::Legio,
            name: None,
            manifest,
            candidates: BTreeMap::new(),
            diagnostics: ImportDiagnostics::default(),
            raw_hydra: None,
            matches: HashMap::new(),
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
    raw: HydraSource,
    games: &[(u32, String)],
    mut matches: HashMap<String, u32>,
) -> SourceCatalog {
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
    for entry in &raw.downloads {
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
                    None => MatchResult::Unresolved,
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
        let id = format!(
            "{:x}",
            Sha256::digest(
                format!(
                    "{}\0{}",
                    entry.title,
                    entry.upload_date.as_deref().unwrap_or("")
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
        return Ok(from_hydra(raw, &[], source.matches));
    }
    let manifest_bytes = serde_json::to_vec(&source.manifest).map_err(|error| error.to_string())?;
    legio_source::parse_manifest(&manifest_bytes)
        .map(SourceCatalog::native)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn malformed_native_documents_never_fall_back_to_hydra() {
        assert!(
            parse_source(br#"{"schemaVersion":99,"name":"Fake","downloads":[]}"#, &[]).is_err()
        );
    }
}
