//! Installs, updates, removes, disables and enables packages in a profile.
//! Network access is injected: package metadata comes from a
//! [`PackageSource`] and archives from a fetch function.
//!
//! All functions here block. Run them in `spawn_blocking`.

use std::{
    collections::{HashMap, HashSet},
    fmt, fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

use super::dependencies::{InstallRequest, PackageSource, ResolveMode, package_key, resolve_plan};
use super::ecosystem::{GameEntry, ModloaderPackage, PackageLoader};
use super::install_rules::{PlannedFile, plan_install};
use super::loader_install::plan_loader_install;
use super::package_cache::{CacheStage, FetchPackage, ensure_cached, package_dir};
use super::package_id::{PackageName, PackageVersionId, Version};
use super::package_manifest::read_manifest;
use super::paths::ModdingPaths;
use super::profiles::{ProfileLocks, copy_file, profile_with_directory};
use crate::database::{Database, ModProfile, ProfileMod, now_milliseconds};

pub(crate) const PROGRESS_EVENT: &str = "modding-install-progress";

/// Everything an operation needs to know about the game and the app.
#[derive(Clone, Copy)]
pub(crate) struct ModdingContext<'a> {
    pub(crate) database: &'a Database,
    pub(crate) paths: &'a ModdingPaths,
    pub(crate) locks: &'a ProfileLocks,
    pub(crate) game: &'a GameEntry,
    pub(crate) loaders: &'a [ModloaderPackage],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InstallStage {
    Resolving,
    Downloading,
    Extracting,
    Installing,
    Done,
}

/// Payload of the `modding-install-progress` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstallProgress {
    pub(crate) profile_id: String,
    /// `Owner-Name-1.2.3`, absent for the stages that cover the whole install.
    pub(crate) package: Option<String>,
    pub(crate) stage: InstallStage,
    /// 1-based position of the package among those being installed.
    pub(crate) current: usize,
    pub(crate) total: usize,
}

pub(crate) type ProgressSink<'a> = &'a dyn Fn(&InstallProgress);

