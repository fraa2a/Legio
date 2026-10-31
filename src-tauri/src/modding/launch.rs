//! Launching a game with its active mod profile (BepInEx via Unity Doorstop).
//!
//! Ported from r2modmanPlus (MIT): `BepInExGameInstructions`, `UnityDoorstopUtils`,
//! `GameInstructionParser` and `ModLinker`. The loader root files of the profile
//! are copied next to the game executable and Doorstop is steered with arguments.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use super::paths::ModdingPaths;
use crate::database::{Database, DatabaseState, Game};

const PRELOADERS: [&str; 5] = [
    "BepInEx.Unity.Mono.Preloader.dll",
    "BepInEx.Unity.IL2CPP.dll",
    "BepInEx.Preloader.dll",
    "BepInEx.IL2CPP.dll",
    "BepInEx.NET.CoreCLR.dll",
];
const DOORSTOP_VERSION_FILE: &str = ".doorstop_version";
const CORLIB_DIRECTORY: &str = "unstripped_corlib";
// Package bookkeeping that is not part of a mod loader.
const UNDEPLOYED_ROOT_FILES: [&str; 4] = ["mods.yml", "manifest.json", "readme.md", "icon.png"];
#[cfg(target_os = "linux")]
const PROXY_DLL: &str = "winhttp";

/// How the preloader path is written for the process that reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathStyle {
    Native,
    Wine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModLaunchPlan {
    pub arguments: Vec<String>,
    /// The proxy DLL must load as native inside the Wine prefix.
    pub wine_dll_override: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModLaunchStatus {
    pub bound: bool,
    pub ecosystem_game_id: Option<String>,
    pub active_profile_id: Option<String>,
    /// False when Legio cannot inject the loader for this game on this system.
    pub launch_supported: bool,
    pub loader_deployed: bool,
    pub preloader_found: bool,
    pub doorstop_version: Option<u32>,
}

/// Plan for launching `game`, or `None` when the game launches as usual. May
/// copy loader files into the game folder and patch the Steam Proton prefix.
pub(crate) fn prepare_launch<R: Runtime>(
    app: &AppHandle<R>,
    game: &Game,
) -> Result<Option<ModLaunchPlan>, String> {
    let state = app.state::<DatabaseState>();
    let database = state.database()?;
    if database.game_modding(&game.id)?.is_none() {
        return Ok(None);
    }
    let Some(style) = path_style(game) else {
        return Ok(None);
    };
    let paths = ModdingPaths::new(&data_dir(app)?);
    let plan = prepare(database, &paths, &game.id, &game_directory(game)?, style)?;
    #[cfg(target_os = "linux")]
    if plan.as_ref().is_some_and(|plan| plan.wine_dll_override) && game.steam_install_path.is_some()
    {
        patch_steam_prefix(game)?;
    }
    Ok(plan)
}

fn launch_status<R: Runtime>(app: &AppHandle<R>, game_id: &str) -> Result<ModLaunchStatus, String> {
    let state = app.state::<DatabaseState>();
    let database = state.database()?;
    let game = database.game(game_id)?;
    let paths = ModdingPaths::new(&data_dir(app)?);
    status(database, &paths, &game.id, path_style(&game).is_some())
}

#[tauri::command]
pub async fn get_mod_launch_status(
    app: AppHandle,
    game_id: String,
) -> Result<ModLaunchStatus, String> {
    tauri::async_runtime::spawn_blocking(move || launch_status(&app, &game_id))
        .await
        .map_err(|error| format!("Mod launch status task failed: {error}"))?
}

#[tauri::command]
pub async fn restore_modded_game_folder(app: AppHandle, game_id: String) -> Result<(), String> {
    crate::startup_recovery::wait(&app).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<crate::game_lifecycle::GameLaunchManager>();
        let _operation = manager.operation()?;
        manager.require_idle(&game_id)?;
        restore_game_folder_of(&app, &game_id)
    })
    .await
    .map_err(|error| format!("Game folder restore task failed: {error}"))?
}

/// Deletes the loader files Legio copied into the game folder.
fn restore_game_folder_of<R: Runtime>(app: &AppHandle<R>, game_id: &str) -> Result<(), String> {
    let state = app.state::<DatabaseState>();
    let database = state.database()?;
    let game = database.game(game_id)?;
    restore_game_folder(database, &game.id, &game_directory(&game)?)
}

