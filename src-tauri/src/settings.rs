use crate::database::{self, DatabaseState, Settings};
use tauri::{AppHandle, Manager};

pub(crate) fn save(app: &AppHandle, settings: Settings) -> Result<Settings, String> {
    crate::appearance::validate_background(app, &settings.appearance)?;
    let state = app.state::<DatabaseState>();
    let previous = state.database()?.settings()?;
    let saved = database::save_settings(&state, settings)?;
    if saved.launch_on_system_start != previous.launch_on_system_start
        && let Err(error) = crate::startup::set_enabled(saved.launch_on_system_start)
    {
        state.database()?.save_settings(previous.clone())?;
        return Err(error);
    }
    app.state::<crate::diagnostics::Diagnostics>()
        .set_enabled(saved.diagnostics_enabled);
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let root = state.database()?.storage_root(&data_dir)?;
    app.state::<crate::download_queue::DownloadQueueState>()
        .set_storage_root(&root)?;
    if saved.language != previous.language
        && let Some(tray) = app.tray_by_id("main-tray")
    {
        use tauri::menu::{Menu, MenuItem};
        let open = MenuItem::with_id(
            app,
            "open",
            saved.language.text("Apri", "Open"),
            true,
            None::<&str>,
        )
        .map_err(|error| error.to_string())?;
        let quit = MenuItem::with_id(
            app,
            "quit",
            saved.language.text("Chiudi", "Quit"),
            true,
            None::<&str>,
        )
        .map_err(|error| error.to_string())?;
        let menu = Menu::with_items(app, &[&open, &quit]).map_err(|error| error.to_string())?;
        tray.set_menu(Some(menu))
            .map_err(|error| error.to_string())?;
    }
    Ok(saved)
}
