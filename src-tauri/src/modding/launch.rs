//! Launching a game with its active mod profile (BepInEx via Unity Doorstop).
//!
//! Ported from r2modmanPlus (MIT): `BepInExGameInstructions`, `UnityDoorstopUtils`,
//! `GameInstructionParser` and `ModLinker`. The loader root files of the profile
//! are copied next to the game executable and Doorstop is steered with arguments.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use super::ecosystem_store::EcosystemStore;
use super::installer::GameRules;
use super::paths::{ModdingPaths, validate_relative_path};
use super::profiles::ProfileLocks;
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
const PROFILE_BUSY: &str =
    "Mods are being changed in the active mod profile. Wait for it to finish and launch again.";
#[cfg(target_os = "linux")]
const PROXY_DLL: &str = "winhttp";

/// How the preloader path is written for the process that reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathStyle {
    // Linux games always run through Wine; native launches are Windows only.
    #[cfg_attr(not(windows), allow(dead_code))]
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
/// Fails while another operation changes the active mod profile.
pub(crate) fn prepare_launch<R: Runtime>(
    app: &AppHandle<R>,
    game: &Game,
) -> Result<Option<ModLaunchPlan>, String> {
    let state = app.state::<DatabaseState>();
    let database = state.database()?;
    let Some(modding) = database.game_modding(&game.id)? else {
        return Ok(None);
    };
    let Some(style) = path_style(game) else {
        return Ok(None);
    };
    let paths = ModdingPaths::new(&data_dir(app)?);
    let game_dir = game_directory(game)?;
    let plan = match modding.active_profile_id.as_deref() {
        None => prepare(database, &paths, &game.id, &game_dir, style, &|_| false)?,
        Some(profile_id) => {
            let locks = app.state::<ProfileLocks>();
            let _profile_guard = locks.try_lock(profile_id)?.ok_or(PROFILE_BUSY)?;
            // The rules only name the loader package, so a stale schema is fine.
            let schema =
                tauri::async_runtime::block_on(app.state::<EcosystemStore>().cached_schema())?;
            let rules = GameRules::from_schema(&schema, &modding.ecosystem_game_id)?;
            let ctx = rules.context(database, &paths, &locks);
            prepare(database, &paths, &game.id, &game_dir, style, &|name| {
                ctx.find_loader(name).is_some()
            })?
        }
    };
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

/// Deletes the loader files of a game that is about to leave the library,
/// before the records that name them are dropped. A failure is logged and
/// does not stop the removal.
pub(crate) fn restore_before_removal<R: Runtime>(app: &AppHandle<R>, game_id: &str) {
    let outcome = app
        .state::<DatabaseState>()
        .database()
        .and_then(|database| restore_removed_game(database, game_id));
    if let Err(error) = outcome {
        crate::application_log::failure("restore_removed_game_folder", error);
    }
}

fn restore_removed_game(database: &Database, game_id: &str) -> Result<(), String> {
    if database.deployed_files(game_id)?.is_empty() {
        return Ok(());
    }
    let game = database.game(game_id)?;
    restore_game_folder(database, game_id, &game_directory(&game)?)
}

fn data_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("Could not resolve application data directory: {error}"))
}

