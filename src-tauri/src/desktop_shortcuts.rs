use crate::database::Game;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShortcutLocation {
    Desktop,
    ApplicationsMenu,
}

pub(crate) fn create(
    game: &Game,
    location: ShortcutLocation,
    icon_path: Option<&std::path::Path>,
) -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    {
        linux::create(game, location, icon_path)
    }
    #[cfg(windows)]
    {
        windows::create(game, location, icon_path)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (game, location, icon_path);
        Err("Game shortcuts are supported on Linux only".to_owned())
    }
}

pub(crate) fn remove(game_id: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        linux::remove(game_id)
    }
    #[cfg(windows)]
    {
        windows::remove(game_id)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = game_id;
        Ok(())
    }
}

pub(crate) fn steam_shortcut_arguments(game: &Game) -> Result<Option<Vec<String>>, String> {
    #[cfg(target_os = "linux")]
    {
        linux::steam_shortcut_arguments(game)
    }
    #[cfg(windows)]
    {
        windows::steam_shortcut_arguments(game)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = game;
        Ok(None)
    }
}

pub(crate) fn remove_steam_shortcuts(game: &Game) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        linux::remove_steam_shortcuts(game)
    }
    #[cfg(windows)]
    {
        windows::remove_steam_shortcuts(game)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = game;
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use super::{Game, PathBuf, ShortcutLocation};
    use serde::Deserialize;
    use std::{path::Path, process::Command};

    const STEAM_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$id = $env:LEGIO_STEAM_APP_ID
$shell = New-Object -ComObject WScript.Shell
$desktop = [Environment]::GetFolderPath('DesktopDirectory')
$programs = [Environment]::GetFolderPath('Programs')
$folders = @($desktop, $programs, (Join-Path $programs 'Steam'))
$found = @()
foreach ($folder in $folders) {
  if (-not [IO.Directory]::Exists($folder)) { continue }
  foreach ($path in [IO.Directory]::EnumerateFiles($folder)) {
    $extension = [IO.Path]::GetExtension($path).ToLowerInvariant()
    $arguments = $null
    try {
      if ($extension -eq '.url') {
        foreach ($line in (Get-Content -LiteralPath $path)) {
          if ($line -match ('^URL=steam://rungameid/' + $id + '(?://(.*))?$')) {
            $arguments = if ($Matches[1]) { [Uri]::UnescapeDataString($Matches[1]) } else { '' }
            break
          }
        }
      } elseif ($extension -eq '.lnk') {
        $link = $shell.CreateShortcut($path)
        if ([IO.Path]::GetFileName($link.TargetPath) -ieq 'steam.exe' -and $link.Arguments -match ('(?i)(?:^|\s)-applaunch\s+' + $id + '(?:\s+(.*))?$')) {
          $arguments = $Matches[1]
        }
      }
    } catch { continue }
    if ($null -ne $arguments) {
      if ($env:LEGIO_STEAM_ACTION -eq 'remove') { [IO.File]::Delete($path) }
      else { $found += @{ path = $path; arguments = $arguments } }
    }
  }
}
if ($env:LEGIO_STEAM_ACTION -ne 'remove') { [Console]::Out.Write((ConvertTo-Json -InputObject @($found) -Compress)) }
"#;

    #[derive(Deserialize)]
    struct SteamShortcut {
        arguments: String,
    }

    fn split_arguments(value: &str) -> Result<Vec<String>, String> {
        let mut arguments = Vec::new();
        let mut current = String::new();
        let mut quoted = false;
        let mut started = false;
        let mut chars = value.chars().peekable();
        while let Some(character) = chars.next() {
            if character == '\\' {
                let mut count = 1;
                while chars.peek() == Some(&'\\') {
                    chars.next();
                    count += 1;
                }
                if chars.peek() == Some(&'"') {
                    for _ in 0..count / 2 {
                        current.push('\\');
                    }
                    chars.next();
                    if count % 2 == 0 {
                        quoted = !quoted;
                    } else {
                        current.push('"');
                    }
                } else {
                    for _ in 0..count {
                        current.push('\\');
                    }
                }
                started = true;
            } else if character == '"' {
                quoted = !quoted;
                started = true;
            } else if character.is_whitespace() && !quoted {
                if started {
                    arguments.push(std::mem::take(&mut current));
                    started = false;
                }
            } else {
                current.push(character);
                started = true;
            }
        }
        if quoted {
            return Err("Steam shortcut has malformed launch arguments".to_owned());
        }
        if started {
            arguments.push(current);
        }
        Ok(arguments)
    }

    const CREATE_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$folder = if ($env:LEGIO_LINK_LOCATION -eq 'desktop') { [Environment]::GetFolderPath('DesktopDirectory') } else { [Environment]::GetFolderPath('Programs') }
if (-not $folder) { throw 'Shortcut folder is unavailable' }
if ($env:LEGIO_LINK_LOCATION -ne 'desktop') { $folder = Join-Path $folder 'Legio' }
[IO.Directory]::CreateDirectory($folder) | Out-Null
$path = Join-Path $folder ($env:LEGIO_LINK_NAME + '.lnk')
$shell = New-Object -ComObject WScript.Shell
if ([IO.File]::Exists($path)) {
  $old = $shell.CreateShortcut($path)
  if ($old.Description -ne ('Legio game ' + $env:LEGIO_GAME_ID)) {
    $path = Join-Path $folder ($env:LEGIO_LINK_NAME + ' (' + $env:LEGIO_GAME_ID + ').lnk')
    if ([IO.File]::Exists($path)) {
      $old = $shell.CreateShortcut($path)
      if ($old.Description -ne ('Legio game ' + $env:LEGIO_GAME_ID)) { throw 'Refusing to replace a shortcut not owned by Legio' }
    }
  }
}
$link = $shell.CreateShortcut($path)
$link.TargetPath = $env:LEGIO_LAUNCHER
$link.Arguments = '--launch-game=' + $env:LEGIO_GAME_ID
$link.Description = 'Legio game ' + $env:LEGIO_GAME_ID
if ($env:LEGIO_LINK_ICON) { $link.IconLocation = $env:LEGIO_LINK_ICON }
$link.Save()
foreach ($existingPath in [IO.Directory]::EnumerateFiles($folder, '*.lnk')) {
  if ($existingPath -eq $path) { continue }
  try { $existing = $shell.CreateShortcut($existingPath) } catch { continue }
  if ($existing.Description -eq ('Legio game ' + $env:LEGIO_GAME_ID)) { [IO.File]::Delete($existingPath) }
}
[Console]::Out.Write($path)
"#;

    const REMOVE_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$shell = New-Object -ComObject WScript.Shell
foreach ($base in @([Environment]::GetFolderPath('DesktopDirectory'), [Environment]::GetFolderPath('Programs'))) {
  foreach ($folder in @($base, (Join-Path $base 'Legio'))) {
    if (-not [IO.Directory]::Exists($folder)) { continue }
    foreach ($path in [IO.Directory]::EnumerateFiles($folder, '*.lnk')) {
      $link = $shell.CreateShortcut($path)
      if ($link.Description -eq ('Legio game ' + $env:LEGIO_GAME_ID)) { [IO.File]::Delete($path) }
    }
  }
}
"#;

    fn run(script: &str, variables: &[(&str, &str)]) -> Result<String, String> {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
        for (key, value) in variables {
            command.env(key, value);
        }
        let output = command
            .output()
            .map_err(|error| format!("Could not start Windows shortcut service: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "Windows shortcut service failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        String::from_utf8(output.stdout)
            .map_err(|error| format!("Windows shortcut path is not UTF-8: {error}"))
    }

    pub(super) fn create(
        game: &Game,
        location: ShortcutLocation,
        icon: Option<&Path>,
    ) -> Result<PathBuf, String> {
        let id = uuid::Uuid::parse_str(&game.id)
            .map_err(|_| "Game ID is invalid".to_owned())?
            .to_string();
        if game.steam_install_path.is_none() {
            let executable = game
                .executable_path
                .as_deref()
                .ok_or_else(|| "Select an executable first".to_owned())?;
            if !Path::new(executable).is_file() {
                return Err("Game executable is unavailable".to_owned());
            }
        }
        let launcher = std::env::current_exe()
            .map_err(|error| format!("Could not find Legio executable: {error}"))?;
        let launcher = launcher
            .to_str()
            .ok_or_else(|| "Legio executable path is not UTF-8".to_owned())?;
        let name = game
            .name
            .chars()
            .map(|character| {
                if "<>:\"/\\|?*".contains(character) || character.is_control() {
                    '_'
                } else {
                    character
                }
            })
            .collect::<String>();
        let name = name.trim().trim_end_matches('.');
        let name = if name.is_empty() { "Game" } else { name };
        let icon = icon.and_then(Path::to_str).unwrap_or(launcher);
        let path = run(
            CREATE_SCRIPT,
            &[
                (
                    "LEGIO_LINK_LOCATION",
                    match location {
                        ShortcutLocation::Desktop => "desktop",
                        ShortcutLocation::ApplicationsMenu => "menu",
                    },
                ),
                ("LEGIO_LINK_NAME", name),
                ("LEGIO_GAME_ID", &id),
                ("LEGIO_LAUNCHER", launcher),
                ("LEGIO_LINK_ICON", icon),
            ],
        )?;
        Ok(PathBuf::from(path))
    }

    pub(super) fn remove(game_id: &str) -> Result<(), String> {
        let id = uuid::Uuid::parse_str(game_id)
            .map_err(|_| "Game ID is invalid".to_owned())?
            .to_string();
        run(REMOVE_SCRIPT, &[("LEGIO_GAME_ID", &id)]).map(|_| ())
    }

    pub(super) fn steam_shortcut_arguments(game: &Game) -> Result<Option<Vec<String>>, String> {
        let id = game
            .steam_app_id
            .ok_or_else(|| "Steam game has no App ID".to_owned())?
            .to_string();
        let output = run(
            STEAM_SCRIPT,
            &[("LEGIO_STEAM_APP_ID", &id), ("LEGIO_STEAM_ACTION", "scan")],
        )?;
        let shortcuts: Vec<SteamShortcut> = serde_json::from_str(&output)
            .map_err(|error| format!("Could not read Steam shortcuts: {error}"))?;
        let mut found = None;
        for shortcut in shortcuts {
            let arguments = split_arguments(&shortcut.arguments)?;
            if arguments.is_empty() {
                continue;
            }
            if found
                .as_ref()
                .is_some_and(|previous| previous != &arguments)
            {
                return Err("Existing Steam shortcuts have different launch arguments".to_owned());
            }
            found = Some(arguments);
        }
        Ok(found)
    }
    pub(super) fn remove_steam_shortcuts(game: &Game) -> Result<(), String> {
        let id = game
            .steam_app_id
            .ok_or_else(|| "Steam game has no App ID".to_owned())?
            .to_string();
        run(
            STEAM_SCRIPT,
            &[
                ("LEGIO_STEAM_APP_ID", &id),
                ("LEGIO_STEAM_ACTION", "remove"),
            ],
        )
        .map(|_| ())
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn requested_game_id(
    arguments: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<Option<String>, String> {
    use std::os::unix::ffi::OsStrExt;

    let mut requested = None;
    for argument in arguments {
        let argument = argument.as_os_str().as_bytes();
        if argument == b"--launch-game" {
            return Err("Expected a game ID in --launch-game=<uuid>".to_owned());
        }
        let Some(game_id) = argument.strip_prefix(b"--launch-game=") else {
            continue;
        };
        if requested.is_some() {
            return Err("Only one --launch-game argument is allowed".to_owned());
        }
        let game_id = std::str::from_utf8(game_id)
            .map_err(|_| "The shortcut game ID is not valid UTF-8".to_owned())?;
        requested = Some(
            uuid::Uuid::parse_str(game_id)
                .map_err(|_| "The shortcut contains an invalid game ID".to_owned())?
                .to_string(),
        );
    }
    Ok(requested)
}

#[cfg(windows)]
pub(crate) fn requested_game_id(
    arguments: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<Option<String>, String> {
    let mut requested = None;
    for argument in arguments {
        let argument = argument
            .into_string()
            .map_err(|_| "The shortcut game ID is not valid UTF-8".to_owned())?;
        if argument == "--launch-game" {
            return Err("Expected a game ID in --launch-game=<uuid>".to_owned());
        }
        if let Some(game_id) = argument.strip_prefix("--launch-game=") {
            if requested.is_some() {
                return Err("Only one --launch-game argument is allowed".to_owned());
            }
            requested = Some(
                uuid::Uuid::parse_str(game_id)
                    .map_err(|_| "The shortcut contains an invalid game ID".to_owned())?
                    .to_string(),
            );
        }
    }
    Ok(requested)
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{Game, PathBuf, ShortcutLocation};
    use std::env;
    use std::fs::{self, OpenOptions};
    use std::io::{self, Write};
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::Path;
    use std::process::{Command, Stdio};

    fn exec_words(value: &str) -> Result<Vec<String>, String> {
        let mut words = Vec::new();
        let mut word = String::new();
        let mut quoted = false;
        let mut escaped = false;
        let mut started = false;
        for character in value.chars() {
            if escaped {
                word.push(character);
                escaped = false;
                started = true;
                continue;
            }
            if character == '\\' {
                escaped = true;
                started = true;
                continue;
            }
            if character == '"' {
                quoted = !quoted;
                started = true;
                continue;
            }
            if character.is_whitespace() && !quoted {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            } else {
                word.push(character);
                started = true;
            }
        }
        if escaped || quoted {
            return Err("Steam shortcut has malformed launch arguments".to_owned());
        }
        if started {
            words.push(word);
        }
        Ok(words)
    }

    fn steam_entries(game: &Game) -> Result<Vec<(PathBuf, Vec<String>)>, String> {
        let app_id = game
            .steam_app_id
            .ok_or_else(|| "Steam game has no App ID".to_owned())?
            .to_string();
        let mut entries = Vec::new();
        for directory in [desktop_directory()?, applications_directory()?] {
            let iter = match fs::read_dir(&directory) {
                Ok(iter) => iter,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("Could not inspect Steam shortcuts: {error}")),
            };
            for entry in iter {
                let entry =
                    entry.map_err(|error| format!("Could not inspect shortcut: {error}"))?;
                let path = entry.path();
                if path.extension().is_none_or(|ext| ext != "desktop") {
                    continue;
                }
                let metadata = fs::symlink_metadata(&path)
                    .map_err(|error| format!("Could not inspect shortcut: {error}"))?;
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata.len() > 64 * 1024
                {
                    continue;
                }
                let content = match fs::read_to_string(&path) {
                    Ok(content) => content,
                    Err(error) if error.kind() == io::ErrorKind::InvalidData => continue,
                    Err(error) => return Err(format!("Could not read shortcut: {error}")),
                };
                if content
                    .lines()
                    .any(|line| line.starts_with("X-Legio-GameId="))
                {
                    continue;
                }
                let Some(exec) = content.lines().find_map(|line| line.strip_prefix("Exec=")) else {
                    continue;
                };
                if !exec.contains(&app_id) {
                    continue;
                }
                let words = exec_words(exec)?;
                if !words
                    .first()
                    .and_then(|program| Path::new(program).file_name())
                    .is_some_and(|name| name == "steam" || name == "steam.sh")
                {
                    continue;
                }
                let match_at = words
                    .iter()
                    .position(|word| {
                        word == &format!("steam://rungameid/{app_id}")
                            || word == &format!("steam://run/{app_id}")
                    })
                    .map(|index| index + 1)
                    .or_else(|| {
                        words
                            .windows(2)
                            .position(|pair| pair[0] == "-applaunch" && pair[1] == app_id)
                            .map(|index| index + 2)
                    });
                if let Some(after) = match_at {
                    let arguments = words[after..]
                        .iter()
                        .filter(|word| !word.starts_with('%'))
                        .cloned()
                        .collect();
                    entries.push((path, arguments));
                }
            }
        }
        Ok(entries)
    }

    pub(super) fn steam_shortcut_arguments(game: &Game) -> Result<Option<Vec<String>>, String> {
        let mut found = None;
        for (_, arguments) in steam_entries(game)? {
            if arguments.is_empty() {
                continue;
            }
            if found
                .as_ref()
                .is_some_and(|previous| previous != &arguments)
            {
                return Err("Existing Steam shortcuts have different launch arguments".to_owned());
            }
            found = Some(arguments);
        }
        Ok(found)
    }

    pub(super) fn remove_steam_shortcuts(game: &Game) -> Result<(), String> {
        for (path, _) in steam_entries(game)? {
            fs::remove_file(&path).map_err(|error| {
                format!(
                    "Could not replace Steam shortcut {}: {error}",
                    path.display()
                )
            })?;
        }
        Ok(())
    }

    pub(super) fn create(
        game: &Game,
        location: ShortcutLocation,
        icon_path: Option<&Path>,
    ) -> Result<PathBuf, String> {
        let game_id = validate_game(game)?;
        let launcher = launcher_path()?;
        let contents = desktop_entry(&game.name, &launcher, &game_id, icon_path)?;
        let directory = match location {
            ShortcutLocation::Desktop => desktop_directory()?,
            ShortcutLocation::ApplicationsMenu => applications_directory()?,
        };
        require_utf8_directory(&directory)?;
        let filename = shortcut_filename(&game.name);
        let filename = if has_foreign_entry(&directory.join(&filename), &game_id)? {
            format!("{} ({game_id}).desktop", safe_filename(&game.name))
        } else {
            filename
        };
        let path = write_entry(&directory, &filename, &game_id, &contents)?;
        remove_other_owned_entries(&directory, &game_id, Some(&path))?;
        Ok(path)
    }

    pub(super) fn remove(game_id: &str) -> Result<(), String> {
        let game_id = uuid::Uuid::parse_str(game_id)
            .map_err(|_| "Could not remove game shortcuts: invalid game ID".to_owned())?
            .to_string();
        let mut errors = Vec::new();
        match desktop_directory() {
            Ok(directory) => {
                if let Err(error) = remove_other_owned_entries(&directory, &game_id, None) {
                    errors.push(format!("Desktop shortcut: {error}"));
                }
            }
            Err(error) => errors.push(format!("Desktop shortcut: {error}")),
        }
        match applications_directory() {
            Ok(directory) => {
                if let Err(error) = remove_other_owned_entries(&directory, &game_id, None) {
                    errors.push(format!("Application-menu shortcut: {error}"));
                }
            }
            Err(error) => errors.push(format!("Application-menu shortcut: {error}")),
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn require_utf8_directory(directory: &Path) -> Result<(), String> {
        directory
            .to_str()
            .ok_or_else(|| "The shortcut directory path is not valid UTF-8".to_owned())?;
        Ok(())
    }

    fn safe_filename(name: &str) -> String {
        let mut filename = String::new();
        for character in name.trim().chars() {
            let character = if character == '/' || character.is_control() {
                '_'
            } else {
                character
            };
            if filename.len() + character.len_utf8() > 160 {
                break;
            }
            filename.push(character);
        }
        let filename = filename.trim().trim_matches('.');
        if filename.is_empty() {
            "Game".to_owned()
        } else {
            filename.to_owned()
        }
    }

    fn shortcut_filename(name: &str) -> String {
        format!("{}.desktop", safe_filename(name))
    }

    fn has_foreign_entry(path: &Path, game_id: &str) -> Result<bool, String> {
        match fs::symlink_metadata(path) {
            Ok(metadata)
                if metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() <= 64 * 1024 =>
            {
                let contents = fs::read_to_string(path)
                    .map_err(|error| format!("Could not inspect desktop entry: {error}"))?;
                Ok(!is_owned_entry(&contents, game_id))
            }
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!("Could not inspect desktop entry: {error}")),
        }
    }

    fn remove_other_owned_entries(
        directory: &Path,
        game_id: &str,
        keep: Option<&Path>,
    ) -> Result<(), String> {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("Could not inspect shortcut directory: {error}")),
        };
        for entry in entries {
            let path = entry
                .map_err(|error| format!("Could not inspect shortcut: {error}"))?
                .path();
            if path
                .extension()
                .is_none_or(|extension| extension != "desktop")
                || keep == Some(path.as_path())
            {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("Could not inspect shortcut: {error}"))?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > 64 * 1024
            {
                continue;
            }
            let contents = fs::read_to_string(&path)
                .map_err(|error| format!("Could not inspect shortcut: {error}"))?;
            if is_owned_entry(&contents, game_id) {
                fs::remove_file(&path)
                    .map_err(|error| format!("Could not remove shortcut: {error}"))?;
            }
        }
        Ok(())
    }

    fn validate_game(game: &Game) -> Result<String, String> {
        let game_id = uuid::Uuid::parse_str(&game.id)
            .map_err(|_| "Could not create a game shortcut: invalid game ID".to_owned())?
            .to_string();
        if game.steam_install_path.is_some() {
            if game.steam_app_id.is_none() {
                return Err("Steam game has no App ID".to_owned());
            }
        } else {
            let executable = game
                .executable_path
                .as_deref()
                .ok_or_else(|| "This manual game has no selected executable".to_owned())?;
            let executable = Path::new(executable);
            if !executable.is_file()
                || !executable
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
            {
                return Err("Selected file is not an available Windows executable".to_owned());
            }
        }
        if game.name.trim().is_empty() {
            return Err("This game has no display name for its shortcut".to_owned());
        }
        Ok(game_id)
    }

    fn desktop_entry(
        name: &str,
        launcher: &Path,
        game_id: &str,
        icon_path: Option<&Path>,
    ) -> Result<String, String> {
        let launcher = launcher
            .to_str()
            .ok_or_else(|| "Legio's executable path is not valid UTF-8".to_owned())?;
        let launcher = quote_exec_argument(launcher)?;
        let icon = icon_path
            .map(|path| {
                let path = path
                    .to_str()
                    .ok_or_else(|| "Game icon path is not valid UTF-8".to_owned())?;
                if !Path::new(path).is_absolute() {
                    return Err("Game icon path is not absolute".to_owned());
                }
                Ok(format!("Icon={}\n", escape_entry_value(path)))
            })
            .transpose()?
            .unwrap_or_default();
        Ok(format!(
            "[Desktop Entry]\nType=Application\nName={}\n{icon}Exec={launcher} --launch-game={game_id}\nTerminal=false\nCategories=Game;\nX-Legio-GameId={game_id}\n",
            escape_entry_value(name)
        ))
    }

    fn escape_entry_value(value: &str) -> String {
        let mut escaped = String::with_capacity(value.len());
        for character in value.chars() {
            match character {
                '\\' => escaped.push_str("\\\\"),
                '\n' => escaped.push_str("\\n"),
                '\r' => escaped.push_str("\\r"),
                '\t' => escaped.push_str("\\t"),
                character if character.is_control() => escaped.push(' '),
                character => escaped.push(character),
            }
        }
        escaped
    }

    fn quote_exec_argument(value: &str) -> Result<String, String> {
        if value.chars().any(char::is_control) {
            return Err("Legio's executable path contains unsupported characters".to_owned());
        }
        let mut quoted = String::with_capacity(value.len() + 2);
        quoted.push('"');
        for character in value.chars() {
            match character {
                '\\' => escape_exec_character(&mut quoted, character, 3),
                '"' => escape_exec_character(&mut quoted, character, 3),
                '$' | '`' => escape_exec_character(&mut quoted, character, 2),
                '%' => quoted.push_str("%%"),
                character => quoted.push(character),
            }
        }
        quoted.push('"');
        Ok(quoted)
    }

    fn escape_exec_character(output: &mut String, character: char, slashes: u8) {
        for _ in 0..slashes {
            output.push('\\');
        }
        output.push(character);
    }

    fn home_directory() -> Result<PathBuf, String> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "Could not locate the current user's home directory".to_owned())?;
        if !home.is_absolute() {
            return Err("The current user's home directory is not absolute".to_owned());
        }
        Ok(home)
    }

    fn applications_directory() -> Result<PathBuf, String> {
        let home = home_directory()?;
        let data_home = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".local/share"));
        Ok(data_home.join("applications"))
    }

    fn desktop_directory() -> Result<PathBuf, String> {
        let home = home_directory()?;
        let output = Command::new("xdg-user-dir")
            .arg("DESKTOP")
            .stdin(Stdio::null())
            .output();
        let directory = match output {
            Ok(output) if !output.status.success() => {
                return Err(format!(
                    "Could not resolve the desktop directory with xdg-user-dir (exit {})",
                    output
                        .status
                        .code()
                        .map_or_else(|| "unknown".to_owned(), |code| code.to_string())
                ));
            }
            Ok(output) => parse_desktop_directory(&output.stdout)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => home.join("Desktop"),
            Err(error) => {
                return Err(format!("Could not start xdg-user-dir: {error}"));
            }
        };
        Ok(directory)
    }

    fn parse_desktop_directory(output: &[u8]) -> Result<PathBuf, String> {
        let value = std::str::from_utf8(output)
            .map_err(|_| "xdg-user-dir returned a non-UTF-8 desktop path".to_owned())?;
        let value = value.trim_end_matches(['\r', '\n']);
        if value.is_empty() || value.contains(['\r', '\n']) {
            return Err("xdg-user-dir returned an invalid desktop path".to_owned());
        }
        let directory = PathBuf::from(value);
        if !directory.is_absolute() {
            return Err("xdg-user-dir returned a non-absolute desktop path".to_owned());
        }
        Ok(directory)
    }

    fn launcher_path() -> Result<PathBuf, String> {
        if env::var_os("LEGIO_AUR_PACKAGE").as_deref() == Some(std::ffi::OsStr::new("1")) {
            return Ok(PathBuf::from("/usr/bin/legio-launcher"));
        }
        let current = env::current_exe()
            .map_err(|error| format!("Could not locate Legio's executable: {error}"))?;
        let app_dir = env::var_os("APPDIR").map(PathBuf::from);
        let app_image = env::var_os("APPIMAGE").map(PathBuf::from);
        resolve_launcher(&current, app_dir.as_deref(), app_image.as_deref())
    }

    fn resolve_launcher(
        current: &Path,
        app_dir: Option<&Path>,
        app_image: Option<&Path>,
    ) -> Result<PathBuf, String> {
        let current = fs::canonicalize(current)
            .map_err(|error| format!("Could not resolve Legio's executable: {error}"))?;
        let Some(app_dir) = app_dir.and_then(|path| fs::canonicalize(path).ok()) else {
            return Ok(current);
        };
        if !current.starts_with(app_dir) {
            return Ok(current);
        }
        let app_image = app_image
            .ok_or_else(|| "Could not resolve the AppImage file for this shortcut".to_owned())?;
        let app_image = fs::canonicalize(app_image)
            .map_err(|error| format!("Could not resolve the AppImage file: {error}"))?;
        let metadata = app_image
            .metadata()
            .map_err(|error| format!("Could not inspect the AppImage file: {error}"))?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
            return Err("The AppImage file is not executable".to_owned());
        }
        Ok(app_image)
    }

    fn write_entry(
        directory: &Path,
        filename: &str,
        game_id: &str,
        contents: &str,
    ) -> Result<PathBuf, String> {
        fs::create_dir_all(directory)
            .map_err(|error| format!("Could not create shortcut directory: {error}"))?;
        let path = directory.join(filename);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                if metadata.len() > 64 * 1024 {
                    return Err("Refusing to replace an oversized desktop entry".to_owned());
                }
                let existing = fs::read_to_string(&path)
                    .map_err(|error| format!("Could not inspect existing shortcut: {error}"))?;
                if !is_owned_entry(&existing, game_id) {
                    return Err("Refusing to replace a desktop entry not owned by Legio".to_owned());
                }
            }
            Ok(_) => return Err("The shortcut path is not a regular file".to_owned()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Could not inspect shortcut path: {error}")),
        }

        let temporary = directory.join(format!(".{filename}.{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o700)
                .open(&temporary)
                .map_err(|error| format!("Could not create temporary shortcut: {error}"))?;
            file.write_all(contents.as_bytes())
                .map_err(|error| format!("Could not write desktop entry: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("Could not flush desktop entry: {error}"))?;
            file.set_permissions(fs::Permissions::from_mode(0o755))
                .map_err(|error| format!("Could not make desktop entry executable: {error}"))?;
            fs::rename(&temporary, &path)
                .map_err(|error| format!("Could not install desktop entry: {error}"))?;
            Ok(())
        })();
        if let Err(error) = result {
            if let Err(cleanup_error) = fs::remove_file(&temporary)
                && cleanup_error.kind() != io::ErrorKind::NotFound
            {
                return Err(format!(
                    "{error}; could not remove temporary file: {cleanup_error}"
                ));
            }
            return Err(error);
        }
        Ok(path)
    }

    #[cfg(test)]
    fn remove_owned_entry(directory: &Path, filename: &str, game_id: &str) -> Result<(), String> {
        let path = directory.join(filename);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("Could not inspect shortcut: {error}")),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("shortcut path is not a regular file".to_owned());
        }
        if metadata.len() > 64 * 1024 {
            return Err("refusing to remove an oversized desktop entry".to_owned());
        }
        let contents = fs::read_to_string(&path)
            .map_err(|error| format!("Could not inspect shortcut: {error}"))?;
        if !is_owned_entry(&contents, game_id) {
            return Err("refusing to remove a desktop entry not owned by Legio".to_owned());
        }
        fs::remove_file(path).map_err(|error| format!("Could not remove shortcut: {error}"))
    }

    fn is_owned_entry(contents: &str, game_id: &str) -> bool {
        let mut lines = contents.lines();
        let marker = format!("X-Legio-GameId={game_id}");
        lines.next() == Some("[Desktop Entry]") && lines.any(|line| line == marker)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn test_dir(label: &str) -> PathBuf {
            let path =
                env::temp_dir().join(format!("legio-shortcut-{label}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            path
        }

        #[test]
        fn desktop_entry_escapes_name_and_exec_arguments() {
            let entry = desktop_entry(
                "Round\n\"Trip\"",
                Path::new("/opt/Legio App %$`\\.AppImage"),
                "00000000-0000-0000-0000-000000000001",
                None,
            )
            .unwrap();
            assert!(entry.contains("Name=Round\\n\"Trip\""));
            assert!(
                entry.contains(
                    "Exec=\"/opt/Legio App %%\\\\$\\\\`\\\\\\\\.AppImage\" --launch-game=00000000-0000-0000-0000-000000000001"
                ),
                "{entry:?}"
            );
            assert!(!entry.lines().any(|line| line == "\"Trip\""));
            let directory = test_dir("escaped-entry");
            let path = write_entry(
                &directory,
                "legio-game-00000000-0000-0000-0000-000000000001.desktop",
                "00000000-0000-0000-0000-000000000001",
                &entry,
            )
            .unwrap();
            if let Ok(output) = Command::new("desktop-file-validate").arg(&path).output() {
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            fs::remove_dir_all(directory).unwrap();
        }

        #[test]
        fn shortcut_is_executable_and_refuses_unowned_replacement() {
            let directory = test_dir("write");
            let contents = desktop_entry(
                "Test Game",
                Path::new("/usr/bin/legio"),
                "00000000-0000-0000-0000-000000000001",
                None,
            )
            .unwrap();
            let path = write_entry(
                &directory,
                "legio-game-00000000-0000-0000-0000-000000000001.desktop",
                "00000000-0000-0000-0000-000000000001",
                &contents,
            )
            .unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), contents);
            assert_ne!(fs::metadata(&path).unwrap().permissions().mode() & 0o111, 0);

            fs::write(
                &path,
                "[Desktop Entry]\nName=Other application\nX-Legio-GameId=other-game\n",
            )
            .unwrap();
            let error = write_entry(
                &directory,
                "legio-game-00000000-0000-0000-0000-000000000001.desktop",
                "00000000-0000-0000-0000-000000000001",
                "replacement",
            )
            .unwrap_err();
            assert!(error.contains("not owned by Legio"));
            fs::remove_dir_all(directory).unwrap();
        }

        #[test]
        fn game_shortcut_uses_readable_filename_and_preserves_name_collisions() {
            let directory = test_dir("readable-name");
            let first_id = "00000000-0000-0000-0000-000000000001";
            let second_id = "00000000-0000-0000-0000-000000000002";
            let filename = shortcut_filename("Portal/Two");
            assert_eq!(filename, "Portal_Two.desktop");
            let first = desktop_entry(
                "Portal/Two",
                Path::new("/usr/bin/legio-launcher"),
                first_id,
                None,
            )
            .unwrap();
            write_entry(&directory, &filename, first_id, &first).unwrap();
            assert!(has_foreign_entry(&directory.join(&filename), second_id).unwrap());
            assert!(!has_foreign_entry(&directory.join(&filename), first_id).unwrap());
            remove_other_owned_entries(&directory, first_id, None).unwrap();
            assert!(!directory.join(filename).exists());
            fs::remove_dir_all(directory).unwrap();
        }

        #[test]
        fn removes_only_legio_owned_shortcuts() {
            let directory = test_dir("remove-owned");
            let game_id = "00000000-0000-0000-0000-000000000001";
            let filename = format!("legio-game-{game_id}.desktop");
            let contents =
                desktop_entry("Test Game", Path::new("/usr/bin/legio"), game_id, None).unwrap();
            let path = write_entry(&directory, &filename, game_id, &contents).unwrap();

            remove_owned_entry(&directory, &filename, game_id).unwrap();

            assert!(!path.exists());
            fs::remove_dir_all(directory).unwrap();
        }

        #[test]
        fn refuses_to_remove_a_shortcut_not_owned_by_legio() {
            let directory = test_dir("remove-foreign");
            let game_id = "00000000-0000-0000-0000-000000000001";
            let filename = format!("legio-game-{game_id}.desktop");
            let path = directory.join(&filename);
            fs::write(
                &path,
                "[Desktop Entry]\nName=Other app\nX-Legio-GameId=other-game\n",
            )
            .unwrap();

            let error = remove_owned_entry(&directory, &filename, game_id).unwrap_err();

            assert!(error.contains("not owned by Legio"));
            assert!(path.is_file());
            fs::remove_dir_all(directory).unwrap();
        }

        #[test]
        fn refuses_to_remove_a_symlinked_shortcut() {
            use std::os::unix::fs::symlink;

            let directory = test_dir("remove-symlink");
            let game_id = "00000000-0000-0000-0000-000000000001";
            let filename = format!("legio-game-{game_id}.desktop");
            let target = directory.join("outside.desktop");
            fs::write(
                &target,
                format!("[Desktop Entry]\nX-Legio-GameId={game_id}\n"),
            )
            .unwrap();
            symlink(&target, directory.join(&filename)).unwrap();

            assert!(remove_owned_entry(&directory, &filename, game_id).is_err());
            assert!(target.is_file());
            fs::remove_dir_all(directory).unwrap();
        }

        #[test]
        fn desktop_directory_output_must_be_absolute_and_single_line() {
            assert_eq!(
                parse_desktop_directory(b"/home/test/Desktop\r\n").unwrap(),
                PathBuf::from("/home/test/Desktop")
            );
            assert!(parse_desktop_directory(b"Desktop\n").is_err());
            assert!(parse_desktop_directory(b"/home/test/Desktop\nInjected").is_err());
            assert!(parse_desktop_directory(b"\xff").is_err());
        }

        #[test]
        fn shortcut_directory_must_be_returnable_to_the_frontend() {
            use std::os::unix::ffi::OsStringExt;

            let directory =
                PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/Legio-\xff".to_vec()));
            assert!(require_utf8_directory(&directory).is_err());
        }

        #[test]
        fn installed_desktop_entry_passes_desktop_file_validate_when_available() {
            let directory = test_dir("validate");
            let contents = desktop_entry(
                "Game with % and \"quotes\"",
                Path::new("/opt/Legio App.AppImage"),
                "00000000-0000-0000-0000-000000000001",
                Some(Path::new("/tmp/Legio Icons/game icon.png")),
            )
            .unwrap();
            let path = write_entry(
                &directory,
                "legio-game-00000000-0000-0000-0000-000000000001.desktop",
                "00000000-0000-0000-0000-000000000001",
                &contents,
            )
            .unwrap();
            assert!(contents.contains("Icon=/tmp/Legio Icons/game icon.png\n"));
            if let Ok(output) = Command::new("desktop-file-validate").arg(&path).output() {
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            fs::remove_dir_all(directory).unwrap();
        }

        #[test]
        fn launch_argument_requires_one_valid_game_id() {
            use std::ffi::OsString;
            use std::os::unix::ffi::OsStringExt;
            assert_eq!(
                super::super::requested_game_id([OsString::from(
                    "--launch-game=00000000-0000-0000-0000-000000000001"
                )])
                .unwrap(),
                Some("00000000-0000-0000-0000-000000000001".to_owned())
            );
            assert!(
                super::super::requested_game_id([OsString::from("--launch-game=invalid")]).is_err()
            );
            assert!(
                super::super::requested_game_id([
                    OsString::from("--launch-game=00000000-0000-0000-0000-000000000001"),
                    OsString::from("--launch-game=00000000-0000-0000-0000-000000000002")
                ])
                .is_err()
            );
            assert!(
                super::super::requested_game_id([OsString::from_vec(
                    b"--launch-game=\xff".to_vec()
                )])
                .is_err()
            );
        }

        #[test]
        fn shortcut_accepts_manual_app_id_and_requires_existing_exe() {
            let directory = test_dir("game-validation");
            let executable = directory.join("Test.exe");
            fs::write(&executable, b"test").unwrap();
            let game = Game {
                id: "00000000-0000-0000-0000-000000000001".to_owned(),
                steam_app_id: None,
                automatic_name: None,
                name_override: Some("Test Game".to_owned()),
                name: "Test Game".to_owned(),
                steam_install_path: None,
                steam_account_id: None,
                executable_path: Some(executable.to_string_lossy().into_owned()),
            };
            assert_eq!(validate_game(&game).unwrap(), game.id);

            let mut steam_game = game.clone();
            steam_game.steam_app_id = Some(42);
            assert_eq!(validate_game(&steam_game).unwrap(), game.id);

            let mut missing_exe = game.clone();
            missing_exe.executable_path =
                Some(directory.join("missing.exe").to_string_lossy().into_owned());
            assert!(validate_game(&missing_exe).is_err());
            fs::remove_dir_all(directory).unwrap();
        }

        #[test]
        fn appimage_shortcut_uses_the_persistent_image_path() {
            let root = test_dir("appimage");
            let app_dir = root.join("mounted-app");
            let executable = app_dir.join("usr/bin/legio");
            let app_image = root.join("Legio.AppImage");
            fs::create_dir_all(executable.parent().unwrap()).unwrap();
            fs::write(&executable, b"bundled binary").unwrap();
            fs::write(&app_image, b"app image").unwrap();
            fs::set_permissions(&app_image, fs::Permissions::from_mode(0o755)).unwrap();

            assert_eq!(
                resolve_launcher(&executable, Some(&app_dir), Some(&app_image)).unwrap(),
                fs::canonicalize(app_image).unwrap()
            );
            fs::remove_dir_all(root).unwrap();
        }
    }
}
