use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::database::{
    AppliedCompatibilityOptions, EffectiveCompatibilityConfig, GraphicsRenderer, WaylandMode,
};
use crate::runner_discovery::{InstalledRunner, RunnerKind};

const STEAM_OVERLAY_LAYER: &str = "ENABLE_VK_LAYER_VALVE_steam_overlay_1";
const STEAM_OVERLAY_GAME_ID: &str = "SteamOverlayGameId";
const PROTON_LOG: &str = "PROTON_LOG";
const PROTON_LOG_DIR: &str = "PROTON_LOG_DIR";
const STEAM_GAME_ID: &str = "SteamGameId";
const WINE_DEBUG: &str = "+timestamp,+pid,+tid,+seh";

#[derive(Debug)]
pub(crate) struct PreparedOptions {
    overlay_libraries: Option<[PathBuf; 2]>,
}

impl PreparedOptions {
    pub(crate) fn prepare(
        config: &EffectiveCompatibilityConfig,
        runner: &InstalledRunner,
        steam_root: Option<&Path>,
        launch_via_steam: bool,
    ) -> Result<Self, String> {
        validate(config, runner)?;
        if launch_via_steam && !matches!(runner.kind, RunnerKind::Proton | RunnerKind::GeProton) {
            return Err("Launch via Steam requires a Proton runner".to_owned());
        }

        let overlay_libraries = if launch_via_steam {
            let steam_root = steam_root
                .ok_or_else(|| "Launch via Steam requires a Steam installation".to_owned())?;
            match load_overlay_libraries(steam_root) {
                Ok(libraries) => Some(libraries),
                Err(error) => {
                    eprintln!("Steam overlay unavailable, launching without it: {error}");
                    None
                }
            }
        } else {
            None
        };

        Ok(Self { overlay_libraries })
    }

    pub(crate) fn diagnostic(
        runner: &InstalledRunner,
        config: &EffectiveCompatibilityConfig,
        launch_via_steam: bool,
    ) -> AppliedCompatibilityOptions {
        AppliedCompatibilityOptions {
            runner: runner.name.clone(),
            version: runner.version.clone(),
            launch_via_steam,
            graphics_renderer: config.graphics_renderer,
            wayland: config.wayland,
            debug_logging: config.debug_logging,
        }
    }

    pub(crate) fn apply(
        &self,
        command: &mut Command,
        config: &EffectiveCompatibilityConfig,
    ) -> Result<(), String> {
        for (key, value) in &config.environment {
            command.env(key, value);
        }

        if !config.debug_logging && !config.environment.contains_key(PROTON_LOG) {
            command.env_remove(PROTON_LOG);
            if !config.environment.contains_key(PROTON_LOG_DIR) {
                command.env_remove(PROTON_LOG_DIR);
            }
        }

        match config.graphics_renderer {
            GraphicsRenderer::RunnerDefault => {
                command.env_remove("PROTON_USE_WINED3D");
            }
            GraphicsRenderer::WineD3d => {
                command.env("PROTON_USE_WINED3D", "1");
            }
        }
        match config.wayland {
            WaylandMode::RunnerDefault => {
                command.env_remove("PROTON_ENABLE_WAYLAND");
            }
            WaylandMode::Disabled => {
                command.env("PROTON_ENABLE_WAYLAND", "0");
            }
            WaylandMode::Native => {
                command.env("PROTON_ENABLE_WAYLAND", "1");
            }
        }
        self.apply_overlay(command, config)
    }

    pub(crate) fn apply_debug_logging(
        command: &mut Command,
        config: &EffectiveCompatibilityConfig,
        runner: &InstalledRunner,
        log_directory: &Path,
    ) {
        if !config.debug_logging {
            return;
        }
        match runner.kind {
            RunnerKind::Proton | RunnerKind::GeProton => {
                command
                    .env(PROTON_LOG, "1")
                    .env(PROTON_LOG_DIR, log_directory);
            }
            RunnerKind::Wine if !config.environment.contains_key("WINEDEBUG") => {
                command.env("WINEDEBUG", WINE_DEBUG);
            }
            RunnerKind::Wine => {}
        }
    }

