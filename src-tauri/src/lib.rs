use tauri::Manager;

mod catalog;
mod commands;
mod database;
mod network;
mod steam_local;

pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(database::DatabaseState::new(app.path().app_data_dir()));
            app.manage(
                network::NetworkState::new(&app.package_info().version.to_string())
                    .map_err(std::io::Error::other)?,
            );
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
            commands::scan_steam_installations,
            commands::get_network_status,
            commands::check_steam_connectivity,
            commands::search_catalog,
            commands::refresh_catalog,
        ])
        .run(tauri::generate_context!())
}
