use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

const MAX_PART_LENGTH: usize = 128;

/// Thunderstore package identity, `Owner-Name`. Neither part contains `-`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageName {
    pub(crate) owner: String,
    pub(crate) name: String,
}

/// Semantic version of a Thunderstore package, `major.minor.patch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Version {
    pub(crate) major: u32,
    pub(crate) minor: u32,
    pub(crate) patch: u32,
}

/// A package at an exact version, written `Owner-Name-1.2.3` in dependency lists.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageVersionId {
    pub(crate) package: PackageName,
    pub(crate) version: Version,
}

impl PackageName {
    pub(crate) fn new(owner: &str, name: &str) -> Result<Self, String> {
        for (label, part) in [("owner", owner), ("name", name)] {
            if part.is_empty()
                || part.len() > MAX_PART_LENGTH
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            {
                return Err(format!(
                    "Thunderstore package {label} must be 1 to {MAX_PART_LENGTH} letters, digits or underscores"
                ));
            }
        }
        Ok(Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }
}

impl FromStr for PackageName {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, String> {
        let (owner, name) = value
            .split_once('-')
            .ok_or_else(|| format!("Package name \"{value}\" is not in Owner-Name form"))?;
        Self::new(owner, name)
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-{}", self.owner, self.name)
    }
}

impl Version {
    pub(crate) const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl FromStr for Version {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, String> {
        let invalid = || format!("Version \"{value}\" is not in major.minor.patch form");
        let mut parts = value.split('.');
        let mut next = || {
            let part = parts.next().ok_or_else(invalid)?;
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(invalid());
            }
            part.parse::<u32>().map_err(|_| invalid())
        };
        let version = Self::new(next()?, next()?, next()?);
        if parts.next().is_some() {
            return Err(invalid());
        }
        Ok(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PackageVersionId {
    pub(crate) fn new(package: PackageName, version: Version) -> Self {
        Self { package, version }
    }
}

impl FromStr for PackageVersionId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, String> {
        let (package, version) = value
            .rsplit_once('-')
            .ok_or_else(|| format!("Dependency \"{value}\" is not in Owner-Name-Version form"))?;
        Ok(Self::new(package.parse()?, version.parse()?))
    }
}

impl fmt::Display for PackageVersionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-{}", self.package, self.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_package_names() {
        let name: PackageName = "BepInEx-BepInExPack".parse().unwrap();
        assert_eq!(name.owner, "BepInEx");
        assert_eq!(name.name, "BepInExPack");
        assert_eq!(name.to_string(), "BepInEx-BepInExPack");
    }

    #[test]
    fn rejects_invalid_package_names() {
        for value in [
            "",
            "Owner",
            "-Name",
            "Owner-",
            "Owner-Na-me",
            "Owner-Na me",
            "Owner-Na/me",
            "Owner-..",
            "Owner\\-Name",
            "Ow ner-Name",
        ] {
            assert!(value.parse::<PackageName>().is_err(), "{value}");
        }
        let long = "a".repeat(129);
        assert!(format!("{long}-Name").parse::<PackageName>().is_err());
    }

    #[test]
    fn parses_and_orders_versions() {
        let version: Version = "1.10.3".parse().unwrap();
        assert_eq!(version, Version::new(1, 10, 3));
        assert_eq!(version.to_string(), "1.10.3");
        assert!(Version::new(1, 10, 0) > Version::new(1, 9, 99));
        assert!(Version::new(2, 0, 0) > Version::new(1, 99, 99));
        assert!(Version::new(1, 0, 1) > Version::new(1, 0, 0));
    }

    #[test]
    fn rejects_invalid_versions() {
        for value in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "1..3",
            "a.b.c",
            "+1.2.3",
            "1.2.-3",
            "1.2.3-beta",
            "1.2.99999999999",
        ] {
            assert!(value.parse::<Version>().is_err(), "{value}");
        }
    }

    #[test]
    fn parses_dependency_strings() {
        let id: PackageVersionId = "RiskofThunder-R2API_Core-5.1.4".parse().unwrap();
        assert_eq!(id.package.to_string(), "RiskofThunder-R2API_Core");
        assert_eq!(id.version, Version::new(5, 1, 4));
        assert_eq!(id.to_string(), "RiskofThunder-R2API_Core-5.1.4");
    }

    #[test]
    fn rejects_invalid_dependency_strings() {
        for value in [
            "",
            "Owner-Name",
            "Owner-1.2.3",
            "Owner-Name-1.2",
            "Name-1.2.3-",
        ] {
            assert!(value.parse::<PackageVersionId>().is_err(), "{value}");
        }
    }

    #[test]
    fn serializes_versions_as_numeric_parts() {
        let value = serde_json::to_value(Version::new(1, 2, 3)).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"major": 1, "minor": 2, "patch": 3})
        );
    }
}
