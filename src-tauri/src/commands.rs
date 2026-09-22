use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::{
    database::{self, CreateGameInput, DatabaseState, Game, Settings, UpdateGameInput},
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
pub fn scan_steam_installations() -> steam_local::SteamScan {
    steam_local::scan_default_installations()
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
