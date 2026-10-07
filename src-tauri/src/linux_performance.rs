use std::path::Path;
use std::process::Command;

use crate::database::LinuxPerformance;

pub(crate) fn wrap(command: Command, options: &LinuxPerformance) -> Result<Command, String> {
    options.validate()?;
    let mut command = command;
    if options.game_mode {
        let executable = crate::runner_discovery::executable_path("gamemoderun")
            .ok_or_else(|| "Install GameMode or disable it for this game".to_owned())?;
        command = prepend(command, &executable, &[]);
    }
    if let Some(config) = &options.gamescope {
        let executable = crate::runner_discovery::executable_path("gamescope")
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

    #[test]
    fn default_options_preserve_direct_launch_and_numeric_limits_are_enforced() {
        let command = wrap(Command::new("/usr/bin/wine"), &LinuxPerformance::default()).unwrap();
        assert_eq!(command.get_program(), "/usr/bin/wine");
        let mut options = LinuxPerformance {
            game_mode: false,
            gamescope: Some(GamescopeConfig {
                width: Some(1280),
                height: Some(720),
                fps: Some(60),
            }),
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