pub(crate) fn report_progress(
    progress: ProgressSink<'_>,
    profile_id: &str,
    package: Option<&PackageVersionId>,
    stage: InstallStage,
    position: (usize, usize),
) {
    progress(&InstallProgress {
        profile_id: profile_id.to_owned(),
        package: package.map(ToString::to_string),
        stage,
        current: position.0,
        total: position.1,
    });
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InstallError {
    /// The extracted files of an installed package are gone. For a package
    /// from Thunderstore the caller can download it again.
    CacheMissing {
        package: PackageVersionId,
        online_source: bool,
    },
    LoaderCannotBeDisabled(String),
    Failed(String),
}

impl fmt::Display for InstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CacheMissing { package, .. } => write!(
                formatter,
                "The stored files of {package} are missing and must be downloaded again"
            ),
            Self::LoaderCannotBeDisabled(package) => {
                write!(formatter, "The mod loader {package} cannot be disabled")
            }
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl From<String> for InstallError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstallOutcome {
    /// Packages installed or updated, dependencies first.
    pub(crate) installed: Vec<PackageVersionId>,
    /// Packages left alone because the profile already has that version.
    pub(crate) skipped: Vec<PackageVersionId>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UninstallOutcome {
    /// Installed mods that list the removed one as a dependency. They are
    /// left installed.
    pub(crate) dependents: Vec<String>,
}

/// Installs `requests` and their dependencies into the profile.
pub(crate) fn install_packages(
    ctx: &ModdingContext<'_>,
    profile_id: &str,
    requests: &[InstallRequest],
    source: &dyn PackageSource,
    fetch: FetchPackage<'_>,
    mode: ResolveMode,
    progress: ProgressSink<'_>,
) -> Result<InstallOutcome, InstallError> {
    let (profile, profile_dir) = profile_with_directory(ctx.database, ctx.paths, profile_id)?;
    let _guard = ctx.locks.lock(&profile.id)?;
    report_progress(progress, &profile.id, None, InstallStage::Resolving, (0, 0));

    let versions = installed_versions(&ctx.database.profile_mods(&profile.id)?);
    let plan = resolve_plan(requests, source, mode, &versions)?;
    let total = plan.packages.len();
    let mut outcome = InstallOutcome {
        installed: Vec::new(),
        skipped: Vec::new(),
        warnings: plan.warnings,
    };
    for (index, package) in plan.packages.iter().enumerate() {
        let position = (index + 1, total);
        if versions.get(&package_key(&package.id.package)) == Some(&package.id.version) {
            outcome.skipped.push(package.id.clone());
            continue;
        }
        let cache_dir = ensure_cached(ctx.paths, package, fetch, &|stage| {
            let stage = match stage {
                CacheStage::Downloading => InstallStage::Downloading,
                CacheStage::Extracting => InstallStage::Extracting,
            };
            report_progress(progress, &profile.id, Some(&package.id), stage, position);
        })?;
        report_progress(
            progress,
            &profile.id,
            Some(&package.id),
            InstallStage::Installing,
            position,
        );
        install_cached_package(ctx, &profile, &profile_dir, &package.id, &cache_dir, true)?;
        outcome.installed.push(package.id.clone());
    }
    report_progress(
        progress,
        &profile.id,
        None,
        InstallStage::Done,
        (total, total),
    );
    Ok(outcome)
}

/// Copies an extracted package into the profile and records it. An installed
/// version of the same package is replaced. On failure the package ends up
/// not installed, with the files it wrote removed.
pub(super) fn install_cached_package(
    ctx: &ModdingContext<'_>,
    profile: &ModProfile,
    profile_dir: &Path,
    id: &PackageVersionId,
    cache_dir: &Path,
    online_source: bool,
) -> Result<ProfileMod, InstallError> {
    let planned = plan_package(ctx, &id.package, cache_dir)?;
    fs::create_dir_all(profile_dir)
        .map_err(|error| format!("Cannot create the profile folder: {error}"))?;

    let mods = ctx.database.profile_mods(&profile.id)?;
    let key = package_key(&id.package);
    let existing = mods
        .iter()
        .find(|record| record.full_name.to_lowercase() == key);
    let full_name =
        existing.map_or_else(|| id.package.to_string(), |record| record.full_name.clone());
    let old_files = match existing {
        Some(record) => ctx
            .database
            .profile_mod_files(&profile.id, &record.full_name)?,
        None => Vec::new(),
    };
    let tracked = tracked_paths(&planned);

    // Rejects files owned by another mod before anything is written.
    ctx.database
        .replace_profile_mod_files(&profile.id, &full_name, &tracked)?;
    let tracked_set: HashSet<&String> = tracked.iter().collect();
    let stale: Vec<String> = old_files
        .iter()
        .filter(|path| !tracked_set.contains(path))
        .cloned()
        .collect();
    if let Err(failure) = remove_files(profile_dir, &stale) {
        restore_tracked_files(ctx, &profile.id, &full_name, &old_files);
        return Err(InstallError::Failed(failure.error));
    }

    let written = match copy_planned(profile_dir, &planned) {
        Ok(written) => written,
        Err(failure) => {
            let error = discard_install(ctx, profile, profile_dir, &full_name, &failure.written);
            return Err(InstallError::Failed(append(failure.error, error)));
        }
    };
    let record = ProfileMod {
        profile_id: profile.id.clone(),
        full_name: full_name.clone(),
        version: id.version.to_string(),
        enabled: true,
        online_source,
        installed_at: now_milliseconds()?,
        install_order: existing.map_or_else(
            || {
                mods.iter()
                    .map(|record| record.install_order)
                    .max()
                    .map_or(0, |order| order + 1)
            },
            |record| record.install_order,
        ),
    };
    if let Err(error) = ctx.database.upsert_profile_mod(&record) {
        let cleanup = discard_install(ctx, profile, profile_dir, &full_name, &written);
        return Err(InstallError::Failed(append(error, cleanup)));
    }
    Ok(record)
}

/// Removes a mod: tracked files are deleted, folders left empty are pruned
/// and the records dropped. Mods that depend on it stay installed and are
/// reported.
pub(crate) fn uninstall_mod(
    ctx: &ModdingContext<'_>,
    profile_id: &str,
    full_name: &str,
) -> Result<UninstallOutcome, InstallError> {
    let (profile, profile_dir) = profile_with_directory(ctx.database, ctx.paths, profile_id)?;
    let _guard = ctx.locks.lock(&profile.id)?;
    let mods = ctx.database.profile_mods(&profile.id)?;
    let record = find_mod(&mods, full_name)?;
    let dependents = find_dependents(ctx.paths, &mods, record);
    let files = ctx
        .database
        .profile_mod_files(&profile.id, &record.full_name)?;
    if let Err(failure) = remove_files(&profile_dir, &files) {
        // Keep tracking what is still on disk so the removal can be retried.
        restore_tracked_files(ctx, &profile.id, &record.full_name, &failure.remaining);
        return Err(InstallError::Failed(failure.error));
    }
    ctx.database
        .remove_profile_mod(&profile.id, &record.full_name)?;
    Ok(UninstallOutcome { dependents })
}

/// Disabling deletes the mod's tracked files and keeps its records; enabling
/// copies them from the package cache again.
pub(crate) fn set_mod_enabled(
    ctx: &ModdingContext<'_>,
    profile_id: &str,
    full_name: &str,
    enabled: bool,
) -> Result<(), InstallError> {
    let (profile, profile_dir) = profile_with_directory(ctx.database, ctx.paths, profile_id)?;
    let _guard = ctx.locks.lock(&profile.id)?;
    let mods = ctx.database.profile_mods(&profile.id)?;
    let record = find_mod(&mods, full_name)?;
    if record.enabled == enabled {
        return Ok(());
    }
    let id = version_id(record)?;
    if enabled {
        enable_mod(ctx, &profile, &profile_dir, record, &id)
    } else {
        if find_loader(ctx, &id.package).is_some() {
            return Err(InstallError::LoaderCannotBeDisabled(id.package.to_string()));
        }
        let files = ctx
            .database
            .profile_mod_files(&profile.id, &record.full_name)?;
        remove_files(&profile_dir, &files)
            .map_err(|failure| InstallError::Failed(failure.error))?;
        ctx.database
            .set_profile_mod_enabled(&profile.id, &record.full_name, false)?;
        Ok(())
    }
}

fn enable_mod(
    ctx: &ModdingContext<'_>,
    profile: &ModProfile,
    profile_dir: &Path,
    record: &ProfileMod,
    id: &PackageVersionId,
) -> Result<(), InstallError> {
    let cache_dir = package_dir(ctx.paths, id, record.online_source);
    if !cache_dir.is_dir() {
        return Err(InstallError::CacheMissing {
            package: id.clone(),
            online_source: record.online_source,
        });
    }
    let planned = plan_package(ctx, &id.package, &cache_dir)?;
    let old_files = ctx
        .database
        .profile_mod_files(&profile.id, &record.full_name)?;
    ctx.database.replace_profile_mod_files(
        &profile.id,
        &record.full_name,
        &tracked_paths(&planned),
    )?;
    let restore = |written: &[String], error: String| {
        let cleanup = remove_files(profile_dir, written).err().map(|f| f.error);
        restore_tracked_files(ctx, &profile.id, &record.full_name, &old_files);
        InstallError::Failed(append(error, cleanup))
    };
    let written = copy_planned(profile_dir, &planned)
        .map_err(|failure| restore(&failure.written, failure.error))?;
    ctx.database
        .set_profile_mod_enabled(&profile.id, &record.full_name, true)
        .map_err(|error| restore(&written, error))?;
    Ok(())
}

fn find_dependents(paths: &ModdingPaths, mods: &[ProfileMod], target: &ProfileMod) -> Vec<String> {
    let prefix = format!("{}-", target.full_name.to_lowercase());
    mods.iter()
        .filter(|other| !other.full_name.eq_ignore_ascii_case(&target.full_name))
        .filter(|other| {
            manifest_dependencies(paths, other)
                .iter()
                .any(|dependency| dependency.to_lowercase().starts_with(&prefix))
        })
        .map(|other| other.full_name.clone())
        .collect()
}

fn manifest_dependencies(paths: &ModdingPaths, record: &ProfileMod) -> Vec<String> {
    let Ok(id) = version_id(record) else {
        return Vec::new();
    };
    match read_manifest(&package_dir(paths, &id, record.online_source)) {
        Ok(manifest) => manifest
            .map(|manifest| manifest.dependencies)
            .unwrap_or_default(),
        Err(error) => {
            crate::application_log::failure("read_mod_dependencies", error);
            Vec::new()
        }
    }
}

fn find_mod<'a>(mods: &'a [ProfileMod], full_name: &str) -> Result<&'a ProfileMod, InstallError> {
    mods.iter()
        .find(|record| record.full_name.eq_ignore_ascii_case(full_name))
        .ok_or_else(|| InstallError::Failed("Mod is not installed in this profile".to_owned()))
}

