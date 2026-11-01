//! Commands that move mod profiles in and out of Legio as `.r2z` files and
//! Thunderstore profile codes.

use std::{fs, io, path::PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::catalog_commands::sync_catalog;
use super::dependencies::{InstallRequest, ResolveMode};
use super::ecosystem::EcosystemSchema;
use super::ecosystem_store::EcosystemStore;
use super::install_commands::install_from_catalog;
use super::installer::{GameRules, InstallError, InstallOutcome, ModdingContext, set_mod_enabled};
use super::package_cache::PackageCache;
use super::package_id::PackageVersionId;
use super::paths::ModdingPaths;
use super::profile_codes::{DEFAULT_BASE_URL, create_profile_code, fetch_profile_code};
use super::profile_commands::{require_profile_not_running, run_modding};
use super::profile_exchange::{
    ExportedMod, ProfileImport, apply_import_files, export_profile, export_profile_to_file,
    read_r2z, read_r2z_file,
};
use super::profiles::{create_profile, delete_profile, profile_for_game};
use super::thunderstore_http::ThunderstoreHttp;
use crate::database::{Database, MAX_PROFILE_NAME_CHARS, ModProfile};

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileImportOutcome {
    profile: ModProfile,
    /// `Owner-Name-1.2.3` of the mods installed in the new profile.
    installed: Vec<String>,
    /// Archive entries that were not imported because they are not allowed
    /// config files in the BepInEx config folder.
    skipped_files: Vec<String>,
    /// Mods of the export that Thunderstore no longer has, in `Owner-Name-1.2.3` form.
    missing_packages: Vec<String>,
    warnings: Vec<String>,
}

/// What an export contains. Mods imported from local archives are left out:
/// nobody else can download them.
struct ExportInput {
    profile: ModProfile,
    directory: PathBuf,
    mods: Vec<ExportedMod>,
}

fn export_input(
    database: &Database,
    paths: &ModdingPaths,
    game_id: &str,
    profile_id: &str,
) -> Result<ExportInput, String> {
    let (profile, directory) = profile_for_game(database, paths, game_id, profile_id)?;
    let mods = database
        .profile_mods(&profile.id)?
        .into_iter()
        .filter(|entry| entry.online_source)
        .map(|entry| {
            Ok(ExportedMod {
                package: entry.full_name.parse()?,
                version: entry.version.parse()?,
                enabled: entry.enabled,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ExportInput {
        profile,
        directory,
        mods,
    })
}

fn require_absolute(path: &str, what: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(format!("{what} paths must be absolute"))
    }
}

#[tauri::command]
pub async fn export_mod_profile_file(
    app: AppHandle,
    game_id: String,
    profile_id: String,
    destination_path: String,
) -> Result<(), String> {
    let destination = require_absolute(&destination_path, "Profile export")?;
    run_modding(app, move |_, database, paths, _| {
        let input = export_input(database, paths, &game_id, &profile_id)?;
        export_profile_to_file(
            &input.directory,
            &input.profile.name,
            &input.mods,
            &destination,
        )
    })
    .await
}

/// Uploads the profile to Thunderstore and returns the code others can import.
#[tauri::command]
pub async fn share_mod_profile_code(
    app: AppHandle,
    game_id: String,
    profile_id: String,
) -> Result<String, String> {
    let archive = run_modding(app.clone(), move |_, database, paths, _| {
        let input = export_input(database, paths, &game_id, &profile_id)?;
        export_profile(&input.directory, &input.profile.name, &input.mods)
    })
    .await?;
    let http = app.state::<ThunderstoreHttp>().inner().clone();
    create_profile_code(&http, DEFAULT_BASE_URL, &archive).await
}

/// Imports an `.r2z` file. Without `target_profile_id` a new profile is
/// created, named `profile_name` or after the file, made unique when the game
/// already has it. With it, that profile is updated instead (see
/// [`update_profile`]) and `profile_name` is ignored.
#[tauri::command]
pub async fn import_mod_profile_file(
    app: AppHandle,
    game_id: String,
    path: String,
    profile_name: Option<String>,
    operation_id: Option<String>,
    target_profile_id: Option<String>,
) -> Result<ProfileImportOutcome, String> {
    crate::startup_recovery::wait(&app).await?;
    let path = require_absolute(&path, "Profile import")?;
    let import = tauri::async_runtime::spawn_blocking(move || read_r2z_file(&path))
        .await
        .map_err(|error| format!("Profile file task failed: {error}"))??;
    import_profile_into_game(
        app,
        game_id,
        import,
        profile_name,
        target_profile_id,
        operation_id,
    )
    .await
}

/// Imports a Thunderstore profile code, like [`import_mod_profile_file`].
#[tauri::command]
pub async fn import_mod_profile_code(
    app: AppHandle,
    game_id: String,
    code: String,
    profile_name: Option<String>,
    operation_id: Option<String>,
    target_profile_id: Option<String>,
) -> Result<ProfileImportOutcome, String> {
    crate::startup_recovery::wait(&app).await?;
    let http = app.state::<ThunderstoreHttp>().inner().clone();
    let archive = fetch_profile_code(&http, DEFAULT_BASE_URL, &code).await?;
    let import = tauri::async_runtime::spawn_blocking(move || read_r2z(&archive))
        .await
        .map_err(|error| format!("Profile data task failed: {error}"))??;
    import_profile_into_game(
        app,
        game_id,
        import,
        profile_name,
        target_profile_id,
        operation_id,
    )
    .await
}

async fn import_profile_into_game(
    app: AppHandle,
    game_id: String,
    import: ProfileImport,
    profile_name: Option<String>,
    target_profile_id: Option<String>,
    operation_id: Option<String>,
) -> Result<ProfileImportOutcome, String> {
    let schema = app.state::<EcosystemStore>().schema().await?;
    let refresh_warning = refresh_catalog_for_import(&app, &schema, &game_id, &import).await?;
    let mut outcome = run_modding(app, move |app, database, paths, locks| {
        let modding = database
            .game_modding(&game_id)?
            .ok_or_else(|| "The game is not set up for mods".to_owned())?;
        let community = modding.ecosystem_game_id;
        let rules = GameRules::from_schema(&schema, &community)?;
        let ctx = rules.context(database, paths, locks);
        let catalog = app.state::<PackageCache>();
        let is_available = |id: &PackageVersionId| Ok(catalog.version(&community, id)?.is_some());
        let install = |requests: &[InstallRequest], profile_id: &str| {
            install_from_catalog(
                app,
                &ctx,
                &community,
                profile_id,
                operation_id.as_deref(),
                requests,
                ResolveMode::Exact,
            )
        };
        match target_profile_id {
            None => import_profile(
                &ctx,
                &game_id,
                import,
                profile_name.as_deref(),
                is_available,
                install,
            ),
            Some(target) => update_profile(
                &ctx,
                &game_id,
                &target,
                import,
                is_available,
                install,
                || require_profile_not_running(app, database, paths, &target),
            ),
        }
    })
    .await?;
    if let Some(warning) = refresh_warning {
        outcome.warnings.insert(0, warning);
    }
    Ok(outcome)
}

/// How much of an import the local catalog can resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogCoverage {
    /// Never downloaded.
    Missing,
    /// Downloaded, but without some mod or version of the import.
    Incomplete,
    Complete,
}

fn catalog_coverage(
    is_synced: bool,
    wanted: &[PackageVersionId],
    is_available: impl Fn(&PackageVersionId) -> Result<bool, String>,
) -> Result<CatalogCoverage, String> {
    if !is_synced {
        return Ok(CatalogCoverage::Missing);
    }
    for id in wanted {
        if !is_available(id)? {
            return Ok(CatalogCoverage::Incomplete);
        }
    }
    Ok(CatalogCoverage::Complete)
}

/// A failed refresh only stops the import when there is no catalog at all.
/// Otherwise the import goes on with the older catalog and says so.
fn refresh_failure(coverage: CatalogCoverage, error: String) -> Result<Option<String>, String> {
    if coverage == CatalogCoverage::Missing {
        Err(error)
    } else {
        Ok(Some(format!(
            "The mod catalog could not be refreshed, so recent mods may be reported as missing: {error}"
        )))
    }
}

/// Refreshes the game's catalog once when it was never downloaded or lacks a
/// mod of the import, so the import does not report mods as missing just
/// because the local copy is old. Returns a warning when an existing catalog
/// could not be refreshed.
async fn refresh_catalog_for_import(
    app: &AppHandle,
    schema: &EcosystemSchema,
    game_id: &str,
    import: &ProfileImport,
) -> Result<Option<String>, String> {
    let ids: Vec<PackageVersionId> = import.mods.iter().map(|entry| entry.id.clone()).collect();
    let game_id = game_id.to_owned();
    let (community, coverage) = run_modding(app.clone(), move |app, database, _, _| {
        let community = database
            .game_modding(&game_id)?
            .ok_or_else(|| "The game is not set up for mods".to_owned())?
            .ecosystem_game_id;
        let catalog = app.state::<PackageCache>();
        let coverage = catalog_coverage(catalog.status(&community)?.is_some(), &ids, |id| {
            Ok(catalog.version(&community, id)?.is_some())
        })?;
        Ok((community, coverage))
    })
    .await?;
    if coverage == CatalogCoverage::Complete {
        return Ok(None);
    }
    let packages = app.state::<PackageCache>();
    let http = app.state::<ThunderstoreHttp>();
    match sync_catalog(app, schema, &packages, &http, &community, true).await {
        Ok(_) => Ok(None),
        Err(error) => refresh_failure(coverage, error),
    }
}

/// Builds a profile from `import`. Mods that `is_available` rejects are
/// reported instead of installed. A failure removes the half-built profile.
fn import_profile(
    ctx: &ModdingContext<'_>,
    game_id: &str,
    import: ProfileImport,
    requested_name: Option<&str>,
    is_available: impl Fn(&PackageVersionId) -> Result<bool, String>,
    install: impl FnOnce(&[InstallRequest], &str) -> Result<InstallOutcome, String>,
) -> Result<ProfileImportOutcome, String> {
    let name = match requested_name {
        Some(name) => name.to_owned(),
        None => unused_profile_name(&ctx.database.mod_profiles(game_id)?, &import.profile_name),
    };
    let mut available = Vec::new();
    let mut missing_packages = Vec::new();
    for imported in &import.mods {
        if is_available(&imported.id)? {
            available.push(imported);
        } else {
            missing_packages.push(imported.id.to_string());
        }
    }

    let profile = create_profile(ctx.database, ctx.paths, game_id, &name)?;
    let requests: Vec<InstallRequest> = available
        .iter()
        .map(|imported| InstallRequest {
            package: imported.id.package.clone(),
            version: Some(imported.id.version),
        })
        .collect();
    let populated = install(&requests, &profile.id).and_then(|outcome| {
        for imported in available.iter().filter(|imported| !imported.enabled) {
            // The loader cannot be disabled and simply stays on.
            match set_mod_enabled(ctx, &profile.id, &imported.id.package.to_string(), false) {
                Ok(()) | Err(InstallError::LoaderCannotBeDisabled(_)) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        let directory = ctx.paths.profile_dir(game_id, &profile.id)?;
        apply_import_files(&import, &directory)?;
        Ok(outcome)
    });
    match populated {
        Ok(outcome) => Ok(ProfileImportOutcome {
            profile,
            installed: outcome.installed.iter().map(ToString::to_string).collect(),
            skipped_files: import.skipped,
            missing_packages,
            warnings: outcome.warnings,
        }),
        Err(error) => Err(match discard_profile(ctx, &profile.id) {
            Some(cleanup) => format!("{error}; {cleanup}"),
            None => error,
        }),
    }
}

/// Replaces the mods and imported config files of an existing profile with
/// the content of `import`, keeping its id, name and place as the game's
/// active profile. Like r2modman, the whole profile is replaced: its current
/// mods, local ones included, and all its other files are discarded.
///
/// The new content is built as a separate temporary profile and only swapped
/// in once complete, under the target's lock, so a failure leaves the target
/// exactly as it was. `ensure_idle` runs once the lock is held and must fail
/// while the game uses the profile.
fn update_profile(
    ctx: &ModdingContext<'_>,
    game_id: &str,
    target_id: &str,
    import: ProfileImport,
    is_available: impl Fn(&PackageVersionId) -> Result<bool, String>,
    install: impl FnOnce(&[InstallRequest], &str) -> Result<InstallOutcome, String>,
    ensure_idle: impl FnOnce() -> Result<(), String>,
) -> Result<ProfileImportOutcome, String> {
    let (target, _) = profile_for_game(ctx.database, ctx.paths, game_id, target_id)?;
    let Some(_guard) = ctx.locks.try_lock(&target.id)? else {
        return Err("The profile is being changed by another operation".to_owned());
    };
    ensure_idle()?;

    let temporary_name = format!("import-{}", uuid::Uuid::new_v4());
    let mut outcome = import_profile(
        ctx,
        game_id,
        import,
        Some(&temporary_name),
        is_available,
        install,
    )?;
    let built = outcome.profile.clone();
    match swap_in_built_profile(ctx, &target, &built) {
        Ok(leftover) => {
            outcome.profile = target;
            outcome.warnings.extend(leftover);
            Ok(outcome)
        }
        Err(error) => Err(match discard_profile(ctx, &built.id) {
            Some(cleanup) => format!("{error}; {cleanup}"),
            None => error,
        }),
    }
}

/// Moves the folder and records of `built` into `target`. On failure both
/// profiles are back as they were. A previous folder that cannot be deleted
/// afterwards is reported, not treated as a failure.
fn swap_in_built_profile(
    ctx: &ModdingContext<'_>,
    target: &ModProfile,
    built: &ModProfile,
) -> Result<Option<String>, String> {
    let target_dir = ctx.paths.profile_dir(&target.game_id, &target.id)?;
    let built_dir = ctx.paths.profile_dir(&built.game_id, &built.id)?;
    let backup = target_dir.with_file_name(format!("{}.replaced", target.id));
    match fs::remove_dir_all(&backup) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Cannot clear {}: {error}", backup.display())),
    }

    let had_folder = match fs::rename(&target_dir, &backup) {
        Ok(()) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(format!(
                "Cannot replace the profile folder {}: {error}",
                target_dir.display()
            ));
        }
    };
    let restore_backup = || {
        if had_folder {
            fs::rename(&backup, &target_dir)
        } else {
            Ok(())
        }
    };
    if let Err(error) = fs::rename(&built_dir, &target_dir) {
        let restored = restore_backup();
        return Err(with_restore_error(
            format!(
                "Cannot replace the profile folder {}: {error}",
                target_dir.display()
            ),
            restored,
        ));
    }
    if let Err(error) = ctx.database.replace_profile_contents(&built.id, &target.id) {
        let restored = fs::rename(&target_dir, &built_dir).and_then(|()| restore_backup());
        return Err(with_restore_error(error, restored));
    }
    if !had_folder {
        return Ok(None);
    }
    Ok(fs::remove_dir_all(&backup).err().map(|error| {
        let message = format!(
            "The previous profile folder could not be removed ({}): {error}",
            backup.display()
        );
        crate::application_log::failure("remove_replaced_profile_directory", &message);
        message
    }))
}

fn with_restore_error(error: String, restored: io::Result<()>) -> String {
    match restored {
        Ok(()) => error,
        Err(restore) => format!("{error}; the profile folder could not be restored: {restore}"),
    }
}

fn discard_profile(ctx: &ModdingContext<'_>, profile_id: &str) -> Option<String> {
    match delete_profile(ctx.database, ctx.paths, ctx.locks, profile_id) {
        Ok(outcome) => outcome.directory_error,
        Err(error) => Some(format!(
            "the unfinished profile could not be removed: {error}"
        )),
    }
}

/// `wanted`, or `wanted (2)`, `wanted (3)` and so on when the name is taken.
fn unused_profile_name(existing: &[ModProfile], wanted: &str) -> String {
    let is_taken = |name: &str| {
        existing
            .iter()
            .any(|profile| profile.name.eq_ignore_ascii_case(name))
    };
    if !is_taken(wanted) {
        return wanted.to_owned();
    }
    let mut number = 2_u32;
    loop {
        let suffix = format!(" ({number})");
        let room = MAX_PROFILE_NAME_CHARS - suffix.chars().count();
        let base: String = wanted.chars().take(room).collect();
        let candidate = format!("{}{suffix}", base.trim_end());
        if !is_taken(&candidate) {
            return candidate;
        }
        number += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modding::{
        dependencies::PackageSource,
        profile_exchange::{ImportedFile, ImportedMod},
        test_support::{Env, FakeRepo},
    };

    const LOADER: &str = "BepInEx-BepInExPack-5.4.2100";

    fn repo() -> FakeRepo {
        let mut repo = FakeRepo::default();
        repo.add(
            LOADER,
            &[],
            &[
                ("manifest.json", "{}"),
                ("BepInExPack/winhttp.dll", "winhttp"),
                ("BepInExPack/BepInEx/core/BepInEx.dll", "core"),
            ],
        );
        repo.add(
            "Owner-Mod-1.0.0",
            &[LOADER],
            &[
                ("manifest.json", "{}"),
                ("Mod.dll", "mod"),
                ("config/Mod.cfg", "default config"),
            ],
        );
        repo
    }

    fn imported(id: &str, enabled: bool) -> ImportedMod {
        ImportedMod {
            id: id.parse().unwrap(),
            enabled,
        }
    }

    fn import(mods: Vec<ImportedMod>) -> ProfileImport {
        ProfileImport {
            profile_name: "Shared".to_owned(),
            mods,
            files: vec![ImportedFile {
                relative_path: "BepInEx/config/Mod.cfg".to_owned(),
                contents: b"imported config".to_vec(),
            }],
            skipped: vec!["Mod.dll".to_owned()],
        }
    }

    fn run(
        env: &Env,
        repo: &FakeRepo,
        import: ProfileImport,
        name: Option<&str>,
    ) -> Result<ProfileImportOutcome, String> {
        import_profile(
            &env.ctx(),
            &env.game_id,
            import,
            name,
            |id| Ok(repo.exact(id)?.is_some()),
            |requests, profile_id| {
                crate::modding::installer::install_packages(
                    &env.ctx(),
                    profile_id,
                    requests,
                    repo,
                    &|package, destination| repo.fetch(package, destination),
                    ResolveMode::Exact,
                    &|_| {},
                )
                .map_err(|error| error.to_string())
            },
        )
    }

    #[test]
    fn imports_mods_and_config() {
        let env = Env::new();
        let repo = repo();
        let outcome = run(
            &env,
            &repo,
            import(vec![
                imported(LOADER, true),
                imported("Owner-Mod-1.0.0", false),
                imported("Gone-Mod-1.0.0", true),
            ]),
            None,
        )
        .unwrap();

        assert_eq!(outcome.profile.name, "Shared");
        assert_eq!(outcome.installed, [LOADER, "Owner-Mod-1.0.0"]);
        assert_eq!(outcome.skipped_files, ["Mod.dll"]);
        assert_eq!(outcome.missing_packages, ["Gone-Mod-1.0.0"]);
        let directory = env
            .paths
            .profile_dir(&env.game_id, &outcome.profile.id)
            .unwrap();
        assert_eq!(
            std::fs::read(directory.join("BepInEx/config/Mod.cfg")).unwrap(),
            b"imported config"
        );
        let mods = env.database.profile_mods(&outcome.profile.id).unwrap();
        let disabled: Vec<_> = mods
            .iter()
            .filter(|record| !record.enabled)
            .map(|record| record.full_name.as_str())
            .collect();
        assert_eq!(disabled, ["Owner-Mod"]);
        assert!(!directory.join("BepInEx/plugins/Owner-Mod/Mod.dll").exists());
    }

    fn run_update(
        env: &Env,
        repo: &FakeRepo,
        import: ProfileImport,
        ensure_idle: impl FnOnce() -> Result<(), String>,
    ) -> Result<ProfileImportOutcome, String> {
        update_profile(
            &env.ctx(),
            &env.game_id,
            &env.profile.id,
            import,
            |id| Ok(repo.exact(id)?.is_some()),
            |requests, profile_id| {
                crate::modding::installer::install_packages(
                    &env.ctx(),
                    profile_id,
                    requests,
                    repo,
                    &|package, destination| repo.fetch(package, destination),
                    ResolveMode::Exact,
                    &|_| {},
                )
                .map_err(|error| error.to_string())
            },
            ensure_idle,
        )
    }

    /// The profile holds the loader and Owner-Mod, plus a file nobody tracks.
    fn populated_env(repo: &FakeRepo) -> Env {
        let env = Env::new();
        env.install(repo, &["Owner-Mod"], ResolveMode::Latest)
            .unwrap();
        let local = env.profile_dir().join("BepInEx/config/local.cfg");
        std::fs::write(local, "local").unwrap();
        env
    }

    fn mod_names(env: &Env) -> Vec<String> {
        env.mods()
            .into_iter()
            .map(|record| record.full_name)
            .collect()
    }

    fn profile_folders(env: &Env) -> usize {
        std::fs::read_dir(env.profile_dir().parent().unwrap())
            .unwrap()
            .count()
    }

    #[test]
    fn update_keeps_the_profile_and_replaces_its_content() {
        let repo = repo();
        let env = populated_env(&repo);
        let before = env.profile.clone();

        let outcome = run_update(
            &env,
            &repo,
            import(vec![
                imported(LOADER, true),
                imported("Gone-Mod-1.0.0", true),
            ]),
            || Ok(()),
        )
        .unwrap();

        assert_eq!(outcome.profile, before);
        assert_eq!(outcome.installed, [LOADER]);
        assert_eq!(outcome.missing_packages, ["Gone-Mod-1.0.0"]);
        assert_eq!(mod_names(&env), ["BepInEx-BepInExPack"]);
        assert_eq!(
            std::fs::read(env.profile_dir().join("BepInEx/config/Mod.cfg")).unwrap(),
            b"imported config"
        );
        assert!(!env.exists("BepInEx/plugins/Owner-Mod/Mod.dll"));
        assert!(!env.exists("BepInEx/config/local.cfg"));
        assert!(env.exists("BepInEx/core/BepInEx.dll"));
        assert!(env.files_of("Owner-Mod").is_empty());
        assert_eq!(
            env.database.mod_profiles(&env.game_id).unwrap(),
            [before.clone()]
        );
        assert_eq!(profile_folders(&env), 1);
    }

    #[test]
    fn update_failure_leaves_the_profile_untouched() {
        let repo = repo();
        let env = populated_env(&repo);
        let mods = env.mods();
        let files = env.files_of("Owner-Mod");
        // Available in the catalog check but impossible to install without its loader.
        let mut incomplete = FakeRepo::default();
        incomplete.add("Other-Mod-1.0.0", &[LOADER], &[("manifest.json", "{}")]);

        let error = run_update(
            &env,
            &incomplete,
            import(vec![imported("Other-Mod-1.0.0", true)]),
            || Ok(()),
        )
        .unwrap_err();

        assert!(error.contains("not found"), "{error}");
        assert_eq!(env.mods(), mods);
        assert_eq!(env.files_of("Owner-Mod"), files);
        assert!(env.exists("BepInEx/plugins/Owner-Mod/Mod.dll"));
        assert!(env.exists("BepInEx/config/local.cfg"));
        assert_eq!(env.database.mod_profiles(&env.game_id).unwrap().len(), 1);
        assert_eq!(profile_folders(&env), 1);
    }

    #[test]
    fn update_is_refused_while_busy_or_in_use() {
        let repo = repo();
        let env = populated_env(&repo);
        let mods = env.mods();

        let busy = env.locks.lock(&env.profile.id).unwrap();
        let error = run_update(&env, &repo, import(Vec::new()), || Ok(())).unwrap_err();
        assert!(error.contains("another operation"), "{error}");
        drop(busy);

        let error = run_update(&env, &repo, import(Vec::new()), || {
            Err("The game is running".to_owned())
        })
        .unwrap_err();
        assert_eq!(error, "The game is running");
        assert_eq!(env.mods(), mods);
        assert!(env.exists("BepInEx/plugins/Owner-Mod/Mod.dll"));
        assert_eq!(env.database.mod_profiles(&env.game_id).unwrap().len(), 1);
        assert_eq!(profile_folders(&env), 1);
    }

    #[test]
    fn update_rejects_a_profile_of_another_game() {
        let repo = repo();
        let env = Env::new();
        let error = update_profile(
            &env.ctx(),
            "other-game",
            &env.profile.id,
            import(Vec::new()),
            |id| Ok(repo.exact(id)?.is_some()),
            |_, _| unreachable!("nothing is installed"),
            || Ok(()),
        )
        .unwrap_err();
        assert!(error.contains("does not belong"), "{error}");
    }

    #[test]
    fn plans_a_catalog_refresh_only_when_needed() {
        let known: PackageVersionId = "Owner-Mod-1.0.0".parse().unwrap();
        let unknown: PackageVersionId = "Gone-Mod-1.0.0".parse().unwrap();
        let stored = known.clone();
        let coverage = |synced, wanted: &[PackageVersionId]| {
            catalog_coverage(synced, wanted, |id| Ok(*id == stored)).unwrap()
        };
        assert_eq!(coverage(false, &[]), CatalogCoverage::Missing);
        assert_eq!(coverage(true, &[]), CatalogCoverage::Complete);
        assert_eq!(coverage(true, &[known.clone()]), CatalogCoverage::Complete);
        assert_eq!(
            coverage(true, &[known, unknown]),
            CatalogCoverage::Incomplete
        );
        assert!(
            catalog_coverage(true, &["A-B-1.0.0".parse().unwrap()], |_| Err("db".into())).is_err()
        );
    }

    #[test]
    fn a_failed_refresh_only_stops_imports_without_a_catalog() {
        let error = "offline".to_owned();
        assert_eq!(
            refresh_failure(CatalogCoverage::Missing, error.clone()),
            Err(error.clone())
        );
        let warning = refresh_failure(CatalogCoverage::Incomplete, error)
            .unwrap()
            .unwrap();
        assert!(warning.contains("offline"));
    }

    #[test]
    fn keeps_a_disabled_loader_enabled() {
        let env = Env::new();
        let repo = repo();
        let outcome = run(&env, &repo, import(vec![imported(LOADER, false)]), None).unwrap();
        let mods = env.database.profile_mods(&outcome.profile.id).unwrap();
        assert!(mods[0].enabled);
    }

    #[test]
    fn removes_the_profile_when_the_install_fails() {
        let env = Env::new();
        // The loader the mod depends on is missing from the repository.
        let mut incomplete = FakeRepo::default();
        incomplete.add("Owner-Mod-1.0.0", &[LOADER], &[("manifest.json", "{}")]);
        let error = run(
            &env,
            &incomplete,
            import(vec![imported("Owner-Mod-1.0.0", true)]),
            Some("Broken"),
        )
        .unwrap_err();
        assert!(error.contains("not found"), "{error}");
        let profiles = env.database.mod_profiles(&env.game_id).unwrap();
        assert!(profiles.iter().all(|profile| profile.name != "Broken"));
        let folders = std::fs::read_dir(env.profile_dir().parent().unwrap())
            .unwrap()
            .count();
        assert_eq!(folders, profiles.len());
    }

    #[test]
    fn names_the_profile_after_the_file_or_the_caller() {
        let env = Env::new();
        let repo = repo();
        let first = run(&env, &repo, import(Vec::new()), None).unwrap();
        let second = run(&env, &repo, import(Vec::new()), None).unwrap();
        let named = run(&env, &repo, import(Vec::new()), Some("Mine")).unwrap();
        assert_eq!(first.profile.name, "Shared");
        assert_eq!(second.profile.name, "Shared (2)");
        assert_eq!(named.profile.name, "Mine");
        assert!(run(&env, &repo, import(Vec::new()), Some("mine")).is_err());
    }

    #[test]
    fn numbers_names_within_the_length_limit() {
        let long = "x".repeat(MAX_PROFILE_NAME_CHARS);
        let existing = [ModProfile {
            id: "id".to_owned(),
            game_id: "game".to_owned(),
            name: long.clone(),
            created_at: 0,
        }];
        let name = unused_profile_name(&existing, &long);
        assert_eq!(name.chars().count(), MAX_PROFILE_NAME_CHARS);
        assert!(name.ends_with(" (2)"));
    }

    #[test]
    fn exports_only_online_mods() {
        let env = Env::new();
        let repo = repo();
        env.install(&repo, &["Owner-Mod"], ResolveMode::Latest)
            .unwrap();
        let input = export_input(&env.database, &env.paths, &env.game_id, &env.profile.id).unwrap();
        let names: Vec<_> = input
            .mods
            .iter()
            .map(|entry| entry.package.to_string())
            .collect();
        assert_eq!(names, ["BepInEx-BepInExPack", "Owner-Mod"]);
        assert!(export_input(&env.database, &env.paths, "other-game", &env.profile.id).is_err());
    }
}
