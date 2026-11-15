use std::path::{Path, PathBuf};
use std::process::Command;

use crate::database::LinuxPerformance;

pub(crate) fn wrap(command: Command, options: &LinuxPerformance) -> Result<Command, String> {
    wrap_with(command, options, crate::runner_discovery::executable_path)
}

// Layers from the game outward: wrapper, MangoHud, GameMode, gamescope.
fn wrap_with(
    command: Command,
    options: &LinuxPerformance,
    find: impl Fn(&str) -> Option<PathBuf>,
) -> Result<Command, String> {
    options.validate()?;
    let mut command = command;
    if let Some((program, arguments)) = options.wrapper.split_first() {
        let executable = if program.contains('/') {
            let path = PathBuf::from(program);
            if !path.is_absolute() {
                return Err(
                    "Custom wrapper command must be a program name or an absolute path".to_owned(),
                );
            }
            path
        } else {
            find(program).ok_or_else(|| {
                format!("Custom wrapper program {program} was not found; fix or clear it")
            })?
        };
        command = prepend(command, &executable, arguments);
    }
    // Gamescope draws MangoHud itself through --mangoapp; wrapping the game
    // with mangohud as well would show the overlay twice.
    if options.mangohud && options.gamescope.is_none() {
        let executable = find("mangohud")
            .ok_or_else(|| "Install MangoHud or disable it for this game".to_owned())?;
        command = prepend(command, &executable, &[]);
    }
    if options.game_mode {
        let executable = find("gamemoderun")
            .ok_or_else(|| "Install GameMode or disable it for this game".to_owned())?;
        command = prepend(command, &executable, &[]);
    }
    if let Some(config) = &options.gamescope {
        let executable = find("gamescope")
            .ok_or_else(|| "Install gamescope or disable it for this game".to_owned())?;
        let mut arguments = Vec::new();
        for (flag, value) in [
            ("-w", config.width),
            ("-h", config.height),
            ("-r", config.fps),
        ] {
            if let Some(value) = value {
                arguments.extend([flag.to_owned(), value.to_string()]);
            }
        }
        if options.mangohud {
            if find("mangohud").is_none() {
                return Err("Install MangoHud or disable it for this game".to_owned());
            }
            arguments.push("--mangoapp".to_owned());
        }
        arguments.push("--".to_owned());
        command = prepend(command, &executable, &arguments);
    }
    Ok(command)
}

