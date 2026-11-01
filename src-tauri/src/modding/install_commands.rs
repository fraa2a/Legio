//! Installing and updating mods from Thunderstore: the catalog-backed package
//! source, the bridge from the blocking installer to async downloads, and the
//! commands built on them.

use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tokio::sync::watch;

use super::{
    archive_cache::ensure_cached,
    cache_management::CACHE_CLEARING,
    dependencies::{InstallRequest, PackageSource, ResolveMode, ResolvedPackage},
    ecosystem_store::EcosystemStore,
    installer::{
        InstallOutcome, InstallProgress, InstallStage, ModdingContext, install_packages,
        set_mod_enabled,
    },
    loader_install::plan_loader_request,
    package_cache::{PackageCache, PackageVersionRecord},
    package_id::{PackageName, PackageVersionId, Version},
    profile_commands::{
        progress_emitter, require_profile_not_running, run_modding, with_game_rules,
    },
    profiles::{ProfileLocks, profile_with_directory},
    thunderstore_http::ThunderstoreHttp,
};
use crate::database::Database;

/// Largest package download accepted, whatever the catalog says.
const MAX_PACKAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const SIZE_MARGIN_BYTES: u64 = 1024 * 1024;
const MAX_REQUESTS: usize = 500;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// A package to install. Without a version the latest one is installed.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModRequest {
    full_name: String,
    version: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModInstallResult {
    /// `Owner-Name-1.2.3` of everything installed or updated, dependencies first.
    installed: Vec<String>,
    /// Packages the profile already had in that version.
    skipped: Vec<String>,
    warnings: Vec<String>,
}

impl From<InstallOutcome> for ModInstallResult {
    fn from(outcome: InstallOutcome) -> Self {
        let names = |ids: Vec<PackageVersionId>| ids.iter().map(ToString::to_string).collect();
        Self {
            installed: names(outcome.installed),
            skipped: names(outcome.skipped),
            warnings: outcome.warnings,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModUpdate {
    full_name: String,
    installed_version: String,
    latest_version: String,
}

/// Cancellation switches of the installs in progress, one per profile.
#[derive(Default)]
pub(crate) struct InstallCancellations {
    active: Mutex<HashMap<String, watch::Sender<bool>>>,
}

struct InstallSession<'a> {
    registry: &'a InstallCancellations,
    key: String,
    cancel: watch::Receiver<bool>,
}

impl InstallCancellations {
    fn begin(&self, profile_id: &str) -> Result<InstallSession<'_>, String> {
        let key = profile_id.to_lowercase();
        let mut active = self.lock()?;
        if active.contains_key(&key) {
            return Err("Mods are already being downloaded for this profile".to_owned());
        }
        let (sender, cancel) = watch::channel(false);
        active.insert(key.clone(), sender);
        Ok(InstallSession {
            registry: self,
            key,
            cancel,
        })
    }

    /// True while any install or download is in progress.
    pub(crate) fn any_active(&self) -> Result<bool, String> {
        Ok(!self.lock()?.is_empty())
    }

    fn cancel(&self, profile_id: &str) -> Result<(), String> {
        if let Some(sender) = self.lock()?.get(&profile_id.to_lowercase()) {
            sender.send_replace(true);
        }
        Ok(())
    }

    fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<String, watch::Sender<bool>>>, String> {
        self.active
            .lock()
            .map_err(|_| "Mod download lock was poisoned".to_owned())
    }
}

impl Drop for InstallSession<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.registry.lock() {
            active.remove(&self.key);
        }
    }
}

/// Package metadata from the synced Thunderstore catalog of one game.
struct CatalogSource<'a> {
    cache: &'a PackageCache,
    community: &'a str,
}

impl PackageSource for CatalogSource<'_> {
    fn latest(&self, package: &PackageName) -> Result<Option<ResolvedPackage>, String> {
        Ok(self
            .cache
            .latest_version(self.community, package)?
            .map(resolved))
    }

    fn exact(&self, id: &PackageVersionId) -> Result<Option<ResolvedPackage>, String> {
        Ok(self.cache.version(self.community, id)?.map(resolved))
    }
}

fn resolved(record: PackageVersionRecord) -> ResolvedPackage {
    ResolvedPackage {
        id: record.id,
        dependencies: record.dependencies,
        download_url: record.download_url,
        file_size: record.file_size,
    }
}

