use std::{collections::HashSet, fmt};

use chrono::{DateTime, FixedOffset};
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::network::NetworkState;

pub const MAX_MANIFEST_BYTES: usize = 2 * 1024 * 1024;
pub const SOURCE_URL: &str = "https://source.taxphobia.top/store.json";

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub generated_at: String,
    pub verified: Vec<SourceEntry>,
    pub unverified: Vec<SourceEntry>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceEntry {
    pub steam_app_id: u32,
    pub name: String,
    pub release: Release,
    pub download: Download,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    pub version: String,
    pub published_at: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Download {
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestErrorKind {
    TooLarge,
    InvalidDocument,
    UnsupportedVersion,
    InvalidField,
    DuplicateAppId,
    InvalidUrl,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ManifestError {
    pub kind: ManifestErrorKind,
    pub field: String,
    pub detail: String,
}

impl ManifestError {
    fn new(kind: ManifestErrorKind, field: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind,
            field: field.into(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.field, self.detail)
    }
}

impl std::error::Error for ManifestError {}

pub async fn fetch_manifest(network: &NetworkState) -> Result<Manifest, String> {
    validate_manifest_url(SOURCE_URL).map_err(|error| error.to_string())?;
    let bytes = network
        .legio_source()
        .await
        .map_err(|error| format!("Could not fetch Legio source: {error:?}"))?;
    parse_manifest(&bytes).map_err(|error| format!("Invalid Legio source: {error}"))
}

pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, ManifestError> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(ManifestError::new(
            ManifestErrorKind::TooLarge,
            "$",
            "manifest exceeds the size limit",
        ));
    }
    let manifest: Manifest = serde_json::from_slice(bytes).map_err(|error| {
        ManifestError::new(
            ManifestErrorKind::InvalidDocument,
            "$",
            format!("invalid manifest JSON: {error}"),
        )
    })?;
    if manifest.schema_version != 1 {
        return Err(ManifestError::new(
            ManifestErrorKind::UnsupportedVersion,
            "schemaVersion",
            format!("unsupported schema version {}", manifest.schema_version),
        ));
    }
    validate_utc_timestamp(&manifest.generated_at, "generatedAt")?;
    let mut seen = HashSet::new();
    for (list_name, entries) in [
        ("verified", &manifest.verified),
        ("unverified", &manifest.unverified),
    ] {
        for (index, entry) in entries.iter().enumerate() {
            let prefix = format!("{list_name}[{index}]");
            if entry.steam_app_id == 0 {
                return Err(invalid_field(
                    format!("{prefix}.steamAppId"),
                    "must be positive",
                ));
            }
            if !seen.insert(entry.steam_app_id) {
                return Err(ManifestError::new(
                    ManifestErrorKind::DuplicateAppId,
                    format!("{prefix}.steamAppId"),
                    format!("Steam App ID {} appears more than once", entry.steam_app_id),
                ));
            }
            validate_nonempty(&entry.name, format!("{prefix}.name"))?;
            validate_nonempty(&entry.release.version, format!("{prefix}.release.version"))?;
            validate_utc_timestamp(
                &entry.release.published_at,
                &format!("{prefix}.release.publishedAt"),
            )?;
            validate_archive_url(&entry.download.url, &format!("{prefix}.download.url"))?;
            if entry.download.sha256.len() != 64
                || !entry
                    .download
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(invalid_field(
                    format!("{prefix}.download.sha256"),
                    "must contain 64 lowercase hexadecimal characters",
                ));
            }
            if entry.download.size_bytes == 0 {
                return Err(invalid_field(
                    format!("{prefix}.download.sizeBytes"),
                    "must be positive",
                ));
            }
        }
    }
    Ok(manifest)
}

pub fn validate_manifest_url(url: &str) -> Result<Url, ManifestError> {
    let parsed = Url::parse(url).map_err(|_| {
        ManifestError::new(ManifestErrorKind::InvalidUrl, "sourceUrl", "invalid URL")
    })?;
    if parsed.scheme() != "https"
        || parsed.host().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(ManifestError::new(
            ManifestErrorKind::InvalidUrl,
            "sourceUrl",
            "manifest source must be an HTTPS URL without credentials or fragment",
        ));
    }
    Ok(parsed)
}

fn validate_archive_url(url: &str, field: &str) -> Result<(), ManifestError> {
    let parsed = Url::parse(url).map_err(|_| {
        ManifestError::new(ManifestErrorKind::InvalidUrl, field, "invalid archive URL")
    })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(ManifestError::new(
            ManifestErrorKind::InvalidUrl,
            field,
            "archive must be an HTTP or HTTPS URL without credentials or fragment",
        ));
    }
    Ok(())
}

fn validate_utc_timestamp(value: &str, field: &str) -> Result<(), ManifestError> {
    let valid = DateTime::<FixedOffset>::parse_from_rfc3339(value)
        .is_ok_and(|timestamp| timestamp.offset().local_minus_utc() == 0);
    if !valid {
        return Err(invalid_field(field, "must be an RFC 3339 UTC timestamp"));
    }
    Ok(())
}