/// `is_loader` tells whether a mod (`Owner-Name`) is the game's mod loader.
fn prepare(
    database: &Database,
    paths: &ModdingPaths,
    game_id: &str,
    game_dir: &Path,
    style: PathStyle,
    is_loader: &dyn Fn(&str) -> bool,
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
    let loader_files = loader_root_files(database, &profile_id, is_loader)?;
    deploy_root_files(database, game_id, &profile_dir, game_dir, &loader_files)?;
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

pub(crate) fn status(
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

/// True when Legio can launch the game with mods.
pub(crate) fn can_launch_with_mods(game: &Game) -> bool {
    path_style(game).is_some()
}

/// How the game launches with mods, for support reports.
pub(crate) fn launch_mode(game: &Game) -> &'static str {
    match path_style(game) {
        Some(PathStyle::Native) => "native",
        Some(PathStyle::Wine) => "wine/proton",
        None => "not supported",
    }
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

/// The files at the profile root that the installed loader package tracks.
/// Only these are copied next to the game executable: other root files of
/// the profile (a shared profile could hold any) stay where they are.
fn loader_root_files(
    database: &Database,
    profile_id: &str,
    is_loader: &dyn Fn(&str) -> bool,
) -> Result<BTreeSet<String>, String> {
    let mut files = BTreeSet::new();
    for record in database.profile_mods(profile_id)? {
        if record.enabled && is_loader(&record.full_name) {
            files.extend(
                database
                    .profile_mod_files(profile_id, &record.full_name)?
                    .into_iter()
                    .filter(|path| !path.contains('/')),
            );
        }
    }
    Ok(files)
}

/// Joins a recorded file name onto the game folder, rejecting anything that could leave it.
fn game_file(game_dir: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.contains('/') || validate_relative_path(relative).is_err() {
        return Err(format!("\"{relative}\" is not a file in the game folder"));
    }
    Ok(game_dir.join(relative))
}

fn deploy_root_files(
    database: &Database,
    game_id: &str,
    profile_dir: &Path,
    game_dir: &Path,
    wanted: &BTreeSet<String>,
) -> Result<(), String> {
    let previous: BTreeSet<String> = database.deployed_files(game_id)?.into_iter().collect();
    for name in wanted {
        let target = game_file(game_dir, name)?;
        if !previous.contains(name) && fs::symlink_metadata(&target).is_ok() {
            return Err(format!(
                "{name} already exists in the game folder and was not installed by Legio."
            ));
        }
    }
    if previous != *wanted {
        // Recorded before copying so an interrupted deploy can still be restored.
        let recorded: Vec<String> = previous.union(wanted).cloned().collect();
        database.replace_deployed_files(game_id, &recorded)?;
    }
    for name in wanted {
        copy_if_different(&profile_dir.join(name), &game_file(game_dir, name)?)?;
    }
    if previous != *wanted {
        for name in previous.difference(wanted) {
            remove_deployed_file(&game_file(game_dir, name)?)?;
        }
        let recorded: Vec<String> = wanted.iter().cloned().collect();
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
    use crate::database::{CreateGameInput, ProfileMod};
    use std::time::{Duration, SystemTime};

    const LOADER: &str = "BepInEx-BepInExPack";

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
            let mut tracked = vec![
                format!("BepInEx/core/{preloader}"),
                "winhttp.dll".to_owned(),
                "doorstop_config.ini".to_owned(),
            ];
            if let Some(version) = doorstop_version {
                fs::write(self.profile_dir().join(DOORSTOP_VERSION_FILE), version).unwrap();
                tracked.push(DOORSTOP_VERSION_FILE.to_owned());
            }
            self.record_mod(LOADER, true, &tracked);
        }

        fn record_mod(&self, full_name: &str, enabled: bool, tracked: &[String]) {
            self.database
                .upsert_profile_mod(&ProfileMod {
                    profile_id: self.profile_id.clone(),
                    full_name: full_name.to_owned(),
                    version: "1.0.0".to_owned(),
                    enabled,
                    online_source: true,
                    installed_at: 1,
                    install_order: 0,
                })
                .unwrap();
            self.database
                .replace_profile_mod_files(&self.profile_id, full_name, tracked)
                .unwrap();
        }

        fn prepare(&self, style: PathStyle) -> Result<Option<ModLaunchPlan>, String> {
            prepare(
                &self.database,
                &self.paths,
                &self.game_id,
                &self.game_dir,
                style,
                &|name| name == LOADER,
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
    fn deploys_only_the_files_the_loader_tracks() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Preloader.dll", None);
        for stray in ["steam_appid.txt", "boot.ini", "mods.yml", "manifest.json"] {
            fs::write(fixture.profile_dir().join(stray), b"stray").unwrap();
        }
        // A mod that is not the loader cannot put files next to the executable.
        fs::write(fixture.profile_dir().join("extra.dll"), b"extra").unwrap();
        fixture.record_mod("Owner-Mod", true, &["extra.dll".to_owned()]);

        fixture.prepare(PathStyle::Native).unwrap();
        let mut deployed: Vec<_> = fs::read_dir(&fixture.game_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        deployed.sort();
        assert_eq!(deployed, ["doorstop_config.ini", "winhttp.dll"]);
        assert_eq!(
            fixture.database.deployed_files(&fixture.game_id).unwrap(),
            deployed
        );
    }

    #[test]
    fn a_disabled_loader_deploys_nothing() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Preloader.dll", None);
        fixture.record_mod(LOADER, false, &["winhttp.dll".to_owned()]);
        fixture.prepare(PathStyle::Native).unwrap();
        assert!(fs::read_dir(&fixture.game_dir).unwrap().next().is_none());
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
        fixture.record_mod(
            LOADER,
            true,
            &[
                "BepInEx/core/BepInEx.Preloader.dll".to_owned(),
                "winhttp.dll".to_owned(),
            ],
        );
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
    fn removal_deletes_the_deployed_loader_files() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Preloader.dll", None);
        fixture.prepare(PathStyle::Native).unwrap();
        let executable = fixture.game_dir.join("game.exe");
        fs::write(&executable, b"exe").unwrap();
        fixture
            .database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE games SET executable_path = ?2 WHERE id = ?1",
                        rusqlite::params![fixture.game_id, executable.to_str().unwrap()],
                    )
                    .map_err(|error| error.to_string())
            })
            .unwrap();

        restore_removed_game(&fixture.database, &fixture.game_id).unwrap();
        assert!(!fixture.game_dir.join("winhttp.dll").exists());
        assert!(!fixture.game_dir.join("doorstop_config.ini").exists());
        assert!(executable.exists());
        assert!(
            fixture
                .database
                .deployed_files(&fixture.game_id)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn removal_needs_no_game_folder_without_deployed_files() {
        let fixture = Fixture::new();
        fixture.activate();
        // The game has no installation folder at all.
        restore_removed_game(&fixture.database, &fixture.game_id).unwrap();

        fixture
            .database
            .replace_deployed_files(&fixture.game_id, &strings(&["winhttp.dll"]))
            .unwrap();
        assert!(restore_removed_game(&fixture.database, &fixture.game_id).is_err());
    }

    #[test]
    fn rejects_paths_that_leave_the_game_folder() {
        let base = Path::new("game");
        for relative in ["", "..", "../x", "a/b", "/etc/passwd", ".", "a:b", "a\\b"] {
            assert!(game_file(base, relative).is_err(), "{relative:?}");
        }
        assert_eq!(
            game_file(base, "winhttp.dll").unwrap(),
            base.join("winhttp.dll")
        );
    }

    #[test]
    fn instances_of_one_game_do_not_share_loader_state() {
        let fixture = Fixture::new();
        fixture.activate();
        fixture.install_loader("BepInEx.Preloader.dll", None);
        let other = fixture
            .database
            .create_game(CreateGameInput {
                name: "Game".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        fixture
            .database
            .set_game_modding(&other.id, "lethal-company")
            .unwrap();
        let other_dir = fixture.directory.join("other");
        fs::create_dir_all(&other_dir).unwrap();
        let launch_other = || {
            prepare(
                &fixture.database,
                &fixture.paths,
                &other.id,
                &other_dir,
                PathStyle::Native,
                &|name| name == LOADER,
            )
        };

        fixture.prepare(PathStyle::Native).unwrap().unwrap();
        assert!(fixture.game_dir.join("winhttp.dll").exists());
        assert_eq!(launch_other().unwrap(), None);
        assert_eq!(fs::read_dir(&other_dir).unwrap().count(), 0);
        assert!(fixture.database.mod_profiles(&other.id).unwrap().is_empty());
        assert!(
            fixture
                .database
                .set_active_mod_profile(&other.id, Some(&fixture.profile_id))
                .is_err()
        );

        let foreign = fixture
            .database
            .create_mod_profile(&other.id, "Main", 2)
            .unwrap();
        assert_ne!(
            fixture.paths.profile_dir(&other.id, &foreign.id).unwrap(),
            fixture.profile_dir()
        );
        fixture
            .database
            .set_active_mod_profile(&other.id, Some(&foreign.id))
            .unwrap();
        // The profile of the first instance has a loader, the new one has none.
        assert!(launch_other().is_err());
        assert_eq!(fs::read_dir(&other_dir).unwrap().count(), 0);
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
