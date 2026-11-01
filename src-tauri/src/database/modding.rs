use super::{Database, database_error, parse_game_id};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;
use uuid::Uuid;

pub(crate) const MAX_PROFILE_NAME_CHARS: usize = 64;
const MAX_ECOSYSTEM_ID_CHARS: usize = 128;

/// Thunderstore binding of a game. `active_profile_id` of `None` means vanilla.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameModding {
    pub game_id: String,
    pub ecosystem_game_id: String,
    pub active_profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModProfile {
    pub id: String,
    pub game_id: String,
    pub name: String,
    pub created_at: i64,
}

/// A mod installed in a profile. `full_name` is the Thunderstore `Owner-Name`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMod {
    pub profile_id: String,
    pub full_name: String,
    pub version: String,
    pub enabled: bool,
    /// False for mods imported from a local archive.
    pub online_source: bool,
    pub installed_at: i64,
    pub install_order: i64,
}

impl Database {
    pub(crate) fn game_modding(&self, game_id: &str) -> Result<Option<GameModding>, String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| read_game_modding(connection, &game_id))
    }

    /// Binds a game to a Thunderstore community, keeping its active profile.
    pub(crate) fn set_game_modding(
        &self,
        game_id: &str,
        ecosystem_game_id: &str,
    ) -> Result<GameModding, String> {
        let game_id = parse_game_id(game_id)?;
        if ecosystem_game_id.is_empty()
            || ecosystem_game_id.chars().count() > MAX_ECOSYSTEM_ID_CHARS
            || ecosystem_game_id.chars().any(char::is_control)
        {
            return Err("Thunderstore game id is invalid".to_owned());
        }
        self.with_connection(|connection| {
            require_game(connection, &game_id)?;
            connection
                .execute(
                    "INSERT INTO game_modding (game_id, ecosystem_game_id) VALUES (?1, ?2)
                     ON CONFLICT(game_id) DO UPDATE SET ecosystem_game_id = excluded.ecosystem_game_id",
                    params![game_id, ecosystem_game_id],
                )
                .map_err(database_error)?;
            read_game_modding(connection, &game_id)?
                .ok_or_else(|| "game modding was not found".to_owned())
        })
    }

    /// Selects the profile used at launch; `None` selects vanilla.
    pub(crate) fn set_active_mod_profile(
        &self,
        game_id: &str,
        profile_id: Option<&str>,
    ) -> Result<GameModding, String> {
        let game_id = parse_game_id(game_id)?;
        let profile_id = profile_id.map(parse_profile_id).transpose()?;
        self.with_connection(|connection| {
            let tx = connection.unchecked_transaction().map_err(database_error)?;
            if let Some(profile_id) = &profile_id {
                let belongs: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM mod_profiles WHERE id = ?1 AND game_id = ?2)",
                        params![profile_id, game_id],
                        |row| row.get(0),
                    )
                    .map_err(database_error)?;
                if !belongs {
                    return Err("Mod profile was not found for this game".to_owned());
                }
            }
            let changed = tx
                .execute(
                    "UPDATE game_modding SET active_profile_id = ?2 WHERE game_id = ?1",
                    params![game_id, profile_id],
                )
                .map_err(database_error)?;
            if changed == 0 {
                return Err("Game is not set up for mods".to_owned());
            }
            let modding = read_game_modding(&tx, &game_id)?
                .ok_or_else(|| "game modding was not found".to_owned())?;
            tx.commit().map_err(database_error)?;
            Ok(modding)
        })
    }

    pub(crate) fn mod_profiles(&self, game_id: &str) -> Result<Vec<ModProfile>, String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT id, game_id, name, created_at FROM mod_profiles
                     WHERE game_id = ?1 ORDER BY created_at, name COLLATE NOCASE, id",
                )
                .map_err(database_error)?;
            statement
                .query_map([&game_id], profile_from_row)
                .map_err(database_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(database_error)
        })
    }

    pub(crate) fn mod_profile(&self, profile_id: &str) -> Result<Option<ModProfile>, String> {
        let profile_id = parse_profile_id(profile_id)?;
        self.with_connection(|connection| read_profile(connection, &profile_id))
    }

    pub(crate) fn create_mod_profile(
        &self,
        game_id: &str,
        name: &str,
        now: i64,
    ) -> Result<ModProfile, String> {
        let game_id = parse_game_id(game_id)?;
        let name = validate_profile_name(name)?;
        self.with_connection(|connection| {
            let tx = connection.unchecked_transaction().map_err(database_error)?;
            require_game(&tx, &game_id)?;
            ensure_unique_profile_name(&tx, &game_id, &name, None)?;
            let id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO mod_profiles (id, game_id, name, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![id, game_id, name, now],
            )
            .map_err(database_error)?;
            tx.commit().map_err(database_error)?;
            Ok(ModProfile {
                id,
                game_id,
                name,
                created_at: now,
            })
        })
    }

    pub(crate) fn rename_mod_profile(
        &self,
        profile_id: &str,
        name: &str,
    ) -> Result<ModProfile, String> {
        let profile_id = parse_profile_id(profile_id)?;
        let name = validate_profile_name(name)?;
        self.with_connection(|connection| {
            let tx = connection.unchecked_transaction().map_err(database_error)?;
            let profile = read_profile(&tx, &profile_id)?.ok_or_else(profile_not_found)?;
            ensure_unique_profile_name(&tx, &profile.game_id, &name, Some(&profile_id))?;
            tx.execute(
                "UPDATE mod_profiles SET name = ?2 WHERE id = ?1",
                params![profile_id, name],
            )
            .map_err(database_error)?;
            tx.commit().map_err(database_error)?;
            Ok(ModProfile { name, ..profile })
        })
    }

    /// Deletes a profile with its mod and file records. A game using it as
    /// the active profile falls back to vanilla.
    pub(crate) fn delete_mod_profile(&self, profile_id: &str) -> Result<(), String> {
        let profile_id = parse_profile_id(profile_id)?;
        self.with_connection(|connection| {
            let changed = connection
                .execute("DELETE FROM mod_profiles WHERE id = ?1", [&profile_id])
                .map_err(database_error)?;
            if changed == 0 {
                return Err(profile_not_found());
            }
            Ok(())
        })
    }

    /// Moves the mod and file records of `from` to `into`, dropping the ones
    /// `into` had, then deletes `from`. Both profiles must belong to one game.
    pub(crate) fn replace_profile_contents(&self, from: &str, into: &str) -> Result<(), String> {
        let from = parse_profile_id(from)?;
        let into = parse_profile_id(into)?;
        self.with_connection(|connection| {
            let tx = connection.unchecked_transaction().map_err(database_error)?;
            let source = read_profile(&tx, &from)?.ok_or_else(profile_not_found)?;
            let target = read_profile(&tx, &into)?.ok_or_else(profile_not_found)?;
            if source.game_id != target.game_id || source.id == target.id {
                return Err("Mod profiles do not belong to the same game".to_owned());
            }
            for table in ["mod_profile_files", "mod_profile_mods"] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE profile_id = ?1"),
                    [&into],
                )
                .map_err(database_error)?;
                tx.execute(
                    &format!("UPDATE {table} SET profile_id = ?2 WHERE profile_id = ?1"),
                    params![from, into],
                )
                .map_err(database_error)?;
            }
            tx.execute("DELETE FROM mod_profiles WHERE id = ?1", [&from])
                .map_err(database_error)?;
            tx.commit().map_err(database_error)
        })
    }

    pub(crate) fn profile_mods(&self, profile_id: &str) -> Result<Vec<ProfileMod>, String> {
        let profile_id = parse_profile_id(profile_id)?;
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT profile_id, full_name, version, enabled, online_source,
                         installed_at, install_order
                     FROM mod_profile_mods WHERE profile_id = ?1
                     ORDER BY install_order, full_name",
                )
                .map_err(database_error)?;
            statement
                .query_map([&profile_id], |row| {
                    Ok(ProfileMod {
                        profile_id: row.get(0)?,
                        full_name: row.get(1)?,
                        version: row.get(2)?,
                        enabled: row.get(3)?,
                        online_source: row.get(4)?,
                        installed_at: row.get(5)?,
                        install_order: row.get(6)?,
                    })
                })
                .map_err(database_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(database_error)
        })
    }

    /// Inserts a mod or replaces the record with the same `full_name`.
    pub(crate) fn upsert_profile_mod(&self, profile_mod: &ProfileMod) -> Result<(), String> {
        let profile_id = parse_profile_id(&profile_mod.profile_id)?;
        require_mod_text(&profile_mod.full_name, "mod name")?;
        require_mod_text(&profile_mod.version, "mod version")?;
        self.with_connection(|connection| {
            require_profile(connection, &profile_id)?;
            connection
                .execute(
                    "INSERT INTO mod_profile_mods (profile_id, full_name, version, enabled,
                         online_source, installed_at, install_order)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(profile_id, full_name) DO UPDATE SET
                         version = excluded.version, enabled = excluded.enabled,
                         online_source = excluded.online_source,
                         installed_at = excluded.installed_at,
                         install_order = excluded.install_order",
                    params![
                        profile_id,
                        profile_mod.full_name,
                        profile_mod.version,
                        profile_mod.enabled,
                        profile_mod.online_source,
                        profile_mod.installed_at,
                        profile_mod.install_order,
                    ],
                )
                .map_err(database_error)?;
            Ok(())
        })
    }

    /// Removes a mod record together with its tracked files.
    pub(crate) fn remove_profile_mod(
        &self,
        profile_id: &str,
        full_name: &str,
    ) -> Result<(), String> {
        let profile_id = parse_profile_id(profile_id)?;
        self.with_connection(|connection| {
            let tx = connection.unchecked_transaction().map_err(database_error)?;
            tx.execute(
                "DELETE FROM mod_profile_files WHERE profile_id = ?1 AND full_name = ?2",
                params![profile_id, full_name],
            )
            .map_err(database_error)?;
            tx.execute(
                "DELETE FROM mod_profile_mods WHERE profile_id = ?1 AND full_name = ?2",
                params![profile_id, full_name],
            )
            .map_err(database_error)?;
            tx.commit().map_err(database_error)
        })
    }

    pub(crate) fn set_profile_mod_enabled(
        &self,
        profile_id: &str,
        full_name: &str,
        enabled: bool,
    ) -> Result<(), String> {
        let profile_id = parse_profile_id(profile_id)?;
        self.with_connection(|connection| {
            let changed = connection
                .execute(
                    "UPDATE mod_profile_mods SET enabled = ?3
                     WHERE profile_id = ?1 AND full_name = ?2",
                    params![profile_id, full_name, enabled],
                )
                .map_err(database_error)?;
            if changed == 0 {
                return Err("Mod was not found in this profile".to_owned());
            }
            Ok(())
        })
    }

    /// Files a mod installed into the profile directory, as `/`-separated
    /// paths relative to it.
    pub(crate) fn profile_mod_files(
        &self,
        profile_id: &str,
        full_name: &str,
    ) -> Result<Vec<String>, String> {
        let profile_id = parse_profile_id(profile_id)?;
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT relative_path FROM mod_profile_files
                     WHERE profile_id = ?1 AND full_name = ?2 ORDER BY relative_path",
                )
                .map_err(database_error)?;
            statement
                .query_map(params![profile_id, full_name], |row| row.get(0))
                .map_err(database_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(database_error)
        })
    }

    /// Replaces the tracked files of a mod. A path already tracked for
    /// another mod of the profile is rejected and nothing changes.
    pub(crate) fn replace_profile_mod_files(
        &self,
        profile_id: &str,
        full_name: &str,
        relative_paths: &[String],
    ) -> Result<(), String> {
        let profile_id = parse_profile_id(profile_id)?;
        require_mod_text(full_name, "mod name")?;
        validate_relative_paths(relative_paths)?;
        self.with_connection(|connection| {
            let tx = connection.unchecked_transaction().map_err(database_error)?;
            require_profile(&tx, &profile_id)?;
            tx.execute(
                "DELETE FROM mod_profile_files WHERE profile_id = ?1 AND full_name = ?2",
                params![profile_id, full_name],
            )
            .map_err(database_error)?;
            {
                let mut insert = tx
                    .prepare(
                        "INSERT INTO mod_profile_files (profile_id, full_name, relative_path)
                         VALUES (?1, ?2, ?3)",
                    )
                    .map_err(database_error)?;
                for path in relative_paths {
                    insert
                        .execute(params![profile_id, full_name, path])
                        .map_err(|error| {
                            if is_constraint_violation(&error) {
                                format!("File {path} is already tracked by another mod")
                            } else {
                                database_error(error)
                            }
                        })?;
                }
            }
            tx.commit().map_err(database_error)
        })
    }

    /// Loader files copied into the game folder, relative to it.
    pub(crate) fn deployed_files(&self, game_id: &str) -> Result<Vec<String>, String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT relative_path FROM game_mod_deployed_files
                     WHERE game_id = ?1 ORDER BY relative_path",
                )
                .map_err(database_error)?;
            statement
                .query_map([&game_id], |row| row.get(0))
                .map_err(database_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(database_error)
        })
    }

    pub(crate) fn replace_deployed_files(
        &self,
        game_id: &str,
        relative_paths: &[String],
    ) -> Result<(), String> {
        let game_id = parse_game_id(game_id)?;
        validate_relative_paths(relative_paths)?;
        self.with_connection(|connection| {
            let tx = connection.unchecked_transaction().map_err(database_error)?;
            require_game(&tx, &game_id)?;
            tx.execute(
                "DELETE FROM game_mod_deployed_files WHERE game_id = ?1",
                [&game_id],
            )
            .map_err(database_error)?;
            {
                let mut insert = tx
                    .prepare(
                        "INSERT OR IGNORE INTO game_mod_deployed_files (game_id, relative_path)
                         VALUES (?1, ?2)",
                    )
                    .map_err(database_error)?;
                for path in relative_paths {
                    insert
                        .execute(params![game_id, path])
                        .map_err(database_error)?;
                }
            }
            tx.commit().map_err(database_error)
        })
    }

    pub(crate) fn clear_deployed_files(&self, game_id: &str) -> Result<(), String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| {
            connection
                .execute(
                    "DELETE FROM game_mod_deployed_files WHERE game_id = ?1",
                    [&game_id],
                )
                .map_err(database_error)?;
            Ok(())
        })
    }
}