fn validate_nonempty(value: &str, field: String) -> Result<(), ManifestError> {
    if value.trim().is_empty() {
        return Err(invalid_field(field, "must not be empty"));
    }
    Ok(())
}

fn invalid_field(field: impl Into<String>, detail: impl Into<String>) -> ManifestError {
    ManifestError::new(ManifestErrorKind::InvalidField, field, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn valid() -> Value {
        json!({
            "schemaVersion": 1,
            "generatedAt": "2026-09-22T00:00:00Z",
            "verified": [{
                "steamAppId": 400,
                "name": "Portal",
                "release": { "version": "1.0.0", "publishedAt": "2026-09-22T00:00:00Z" },
                "download": {
                    "url": "https://downloads.example.invalid/portal.zip",
                    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    "sizeBytes": 123456789
                }
            }],
            "unverified": []
        })
    }

    fn parse(value: Value) -> Result<Manifest, ManifestError> {
        parse_manifest(&serde_json::to_vec(&value).unwrap())
    }

    #[test]
    fn parses_canonical_manifest_without_losing_trust_lists() {
        let mut document = valid();
        document["unverified"] = json!([{
            "steamAppId": 570,
            "name": "Dota 2",
            "release": { "version": "2", "publishedAt": "2026-09-22T01:00:00+00:00" },
            "download": {
                "url": "http://downloads.example.invalid/dota.zip?token=abc",
                "sha256": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
                "sizeBytes": 1
            }
        }]);
        let result = parse(document).unwrap();
        assert_eq!(result.verified[0].steam_app_id, 400);
        assert_eq!(result.unverified[0].steam_app_id, 570);
    }

    #[test]
    fn rejects_invalid_versions_and_conflicting_ids() {
        let mut document = valid();
        document["schemaVersion"] = json!(2);
        assert_eq!(
            parse(document).unwrap_err().kind,
            ManifestErrorKind::UnsupportedVersion
        );

        let mut document = valid();
        document["unverified"] = json!([document["verified"][0].clone()]);
        assert_eq!(
            parse(document).unwrap_err().kind,
            ManifestErrorKind::DuplicateAppId
        );

        let mut document = valid();
        let duplicate = document["verified"][0].clone();
        document["verified"].as_array_mut().unwrap().push(duplicate);
        assert_eq!(
            parse(document).unwrap_err().kind,
            ManifestErrorKind::DuplicateAppId
        );
    }

    #[test]
    fn rejects_missing_fields_unknown_fields_and_invalid_values() {
        let mut document = valid();
        document["verified"][0]["release"]
            .as_object_mut()
            .unwrap()
            .remove("version");
        assert_eq!(
            parse(document).unwrap_err().kind,
            ManifestErrorKind::InvalidDocument
        );

        let mut document = valid();
        document["verified"][0]["download"]["script"] = json!("run.sh");
        assert_eq!(
            parse(document).unwrap_err().kind,
            ManifestErrorKind::InvalidDocument
        );

        for (path, value, kind) in [
            (
                "/generatedAt",
                json!("2026-09-22T02:00:00+02:00"),
                ManifestErrorKind::InvalidField,
            ),
            (
                "/verified/0/steamAppId",
                json!(0),
                ManifestErrorKind::InvalidField,
            ),
            (
                "/verified/0/release/version",
                json!(" "),
                ManifestErrorKind::InvalidField,
            ),
            (
                "/verified/0/download/sizeBytes",
                json!(0),
                ManifestErrorKind::InvalidField,
            ),
            (
                "/verified/0/download/sha256",
                json!("ABCD"),
                ManifestErrorKind::InvalidField,
            ),
            (
                "/verified/0/download/url",
                json!("file:///tmp/archive.zip"),
                ManifestErrorKind::InvalidUrl,
            ),
        ] {
            let mut document = valid();
            *document.pointer_mut(path).unwrap() = value;
            assert_eq!(parse(document).unwrap_err().kind, kind, "{path}");
        }
    }

    #[test]
    fn enforces_https_for_manifest_sources() {
        assert!(validate_manifest_url("https://legio.example.invalid/source.json").is_ok());
        for url in [
            "http://legio.example.invalid/source.json",
            "file:///tmp/source.json",
            "https://user@legio.example.invalid/source.json",
        ] {
            assert_eq!(
                validate_manifest_url(url).unwrap_err().kind,
                ManifestErrorKind::InvalidUrl
            );
        }
    }

    #[test]
    fn rejects_oversized_manifest_before_parsing() {
        assert_eq!(
            parse_manifest(&vec![b' '; MAX_MANIFEST_BYTES + 1])
                .unwrap_err()
                .kind,
            ManifestErrorKind::TooLarge
        );
    }
}