// Launch stdio is configured after wrapping.
fn prepend(command: Command, executable: &Path, arguments: &[String]) -> Command {
    let mut wrapped = Command::new(executable);
    wrapped
        .args(arguments)
        .arg(command.get_program())
        .args(command.get_args());
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            wrapped.env(key, value);
        } else {
            wrapped.env_remove(key);
        }
    }
    if let Some(directory) = command.get_current_dir() {
        wrapped.current_dir(directory);
    }
    wrapped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::GamescopeConfig;

    #[test]
    fn wrapper_chain_preserves_arguments_and_environment_without_a_shell() {
        let mut game = Command::new("/runner with spaces/umu-run");
        game.env("PROTONPATH", "/proton with spaces")
            .env_remove("REMOVE_ME")
            .args(["/game with spaces/game.exe", "$(touch unsafe)", ";exit"])
            .current_dir("/game with spaces");
        let game_mode = prepend(game, Path::new("/usr/bin/gamemoderun"), &[]);
        let wrapped = prepend(
            game_mode,
            Path::new("/usr/bin/gamescope"),
            &["-r".to_owned(), "60".to_owned(), "--".to_owned()],
        );
        assert_eq!(wrapped.get_program(), "/usr/bin/gamescope");
        let arguments: Vec<_> = wrapped
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        assert_eq!(
            arguments,
            [
                "-r",
                "60",
                "--",
                "/usr/bin/gamemoderun",
                "/runner with spaces/umu-run",
                "/game with spaces/game.exe",
                "$(touch unsafe)",
                ";exit"
            ]
        );
        assert_eq!(
            wrapped.get_current_dir(),
            Some(Path::new("/game with spaces"))
        );
        assert!(
            wrapped
                .get_envs()
                .any(|(key, value)| key == "REMOVE_ME" && value.is_none())
        );
        assert!(wrapped.get_envs().any(|(key, value)| key == "PROTONPATH"
            && value == Some(std::ffi::OsStr::new("/proton with spaces"))));
    }

    fn fake_find(name: &str) -> Option<PathBuf> {
        Some(PathBuf::from("/bin").join(name))
    }

    fn composed(options: &LinuxPerformance) -> Vec<String> {
        let mut game = Command::new("/usr/bin/umu-run");
        game.arg("game.exe");
        let command = wrap_with(game, options, fake_find).unwrap();
        std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(|value| value.to_str().unwrap().to_owned())
            .collect()
    }

    fn gamescope() -> Option<GamescopeConfig> {
        Some(GamescopeConfig {
            fps: Some(60),
            ..GamescopeConfig::default()
        })
    }

    #[test]
    fn mangohud_wraps_the_game_alone_and_under_gamemode() {
        let mut options = LinuxPerformance {
            mangohud: true,
            ..LinuxPerformance::default()
        };
        assert_eq!(
            composed(&options),
            ["/bin/mangohud", "/usr/bin/umu-run", "game.exe"]
        );
        options.game_mode = true;
        assert_eq!(
            composed(&options),
            [
                "/bin/gamemoderun",
                "/bin/mangohud",
                "/usr/bin/umu-run",
                "game.exe"
            ]
        );
    }

    #[test]
    fn mangohud_uses_mangoapp_with_gamescope() {
        let options = LinuxPerformance {
            mangohud: true,
            gamescope: gamescope(),
            ..LinuxPerformance::default()
        };
        assert_eq!(
            composed(&options),
            [
                "/bin/gamescope",
                "-r",
                "60",
                "--mangoapp",
                "--",
                "/usr/bin/umu-run",
                "game.exe"
            ]
        );
    }

    #[test]
    fn wrapper_sits_directly_before_the_game() {
        let options = LinuxPerformance {
            wrapper: vec!["taskset".to_owned(), "-c".to_owned(), "0-7".to_owned()],
            ..LinuxPerformance::default()
        };
        assert_eq!(
            composed(&options),
            ["/bin/taskset", "-c", "0-7", "/usr/bin/umu-run", "game.exe"]
        );
    }

    #[test]
    fn all_layers_compose_in_order() {
        let options = LinuxPerformance {
            game_mode: true,
            mangohud: true,
            gamescope: gamescope(),
            wrapper: vec!["/opt/prime-run".to_owned()],
        };
        assert_eq!(
            composed(&options),
            [
                "/bin/gamescope",
                "-r",
                "60",
                "--mangoapp",
                "--",
                "/bin/gamemoderun",
                "/opt/prime-run",
                "/usr/bin/umu-run",
                "game.exe"
            ]
        );
    }

    #[test]
    fn missing_binaries_are_reported() {
        let options = LinuxPerformance {
            mangohud: true,
            ..LinuxPerformance::default()
        };
        assert!(wrap_with(Command::new("game"), &options, |_| None).is_err());
        let options = LinuxPerformance {
            wrapper: vec!["prime-run".to_owned()],
            ..LinuxPerformance::default()
        };
        assert!(wrap_with(Command::new("game"), &options, |_| None).is_err());
    }

    #[test]
    fn wrapper_validation_rejects_unsafe_values() {
        let with = |wrapper: &[&str]| LinuxPerformance {
            wrapper: wrapper.iter().map(|value| (*value).to_owned()).collect(),
            ..LinuxPerformance::default()
        };
        assert!(with(&["prime-run"]).validate().is_ok());
        assert!(with(&["taskset", "-c", "0-7"]).validate().is_ok());
        assert!(with(&[""]).validate().is_err());
        assert!(with(&["  ", "x"]).validate().is_err());
        assert!(with(&["prime-run\n"]).validate().is_err());
        assert!(with(&["env", "A=\u{7}"]).validate().is_err());
        assert!(with(&["a\0b"]).validate().is_err());
        let relative = with(&["./run"]);
        assert!(wrap_with(Command::new("game"), &relative, fake_find).is_err());
    }

    #[test]
    fn default_options_preserve_direct_launch_and_numeric_limits_are_enforced() {
        let command = wrap(Command::new("/usr/bin/wine"), &LinuxPerformance::default()).unwrap();
        assert_eq!(command.get_program(), "/usr/bin/wine");
        let mut options = LinuxPerformance {
            gamescope: Some(GamescopeConfig {
                width: Some(1280),
                height: Some(720),
                fps: Some(60),
            }),
            ..LinuxPerformance::default()
        };
        assert!(options.validate().is_ok());
        options.gamescope.as_mut().unwrap().height = None;
        assert!(options.validate().is_err());
        options.gamescope.as_mut().unwrap().height = Some(720);
        options.gamescope.as_mut().unwrap().fps = Some(0);
        assert!(options.validate().is_err());
        options.gamescope.as_mut().unwrap().fps = Some(361);
        assert!(options.validate().is_err());
    }
}
