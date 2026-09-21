use tauri::Manager;

mod commands;
mod database;
mod steam_local;

pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(database::DatabaseState::new(app.path().app_data_dir()));
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
        ])
        .run(tauri::generate_context!())
}
