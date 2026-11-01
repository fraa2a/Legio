//! Tauri commands for mod profiles and the mods installed in them.

use std::{fs, path::Path, sync::Arc};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::ecosystem::EcosystemSchema;
use super::ecosystem_store::EcosystemStore;
use super::install_commands::redownload_package;
use super::installer::{
    GameRules, InstallError, InstallProgress, ModdingContext, PROGRESS_EVENT, ToggleAllOutcome,
    UninstallOutcome, declared_dependencies, set_all_mods_enabled, set_mod_enabled, uninstall_mod,
};
use super::local_import::{self, LocalImportOutcome};
use super::paths::ModdingPaths;
use super::profiles::{
    DeleteProfileOutcome, ProfileLocks, create_profile, delete_profile, duplicate_profile,
    profile_with_directory, set_active_profile,
};
use crate::database::{Database, DatabaseState, GameModding, ModProfile, ProfileMod};

/// A mod of a profile, whether it is the game's mod loader and the packages
/// (`Owner-Name`) its installed version declares as dependencies.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileModEntry {
    #[serde(flatten)]
    record: ProfileMod,
    is_loader: bool,
    dependencies: Vec<String>,
}

/// Forwards install progress to the frontend, tagged with the caller's
/// `operation_id`.
pub(crate) fn progress_emitter(
    app: &AppHandle,
    operation_id: Option<String>,
) -> impl Fn(&InstallProgress) + Send + use<> {
    let app = app.clone();
    move |progress| {
        let event = progress.with_operation(operation_id.as_deref());
        if let Err(error) = app.emit(PROGRESS_EVENT, event) {
            crate::application_log::failure("publish_modding_progress", error);
        }
    }
}

#[tauri::command]
pub async fn list_mod_profiles(app: AppHandle, game_id: String) -> Result<Vec<ModProfile>, String> {
    run_modding(app, move |_, database, _, _| {
        database.mod_profiles(&game_id)
    })
    .await
}

#[tauri::command]
pub async fn create_mod_profile(
    app: AppHandle,
    game_id: String,
    name: String,
) -> Result<ModProfile, String> {
    run_modding(app, move |_, database, paths, _| {
        create_profile(database, paths, &game_id, &name)
    })
    .await
}

#[tauri::command]
pub async fn rename_mod_profile(
    app: AppHandle,
    profile_id: String,
    name: String,
) -> Result<ModProfile, String> {
    run_modding(app, move |_, database, _, _| {
        database.rename_mod_profile(&profile_id, &name)
    })
    .await
}

#[tauri::command]
pub async fn duplicate_mod_profile(
    app: AppHandle,
    profile_id: String,
    name: String,
) -> Result<ModProfile, String> {
    run_modding(app, move |_, database, paths, locks| {
        duplicate_profile(database, paths, locks, &profile_id, &name)
    })
    .await
}

#[tauri::command]
pub async fn delete_mod_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<DeleteProfileOutcome, String> {
    run_modding(app, move |app, database, paths, locks| {
        require_profile_not_running(app, database, paths, &profile_id)?;
        delete_profile(database, paths, locks, &profile_id)
    })
    .await
}