/// The network side of an install, used from a blocking thread.
struct Online<'a> {
    app: &'a AppHandle,
    cache: PackageCache,
    http: ThunderstoreHttp,
    community: &'a str,
    operation_id: Option<String>,
    cancel: watch::Receiver<bool>,
}

impl Online<'_> {
    fn source(&self) -> CatalogSource<'_> {
        CatalogSource {
            cache: &self.cache,
            community: self.community,
        }
    }

    /// Downloads `package` to `destination`, reporting bytes as install progress.
    fn download(
        &self,
        profile_id: &str,
        package: &ResolvedPackage,
        destination: &Path,
        position: (usize, usize),
    ) -> Result<(), String> {
        let emit = progress_emitter(self.app, self.operation_id.clone());
        let mut last_report: Option<Instant> = None;
        let on_progress = |received: u64, total: Option<u64>| {
            let finished = total == Some(received);
            if !finished && last_report.is_some_and(|at| at.elapsed() < PROGRESS_INTERVAL) {
                return;
            }
            last_report = Some(Instant::now());
            emit(&InstallProgress {
                downloaded_bytes: Some(received),
                total_bytes: total,
                ..InstallProgress::new(
                    profile_id,
                    Some(&package.id),
                    InstallStage::Downloading,
                    position,
                )
            });
        };
        // Runs on a blocking thread, never on an executor thread.
        tauri::async_runtime::block_on(self.http.download_to_file(
            &package.download_url,
            destination,
            download_limit(package.file_size),
            on_progress,
            self.cancel.clone(),
        ))
        .map_err(|error| error.to_string())
    }
}

/// The catalog size plus some margin, never above the absolute ceiling.
fn download_limit(file_size: u64) -> u64 {
    if file_size == 0 {
        return MAX_PACKAGE_BYTES;
    }
    file_size
        .saturating_add(file_size / 10)
        .saturating_add(SIZE_MARGIN_BYTES)
        .min(MAX_PACKAGE_BYTES)
}

fn require_synced(cache: &PackageCache, community: &str) -> Result<(), String> {
    cache.status(community)?.map(|_| ()).ok_or_else(|| {
        "The mod catalog of this game has not been downloaded yet. Refresh it first".to_owned()
    })
}

/// Fails unless the mod catalog of `community` was synced at least once.
pub(crate) fn require_catalog(app: &AppHandle, community: &str) -> Result<(), String> {
    require_synced(&app.state::<PackageCache>(), community)
}

