#[cfg(not(any(target_os = "linux", windows)))]
compile_error!("Legio supports Linux and Windows only");

use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

pub mod archive_install;
mod catalog;
mod commands;
#[cfg(target_os = "linux")]
mod compatibility_logs;
#[cfg(target_os = "linux")]
mod compatibility_options;
mod database;
mod desktop_shortcuts;
mod diagnostics;
mod download_queue;
mod finalize_install;
mod game_artwork;
mod game_lifecycle;
mod game_process;
mod game_transfer;
mod image_format;
mod image_trim;
mod installed_folder;
pub mod legio_source;
mod legio_source_cache;
mod locale;
mod manual_import;
mod network;
mod online_fix;
mod pe_icons;
mod playtime_activity;
mod runner_discovery;
mod settings;
mod startup;

struct TrayAvailable(bool);
#[derive(Default)]
pub(crate) struct StartupLaunch {
    pub game_id: Option<String>,
    pub error: Option<String>,
}
mod steam_assets;
mod steam_details;
mod steam_import;
mod steam_local;
mod steam_pics;
mod steam_process;
mod steam_switch;
mod steam_vdf;

pub fn run() -> tauri::Result<()> {
    let shortcut_game_id = desktop_shortcuts::requested_game_id(std::env::args_os().skip(1))
        .map_err(std::io::Error::other)?;

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let handle = app.clone();
            if let Err(error) = app.run_on_main_thread(move || {
                show_main_window(&handle);
                match desktop_shortcuts::requested_game_id(
                    args.into_iter().skip(1).map(std::ffi::OsString::from),
                ) {
                    Ok(Some(id)) => {
                        if let Err(error) = launch_shortcut(&handle, &id) {
                            eprintln!("Could not launch forwarded shortcut: {error}");
                        }
                    }
                    Ok(None) => {}
                    Err(error) => eprintln!("Invalid forwarded shortcut: {error}"),
                }
            }) {
                eprintln!("Could not forward second-instance request: {error}");
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
            let database = database::DatabaseState::new(app.path().app_data_dir());
            let bandwidth_limit = database
                .database()
                .and_then(database::Database::download_bandwidth_limit)
                .map_err(std::io::Error::other)?;
            game_transfer::recover(database.database()?).map_err(std::io::Error::other)?;
            app.manage(database);
            let download_queue = download_queue::DownloadQueueState::new(
                app.path().app_data_dir(),
                &app.package_info().version.to_string(),
            )
            .map_err(std::io::Error::other)?;
            download_queue.set_bandwidth_limit(bandwidth_limit);
            let storage_root = app
                .state::<database::DatabaseState>()
                .database()
                .and_then(|database| {
                    database.storage_root(
                        &app.path()
                            .app_data_dir()
                            .map_err(|error| error.to_string())?,
                    )
                })
                .map_err(std::io::Error::other)?;
            download_queue
                .set_storage_root(&storage_root)
                .map_err(std::io::Error::other)?;
            app.manage(download_queue);
            download_queue::start(app.handle().clone()).map_err(std::io::Error::other)?;
            #[cfg(target_os = "linux")]
            app.manage(game_lifecycle::GameLaunchManager::with_log_directory(
                app.path().app_log_dir().map_err(|error| error.to_string()),
            ));
            #[cfg(not(target_os = "linux"))]
            app.manage(game_lifecycle::GameLaunchManager::new());
            let diagnostics = diagnostics::Diagnostics::new(
                app.path().app_log_dir().map_err(|error| error.to_string()),
            );
            app.manage(steam_assets::AssetCacheState::new(
                app.path().app_cache_dir(),
            ));
            app.manage(game_artwork::GameArtworkStore::new(
                app.path().app_data_dir().map_err(|error| error.to_string()),
            ));
            app.manage(
                network::NetworkState::new(
                    &app.package_info().version.to_string(),
                    diagnostics.clone(),
                )
                .map_err(std::io::Error::other)?,
            );
            app.manage(diagnostics);
            let language = app
                .state::<database::DatabaseState>()
                .database()?
                .settings()?
                .language;
            let open = MenuItem::with_id(
                app,
                "open",
                language.text("Apri", "Open"),
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(
                app,
                "quit",
                language.text("Chiudi", "Quit"),
                true,
                None::<&str>,
            )?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let icon = image::load_from_memory(include_bytes!("../icons/tray.png"))
                .map_err(std::io::Error::other)?
                .to_rgba8();
            let (width, height) = icon.dimensions();
            let icon = tauri::image::Image::new_owned(icon.into_raw(), width, height);
            let tray_result = TrayIconBuilder::with_id("main-tray")
                .icon(icon)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => {
                        show_main_window(app);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app);
            if let Err(error) = &tray_result {
                eprintln!("Could not create system tray icon: {error}");
            }
            app.manage(TrayAvailable(tray_result.is_ok()));
            if std::env::args_os().any(|argument| argument == "--minimized") {
                let minimized = app
                    .state::<database::DatabaseState>()
                    .database()
                    .and_then(database::Database::settings)
                    .is_ok_and(|settings| {
                        settings.launch_on_system_start && settings.launch_minimized
                    });
                if minimized
                    && tray_result.is_ok()
                    && let Some(window) = app.get_webview_window("main")
                {
                    window.hide()?;
                }
            }

            let mut startup_launch = StartupLaunch::default();
            if let Some(game_id) = shortcut_game_id {
                startup_launch.game_id = Some(game_id.clone());
                let launch_result = launch_shortcut(app.handle(), &game_id);
                if let Err(error) = launch_result {
                    eprintln!("Could not launch game from shortcut: {error}");
                    startup_launch.error = Some(error);
                }
            }
            app.manage(startup_launch);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let close_to_tray = window
                    .app_handle()
                    .state::<database::DatabaseState>()
                    .database()
                    .and_then(database::Database::settings)
                    .is_ok_and(|settings| settings.close_to_tray);
                if close_to_tray && window.app_handle().state::<TrayAvailable>().0 {
                    api.prevent_close();
                    if let Err(error) = window.hide() {
                        eprintln!("Could not hide main window: {error}");
                    }
                } else {
                    api.prevent_close();
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::is_aur_package,
            commands::get_app_info,
            commands::get_settings,
            commands::save_settings,
            commands::get_compatibility_defaults,
            commands::save_compatibility_defaults,
            commands::get_game_compatibility_overrides,
            commands::get_game_online_fix_detected,
            commands::save_game_compatibility_overrides,
            commands::get_native_launch_config,
            commands::save_native_launch_config,
            commands::get_steam_launch_config,
            commands::save_steam_launch_config,
            commands::list_games,
            commands::get_playtime_summaries,
            commands::get_playtime_activity,
            commands::create_game,
            commands::create_game_shortcut,
            game_transfer::transfer_game,
            commands::set_game_icon,
            commands::extract_game_icon,
            commands::get_game_icon,
            commands::reset_game_icon,
            commands::set_game_banner,
            commands::get_game_banner,
            commands::reset_game_banner,
            commands::update_game,
            commands::remove_game,
            commands::launch_steam_game,
            commands::launch_game_with_runner,
            commands::launch_configured_game_with_runner,
            commands::launch_native_game,
            commands::list_game_launch_states,
            commands::get_compatibility_logs_directory,
            commands::cancel_game_launch,
            commands::stop_game,
            commands::inspect_steam_game_launch,
            commands::list_saved_steam_accounts,
            commands::set_game_steam_account_preference,
            commands::check_game_steam_account,
            commands::scan_steam_installations,
            commands::import_steam_installations,
            commands::scan_game_executables,
            commands::import_manual_game,
            commands::identify_manual_game_steam_app_id,
            commands::preview_manual_game_steam_app_id,
            commands::set_game_executable,
            commands::get_network_status,
            commands::get_network_log_status,
            commands::get_legio_source,
            download_queue::list_downloads,
            finalize_install::finalize_download,
            finalize_install::scan_staged_executables,
            download_queue::queue_download,
            download_queue::pause_download,
            download_queue::resume_download,
            download_queue::retry_download,
            download_queue::cancel_download,
            download_queue::set_download_bandwidth_limit,
            download_queue::get_download_bandwidth_limit,
            download_queue::remove_download,
            download_queue::remove_finished_downloads,
            download_queue::reorder_downloads,
            installed_folder::open_installed_folder,
            installed_folder::get_installed_folder_info,
            commands::refresh_legio_source,
            commands::check_steam_connectivity,
            commands::search_catalog,
            commands::refresh_catalog,
            commands::get_steam_details,
            commands::get_steam_asset,
            runner_discovery::list_compatibility_runners,
        ])
        .run(tauri::generate_context!())
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main")
        && let Err(error) = window.show().and_then(|()| window.set_focus())
    {
        eprintln!("Could not show main window: {error}");
    }
}

fn launch_shortcut(app: &tauri::AppHandle, game_id: &str) -> Result<(), String> {
    let game = app
        .state::<database::DatabaseState>()
        .database()?
        .game(game_id)?;
    let manager = app.state::<game_lifecycle::GameLaunchManager>();
    if game.steam_install_path.is_some() {
        manager
            .launch(app.clone(), game_id.to_owned(), false)
            .map(|_| ())
    } else {
        #[cfg(target_os = "linux")]
        {
            manager.launch_configured(app.clone(), game_id.to_owned())
        }
        #[cfg(windows)]
        {
            manager.launch_native(app.clone(), game_id.to_owned())
        }
    }
}