fn data_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))
}

fn prepare(
    database: &Database,
    paths: &ModdingPaths,
    game_id: &str,
    game_dir: &Path,
    style: PathStyle,
) -> Result<Option<ModLaunchPlan>, String> {
    let Some(modding) = database.game_modding(game_id)? else {
        return Ok(None);
    };
    let Some(profile_id) = modding.active_profile_id else {
        return vanilla_plan(database, game_id, game_dir);
    };
    let profile_dir = paths.profile_dir(game_id, &profile_id)?;
    let preloader = find_preloader(&profile_dir)?;
    let version = doorstop_version(&profile_dir.join(DOORSTOP_VERSION_FILE));
    deploy_root_files(database, game_id, &profile_dir, game_dir)?;
    let preloader = path_argument(&preloader, style)?;
    let corlib = profile_dir.join(CORLIB_DIRECTORY);
    let corlib = if style == PathStyle::Wine && corlib.is_dir() {
        Some(path_argument(&corlib, style)?)
    } else {
        None
    };
    Ok(Some(ModLaunchPlan {
        arguments: enable_arguments(version, &preloader, corlib.as_deref()),
        wine_dll_override: style == PathStyle::Wine,
    }))
}

// A vanilla launch must switch Doorstop off while its files stay in the game folder.
fn vanilla_plan(
    database: &Database,
    game_id: &str,
    game_dir: &Path,
) -> Result<Option<ModLaunchPlan>, String> {
    let deployed = database.deployed_files(game_id)?;
    if deployed.is_empty() {
        return Ok(None);
    }
    let version = if deployed.iter().any(|path| path == DOORSTOP_VERSION_FILE) {
        doorstop_version(&game_dir.join(DOORSTOP_VERSION_FILE))
    } else {
        3
    };
    Ok(Some(ModLaunchPlan {
        arguments: disable_arguments(version),
        wine_dll_override: false,
    }))
}

fn status(
    database: &Database,
    paths: &ModdingPaths,
    game_id: &str,
    launch_supported: bool,
) -> Result<ModLaunchStatus, String> {
    let modding = database.game_modding(game_id)?;
    let deployed = database.deployed_files(game_id)?;
    let mut status = ModLaunchStatus {
        bound: modding.is_some(),
        ecosystem_game_id: None,
        active_profile_id: None,
        launch_supported,
        loader_deployed: !deployed.is_empty(),
        preloader_found: false,
        doorstop_version: None,
    };
    let Some(modding) = modding else {
        return Ok(status);
    };
    status.ecosystem_game_id = Some(modding.ecosystem_game_id);
    if let Some(profile_id) = &modding.active_profile_id {
        let profile_dir = paths.profile_dir(game_id, profile_id)?;
        status.preloader_found = find_preloader(&profile_dir).is_ok();
        if status.preloader_found {
            status.doorstop_version =
                Some(doorstop_version(&profile_dir.join(DOORSTOP_VERSION_FILE)));
        }
    }
    status.active_profile_id = modding.active_profile_id;
    Ok(status)
}

fn find_preloader(profile_dir: &Path) -> Result<PathBuf, String> {
    let core = profile_dir.join("BepInEx").join("core");
    PRELOADERS
        .iter()
        .map(|name| core.join(name))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            "The active mod profile has no BepInEx loader. Install BepInExPack in the profile first."
                .to_owned()
        })
}

/// Major Doorstop version; a missing or unreadable file means version 3.
fn doorstop_version(path: &Path) -> u32 {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| content.trim().split('.').next()?.parse::<u32>().ok())
        .filter(|major| *major > 3)
        .unwrap_or(3)
}

fn enable_arguments(version: u32, preloader: &str, corlib: Option<&str>) -> Vec<String> {
    let (enable, target, corlib_flag) = if version >= 4 {
        (
            "--doorstop-enabled",
            "--doorstop-target-assembly",
            "--doorstop-mono-dll-search-path-override",
        )
    } else {
        (
            "--doorstop-enable",
            "--doorstop-target",
            "--doorstop-dll-search-override",
        )
    };
    let mut arguments = vec![
        enable.to_owned(),
        "true".to_owned(),
        target.to_owned(),
        preloader.to_owned(),
    ];
    if let Some(corlib) = corlib {
        arguments.push(corlib_flag.to_owned());
        arguments.push(corlib.to_owned());
    }
    arguments
}

