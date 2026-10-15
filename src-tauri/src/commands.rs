use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_notification::NotificationExt;

use crate::{
    database::{
        self, AccountCheck, CreateGameInput, DatabaseState, Game, Settings, UpdateGameInput,
    },
    diagnostics::{Diagnostics, LogStatus},
    legio_source_cache, manual_import,
    network::{ConnectivityCheck, NetworkState, NetworkStatus},
    steam_local,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: String,
    version: String,
    platform: String,
    tray_available: bool,
    desktop_environment: Option<String>,
    startup_launch_game_id: Option<String>,
    startup_launch_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameActionResult {
    game: Game,
    shortcut_warning: Option<String>,
}

#[tauri::command]
pub async fn get_app_info(app: AppHandle) -> Result<AppInfo, String> {
    let recovery_error = crate::startup_recovery::wait(&app).await.err();
    let package_info = app.package_info();

    Ok(AppInfo {
        name: package_info.name.clone(),
        version: package_info.version.to_string(),
        platform: std::env::consts::OS.to_owned(),
        tray_available: app.state::<crate::TrayAvailable>().0,
        desktop_environment: desktop_environment(),
        startup_launch_game_id: app.state::<crate::StartupLaunch>().game_id.clone(),
        startup_launch_error: recovery_error.or(app
            .state::<crate::StartupLaunch>()
            .error
            .lock()
            .map_err(|_| "Shortcut launch state is unavailable".to_owned())?
            .clone()),
    })
}

#[tauri::command]
pub fn notify_update_available(app: AppHandle, version: String, aur: bool) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Ok(());
    }
    if version.is_empty() || version.len() > 64 || version.chars().any(char::is_control) {
        return Err("Update version is invalid".to_owned());
    }
    let settings = app.state::<DatabaseState>().database()?.settings()?;
    let body = if aur {
        format!(
            "Legio {version}. {}",
            settings
                .language
                .text("Aggiorna tramite AUR.", "Update through AUR.")
        )
    } else {
        format!("Legio {version}")
    };
    app.notification()
        .builder()
        .title(
            settings
                .language
                .text("Aggiornamento disponibile", "Update available"),
        )
        .body(body)
        .show()
        .map_err(|error| format!("Could not show update notification: {error}"))
}

pub(crate) fn desktop_environment() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    normalize_desktop_environment(
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some(),
        std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
    )
}

fn normalize_desktop_environment(hyprland: bool, current_desktop: Option<&str>) -> Option<String> {
    if hyprland {
        return Some("hyprland".to_owned());
    }
    let primary = current_desktop?.split(':').next()?.trim().to_lowercase();
    if primary.is_empty() {
        return None;
    }
    Some(primary)
}

