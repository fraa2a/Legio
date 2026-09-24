use tauri::Manager;

pub mod archive_install;
mod catalog;
mod commands;
mod database;
mod diagnostics;
mod download_queue;
mod game_lifecycle;
mod game_process;
pub mod legio_source;
mod legio_source_cache;
mod network;
mod steam_assets;
mod steam_details;
mod steam_import;
mod steam_local;
mod steam_process;
mod steam_switch;
mod steam_vdf;

pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(database::DatabaseState::new(app.path().app_data_dir()));
            app.manage(
                download_queue::DownloadQueueState::new(
                    app.path().app_data_dir(),
                    &app.package_info().version.to_string(),
                )
                .map_err(std::io::Error::other)?,
            );
            download_queue::start(app.handle().clone()).map_err(std::io::Error::other)?;
            app.manage(game_lifecycle::GameLaunchManager::new());
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
            commands::list_game_launch_states,
            commands::cancel_game_launch,
            commands::stop_game,
            commands::inspect_steam_game_launch,
            commands::list_saved_steam_accounts,
            commands::set_game_steam_account_preference,
            commands::check_game_steam_account,
            commands::scan_steam_installations,
            commands::import_steam_installations,
            commands::get_network_status,
            commands::get_network_log_status,
            commands::get_legio_source,
            download_queue::list_downloads,
            download_queue::queue_download,
            download_queue::pause_download,
            download_queue::resume_download,
            download_queue::retry_download,
            download_queue::cancel_download,
            download_queue::set_download_bandwidth_limit,
            commands::refresh_legio_source,
            commands::check_steam_connectivity,
            commands::search_catalog,
            commands::refresh_catalog,
            commands::get_steam_details,
            commands::get_steam_asset,
        ])
        .run(tauri::generate_context!())
}