fn disable_arguments(version: u32) -> Vec<String> {
    let flag = if version >= 4 {
        "--doorstop-enabled"
    } else {
        "--doorstop-enable"
    };
    vec![flag.to_owned(), "false".to_owned()]
}

fn path_argument(path: &Path, style: PathStyle) -> Result<String, String> {
    let path = dunce::canonicalize(path)
        .map_err(|error| format!("Could not resolve {}: {error}", path.display()))?;
    let path = path
        .to_str()
        .ok_or_else(|| "The mod profile path is not valid UTF-8".to_owned())?;
    Ok(match style {
        PathStyle::Native => path.to_owned(),
        PathStyle::Wine => wine_path(path),
    })
}

/// Windows path of a Unix path, as seen through the `Z:` drive of a Wine prefix.
fn wine_path(unix_path: &str) -> String {
    format!("Z:{}", unix_path.replace('/', "\\"))
}

fn game_directory(game: &Game) -> Result<PathBuf, String> {
    let directory = match (&game.steam_install_path, &game.executable_path) {
        (Some(install), _) => PathBuf::from(install),
        (None, Some(executable)) => Path::new(executable)
            .parent()
            .ok_or_else(|| "Game executable has no parent directory".to_owned())?
            .to_path_buf(),
        (None, None) => return Err("This game has no installation folder".to_owned()),
    };
    let directory = dunce::canonicalize(&directory)
        .map_err(|error| format!("Could not inspect the game folder: {error}"))?;
    if !directory.is_dir() {
        return Err("The game folder is not a directory".to_owned());
    }
    Ok(directory)
}

#[cfg(windows)]
fn path_style(_game: &Game) -> Option<PathStyle> {
    Some(PathStyle::Native)
}