fn with_online<T>(
    app: &AppHandle,
    community: &str,
    profile_id: &str,
    operation_id: Option<&str>,
    work: impl FnOnce(&Online<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let cache = app.state::<PackageCache>().inner().clone();
    require_synced(&cache, community)?;
    let cancellations = app.state::<InstallCancellations>();
    let session = cancellations.begin(profile_id)?;
    // Checked after registering the session: clearing the cache sets its flag
    // first and then looks at the sessions, so one of the two sees the other.
    if app.state::<ProfileLocks>().is_exclusive()? {
        return Err(CACHE_CLEARING.to_owned());
    }
    let online = Online {
        app,
        cache,
        http: app.state::<ThunderstoreHttp>().inner().clone(),
        community,
        operation_id: operation_id.map(str::to_owned),
        cancel: session.cancel.clone(),
    };
    work(&online)
}

/// Installs packages from the Thunderstore catalog of `community`. Progress
/// events carry `operation_id`.
pub(crate) fn install_from_catalog(
    app: &AppHandle,
    ctx: &ModdingContext<'_>,
    community: &str,
    profile_id: &str,
    operation_id: Option<&str>,
    requests: &[InstallRequest],
    mode: ResolveMode,
) -> Result<InstallOutcome, String> {
    with_online(app, community, profile_id, operation_id, |online| {
        let emit = progress_emitter(app, operation_id.map(str::to_owned));
        let position = Cell::new((0, 0));
        let progress = |progress: &InstallProgress| {
            position.set((progress.current, progress.total));
            emit(progress);
        };
        install_packages(
            ctx,
            profile_id,
            requests,
            &online.source(),
            &|package, destination| {
                online.download(profile_id, package, destination, position.get())
            },
            mode,
            &progress,
        )
        .map_err(|error| error.to_string())
    })
}

/// Downloads a package version again into the package cache, for an
/// installed mod whose stored files were lost.
pub(crate) fn redownload_package(
    app: &AppHandle,
    ctx: &ModdingContext<'_>,
    community: &str,
    profile_id: &str,
    id: &PackageVersionId,
) -> Result<(), String> {
    with_online(app, community, profile_id, None, |online| {
        let package = online
            .source()
            .exact(id)?
            .ok_or_else(|| format!("{id} is no longer available on Thunderstore"))?;
        ensure_cached(
            ctx.paths,
            &package,
            &|package, destination| online.download(profile_id, package, destination, (1, 1)),
            &|_| {},
        )
        .map(|_| ())
    })
}

fn parse_requests(packages: &[ModRequest]) -> Result<Vec<InstallRequest>, String> {
    if packages.is_empty() {
        return Err("Choose at least one mod to install".to_owned());
    }
    if packages.len() > MAX_REQUESTS {
        return Err("Too many mods were requested at once".to_owned());
    }
    packages
        .iter()
        .map(|request| {
            Ok(InstallRequest {
                package: request.full_name.parse()?,
                version: request.version.as_deref().map(str::parse).transpose()?,
            })
        })
        .collect()
}

/// Online mods whose catalog has a newer version than the profile's.
fn find_updates(
    database: &Database,
    cache: &PackageCache,
    community: &str,
    profile_id: &str,
) -> Result<Vec<ModUpdate>, String> {
    let mut updates = Vec::new();
    for record in database.profile_mods(profile_id)? {
        if !record.online_source {
            continue;
        }
        let installed: Version = record.version.parse()?;
        let package: PackageName = record.full_name.parse()?;
        if let Some(latest) = cache.latest_version(community, &package)?
            && latest.id.version > installed
        {
            updates.push(ModUpdate {
                full_name: record.full_name,
                installed_version: record.version,
                latest_version: latest.id.version.to_string(),
            });
        }
    }
    Ok(updates)
}

#[tauri::command]
pub async fn install_mods(
    app: AppHandle,
    profile_id: String,
    packages: Vec<ModRequest>,
    operation_id: Option<String>,
) -> Result<ModInstallResult, String> {
    let requests = parse_requests(&packages)?;
    let schema = app.state::<EcosystemStore>().schema().await?;
    run_modding(app, move |app, database, paths, locks| {
        require_profile_not_running(app, database, paths, &profile_id)?;
        with_game_rules(
            database,
            paths,
            locks,
            &schema,
            &profile_id,
            |ctx, community| {
                install_from_catalog(
                    app,
                    ctx,
                    community,
                    &profile_id,
                    operation_id.as_deref(),
                    &requests,
                    ResolveMode::Latest,
                )
                .map(ModInstallResult::from)
            },
        )
    })
    .await
}

/// Installs the newest version of the game's mod loader into the profile.
#[tauri::command]
pub async fn install_mod_loader(
    app: AppHandle,
    profile_id: String,
    operation_id: Option<String>,
) -> Result<ModInstallResult, String> {
    let schema = app.state::<EcosystemStore>().schema().await?;
    run_modding(app, move |app, database, paths, locks| {
        require_profile_not_running(app, database, paths, &profile_id)?;
        with_game_rules(
            database,
            paths,
            locks,
            &schema,
            &profile_id,
            |ctx, community| {
                require_catalog(app, community)?;
                let cache = app.state::<PackageCache>();
                let request = plan_loader_request(
                    ctx,
                    &CatalogSource {
                        cache: &cache,
                        community,
                    },
                )?;
                install_from_catalog(
                    app,
                    ctx,
                    community,
                    &profile_id,
                    operation_id.as_deref(),
                    &[request],
                    ResolveMode::Latest,
                )
                .map(ModInstallResult::from)
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn list_mod_updates(
    app: AppHandle,
    profile_id: String,
) -> Result<Vec<ModUpdate>, String> {
    run_modding(app, move |app, database, paths, _| {
        let (profile, _) = profile_with_directory(database, paths, &profile_id)?;
        let community = database
            .game_modding(&profile.game_id)?
            .ok_or_else(|| "The game is not set up for mods".to_owned())?
            .ecosystem_game_id;
        find_updates(
            database,
            &app.state::<PackageCache>(),
            &community,
            &profile.id,
        )
    })
    .await
}

/// Updates the given mods, or every mod with an update when `full_names` is
/// absent. Mods that were disabled stay disabled.
#[tauri::command]
pub async fn update_profile_mods(
    app: AppHandle,
    profile_id: String,
    full_names: Option<Vec<String>>,
    operation_id: Option<String>,
) -> Result<ModInstallResult, String> {
    let schema = app.state::<EcosystemStore>().schema().await?;
    run_modding(app, move |app, database, paths, locks| {
        require_profile_not_running(app, database, paths, &profile_id)?;
        with_game_rules(
            database,
            paths,
            locks,
            &schema,
            &profile_id,
            |ctx, community| {
                require_catalog(app, community)?;
                let wanted: Option<HashSet<String>> =
                    full_names.map(|names| names.iter().map(|name| name.to_lowercase()).collect());
                let updates = find_updates(
                    database,
                    &app.state::<PackageCache>(),
                    community,
                    &profile_id,
                )?
                .into_iter()
                .filter(|update| {
                    wanted
                        .as_ref()
                        .is_none_or(|wanted| wanted.contains(&update.full_name.to_lowercase()))
                })
                .collect::<Vec<_>>();
                if updates.is_empty() {
                    return Ok(ModInstallResult::from(InstallOutcome {
                        installed: Vec::new(),
                        skipped: Vec::new(),
                        warnings: Vec::new(),
                    }));
                }
                let disabled: HashSet<String> = database
                    .profile_mods(&profile_id)?
                    .into_iter()
                    .filter(|record| !record.enabled)
                    .map(|record| record.full_name.to_lowercase())
                    .collect();
                let requests = updates
                    .iter()
                    .map(|update| {
                        Ok(InstallRequest {
                            package: update.full_name.parse()?,
                            version: Some(update.latest_version.parse()?),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let outcome = install_from_catalog(
                    app,
                    ctx,
                    community,
                    &profile_id,
                    operation_id.as_deref(),
                    &requests,
                    ResolveMode::Latest,
                )?;
                for id in outcome
                    .installed
                    .iter()
                    .filter(|id| disabled.contains(&id.package.to_string().to_lowercase()))
                {
                    set_mod_enabled(ctx, &profile_id, &id.package.to_string(), false).map_err(
                        |error| {
                            format!("{id} was updated but could not be disabled again: {error}")
                        },
                    )?;
                }
                Ok(ModInstallResult::from(outcome))
            },
        )
    })
    .await
}

/// Stops the downloads of the profile's running install.
#[tauri::command]
pub fn cancel_mod_install(
    cancellations: State<'_, InstallCancellations>,
    profile_id: String,
) -> Result<(), String> {
    cancellations.cancel(&profile_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_downloads_by_declared_size() {
        assert_eq!(download_limit(0), MAX_PACKAGE_BYTES);
        assert_eq!(download_limit(1000), 1000 + 100 + SIZE_MARGIN_BYTES);
        assert_eq!(download_limit(u64::MAX), MAX_PACKAGE_BYTES);
        assert_eq!(download_limit(MAX_PACKAGE_BYTES), MAX_PACKAGE_BYTES);
    }

    #[test]
    fn parses_requests() {
        let requests = parse_requests(&[
            ModRequest {
                full_name: "Owner-Name".to_owned(),
                version: None,
            },
            ModRequest {
                full_name: "Other-Mod".to_owned(),
                version: Some("1.2.3".to_owned()),
            },
        ])
        .unwrap();
        assert_eq!(requests[0].version, None);
        assert_eq!(requests[1].version, Some(Version::new(1, 2, 3)));
        assert!(parse_requests(&[]).is_err());
        for (name, version) in [("NoDash", None), ("Owner-Name", Some("1.2"))] {
            let request = ModRequest {
                full_name: name.to_owned(),
                version: version.map(str::to_owned),
            };
            assert!(parse_requests(&[request]).is_err(), "{name}");
        }
    }

    #[test]
    fn reports_active_installs() {
        let registry = InstallCancellations::default();
        assert!(!registry.any_active().unwrap());
        let session = registry.begin("A").unwrap();
        assert!(registry.any_active().unwrap());
        drop(session);
        assert!(!registry.any_active().unwrap());
    }

    #[test]
    fn cancels_only_the_running_profile() {
        let registry = InstallCancellations::default();
        let session = registry.begin("A").unwrap();
        assert!(registry.begin("a").is_err());
        let other = registry.begin("B").unwrap();
        registry.cancel("a").unwrap();
        assert!(*session.cancel.borrow());
        assert!(!*other.cancel.borrow());
        drop(session);
        assert!(registry.begin("A").is_ok());
    }
}
