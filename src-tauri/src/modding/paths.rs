use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::package_id::{PackageName, Version};

/// Locations under `<app data>/modding`. Game and profile ids are validated
/// as UUIDs before they become path components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModdingPaths {
    root: PathBuf,
}

impl ModdingPaths {
    pub(crate) fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("modding"),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn ecosystem_cache(&self) -> PathBuf {
        self.root.join("ecosystem.json")
    }

    pub(crate) fn package_db(&self) -> PathBuf {
        self.root.join("packages.sqlite3")
    }

    /// Extracted package contents, shared by all games and profiles.
    pub(crate) fn package_cache_dir(&self, package: &PackageName, version: &Version) -> PathBuf {
        self.root
            .join("cache")
            .join(package.to_string())
            .join(version.to_string())
    }

    pub(crate) fn game_dir(&self, game_id: &str) -> Result<PathBuf, String> {
        Ok(self.root.join("games").join(canonical_id(game_id, "game")?))
    }

    pub(crate) fn profile_dir(&self, game_id: &str, profile_id: &str) -> Result<PathBuf, String> {
        Ok(self
            .game_dir(game_id)?
            .join("profiles")
            .join(canonical_id(profile_id, "profile")?))
    }

    /// Thunderstore package icons, named by the hash of their address.
    pub(crate) fn icon_cache_dir(&self) -> PathBuf {
        self.root.join("icons")
    }

    pub(crate) fn temp_dir(&self) -> PathBuf {
        self.root.join("temp")
    }
}

/// Checks a `/`-separated path that is relative to a profile or game folder:
/// no empty, `.` or `..` parts, no backslashes, colons or control characters,
/// and no part ending in a dot or space (Windows drops them).
pub(crate) fn validate_relative_path(path: &str) -> Result<(), String> {
    let invalid = path.split('/').any(|part| {
        part.is_empty()
            || part == "."
            || part == ".."
            || part.contains(['\\', ':'])
            || part.chars().any(char::is_control)
            || part.ends_with(['.', ' '])
    });
    if invalid {
        Err(format!("File path \"{path}\" is not a safe relative path"))
    } else {
        Ok(())
    }
}

/// Joins a validated relative path onto `base`.
pub(crate) fn join_relative(base: &Path, relative: &str) -> Result<PathBuf, String> {
    validate_relative_path(relative)?;
    Ok(relative
        .split('/')
        .fold(base.to_path_buf(), |path, part| path.join(part)))
}

fn canonical_id(value: &str, label: &str) -> Result<String, String> {
    Uuid::parse_str(value)
        .map(|id| id.to_string())
        .map_err(|_| format!("{label} id is invalid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAME: &str = "00000000-0000-4000-8000-000000000001";
    const PROFILE: &str = "00000000-0000-4000-8000-000000000002";

    #[test]
    fn builds_paths_under_the_modding_root() {
        let paths = ModdingPaths::new(Path::new("data"));
        let root = Path::new("data").join("modding");
        assert_eq!(paths.root(), root);
        assert_eq!(paths.ecosystem_cache(), root.join("ecosystem.json"));
        assert_eq!(paths.package_db(), root.join("packages.sqlite3"));
        assert_eq!(paths.temp_dir(), root.join("temp"));
        assert_eq!(
            paths.package_cache_dir(&"Owner-Name".parse().unwrap(), &Version::new(1, 2, 3)),
            root.join("cache").join("Owner-Name").join("1.2.3")
        );
        assert_eq!(paths.game_dir(GAME).unwrap(), root.join("games").join(GAME));
        assert_eq!(
            paths.profile_dir(GAME, PROFILE).unwrap(),
            root.join("games").join(GAME).join("profiles").join(PROFILE)
        );
    }

    #[test]
    fn rejects_ids_that_are_not_uuids() {
        let paths = ModdingPaths::new(Path::new("data"));
        for id in ["", "..", "../x", "a/b", "a\\b", "not-a-uuid"] {
            assert!(paths.game_dir(id).is_err(), "{id}");
            assert!(paths.profile_dir(GAME, id).is_err(), "{id}");
            assert!(paths.profile_dir(id, PROFILE).is_err(), "{id}");
        }
    }

    #[test]
    fn accepts_plain_relative_paths() {
        for path in [
            "a",
            "a/b.txt",
            "BepInEx/plugins/Owner-Mod/Mod.dll",
            ".hidden",
        ] {
            assert!(validate_relative_path(path).is_ok(), "{path}");
        }
        assert_eq!(
            join_relative(Path::new("base"), "a/b.txt").unwrap(),
            Path::new("base").join("a").join("b.txt")
        );
    }

    #[test]
    fn rejects_unsafe_relative_paths() {
        for path in [
            "",
            "/abs",
            "a//b",
            "a/",
            ".",
            "..",
            "../x",
            "a/../x",
            "a/./b",
            "a\\b",
            "C:/x",
            "a/b:stream",
            "a/b\u{7}",
            "a/b.",
            "a /b",
            "a/b ",
        ] {
            assert!(validate_relative_path(path).is_err(), "{path:?}");
            assert!(join_relative(Path::new("base"), path).is_err(), "{path:?}");
        }
    }

    #[test]
    fn canonicalizes_uppercase_uuids() {
        let paths = ModdingPaths::new(Path::new("data"));
        assert_eq!(
            paths.game_dir(&GAME.to_uppercase()).unwrap(),
            paths.game_dir(GAME).unwrap()
        );
    }
}
