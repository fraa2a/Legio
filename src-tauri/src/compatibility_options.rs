use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::database::{
    AppliedCompatibilityOptions, EffectiveCompatibilityConfig, GraphicsRenderer, SteamOverlayMode,
    SteamRuntimeMode, WaylandMode,
};
use crate::runner_discovery::{self, InstalledRunner, RunnerKind};

const STEAM_OVERLAY_LAYER: &str = "ENABLE_VK_LAYER_VALVE_steam_overlay_1";
const STEAM_OVERLAY_GAME_ID: &str = "SteamOverlayGameId";

#[derive(Debug)]
pub(crate) struct PreparedOptions {
    runtime_path: Option<PathBuf>,
    overlay_libraries: Option<[PathBuf; 2]>,
}

impl PreparedOptions {
    pub(crate) fn prepare(
        config: &EffectiveCompatibilityConfig,
        runner: &InstalledRunner,
        steam_root: Option<&Path>,
    ) -> Result<Self, String> {
        validate(config, runner)?;

        let runtime_path = match config.steam_runtime {
            SteamRuntimeMode::RunnerDefault => None,
            SteamRuntimeMode::SteamLinuxRuntime => {
                Some(runner_discovery::steam_runtime_path(runner)?)
            }
        };
        let overlay_libraries = match config.steam_overlay {
            SteamOverlayMode::Enabled => {
                Some(load_overlay_libraries(steam_root.ok_or_else(|| {
                    "Steam overlay was enabled but no Steam installation was found".to_owned()
                })?)?)
            }
            SteamOverlayMode::RunnerDefault | SteamOverlayMode::Disabled => None,
        };

        Ok(Self {
            runtime_path,
            overlay_libraries,
        })
    }

    pub(crate) fn runtime_path(&self) -> Option<&Path> {
        self.runtime_path.as_deref()
    }

