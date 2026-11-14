//! Diagnostics for support: the BepInEx log of the active profile and a plain
//! text report of the setup. The home folder of the user is replaced by `~`.

use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};

use super::launch::{self, ModLaunchStatus};
use super::paths::ModdingPaths;
use super::profile_commands::run_modding;
use super::profiles::profile_with_directory;
use crate::database::{Database, ProfileMod};

const MAX_LOG_BYTES: u64 = 1024 * 1024;
// BepInEx 5 and 6 write the .log name; the .txt name is kept for older builds.
const LOG_NAMES: [&str; 2] = ["LogOutput.log", "LogOutput.txt"];
const NO_LOG: &str = "No BepInEx log found: launch the game with this profile at least once";
const NO_PROFILE: &str = "This game has no active mod profile";

/// The most recently written BepInEx log of the profile.
fn find_log(profile_dir: &Path) -> Result<PathBuf, String> {
    let bepinex = profile_dir.join("BepInEx");
    LOG_NAMES
        .iter()
        .map(|name| bepinex.join(name))
        .filter_map(|path| {
            let metadata = fs::metadata(&path).ok().filter(fs::Metadata::is_file)?;
            Some((metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH), path))
        })
        .max()
        .map(|(_, path)| path)
        .ok_or_else(|| NO_LOG.to_owned())
}

/// The last `MAX_LOG_BYTES` of the file, starting at a line boundary.
fn read_tail(path: &Path) -> Result<String, String> {
    read_tail_limited(path, MAX_LOG_BYTES)
}

/// The last `limit` bytes of the file, starting at a line boundary.
pub(crate) fn read_tail_limited(path: &Path, limit: u64) -> Result<String, String> {
    let read_error = |error: std::io::Error| format!("Could not read the log: {error}");
    let mut file = File::open(path).map_err(read_error)?;
    let length = file.metadata().map_err(read_error)?.len();
    let start = length.saturating_sub(limit);
    file.seek(SeekFrom::Start(start)).map_err(read_error)?;
    let mut bytes = Vec::new();
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(read_error)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = if start > 0 {
        text.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        &text
    };
    Ok(text.to_owned())
}

/// Replaces `home` (in either slash style, ignoring ASCII case) with `~`.
pub(crate) fn redact_home(text: &str, home: &Path) -> String {
    let home = home.to_string_lossy();
    if home.is_empty() {
        return text.to_owned();
    }
    let mut redacted = text.to_owned();
    for variant in [
        home.to_string(),
        home.replace('\\', "/"),
        home.replace('/', "\\"),
    ] {
        let needle = variant.trim_end_matches(['/', '\\']).to_ascii_lowercase();
        if needle.is_empty() {
            continue;
        }
        let mut result = String::with_capacity(redacted.len());
        let lowered = redacted.to_ascii_lowercase();
        let mut cursor = 0;
        while let Some(found) = lowered[cursor..].find(&needle) {
            let start = cursor + found;
            result.push_str(&redacted[cursor..start]);
            result.push('~');
            cursor = start + needle.len();
        }
        result.push_str(&redacted[cursor..]);
        redacted = result;
    }
    redacted
}

struct ReportData<'a> {
    legio_version: &'a str,
    game_name: &'a str,
    ecosystem_game_id: Option<&'a str>,
    launch_mode: &'a str,
    profile_name: Option<&'a str>,
    status: &'a ModLaunchStatus,
    mods: &'a [ProfileMod],
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn build_report(data: &ReportData<'_>) -> String {
    let mut lines = vec![
        format!("Legio: {}", data.legio_version),
        format!("OS: {} ({})", std::env::consts::OS, std::env::consts::ARCH),
        format!("Game: {}", data.game_name),
        format!(
            "Thunderstore community: {}",
            data.ecosystem_game_id.unwrap_or("none")
        ),
        format!("Launch mode: {}", data.launch_mode),
        format!("Active profile: {}", data.profile_name.unwrap_or("none")),
        format!("Loader deployed: {}", yes_no(data.status.loader_deployed)),
        format!(
            "Doorstop version: {}",
            data.status
                .doorstop_version
                .map_or_else(|| "unknown".to_owned(), |version| version.to_string())
        ),
        format!("Preloader found: {}", yes_no(data.status.preloader_found)),
        format!("Mods ({}):", data.mods.len()),
    ];
    lines.extend(data.mods.iter().map(|entry| {
        let state = if entry.enabled { "enabled" } else { "disabled" };
        format!("- {} {} ({state})", entry.full_name, entry.version)
    }));
    lines.join("\n")
}

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

/// The directory of the game's active profile.
fn active_profile_dir(
    database: &Database,
    paths: &ModdingPaths,
    game_id: &str,
) -> Result<PathBuf, String> {
    let profile_id = database
        .game_modding(game_id)?
        .and_then(|modding| modding.active_profile_id)
        .ok_or_else(|| NO_PROFILE.to_owned())?;
    paths.profile_dir(game_id, &profile_id)
}

#[tauri::command]
pub async fn read_mod_log(app: tauri::AppHandle, game_id: String) -> Result<String, String> {
    run_modding(app, move |_, database, paths, _| {
        let log = find_log(&active_profile_dir(database, paths, &game_id)?)?;
        Ok(redact_home(&read_tail(&log)?, &home_dir()))
    })
    .await
}

#[tauri::command]
pub async fn open_mod_log(app: tauri::AppHandle, game_id: String) -> Result<(), String> {
    run_modding(app, move |_, database, paths, _| {
        let log = find_log(&active_profile_dir(database, paths, &game_id)?)?;
        crate::installed_folder::open_file(&log)
    })
    .await
}