#[tauri::command]
pub fn get_settings(state: State<'_, DatabaseState>) -> Result<Settings, String> {
    state.database()?.settings()
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<Settings, String> {
    crate::settings::save(&app, settings)
}

#[tauri::command]
pub fn get_compatibility_defaults(
    state: State<'_, DatabaseState>,
) -> Result<database::CompatibilityDefaults, String> {
    state.database()?.compatibility_defaults()
}

#[tauri::command]
pub fn save_compatibility_defaults(
    state: State<'_, DatabaseState>,
    defaults: database::CompatibilityDefaults,
) -> Result<database::CompatibilityDefaults, String> {
    state.database()?.save_compatibility_defaults(defaults)
}

#[tauri::command]
pub fn get_game_compatibility_overrides(
    state: State<'_, DatabaseState>,
    game_id: String,
) -> Result<database::GameCompatibilityOverrides, String> {
    state.database()?.game_compatibility_overrides(&game_id)
}

#[tauri::command]
pub async fn get_game_online_fix_detected(
    state: State<'_, DatabaseState>,
    game_id: String,
) -> Result<bool, String> {
    let database = state.shared_database()?;
    tauri::async_runtime::spawn_blocking(move || {
        let game = database.game(&game_id)?;
        crate::online_fix::detected(&database, &game)
    })
    .await
    .map_err(|error| format!("OnlineFix detection task failed: {error}"))?
}

#[tauri::command]
pub fn get_native_launch_config(
    state: State<'_, DatabaseState>,
    game_id: String,
) -> Result<database::NativeLaunchConfig, String> {
    state.database()?.native_launch_config(&game_id)
}

#[tauri::command]
pub fn get_steam_launch_config(
    state: State<'_, DatabaseState>,
    game_id: String,
) -> Result<database::SteamLaunchConfig, String> {
    state.database()?.steam_launch_config(&game_id)
}

#[tauri::command]
pub fn save_steam_launch_config(
    state: State<'_, DatabaseState>,
    game_id: String,
    config: database::SteamLaunchConfig,
) -> Result<database::SteamLaunchConfig, String> {
    state.database()?.save_steam_launch_config(&game_id, config)
}

#[tauri::command]
pub fn save_native_launch_config(
    state: State<'_, DatabaseState>,
    game_id: String,
    config: database::NativeLaunchConfig,
) -> Result<database::NativeLaunchConfig, String> {
    state
        .database()?
        .save_native_launch_config(&game_id, config)
}

#[tauri::command]
pub fn save_game_compatibility_overrides(
    state: State<'_, DatabaseState>,
    game_id: String,
    overrides: database::GameCompatibilityOverrides,
) -> Result<database::GameCompatibilityOverrides, String> {
    state
        .database()?
        .save_game_compatibility_overrides(&game_id, overrides)
}

#[tauri::command]
pub fn list_games(state: State<'_, DatabaseState>) -> Result<Vec<Game>, String> {
    let result = state.database().and_then(database::Database::games);
    if let Err(error) = &result {
        crate::application_log::failure("library_load", error);
    }
    result
}

#[tauri::command]
pub fn get_application_log_status(
    state: State<'_, crate::application_log::ApplicationLog>,
) -> crate::application_log::LogStatus {
    state.status()
}

#[tauri::command]
pub async fn report_application_event(
    state: State<'_, crate::application_log::ApplicationLog>,
    level: crate::application_log::Level,
    event: crate::application_log::RendererEvent,
    message: Option<String>,
    stack: Option<String>,
) -> Result<(), String> {
    let log = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        log.report(level, event, message.as_deref(), stack.as_deref())
    })
    .await
    .map_err(|_| "Application log report task failed".to_owned())?
}

#[tauri::command]
pub async fn get_playtime_summaries(
    app: AppHandle,
) -> Result<Vec<database::PlaytimeSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DatabaseState>()
            .database()?
            .playtime_summaries(database::now_milliseconds()?)
    })
    .await
    .map_err(|error| format!("Playtime summary task failed: {error}"))?
}

#[tauri::command]
pub async fn set_game_playtime(
    app: AppHandle,
    game_id: String,
    total_milliseconds: i64,
) -> Result<(), String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<crate::game_lifecycle::GameLaunchManager>();
        let _operation = manager.operation()?;
        manager.require_idle(&game_id)?;
        let state = app.state::<DatabaseState>();
        let database = state.database()?;
        let game = database.game(&game_id)?;
        if let Some(root) = game
            .installation_root
            .as_deref()
            .or(game.steam_install_path.as_deref())
            .map(std::path::Path::new)
            && root.is_dir()
            && crate::download_policy::external_busy(root)?
        {
            return Err("Close the game before changing its playtime".into());
        }
        database.set_playtime(&game_id, total_milliseconds, database::now_milliseconds()?)
    })
    .await
    .map_err(|e| format!("Playtime edit task failed: {e}"))?
}

#[tauri::command]
pub async fn get_game_libraries(
    app: AppHandle,
) -> Result<Vec<crate::game_library::LibraryInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::game_library::libraries(app.state::<DatabaseState>().database()?)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn import_game_library(app: AppHandle, directory: String) -> Result<(), String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<crate::game_lifecycle::GameLaunchManager>();
        let _operation = manager.operation()?;
        let database = app.state::<DatabaseState>();
        for game in database.database()?.games()? {
            manager.require_idle(&game.id)?;
        }
        crate::game_library::import(database.database()?, std::path::Path::new(&directory))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn repair_game_library(app: AppHandle, directory: String) -> Result<(), String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::game_library::repair(
            app.state::<DatabaseState>().database()?,
            std::path::Path::new(&directory),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_playtime_activity(
    app: AppHandle,
    day_boundaries: Vec<i64>,
) -> Result<Vec<i64>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DatabaseState>();
        crate::playtime_activity::daily_playtime(
            state.database()?,
            &day_boundaries,
            database::now_milliseconds()?,
        )
    })
    .await
    .map_err(|error| format!("could not load playtime activity: {error}"))?
}