// Native Linux builds need the BepInEx shell wrapper, which Legio does not run.
#[cfg(target_os = "linux")]
fn path_style(game: &Game) -> Option<PathStyle> {
    if game.steam_install_path.is_some() {
        return steam_prefix(game).map(|_| PathStyle::Wine);
    }
    game.executable_path
        .as_deref()
        .and_then(|path| Path::new(path).extension())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        .then_some(PathStyle::Wine)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn path_style(_game: &Game) -> Option<PathStyle> {
    None
}

/// Proton prefix of a Steam game: `<library>/steamapps/compatdata/<appid>/pfx`.
#[cfg(target_os = "linux")]
fn steam_prefix(game: &Game) -> Option<PathBuf> {
    let app_id = game.steam_app_id?;
    let install = Path::new(game.steam_install_path.as_deref()?);
    let prefix = install
        .parent()?
        .parent()?
        .join("compatdata")
        .join(app_id.to_string())
        .join("pfx");
    prefix.is_dir().then_some(prefix)
}

#[cfg(target_os = "linux")]
fn patch_steam_prefix(game: &Game) -> Result<(), String> {
    let prefix = steam_prefix(game).ok_or_else(|| {
        "The Proton prefix of this game was not found. Run the game once from Steam first."
            .to_owned()
    })?;
    let now = u64::try_from(crate::database::now_milliseconds()? / 1000).unwrap_or_default();
    super::wine_registry::patch_user_reg(&prefix.join("user.reg"), PROXY_DLL, "native,builtin", now)
}

fn root_files(profile_dir: &Path) -> Result<Vec<String>, String> {
    let entries = fs::read_dir(profile_dir)
        .map_err(|error| format!("Could not read the mod profile folder: {error}"))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Could not read the mod profile folder: {error}"))?;
        // file_type does not follow symlinks, so links are never deployed.
        let is_file = entry
            .file_type()
            .map_err(|error| format!("Could not inspect the mod profile folder: {error}"))?
            .is_file();
        if !is_file {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "A mod profile file has a name that is not valid UTF-8".to_owned())?;
        if !UNDEPLOYED_ROOT_FILES
            .iter()
            .any(|excluded| name.eq_ignore_ascii_case(excluded))
        {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// Joins a recorded relative path onto the game folder, rejecting anything that could leave it.
fn game_file(game_dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    let mut components = path.components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(format!("\"{relative}\" is not a file in the game folder"));
    }
    Ok(game_dir.join(path))
}

fn deploy_root_files(
    database: &Database,
    game_id: &str,
    profile_dir: &Path,
    game_dir: &Path,
) -> Result<(), String> {
    let wanted: BTreeSet<String> = root_files(profile_dir)?.into_iter().collect();
    let previous: BTreeSet<String> = database.deployed_files(game_id)?.into_iter().collect();
    for name in &wanted {
        let target = game_file(game_dir, name)?;
        if !previous.contains(name) && fs::symlink_metadata(&target).is_ok() {
            return Err(format!(
                "{name} already exists in the game folder and was not installed by Legio. \
                 Move or remove it to launch with mods."
            ));
        }
    }
    if previous != wanted {
        // Recorded before copying so an interrupted deploy can still be restored.
        let recorded: Vec<String> = previous.union(&wanted).cloned().collect();
        database.replace_deployed_files(game_id, &recorded)?;
    }
    for name in &wanted {
        copy_if_different(&profile_dir.join(name), &game_file(game_dir, name)?)?;
    }
    if previous != wanted {
        for name in previous.difference(&wanted) {
            remove_deployed_file(&game_file(game_dir, name)?)?;
        }
        let recorded: Vec<String> = wanted.into_iter().collect();
        database.replace_deployed_files(game_id, &recorded)?;
    }
    Ok(())
}

fn copy_if_different(source: &Path, target: &Path) -> Result<(), String> {
    let describe = |error: std::io::Error| {
        format!(
            "Could not copy {} to the game folder: {error}. Close the game and try again.",
            source.display()
        )
    };
    let source_metadata = fs::metadata(source).map_err(describe)?;
    if let Ok(target_metadata) = fs::metadata(target)
        && target_metadata.len() == source_metadata.len()
        && target_metadata.modified().ok() == source_metadata.modified().ok()
    {
        return Ok(());
    }
    fs::copy(source, target).map_err(describe)?;
    // Matching mtimes are how an unchanged file is recognized on the next launch.
    let modified = source_metadata.modified().map_err(describe)?;
    fs::File::options()
        .write(true)
        .open(target)
        .and_then(|file| file.set_modified(modified))
        .map_err(describe)
}

fn remove_deployed_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Could not remove {} from the game folder: {error}",
            path.display()
        )),
    }
}

