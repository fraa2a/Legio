use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::{
    database::{
        self, AccountCheck, CreateGameInput, DatabaseState, Game, Settings, UpdateGameInput,
    },
    diagnostics::{Diagnostics, LogStatus},
    legio_source,
    network::{ConnectivityCheck, NetworkState, NetworkStatus},
    steam_local,
};

#[derive(Serialize)]
pub struct AppInfo {
    name: String,
    version: String,
    platform: String,
}

#[tauri::command]
pub fn get_app_info(app: AppHandle) -> AppInfo {
    let package_info = app.package_info();

    AppInfo {
        name: package_info.name.clone(),
        version: package_info.version.to_string(),
        platform: std::env::consts::OS.to_owned(),
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, DatabaseState>) -> Result<Settings, String> {
    database::get_settings(&state)
}

#[tauri::command]
pub fn save_settings(
    state: State<'_, DatabaseState>,
    settings: Settings,
) -> Result<Settings, String> {
    database::save_settings(&state, settings)
}

#[tauri::command]
pub fn list_games(state: State<'_, DatabaseState>) -> Result<Vec<Game>, String> {
    database::list_games(&state)
}

#[tauri::command]
pub fn create_game(
    state: State<'_, DatabaseState>,
    input: CreateGameInput,
) -> Result<Game, String> {
    database::create_game(&state, input)
}

#[tauri::command]
pub fn update_game(
    state: State<'_, DatabaseState>,
    input: UpdateGameInput,
) -> Result<Game, String> {
    database::update_game(&state, input)
}

#[tauri::command]
pub fn remove_game(state: State<'_, DatabaseState>, id: String) -> Result<(), String> {
    database::remove_game(&state, &id)
}

#[tauri::command]
pub async fn launch_steam_game(
    app: AppHandle,
    game_id: String,
) -> Result<crate::steam_launch::SteamLaunchResult, String> {
    tauri::async_runtime::spawn_blocking(move || crate::steam_launch::launch(&app, &game_id))
        .await
        .map_err(|error| format!("Steam launch task failed: {error}"))?
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
        database::set_game_steam_account(
            &app.state::<DatabaseState>(),
            &game_id,
            steam_id.as_deref(),
        )
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
        let check = database::check_game_steam_account(&app.state::<DatabaseState>(), &game_id)?;
        if check.selected_steam_id.is_none() {
            return Ok(check);
        }
        Ok(check_saved_account_presence(
            check,
            &steam_local::scan_saved_accounts(),
        ))
    })
    .await
    .map_err(|error| format!("Steam account check task failed: {error}"))?
}

fn check_saved_account_presence(
    mut check: AccountCheck,
    saved: &steam_local::SavedSteamAccounts,
) -> AccountCheck {
    if let Some(ref id) = check.selected_steam_id
        && !saved.accounts.iter().any(|account| account.steam_id == *id)
        && saved.diagnostics.is_empty()
    {
        check.status = database::AccountCheckStatus::MissingSavedAccount;
        check.message = Some(
            "Selected Steam account is no longer saved. Choose another saved account or clear the preference.",
        );
    }
    check
}

#[cfg(test)]
mod account_tests {
    use super::*;
    use database::AccountCheckStatus;

    #[test]
    fn missing_saved_account_is_distinct_from_uncertain_metadata() {
        let check = AccountCheck {
            status: AccountCheckStatus::Unknown,
            account_requirement_met: false,
            selected_steam_id: Some("76561198000000001".to_owned()),
            message: None,
        };
        let empty = steam_local::SavedSteamAccounts {
            accounts: vec![],
            diagnostics: vec![],
        };
        let missing = check_saved_account_presence(check.clone(), &empty);
        assert_eq!(missing.status, AccountCheckStatus::MissingSavedAccount);
        assert!(!missing.account_requirement_met);
        assert!(missing.message.unwrap().contains("Choose another"));
        let uncertain = steam_local::SavedSteamAccounts {
            accounts: vec![],
            diagnostics: vec!["unreadable metadata".to_owned()],
        };
        assert_eq!(
            check_saved_account_presence(check.clone(), &uncertain).status,
            AccountCheckStatus::Unknown
        );
        let saved = steam_local::SavedSteamAccounts {
            accounts: vec![steam_local::SavedSteamAccount {
                steam_id: "76561198000000001".to_owned(),
                display_name: "Display".to_owned(),
            }],
            diagnostics: vec![],
        };
        assert_eq!(
            check_saved_account_presence(check, &saved).status,
            AccountCheckStatus::Unknown
        );
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
pub fn get_network_status(state: State<'_, NetworkState>) -> Result<NetworkStatus, String> {
    state.status()
}

#[tauri::command]
pub fn get_network_log_status(state: State<'_, Diagnostics>) -> LogStatus {
    state.status()
}

#[tauri::command]
pub async fn get_legio_source(
    state: State<'_, NetworkState>,
) -> Result<legio_source::Manifest, String> {
    legio_source::fetch_manifest(state.inner()).await
}

#[tauri::command]
pub async fn check_steam_connectivity(
    state: State<'_, NetworkState>,
) -> Result<ConnectivityCheck, String> {
    Ok(state.inner().clone().check_connectivity().await)
}

#[tauri::command]
pub async fn search_catalog(
    app: AppHandle,
    query: String,
) -> Result<crate::catalog::CatalogSearch, crate::catalog::CatalogError> {
    crate::catalog::search_catalog(app, query).await
}

#[tauri::command]
pub async fn refresh_catalog(
    app: AppHandle,
    state: State<'_, NetworkState>,
    query: String,
) -> Result<crate::catalog::CatalogSearch, crate::catalog::CatalogError> {
    crate::catalog::refresh_catalog(app, state.inner(), query).await
}

#[tauri::command]
pub async fn get_steam_details(
    app: AppHandle,
    state: State<'_, NetworkState>,
    steam_app_id: u32,
    refresh: bool,
) -> Result<crate::steam_details::DetailsResult, crate::steam_details::DetailsError> {
    crate::steam_details::get_details(app, state.inner(), steam_app_id, refresh).await
}

#[tauri::command]
pub async fn get_steam_asset(
    app: AppHandle,
    state: State<'_, NetworkState>,
    steam_app_id: u32,
    asset: crate::steam_assets::AssetKind,
    index: Option<usize>,
) -> Result<crate::steam_assets::AssetResult, String> {
    crate::steam_assets::get_asset(app, state.inner(), steam_app_id, asset, index).await
}
