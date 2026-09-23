use tauri::Manager;

mod catalog;
mod commands;
mod database;
mod diagnostics;
mod network;
mod steam_assets;
mod steam_details;
mod steam_import;
mod steam_launch;
mod steam_local;

pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .setup(|app| {
            app.manage(database::DatabaseState::new(app.path().app_data_dir()));
            let diagnostics = diagnostics::Diagnostics::new(
                app.path().app_log_dir().map_err(|error| error.to_string()),
            );
            app.manage(steam_assets::AssetCacheState::new(
                app.path().app_cache_dir(),
            ));
            app.manage(
                network::NetworkState::new(
                    &app.package_info().version.to_string(),
                    diagnostics.clone(),
                )
                .map_err(std::io::Error::other)?,
            );
            app.manage(diagnostics);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_settings,
            commands::save_settings,
            commands::list_games,
            commands::create_game,
            commands::update_game,
            commands::remove_game,
            commands::launch_steam_game,
            commands::list_saved_steam_accounts,
            commands::set_game_steam_account_preference,
            commands::check_game_steam_account,
            commands::scan_steam_installations,
            commands::import_steam_installations,
            commands::get_network_status,
            commands::get_network_log_status,
            commands::check_steam_connectivity,
            commands::search_catalog,
            commands::refresh_catalog,
            commands::get_steam_details,
            commands::get_steam_asset,
        ])
        .run(tauri::generate_context!())
}
