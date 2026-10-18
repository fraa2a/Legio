use crate::{
    database::EffectiveCompatibilityConfig,
    runner_discovery::{self, InstalledRunner, RunnerKind},
};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};
use tauri::{AppHandle, Manager, Runtime};

pub(super) struct LaunchPaths<'a> {
    pub executable: &'a Path,
    pub working_directory: &'a Path,
    pub compat_data: &'a Path,
    pub wine_prefix: &'a Path,
    pub steam_root: Option<&'a Path>,
}

pub(super) fn prepare<R: Runtime>(
    app: &AppHandle<R>,
    game_id: &str,
    runner: &InstalledRunner,
    config: &EffectiveCompatibilityConfig,
    paths: LaunchPaths<'_>,
    launch_via_steam: bool,
) -> Result<Command, String> {
    let LaunchPaths {
        executable: executable_path,
        working_directory,
        compat_data: compat_data_path,
        wine_prefix,
        steam_root,
    } = paths;
    let direct_proton = config.online_fix && runner.kind != RunnerKind::Wine;
    let options = crate::compatibility_options::PreparedOptions::prepare(
        config,
        runner,
        steam_root,
        launch_via_steam,
    )?;
    let command = if direct_proton {
        let root = steam_root.ok_or("Steam installation is unavailable")?;
        let mut command =
            crate::proton_runtime::prepare(runner, root, compat_data_path, wine_prefix)?;
        command
            .args(&config.arguments_before)
            .arg(executable_path)
            .args(&config.arguments_after)
            .env(
                "STEAM_COMPAT_INSTALL_PATH",
                executable_path.parent().unwrap_or(working_directory),
            );
        command
    } else {
        let umu = if runner.kind != RunnerKind::Wine {
            Some(runner_discovery::umu_path()?)
        } else {
            None
        };
        runner_discovery::launch_command(
            runner,
            executable_path,
            &config.arguments_before,
            &config.arguments_after,
            umu.as_deref(),
        )?
    };
    let mut command = crate::linux_performance::wrap(command, &config.linux_performance)?;
    command.current_dir(working_directory).stdin(Stdio::null());
    options.apply(&mut command, config)?;
    if !config.environment.contains_key("WINEDEBUG") && !config.debug_logging {
        command.env("WINEDEBUG", "-all");
    }
    super::apply_dll_overrides(&mut command, config);
    command.env("WINEPREFIX", wine_prefix);
    if !direct_proton {
        command.env_remove("STEAM_COMPAT_DATA_PATH");
    }
    if let Some(steam_root) = steam_root {
        command.env("STEAM_COMPAT_CLIENT_INSTALL_PATH", steam_root);
    } else {
        command.env_remove("STEAM_COMPAT_CLIENT_INSTALL_PATH");
    }
    if runner.kind != RunnerKind::Wine
        && !config.environment.contains_key("VKD3D_SHADER_CACHE_PATH")
        && std::env::var_os("VKD3D_SHADER_CACHE_PATH").is_none()
    {
        let shader_cache = app
            .path()
            .app_cache_dir()
            .map_err(|error| format!("Could not resolve shader cache directory: {error}"))?
            .join("compatibility-shaders")
            .join(game_id);
        fs::create_dir_all(&shader_cache)
            .map_err(|error| format!("Could not create game shader cache: {error}"))?;
        command.env("VKD3D_SHADER_CACHE_PATH", shader_cache);
    }
    Ok(command)
}