#[tauri::command]
pub async fn set_active_mod_profile(
    app: AppHandle,
    game_id: String,
    profile_id: Option<String>,
) -> Result<GameModding, String> {
    run_modding(app, move |app, database, _, _| {
        app.state::<crate::game_lifecycle::GameLaunchManager>()
            .require_idle(&game_id)?;
        set_active_profile(database, &game_id, profile_id.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn list_profile_mods(
    app: AppHandle,
    profile_id: String,
) -> Result<Vec<ProfileModEntry>, String> {
    let schema = local_schema(&app).await;
    run_modding(app, move |_, database, paths, locks| {
        let mods = database.profile_mods(&profile_id)?;
        let entries = schema
            .map_err(|error| format!("the game list is not available: {error}"))
            .and_then(|schema| {
                with_game_rules(database, paths, locks, &schema, &profile_id, |ctx, _| {
                    Ok(mod_entries(database, &mods, paths, |name| {
                        ctx.find_loader(name).is_some()
                    }))
                })
            })
            .unwrap_or_else(|error| {
                crate::application_log::failure("find_profile_mod_loaders", error);
                mod_entries(database, &mods, paths, |_| false)
            });
        Ok(entries)
    })
    .await
}

fn mod_entries(
    database: &Database,
    mods: &[ProfileMod],
    paths: &ModdingPaths,
    is_loader: impl Fn(&str) -> bool,
) -> Vec<ProfileModEntry> {
    let mut dependencies = declared_dependencies(database, paths, mods);
    mods.iter()
        .map(|record| ProfileModEntry {
            is_loader: is_loader(&record.full_name),
            dependencies: dependencies
                .remove(&record.full_name.to_lowercase())
                .unwrap_or_default(),
            record: record.clone(),
        })
        .collect()
}

#[tauri::command]
pub async fn uninstall_profile_mod(
    app: AppHandle,
    profile_id: String,
    full_name: String,
) -> Result<UninstallOutcome, String> {
    let schema = local_schema(&app).await?;
    run_modding(app, move |app, database, paths, locks| {
        require_profile_not_running(app, database, paths, &profile_id)?;
        with_game_rules(database, paths, locks, &schema, &profile_id, |ctx, _| {
            uninstall_mod(ctx, &profile_id, &full_name).map_err(|error| error.to_string())
        })
    })
    .await
}

#[tauri::command]
pub async fn set_profile_mod_enabled(
    app: AppHandle,
    profile_id: String,
    full_name: String,
    enabled: bool,
) -> Result<(), String> {
    let schema = local_schema(&app).await?;
    run_modding(app, move |app, database, paths, locks| {
        require_profile_not_running(app, database, paths, &profile_id)?;
        with_game_rules(
            database,
            paths,
            locks,
            &schema,
            &profile_id,
            |ctx, community| match set_mod_enabled(ctx, &profile_id, &full_name, enabled) {
                // Enabling copies from the package cache, so a lost cache
                // is downloaded once and the attempt repeated.
                Err(InstallError::CacheMissing {
                    package,
                    online_source: true,
                }) => {
                    redownload_package(app, ctx, community, &profile_id, &package)?;
                    set_mod_enabled(ctx, &profile_id, &full_name, enabled)
                        .map_err(|error| error.to_string())
                }
                result => result.map_err(|error| error.to_string()),
            },
        )
    })
    .await
}

/// Enables or disables every mod of the profile but the loader. Mods that
/// could not be switched are listed in the outcome and stay as they were.
#[tauri::command]
pub async fn set_all_profile_mods_enabled(
    app: AppHandle,
    profile_id: String,
    enabled: bool,
) -> Result<ToggleAllOutcome, String> {
    let schema = local_schema(&app).await?;
    run_modding(app, move |app, database, paths, locks| {
        require_profile_not_running(app, database, paths, &profile_id)?;
        with_game_rules(
            database,
            paths,
            locks,
            &schema,
            &profile_id,
            |ctx, community| {
                set_all_mods_enabled(ctx, &profile_id, enabled, |package| {
                    redownload_package(app, ctx, community, &profile_id, package)
                })
                .map_err(|error| error.to_string())
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn import_local_mod(
    app: AppHandle,
    profile_id: String,
    archive_path: String,
) -> Result<LocalImportOutcome, String> {
    let progress = progress_emitter(&app, None);
    let schema = local_schema(&app).await?;
    run_modding(app, move |app, database, paths, locks| {
        require_profile_not_running(app, database, paths, &profile_id)?;
        with_game_rules(database, paths, locks, &schema, &profile_id, |ctx, _| {
            local_import::import_local_mod(ctx, &profile_id, Path::new(&archive_path), &progress)
                .map_err(|error| error.to_string())
        })
    })
    .await
}

#[tauri::command]
pub async fn open_mod_profile_folder(app: AppHandle, profile_id: String) -> Result<(), String> {
    run_modding(app, move |_, database, paths, _| {
        let (_, directory) = profile_with_directory(database, paths, &profile_id)?;
        fs::create_dir_all(&directory)
            .map_err(|error| format!("Cannot create the profile folder: {error}"))?;
        crate::installed_folder::open_directory(&directory)
    })
    .await
}

/// Bytes used by the profile folder; a missing folder counts as empty.
#[tauri::command]
pub async fn get_mod_profile_size(app: AppHandle, profile_id: String) -> Result<u64, String> {
    run_modding(app, move |_, database, paths, _| {
        let (_, directory) = profile_with_directory(database, paths, &profile_id)?;
        super::cache_management::directory_size(&directory)
    })
    .await
}

/// The schema for actions that never download anything, so they keep
/// working offline.
async fn local_schema(app: &AppHandle) -> Result<Arc<EcosystemSchema>, String> {
    app.state::<EcosystemStore>().cached_schema().await
}

/// Runs a blocking operation once startup recovery is done.
pub(crate) async fn run_modding<T, F>(app: AppHandle, operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&AppHandle, &Database, &ModdingPaths, &ProfileLocks) -> Result<T, String>
        + Send
        + 'static,
{
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DatabaseState>();
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("Could not locate app data: {error}"))?;
        let locks = app.state::<ProfileLocks>();
        operation(
            &app,
            state.database()?,
            &ModdingPaths::new(&data_dir),
            &locks,
        )
    })
    .await
    .map_err(|error| format!("Mod task failed: {error}"))?
}

/// Refuses to change files of the profile the running game uses.
pub(crate) fn require_profile_not_running(
    app: &AppHandle,
    database: &Database,
    paths: &ModdingPaths,
    profile_id: &str,
) -> Result<(), String> {
    let (profile, _) = profile_with_directory(database, paths, profile_id)?;
    let active = database
        .game_modding(&profile.game_id)?
        .and_then(|modding| modding.active_profile_id);
    if active.as_deref() == Some(profile.id.as_str()) {
        app.state::<crate::game_lifecycle::GameLaunchManager>()
            .require_idle(&profile.game_id)?;
    }
    Ok(())
}

/// Runs `operation` with the install rules of the profile's game and the
/// Thunderstore game id it is bound to.
pub(crate) fn with_game_rules<T>(
    database: &Database,
    paths: &ModdingPaths,
    locks: &ProfileLocks,
    schema: &EcosystemSchema,
    profile_id: &str,
    operation: impl FnOnce(&ModdingContext<'_>, &str) -> Result<T, String>,
) -> Result<T, String> {
    let (profile, _) = profile_with_directory(database, paths, profile_id)?;
    let modding = database
        .game_modding(&profile.game_id)?
        .ok_or_else(|| "The game is not set up for mods".to_owned())?;
    let rules = GameRules::from_schema(schema, &modding.ecosystem_game_id)?;
    operation(
        &rules.context(database, paths, locks),
        &modding.ecosystem_game_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modding::{
        dependencies::ResolveMode,
        test_support::{Env, FakeRepo},
    };

    #[test]
    fn marks_the_loader_among_the_mods() {
        let env = Env::new();
        let mut repo = FakeRepo::default();
        repo.add(
            "BepInEx-BepInExPack-5.4.2100",
            &[],
            &[("BepInExPack/winhttp.dll", "x")],
        );
        repo.add(
            "Owner-Mod-1.0.0",
            &["BepInEx-BepInExPack-5.4.2100"],
            &[
                ("Mod.dll", "x"),
                (
                    "manifest.json",
                    "{\"dependencies\":[\"BepInEx-BepInExPack-5.4.2100\",\"bepinex-bepinexpack-5.4.2100\",\"bad\"]}",
                ),
            ],
        );
        env.install(&repo, &["Owner-Mod"], ResolveMode::Latest)
            .unwrap();
        let ctx = env.ctx();
        let entries = mod_entries(&env.database, &env.mods(), &env.paths, |name| {
            ctx.find_loader(name).is_some()
        });
        let flags: Vec<_> = entries
            .iter()
            .map(|entry| (entry.record.full_name.as_str(), entry.is_loader))
            .collect();
        assert_eq!(flags, [("BepInEx-BepInExPack", true), ("Owner-Mod", false)]);
        let json = serde_json::to_value(&entries[0]).unwrap();
        assert_eq!(json["isLoader"], true);
        assert_eq!(json["fullName"], "BepInEx-BepInExPack");
        assert_eq!(json["onlineSource"], true);
        assert_eq!(json["dependencies"], serde_json::json!([]));
        // Duplicates differing in case and unparsable entries are dropped.
        assert_eq!(entries[1].dependencies, ["BepInEx-BepInExPack"]);
    }
}