fn restore_game_folder(database: &Database, game_id: &str, game_dir: &Path) -> Result<(), String> {
    let mut failures = Vec::new();
    let mut remaining = Vec::new();
    for relative in database.deployed_files(game_id)? {
        if let Err(error) =
            game_file(game_dir, &relative).and_then(|path| remove_deployed_file(&path))
        {
            failures.push(error);
            remaining.push(relative);
        }
    }
    if failures.is_empty() {
        return database.clear_deployed_files(game_id);
    }
    database.replace_deployed_files(game_id, &remaining)?;
    Err(failures.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::CreateGameInput;
    use std::time::{Duration, SystemTime};

    struct Fixture {
        directory: PathBuf,
        database: Database,
        paths: ModdingPaths,
        game_id: String,
        profile_id: String,
        game_dir: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let directory =
                std::env::temp_dir().join(format!("legio-launch-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&directory).unwrap();
            let database = Database::open(&directory).unwrap();
            let game = database
                .create_game(CreateGameInput {
                    name: "Game".to_owned(),
                    steam_app_id: None,
                })
                .unwrap();
            let paths = ModdingPaths::new(&directory);
            let profile = database.create_mod_profile(&game.id, "Main", 1).unwrap();
            let game_dir = directory.join("game");
            fs::create_dir_all(&game_dir).unwrap();
            Self {
                directory,
                database,
                paths,
                game_id: game.id,
                profile_id: profile.id,
                game_dir,
            }
        }

        fn bind(&self) {
            self.database
                .set_game_modding(&self.game_id, "lethal-company")
                .unwrap();
        }

        fn activate(&self) {
            self.bind();
            self.database
                .set_active_mod_profile(&self.game_id, Some(&self.profile_id))
                .unwrap();
        }

        fn profile_dir(&self) -> PathBuf {
            self.paths
                .profile_dir(&self.game_id, &self.profile_id)
                .unwrap()
        }

        fn install_loader(&self, preloader: &str, doorstop_version: Option<&str>) {
            let core = self.profile_dir().join("BepInEx").join("core");
            fs::create_dir_all(&core).unwrap();
            fs::write(core.join(preloader), b"preloader").unwrap();
            fs::write(self.profile_dir().join("winhttp.dll"), b"proxy").unwrap();
            fs::write(self.profile_dir().join("doorstop_config.ini"), b"ini").unwrap();
            if let Some(version) = doorstop_version {
                fs::write(self.profile_dir().join(DOORSTOP_VERSION_FILE), version).unwrap();
            }
        }

        fn prepare(&self, style: PathStyle) -> Result<Option<ModLaunchPlan>, String> {
            prepare(
                &self.database,
                &self.paths,
                &self.game_id,
                &self.game_dir,
                style,
            )
        }

        fn preloader(&self, name: &str) -> String {
            path_argument(
                &self.profile_dir().join("BepInEx").join("core").join(name),
                PathStyle::Native,
            )
            .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn unbound_games_launch_as_usual() {
        let fixture = Fixture::new();
        assert_eq!(fixture.prepare(PathStyle::Native).unwrap(), None);
        fixture
            .database
            .replace_deployed_files(&fixture.game_id, &strings(&["winhttp.dll"]))
            .unwrap();
        assert_eq!(fixture.prepare(PathStyle::Native).unwrap(), None);
    }

    #[test]
    fn bound_vanilla_game_without_deployed_files_launches_as_usual() {
        let fixture = Fixture::new();
        fixture.bind();
        assert_eq!(fixture.prepare(PathStyle::Native).unwrap(), None);
    }

    #[test]
    fn doorstop_v3_is_the_default() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Preloader.dll", None);
        let plan = fixture.prepare(PathStyle::Native).unwrap().unwrap();
        assert_eq!(
            plan.arguments,
            [
                "--doorstop-enable",
                "true",
                "--doorstop-target",
                fixture.preloader("BepInEx.Preloader.dll").as_str(),
            ]
        );
        assert!(!plan.wine_dll_override);
    }

    #[test]
    fn doorstop_v4_uses_its_own_arguments() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Unity.Mono.Preloader.dll", Some("4.3.0\n"));
        let plan = fixture.prepare(PathStyle::Native).unwrap().unwrap();
        assert_eq!(
            plan.arguments,
            [
                "--doorstop-enabled",
                "true",
                "--doorstop-target-assembly",
                fixture
                    .preloader("BepInEx.Unity.Mono.Preloader.dll")
                    .as_str(),
            ]
        );
    }

    #[test]
    fn unparsable_or_old_doorstop_versions_mean_v3() {
        let directory =
            std::env::temp_dir().join(format!("legio-doorstop-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let file = directory.join(DOORSTOP_VERSION_FILE);
        assert_eq!(doorstop_version(&file), 3);
        for (content, expected) in [
            ("3.4.0", 3),
            ("2.1", 3),
            ("", 3),
            ("abc", 3),
            ("4", 4),
            ("5.0.1", 5),
        ] {
            fs::write(&file, content).unwrap();
            assert_eq!(doorstop_version(&file), expected, "{content:?}");
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn preloader_follows_the_documented_order() {
        let fixture = Fixture::new();
        let core = fixture.profile_dir().join("BepInEx").join("core");
        fs::create_dir_all(&core).unwrap();
        assert!(find_preloader(&fixture.profile_dir()).is_err());
        fs::write(core.join("BepInEx.NET.CoreCLR.dll"), b"x").unwrap();
        fs::write(core.join("BepInEx.IL2CPP.dll"), b"x").unwrap();
        assert!(
            find_preloader(&fixture.profile_dir())
                .unwrap()
                .ends_with("BepInEx.IL2CPP.dll")
        );
        fs::write(core.join("BepInEx.Unity.IL2CPP.dll"), b"x").unwrap();
        assert!(
            find_preloader(&fixture.profile_dir())
                .unwrap()
                .ends_with("BepInEx.Unity.IL2CPP.dll")
        );
    }

    #[test]
    fn active_profile_without_loader_is_a_clear_error() {
        let fixture = Fixture::new();
        fixture.activate();
        let error = fixture.prepare(PathStyle::Native).unwrap_err();
        assert!(error.contains("Install BepInExPack"), "{error}");
        assert!(fs::read_dir(&fixture.game_dir).unwrap().next().is_none());
    }

    #[test]
    fn wine_paths_use_the_z_drive_with_backslashes() {
        assert_eq!(
            wine_path("/home/user/.local/share/legio/BepInEx/core/BepInEx.Preloader.dll"),
            "Z:\\home\\user\\.local\\share\\legio\\BepInEx\\core\\BepInEx.Preloader.dll"
        );
    }

    #[test]
    fn wine_launch_requests_the_dll_override_and_corlib() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Preloader.dll", None);
        fs::create_dir(fixture.profile_dir().join(CORLIB_DIRECTORY)).unwrap();
        let plan = fixture.prepare(PathStyle::Wine).unwrap().unwrap();
        assert!(plan.wine_dll_override);
        assert!(plan.arguments[3].starts_with("Z:"));
        assert!(plan.arguments[3].ends_with("BepInEx.Preloader.dll"));
        assert_eq!(plan.arguments[4], "--doorstop-dll-search-override");
        assert!(plan.arguments[5].starts_with("Z:"));
        assert!(plan.arguments[5].ends_with(CORLIB_DIRECTORY));
    }

    #[test]
    fn native_launch_ignores_the_corlib_directory() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Preloader.dll", None);
        fs::create_dir(fixture.profile_dir().join(CORLIB_DIRECTORY)).unwrap();
        assert_eq!(
            fixture
                .prepare(PathStyle::Native)
                .unwrap()
                .unwrap()
                .arguments
                .len(),
            4
        );
    }

    #[test]
    fn corlib_flag_depends_on_the_doorstop_version() {
        assert_eq!(
            enable_arguments(4, "p", Some("c"))[4..],
            ["--doorstop-mono-dll-search-path-override", "c"]
        );
        assert_eq!(
            enable_arguments(3, "p", Some("c"))[4..],
            ["--doorstop-dll-search-override", "c"]
        );
    }

    #[test]
    fn vanilla_launch_disables_doorstop_with_the_deployed_version() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Unity.Mono.Preloader.dll", Some("4.0.0"));
        fixture.prepare(PathStyle::Native).unwrap();
        fixture
            .database
            .set_active_mod_profile(&fixture.game_id, None)
            .unwrap();
        let plan = fixture.prepare(PathStyle::Native).unwrap().unwrap();
        assert_eq!(plan.arguments, ["--doorstop-enabled", "false"]);
        assert!(!plan.wine_dll_override);

        fs::remove_file(fixture.game_dir.join(DOORSTOP_VERSION_FILE)).unwrap();
        fixture
            .database
            .replace_deployed_files(&fixture.game_id, &strings(&["winhttp.dll"]))
            .unwrap();
        let plan = fixture.prepare(PathStyle::Native).unwrap().unwrap();
        assert_eq!(plan.arguments, ["--doorstop-enable", "false"]);
    }

    #[test]
    fn deploys_root_files_and_records_them() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Preloader.dll", Some("4.0.0"));
        fs::write(fixture.profile_dir().join("mods.yml"), b"x").unwrap();
        fs::write(fixture.profile_dir().join("manifest.json"), b"x").unwrap();
        fixture.prepare(PathStyle::Native).unwrap();
        let mut deployed: Vec<_> = fs::read_dir(&fixture.game_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        deployed.sort();
        assert_eq!(
            deployed,
            [".doorstop_version", "doorstop_config.ini", "winhttp.dll"]
        );
        assert_eq!(
            fixture.database.deployed_files(&fixture.game_id).unwrap(),
            deployed
        );
        assert!(!fixture.game_dir.join("BepInEx").exists());
    }

    #[test]
    fn copies_only_changed_files() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Preloader.dll", None);
        fixture.prepare(PathStyle::Native).unwrap();
        let target = fixture.game_dir.join("winhttp.dll");
        let source = fixture.profile_dir().join("winhttp.dll");
        assert_eq!(
            fs::metadata(&target).unwrap().modified().unwrap(),
            fs::metadata(&source).unwrap().modified().unwrap()
        );

        // Same size and mtime as the source: a rewrite would be undone by a copy.
        let source_time = fs::metadata(&source).unwrap().modified().unwrap();
        fs::write(&target, b"PROXY").unwrap();
        fs::File::options()
            .write(true)
            .open(&target)
            .unwrap()
            .set_modified(source_time)
            .unwrap();
        fixture.prepare(PathStyle::Native).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"PROXY");

        let later = SystemTime::now() + Duration::from_secs(60);
        fs::write(&source, b"proxy v2").unwrap();
        fs::File::options()
            .write(true)
            .open(&source)
            .unwrap()
            .set_modified(later)
            .unwrap();
        fixture.prepare(PathStyle::Native).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"proxy v2");
    }

    #[test]
    fn never_overwrites_a_game_file_legio_did_not_deploy() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Preloader.dll", None);
        fs::write(fixture.game_dir.join("winhttp.dll"), b"user file").unwrap();
        let error = fixture.prepare(PathStyle::Native).unwrap_err();
        assert!(error.contains("winhttp.dll already exists"), "{error}");
        assert_eq!(
            fs::read(fixture.game_dir.join("winhttp.dll")).unwrap(),
            b"user file"
        );
        assert!(!fixture.game_dir.join("doorstop_config.ini").exists());
        assert!(
            fixture
                .database
                .deployed_files(&fixture.game_id)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn removes_deployed_files_the_profile_no_longer_has() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Preloader.dll", None);
        fixture.prepare(PathStyle::Native).unwrap();
        fs::remove_file(fixture.profile_dir().join("doorstop_config.ini")).unwrap();
        fixture.prepare(PathStyle::Native).unwrap();
        assert!(!fixture.game_dir.join("doorstop_config.ini").exists());
        assert_eq!(
            fixture.database.deployed_files(&fixture.game_id).unwrap(),
            ["winhttp.dll"]
        );
    }

    #[test]
    fn restore_deletes_recorded_files_only() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Preloader.dll", None);
        fixture.prepare(PathStyle::Native).unwrap();
        fs::write(fixture.game_dir.join("game.exe"), b"exe").unwrap();
        fs::remove_file(fixture.game_dir.join("doorstop_config.ini")).unwrap();

        restore_game_folder(&fixture.database, &fixture.game_id, &fixture.game_dir).unwrap();
        assert!(!fixture.game_dir.join("winhttp.dll").exists());
        assert!(fixture.game_dir.join("game.exe").exists());
        assert!(
            fixture
                .database
                .deployed_files(&fixture.game_id)
                .unwrap()
                .is_empty()
        );
        restore_game_folder(&fixture.database, &fixture.game_id, &fixture.game_dir).unwrap();
    }

    #[test]
    fn rejects_paths_that_leave_the_game_folder() {
        let base = Path::new("game");
        for relative in ["", "..", "../x", "a/b", "/etc/passwd", "."] {
            assert!(game_file(base, relative).is_err(), "{relative:?}");
        }
        assert_eq!(
            game_file(base, "winhttp.dll").unwrap(),
            base.join("winhttp.dll")
        );
    }

    #[test]
    fn status_reports_loader_state() {
        let fixture = Fixture::new();
        let status = |supported| {
            status(
                &fixture.database,
                &fixture.paths,
                &fixture.game_id,
                supported,
            )
            .unwrap()
        };
        let unbound = status(true);
        assert!(!unbound.bound && !unbound.loader_deployed && !unbound.preloader_found);

        fixture.activate();
        let empty = status(true);
        assert!(empty.bound && !empty.preloader_found);
        assert_eq!(empty.doorstop_version, None);
        assert_eq!(
            empty.active_profile_id.as_deref(),
            Some(fixture.profile_id.as_str())
        );

        fixture.install_loader("BepInEx.Preloader.dll", Some("4.0.0"));
        fixture.prepare(PathStyle::Native).unwrap();
        let ready = status(false);
        assert!(ready.loader_deployed && ready.preloader_found && !ready.launch_supported);
        assert_eq!(ready.doorstop_version, Some(4));
        assert_eq!(ready.ecosystem_game_id.as_deref(), Some("lethal-company"));
    }
}
