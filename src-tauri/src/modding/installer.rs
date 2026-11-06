//! Installs, updates, removes, disables and enables packages in a profile.
//! Network access is injected: package metadata comes from a
//! [`PackageSource`] and archives from a fetch function.
//!
//! All functions here block. Run them in `spawn_blocking`.

use std::{collections::HashMap, fmt, fs, path::Path};

use serde::Serialize;

use super::archive_cache::{CacheStage, FetchPackage, TempPath, ensure_cached, package_dir};
use super::dependencies::{InstallRequest, PackageSource, ResolveMode, package_key, resolve_plan};
use super::ecosystem::{EcosystemSchema, GameEntry, ModloaderPackage, PackageLoader};
use super::install_rules::{PlannedFile, plan_install};
use super::loader_install::{plan_loader_install, unsupported_loader_message};
use super::package_id::{PackageName, PackageVersionId, Version};
use super::package_manifest::read_manifest;
use super::paths::{ModdingPaths, join_relative};
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

impl<'a> ModdingContext<'a> {
    /// The loader package of the game's mod loader named `full_name`
    /// (`Owner-Name`), if it is one.
    pub(crate) fn find_loader(&self, full_name: &str) -> Option<&'a ModloaderPackage> {
        self.loaders.iter().find(|loader| {
            loader.loader == self.game.package_loader
                && loader.package_id.eq_ignore_ascii_case(full_name)
        })
    }
}

/// Install rules of one game, copied out of the shared ecosystem schema.
pub(crate) struct GameRules {
    entry: GameEntry,
    loaders: Vec<ModloaderPackage>,
}

impl GameRules {
    pub(crate) fn from_schema(
        schema: &EcosystemSchema,
        ecosystem_game_id: &str,
    ) -> Result<Self, String> {
        let entry = schema
            .games
            .get(ecosystem_game_id)
            .and_then(|game| game.game_entry())
            .cloned()
            .ok_or_else(|| format!("Thunderstore has no mod support for {ecosystem_game_id}"))?;
        Ok(Self {
            entry,
            loaders: schema.modloader_packages.clone(),
        })
    }

    pub(crate) fn context<'a>(
        &'a self,
        database: &'a Database,
        paths: &'a ModdingPaths,
        locks: &'a ProfileLocks,
    ) -> ModdingContext<'a> {
        ModdingContext {
            database,
            paths,
            locks,
            game: &self.entry,
            loaders: &self.loaders,
        }
    }
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
    /// Bytes received so far, only while downloading a package.
    pub(crate) downloaded_bytes: Option<u64>,
    /// Size announced by the server, when it announced one.
    pub(crate) total_bytes: Option<u64>,
    /// The `operationId` the caller passed to the command, if any.
    pub(crate) operation_id: Option<String>,
    /// True while cancelling can stop the work, which is only the download.
    pub(crate) cancellable: bool,
}

impl InstallProgress {
    pub(crate) fn new(
        profile_id: &str,
        package: Option<&PackageVersionId>,
        stage: InstallStage,
        position: (usize, usize),
    ) -> Self {
        Self {
            profile_id: profile_id.to_owned(),
            package: package.map(ToString::to_string),
            stage,
            current: position.0,
            total: position.1,
            downloaded_bytes: None,
            total_bytes: None,
            operation_id: None,
            cancellable: stage == InstallStage::Downloading,
        }
    }

    pub(crate) fn with_operation(&self, operation_id: Option<&str>) -> Self {
        Self {
            operation_id: operation_id.map(str::to_owned),
            ..self.clone()
        }
    }
}

pub(crate) type ProgressSink<'a> = &'a dyn Fn(&InstallProgress);