    pub(crate) fn diagnostic(
        runner: &InstalledRunner,
        config: &EffectiveCompatibilityConfig,
    ) -> AppliedCompatibilityOptions {
        AppliedCompatibilityOptions {
            runner: runner.name.clone(),
            version: runner.version.clone(),
            steam_runtime: config.steam_runtime,
            steam_overlay: config.steam_overlay,
            graphics_renderer: config.graphics_renderer,
            wayland: config.wayland,
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

        match config.graphics_renderer {
            GraphicsRenderer::RunnerDefault => {}
            GraphicsRenderer::WineD3d => {
                command.env("PROTON_USE_WINED3D", "1");
            }
        }
        match config.wayland {
            WaylandMode::RunnerDefault => {}
            WaylandMode::Disabled => {
                command.env("PROTON_ENABLE_WAYLAND", "0");
            }
            WaylandMode::Native => {
                command.env("PROTON_ENABLE_WAYLAND", "1");
            }
        }
        self.apply_overlay(command, config)
    }

    fn apply_overlay(
        &self,
        command: &mut Command,
        config: &EffectiveCompatibilityConfig,
    ) -> Result<(), String> {
        match config.steam_overlay {
            SteamOverlayMode::RunnerDefault => {}
            SteamOverlayMode::Enabled => {
                let libraries = self
                    .overlay_libraries
                    .as_ref()
                    .ok_or_else(|| "Steam overlay libraries were not prepared".to_owned())?;
                let preload = overlay_preload(&config.environment, libraries)?;
                command
                    .env("LD_PRELOAD", preload)
                    .env(STEAM_OVERLAY_LAYER, "1")
                    .env(STEAM_OVERLAY_GAME_ID, "480");
            }
            SteamOverlayMode::Disabled => {
                command.env(STEAM_OVERLAY_LAYER, "0");
                command.env_remove(STEAM_OVERLAY_GAME_ID);
                if let Some(preload) = preload_without_overlay(&config.environment)? {
                    command.env("LD_PRELOAD", preload);
                } else {
                    command.env_remove("LD_PRELOAD");
                }
            }
        }
        Ok(())
    }
}

fn validate(config: &EffectiveCompatibilityConfig, runner: &InstalledRunner) -> Result<(), String> {
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

    let uses_proton = matches!(runner.kind, RunnerKind::Proton | RunnerKind::GeProton);
    if config.steam_runtime == SteamRuntimeMode::SteamLinuxRuntime && !uses_proton {
        return Err("Steam Linux Runtime requires a Proton runner".to_owned());
    }
    if config.graphics_renderer == GraphicsRenderer::WineD3d && !uses_proton {
        return Err("WineD3D selection requires a Proton runner".to_owned());
    }
    if config.steam_overlay == SteamOverlayMode::Enabled && !uses_proton {
        return Err("Steam overlay integration requires a Proton runner".to_owned());
    }
    if config.wayland != WaylandMode::RunnerDefault && runner.kind != RunnerKind::GeProton {
        return Err("Native Wayland selection requires a GE-Proton runner".to_owned());
    }
    if config.steam_overlay == SteamOverlayMode::Enabled
        && config
            .environment
            .get("LD_PRELOAD")
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err("Steam overlay cannot be combined with a custom LD_PRELOAD value".to_owned());
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
    fn typed_options_reach_the_child_process_environment() {
        let config = EffectiveCompatibilityConfig {
            graphics_renderer: GraphicsRenderer::WineD3d,
            wayland: WaylandMode::Native,
            ..EffectiveCompatibilityConfig::default()
        };
        let prepared =
            PreparedOptions::prepare(&config, &runner(RunnerKind::GeProton), None).unwrap();
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
    fn overlay_options_use_verified_libraries_and_update_preload() {
        let steam_root = overlay_root();
        let config = EffectiveCompatibilityConfig {
            steam_overlay: SteamOverlayMode::Enabled,
            ..EffectiveCompatibilityConfig::default()
        };
        let prepared =
            PreparedOptions::prepare(&config, &runner(RunnerKind::Proton), Some(&steam_root))
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
    fn disabling_overlay_clears_overlay_environment_and_preload() {
        let mut environment = BTreeMap::new();
        environment.insert(
            "LD_PRELOAD".to_owned(),
            "/usr/lib/gameoverlayrenderer.so:/opt/custom.so".to_owned(),
        );
        let config = EffectiveCompatibilityConfig {
            environment,
            steam_overlay: SteamOverlayMode::Disabled,
            ..EffectiveCompatibilityConfig::default()
        };
        let prepared =
            PreparedOptions::prepare(&config, &runner(RunnerKind::Proton), None).unwrap();
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
        let error =
            PreparedOptions::prepare(&config, &runner(RunnerKind::GeProton), None).unwrap_err();
        assert!(error.contains("managed by the typed launch options"));
    }

    #[test]
    fn native_wayland_is_rejected_for_runner_without_the_documented_option() {
        let config = EffectiveCompatibilityConfig {
            wayland: WaylandMode::Native,
            ..EffectiveCompatibilityConfig::default()
        };
        let error =
            PreparedOptions::prepare(&config, &runner(RunnerKind::Proton), None).unwrap_err();
        assert!(error.contains("requires a GE-Proton runner"));
    }

    #[test]
    fn proton_only_settings_are_rejected_for_wine() {
        for config in [
            EffectiveCompatibilityConfig {
                steam_runtime: SteamRuntimeMode::SteamLinuxRuntime,
                ..EffectiveCompatibilityConfig::default()
            },
            EffectiveCompatibilityConfig {
                steam_overlay: SteamOverlayMode::Enabled,
                ..EffectiveCompatibilityConfig::default()
            },
            EffectiveCompatibilityConfig {
                graphics_renderer: GraphicsRenderer::WineD3d,
                ..EffectiveCompatibilityConfig::default()
            },
        ] {
            let error =
                PreparedOptions::prepare(&config, &runner(RunnerKind::Wine), None).unwrap_err();
            assert!(error.contains("requires a Proton runner"));
        }
    }

    #[test]
    fn custom_preload_and_overlay_fail_with_a_clear_diagnostic() {
        let root = overlay_root();
        let mut environment = BTreeMap::new();
        environment.insert("LD_PRELOAD".to_owned(), "/opt/custom.so".to_owned());
        let config = EffectiveCompatibilityConfig {
            environment,
            steam_overlay: SteamOverlayMode::Enabled,
            ..EffectiveCompatibilityConfig::default()
        };
        let error = PreparedOptions::prepare(&config, &runner(RunnerKind::GeProton), Some(&root))
            .unwrap_err();
        assert!(error.contains("custom LD_PRELOAD"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_overlay_library_fails_before_process_start() {
        let root = test_dir("missing-overlay");
        fs::create_dir_all(&root).unwrap();
        let config = EffectiveCompatibilityConfig {
            steam_overlay: SteamOverlayMode::Enabled,
            ..EffectiveCompatibilityConfig::default()
        };
        let error = PreparedOptions::prepare(&config, &runner(RunnerKind::GeProton), Some(&root))
            .unwrap_err();
        assert!(error.contains("renderer library is unavailable"));
        fs::remove_dir_all(root).unwrap();
    }
}
