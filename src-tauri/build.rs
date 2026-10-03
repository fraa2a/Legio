use std::path::Path;

fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "is_aur_package",
            "get_app_info",
            "get_settings",
            "save_settings",
            "get_compatibility_defaults",
            "save_compatibility_defaults",
            "get_game_compatibility_overrides",
            "get_game_online_fix_detected",
            "save_game_compatibility_overrides",
            "get_native_launch_config",
            "save_native_launch_config",
            "get_steam_launch_config",
            "save_steam_launch_config",
            "list_games",
            "get_playtime_summaries",
            "get_playtime_activity",
            "create_game",
            "create_game_shortcut",
            "transfer_game",
            "set_game_icon",
            "extract_game_icon",
            "get_game_icon",
            "reset_game_icon",
            "set_game_banner",
            "get_game_banner",
            "reset_game_banner",
            "update_game",
            "remove_game",
            "launch_steam_game",
            "launch_game_with_runner",
            "launch_configured_game_with_runner",
            "launch_native_game",
            "list_game_launch_states",
            "get_compatibility_logs_directory",
            "cancel_game_launch",
            "stop_game",
            "inspect_steam_game_launch",
            "list_saved_steam_accounts",
            "set_game_steam_account_preference",
            "check_game_steam_account",
            "scan_steam_installations",
            "import_steam_installations",
            "scan_game_executables",
            "import_manual_game",
            "identify_manual_game_steam_app_id",
            "preview_manual_game_steam_app_id",
            "set_game_executable",
            "get_network_status",
            "get_network_log_status",
            "get_legio_source",
            "list_downloads",
            "finalize_download",
            "scan_staged_executables",
            "queue_download",
            "pause_download",
            "resume_download",
            "retry_download",
            "cancel_download",
            "set_download_bandwidth_limit",
            "get_download_bandwidth_limit",
            "remove_download",
            "remove_finished_downloads",
            "reorder_downloads",
            "open_installed_folder",
            "get_installed_folder_info",
            "refresh_legio_source",
            "check_steam_connectivity",
            "search_catalog",
            "refresh_catalog",
            "get_steam_details",
            "get_steam_asset",
            "list_compatibility_runners",
        ]),
    ))
    .expect("Tauri build configuration failed");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-test.manifest");
        println!("cargo::rerun-if-changed={}", manifest.display());
        println!("cargo::rerun-if-env-changed=LEGIO_WINDOWS_TESTS");
        if std::env::var_os("LEGIO_WINDOWS_TESTS").is_some() {
            println!("cargo::rustc-link-arg=/MANIFEST:EMBED");
            println!(
                "cargo::rustc-link-arg=/MANIFESTINPUT:{}",
                manifest.display()
            );
        }
    }
}