#[tauri::command]
pub fn create_game(
    state: State<'_, DatabaseState>,
    input: CreateGameInput,
) -> Result<Game, String> {
    state.database()?.create_game(input)
}

#[tauri::command]
pub async fn create_game_shortcut(
    app: AppHandle,
    game_id: String,
    location: crate::desktop_shortcuts::ShortcutLocation,
) -> Result<crate::desktop_shortcuts::ShortcutCreationResult, String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::desktop_shortcuts::create_for_game(&app, &game_id, location)
    })
    .await
    .map_err(|error| format!("Game shortcut task failed: {error}"))?
}

#[tauri::command]
pub async fn set_game_icon(
    app: AppHandle,
    game_id: String,
    file_path: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DatabaseState>().database()?.game(&game_id)?;
        app.state::<crate::game_artwork::GameArtworkStore>()
            .set(
                &game_id,
                crate::game_artwork::ArtworkKind::Icon,
                std::path::Path::new(&file_path),
            )?
            .into_response()
    })
    .await
    .map_err(|error| format!("Game artwork task failed: {error}"))?
}

#[tauri::command]
pub async fn get_game_icon(
    app: AppHandle,
    game_id: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DatabaseState>().database()?.game(&game_id)?;
        let store = app.state::<crate::game_artwork::GameArtworkStore>();
        let artwork = store.get(&game_id, crate::game_artwork::ArtworkKind::Icon)?;
        match artwork {
            Some(value) => value.into_response(),
            None => Ok(tauri::ipc::Response::new(Vec::new())),
        }
    })
    .await
    .map_err(|error| format!("Game artwork task failed: {error}"))?
}

#[tauri::command]
pub async fn reset_game_icon(app: AppHandle, game_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DatabaseState>().database()?.game(&game_id)?;
        app.state::<crate::game_artwork::GameArtworkStore>()
            .remove(&game_id, crate::game_artwork::ArtworkKind::Icon)
    })
    .await
    .map_err(|error| format!("Game artwork task failed: {error}"))?
}

#[tauri::command]
pub async fn set_game_banner(
    app: AppHandle,
    game_id: String,
    file_path: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DatabaseState>().database()?.game(&game_id)?;
        app.state::<crate::game_artwork::GameArtworkStore>()
            .set(
                &game_id,
                crate::game_artwork::ArtworkKind::Banner,
                std::path::Path::new(&file_path),
            )?
            .into_response()
    })
    .await
    .map_err(|error| format!("Game artwork task failed: {error}"))?
}

#[tauri::command]
pub async fn get_game_banner(
    app: AppHandle,
    game_id: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DatabaseState>().database()?.game(&game_id)?;
        let artwork = app
            .state::<crate::game_artwork::GameArtworkStore>()
            .get(&game_id, crate::game_artwork::ArtworkKind::Banner)?;
        match artwork {
            Some(value) => value.into_response(),
            None => Ok(tauri::ipc::Response::new(Vec::new())),
        }
    })
    .await
    .map_err(|error| format!("Game artwork task failed: {error}"))?
}

#[tauri::command]
pub async fn reset_game_banner(app: AppHandle, game_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DatabaseState>().database()?.game(&game_id)?;
        app.state::<crate::game_artwork::GameArtworkStore>()
            .remove(&game_id, crate::game_artwork::ArtworkKind::Banner)
    })
    .await
    .map_err(|error| format!("Game artwork task failed: {error}"))?
}

#[tauri::command]
pub fn update_game(app: AppHandle, input: UpdateGameInput) -> Result<Game, String> {
    let game = app
        .state::<DatabaseState>()
        .database()?
        .update_game(input)?;
    crate::game_library::sync_default(&app)?;
    Ok(game)
}

#[tauri::command]
pub async fn remove_game(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::game_lifecycle::remove_game(&app, &id)?;
        crate::game_library::remove_default(&app, &id)
    })
    .await
    .map_err(|error| format!("Game removal task failed: {error}"))?
}

#[tauri::command]
pub async fn launch_steam_game(
    app: AppHandle,
    game_id: String,
    confirm_account_switch: bool,
) -> Result<crate::steam_switch::SteamLaunchResult, String> {
    let manager = app
        .state::<crate::game_lifecycle::GameLaunchManager>()
        .inner()
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        manager.record_launch_request(&game_id, false);
        let result = manager.launch(app, game_id.clone(), confirm_account_switch);
        if result.is_err() {
            manager.record_launch_request(&game_id, true);
        }
        result
    })
    .await
    .map_err(|error| format!("Steam launch task failed: {error}"))?
}

