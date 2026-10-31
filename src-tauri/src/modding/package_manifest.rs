//! The `manifest.json` of a Thunderstore package.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

const MANIFEST_FILE: &str = "manifest.json";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct PackageManifest {
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) version_number: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(crate) dependencies: Vec<String>,
}

fn null_as_default<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<String>>::deserialize(deserializer)?.unwrap_or_default())
}

/// Reads the manifest at the root of an extracted package. `Ok(None)` when
/// there is none.
pub(crate) fn read_manifest(package_dir: &Path) -> Result<Option<PackageManifest>, String> {
    let path = package_dir.join(MANIFEST_FILE);
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot inspect {MANIFEST_FILE}: {error}")),
    };
    if !metadata.is_file() {
        return Err(format!("{MANIFEST_FILE} is not a file"));
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(format!("{MANIFEST_FILE} is too large"));
    }
    let bytes = fs::read(&path).map_err(|error| format!("Cannot read {MANIFEST_FILE}: {error}"))?;
    // A UTF-8 byte order mark is common in manifests written on Windows.
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    serde_json::from_slice(bytes)
        .map(Some)
        .map_err(|error| format!("{MANIFEST_FILE} is not valid: {error}"))
}

/// Writes a manifest for a package that came without one.
pub(crate) fn write_manifest(package_dir: &Path, manifest: &PackageManifest) -> Result<(), String> {
    let content = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("Cannot write {MANIFEST_FILE}: {error}"))?;
    fs::write(package_dir.join(MANIFEST_FILE), content)
        .map_err(|error| format!("Cannot write {MANIFEST_FILE}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(content: Option<&[u8]>) -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("legio-manifest-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        if let Some(content) = content {
            fs::write(directory.join(MANIFEST_FILE), content).unwrap();
        }
        directory
    }

    #[test]
    fn reads_the_fields_mod_managers_need() {
        let directory = directory(Some(
            b"\xEF\xBB\xBF{\"name\":\"Mod\",\"version_number\":\"1.2.3\",\"website_url\":\"\",\
              \"dependencies\":[\"A-B-1.0.0\"],\"extra\":1}",
        ));
        let manifest = read_manifest(&directory).unwrap().unwrap();
        assert_eq!(manifest.name.as_deref(), Some("Mod"));
        assert_eq!(manifest.version_number.as_deref(), Some("1.2.3"));
        assert_eq!(manifest.dependencies, ["A-B-1.0.0"]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn tolerates_missing_and_null_fields() {
        let directory = directory(Some(br#"{"dependencies":null}"#));
        assert_eq!(
            read_manifest(&directory).unwrap().unwrap(),
            PackageManifest::default()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reports_absent_and_invalid_manifests() {
        let none = directory(None);
        assert_eq!(read_manifest(&none).unwrap(), None);
        fs::remove_dir_all(none).unwrap();
        let invalid = directory(Some(b"not json"));
        assert!(read_manifest(&invalid).is_err());
        fs::remove_dir_all(invalid).unwrap();
    }

    #[test]
    fn round_trips_a_written_manifest() {
        let directory = directory(None);
        let manifest = PackageManifest {
            name: Some("Mod".to_owned()),
            version_number: Some("0.0.0".to_owned()),
            dependencies: Vec::new(),
        };
        write_manifest(&directory, &manifest).unwrap();
        assert_eq!(read_manifest(&directory).unwrap(), Some(manifest));
        fs::remove_dir_all(directory).unwrap();
    }
}