fn version_id(record: &ProfileMod) -> Result<PackageVersionId, InstallError> {
    format!("{}-{}", record.full_name, record.version)
        .parse()
        .map_err(|error: String| {
            InstallError::Failed(format!(
                "Installed mod {} is invalid: {error}",
                record.full_name
            ))
        })
}

fn installed_versions(mods: &[ProfileMod]) -> HashMap<String, Version> {
    mods.iter()
        .filter_map(|record| {
            let version = record.version.parse().ok()?;
            Some((record.full_name.to_lowercase(), version))
        })
        .collect()
}

fn find_loader<'a>(
    ctx: &ModdingContext<'a>,
    package: &PackageName,
) -> Option<&'a ModloaderPackage> {
    let name = package.to_string();
    ctx.loaders.iter().find(|loader| {
        loader.loader == ctx.game.package_loader && loader.package_id.eq_ignore_ascii_case(&name)
    })
}

fn plan_package(
    ctx: &ModdingContext<'_>,
    package: &PackageName,
    cache_dir: &Path,
) -> Result<Vec<PlannedFile>, InstallError> {
    let planned = match find_loader(ctx, package) {
        Some(loader) if loader.loader == PackageLoader::Bepinex => {
            plan_loader_install(cache_dir, &loader.root_folder)
        }
        Some(loader) => {
            return Err(InstallError::Failed(format!(
                "The {:?} mod loader is not supported yet",
                loader.loader
            )));
        }
        None => plan_install(
            &ctx.game.install_rules,
            &ctx.game.relative_file_exclusions,
            package,
            cache_dir,
        ),
    };
    Ok(planned?)
}

fn tracked_paths(planned: &[PlannedFile]) -> Vec<String> {
    planned
        .iter()
        .filter(|file| file.tracked)
        .map(|file| file.destination.clone())
        .collect()
}