    fn apply_overlay(
        &self,
        command: &mut Command,
        config: &EffectiveCompatibilityConfig,
    ) -> Result<(), String> {
        if let Some(libraries) = self.overlay_libraries.as_ref() {
            let preload = overlay_preload(&config.environment, libraries)?;
            command
                .env("LD_PRELOAD", preload)
                .env(STEAM_OVERLAY_LAYER, "1")
                .env(STEAM_OVERLAY_GAME_ID, "480");
        } else {
            command.env(STEAM_OVERLAY_LAYER, "0");
            command.env_remove(STEAM_OVERLAY_GAME_ID);
            if let Some(preload) = preload_without_overlay(&config.environment)? {
                command.env("LD_PRELOAD", preload);
            } else {
                command.env_remove("LD_PRELOAD");
            }
        }
        Ok(())
    }
}

pub(crate) fn proton_log_name(config: &EffectiveCompatibilityConfig) -> String {
    let game_id = config
        .environment
        .get("GAMEID")
        .map(String::as_str)
        .filter(|id| !id.is_empty())
        .unwrap_or("umu-default");
    let app_id = game_id
        .strip_prefix("umu-")
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 80
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        .unwrap_or("0");
    format!("steam-{app_id}.log")
}

fn validate(config: &EffectiveCompatibilityConfig, runner: &InstalledRunner) -> Result<(), String> {
    if config.environment.get("GAMEID").is_some_and(|id| {
        id.len() > 84
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    }) {
        return Err(
            "GAMEID must be a short umu identifier without spaces or path separators".to_owned(),
        );
    }
    for key in [
        "PROTON_USE_WINED3D",
        "PROTON_ENABLE_WAYLAND",
        STEAM_OVERLAY_LAYER,
        STEAM_OVERLAY_GAME_ID,
    ] {
        if config
            .environment
            .keys()
            .any(|configured| configured.eq_ignore_ascii_case(key))
        {
            return Err(format!(
                "Compatibility environment variable {key} is managed by the typed launch options"
            ));
        }
    }

    if config.debug_logging {
        for key in [PROTON_LOG, PROTON_LOG_DIR, STEAM_GAME_ID] {
            if config
                .environment
                .keys()
                .any(|configured| configured.eq_ignore_ascii_case(key))
            {
                return Err(format!(
                    "Compatibility environment variable {key} is managed by debug logging"
                ));
            }
        }
    }

    let uses_proton = matches!(runner.kind, RunnerKind::Proton | RunnerKind::GeProton);
    if config.graphics_renderer == GraphicsRenderer::WineD3d && !uses_proton {
        return Err("WineD3D selection requires a Proton runner".to_owned());
    }
    if config.wayland != WaylandMode::RunnerDefault && runner.kind != RunnerKind::GeProton {
        return Err("Native Wayland selection requires a GE-Proton runner".to_owned());
    }
    Ok(())
}