fn parse_profile_id(value: &str) -> Result<String, String> {
    Uuid::parse_str(value)
        .map(|id| id.to_string())
        .map_err(|_| "mod profile id is invalid".to_owned())
}

fn profile_not_found() -> String {
    "Mod profile was not found".to_owned()
}

fn require_game(connection: &Connection, game_id: &str) -> Result<(), String> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM games WHERE id = ?1)",
            [game_id],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if exists {
        Ok(())
    } else {
        Err("game was not found".to_owned())
    }
}

fn require_profile(connection: &Connection, profile_id: &str) -> Result<(), String> {
    read_profile(connection, profile_id)?
        .map(|_| ())
        .ok_or_else(profile_not_found)
}

fn read_game_modding(
    connection: &Connection,
    game_id: &str,
) -> Result<Option<GameModding>, String> {
    connection
        .query_row(
            "SELECT game_id, ecosystem_game_id, active_profile_id FROM game_modding
             WHERE game_id = ?1",
            [game_id],
            |row| {
                Ok(GameModding {
                    game_id: row.get(0)?,
                    ecosystem_game_id: row.get(1)?,
                    active_profile_id: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(database_error)
}

fn read_profile(connection: &Connection, profile_id: &str) -> Result<Option<ModProfile>, String> {
    connection
        .query_row(
            "SELECT id, game_id, name, created_at FROM mod_profiles WHERE id = ?1",
            [profile_id],
            profile_from_row,
        )
        .optional()
        .map_err(database_error)
}

fn profile_from_row(row: &Row<'_>) -> rusqlite::Result<ModProfile> {
    Ok(ModProfile {
        id: row.get(0)?,
        game_id: row.get(1)?,
        name: row.get(2)?,
        created_at: row.get(3)?,
    })
}

fn ensure_unique_profile_name(
    connection: &Connection,
    game_id: &str,
    name: &str,
    except_id: Option<&str>,
) -> Result<(), String> {
    let taken: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM mod_profiles
                 WHERE game_id = ?1 AND name = ?2 COLLATE NOCASE AND id IS NOT ?3)",
            params![game_id, name, except_id],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if taken {
        Err(format!("A mod profile named \"{name}\" already exists"))
    } else {
        Ok(())
    }
}

fn validate_profile_name(value: &str) -> Result<String, String> {
    let name = value.trim();
    let length = name.chars().count();
    if length == 0 || length > MAX_PROFILE_NAME_CHARS {
        return Err(format!(
            "Profile name must be between 1 and {MAX_PROFILE_NAME_CHARS} characters"
        ));
    }
    if name
        .chars()
        .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err("Profile name cannot contain control characters or path separators".to_owned());
    }
    Ok(name.to_owned())
}

fn require_mod_text(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

fn validate_relative_paths(paths: &[String]) -> Result<(), String> {
    paths
        .iter()
        .try_for_each(|path| crate::modding::paths::validate_relative_path(path))
}

fn is_constraint_violation(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{CreateGameInput, SCHEMA_VERSION, migrate};

    fn game_input(name: &str) -> CreateGameInput {
        CreateGameInput {
            name: name.to_owned(),
            steam_app_id: None,
        }
    }

    fn open() -> (Database, std::path::PathBuf) {
        let directory = std::env::temp_dir().join(format!("legio-modding-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        (Database::open(&directory).unwrap(), directory)
    }

    fn count(database: &Database, table: &str) -> i64 {
        database
            .with_connection(|connection| {
                connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                    .map_err(database_error)
            })
            .unwrap()
    }

    fn mod_record(profile_id: &str, full_name: &str, order: i64) -> ProfileMod {
        ProfileMod {
            profile_id: profile_id.to_owned(),
            full_name: full_name.to_owned(),
            version: "1.0.0".to_owned(),
            enabled: true,
            online_source: true,
            installed_at: 5,
            install_order: order,
        }
    }

    fn paths(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn migrates_a_v35_database() {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        connection
            .execute_batch(
                "DROP TABLE game_mod_deployed_files; DROP TABLE mod_profile_files;
                 DROP TABLE mod_profile_mods; DROP TABLE game_modding; DROP TABLE mod_profiles;
                 INSERT INTO games (id, name_override) VALUES ('00000000-0000-4000-8000-000000000001', 'Existing');
                 PRAGMA user_version = 35;",
            )
            .unwrap();
        migrate(&connection).unwrap();
        let tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN
                 ('game_modding', 'mod_profiles', 'mod_profile_mods', 'mod_profile_files',
                  'game_mod_deployed_files')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let games: i64 = connection
            .query_row("SELECT COUNT(*) FROM games", [], |row| row.get(0))
            .unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!((tables, games, version), (5, 1, SCHEMA_VERSION));
    }

    #[test]
    fn binds_a_game_and_switches_profiles() {
        let (database, directory) = open();
        let game = database.create_game(game_input("Game")).unwrap();
        assert_eq!(database.game_modding(&game.id).unwrap(), None);
        assert!(database.set_game_modding(&game.id, "").is_err());
        assert!(
            database
                .set_game_modding(&Uuid::new_v4().to_string(), "x")
                .is_err()
        );

        let bound = database
            .set_game_modding(&game.id, "lethal-company")
            .unwrap();
        assert_eq!(bound.active_profile_id, None);
        let profile = database.create_mod_profile(&game.id, "Main", 1).unwrap();
        let active = database
            .set_active_mod_profile(&game.id, Some(&profile.id))
            .unwrap();
        assert_eq!(
            active.active_profile_id.as_deref(),
            Some(profile.id.as_str())
        );

        let rebound = database.set_game_modding(&game.id, "riskofrain2").unwrap();
        assert_eq!(rebound.ecosystem_game_id, "riskofrain2");
        assert_eq!(rebound.active_profile_id, active.active_profile_id);

        let vanilla = database.set_active_mod_profile(&game.id, None).unwrap();
        assert_eq!(vanilla.active_profile_id, None);
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_activating_a_profile_of_another_game() {
        let (database, directory) = open();
        let first = database.create_game(game_input("First")).unwrap();
        let second = database.create_game(game_input("Second")).unwrap();
        database.set_game_modding(&first.id, "a").unwrap();
        let foreign = database.create_mod_profile(&second.id, "Other", 1).unwrap();
        assert!(
            database
                .set_active_mod_profile(&first.id, Some(&foreign.id))
                .is_err()
        );
        assert!(
            database
                .set_active_mod_profile(&first.id, Some(&Uuid::new_v4().to_string()))
                .is_err()
        );
        assert!(database.set_active_mod_profile(&second.id, None).is_err());
        assert_eq!(
            database
                .game_modding(&first.id)
                .unwrap()
                .unwrap()
                .active_profile_id,
            None
        );
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn validates_profile_names() {
        let (database, directory) = open();
        let game = database.create_game(game_input("Game")).unwrap();
        let other = database.create_game(game_input("Other")).unwrap();
        let created = database
            .create_mod_profile(&game.id, "  Main  ", 1)
            .unwrap();
        assert_eq!(created.name, "Main");

        for name in ["", "   ", "a/b", "a\\b", "tab\there", &"x".repeat(65)] {
            assert!(
                database.create_mod_profile(&game.id, name, 2).is_err(),
                "{name:?}"
            );
        }
        assert!(
            database
                .create_mod_profile(&game.id, &"x".repeat(64), 2)
                .is_ok()
        );

        let duplicate = database
            .create_mod_profile(&game.id, "MAIN", 3)
            .unwrap_err();
        assert!(duplicate.contains("already exists"), "{duplicate}");
        assert!(database.create_mod_profile(&other.id, "Main", 3).is_ok());

        let second = database.create_mod_profile(&game.id, "Second", 4).unwrap();
        assert!(database.rename_mod_profile(&second.id, "main").is_err());
        assert_eq!(
            database
                .rename_mod_profile(&created.id, "MAIN")
                .unwrap()
                .name,
            "MAIN"
        );
        assert_eq!(
            database
                .rename_mod_profile(&second.id, "Renamed")
                .unwrap()
                .name,
            "Renamed"
        );
        assert!(
            database
                .rename_mod_profile(&Uuid::new_v4().to_string(), "Name")
                .is_err()
        );
        let names: Vec<_> = database
            .mod_profiles(&game.id)
            .unwrap()
            .into_iter()
            .map(|profile| profile.name)
            .collect();
        assert_eq!(names, ["MAIN", &"x".repeat(64), "Renamed"]);
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn deleting_the_active_profile_returns_to_vanilla() {
        let (database, directory) = open();
        let game = database.create_game(game_input("Game")).unwrap();
        database.set_game_modding(&game.id, "a").unwrap();
        let profile = database.create_mod_profile(&game.id, "Main", 1).unwrap();
        database
            .set_active_mod_profile(&game.id, Some(&profile.id))
            .unwrap();
        database
            .upsert_profile_mod(&mod_record(&profile.id, "Owner-Mod", 0))
            .unwrap();
        database
            .replace_profile_mod_files(&profile.id, "Owner-Mod", &paths(&["BepInEx/plugins/a.dll"]))
            .unwrap();

        database.delete_mod_profile(&profile.id).unwrap();
        assert_eq!(
            database
                .game_modding(&game.id)
                .unwrap()
                .unwrap()
                .active_profile_id,
            None
        );
        assert_eq!(count(&database, "mod_profile_mods"), 0);
        assert_eq!(count(&database, "mod_profile_files"), 0);
        assert!(database.delete_mod_profile(&profile.id).is_err());
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn tracks_mods_and_their_files() {
        let (database, directory) = open();
        let game = database.create_game(game_input("Game")).unwrap();
        let profile = database.create_mod_profile(&game.id, "Main", 1).unwrap();
        let id = profile.id.as_str();
        database
            .upsert_profile_mod(&mod_record(id, "B-Second", 2))
            .unwrap();
        database
            .upsert_profile_mod(&mod_record(id, "A-First", 1))
            .unwrap();
        assert!(
            database
                .upsert_profile_mod(&mod_record(&Uuid::new_v4().to_string(), "A-First", 1))
                .is_err()
        );

        let mut updated = mod_record(id, "A-First", 1);
        updated.version = "2.0.0".to_owned();
        updated.online_source = false;
        database.upsert_profile_mod(&updated).unwrap();
        let mods = database.profile_mods(id).unwrap();
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0], updated);
        assert_eq!(mods[1].full_name, "B-Second");

        database
            .set_profile_mod_enabled(id, "A-First", false)
            .unwrap();
        assert!(!database.profile_mods(id).unwrap()[0].enabled);
        assert!(
            database
                .set_profile_mod_enabled(id, "Missing-Mod", true)
                .is_err()
        );

        database
            .replace_profile_mod_files(
                id,
                "A-First",
                &paths(&["BepInEx/plugins/a/b.dll", "BepInEx/config/a.cfg"]),
            )
            .unwrap();
        database
            .replace_profile_mod_files(id, "B-Second", &paths(&["BepInEx/plugins/c.dll"]))
            .unwrap();
        assert_eq!(
            database.profile_mod_files(id, "A-First").unwrap(),
            ["BepInEx/config/a.cfg", "BepInEx/plugins/a/b.dll"]
        );
        let conflict = database
            .replace_profile_mod_files(id, "B-Second", &paths(&["BepInEx/plugins/a/b.dll"]))
            .unwrap_err();
        assert!(conflict.contains("already tracked"), "{conflict}");
        assert_eq!(
            database.profile_mod_files(id, "B-Second").unwrap(),
            ["BepInEx/plugins/c.dll"]
        );

        database
            .replace_profile_mod_files(id, "A-First", &paths(&["BepInEx/plugins/new.dll"]))
            .unwrap();
        assert_eq!(
            database.profile_mod_files(id, "A-First").unwrap(),
            ["BepInEx/plugins/new.dll"]
        );

        database.remove_profile_mod(id, "A-First").unwrap();
        assert!(
            database
                .profile_mod_files(id, "A-First")
                .unwrap()
                .is_empty()
        );
        assert_eq!(database.profile_mods(id).unwrap().len(), 1);
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn moves_profile_contents_to_another_profile() {
        let (database, directory) = open();
        let game = database.create_game(game_input("Game")).unwrap();
        let other = database.create_game(game_input("Other")).unwrap();
        let old = database.create_mod_profile(&game.id, "Old", 1).unwrap();
        let fresh = database.create_mod_profile(&game.id, "Fresh", 2).unwrap();
        let foreign = database
            .create_mod_profile(&other.id, "Foreign", 3)
            .unwrap();
        database
            .set_game_modding(&game.id, "lethal-company")
            .unwrap();
        database
            .set_active_mod_profile(&game.id, Some(&old.id))
            .unwrap();
        database
            .upsert_profile_mod(&mod_record(&old.id, "Old-Mod", 0))
            .unwrap();
        database
            .replace_profile_mod_files(&old.id, "Old-Mod", &paths(&["BepInEx/plugins/old.dll"]))
            .unwrap();
        database
            .upsert_profile_mod(&mod_record(&fresh.id, "New-Mod", 0))
            .unwrap();
        database
            .replace_profile_mod_files(&fresh.id, "New-Mod", &paths(&["BepInEx/plugins/new.dll"]))
            .unwrap();

        assert!(
            database
                .replace_profile_contents(&foreign.id, &old.id)
                .is_err()
        );
        database
            .replace_profile_contents(&fresh.id, &old.id)
            .unwrap();

        let mods = database.profile_mods(&old.id).unwrap();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].full_name, "New-Mod");
        assert_eq!(
            database.profile_mod_files(&old.id, "New-Mod").unwrap(),
            ["BepInEx/plugins/new.dll"]
        );
        assert!(
            database
                .profile_mod_files(&old.id, "Old-Mod")
                .unwrap()
                .is_empty()
        );
        assert_eq!(database.mod_profile(&fresh.id).unwrap(), None);
        assert_eq!(
            database
                .game_modding(&game.id)
                .unwrap()
                .unwrap()
                .active_profile_id,
            Some(old.id.clone())
        );
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_unsafe_tracked_paths() {
        let (database, directory) = open();
        let game = database.create_game(game_input("Game")).unwrap();
        let profile = database.create_mod_profile(&game.id, "Main", 1).unwrap();
        for path in [
            "", "/abs", "a/../b", "..", "a//b", "a\\b", "C:/x", "./a", "a/",
        ] {
            assert!(
                database
                    .replace_profile_mod_files(&profile.id, "Owner-Mod", &paths(&[path]))
                    .is_err(),
                "{path}"
            );
            assert!(
                database
                    .replace_deployed_files(&game.id, &paths(&[path]))
                    .is_err(),
                "{path}"
            );
        }
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn manages_deployed_files() {
        let (database, directory) = open();
        let game = database.create_game(game_input("Game")).unwrap();
        database
            .replace_deployed_files(&game.id, &paths(&["winhttp.dll", "doorstop_config.ini"]))
            .unwrap();
        assert_eq!(
            database.deployed_files(&game.id).unwrap(),
            ["doorstop_config.ini", "winhttp.dll"]
        );
        database
            .replace_deployed_files(&game.id, &paths(&["winhttp.dll"]))
            .unwrap();
        assert_eq!(database.deployed_files(&game.id).unwrap(), ["winhttp.dll"]);
        database.clear_deployed_files(&game.id).unwrap();
        assert!(database.deployed_files(&game.id).unwrap().is_empty());
        assert!(
            database
                .replace_deployed_files(&Uuid::new_v4().to_string(), &paths(&["a"]))
                .is_err()
        );
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn removing_a_game_removes_its_modding_data() {
        let (database, directory) = open();
        let game = database.create_game(game_input("Game")).unwrap();
        database.set_game_modding(&game.id, "a").unwrap();
        let profile = database.create_mod_profile(&game.id, "Main", 1).unwrap();
        database
            .set_active_mod_profile(&game.id, Some(&profile.id))
            .unwrap();
        database
            .upsert_profile_mod(&mod_record(&profile.id, "Owner-Mod", 0))
            .unwrap();
        database
            .replace_profile_mod_files(&profile.id, "Owner-Mod", &paths(&["BepInEx/plugins/a.dll"]))
            .unwrap();
        database
            .replace_deployed_files(&game.id, &paths(&["winhttp.dll"]))
            .unwrap();

        database.remove_game(&game.id).unwrap();
        for table in [
            "game_modding",
            "mod_profiles",
            "mod_profile_mods",
            "mod_profile_files",
            "game_mod_deployed_files",
        ] {
            assert_eq!(count(&database, table), 0, "{table}");
        }
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