#[tauri::command]
pub async fn launch_game_with_runner(
    app: AppHandle,
    game_id: String,
    runner_path: String,
) -> Result<(), String> {
    let manager = app
        .state::<crate::game_lifecycle::GameLaunchManager>()
        .inner()
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        manager.record_launch_request(&game_id, false);
        let result = manager.launch_with_runner(app, game_id.clone(), runner_path);
        if result.is_err() {
            manager.record_launch_request(&game_id, true);
        }
        result
    })
    .await
    .map_err(|error| format!("Compatibility runner launch task failed: {error}"))?
}

#[tauri::command]
pub async fn launch_configured_game_with_runner(
    app: AppHandle,
    game_id: String,
) -> Result<(), String> {
    let manager = app
        .state::<crate::game_lifecycle::GameLaunchManager>()
        .inner()
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        manager.record_launch_request(&game_id, false);
        let result = manager.launch_configured(app, game_id.clone());
        if result.is_err() {
            manager.record_launch_request(&game_id, true);
        }
        result
    })
    .await
    .map_err(|error| format!("Compatibility runner launch task failed: {error}"))?
}

#[tauri::command]
pub async fn launch_native_game(app: AppHandle, game_id: String) -> Result<(), String> {
    let manager = app
        .state::<crate::game_lifecycle::GameLaunchManager>()
        .inner()
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        manager.record_launch_request(&game_id, false);
        let result = manager.launch_native(app, game_id.clone());
        if result.is_err() {
            manager.record_launch_request(&game_id, true);
        }
        result
    })
    .await
    .map_err(|error| format!("Native game launch task failed: {error}"))?
}

#[tauri::command]
pub fn list_game_launch_states(
    state: State<'_, crate::game_lifecycle::GameLaunchManager>,
) -> Result<Vec<crate::game_lifecycle::GameLaunchState>, String> {
    state.list()
}

#[tauri::command]
pub fn get_compatibility_logs_directory(
    state: State<'_, crate::game_lifecycle::GameLaunchManager>,
) -> Result<std::path::PathBuf, String> {
    state.compatibility_logs_directory()
}

#[tauri::command]
pub fn cancel_game_launch(
    state: State<'_, crate::game_lifecycle::GameLaunchManager>,
    game_id: String,
) -> Result<(), String> {
    state.cancel(&game_id)
}

#[tauri::command]
pub async fn stop_game(app: AppHandle, game_id: String) -> Result<(), String> {
    let manager = app
        .state::<crate::game_lifecycle::GameLaunchManager>()
        .inner()
        .clone();
    tauri::async_runtime::spawn_blocking(move || manager.stop(&game_id))
        .await
        .map_err(|error| format!("Game stop task failed: {error}"))?
}

#[tauri::command]
pub async fn inspect_steam_game_launch(
    app: AppHandle,
    game_id: String,
) -> Result<crate::steam_switch::LaunchInspection, String> {
    tauri::async_runtime::spawn_blocking(move || crate::steam_switch::inspect(&app, &game_id))
        .await
        .map_err(|error| format!("Steam launch inspection task failed: {error}"))?
}

#[tauri::command]
pub async fn list_saved_steam_accounts() -> Result<steam_local::SavedSteamAccounts, String> {
    tauri::async_runtime::spawn_blocking(steam_local::scan_saved_accounts)
        .await
        .map_err(|error| format!("Steam account scan task failed: {error}"))
}

#[tauri::command]
pub async fn set_game_steam_account_preference(
    app: AppHandle,
    game_id: String,
    steam_id: Option<String>,
) -> Result<Game, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(ref steam_id) = steam_id {
            let saved = steam_local::scan_saved_accounts();
            if !saved
                .accounts
                .iter()
                .any(|account| account.steam_id == *steam_id)
            {
                return Err("Steam account is not in the local saved account list".to_owned());
            }
        }
        app.state::<DatabaseState>()
            .database()?
            .set_game_steam_account(&game_id, steam_id.as_deref())
    })
    .await
    .map_err(|error| format!("Steam account preference task failed: {error}"))?
}