struct CopyFailure {
    /// Files created or overwritten before the failure, including the one
    /// being copied.
    written: Vec<String>,
    error: String,
}

/// Copies the planned files into the profile. Tracked files replace what is
/// there; untracked ones (configuration) are left alone when they exist.
fn copy_planned(profile_dir: &Path, planned: &[PlannedFile]) -> Result<Vec<String>, CopyFailure> {
    let mut written = Vec::new();
    for file in planned {
        let target = match destination_path(profile_dir, &file.destination) {
            Ok(target) => target,
            Err(error) => return Err(CopyFailure { written, error }),
        };
        match prepare_target(&target, file.tracked) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(error) => return Err(CopyFailure { written, error }),
        }
        written.push(file.destination.clone());
        if let Err(error) = copy_file(&file.source, &target) {
            return Err(CopyFailure { written, error });
        }
    }
    Ok(written)
}

/// Creates the parent folders and reports whether the file should be copied.
fn prepare_target(target: &Path, replace_existing: bool) -> Result<bool, String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
    }
    match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.file_type().is_symlink() || metadata.is_dir() => Err(format!(
            "{} is not a regular file and was not overwritten",
            target.display()
        )),
        Ok(_) => Ok(replace_existing),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(format!("Cannot inspect {}: {error}", target.display())),
    }
}

/// Joins a `/`-separated relative path onto the profile folder, refusing
/// anything that could leave it.
fn destination_path(profile_dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let mut path = profile_dir.to_path_buf();
    for part in relative.split('/') {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.contains(['\\', ':'])
            || part.chars().any(char::is_control)
        {
            return Err(format!("File path \"{relative}\" is not allowed"));
        }
        path.push(part);
    }
    Ok(path)
}

struct RemoveFailure {
    /// Files that are still on disk.
    remaining: Vec<String>,
    error: String,
}

/// Deletes files and then the folders they leave empty, never the profile
/// folder itself. Files that are already gone are fine.
fn remove_files(profile_dir: &Path, relatives: &[String]) -> Result<(), RemoveFailure> {
    let mut remaining = Vec::new();
    let mut first_error: Option<String> = None;
    for relative in relatives {
        let removed =
            destination_path(profile_dir, relative).and_then(|path| match fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!("Cannot remove {relative}: {error}")),
            });
        if let Err(error) = removed {
            remaining.push(relative.clone());
            first_error.get_or_insert(error);
        }
    }
    prune_empty_directories(profile_dir, relatives);
    match first_error {
        None => Ok(()),
        Some(error) => Err(RemoveFailure {
            remaining,
            error: format!("{error}. Close the game if it is running and try again"),
        }),
    }
}

fn prune_empty_directories(profile_dir: &Path, relatives: &[String]) {
    for relative in relatives {
        let Ok(path) = destination_path(profile_dir, relative) else {
            continue;
        };
        let mut directory = path.parent();
        while let Some(current) = directory {
            // remove_dir only succeeds on an empty folder, so a failure here
            // means the folder is still in use and is expected.
            if current == profile_dir
                || !current.starts_with(profile_dir)
                || fs::remove_dir(current).is_err()
            {
                break;
            }
            directory = current.parent();
        }
    }
}

/// Undoes a failed install: removes the written files and the mod's records.
/// Returns a description of cleanup problems.
fn discard_install(
    ctx: &ModdingContext<'_>,
    profile: &ModProfile,
    profile_dir: &Path,
    full_name: &str,
    written: &[String],
) -> Option<String> {
    let mut problems = Vec::new();
    if let Err(failure) = remove_files(profile_dir, written) {
        problems.push(failure.error);
    }
    if let Err(error) = ctx.database.remove_profile_mod(&profile.id, full_name) {
        problems.push(error);
    }
    if problems.is_empty() {
        None
    } else {
        let message = problems.join("; ");
        crate::application_log::failure("discard_mod_install", &message);
        Some(format!("cleanup also failed: {message}"))
    }
}

fn restore_tracked_files(
    ctx: &ModdingContext<'_>,
    profile_id: &str,
    full_name: &str,
    files: &[String],
) {
    if let Err(error) = ctx
        .database
        .replace_profile_mod_files(profile_id, full_name, files)
    {
        crate::application_log::failure("restore_mod_files", error);
    }
}