pub(crate) fn report_progress(
    progress: ProgressSink<'_>,
    profile_id: &str,
    package: Option<&PackageVersionId>,
    stage: InstallStage,
    position: (usize, usize),
) {
    progress(&InstallProgress::new(profile_id, package, stage, position));
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
            Self::CacheMissing {
                package,
                online_source: true,
            } => write!(
                formatter,
                "The stored files of {package} are missing and must be downloaded again"
            ),
            Self::CacheMissing {
                package,
                online_source: false,
            } => write!(
                formatter,
                "The stored files of {package} are missing. Import the archive again"
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
/// version of the same package is replaced: its files are moved aside and
/// deleted only once the new version is in place. A disabled mod stays
/// disabled. On failure a new package ends up not installed and an update
/// keeps the old version.
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
    let record = ProfileMod {
        profile_id: profile.id.clone(),
        full_name: full_name.clone(),
        version: id.version.to_string(),
        enabled: existing.is_none_or(|record| record.enabled),
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

    // Rejects files owned by another mod before anything is written.
    ctx.database
        .replace_profile_mod_files(&profile.id, &full_name, &tracked_paths(&planned))?;
    if !record.enabled {
        // A disabled mod has no files in the profile, so only its records
        // change; enabling it copies the new version.
        if let Err(error) = ctx.database.upsert_profile_mod(&record) {
            restore_tracked_files(ctx, &profile.id, &full_name, &old_files);
            return Err(InstallError::Failed(error));
        }
        store_dependencies(ctx, &profile.id, &full_name, cache_dir);
        return Ok(record);
    }
    let set_aside = match SetAside::move_files(
        ctx.paths,
        profile_dir,
        &without_untracked(old_files.clone(), &planned),
    ) {
        Ok(set_aside) => set_aside,
        Err(error) => {
            restore_tracked_files(ctx, &profile.id, &full_name, &old_files);
            return Err(InstallError::Failed(error));
        }
    };
    let installed = copy_planned(profile_dir, &planned)
        .map_err(|failure| (failure.written, failure.error))
        .and_then(|written| {
            ctx.database
                .upsert_profile_mod(&record)
                .map_err(|error| (written, error))
        });
    if let Err((written, error)) = installed {
        let cleanup = if existing.is_some() {
            let mut problems = Vec::new();
            if let Err(failure) = remove_files(profile_dir, &written) {
                problems.push(failure.error);
            }
            problems.extend(set_aside.restore(profile_dir));
            restore_tracked_files(ctx, &profile.id, &full_name, &old_files);
            cleanup_message("restore_mod_update", problems)
        } else {
            discard_install(ctx, profile, profile_dir, &full_name, &written)
        };
        return Err(InstallError::Failed(append(error, cleanup)));
    }
    set_aside.discard(profile_dir);
    store_dependencies(ctx, &profile.id, &full_name, cache_dir);
    Ok(record)
}

/// Files of the installed version moved out of the profile during an update,
/// so that a failed update can put them back.
struct SetAside {
    backup: TempPath,
    moved: Vec<String>,
}

impl SetAside {
    /// Moves the files that exist into a temporary folder. When one cannot be
    /// moved, the others are put back.
    fn move_files(
        paths: &ModdingPaths,
        profile_dir: &Path,
        files: &[String],
    ) -> Result<Self, String> {
        let mut set_aside = Self {
            backup: TempPath::new(paths, "-backup")?,
            moved: Vec::new(),
        };
        for relative in files {
            if let Err(error) = set_aside.move_file(profile_dir, relative) {
                let error = format!("{error}. Close the game if it is running and try again");
                let problems = set_aside.restore(profile_dir);
                return Err(append(
                    error,
                    cleanup_message("restore_mod_files", problems),
                ));
            }
        }
        Ok(set_aside)
    }

    fn move_file(&mut self, profile_dir: &Path, relative: &str) -> Result<(), String> {
        let source = join_relative(profile_dir, relative)?;
        match fs::symlink_metadata(&source) {
            Ok(metadata) if metadata.is_dir() => {
                return Err(format!("Cannot replace {relative}: it is a folder"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("Cannot inspect {relative}: {error}")),
        }
        let target = join_relative(self.backup.path(), relative)?;
        rename_into(&source, &target)
            .map_err(|error| format!("Cannot replace {relative}: {error}"))?;
        self.moved.push(relative.to_owned());
        Ok(())
    }

    /// Moves the files back into the profile and returns the problems. When
    /// a file cannot be moved back, the temporary folder is kept and named.
    fn restore(self, profile_dir: &Path) -> Vec<String> {
        let mut problems = Vec::new();
        for relative in &self.moved {
            let restored = join_relative(self.backup.path(), relative).and_then(|source| {
                rename_into(&source, &join_relative(profile_dir, relative)?)
                    .map_err(|error| format!("Cannot put back {relative}: {error}"))
            });
            if let Err(error) = restored {
                problems.push(error);
            }
        }
        if !problems.is_empty() {
            let kept = self.backup.keep();
            problems.push(format!("the previous files are kept in {}", kept.display()));
        }
        problems
    }

    /// Deletes the moved files once the new version is in place.
    fn discard(self, profile_dir: &Path) {
        prune_empty_directories(profile_dir, &self.moved);
    }
}

fn rename_into(source: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::rename(source, target).map_err(|error| error.to_string())
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
    let dependencies = declared_dependencies(ctx.database, ctx.paths, &mods);
    let dependents = find_dependents(&mods, &dependencies, record);
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
    toggle_mod(ctx, &profile, &profile_dir, record, enabled)
}

/// The profile lock must be held by the caller.
fn toggle_mod(
    ctx: &ModdingContext<'_>,
    profile: &ModProfile,
    profile_dir: &Path,
    record: &ProfileMod,
    enabled: bool,
) -> Result<(), InstallError> {
    if record.enabled == enabled {
        return Ok(());
    }
    let id = version_id(record)?;
    if enabled {
        enable_mod(ctx, profile, profile_dir, record, &id)
    } else {
        if ctx.find_loader(&id.package.to_string()).is_some() {
            return Err(InstallError::LoaderCannotBeDisabled(id.package.to_string()));
        }
        let files = ctx
            .database
            .profile_mod_files(&profile.id, &record.full_name)?;
        remove_files(profile_dir, &files).map_err(|failure| InstallError::Failed(failure.error))?;
        ctx.database
            .set_profile_mod_enabled(&profile.id, &record.full_name, false)?;
        Ok(())
    }
}

/// A mod that [`set_all_mods_enabled`] could not switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToggleFailure {
    pub(crate) full_name: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToggleAllOutcome {
    /// Mods that were switched.
    pub(crate) changed: usize,
    pub(crate) failed: Vec<ToggleFailure>,
}

/// Enables or disables every mod of the profile except the loader under one
/// hold of the profile lock. Each mod is switched like in
/// [`set_mod_enabled`], so a failing one stays as it was and the others are
/// still processed. Enabling goes in install order, disabling in reverse.
/// `restore_cache` downloads a lost package again.
pub(crate) fn set_all_mods_enabled(
    ctx: &ModdingContext<'_>,
    profile_id: &str,
    enabled: bool,
    mut restore_cache: impl FnMut(&PackageVersionId) -> Result<(), String>,
) -> Result<ToggleAllOutcome, InstallError> {
    let (profile, profile_dir) = profile_with_directory(ctx.database, ctx.paths, profile_id)?;
    let _guard = ctx.locks.lock(&profile.id)?;
    let mut mods = ctx.database.profile_mods(&profile.id)?;
    if !enabled {
        mods.reverse();
    }
    let mut outcome = ToggleAllOutcome {
        changed: 0,
        failed: Vec::new(),
    };
    for record in &mods {
        if record.enabled == enabled || ctx.find_loader(&record.full_name).is_some() {
            continue;
        }
        let result = match toggle_mod(ctx, &profile, &profile_dir, record, enabled) {
            Err(InstallError::CacheMissing {
                package,
                online_source: true,
            }) => restore_cache(&package)
                .map_err(InstallError::Failed)
                .and_then(|()| toggle_mod(ctx, &profile, &profile_dir, record, enabled)),
            result => result,
        };
        match result {
            Ok(()) => outcome.changed += 1,
            Err(error) => outcome.failed.push(ToggleFailure {
                full_name: record.full_name.clone(),
                message: error.to_string(),
            }),
        }
    }
    Ok(outcome)
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

fn find_dependents(
    mods: &[ProfileMod],
    dependencies: &HashMap<String, Vec<String>>,
    target: &ProfileMod,
) -> Vec<String> {
    mods.iter()
        .filter(|other| !other.full_name.eq_ignore_ascii_case(&target.full_name))
        .filter(|other| {
            dependencies
                .get(&other.full_name.to_lowercase())
                .is_some_and(|declared| {
                    declared
                        .iter()
                        .any(|package| package.eq_ignore_ascii_case(&target.full_name))
                })
        })
        .map(|other| other.full_name.clone())
        .collect()
}

/// The packages (`Owner-Name`, no version, no duplicates) a package directory
/// declares as dependencies. `None` when the directory is gone or its
/// manifest cannot be read.
fn read_package_dependencies(package_dir: &Path) -> Option<Vec<String>> {
    if !package_dir.is_dir() {
        return None;
    }
    let manifest = match read_manifest(package_dir) {
        Ok(manifest) => manifest,
        Err(error) => {
            crate::application_log::failure("read_mod_dependencies", error);
            return None;
        }
    };
    let mut packages: Vec<String> = Vec::new();
    for dependency in manifest
        .map(|manifest| manifest.dependencies)
        .unwrap_or_default()
    {
        let Ok(id) = dependency.parse::<PackageVersionId>() else {
            continue;
        };
        let package = id.package.to_string();
        if !packages
            .iter()
            .any(|known| known.eq_ignore_ascii_case(&package))
        {
            packages.push(package);
        }
    }
    Some(packages)
}

/// Stores the dependencies a freshly installed package declares, so they stay
/// known when the package cache is cleared. A failure is logged only: the
/// list is read from the cache again when it is missing.
fn store_dependencies(ctx: &ModdingContext<'_>, profile_id: &str, full_name: &str, dir: &Path) {
    let dependencies = read_package_dependencies(dir);
    if let Err(error) =
        ctx.database
            .set_profile_mod_dependencies(profile_id, full_name, dependencies.as_deref())
    {
        crate::application_log::failure("store_mod_dependencies", error);
    }
}

/// The packages (`Owner-Name`) each installed mod depends on, keyed by
/// lowercase `full_name`. They are stored at install time, so they survive
/// clearing the package cache; mods installed before that are read from the
/// cache and stored. A mod with neither reports none.
pub(crate) fn declared_dependencies(
    database: &Database,
    paths: &ModdingPaths,
    mods: &[ProfileMod],
) -> HashMap<String, Vec<String>> {
    let Some(first) = mods.first() else {
        return HashMap::new();
    };
    let mut known = database
        .profile_mod_dependencies(&first.profile_id)
        .unwrap_or_else(|error| {
            crate::application_log::failure("read_mod_dependencies", error);
            HashMap::new()
        });
    for record in mods {
        let key = record.full_name.to_lowercase();
        if known.contains_key(&key) {
            continue;
        }
        let Ok(id) = version_id(record) else {
            continue;
        };
        let Some(dependencies) =
            read_package_dependencies(&package_dir(paths, &id, record.online_source))
        else {
            continue;
        };
        if let Err(error) = database.set_profile_mod_dependencies(
            &record.profile_id,
            &record.full_name,
            Some(&dependencies),
        ) {
            crate::application_log::failure("store_mod_dependencies", error);
        }
        known.insert(key, dependencies);
    }
    known
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

fn plan_package(
    ctx: &ModdingContext<'_>,
    package: &PackageName,
    cache_dir: &Path,
) -> Result<Vec<PlannedFile>, InstallError> {
    let planned = match ctx.find_loader(&package.to_string()) {
        Some(loader) if loader.loader == PackageLoader::Bepinex => {
            plan_loader_install(cache_dir, &loader.root_folder)
        }
        Some(loader) => {
            return Err(InstallError::Failed(unsupported_loader_message(
                loader.loader,
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

/// Drops the files the new plan leaves untracked: a file an older version
/// tracked but the new one treats as configuration stays with the user.
fn without_untracked(files: Vec<String>, planned: &[PlannedFile]) -> Vec<String> {
    files
        .into_iter()
        .filter(|path| {
            !planned
                .iter()
                .any(|file| !file.tracked && file.destination == *path)
        })
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
        let target = match join_relative(profile_dir, &file.destination) {
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
            join_relative(profile_dir, relative).and_then(|path| match fs::remove_file(&path) {
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
        let Ok(path) = join_relative(profile_dir, relative) else {
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
    cleanup_message("discard_mod_install", problems)
}

/// Logs the problems of undoing a failed change and describes them.
fn cleanup_message(operation: &'static str, problems: Vec<String>) -> Option<String> {
    if problems.is_empty() {
        return None;
    }
    let message = problems.join("; ");
    crate::application_log::failure(operation, &message);
    Some(format!("cleanup also failed: {message}"))
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
        assert_eq!(json["cancellable"], true);
        assert_eq!(json["operationId"], serde_json::Value::Null);
        let tagged = serde_json::to_value(events[1].with_operation(Some("op-1"))).unwrap();
        assert_eq!(tagged["operationId"], "op-1");
        let cancellable: Vec<_> = events.iter().map(|event| event.cancellable).collect();
        assert_eq!(
            cancellable,
            [false, true, false, false, true, false, false, false]
        );
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
        assert_eq!(fs::read_dir(env.paths.temp_dir()).unwrap().count(), 0);
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
    fn updating_a_disabled_mod_keeps_it_disabled() {
        let env = Env::new();
        let repo = repo();
        env.install(&repo, &["Owner-Mod-1.0.0"], ResolveMode::Latest)
            .unwrap();
        set_mod_enabled(&env.ctx(), &env.profile.id, "Owner-Mod", false).unwrap();

        env.install(&repo, &["Owner-Mod-2.0.0"], ResolveMode::Latest)
            .unwrap();
        let record = env.mods().remove(1);
        assert_eq!((record.version.as_str(), record.enabled), ("2.0.0", false));
        assert!(!env.exists("BepInEx/plugins/Owner-Mod"));

        set_mod_enabled(&env.ctx(), &env.profile.id, "Owner-Mod", true).unwrap();
        assert_eq!(
            fs::read(env.profile_dir().join("BepInEx/plugins/Owner-Mod/Mod.dll")).unwrap(),
            b"mod v2"
        );
        assert!(!env.exists("BepInEx/plugins/Owner-Mod/sound.ogg"));
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

    fn loader_repo() -> FakeRepo {
        let mut repo = FakeRepo::default();
        for (id, core) in [
            (LOADER, "core 1"),
            ("BepInEx-BepInExPack-5.4.2200", "core 2"),
        ] {
            repo.add(
                id,
                &[],
                &[
                    ("BepInExPack/BepInEx/core/BepInEx.dll", core),
                    ("BepInExPack/BepInEx/config/BepInEx.cfg", "default"),
                ],
            );
        }
        repo
    }

    #[test]
    fn loader_update_and_uninstall_keep_its_config() {
        let env = Env::new();
        let repo = loader_repo();
        env.install(&repo, &[LOADER], ResolveMode::Latest).unwrap();
        let config = env.profile_dir().join("BepInEx/config/BepInEx.cfg");
        assert_eq!(fs::read(&config).unwrap(), b"default");
        assert_eq!(
            env.files_of("BepInEx-BepInExPack"),
            ["BepInEx/core/BepInEx.dll"]
        );
        fs::write(&config, "user edited").unwrap();

        env.install(&repo, &["BepInEx-BepInExPack"], ResolveMode::Latest)
            .unwrap();
        assert_eq!(
            fs::read(env.profile_dir().join("BepInEx/core/BepInEx.dll")).unwrap(),
            b"core 2"
        );
        assert_eq!(fs::read(&config).unwrap(), b"user edited");

        uninstall_mod(&env.ctx(), &env.profile.id, "BepInEx-BepInExPack").unwrap();
        assert!(!env.exists("BepInEx/core/BepInEx.dll"));
        assert_eq!(fs::read(&config).unwrap(), b"user edited");
    }

    #[test]
    fn loader_update_keeps_a_config_tracked_by_older_installs() {
        let env = Env::new();
        let repo = loader_repo();
        env.install(&repo, &[LOADER], ResolveMode::Latest).unwrap();
        // Older installs tracked the loader config like any loader file.
        env.database
            .replace_profile_mod_files(
                &env.profile.id,
                "BepInEx-BepInExPack",
                &[
                    "BepInEx/config/BepInEx.cfg".to_owned(),
                    "BepInEx/core/BepInEx.dll".to_owned(),
                ],
            )
            .unwrap();
        let config = env.profile_dir().join("BepInEx/config/BepInEx.cfg");
        fs::write(&config, "user edited").unwrap();

        env.install(&repo, &["BepInEx-BepInExPack"], ResolveMode::Latest)
            .unwrap();
        assert_eq!(fs::read(&config).unwrap(), b"user edited");
        assert_eq!(
            env.files_of("BepInEx-BepInExPack"),
            ["BepInEx/core/BepInEx.dll"]
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
    fn a_failed_update_keeps_the_old_version() {
        let env = Env::new();
        let mut repo = FakeRepo::default();
        repo.add(
            "Owner-Wide-1.0.0",
            &[],
            &[("b.dll", "b1"), ("c.dll", "c1"), ("d.dll", "d1")],
        );
        repo.add(
            "Owner-Wide-2.0.0",
            &[],
            &[
                ("a.dll", "a2"),
                ("b.dll", "b2"),
                ("c.dll", "c2"),
                ("e.dll", "e2"),
            ],
        );
        env.install(&repo, &["Owner-Wide-1.0.0"], ResolveMode::Latest)
            .unwrap();
        let before = env.mods();
        let tracked = env.files_of("Owner-Wide");
        // A folder where the last new file belongs makes the copy fail after
        // the old files were replaced.
        let blocker = env.profile_dir().join("BepInEx/plugins/Owner-Wide/e.dll");
        fs::create_dir_all(&blocker).unwrap();

        env.install(&repo, &["Owner-Wide-2.0.0"], ResolveMode::Latest)
            .unwrap_err();
        assert_eq!(env.mods(), before);
        assert_eq!(env.files_of("Owner-Wide"), tracked);
        for (name, content) in [("b.dll", "b1"), ("c.dll", "c1"), ("d.dll", "d1")] {
            let path = env
                .profile_dir()
                .join(format!("BepInEx/plugins/Owner-Wide/{name}"));
            assert_eq!(fs::read_to_string(path).unwrap(), content);
        }
        assert!(!env.exists("BepInEx/plugins/Owner-Wide/a.dll"));
        assert_eq!(fs::read_dir(env.paths.temp_dir()).unwrap().count(), 0);
    }

    #[test]
    fn a_blocked_removal_keeps_the_old_version_recorded() {
        let env = Env::new();
        let repo = repo();
        env.install(&repo, &["Owner-Mod-1.0.0"], ResolveMode::Latest)
            .unwrap();
        let tracked = env.files_of("Owner-Mod");
        let blocker = env.profile_dir().join("BepInEx/plugins/Owner-Mod/Mod.dll");
        fs::remove_file(&blocker).unwrap();
        fs::create_dir_all(&blocker).unwrap();

        env.install(&repo, &["Owner-Mod-2.0.0"], ResolveMode::Latest)
            .unwrap_err();
        let record = env
            .mods()
            .into_iter()
            .find(|record| record.full_name == "Owner-Mod")
            .unwrap();
        assert_eq!(record.version, "1.0.0");
        assert_eq!(env.files_of("Owner-Mod"), tracked);
        assert!(env.exists("BepInEx/plugins/Owner-Mod/sound.ogg"));
        assert!(env.exists("BepInEx/plugins/Owner-Mod/manifest.json"));
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

    fn enabled_flags(env: &Env) -> Vec<(String, bool)> {
        env.mods()
            .into_iter()
            .map(|record| (record.full_name, record.enabled))
            .collect()
    }

    #[test]
    fn bulk_toggle_switches_every_mod_but_the_loader() {
        let env = Env::new();
        env.install(&repo(), &["Other-Addon-1.0.0"], ResolveMode::Latest)
            .unwrap();
        let ctx = env.ctx();

        let outcome = set_all_mods_enabled(&ctx, &env.profile.id, false, |_| Ok(())).unwrap();
        assert_eq!((outcome.changed, outcome.failed.len()), (2, 0));
        assert_eq!(
            enabled_flags(&env),
            [
                ("BepInEx-BepInExPack".to_owned(), true),
                ("Owner-Mod".to_owned(), false),
                ("Other-Addon".to_owned(), false),
            ]
        );
        assert!(!env.exists("BepInEx/plugins/Owner-Mod"));
        assert!(!env.exists("BepInEx/plugins/Other-Addon"));
        assert!(env.exists("winhttp.dll"));

        let outcome = set_all_mods_enabled(&ctx, &env.profile.id, true, |_| Ok(())).unwrap();
        assert_eq!((outcome.changed, outcome.failed.len()), (2, 0));
        assert!(enabled_flags(&env).iter().all(|(_, enabled)| *enabled));
        assert!(env.exists("BepInEx/plugins/Owner-Mod/Mod.dll"));
        assert!(env.exists("BepInEx/plugins/Other-Addon/Addon.dll"));

        // Nothing left to switch.
        let outcome = set_all_mods_enabled(&ctx, &env.profile.id, true, |_| Ok(())).unwrap();
        assert_eq!((outcome.changed, outcome.failed.len()), (0, 0));
    }

    #[test]
    fn bulk_enable_reports_mods_that_cannot_be_restored() {
        let env = Env::new();
        env.install(&repo(), &["Other-Addon-1.0.0"], ResolveMode::Latest)
            .unwrap();
        let ctx = env.ctx();
        set_all_mods_enabled(&ctx, &env.profile.id, false, |_| Ok(())).unwrap();
        let lost = package_dir(&env.paths, &"Owner-Mod-2.0.0".parse().unwrap(), true);
        fs::remove_dir_all(&lost).unwrap();

        let restored = RefCell::new(Vec::new());
        let outcome = set_all_mods_enabled(&ctx, &env.profile.id, true, |package| {
            restored.borrow_mut().push(package.to_string());
            Err("offline".to_owned())
        })
        .unwrap();

        assert_eq!(restored.into_inner(), ["Owner-Mod-2.0.0"]);
        assert_eq!(outcome.changed, 1);
        assert_eq!(
            outcome.failed,
            [ToggleFailure {
                full_name: "Owner-Mod".to_owned(),
                message: "offline".to_owned(),
            }]
        );
        // The failed mod is still disabled and has no files; the other one is on.
        assert_eq!(
            enabled_flags(&env),
            [
                ("BepInEx-BepInExPack".to_owned(), true),
                ("Owner-Mod".to_owned(), false),
                ("Other-Addon".to_owned(), true),
            ]
        );
        assert!(!env.exists("BepInEx/plugins/Owner-Mod"));
        assert!(env.exists("BepInEx/plugins/Other-Addon/Addon.dll"));
    }

    #[test]
    fn bulk_toggle_on_a_missing_profile_fails() {
        let env = Env::new();
        let missing = uuid::Uuid::new_v4().to_string();
        assert!(set_all_mods_enabled(&env.ctx(), &missing, false, |_| Ok(())).is_err());
    }

    #[test]
    fn declared_dependencies_survive_clearing_the_cache() {
        let env = Env::new();
        env.install(&repo(), &["Other-Addon-1.0.0"], ResolveMode::Latest)
            .unwrap();
        let declared = |name: &str| {
            let mods = env.mods();
            declared_dependencies(&env.database, &env.paths, &mods)
                .remove(&name.to_lowercase())
                .unwrap_or_default()
        };
        assert_eq!(declared("Other-Addon"), ["Owner-Mod"]);
        assert_eq!(declared("Owner-Mod"), ["BepInEx-BepInExPack"]);
        assert!(declared("BepInEx-BepInExPack").is_empty());

        fs::remove_dir_all(env.paths.root().join("cache")).unwrap();
        assert_eq!(declared("Other-Addon"), ["Owner-Mod"]);
        assert_eq!(declared("Owner-Mod"), ["BepInEx-BepInExPack"]);

        // The dependents of an uninstall come from the stored lists too.
        let outcome = uninstall_mod(&env.ctx(), &env.profile.id, "Owner-Mod").unwrap();
        assert_eq!(outcome.dependents, ["Other-Addon"]);
    }

    #[test]
    fn dependencies_of_older_installs_are_read_from_the_cache_once() {
        let env = Env::new();
        env.install(&repo(), &["Other-Addon-1.0.0"], ResolveMode::Latest)
            .unwrap();
        for record in env.mods() {
            env.database
                .set_profile_mod_dependencies(&env.profile.id, &record.full_name, None)
                .unwrap();
        }
        assert!(
            env.database
                .profile_mod_dependencies(&env.profile.id)
                .unwrap()
                .is_empty()
        );

        let mods = env.mods();
        let known = declared_dependencies(&env.database, &env.paths, &mods);
        assert_eq!(known["other-addon"], ["Owner-Mod"]);
        fs::remove_dir_all(env.paths.root().join("cache")).unwrap();
        let stored = env
            .database
            .profile_mod_dependencies(&env.profile.id)
            .unwrap();
        assert_eq!(stored["other-addon"], ["Owner-Mod"]);
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
    fn operations_on_missing_profiles_or_mods_fail() {
        let env = Env::new();
        let missing = uuid::Uuid::new_v4().to_string();
        assert!(uninstall_mod(&env.ctx(), &missing, "A-B").is_err());
        assert!(uninstall_mod(&env.ctx(), &env.profile.id, "A-B").is_err());
        assert!(set_mod_enabled(&env.ctx(), &env.profile.id, "A-B", false).is_err());
    }
}