#[tauri::command]
pub async fn check_game_steam_account(
    app: AppHandle,
    game_id: String,
) -> Result<AccountCheck, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let check = app
            .state::<DatabaseState>()
            .database()?
            .check_game_steam_account(&game_id)?;
        if check.selected_steam_id.is_none() {
            return Ok(check);
        }
        Ok(steam_local::check_saved_account_presence(
            check,
            &steam_local::scan_saved_accounts(),
        ))
    })
    .await
    .map_err(|error| format!("Steam account check task failed: {error}"))?
}

#[cfg(test)]
mod desktop_environment_tests {
    use super::normalize_desktop_environment;

    #[test]
    fn hyprland_signature_wins_over_current_desktop() {
        assert_eq!(
            normalize_desktop_environment(true, Some("KDE")),
            Some("hyprland".to_owned())
        );
    }

    #[test]
    fn current_desktop_uses_the_first_entry() {
        assert_eq!(
            normalize_desktop_environment(false, Some("ubuntu:GNOME")),
            Some("ubuntu".to_owned())
        );
        assert_eq!(
            normalize_desktop_environment(false, Some("KDE")),
            Some("kde".to_owned())
        );
    }

    #[test]
    fn missing_or_empty_current_desktop_is_unknown() {
        assert_eq!(normalize_desktop_environment(false, None), None);
        assert_eq!(normalize_desktop_environment(false, Some("  ")), None);
    }
}

#[tauri::command]
pub async fn scan_steam_installations() -> Result<steam_local::SteamScan, String> {
    tauri::async_runtime::spawn_blocking(steam_local::scan_default_installations)
        .await
        .map_err(|error| format!("Steam scan task failed: {error}"))
}

#[tauri::command]
pub async fn import_steam_installations(
    app: AppHandle,
) -> Result<crate::steam_import::SteamImportResult, String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::steam_import::import_default_installations(&app.state::<DatabaseState>())
    })
    .await
    .map_err(|error| format!("Steam import task failed: {error}"))?
}

#[tauri::command]
pub async fn scan_game_executables(
    directory: String,
    game_name: Option<String>,
) -> Result<manual_import::ExecutableScan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        manual_import::scan_directory(&directory, game_name.as_deref())
    })
    .await
    .map_err(|error| format!("Executable scan task failed: {error}"))?
}

#[tauri::command]
pub async fn import_manual_game(
    app: AppHandle,
    input: manual_import::ManualImportInput,
) -> Result<GameActionResult, String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let game = manual_import::import(&app.state::<DatabaseState>(), input)?;
        crate::game_library::sync_default(&app)?;
        Ok(GameActionResult {
            shortcut_warning: crate::desktop_shortcuts::create_application_menu(&app, &game)
                .unwrap_or_else(|error| {
                    Some(format!(
                        "Could not create application-menu shortcut: {error}"
                    ))
                }),
            game,
        })
    })
    .await
    .map_err(|error| format!("Manual import task failed: {error}"))?
}

#[tauri::command]
pub async fn identify_manual_game_steam_app_id(
    app: AppHandle,
    state: State<'_, NetworkState>,
    game_id: String,
) -> Result<manual_import::SteamIdentificationResult, String> {
    manual_import::identify_steam_app_id(app, state.inner().clone(), game_id).await
}

#[tauri::command]
pub async fn preview_manual_game_steam_app_id(
    app: AppHandle,
    state: State<'_, NetworkState>,
    executable_path: String,
) -> Result<manual_import::SteamIdentificationPreview, String> {
    manual_import::preview_steam_app_id(app, state.inner().clone(), executable_path).await
}

#[tauri::command]
pub async fn set_game_executable(
    app: AppHandle,
    game_id: String,
    executable_path: String,
) -> Result<GameActionResult, String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let game = manual_import::set_executable(
            &app.state::<DatabaseState>(),
            &game_id,
            &executable_path,
        )?;
        crate::game_library::sync_default(&app)?;
        Ok(GameActionResult {
            shortcut_warning: crate::desktop_shortcuts::create_application_menu(&app, &game)
                .unwrap_or_else(|error| {
                    Some(format!(
                        "Could not create application-menu shortcut: {error}"
                    ))
                }),
            game,
        })
    })
    .await
    .map_err(|error| format!("Executable selection task failed: {error}"))?
}

#[tauri::command]
pub fn get_network_status(state: State<'_, NetworkState>) -> Result<NetworkStatus, String> {
    state.status()
}