fn append(error: String, cleanup: Option<String>) -> String {
    match cleanup {
        Some(cleanup) => format!("{error}; {cleanup}"),
        None => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modding::test_support::{Env, FakeRepo, request};
    use std::cell::RefCell;

    const LOADER: &str = "BepInEx-BepInExPack-5.4.2100";

    fn repo() -> FakeRepo {
        let mut repo = FakeRepo::default();
        repo.add(
            LOADER,
            &[],
            &[
                ("manifest.json", "{}"),
                ("README.md", "readme"),
                ("BepInExPack/winhttp.dll", "winhttp"),
                ("BepInExPack/doorstop_config.ini", "doorstop"),
                ("BepInExPack/BepInEx/core/BepInEx.dll", "core"),
            ],
        );
        repo.add(
            "Owner-Mod-1.0.0",
            &[LOADER],
            &[
                (
                    "manifest.json",
                    "{\"dependencies\":[\"BepInEx-BepInExPack-5.4.2100\"]}",
                ),
                ("Mod.dll", "mod v1"),
                ("assets/sound.ogg", "ogg"),
                ("config/Mod.cfg", "default config"),
            ],
        );
        repo.add(
            "Owner-Mod-2.0.0",
            &[LOADER],
            &[
                (
                    "manifest.json",
                    "{\"dependencies\":[\"BepInEx-BepInExPack-5.4.2100\"]}",
                ),
                ("Mod.dll", "mod v2"),
                ("config/Mod.cfg", "new config"),
            ],
        );
        repo.add(
            "Other-Addon-1.0.0",
            &["Owner-Mod-1.0.0"],
            &[
                ("manifest.json", "{\"dependencies\":[\"Owner-Mod-1.0.0\"]}"),
                ("Addon.dll", "addon"),
            ],
        );
        repo
    }

    fn state_game_env() -> Env {
        let mut env = Env::new();
        let entry = env.schema.games.get_mut("lethal-company").unwrap();
        entry.r2modman[0].install_rules = vec![crate::modding::ecosystem::InstallRule {
            route: "Mods".to_owned(),
            default_file_extensions: vec![".dll".to_owned()],
            tracking_method: crate::modding::ecosystem::TrackingMethod::State,
            is_default_location: true,
            sub_routes: Vec::new(),
        }];
        env
    }

    #[test]
    fn installs_a_mod_with_its_loader_first() {
        let env = Env::new();
        let outcome = env
            .install(&repo(), &["Owner-Mod"], ResolveMode::Latest)
            .unwrap();
        let installed: Vec<_> = outcome.installed.iter().map(ToString::to_string).collect();
        assert_eq!(installed, [LOADER, "Owner-Mod-2.0.0"]);

        assert!(env.exists("winhttp.dll"));
        assert!(env.exists("doorstop_config.ini"));
        assert!(env.exists("BepInEx/core/BepInEx.dll"));
        assert!(!env.exists("manifest.json"));
        assert!(!env.exists("README.md"));
        assert_eq!(
            fs::read(env.profile_dir().join("BepInEx/plugins/Owner-Mod/Mod.dll")).unwrap(),
            b"mod v2"
        );
        assert_eq!(
            fs::read(env.profile_dir().join("BepInEx/config/Mod.cfg")).unwrap(),
            b"new config"
        );

        let mods = env.mods();
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].full_name, "BepInEx-BepInExPack");
        assert_eq!(mods[0].install_order, 0);
        assert_eq!(mods[1].full_name, "Owner-Mod");
        assert_eq!(mods[1].version, "2.0.0");
        assert_eq!(mods[1].install_order, 1);
        assert!(
            mods.iter()
                .all(|record| record.enabled && record.online_source)
        );
        assert_eq!(
            env.files_of("Owner-Mod"),
            [
                "BepInEx/plugins/Owner-Mod/Mod.dll",
                "BepInEx/plugins/Owner-Mod/manifest.json",
            ]
        );
        assert!(
            env.files_of("BepInEx-BepInExPack")
                .contains(&"winhttp.dll".to_owned())
        );
    }

    #[test]
    fn reports_progress_per_package() {
        let env = Env::new();
        let events = RefCell::new(Vec::new());
        env.install_with(
            &repo(),
            &["Owner-Mod-1.0.0"],
            ResolveMode::Latest,
            &|event| {
                events.borrow_mut().push(event.clone());
            },
        )
        .unwrap();
        let events = events.into_inner();
        let summary: Vec<_> = events
            .iter()
            .map(|event| {
                (
                    event.package.as_deref(),
                    event.stage,
                    event.current,
                    event.total,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (None, InstallStage::Resolving, 0, 0),
                (Some(LOADER), InstallStage::Downloading, 1, 2),
                (Some(LOADER), InstallStage::Extracting, 1, 2),
                (Some(LOADER), InstallStage::Installing, 1, 2),
                (Some("Owner-Mod-1.0.0"), InstallStage::Downloading, 2, 2),
                (Some("Owner-Mod-1.0.0"), InstallStage::Extracting, 2, 2),
                (Some("Owner-Mod-1.0.0"), InstallStage::Installing, 2, 2),
                (None, InstallStage::Done, 2, 2),
            ]
        );
        assert!(
            events
                .iter()
                .all(|event| event.profile_id == env.profile.id)
        );
        let json = serde_json::to_value(&events[1]).unwrap();
        assert_eq!(json["profileId"], env.profile.id.as_str());
        assert_eq!(json["stage"], "downloading");
    }

    #[test]
    fn cached_packages_are_not_downloaded_again() {
        let env = Env::new();
        let repo = repo();
        env.install(&repo, &["Owner-Mod-1.0.0"], ResolveMode::Latest)
            .unwrap();
        assert_eq!(repo.fetch_count(), 2);

        let second = crate::modding::profiles::create_profile(
            &env.database,
            &env.paths,
            &env.game_id,
            "Second",
        )
        .unwrap();
        install_packages(
            &env.ctx(),
            &second.id,
            &[request("Owner-Mod-1.0.0")],
            &repo,
            &|package, destination| repo.fetch(package, destination),
            ResolveMode::Latest,
            &|_| {},
        )
        .unwrap();
        assert_eq!(repo.fetch_count(), 2);
        assert!(
            env.paths
                .profile_dir(&env.game_id, &second.id)
                .unwrap()
                .join("BepInEx/plugins/Owner-Mod/Mod.dll")
                .is_file()
        );
    }

    #[test]
    fn same_version_is_skipped_and_dependencies_are_not_updated() {
        let env = Env::new();
        let repo = repo();
        env.install(&repo, &["Owner-Mod-1.0.0"], ResolveMode::Latest)
            .unwrap();
        let outcome = env
            .install(&repo, &["Owner-Mod-1.0.0"], ResolveMode::Latest)
            .unwrap();
        assert!(outcome.installed.is_empty());
        assert_eq!(outcome.skipped.len(), 1);

        let outcome = env
            .install(&repo, &["Other-Addon"], ResolveMode::Latest)
            .unwrap();
        let installed: Vec<_> = outcome.installed.iter().map(ToString::to_string).collect();
        assert_eq!(installed, ["Other-Addon-1.0.0"]);
        assert_eq!(
            env.mods()
                .iter()
                .find(|record| record.full_name == "Owner-Mod")
                .unwrap()
                .version,
            "1.0.0"
        );
    }

    #[test]
    fn updating_replaces_the_old_version_files() {
        let env = Env::new();
        let repo = repo();
        env.install(&repo, &["Owner-Mod-1.0.0"], ResolveMode::Latest)
            .unwrap();
        assert!(env.exists("BepInEx/plugins/Owner-Mod/sound.ogg"));
        let order_before = env.mods()[1].install_order;

        env.install(&repo, &["Owner-Mod-2.0.0"], ResolveMode::Latest)
            .unwrap();
        assert!(!env.exists("BepInEx/plugins/Owner-Mod/sound.ogg"));
        assert_eq!(
            fs::read(env.profile_dir().join("BepInEx/plugins/Owner-Mod/Mod.dll")).unwrap(),
            b"mod v2"
        );
        let mods = env.mods();
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[1].version, "2.0.0");
        assert_eq!(mods[1].install_order, order_before);
        assert_eq!(
            env.files_of("Owner-Mod"),
            [
                "BepInEx/plugins/Owner-Mod/Mod.dll",
                "BepInEx/plugins/Owner-Mod/manifest.json",
            ]
        );
    }

    #[test]
    fn existing_configuration_is_kept_on_update() {
        let env = Env::new();
        let repo = repo();
        env.install(&repo, &["Owner-Mod-1.0.0"], ResolveMode::Latest)
            .unwrap();
        let config = env.profile_dir().join("BepInEx/config/Mod.cfg");
        fs::write(&config, "user edited").unwrap();
        env.install(&repo, &["Owner-Mod-2.0.0"], ResolveMode::Latest)
            .unwrap();
        assert_eq!(fs::read(&config).unwrap(), b"user edited");
        assert!(
            !env.files_of("Owner-Mod")
                .iter()
                .any(|path| path.contains("config"))
        );
    }

    #[test]
    fn exact_mode_installs_the_listed_dependency_versions() {
        let env = Env::new();
        let mut repo = repo();
        repo.add(
            "Pinned-Pack-1.0.0",
            &["Owner-Mod-1.0.0"],
            &[("Pack.dll", "pack")],
        );
        let outcome = env
            .install(&repo, &["Pinned-Pack-1.0.0"], ResolveMode::Exact)
            .unwrap();
        let installed: Vec<_> = outcome.installed.iter().map(ToString::to_string).collect();
        assert_eq!(installed, [LOADER, "Owner-Mod-1.0.0", "Pinned-Pack-1.0.0"]);
        assert_eq!(
            fs::read(env.profile_dir().join("BepInEx/plugins/Owner-Mod/Mod.dll")).unwrap(),
            b"mod v1"
        );
    }

    #[test]
    fn dependency_cycles_install_and_warn() {
        let env = Env::new();
        let mut repo = FakeRepo::default();
        repo.add("A-One-1.0.0", &["B-Two-1.0.0"], &[("One.dll", "1")]);
        repo.add("B-Two-1.0.0", &["A-One-1.0.0"], &[("Two.dll", "2")]);
        let outcome = env.install(&repo, &["A-One"], ResolveMode::Latest).unwrap();
        assert_eq!(outcome.installed.len(), 2);
        assert_eq!(outcome.warnings.len(), 1);
    }

    #[test]
    fn a_tracked_file_of_another_mod_is_never_overwritten() {
        let env = state_game_env();
        let mut repo = FakeRepo::default();
        repo.add("A-First-1.0.0", &[], &[("shared.dll", "first")]);
        repo.add(
            "B-Second-1.0.0",
            &[],
            &[("shared.dll", "second"), ("extra.dll", "x")],
        );
        env.install(&repo, &["A-First"], ResolveMode::Latest)
            .unwrap();
        let error = env
            .install(&repo, &["B-Second"], ResolveMode::Latest)
            .unwrap_err();
        assert!(error.to_string().contains("already tracked"), "{error}");
        assert_eq!(
            fs::read(env.profile_dir().join("Mods/shared.dll")).unwrap(),
            b"first"
        );
        assert!(!env.exists("Mods/extra.dll"));
        assert_eq!(env.mods().len(), 1);
    }

    #[test]
    fn a_failed_install_rolls_back_its_files() {
        let env = Env::new();
        let repo = repo();
        env.install(&repo, &[LOADER], ResolveMode::Latest).unwrap();
        // A folder where a file belongs makes the second copy fail.
        fs::create_dir_all(
            env.profile_dir()
                .join("BepInEx/plugins/Owner-Mod/manifest.json"),
        )
        .unwrap();
        let before = env.mods();

        let error = env
            .install(&repo, &["Owner-Mod-1.0.0"], ResolveMode::Latest)
            .unwrap_err();
        assert!(error.to_string().contains("not a regular file"), "{error}");
        assert!(!env.exists("BepInEx/plugins/Owner-Mod/Mod.dll"));
        assert!(!env.exists("BepInEx/plugins/Owner-Mod/sound.ogg"));
        assert_eq!(env.mods(), before);
        assert!(env.files_of("Owner-Mod").is_empty());
        // The loader installed earlier is untouched.
        assert!(env.exists("winhttp.dll"));
    }

    #[test]
    fn a_failed_update_leaves_the_mod_uninstalled() {
        let env = Env::new();
        let repo = repo();
        env.install(&repo, &["Owner-Mod-1.0.0"], ResolveMode::Latest)
            .unwrap();
        fs::remove_file(
            env.profile_dir()
                .join("BepInEx/plugins/Owner-Mod/manifest.json"),
        )
        .unwrap();
        fs::create_dir_all(
            env.profile_dir()
                .join("BepInEx/plugins/Owner-Mod/manifest.json"),
        )
        .unwrap();
        env.install(&repo, &["Owner-Mod-2.0.0"], ResolveMode::Latest)
            .unwrap_err();
        assert!(
            env.mods()
                .iter()
                .all(|record| record.full_name != "Owner-Mod")
        );
        assert!(!env.exists("BepInEx/plugins/Owner-Mod/Mod.dll"));
    }

    #[test]
    fn download_failures_change_nothing() {
        let env = Env::new();
        let repo = repo();
        let requests = [request("Owner-Mod-1.0.0")];
        let error = install_packages(
            &env.ctx(),
            &env.profile.id,
            &requests,
            &repo,
            &|_, _| Err("offline".to_owned()),
            ResolveMode::Latest,
            &|_| {},
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "offline");
        assert!(env.mods().is_empty());
        assert_eq!(fs::read_dir(env.profile_dir()).unwrap().count(), 0);
    }

    #[test]
    fn uninstall_removes_files_and_empty_folders_only() {
        let env = Env::new();
        let repo = repo();
        env.install(&repo, &["Other-Addon"], ResolveMode::Latest)
            .unwrap();
        let config = env.profile_dir().join("BepInEx/config/Mod.cfg");
        assert!(config.is_file());

        let outcome = uninstall_mod(&env.ctx(), &env.profile.id, "Owner-Mod").unwrap();
        assert_eq!(outcome.dependents, ["Other-Addon"]);
        assert!(!env.exists("BepInEx/plugins/Owner-Mod"));
        assert!(env.exists("BepInEx/plugins/Other-Addon/Addon.dll"));
        assert!(env.exists("BepInEx/plugins"));
        assert!(config.is_file());
        assert!(
            env.mods()
                .iter()
                .all(|record| record.full_name != "Owner-Mod")
        );
        assert!(env.files_of("Owner-Mod").is_empty());

        uninstall_mod(&env.ctx(), &env.profile.id, "Other-Addon").unwrap();
        assert!(!env.exists("BepInEx/plugins/Other-Addon"));
        assert!(env.profile_dir().is_dir());
        assert!(uninstall_mod(&env.ctx(), &env.profile.id, "Other-Addon").is_err());
    }

    #[test]
    fn uninstall_tolerates_files_that_are_already_gone() {
        let env = Env::new();
        env.install(&repo(), &["Owner-Mod-1.0.0"], ResolveMode::Latest)
            .unwrap();
        fs::remove_file(env.profile_dir().join("BepInEx/plugins/Owner-Mod/Mod.dll")).unwrap();
        let outcome = uninstall_mod(&env.ctx(), &env.profile.id, "owner-mod").unwrap();
        assert!(outcome.dependents.is_empty());
        assert!(!env.exists("BepInEx/plugins/Owner-Mod"));
    }

    #[test]
    fn disable_and_enable_move_files_in_and_out() {
        let env = Env::new();
        env.install(&repo(), &["Owner-Mod-1.0.0"], ResolveMode::Latest)
            .unwrap();
        let tracked = env.files_of("Owner-Mod");

        set_mod_enabled(&env.ctx(), &env.profile.id, "Owner-Mod", false).unwrap();
        assert!(!env.exists("BepInEx/plugins/Owner-Mod"));
        assert!(env.exists("BepInEx/config/Mod.cfg"));
        assert!(
            !env.mods()
                .iter()
                .find(|r| r.full_name == "Owner-Mod")
                .unwrap()
                .enabled
        );
        assert_eq!(env.files_of("Owner-Mod"), tracked);
        // Disabling twice is a no-op.
        set_mod_enabled(&env.ctx(), &env.profile.id, "Owner-Mod", false).unwrap();

        set_mod_enabled(&env.ctx(), &env.profile.id, "Owner-Mod", true).unwrap();
        assert_eq!(
            fs::read(env.profile_dir().join("BepInEx/plugins/Owner-Mod/Mod.dll")).unwrap(),
            b"mod v1"
        );
        assert!(
            env.mods()
                .iter()
                .find(|r| r.full_name == "Owner-Mod")
                .unwrap()
                .enabled
        );
        assert_eq!(env.files_of("Owner-Mod"), tracked);
    }

    #[test]
    fn enabling_without_cached_files_returns_a_typed_error() {
        let env = Env::new();
        env.install(&repo(), &["Owner-Mod-1.0.0"], ResolveMode::Latest)
            .unwrap();
        set_mod_enabled(&env.ctx(), &env.profile.id, "Owner-Mod", false).unwrap();
        fs::remove_dir_all(env.paths.root().join("cache")).unwrap();

        let error = set_mod_enabled(&env.ctx(), &env.profile.id, "Owner-Mod", true).unwrap_err();
        assert_eq!(
            error,
            InstallError::CacheMissing {
                package: "Owner-Mod-1.0.0".parse().unwrap(),
                online_source: true,
            }
        );
        assert!(
            !env.mods()
                .iter()
                .find(|r| r.full_name == "Owner-Mod")
                .unwrap()
                .enabled
        );
        assert!(!env.exists("BepInEx/plugins/Owner-Mod"));
    }

    #[test]
    fn the_loader_cannot_be_disabled() {
        let env = Env::new();
        env.install(&repo(), &[LOADER], ResolveMode::Latest)
            .unwrap();
        let error =
            set_mod_enabled(&env.ctx(), &env.profile.id, "BepInEx-BepInExPack", false).unwrap_err();
        assert!(matches!(error, InstallError::LoaderCannotBeDisabled(_)));
        assert!(env.exists("winhttp.dll"));
        assert!(env.mods()[0].enabled);
    }

    #[test]
    fn unsupported_loaders_fail_clearly() {
        let env = Env::for_game("backpack-hero");
        let mut repo = FakeRepo::default();
        repo.add("LavaGang-MelonLoader-0.7.0", &[], &[("version.dll", "x")]);
        let error = env
            .install(&repo, &["LavaGang-MelonLoader"], ResolveMode::Latest)
            .unwrap_err();
        assert!(error.to_string().contains("not supported"), "{error}");
        assert!(env.mods().is_empty());
    }

    #[test]
    fn packages_with_only_a_manifest_are_recorded() {
        let env = Env::new();
        let mut repo = FakeRepo::default();
        repo.add("Owner-Empty-1.0.0", &[], &[("manifest.json", "{}")]);
        env.install(&repo, &["Owner-Empty"], ResolveMode::Latest)
            .unwrap();
        assert_eq!(env.mods().len(), 1);
        assert_eq!(
            env.files_of("Owner-Empty"),
            ["BepInEx/plugins/Owner-Empty/manifest.json"]
        );
    }

    #[test]
    fn destination_paths_cannot_leave_the_profile() {
        let root = Path::new("profile");
        assert_eq!(
            destination_path(root, "a/b.txt").unwrap(),
            root.join("a").join("b.txt")
        );
        for bad in [
            "", "../x", "a/../x", "/abs", "a//b", "a\\b", "C:/x", "a/./b",
        ] {
            assert!(destination_path(root, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn operations_on_missing_profiles_or_mods_fail() {
        let env = Env::new();
        let missing = uuid::Uuid::new_v4().to_string();
        assert!(uninstall_mod(&env.ctx(), &missing, "A-B").is_err());
        assert!(uninstall_mod(&env.ctx(), &env.profile.id, "A-B").is_err());
        assert!(set_mod_enabled(&env.ctx(), &env.profile.id, "A-B", false).is_err());
    }
}
