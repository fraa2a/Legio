use super::{
    Database, EffectiveCompatibilityConfig, LinuxPerformance, NativeLaunchConfig,
    SteamLaunchConfig, database_error, parse_game_id,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

pub(crate) const MAX_PROFILE_NAME_CHARS: usize = 48;
const MAX_PROFILES_PER_GAME: usize = 12;

/// Launch settings a profile replaces. `None` inherits the game's base settings;
/// a present value replaces the base value as a whole, so an empty list or map
/// clears it. Mod profile selection is not part of a launch profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct LaunchOverrides {
    /// Arguments after the executable (Steam and native launches: all arguments).
    pub arguments: Option<Vec<String>>,
    pub environment: Option<BTreeMap<String, String>>,
    /// Compatibility runner for Linux launches of Windows games.
    pub runner_path: Option<String>,
    pub linux_performance: Option<LinuxPerformance>,
}

impl LaunchOverrides {
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) fn resolve_compatibility(
        &self,
        mut config: EffectiveCompatibilityConfig,
    ) -> EffectiveCompatibilityConfig {
        if let Some(arguments) = &self.arguments {
            config.arguments_after.clone_from(arguments);
        }
        if let Some(environment) = &self.environment {
            config.environment.clone_from(environment);
        }
        if let Some(runner_path) = &self.runner_path {
            config.runner_path = Some(runner_path.clone());
        }
        if let Some(performance) = &self.linux_performance {
            config.linux_performance.clone_from(performance);
        }
        config
    }

    pub(crate) fn resolve_native(&self, mut config: NativeLaunchConfig) -> NativeLaunchConfig {
        if let Some(arguments) = &self.arguments {
            config.arguments.clone_from(arguments);
        }
        if let Some(environment) = &self.environment {
            config.environment.clone_from(environment);
        }
        config
    }

    pub(crate) fn resolve_steam(&self, mut config: SteamLaunchConfig) -> SteamLaunchConfig {
        if let Some(arguments) = &self.arguments {
            config.arguments.clone_from(arguments);
        }
        config
    }

    fn validate(&self, steam_game: bool) -> Result<(), String> {
        if steam_game
            && (self.environment.is_some()
                || self.runner_path.is_some()
                || self.linux_performance.is_some())
        {
            return Err("Profiles of Steam games can only override launch arguments".to_owned());
        }
        if let Some(arguments) = &self.arguments {
            crate::game_lifecycle::validate_launch_arguments(arguments)?;
        }
        if let Some(environment) = &self.environment {
            validate_environment(environment)?;
        }
        if let Some(runner_path) = &self.runner_path
            && (runner_path.is_empty() || runner_path.contains('\0'))
        {
            return Err("Profile runner path is invalid".to_owned());
        }
        if let Some(performance) = &self.linux_performance {
            performance.validate()?;
            #[cfg(not(target_os = "linux"))]
            if *performance != LinuxPerformance::default() {
                return Err("Linux performance options are available on Linux only".to_owned());
            }
        }
        #[cfg(not(target_os = "linux"))]
        if self.runner_path.is_some() {
            return Err("Compatibility runners are available on Linux only".to_owned());
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn validate_environment(environment: &BTreeMap<String, String>) -> Result<(), String> {
    crate::game_lifecycle::validate_launch_environment(environment)
}

#[cfg(not(target_os = "linux"))]
fn validate_environment(environment: &BTreeMap<String, String>) -> Result<(), String> {
    crate::game_process::validate_environment_variables(environment, "Launch environment")
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProfile {
    pub id: String,
    pub game_id: String,
    pub name: String,
    pub overrides: LaunchOverrides,
}

/// Every profile in creation order, and for each game the profile it launched last.
/// A game without an entry launches with its base settings.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProfiles {
    pub profiles: Vec<LaunchProfile>,
    pub last_used: BTreeMap<String, String>,
}

impl Database {
    pub fn launch_profiles(&self) -> Result<LaunchProfiles, String> {
        self.with_connection(load_profiles)
    }

    pub fn create_launch_profile(
        &self,
        game_id: &str,
        name: &str,
        overrides: LaunchOverrides,
    ) -> Result<LaunchProfiles, String> {
        let game_id = parse_game_id(game_id)?;
        let name = validate_name(name)?;
        overrides.validate(self.game(&game_id)?.steam_install_path.is_some())?;
        let encoded = encode(&overrides)?;
        self.with_connection(|connection| {
            let transaction = connection.unchecked_transaction().map_err(database_error)?;
            let existing = game_profile_names(&transaction, &game_id)?;
            if existing.len() >= MAX_PROFILES_PER_GAME {
                return Err(format!(
                    "A game can have at most {MAX_PROFILES_PER_GAME} launch profiles"
                ));
            }
            ensure_unique_name(&existing, &name, None)?;
            transaction
                .execute(
                    "INSERT INTO launch_profiles (id, game_id, name, position, overrides)
                     SELECT ?1, ?2, ?3, COALESCE(MAX(position) + 1, 0), ?4
                     FROM launch_profiles WHERE game_id = ?2",
                    params![Uuid::new_v4().to_string(), game_id, name, encoded],
                )
                .map_err(database_error)?;
            transaction.commit().map_err(database_error)?;
            load_profiles(connection)
        })
    }

    /// Replaces the name and overrides of a profile.
    pub fn update_launch_profile(
        &self,
        id: &str,
        name: &str,
        overrides: LaunchOverrides,
    ) -> Result<LaunchProfiles, String> {
        let id = parse_profile_id(id)?;
        let name = validate_name(name)?;
        let game_id = self.with_connection(|connection| profile_game(connection, &id))?;
        overrides.validate(self.game(&game_id)?.steam_install_path.is_some())?;
        let encoded = encode(&overrides)?;
        self.with_connection(|connection| {
            let transaction = connection.unchecked_transaction().map_err(database_error)?;
            ensure_unique_name(
                &game_profile_names(&transaction, &game_id)?,
                &name,
                Some(&id),
            )?;
            transaction
                .execute(
                    "UPDATE launch_profiles SET name = ?2, overrides = ?3 WHERE id = ?1",
                    params![id, name, encoded],
                )
                .map_err(database_error)?;
            transaction.commit().map_err(database_error)?;
            load_profiles(connection)
        })
    }

    pub fn delete_launch_profile(&self, id: &str) -> Result<LaunchProfiles, String> {
        let id = parse_profile_id(id)?;
        self.with_connection(|connection| {
            // The last-used selection of a deleted profile goes with it.
            let changed = connection
                .execute("DELETE FROM launch_profiles WHERE id = ?1", [&id])
                .map_err(database_error)?;
            if changed == 0 {
                return Err("launch profile was not found".to_owned());
            }
            load_profiles(connection)
        })
    }

    /// Copies the overrides of a profile into a new one named `name`.
    pub fn duplicate_launch_profile(&self, id: &str, name: &str) -> Result<LaunchProfiles, String> {
        let id = parse_profile_id(id)?;
        let source = self.with_connection(|connection| {
            load_profiles(connection)?
                .profiles
                .into_iter()
                .find(|profile| profile.id == id)
                .ok_or_else(|| "launch profile was not found".to_owned())
        })?;
        self.create_launch_profile(&source.game_id, name, source.overrides)
    }

    /// The profile a launch asks for. `None` selects the base settings; an id
    /// that is unknown or belongs to another game is an error.
    pub fn launch_profile(
        &self,
        game_id: &str,
        profile_id: Option<&str>,
    ) -> Result<Option<LaunchProfile>, String> {
        let Some(profile_id) = profile_id else {
            return Ok(None);
        };
        let game_id = parse_game_id(game_id)?;
        let profile_id = parse_profile_id(profile_id)?;
        self.with_connection(|connection| {
            load_profiles(connection)?
                .profiles
                .into_iter()
                .find(|profile| profile.id == profile_id && profile.game_id == game_id)
                .map(Some)
                .ok_or_else(|| "launch profile was not found for this game".to_owned())
        })
    }

    /// Remembers which profile a game launched last; `None` is the base settings.
    pub fn set_last_launch_profile(
        &self,
        game_id: &str,
        profile_id: Option<&str>,
    ) -> Result<(), String> {
        let game_id = parse_game_id(game_id)?;
        let profile_id = profile_id.map(parse_profile_id).transpose()?;
        self.with_connection(|connection| {
            match profile_id {
                None => {
                    connection
                        .execute(
                            "DELETE FROM game_last_launch_profile WHERE game_id = ?1",
                            [&game_id],
                        )
                        .map_err(database_error)?;
                }
                Some(profile_id) => {
                    if profile_game(connection, &profile_id)? != game_id {
                        return Err("launch profile was not found for this game".to_owned());
                    }
                    connection
                        .execute(
                            "INSERT INTO game_last_launch_profile (game_id, profile_id)
                             VALUES (?1, ?2)
                             ON CONFLICT(game_id) DO UPDATE SET profile_id = excluded.profile_id",
                            params![game_id, profile_id],
                        )
                        .map_err(database_error)?;
                }
            }
            Ok(())
        })
    }
}

fn load_profiles(connection: &Connection) -> Result<LaunchProfiles, String> {
    let mut statement = connection
        .prepare("SELECT id, game_id, name, overrides FROM launch_profiles ORDER BY game_id, position, id")
        .map_err(database_error)?;
    let profiles = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(database_error)?
        .map(|row| {
            let (id, game_id, name, overrides) = row.map_err(database_error)?;
            Ok(LaunchProfile {
                id,
                game_id,
                name,
                overrides: serde_json::from_str(&overrides)
                    .map_err(|error| format!("stored launch profile is invalid: {error}"))?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut statement = connection
        .prepare("SELECT game_id, profile_id FROM game_last_launch_profile")
        .map_err(database_error)?;
    let last_used = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(database_error)?
        .collect::<Result<_, _>>()
        .map_err(database_error)?;
    Ok(LaunchProfiles {
        profiles,
        last_used,
    })
}

fn encode(overrides: &LaunchOverrides) -> Result<String, String> {
    serde_json::to_string(overrides)
        .map_err(|error| format!("could not encode launch profile: {error}"))
}

fn parse_profile_id(value: &str) -> Result<String, String> {
    Uuid::parse_str(value)
        .map(|id| id.to_string())
        .map_err(|_| "launch profile id is invalid".to_owned())
}

fn validate_name(value: &str) -> Result<String, String> {
    let name = value.trim();
    let length = name.chars().count();
    if length == 0 || length > MAX_PROFILE_NAME_CHARS {
        return Err(format!(
            "Launch profile name must be between 1 and {MAX_PROFILE_NAME_CHARS} characters"
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("Launch profile name cannot contain control characters".to_owned());
    }
    Ok(name.to_owned())
}

fn profile_game(connection: &Connection, id: &str) -> Result<String, String> {
    connection
        .query_row(
            "SELECT game_id FROM launch_profiles WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| "launch profile was not found".to_owned())
}

/// Ids and names of the profiles of one game.
fn game_profile_names(
    connection: &Connection,
    game_id: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut statement = connection
        .prepare("SELECT id, name FROM launch_profiles WHERE game_id = ?1")
        .map_err(database_error)?;
    statement
        .query_map([game_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(database_error)?
        .collect::<Result<_, _>>()
        .map_err(database_error)
}

fn ensure_unique_name(
    existing: &[(String, String)],
    name: &str,
    except: Option<&str>,
) -> Result<(), String> {
    let folded = name.to_lowercase();
    if existing
        .iter()
        .any(|(id, other)| Some(id.as_str()) != except && other.to_lowercase() == folded)
    {
        return Err("This game already has a launch profile with this name".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{CreateGameInput, GraphicsRenderer};
    use std::fs;

    struct Fixture(std::path::PathBuf);

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn open() -> (Fixture, Database) {
        let directory = std::env::temp_dir().join(format!("legio-profiles-{}", Uuid::new_v4()));
        let database = Database::open(&directory).unwrap();
        (Fixture(directory), database)
    }

    fn game(database: &Database, name: &str) -> String {
        database
            .create_game(CreateGameInput {
                name: name.to_owned(),
                steam_app_id: None,
            })
            .unwrap()
            .id
    }

    fn arguments(values: &[&str]) -> LaunchOverrides {
        LaunchOverrides {
            arguments: Some(values.iter().map(|value| (*value).to_owned()).collect()),
            ..LaunchOverrides::default()
        }
    }

    fn base_compatibility() -> EffectiveCompatibilityConfig {
        EffectiveCompatibilityConfig {
            runner_path: Some("/runners/base".to_owned()),
            arguments_before: vec!["-before".to_owned()],
            arguments_after: vec!["-base".to_owned()],
            environment: BTreeMap::from([("BASE".to_owned(), "1".to_owned())]),
            graphics_renderer: GraphicsRenderer::WineD3d,
            linux_performance: LinuxPerformance {
                game_mode: true,
                mangohud: false,
                gamescope: None,
                wrapper: Vec::new(),
            },
            ..EffectiveCompatibilityConfig::default()
        }
    }

    #[test]
    fn empty_overrides_inherit_every_setting() {
        let overrides = LaunchOverrides::default();
        assert_eq!(
            overrides.resolve_compatibility(base_compatibility()),
            base_compatibility()
        );
        let native = NativeLaunchConfig {
            arguments: vec!["-base".to_owned()],
            working_directory: Some("/work".to_owned()),
            environment: BTreeMap::from([("BASE".to_owned(), "1".to_owned())]),
        };
        assert_eq!(overrides.resolve_native(native.clone()), native);
        let steam = SteamLaunchConfig {
            arguments: vec!["-base".to_owned()],
        };
        assert_eq!(overrides.resolve_steam(steam.clone()), steam);
    }

    #[test]
    fn each_override_replaces_only_its_own_setting() {
        let base = base_compatibility();
        let resolved = arguments(&["-profile"]).resolve_compatibility(base.clone());
        assert_eq!(resolved.arguments_after, ["-profile"]);
        assert_eq!(
            EffectiveCompatibilityConfig {
                arguments_after: base.arguments_after.clone(),
                ..resolved
            },
            base
        );

        let cleared = LaunchOverrides {
            environment: Some(BTreeMap::new()),
            ..LaunchOverrides::default()
        }
        .resolve_compatibility(base.clone());
        assert!(cleared.environment.is_empty());
        assert_eq!(cleared.arguments_after, base.arguments_after);

        let runner = LaunchOverrides {
            runner_path: Some("/runners/other".to_owned()),
            ..LaunchOverrides::default()
        }
        .resolve_compatibility(base.clone());
        assert_eq!(runner.runner_path.as_deref(), Some("/runners/other"));
        assert_eq!(runner.environment, base.environment);

        let performance = LaunchOverrides {
            linux_performance: Some(LinuxPerformance::default()),
            ..LaunchOverrides::default()
        }
        .resolve_compatibility(base.clone());
        assert_eq!(performance.linux_performance, LinuxPerformance::default());
        assert_eq!(performance.runner_path, base.runner_path);
        assert_eq!(performance.arguments_before, base.arguments_before);
    }

    #[test]
    fn native_and_steam_resolution_replace_arguments_and_environment() {
        let native = NativeLaunchConfig {
            arguments: vec!["-base".to_owned()],
            working_directory: Some("/work".to_owned()),
            environment: BTreeMap::from([("BASE".to_owned(), "1".to_owned())]),
        };
        let overrides = LaunchOverrides {
            arguments: Some(vec!["-profile".to_owned()]),
            environment: Some(BTreeMap::from([("PROFILE".to_owned(), "1".to_owned())])),
            ..LaunchOverrides::default()
        };
        let resolved = overrides.resolve_native(native.clone());
        assert_eq!(resolved.arguments, ["-profile"]);
        assert_eq!(resolved.environment["PROFILE"], "1");
        assert!(!resolved.environment.contains_key("BASE"));
        assert_eq!(resolved.working_directory, native.working_directory);

        let steam = overrides.resolve_steam(SteamLaunchConfig {
            arguments: vec!["-base".to_owned()],
        });
        assert_eq!(steam.arguments, ["-profile"]);
    }

    #[test]
    fn names_are_trimmed_bounded_and_unique_ignoring_case() {
        let (_fixture, database) = open();
        let id = game(&database, "Game");
        let other = game(&database, "Other");
        let profiles = database
            .create_launch_profile(&id, "  Vanilla  ", LaunchOverrides::default())
            .unwrap();
        assert_eq!(profiles.profiles[0].name, "Vanilla");
        assert!(
            database
                .create_launch_profile(&id, "VANILLA", LaunchOverrides::default())
                .is_err()
        );
        database
            .create_launch_profile(&other, "vanilla", LaunchOverrides::default())
            .unwrap();
        for invalid in ["", "   ", "a\nb", &"x".repeat(49)] {
            assert!(
                database
                    .create_launch_profile(&id, invalid, LaunchOverrides::default())
                    .is_err(),
                "{invalid:?}"
            );
        }
        database
            .create_launch_profile(&id, &"é".repeat(48), LaunchOverrides::default())
            .unwrap();

        let vanilla = profiles.profiles[0].id.clone();
        let second = database
            .create_launch_profile(&id, "Modded", LaunchOverrides::default())
            .unwrap()
            .profiles
            .iter()
            .find(|profile| profile.name == "Modded")
            .unwrap()
            .id
            .clone();
        // Renaming to its own name in another case is allowed, to another profile's is not.
        database
            .update_launch_profile(&vanilla, "vanilla", LaunchOverrides::default())
            .unwrap();
        assert!(
            database
                .update_launch_profile(&second, "Vanilla", LaunchOverrides::default())
                .is_err()
        );
    }

    #[test]
    fn a_game_holds_a_limited_number_of_profiles() {
        let (_fixture, database) = open();
        let id = game(&database, "Game");
        for index in 0..MAX_PROFILES_PER_GAME {
            database
                .create_launch_profile(&id, &format!("Profile {index}"), LaunchOverrides::default())
                .unwrap();
        }
        assert!(
            database
                .create_launch_profile(&id, "One more", LaunchOverrides::default())
                .is_err()
        );
        assert_eq!(
            database.launch_profiles().unwrap().profiles.len(),
            MAX_PROFILES_PER_GAME
        );
    }

    #[test]
    fn overrides_use_the_base_validators() {
        let (_fixture, database) = open();
        let id = game(&database, "Game");
        let invalid = [
            arguments(&["bad\0argument"]),
            LaunchOverrides {
                environment: Some(BTreeMap::from([("A=B".to_owned(), "1".to_owned())])),
                ..LaunchOverrides::default()
            },
            LaunchOverrides {
                environment: Some(BTreeMap::from([(
                    "LEGIO_LAUNCH_TOKEN".to_owned(),
                    "1".to_owned(),
                )])),
                ..LaunchOverrides::default()
            },
            LaunchOverrides {
                runner_path: Some(String::new()),
                ..LaunchOverrides::default()
            },
            LaunchOverrides {
                linux_performance: Some(LinuxPerformance {
                    game_mode: false,
                    mangohud: false,
                    wrapper: Vec::new(),
                    gamescope: Some(crate::database::GamescopeConfig {
                        width: Some(1920),
                        height: None,
                        fps: None,
                    }),
                }),
                ..LaunchOverrides::default()
            },
        ];
        for overrides in invalid {
            assert!(
                database
                    .create_launch_profile(&id, "Invalid", overrides.clone())
                    .is_err(),
                "{overrides:?}"
            );
        }
        assert!(database.launch_profiles().unwrap().profiles.is_empty());
    }

    #[test]
    fn overrides_round_trip_and_reject_unknown_fields() {
        let (_fixture, database) = open();
        let id = game(&database, "Game");
        let overrides = LaunchOverrides {
            arguments: Some(vec!["-windowed".to_owned()]),
            environment: Some(BTreeMap::from([("MODE".to_owned(), "window".to_owned())])),
            ..LaunchOverrides::default()
        };
        let profiles = database
            .create_launch_profile(&id, "Window", overrides.clone())
            .unwrap();
        assert_eq!(profiles.profiles[0].overrides, overrides);
        assert_eq!(profiles.profiles[0].game_id, id);
        assert!(serde_json::from_str::<LaunchOverrides>(r#"{"unknown":1}"#).is_err());
    }

    #[test]
    fn duplicate_copies_overrides_under_a_new_name() {
        let (_fixture, database) = open();
        let id = game(&database, "Game");
        let profiles = database
            .create_launch_profile(&id, "Window", arguments(&["-windowed"]))
            .unwrap();
        let source = profiles.profiles[0].id.clone();
        let profiles = database
            .duplicate_launch_profile(&source, "Window copy")
            .unwrap();
        assert_eq!(profiles.profiles.len(), 2);
        assert_eq!(profiles.profiles[1].name, "Window copy");
        assert_eq!(profiles.profiles[1].overrides, arguments(&["-windowed"]));
        assert!(
            database
                .duplicate_launch_profile(&source, "window COPY")
                .is_err()
        );
        assert!(
            database
                .duplicate_launch_profile(&Uuid::new_v4().to_string(), "Missing")
                .is_err()
        );
    }

    #[test]
    fn launch_lookup_requires_a_profile_of_the_same_game() {
        let (_fixture, database) = open();
        let id = game(&database, "Game");
        let other = game(&database, "Other");
        let profile = database
            .create_launch_profile(&id, "Window", arguments(&["-windowed"]))
            .unwrap()
            .profiles[0]
            .id
            .clone();
        assert_eq!(database.launch_profile(&id, None).unwrap(), None);
        assert_eq!(
            database
                .launch_profile(&id, Some(&profile))
                .unwrap()
                .unwrap()
                .name,
            "Window"
        );
        assert!(
            database
                .launch_profile(&id, Some(&Uuid::new_v4().to_string()))
                .is_err()
        );
        assert!(database.launch_profile(&id, Some("not-an-id")).is_err());
        assert!(database.launch_profile(&other, Some(&profile)).is_err());
        assert!(
            database
                .set_last_launch_profile(&other, Some(&profile))
                .is_err()
        );
    }

    #[test]
    fn deleting_the_last_used_profile_returns_to_the_base_settings() {
        let (_fixture, database) = open();
        let id = game(&database, "Game");
        let profile = database
            .create_launch_profile(&id, "Window", LaunchOverrides::default())
            .unwrap()
            .profiles[0]
            .id
            .clone();
        database
            .set_last_launch_profile(&id, Some(&profile))
            .unwrap();
        assert_eq!(database.launch_profiles().unwrap().last_used[&id], profile);

        let profiles = database.delete_launch_profile(&profile).unwrap();
        assert!(profiles.profiles.is_empty());
        assert!(profiles.last_used.is_empty());
        assert!(database.delete_launch_profile(&profile).is_err());

        let profile = database
            .create_launch_profile(&id, "Window", LaunchOverrides::default())
            .unwrap()
            .profiles[0]
            .id
            .clone();
        database
            .set_last_launch_profile(&id, Some(&profile))
            .unwrap();
        database.set_last_launch_profile(&id, None).unwrap();
        assert!(database.launch_profiles().unwrap().last_used.is_empty());
    }

    #[test]
    fn removing_a_game_removes_its_profiles_and_selection() {
        let (_fixture, database) = open();
        let id = game(&database, "Game");
        let kept = game(&database, "Kept");
        let profile = database
            .create_launch_profile(&id, "Window", LaunchOverrides::default())
            .unwrap()
            .profiles[0]
            .id
            .clone();
        database
            .create_launch_profile(&kept, "Window", LaunchOverrides::default())
            .unwrap();
        database
            .set_last_launch_profile(&id, Some(&profile))
            .unwrap();

        database.remove_game(&id).unwrap();
        let profiles = database.launch_profiles().unwrap();
        assert_eq!(profiles.profiles.len(), 1);
        assert_eq!(profiles.profiles[0].game_id, kept);
        assert!(profiles.last_used.is_empty());
    }
}