#[tauri::command]
pub fn get_network_log_status(state: State<'_, Diagnostics>) -> LogStatus {
    state.status()
}

#[tauri::command]
pub async fn get_legio_source(
    app: AppHandle,
) -> Result<legio_source_cache::SourceSnapshot, String> {
    legio_source_cache::cached_source(app).await
}

#[tauri::command]
pub async fn refresh_legio_source(
    app: AppHandle,
    state: State<'_, NetworkState>,
) -> Result<legio_source_cache::SourceSnapshot, String> {
    let snapshot = legio_source_cache::refresh_source(app.clone(), state.inner()).await?;
    crate::game_updates::schedule(app);
    Ok(snapshot)
}

#[tauri::command]
pub async fn check_steam_connectivity(
    app: AppHandle,
    state: State<'_, NetworkState>,
) -> Result<ConnectivityCheck, String> {
    let mut result = state.inner().clone().check_connectivity().await;
    if result.status == NetworkStatus::Online
        && let Err(error) = crate::download_queue::resume_waiting(app).await
    {
        result.detail = Some(format!(
            "Steam is reachable, but waiting downloads could not resume: {error}"
        ));
    }
    Ok(result)
}

#[tauri::command]
pub async fn search_catalog(
    app: AppHandle,
    query: String,
    limit: Option<u32>,
    options: Option<crate::catalog::CatalogOptions>,
) -> Result<crate::catalog::CatalogSearch, crate::catalog::CatalogError> {
    crate::catalog::search_catalog(app, query, limit, options).await
}

