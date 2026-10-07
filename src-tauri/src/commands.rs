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
pub fn get_app_info(app: AppHandle) -> AppInfo {
    let package_info = app.package_info();

    AppInfo {
        name: package_info.name.clone(),
        version: package_info.version.to_string(),
        platform: std::env::consts::OS.to_owned(),
        tray_available: app.state::<crate::TrayAvailable>().0,
        desktop_environment: desktop_environment(),
        startup_launch_game_id: app.state::<crate::StartupLaunch>().game_id.clone(),
        startup_launch_error: app.state::<crate::StartupLaunch>().error.clone(),
    }
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
    state.database()?.games()
}

#[tauri::command]
pub fn get_playtime_summaries(
    state: State<'_, DatabaseState>,
) -> Result<Vec<database::PlaytimeSummary>, String> {
    state
        .database()?
        .playtime_summaries(database::now_milliseconds()?)
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
pub async fn extract_game_icon(
    app: AppHandle,
    game_id: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let game = app.state::<DatabaseState>().database()?.game(&game_id)?;
        crate::game_artwork::extract_for_game(
            &app.state::<crate::game_artwork::GameArtworkStore>(),
            &game,
        )
    })
    .await
    .map_err(|error| format!("Game icon extraction task failed: {error}"))?
    .and_then(crate::game_artwork::GameArtworkResult::into_response)
}

#[tauri::command]
pub async fn get_game_icon(
    app: AppHandle,
    game_id: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DatabaseState>().database()?.game(&game_id)?;
        let artwork = app
            .state::<crate::game_artwork::GameArtworkStore>()
            .get(&game_id, crate::game_artwork::ArtworkKind::Icon)?;
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
pub fn update_game(
    state: State<'_, DatabaseState>,
    input: UpdateGameInput,
) -> Result<Game, String> {
    state.database()?.update_game(input)
}

#[tauri::command]
pub fn remove_game(app: AppHandle, id: String) -> Result<(), String> {
    let manager = app.state::<crate::game_lifecycle::GameLaunchManager>();
    let _operation = manager.operation()?;
    manager.require_idle(&id)?;
    app.state::<DatabaseState>().database()?.remove_game(&id)?;
    let mut errors = Vec::new();
    if let Err(error) = app
        .state::<crate::game_artwork::GameArtworkStore>()
        .remove_for_game(&id)
    {
        errors.push(format!("custom artwork: {error}"));
    }
    if let Err(error) = crate::desktop_shortcuts::remove(&id) {
        errors.push(format!("desktop shortcuts: {error}"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Game was removed, but cleanup failed: {}",
            errors.join("; ")
        ))
    }
}

#[tauri::command]
pub async fn launch_steam_game(
    app: AppHandle,
    game_id: String,
    confirm_account_switch: bool,
) -> Result<crate::steam_switch::SteamLaunchResult, String> {
    app.state::<crate::game_lifecycle::GameLaunchManager>()
        .launch(app.clone(), game_id, confirm_account_switch)
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
        manager.launch_with_runner(app, game_id, runner_path)
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
    tauri::async_runtime::spawn_blocking(move || manager.launch_configured(app, game_id))
        .await
        .map_err(|error| format!("Compatibility runner launch task failed: {error}"))?
}

#[tauri::command]
pub async fn launch_native_game(app: AppHandle, game_id: String) -> Result<(), String> {
    let manager = app
        .state::<crate::game_lifecycle::GameLaunchManager>()
        .inner()
        .clone();
    tauri::async_runtime::spawn_blocking(move || manager.launch_native(app, game_id))
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
    tauri::async_runtime::spawn_blocking(move || {
        let game = manual_import::import(&app.state::<DatabaseState>(), input)?;
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
    tauri::async_runtime::spawn_blocking(move || {
        let game = manual_import::set_executable(
            &app.state::<DatabaseState>(),
            &game_id,
            &executable_path,
        )?;
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
    legio_source_cache::refresh_source(app, state.inner()).await
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
) -> Result<crate::catalog::CatalogSearch, crate::catalog::CatalogError> {
    crate::catalog::search_catalog(app, query, limit).await
}

#[tauri::command]
pub async fn refresh_catalog(
    app: AppHandle,
    state: State<'_, NetworkState>,
    query: String,
    skip: Option<usize>,
    limit: Option<u32>,
) -> Result<crate::catalog::CatalogSearch, crate::catalog::CatalogError> {
    crate::catalog::refresh_catalog(app, state.inner(), query, skip, limit).await
}

#[tauri::command]
pub async fn get_steam_details(
    app: AppHandle,
    state: State<'_, NetworkState>,
    steam_app_id: u32,
    refresh: bool,
    language: crate::locale::LanguagePreference,
) -> Result<crate::steam_details::DetailsResult, crate::steam_details::DetailsError> {
    crate::steam_details::get_details_localized(
        app,
        state.inner(),
        steam_app_id,
        refresh,
        language.resolve(),
    )
    .await
}

#[tauri::command]
pub async fn get_steam_asset(
    app: AppHandle,
    state: State<'_, NetworkState>,
    steam_app_id: u32,
    asset: crate::steam_assets::AssetKind,
    index: Option<usize>,
    full: Option<bool>,
    refresh: Option<bool>,
) -> Result<tauri::ipc::Response, String> {
    crate::steam_assets::get_asset(
        app,
        state.inner(),
        steam_app_id,
        asset,
        index,
        full.unwrap_or(false),
        refresh.unwrap_or(false),
    )
    .await?
    .into_response()
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
    crate::steam_assets::get_asset(
        app,
        state.inner(),
        steam_app_id,
        crate::steam_assets::AssetKind::Hero,
        None,
        false,
        false,
    )
    .await
    .map(|_| ())
}
