//! Disk usage of the modding caches and commands to clear them. Profiles are
//! never touched: a disabled mod whose files were cleared is downloaded again
//! when it is enabled.

use std::{fs, io, path::Path};

use serde::{Deserialize, Serialize};
use tauri::Manager;

use super::archive_cache::TempPath;
use super::install_commands::InstallCancellations;
use super::package_cache::PackageCache;
use super::paths::ModdingPaths;
use super::profile_commands::run_modding;

const BUSY: &str = "Mods are being installed or changed. Wait for it to finish and try again";
pub(crate) const CACHE_CLEARING: &str =
    "The mod cache is being cleared. Wait for it to finish and try again";

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModCacheKind {
    /// Extracted packages downloaded from Thunderstore.
    Packages,
    /// Local copy of the Thunderstore package listings.
    Catalog,
    Icons,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModCacheUsage {
    packages_bytes: u64,
    catalog_bytes: u64,
    icons_bytes: u64,
}

fn usage(paths: &ModdingPaths) -> Result<ModCacheUsage, String> {
    let database = paths.package_db();
    let mut catalog_bytes = 0;
    for suffix in ["", "-wal", "-shm"] {
        let mut name = database.clone().into_os_string();
        name.push(suffix);
        catalog_bytes += file_size(Path::new(&name))?;
    }
    Ok(ModCacheUsage {
        packages_bytes: directory_size(&paths.package_cache_root())?,
        catalog_bytes,
        icons_bytes: directory_size(&paths.icon_cache_dir())?,
    })
}

fn file_size(path: &Path) -> Result<u64, String> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(format!("Could not measure the mod cache: {error}")),
    }
}

/// Total size of the files under `directory`; links are not followed.
fn directory_size(directory: &Path) -> Result<u64, String> {
    let measure = |error: io::Error| format!("Could not measure the mod cache: {error}");
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(measure(error)),
    };
    let mut total = 0_u64;
    for entry in entries {
        let entry = entry.map_err(measure)?;
        let file_type = entry.file_type().map_err(measure)?;
        if file_type.is_dir() {
            total = total.saturating_add(directory_size(&entry.path())?);
        } else if file_type.is_file() {
            total = total.saturating_add(entry.metadata().map_err(measure)?.len());
        }
    }
    Ok(total)
}

fn remove_directory(directory: &Path) -> Result<(), String> {
    match fs::remove_dir_all(directory) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            Err(format!("Could not clear the mod cache: {error}"))
        }
        _ => Ok(()),
    }
}

/// Moves the package cache out of the way in one step before deleting it, so
/// a package folder is never visible half deleted. The folder is moved into
/// the temp folder, which is on the same volume.
fn clear_packages(paths: &ModdingPaths) -> Result<(), String> {
    let cache = paths.package_cache_root();
    if !cache.is_dir() {
        return Ok(());
    }
    let discarded = TempPath::new(paths, "-cache")?;
    match fs::rename(&cache, discarded.path()) {
        Ok(()) => remove_directory(discarded.path()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not clear the mod cache: {error}")),
    }
}

fn clear_files(paths: &ModdingPaths, kind: ModCacheKind) -> Result<(), String> {
    match kind {
        ModCacheKind::Packages => clear_packages(paths),
        ModCacheKind::Icons => remove_directory(&paths.icon_cache_dir()),
        ModCacheKind::Catalog => Err("The catalog is not a folder".to_owned()),
    }
}

#[tauri::command]
pub async fn get_mod_cache_usage(app: tauri::AppHandle) -> Result<ModCacheUsage, String> {
    run_modding(app, |_, _, paths, _| usage(paths)).await
}

#[tauri::command]
pub async fn clear_mod_cache(app: tauri::AppHandle, kind: ModCacheKind) -> Result<(), String> {
    run_modding(app, move |app, _, paths, locks| {
        // Holding every profile keeps installs, imports and toggles from
        // starting, or makes them wait, until the cache is cleared.
        let _exclusive = locks.try_lock_all()?.ok_or(BUSY)?;
        if app.state::<InstallCancellations>().any_active()? {
            return Err(BUSY.to_owned());
        }
        match kind {
            ModCacheKind::Catalog => app.state::<PackageCache>().clear(),
            ModCacheKind::Packages | ModCacheKind::Icons => clear_files(paths, kind),
        }?;
        crate::application_log::state_change("mod_cache_cleared", &format!("{kind:?}"));
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modding::test_support::TempDir;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0_u8; bytes]).unwrap();
    }

    #[test]
    fn measures_each_cache() {
        let dir = TempDir::new();
        let paths = ModdingPaths::new(dir.path());
        assert_eq!(
            usage(&paths).unwrap(),
            ModCacheUsage {
                packages_bytes: 0,
                catalog_bytes: 0,
                icons_bytes: 0
            }
        );
        write(&paths.root().join("cache/Owner-Mod/1.0.0/a.dll"), 10);
        write(&paths.root().join("cache/Owner-Mod/1.0.0/sub/b.dll"), 5);
        // Imported archives cannot be downloaded again, so they are not counted.
        write(&paths.root().join("local-cache/Local-Mod/1.0.0/c.dll"), 99);
        write(&paths.package_db(), 100);
        write(&paths.root().join("packages.sqlite3-wal"), 20);
        write(&paths.icon_cache_dir().join("x.png"), 7);
        assert_eq!(
            usage(&paths).unwrap(),
            ModCacheUsage {
                packages_bytes: 15,
                catalog_bytes: 120,
                icons_bytes: 7
            }
        );
    }

    #[test]
    fn clears_packages_and_icons_only() {
        let dir = TempDir::new();
        let paths = ModdingPaths::new(dir.path());
        write(&paths.root().join("cache/Owner-Mod/1.0.0/a.dll"), 1);
        write(&paths.root().join("local-cache/Local-Mod/1.0.0/c.dll"), 1);
        write(&paths.icon_cache_dir().join("x.png"), 1);
        let profile = paths
            .profile_dir(
                "00000000-0000-4000-8000-000000000001",
                "00000000-0000-4000-8000-000000000002",
            )
            .unwrap();
        write(&profile.join("BepInEx/plugins/Mod.dll"), 1);

        clear_files(&paths, ModCacheKind::Packages).unwrap();
        clear_files(&paths, ModCacheKind::Icons).unwrap();
        clear_files(&paths, ModCacheKind::Icons).unwrap();

        assert!(!paths.package_cache_root().exists());
        assert!(!paths.icon_cache_dir().exists());
        // The moved folder does not stay behind.
        assert_eq!(fs::read_dir(paths.temp_dir()).unwrap().count(), 0);
        assert!(paths.root().join("local-cache").is_dir());
        assert!(profile.join("BepInEx/plugins/Mod.dll").is_file());
    }

    #[test]
    fn clearing_the_catalog_lets_it_start_again() {
        let dir = TempDir::new();
        let paths = ModdingPaths::new(dir.path());
        let cache = PackageCache::new(paths.package_db());
        assert_eq!(cache.status("risk-of-rain-2").unwrap(), None);
        assert!(paths.package_db().is_file());

        cache.clear().unwrap();
        assert!(!paths.package_db().exists());
        assert_eq!(cache.status("risk-of-rain-2").unwrap(), None);
        cache.clear().unwrap();
    }
}