#[tauri::command]
pub async fn build_mod_support_report(
    app: tauri::AppHandle,
    game_id: String,
) -> Result<String, String> {
    run_modding(app, move |app, database, paths, _| {
        let game = database.game(&game_id)?;
        let modding = database.game_modding(&game.id)?;
        let profile = match modding
            .as_ref()
            .and_then(|modding| modding.active_profile_id.as_deref())
        {
            Some(profile_id) => Some(profile_with_directory(database, paths, profile_id)?.0),
            None => None,
        };
        let mods = match &profile {
            Some(profile) => database.profile_mods(&profile.id)?,
            None => Vec::new(),
        };
        let status = launch::status(
            database,
            paths,
            &game.id,
            launch::can_launch_with_mods(&game),
        )?;
        let version = app.package_info().version.to_string();
        let report = build_report(&ReportData {
            legio_version: &version,
            game_name: &game.name,
            ecosystem_game_id: modding
                .as_ref()
                .map(|modding| modding.ecosystem_game_id.as_str()),
            launch_mode: launch::launch_mode(&game),
            profile_name: profile.as_ref().map(|profile| profile.name.as_str()),
            status: &status,
            mods: &mods,
        });
        Ok(redact_home(&report, &home_dir()))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modding::test_support::TempDir;

    fn write_log(profile: &Path, name: &str, content: &[u8]) {
        let directory = profile.join("BepInEx");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(name), content).unwrap();
    }

    #[test]
    fn missing_log_has_a_clear_error() {
        let profile = TempDir::new();
        assert_eq!(find_log(&profile).unwrap_err(), NO_LOG);
        fs::create_dir_all(profile.join("BepInEx/LogOutput.log")).unwrap();
        assert_eq!(find_log(&profile).unwrap_err(), NO_LOG);
    }

    #[test]
    fn finds_log_name_variants() {
        let profile = TempDir::new();
        write_log(&profile, "LogOutput.txt", b"old");
        assert!(find_log(&profile).unwrap().ends_with("LogOutput.txt"));
        std::thread::sleep(std::time::Duration::from_millis(50));
        write_log(&profile, "LogOutput.log", b"new");
        assert!(find_log(&profile).unwrap().ends_with("LogOutput.log"));
    }

    #[test]
    fn small_logs_are_read_whole() {
        let profile = TempDir::new();
        write_log(&profile, "LogOutput.log", b"line 1\nline 2\n");
        let log = find_log(&profile).unwrap();
        assert_eq!(read_tail(&log).unwrap(), "line 1\nline 2\n");
    }

    #[test]
    fn long_logs_keep_the_last_mebibyte_from_a_line_start() {
        let profile = TempDir::new();
        let line = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abc\n";
        let content = line.repeat((MAX_LOG_BYTES as usize / line.len()) + 10);
        write_log(&profile, "LogOutput.log", content.as_bytes());
        let tail = read_tail(&find_log(&profile).unwrap()).unwrap();
        assert!(tail.len() as u64 <= MAX_LOG_BYTES);
        assert!(tail.len() as u64 > MAX_LOG_BYTES - line.len() as u64 * 2);
        assert!(tail.starts_with("0123456789abcdef"));
        assert!(tail.ends_with(line));
    }

    #[test]
    fn invalid_utf8_in_the_log_is_replaced() {
        let profile = TempDir::new();
        write_log(&profile, "LogOutput.log", b"ok \xff\xfe end");
        let text = read_tail(&find_log(&profile).unwrap()).unwrap();
        assert!(text.starts_with("ok ") && text.ends_with(" end"));
    }

    #[test]
    fn redacts_the_home_folder() {
        let home = Path::new("C:\\Users\\Mario Rossi");
        let text =
            "path C:\\Users\\Mario Rossi\\AppData and c:/users/mario rossi/x, C:\\Users\\Other";
        assert_eq!(
            redact_home(text, home),
            "path ~\\AppData and ~/x, C:\\Users\\Other"
        );
        assert_eq!(redact_home("a/b", Path::new("")), "a/b");
        assert_eq!(redact_home("/home/me/x", Path::new("/home/me")), "~/x");
    }

    #[test]
    fn report_lists_the_setup() {
        let mods = [
            ProfileMod {
                profile_id: "p".to_owned(),
                full_name: "Owner-Mod".to_owned(),
                version: "1.2.3".to_owned(),
                enabled: true,
                online_source: true,
                installed_at: 0,
                install_order: 0,
                local_cache_key: None,
            },
            ProfileMod {
                profile_id: "p".to_owned(),
                full_name: "Owner-Other".to_owned(),
                version: "0.1.0".to_owned(),
                enabled: false,
                online_source: true,
                installed_at: 0,
                install_order: 1,
                local_cache_key: None,
            },
        ];
        let status = ModLaunchStatus {
            bound: true,
            ecosystem_game_id: Some("lethal-company".to_owned()),
            active_profile_id: Some("p".to_owned()),
            launch_supported: true,
            loader_deployed: true,
            preloader_found: true,
            doorstop_version: Some(4),
        };
        let report = build_report(&ReportData {
            legio_version: "0.2.4",
            game_name: "Lethal Company",
            ecosystem_game_id: Some("lethal-company"),
            launch_mode: "wine/proton",
            profile_name: Some("Default"),
            status: &status,
            mods: &mods,
        });
        for expected in [
            "Legio: 0.2.4",
            "Game: Lethal Company",
            "Thunderstore community: lethal-company",
            "Launch mode: wine/proton",
            "Active profile: Default",
            "Loader deployed: yes",
            "Doorstop version: 4",
            "Preloader found: yes",
            "Mods (2):",
            "- Owner-Mod 1.2.3 (enabled)",
            "- Owner-Other 0.1.0 (disabled)",
        ] {
            assert!(report.lines().any(|line| line == expected), "{expected}");
        }
    }
}