fn load_overlay_libraries(steam_root: &Path) -> Result<[PathBuf; 2], String> {
    let paths = [
        steam_root.join("ubuntu12_32/gameoverlayrenderer.so"),
        steam_root.join("ubuntu12_64/gameoverlayrenderer.so"),
    ];
    paths
        .map(|path| {
            let resolved = std::fs::canonicalize(&path).map_err(|error| {
                format!(
                    "Steam overlay was enabled but its renderer library is unavailable at {}: {error}",
                    path.display()
                )
            })?;
            if !resolved.is_file() {
                return Err(format!(
                    "Steam overlay renderer is not a file: {}",
                    resolved.display()
                ));
            }
            Ok(resolved)
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| "Steam overlay renderer library list is incomplete".to_owned())
}

fn overlay_preload(
    configured_environment: &std::collections::BTreeMap<String, String>,
    libraries: &[PathBuf; 2],
) -> Result<OsString, String> {
    let inherited = match configured_environment.get("LD_PRELOAD") {
        Some(value) => (!value.is_empty()).then(|| OsString::from(value)),
        None => std::env::var_os("LD_PRELOAD"),
    };
    let existing = inherited
        .as_deref()
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .filter(|path| !is_overlay_library(path));
    std::env::join_paths(libraries.iter().cloned().chain(existing))
        .map_err(|error| format!("Could not build Steam overlay preload list: {error}"))
}

fn preload_without_overlay(
    configured_environment: &std::collections::BTreeMap<String, String>,
) -> Result<Option<OsString>, String> {
    let inherited = match configured_environment.get("LD_PRELOAD") {
        Some(value) => (!value.is_empty()).then(|| OsString::from(value)),
        None => std::env::var_os("LD_PRELOAD"),
    };
    let Some(inherited) = inherited else {
        return Ok(None);
    };
    let remaining = std::env::split_paths(&inherited)
        .filter(|path| !is_overlay_library(path))
        .collect::<Vec<_>>();
    if remaining.is_empty() {
        return Ok(None);
    }
    std::env::join_paths(remaining)
        .map(Some)
        .map_err(|error| format!("Could not update LD_PRELOAD for Steam overlay: {error}"))
}

fn is_overlay_library(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == "gameoverlayrenderer.so")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("legio-{name}-{}", uuid::Uuid::new_v4()))
    }

    fn runner(kind: RunnerKind) -> InstalledRunner {
        InstalledRunner {
            kind,
            name: "GE-Proton10-33".to_owned(),
            version: "GE-Proton10-33".to_owned(),
            path: "/unused/GE-Proton10-33".to_owned(),
        }
    }

    fn overlay_root() -> PathBuf {
        let root = test_dir("overlay");
        for architecture in ["ubuntu12_32", "ubuntu12_64"] {
            let directory = root.join(architecture);
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("gameoverlayrenderer.so"), b"test library").unwrap();
        }
        root
    }

    #[test]
    fn umu_log_identity_follows_game_id_without_assigning_a_fake_steam_id() {
        let mut config = EffectiveCompatibilityConfig::default();
        assert_eq!(proton_log_name(&config), "steam-default.log");
        config
            .environment
            .insert("GAMEID".to_owned(), "umu-1234".to_owned());
        assert_eq!(proton_log_name(&config), "steam-1234.log");
        config
            .environment
            .insert("GAMEID".to_owned(), "../outside".to_owned());
        assert!(
            PreparedOptions::prepare(&config, &runner(RunnerKind::GeProton), None, false).is_err()
        );
    }

    #[test]
    fn typed_options_reach_the_child_process_environment() {
        let config = EffectiveCompatibilityConfig {
            graphics_renderer: GraphicsRenderer::WineD3d,
            wayland: WaylandMode::Native,
            ..EffectiveCompatibilityConfig::default()
        };
        let prepared =
            PreparedOptions::prepare(&config, &runner(RunnerKind::GeProton), None, false).unwrap();
        let mut command = Command::new("/usr/bin/env");
        prepared.apply(&mut command, &config).unwrap();
        let output = command.output().unwrap();
        assert!(output.status.success());
        let environment = String::from_utf8(output.stdout).unwrap();
        assert!(
            environment
                .lines()
                .any(|line| line == "PROTON_USE_WINED3D=1")
        );
        assert!(
            environment
                .lines()
                .any(|line| line == "PROTON_ENABLE_WAYLAND=1")
        );
    }

    #[test]
    fn disabled_debug_logging_clears_inherited_proton_flags_and_preserves_explicit_values() {
        let runner = runner(RunnerKind::Proton);
        let mut config = EffectiveCompatibilityConfig::default();
        let prepared = PreparedOptions::prepare(&config, &runner, None, false).unwrap();
        let mut command = Command::new("/usr/bin/env");
        command
            .env(PROTON_LOG, "1")
            .env(PROTON_LOG_DIR, "/inherited-log-dir");
        prepared.apply(&mut command, &config).unwrap();
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == PROTON_LOG && value.is_none())
        );
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == PROTON_LOG_DIR && value.is_none())
        );
        config
            .environment
            .insert(PROTON_LOG.to_owned(), "1".to_owned());
        config
            .environment
            .insert(PROTON_LOG_DIR.to_owned(), "/explicit-log-dir".to_owned());
        prepared.apply(&mut command, &config).unwrap();
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == PROTON_LOG && value == Some("1".as_ref()))
        );
        assert!(command.get_envs().any(
            |(key, value)| key == PROTON_LOG_DIR && value == Some("/explicit-log-dir".as_ref())
        ));
    }

    #[test]
    fn proton_debug_logging_writes_logs_to_the_game_directory() {
        let log_directory = test_dir("proton-debug");
        let config = EffectiveCompatibilityConfig {
            debug_logging: true,
            ..EffectiveCompatibilityConfig::default()
        };
        let runner = runner(RunnerKind::Proton);
        let prepared = PreparedOptions::prepare(&config, &runner, None, false).unwrap();
        let mut command = Command::new("/usr/bin/env");
        prepared.apply(&mut command, &config).unwrap();
        PreparedOptions::apply_debug_logging(&mut command, &config, &runner, &log_directory);
        let output = command.output().unwrap();
        let environment = String::from_utf8(output.stdout).unwrap();
        assert!(environment.lines().any(|line| line == "PROTON_LOG=1"));
        assert!(
            environment
                .lines()
                .any(|line| { line == format!("PROTON_LOG_DIR={}", log_directory.display()) })
        );
        assert!(
            !environment
                .lines()
                .any(|line| line.starts_with("SteamGameId="))
        );
    }

    #[test]
    fn wine_debug_logging_sets_default_and_preserves_user_channels() {
        let runner = runner(RunnerKind::Wine);
        let mut config = EffectiveCompatibilityConfig {
            debug_logging: true,
            ..EffectiveCompatibilityConfig::default()
        };
        let prepared = PreparedOptions::prepare(&config, &runner, None, false).unwrap();
        let mut command = Command::new("/usr/bin/env");
        prepared.apply(&mut command, &config).unwrap();
        PreparedOptions::apply_debug_logging(&mut command, &config, &runner, Path::new("/logs"));
        assert!(
            command
                .get_envs()
                .any(|(key, value)| { key == "WINEDEBUG" && value == Some(WINE_DEBUG.as_ref()) })
        );

        config
            .environment
            .insert("WINEDEBUG".to_owned(), "+seh".to_owned());
        let prepared = PreparedOptions::prepare(&config, &runner, None, false).unwrap();
        let mut command = Command::new("/usr/bin/env");
        prepared.apply(&mut command, &config).unwrap();
        PreparedOptions::apply_debug_logging(&mut command, &config, &runner, Path::new("/logs"));
        assert!(
            command
                .get_envs()
                .any(|(key, value)| { key == "WINEDEBUG" && value == Some("+seh".as_ref()) })
        );
    }

    #[test]
    fn debug_logging_rejects_runner_managed_log_environment() {
        let mut environment = BTreeMap::new();
        environment.insert("PROTON_LOG_DIR".to_owned(), "/tmp/custom".to_owned());
        let config = EffectiveCompatibilityConfig {
            debug_logging: true,
            environment,
            ..EffectiveCompatibilityConfig::default()
        };
        let error = PreparedOptions::prepare(&config, &runner(RunnerKind::Proton), None, false)
            .unwrap_err();
        assert!(error.contains("PROTON_LOG_DIR is managed by debug logging"));
    }

    #[test]
    fn launch_via_steam_uses_verified_overlay_libraries() {
        let steam_root = overlay_root();
        let config = EffectiveCompatibilityConfig::default();
        let prepared = PreparedOptions::prepare(
            &config,
            &runner(RunnerKind::Proton),
            Some(&steam_root),
            true,
        )
        .unwrap();
        let mut command = Command::new("/usr/bin/env");
        prepared.apply(&mut command, &config).unwrap();
        let environment = command
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.to_string_lossy().into_owned(),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert!(
            environment
                .get(STEAM_OVERLAY_LAYER)
                .is_some_and(|value| value == "1")
        );
        assert!(
            environment
                .get(STEAM_OVERLAY_GAME_ID)
                .is_some_and(|value| value == "480")
        );
        let preload = environment.get("LD_PRELOAD").unwrap();
        assert!(preload.contains("ubuntu12_32/gameoverlayrenderer.so"));
        assert!(preload.contains("ubuntu12_64/gameoverlayrenderer.so"));
        fs::remove_dir_all(steam_root).unwrap();
    }

    #[test]
    fn launch_without_steam_clears_overlay_environment_and_preload() {
        let mut environment = BTreeMap::new();
        environment.insert(
            "LD_PRELOAD".to_owned(),
            "/usr/lib/gameoverlayrenderer.so:/opt/custom.so".to_owned(),
        );
        let config = EffectiveCompatibilityConfig {
            environment,
            ..EffectiveCompatibilityConfig::default()
        };
        let prepared =
            PreparedOptions::prepare(&config, &runner(RunnerKind::Proton), None, false).unwrap();
        let mut command = Command::new("/usr/bin/env");
        prepared.apply(&mut command, &config).unwrap();
        let environment = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            environment.get(STEAM_OVERLAY_LAYER),
            Some(&Some("0".to_owned()))
        );
        assert_eq!(environment.get(STEAM_OVERLAY_GAME_ID), Some(&None));
        assert_eq!(
            environment.get("LD_PRELOAD"),
            Some(&Some("/opt/custom.so".to_owned()))
        );
    }

    #[test]
    fn typed_options_reject_environment_variable_bypasses() {
        let mut environment = BTreeMap::new();
        environment.insert("PROTON_USE_WINED3D".to_owned(), "0".to_owned());
        let config = EffectiveCompatibilityConfig {
            environment,
            ..EffectiveCompatibilityConfig::default()
        };
        let error = PreparedOptions::prepare(&config, &runner(RunnerKind::GeProton), None, false)
            .unwrap_err();
        assert!(error.contains("managed by the typed launch options"));
    }

    #[test]
    fn native_wayland_is_rejected_for_runner_without_the_documented_option() {
        let config = EffectiveCompatibilityConfig {
            wayland: WaylandMode::Native,
            ..EffectiveCompatibilityConfig::default()
        };
        let error = PreparedOptions::prepare(&config, &runner(RunnerKind::Proton), None, false)
            .unwrap_err();
        assert!(error.contains("requires a GE-Proton runner"));
    }

    #[test]
    fn wine_d3d_is_rejected_for_wine() {
        let config = EffectiveCompatibilityConfig {
            graphics_renderer: GraphicsRenderer::WineD3d,
            ..EffectiveCompatibilityConfig::default()
        };
        let error =
            PreparedOptions::prepare(&config, &runner(RunnerKind::Wine), None, false).unwrap_err();
        assert!(error.contains("requires a Proton runner"));
    }

    #[test]
    fn launch_via_steam_preserves_custom_preload() {
        let root = overlay_root();
        let mut environment = BTreeMap::new();
        environment.insert("LD_PRELOAD".to_owned(), "/opt/custom.so".to_owned());
        let config = EffectiveCompatibilityConfig {
            environment,
            ..EffectiveCompatibilityConfig::default()
        };
        let prepared =
            PreparedOptions::prepare(&config, &runner(RunnerKind::GeProton), Some(&root), true)
                .unwrap();
        let mut command = Command::new("/usr/bin/env");
        prepared.apply(&mut command, &config).unwrap();
        let preload = command
            .get_envs()
            .find(|(key, _)| *key == "LD_PRELOAD")
            .and_then(|(_, value)| value)
            .unwrap()
            .to_string_lossy();
        assert!(preload.contains("/opt/custom.so"));
        assert!(preload.contains("gameoverlayrenderer.so"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn launch_via_steam_continues_without_missing_overlay_libraries() {
        let root = test_dir("missing-overlay");
        fs::create_dir_all(&root).unwrap();
        let config = EffectiveCompatibilityConfig::default();
        let prepared =
            PreparedOptions::prepare(&config, &runner(RunnerKind::GeProton), Some(&root), true)
                .unwrap();
        assert!(prepared.overlay_libraries.is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