#[tauri::command]
pub async fn refresh_catalog(
    app: AppHandle,
    state: State<'_, NetworkState>,
    query: String,
    skip: Option<usize>,
    limit: Option<u32>,
    options: Option<crate::catalog::CatalogOptions>,
) -> Result<crate::catalog::CatalogSearch, crate::catalog::CatalogError> {
    crate::catalog::refresh_catalog(app, state.inner(), query, skip, limit, options).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn get_steam_details(
    app: AppHandle,
    state: State<'_, NetworkState>,
    scheduler: State<'_, crate::artwork_scheduler::ArtworkScheduler>,
    steam_app_id: u32,
    policy: crate::steam_details::DetailsPolicy,
    language: crate::locale::LanguagePreference,
    request_id: String,
    origin: crate::artwork_scheduler::WorkOrigin,
) -> Result<crate::steam_details::DetailsResult, crate::steam_details::DetailsError> {
    let work = scheduler
        .begin(request_id, steam_app_id, origin)
        .map_err(|error| crate::steam_details::DetailsError {
            kind: crate::steam_details::DetailsErrorKind::Cancelled,
            message: error.to_string(),
            status: None,
        })?;
    work.mark_metadata();
    Box::pin(crate::steam_details::get_details_localized_with_policy(
        app,
        state.inner(),
        steam_app_id,
        policy,
        language.resolve(),
        &work,
    ))
    .await
}

#[tauri::command]
pub async fn reset_steam_artwork_cache(app: AppHandle, steam_app_id: u32) -> Result<(), String> {
    crate::steam_assets::reset(app, steam_app_id).await
}

#[tauri::command]
pub fn get_artwork_display_width(app: AppHandle) -> u32 {
    crate::artwork_display::refresh(&app)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn get_steam_asset(
    app: AppHandle,
    state: State<'_, NetworkState>,
    steam_app_id: u32,
    asset: crate::steam_assets::AssetKind,
    index: Option<usize>,
    full: Option<bool>,
    policy: crate::steam_assets::AssetPolicy,
    screenshot_identity: Option<String>,
    request_id: String,
    origin: crate::artwork_scheduler::WorkOrigin,
    publication: crate::artwork_publication::PublicationMode,
) -> Result<tauri::ipc::Response, crate::artwork_scheduler::WorkError> {
    Box::pin(crate::steam_assets::get_asset_for_request(
        app,
        state.inner(),
        crate::steam_assets::AssetRequest {
            app_id: steam_app_id,
            asset,
            index,
            full: full.unwrap_or(false),
            policy,
            screenshot_identity,
        },
        crate::steam_assets::AssetDemand {
            request_id,
            origin,
            publication,
        },
    ))
    .await?
    .into_response()
    .map_err(crate::artwork_scheduler::WorkError::Failed)
}
#[tauri::command]
pub async fn finish_steam_asset_update(
    state: State<'_, crate::steam_assets::AssetCacheState>,
    token: String,
    outcome: crate::artwork_publication::PublicationOutcome,
) -> Result<crate::artwork_publication::PublicationResult, String> {
    let cache = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cache.finish(&token, outcome))
        .await
        .map_err(|error| format!("Artwork publication task failed: {error}"))?
}
#[tauri::command]
pub fn is_aur_package() -> bool {
    std::env::var_os("LEGIO_AUR_PACKAGE").is_some()
}

#[tauri::command]
pub async fn get_news(app: AppHandle) -> Result<crate::news::Snapshot, String> {
    crate::news::cached(app).await
}

#[tauri::command]
pub async fn refresh_news(
    app: AppHandle,
    state: State<'_, crate::network::NetworkState>,
) -> Result<crate::news::Snapshot, String> {
    crate::news::refresh(app, state.inner()).await
}

#[tauri::command]
pub async fn prefetch_steam_hero(
    app: AppHandle,
    state: State<'_, NetworkState>,
    steam_app_id: u32,
) -> Result<(), String> {
    Box::pin(crate::steam_assets::get_asset(
        app,
        state.inner(),
        steam_app_id,
        crate::steam_assets::AssetKind::Hero,
        None,
        false,
        false,
    ))
    .await
    .map(|_| ())
}

#[tauri::command]
pub async fn add_download_source(
    app: tauri::AppHandle,
    state: State<'_, NetworkState>,
    url: String,
) -> Result<legio_source_cache::SourceSnapshot, String> {
    legio_source_cache::add_source(app, state.inner(), url).await
}

#[tauri::command]
pub async fn remove_download_source(
    app: tauri::AppHandle,
    id: String,
) -> Result<legio_source_cache::SourceSnapshot, String> {
    legio_source_cache::remove_source(app, id).await
}

#[tauri::command]
pub fn take_source_links(
    state: State<'_, crate::source_links::SourceLinks>,
) -> Result<crate::source_links::PendingLinks, String> {
    state.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn future_size<A, F: std::future::Future>(_: impl FnOnce(A) -> F) -> usize {
        std::mem::size_of::<F>()
    }

    #[test]
    fn artwork_command_futures_fit_the_windows_ui_stack_budget() {
        // Tauri moves command futures through several frames on the 1 MiB UI stack.
        let asset = future_size(|(app, state)| {
            get_steam_asset(
                app,
                state,
                730,
                crate::steam_assets::AssetKind::HeroBlur,
                None,
                None,
                crate::steam_assets::AssetPolicy::CacheOrFetch,
                None,
                "future-size".into(),
                crate::artwork_scheduler::WorkOrigin::Visible,
                crate::artwork_publication::PublicationMode::Display,
            )
        });
        let publication = future_size(|state| {
            finish_steam_asset_update(
                state,
                "future-size".into(),
                crate::artwork_publication::PublicationOutcome::Displayed,
            )
        });
        let metadata = future_size(|(app, network, scheduler)| {
            get_steam_details(
                app,
                network,
                scheduler,
                730,
                crate::steam_details::DetailsPolicy::CacheOrFetch,
                crate::locale::LanguagePreference::English,
                "future-size".into(),
                crate::artwork_scheduler::WorkOrigin::Visible,
            )
        });
        assert!(
            publication <= 4096,
            "Artwork publication future occupies {publication} bytes"
        );
        assert!(
            metadata <= 4096,
            "Artwork metadata future occupies {metadata} bytes"
        );
        let prefetch = future_size(|(app, state)| prefetch_steam_hero(app, state, 730));
        assert!(
            asset <= 4096,
            "Artwork command future occupies {asset} bytes"
        );
        assert!(
            prefetch <= 4096,
            "Artwork prefetch future occupies {prefetch} bytes"
        );
    }
}

#[tauri::command]
pub fn set_artwork_game(
    state: State<'_, crate::artwork_scheduler::ArtworkScheduler>,
    steam_app_id: Option<u32>,
) -> Result<(), crate::artwork_scheduler::WorkError> {
    state.select_game(steam_app_id)
}
#[tauri::command]
pub fn cancel_artwork_request(
    state: State<'_, crate::artwork_scheduler::ArtworkScheduler>,
    request_id: String,
) -> Result<(), crate::artwork_scheduler::WorkError> {
    state.cancel(&request_id)
}
