//! Commands that move mod profiles in and out of Legio as `.r2z` files and
//! Thunderstore profile codes.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::dependencies::{InstallRequest, ResolveMode};
use super::ecosystem_store::EcosystemStore;
use super::install_commands::{install_from_catalog, require_catalog};
use super::installer::{GameRules, InstallError, InstallOutcome, ModdingContext, set_mod_enabled};
use super::package_cache::PackageCache;
use super::package_id::PackageVersionId;
use super::paths::ModdingPaths;
use super::profile_codes::{DEFAULT_BASE_URL, create_profile_code, fetch_profile_code};
use super::profile_commands::run_modding;
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

/// Creates a new profile from an `.r2z` file. Without `profile_name` the name
/// stored in the file is used, made unique when the game already has it.
#[tauri::command]
pub async fn import_mod_profile_file(
    app: AppHandle,
    game_id: String,
    path: String,
    profile_name: Option<String>,
    operation_id: Option<String>,
) -> Result<ProfileImportOutcome, String> {
    crate::startup_recovery::wait(&app).await?;
    let path = require_absolute(&path, "Profile import")?;
    let import = tauri::async_runtime::spawn_blocking(move || read_r2z_file(&path))
        .await
        .map_err(|error| format!("Profile file task failed: {error}"))??;
    import_into_new_profile(app, game_id, import, profile_name, operation_id).await
}

/// Creates a new profile from a Thunderstore profile code.
#[tauri::command]
pub async fn import_mod_profile_code(
    app: AppHandle,
    game_id: String,
    code: String,
    profile_name: Option<String>,
    operation_id: Option<String>,
) -> Result<ProfileImportOutcome, String> {
    crate::startup_recovery::wait(&app).await?;
    let http = app.state::<ThunderstoreHttp>().inner().clone();
    let archive = fetch_profile_code(&http, DEFAULT_BASE_URL, &code).await?;
    let import = tauri::async_runtime::spawn_blocking(move || read_r2z(&archive))
        .await
        .map_err(|error| format!("Profile data task failed: {error}"))??;
    import_into_new_profile(app, game_id, import, profile_name, operation_id).await
}

async fn import_into_new_profile(
    app: AppHandle,
    game_id: String,
    import: ProfileImport,
    profile_name: Option<String>,
    operation_id: Option<String>,
) -> Result<ProfileImportOutcome, String> {
    let schema = app.state::<EcosystemStore>().schema().await?;
    run_modding(app, move |app, database, paths, locks| {
        let modding = database
            .game_modding(&game_id)?
            .ok_or_else(|| "The game is not set up for mods".to_owned())?;
        let community = modding.ecosystem_game_id;
        require_catalog(app, &community)?;
        let rules = GameRules::from_schema(&schema, &community)?;
        let ctx = rules.context(database, paths, locks);
        let catalog = app.state::<PackageCache>();
        import_profile(
            &ctx,
            &game_id,
            import,
            profile_name.as_deref(),
            |id| Ok(catalog.version(&community, id)?.is_some()),
            |requests, profile_id| {
                install_from_catalog(
                    app,
                    &ctx,
                    &community,
                    profile_id,
                    operation_id.as_deref(),
                    requests,
                    ResolveMode::Exact,
                )
            },
        )
    })
    .await
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
