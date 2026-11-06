//! Extracted package cache and the temporary files used to fill it.

use std::{
    fs,
    path::{Path, PathBuf},
};

use uuid::Uuid;

use super::dependencies::ResolvedPackage;
use super::package_id::PackageVersionId;
use super::paths::ModdingPaths;
use crate::archive_install::{StageOptions, verify_and_stage_with};
use crate::database::ProfileMod;

/// Downloads a package archive to the given path. Used only when the package
/// is not cached yet.
pub(crate) type FetchPackage<'a> = &'a dyn Fn(&ResolvedPackage, &Path) -> Result<(), String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheStage {
    Downloading,
    Extracting,
}

/// A file or directory in the modding temp folder, removed when dropped.
pub(crate) struct TempPath(PathBuf);

impl TempPath {
    pub(crate) fn new(paths: &ModdingPaths, suffix: &str) -> Result<Self, String> {
        let temp = paths.temp_dir();
        fs::create_dir_all(&temp)
            .map_err(|error| format!("Cannot create the temporary folder: {error}"))?;
        Ok(Self(temp.join(format!("{}{suffix}", Uuid::new_v4()))))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    /// Leaves the path on disk instead of removing it when dropped.
    pub(crate) fn keep(mut self) -> PathBuf {
        std::mem::take(&mut self.0)
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        let result = if self.0.is_dir() {
            fs::remove_dir_all(&self.0)
        } else if self.0.exists() {
            fs::remove_file(&self.0)
        } else {
            return;
        };
        if let Err(error) = result {
            crate::application_log::failure("remove_modding_temp", error);
        }
    }
}

/// Where the extracted files of an installed package come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PackageOrigin<'a> {
    Thunderstore,
    /// Imported from a local archive and stored under its content key, so
    /// importing another archive never changes it.
    LocalArchive(&'a str),
    /// Imported from a local archive before imports were keyed by content.
    LegacyLocal,
}

impl<'a> PackageOrigin<'a> {
    pub(crate) fn of(record: &'a ProfileMod) -> Self {
        match (record.online_source, record.local_cache_key.as_deref()) {
            (true, _) => Self::Thunderstore,
            (false, Some(key)) => Self::LocalArchive(key),
            (false, None) => Self::LegacyLocal,
        }
    }
}

/// Where the extracted contents of a package version live. Packages imported
/// from a local archive use their own tree so they cannot collide with a
/// Thunderstore package of the same name.
pub(crate) fn package_dir(
    paths: &ModdingPaths,
    id: &PackageVersionId,
    origin: PackageOrigin<'_>,
) -> PathBuf {
    let local = paths.root().join("local-cache");
    match origin {
        PackageOrigin::Thunderstore => paths.package_cache_dir(&id.package, &id.version),
        // A hex key cannot collide with a legacy `Owner-Name` folder.
        PackageOrigin::LocalArchive(key) => local.join(key),
        PackageOrigin::LegacyLocal => local
            .join(id.package.to_string())
            .join(id.version.to_string()),
    }
}

/// Extracts the package into the cache unless it is already there and returns
/// its directory.
pub(crate) fn ensure_cached(
    paths: &ModdingPaths,
    package: &ResolvedPackage,
    fetch: FetchPackage<'_>,
    on_stage: &dyn Fn(CacheStage),
) -> Result<PathBuf, String> {
    let target = package_dir(paths, &package.id, PackageOrigin::Thunderstore);
    if target.is_dir() {
        return Ok(target);
    }
    let archive = TempPath::new(paths, ".zip")?;
    on_stage(CacheStage::Downloading);
    fetch(package, archive.path())?;
    on_stage(CacheStage::Extracting);
    let stage = stage_archive(paths, archive.path())?;
    drop(archive);
    publish_directory(stage, &target)?;
    Ok(target)
}

/// Extracts an untrusted archive into a fresh temporary directory.
pub(crate) fn stage_archive(paths: &ModdingPaths, archive: &Path) -> Result<TempPath, String> {
    let stage = TempPath::new(paths, "-stage")?;
    verify_and_stage_with(
        archive,
        None,
        stage.path(),
        StageOptions {
            normalize_backslashes: true,
        },
    )?;
    Ok(stage)
}

/// Moves a staged directory to `target`. Cached packages are never replaced:
/// a directory that already exists there (another install won the race) is
/// kept as is.
pub(crate) fn publish_directory(stage: TempPath, target: &Path) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or_else(|| "Cache location has no parent".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Cannot create the package cache: {error}"))?;
    match fs::rename(stage.path(), target) {
        Ok(()) => Ok(()),
        Err(_) if target.is_dir() => Ok(()),
        Err(error) => Err(format!("Cannot store the package in the cache: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modding::test_support::write_zip;
    use std::cell::{Cell, RefCell};

    struct Root(PathBuf);

    impl Root {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("legio-cache-{}", Uuid::new_v4()));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }

        fn paths(&self) -> ModdingPaths {
            ModdingPaths::new(&self.0)
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn package(id: &str) -> ResolvedPackage {
        ResolvedPackage {
            id: id.parse().unwrap(),
            dependencies: Vec::new(),
            download_url: "https://example.invalid/p.zip".to_owned(),
            file_size: 1,
        }
    }

    fn temp_entries(paths: &ModdingPaths) -> usize {
        fs::read_dir(paths.temp_dir()).map_or(0, Iterator::count)
    }

    #[test]
    fn downloads_extracts_and_caches_once() {
        let root = Root::new();
        let paths = root.paths();
        let fetches = Cell::new(0);
        let stages = RefCell::new(Vec::new());
        let fetch = |_: &ResolvedPackage, destination: &Path| {
            fetches.set(fetches.get() + 1);
            write_zip(
                destination,
                &[("manifest.json", "{}"), ("plugins\\Mod.dll", "dll")],
            );
            Ok(())
        };
        let on_stage = |stage| stages.borrow_mut().push(stage);

        let first = ensure_cached(&paths, &package("Owner-Mod-1.2.3"), &fetch, &on_stage).unwrap();
        assert_eq!(
            first,
            paths.package_cache_dir(&"Owner-Mod".parse().unwrap(), &"1.2.3".parse().unwrap())
        );
        assert_eq!(fs::read(first.join("plugins/Mod.dll")).unwrap(), b"dll");
        let second = ensure_cached(&paths, &package("Owner-Mod-1.2.3"), &fetch, &on_stage).unwrap();
        assert_eq!(first, second);
        assert_eq!(fetches.get(), 1);
        assert_eq!(
            *stages.borrow(),
            [CacheStage::Downloading, CacheStage::Extracting]
        );
        assert_eq!(temp_entries(&paths), 0);
    }

    #[test]
    fn failed_fetch_leaves_no_cache_or_temp_files() {
        let root = Root::new();
        let paths = root.paths();
        let fetch = |_: &ResolvedPackage, destination: &Path| {
            fs::write(destination, b"partial").unwrap();
            Err("network down".to_owned())
        };
        let error =
            ensure_cached(&paths, &package("Owner-Mod-1.0.0"), &fetch, &|_| {}).unwrap_err();
        assert_eq!(error, "network down");
        assert!(
            !package_dir(
                &paths,
                &package("Owner-Mod-1.0.0").id,
                PackageOrigin::Thunderstore
            )
            .exists()
        );
        assert_eq!(temp_entries(&paths), 0);
    }

    #[test]
    fn rejects_unsafe_and_non_archive_downloads() {
        let root = Root::new();
        let paths = root.paths();
        let evil = |_: &ResolvedPackage, destination: &Path| {
            write_zip(destination, &[("..\\..\\escape.txt", "x")]);
            Ok(())
        };
        assert!(ensure_cached(&paths, &package("Owner-Mod-1.0.0"), &evil, &|_| {}).is_err());
        let html = |_: &ResolvedPackage, destination: &Path| {
            fs::write(destination, b"<html>not an archive</html>").unwrap();
            Ok(())
        };
        assert!(ensure_cached(&paths, &package("Owner-Mod-1.0.0"), &html, &|_| {}).is_err());
        assert!(!root.0.join("escape.txt").exists());
        assert!(
            !package_dir(
                &paths,
                &package("Owner-Mod-1.0.0").id,
                PackageOrigin::Thunderstore
            )
            .exists()
        );
        assert_eq!(temp_entries(&paths), 0);
    }

    #[test]
    fn publishing_over_an_existing_directory_keeps_it() {
        let root = Root::new();
        let paths = root.paths();
        let target = root.0.join("cache").join("x");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("old"), "old").unwrap();
        let stage = TempPath::new(&paths, "-stage").unwrap();
        fs::create_dir_all(stage.path()).unwrap();
        fs::write(stage.path().join("new"), "new").unwrap();
        let stage_path = stage.path().to_path_buf();
        publish_directory(stage, &target).unwrap();
        assert!(target.join("old").is_file());
        assert!(!target.join("new").exists());
        assert!(!stage_path.exists());
    }

    #[test]
    fn local_packages_use_their_own_tree() {
        let paths = ModdingPaths::new(Path::new("data"));
        let id: PackageVersionId = "Local-Mod-1.0.0".parse().unwrap();
        let key = "ab".repeat(32);
        let local = paths.root().join("local-cache");
        assert_eq!(
            package_dir(&paths, &id, PackageOrigin::Thunderstore),
            paths.package_cache_dir(&id.package, &id.version)
        );
        assert_eq!(
            package_dir(&paths, &id, PackageOrigin::LocalArchive(&key)),
            local.join(&key)
        );
        assert_eq!(
            package_dir(&paths, &id, PackageOrigin::LegacyLocal),
            local.join("Local-Mod").join("1.0.0")
        );
    }
}
