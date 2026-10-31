//! Tauri commands for mod profiles and the mods installed in them.

use std::{fs, path::Path};

use tauri::{AppHandle, Emitter, Manager};

use super::ecosystem::EcosystemSchema;
use super::ecosystem_store::EcosystemStore;
use super::install_commands::redownload_package;
use super::installer::{
    GameRules, InstallError, InstallProgress, ModdingContext, PROGRESS_EVENT, UninstallOutcome,
    set_mod_enabled, uninstall_mod,
};
use super::local_import::{self, LocalImportOutcome};
use super::paths::ModdingPaths;
use super::profiles::{
    DeleteProfileOutcome, ProfileLocks, create_profile, delete_profile, duplicate_profile,
    profile_with_directory, rename_profile, set_active_profile,
};
use crate::database::{Database, DatabaseState, GameModding, ModProfile, ProfileMod};

/// Forwards install progress to the frontend.
pub(crate) fn progress_emitter(app: &AppHandle) -> impl Fn(&InstallProgress) + Send + use<> {
    let app = app.clone();
    move |progress| {
        if let Err(error) = app.emit(PROGRESS_EVENT, progress) {
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
    run_modding(app, move |_, database, _, locks| {
        rename_profile(database, locks, &profile_id, &name)
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
) -> Result<Vec<ProfileMod>, String> {
    run_modding(app, move |_, database, _, _| {
        database.profile_mods(&profile_id)
    })
    .await
}

#[tauri::command]
pub async fn uninstall_profile_mod(
    app: AppHandle,
    profile_id: String,
    full_name: String,
) -> Result<UninstallOutcome, String> {
    let schema = game_schema(&app).await?;
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
    let schema = game_schema(&app).await?;
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

#[tauri::command]
pub async fn import_local_mod(
    app: AppHandle,
    profile_id: String,
    archive_path: String,
) -> Result<LocalImportOutcome, String> {
    let progress = progress_emitter(&app);
    let schema = game_schema(&app).await?;
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

async fn game_schema(app: &AppHandle) -> Result<std::sync::Arc<EcosystemSchema>, String> {
    app.state::<EcosystemStore>().schema().await
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
