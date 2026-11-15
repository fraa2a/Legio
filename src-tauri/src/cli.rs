use crate::database::Database;
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

pub(crate) const USAGE: &str = "Usage: legio launch <game-id> | legio stop <game-id> | legio list";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Launch(String),
    Stop(String),
    List,
}

fn parse_game_id(arguments: &mut impl Iterator<Item = OsString>) -> Result<String, String> {
    let id = arguments
        .next()
        .ok_or_else(|| format!("Missing game ID. {USAGE}"))?;
    if arguments.next().is_some() {
        return Err(format!("Too many arguments. {USAGE}"));
    }
    let id = id
        .into_string()
        .map_err(|_| "The game ID is not valid UTF-8".to_owned())?;
    uuid::Uuid::parse_str(&id)
        .map(|id| id.to_string())
        .map_err(|_| "The game ID is not valid".to_owned())
}

/// Reads a subcommand from the arguments after the program name. Other arguments,
/// such as links and flags, belong to other handlers and yield `None`.
fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Option<Command>, String> {
    let mut arguments = arguments.into_iter();
    let Some(first) = arguments.next() else {
        return Ok(None);
    };
    match first.to_str() {
        Some("launch") => parse_game_id(&mut arguments).map(Command::Launch).map(Some),
        Some("stop") => parse_game_id(&mut arguments).map(Command::Stop).map(Some),
        Some("list") if arguments.next().is_some() => Err(format!("Too many arguments. {USAGE}")),
        Some("list") => Ok(Some(Command::List)),
        _ => Ok(None),
    }
}

/// Subcommands and the desktop shortcut argument, which launches like `launch`.
pub(crate) fn requested_command(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<Option<Command>, String> {
    let arguments: Vec<OsString> = arguments.into_iter().collect();
    if let Some(command) = parse(arguments.iter().cloned())? {
        return Ok(Some(command));
    }
    Ok(crate::desktop_shortcuts::requested_game_id(arguments)?.map(Command::Launch))
}

fn data_dir(identifier: &str) -> Result<PathBuf, String> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
        });
    base.map(|base| base.join(identifier))
        .ok_or_else(|| "Could not locate the application data directory".to_owned())
}

fn single_line(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn write_list(database: &Database, output: &mut impl Write) -> Result<(), String> {
    for game in database.visible_games()? {
        writeln!(output, "{}\t{}", game.id, single_line(&game.name))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Release builds on Windows have no console, so output only appears when the
/// parent process is a terminal that the launcher can attach to.
#[cfg(windows)]
fn attach_parent_console() {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
        SetStdHandle,
    };

    // SAFETY: these calls take no pointers; the handles passed to SetStdHandle come from
    // files that are kept open for the rest of the process.
    unsafe {
        let redirected = |kind| {
            let handle = GetStdHandle(kind);
            !handle.is_null() && handle != windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE
        };
        if redirected(STD_OUTPUT_HANDLE) || AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        if let Ok(console) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
            let handle = console.as_raw_handle();
            SetStdHandle(STD_OUTPUT_HANDLE, handle);
            if !redirected(STD_ERROR_HANDLE) {
                SetStdHandle(STD_ERROR_HANDLE, handle);
            }
            std::mem::forget(console);
        }
    }
}

/// Prints the library to stdout without starting the application and returns the exit code.
pub(crate) fn run_list(identifier: &str) -> i32 {
    #[cfg(windows)]
    attach_parent_console();
    let result = data_dir(identifier)
        .and_then(|dir| Database::open_read_only(&dir))
        .and_then(|database| write_list(&database, &mut std::io::stdout().lock()));
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("legio: {error}");
            1
        }
    }
}

pub(crate) fn report_error(error: &str) -> i32 {
    #[cfg(windows)]
    attach_parent_console();
    eprintln!("legio: {error}");
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0b9c3c3e-6a5e-4c35-9d0e-7f6a2b1c4d5e";

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_each_subcommand() {
        assert_eq!(
            parse(args(&["launch", ID])),
            Ok(Some(Command::Launch(ID.to_owned())))
        );
        assert_eq!(
            parse(args(&["stop", ID])),
            Ok(Some(Command::Stop(ID.to_owned())))
        );
        assert_eq!(parse(args(&["list"])), Ok(Some(Command::List)));
    }

    #[test]
    fn game_id_is_normalized() {
        assert_eq!(
            parse(args(&["launch", &ID.to_uppercase()])),
            Ok(Some(Command::Launch(ID.to_owned())))
        );
    }

    #[test]
    fn other_arguments_are_left_alone() {
        assert_eq!(parse(args(&[])), Ok(None));
        assert_eq!(parse(args(&["--minimized"])), Ok(None));
        assert_eq!(parse(args(&["legio://source?url=x"])), Ok(None));
        assert_eq!(parse(args(&["--minimized", "launch", ID])), Ok(None));
    }

    #[test]
    fn rejects_bad_ids_and_arity() {
        assert!(parse(args(&["launch"])).is_err());
        assert!(parse(args(&["launch", "not-a-uuid"])).is_err());
        assert!(parse(args(&["launch", "../../etc"])).is_err());
        assert!(parse(args(&["stop", ID, "extra"])).is_err());
        assert!(parse(args(&["list", "extra"])).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_utf8_ids() {
        use std::os::unix::ffi::OsStringExt;
        let invalid = OsString::from_vec(vec![0xff, 0xfe]);
        assert!(parse([OsString::from("stop"), invalid]).is_err());
    }

    #[test]
    fn shortcut_argument_still_launches() {
        assert_eq!(
            requested_command(args(&[&format!("--launch-game={ID}")])),
            Ok(Some(Command::Launch(ID.to_owned())))
        );
        assert!(requested_command(args(&["--launch-game=bad"])).is_err());
        assert_eq!(
            requested_command(args(&["stop", ID])),
            Ok(Some(Command::Stop(ID.to_owned())))
        );
        assert_eq!(requested_command(args(&["--minimized"])), Ok(None));
    }

    #[test]
    fn list_output_is_one_line_per_game() {
        assert_eq!(single_line("A\nB\tC"), "A B C");
    }

    #[test]
    fn list_reads_the_library_read_only() {
        let dir = std::env::temp_dir().join(format!("legio-cli-test-{}", uuid::Uuid::new_v4()));
        let game = {
            let database = Database::open(&dir).unwrap();
            database
                .create_game(crate::database::CreateGameInput {
                    name: "My\nGame".to_owned(),
                    steam_app_id: None,
                })
                .unwrap()
        };
        let database = Database::open_read_only(&dir).unwrap();
        let mut output = Vec::new();
        write_list(&database, &mut output).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            format!("{}\tMy Game\n", game.id)
        );
        assert!(
            database
                .create_game(crate::database::CreateGameInput {
                    name: "Other".to_owned(),
                    steam_app_id: None,
                })
                .is_err()
        );
        drop(database);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
