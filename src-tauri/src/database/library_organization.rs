use super::{Database, database_error, parse_game_id};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub(crate) const MAX_COLLECTION_NAME_CHARS: usize = 64;
const MAX_COLLECTIONS: usize = 256;
// One library card groups the versions of a game, so a call carries a few IDs.
const MAX_GAME_IDS: usize = 256;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum PlayStatus {
    #[default]
    None,
    Backlog,
    Playing,
    Completed,
    OnHold,
    Dropped,
}

impl PlayStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Backlog => "backlog",
            Self::Playing => "playing",
            Self::Completed => "completed",
            Self::OnHold => "on_hold",
            Self::Dropped => "dropped",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "none" => Ok(Self::None),
            "backlog" => Ok(Self::Backlog),
            "playing" => Ok(Self::Playing),
            "completed" => Ok(Self::Completed),
            "on_hold" => Ok(Self::OnHold),
            "dropped" => Ok(Self::Dropped),
            _ => Err(format!("stored play status is invalid: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameOrganization {
    pub game_id: String,
    pub favorite: bool,
    pub play_status: PlayStatus,
    /// Milliseconds since the Unix epoch; unknown for games added before schema 40.
    pub added_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub name: String,
    /// Member games in the order they were added.
    pub game_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryOrganization {
    pub games: Vec<GameOrganization>,
    pub collections: Vec<Collection>,
}

impl Database {
    pub fn library_organization(&self) -> Result<LibraryOrganization, String> {
        self.with_connection(load_organization)
    }

    pub fn set_games_favorite(
        &self,
        game_ids: &[String],
        favorite: bool,
    ) -> Result<LibraryOrganization, String> {
        let ids = parse_game_ids(game_ids)?;
        self.with_connection(|connection| {
            let transaction = connection.unchecked_transaction().map_err(database_error)?;
            require_games(&transaction, &ids)?;
            for id in &ids {
                transaction
                    .execute(
                        "UPDATE games SET favorite = ?2 WHERE id = ?1",
                        params![id, favorite],
                    )
                    .map_err(database_error)?;
            }
            transaction.commit().map_err(database_error)?;
            load_organization(connection)
        })
    }

    pub fn set_games_play_status(
        &self,
        game_ids: &[String],
        status: PlayStatus,
    ) -> Result<LibraryOrganization, String> {
        let ids = parse_game_ids(game_ids)?;
        self.with_connection(|connection| {
            let transaction = connection.unchecked_transaction().map_err(database_error)?;
            require_games(&transaction, &ids)?;
            for id in &ids {
                transaction
                    .execute(
                        "UPDATE games SET play_status = ?2 WHERE id = ?1",
                        params![id, status.as_str()],
                    )
                    .map_err(database_error)?;
            }
            transaction.commit().map_err(database_error)?;
            load_organization(connection)
        })
    }

    /// Makes `collection_ids` the exact set of collections holding each game.
    /// Kept memberships keep their position; new ones go to the end.
    pub fn set_games_collections(
        &self,
        game_ids: &[String],
        collection_ids: &[String],
    ) -> Result<LibraryOrganization, String> {
        let ids = parse_game_ids(game_ids)?;
        let collections = parse_collection_ids(collection_ids, false)?;
        self.with_connection(|connection| {
            let transaction = connection.unchecked_transaction().map_err(database_error)?;
            require_games(&transaction, &ids)?;
            for collection in &collections {
                require_collection(&transaction, collection)?;
            }
            for id in &ids {
                let current = member_collections(&transaction, id)?;
                for collection in current.iter().filter(|item| !collections.contains(item)) {
                    transaction
                        .execute(
                            "DELETE FROM collection_games WHERE collection_id = ?1 AND game_id = ?2",
                            params![collection, id],
                        )
                        .map_err(database_error)?;
                }
                for collection in collections.iter().filter(|item| !current.contains(item)) {
                    transaction
                        .execute(
                            "INSERT INTO collection_games (collection_id, game_id, position)
                             SELECT ?1, ?2, COALESCE(MAX(position) + 1, 0)
                             FROM collection_games WHERE collection_id = ?1",
                            params![collection, id],
                        )
                        .map_err(database_error)?;
                }
            }
            transaction.commit().map_err(database_error)?;
            load_organization(connection)
        })
    }

    pub fn create_collection(&self, name: &str) -> Result<LibraryOrganization, String> {
        let name = validate_collection_name(name)?;
        self.with_connection(|connection| {
            let transaction = connection.unchecked_transaction().map_err(database_error)?;
            let count: i64 = transaction
                .query_row("SELECT COUNT(*) FROM collections", [], |row| row.get(0))
                .map_err(database_error)?;
            if usize::try_from(count).map_or(true, |count| count >= MAX_COLLECTIONS) {
                return Err(format!(
                    "A library can have at most {MAX_COLLECTIONS} collections"
                ));
            }
            ensure_unique_name(&transaction, &name, None)?;
            transaction
                .execute(
                    "INSERT INTO collections (id, name, position)
                     SELECT ?1, ?2, COALESCE(MAX(position) + 1, 0) FROM collections",
                    params![Uuid::new_v4().to_string(), name],
                )
                .map_err(database_error)?;
            transaction.commit().map_err(database_error)?;
            load_organization(connection)
        })
    }

    pub fn rename_collection(&self, id: &str, name: &str) -> Result<LibraryOrganization, String> {
        let id = parse_collection_id(id)?;
        let name = validate_collection_name(name)?;
        self.with_connection(|connection| {
            let transaction = connection.unchecked_transaction().map_err(database_error)?;
            require_collection(&transaction, &id)?;
            ensure_unique_name(&transaction, &name, Some(&id))?;
            transaction
                .execute(
                    "UPDATE collections SET name = ?2 WHERE id = ?1",
                    params![id, name],
                )
                .map_err(database_error)?;
            transaction.commit().map_err(database_error)?;
            load_organization(connection)
        })
    }

    pub fn delete_collection(&self, id: &str) -> Result<LibraryOrganization, String> {
        let id = parse_collection_id(id)?;
        self.with_connection(|connection| {
            let changed = connection
                .execute("DELETE FROM collections WHERE id = ?1", [&id])
                .map_err(database_error)?;
            if changed == 0 {
                return Err("collection was not found".to_owned());
            }
            load_organization(connection)
        })
    }

    /// Stores a new order; `ids` must list every collection exactly once.
    pub fn reorder_collections(&self, ids: &[String]) -> Result<LibraryOrganization, String> {
        let ids = parse_collection_ids(ids, true)?;
        self.with_connection(|connection| {
            let transaction = connection.unchecked_transaction().map_err(database_error)?;
            let mut existing = collection_ids(&transaction)?;
            let mut requested = ids.clone();
            existing.sort_unstable();
            requested.sort_unstable();
            if existing != requested {
                return Err("The new order must list every collection exactly once".to_owned());
            }
            for (position, id) in ids.iter().enumerate() {
                let position = i64::try_from(position)
                    .map_err(|_| "collection position exceeds platform limits".to_owned())?;
                transaction
                    .execute(
                        "UPDATE collections SET position = ?2 WHERE id = ?1",
                        params![id, position],
                    )
                    .map_err(database_error)?;
            }
            transaction.commit().map_err(database_error)?;
            load_organization(connection)
        })
    }
}

fn load_organization(connection: &Connection) -> Result<LibraryOrganization, String> {
    let mut statement = connection
        .prepare("SELECT id, favorite, play_status, added_at FROM games ORDER BY id")
        .map_err(database_error)?;
    let games = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })
        .map_err(database_error)?
        .map(|row| {
            let (game_id, favorite, status, added_at) = row.map_err(database_error)?;
            Ok(GameOrganization {
                game_id,
                favorite,
                play_status: PlayStatus::parse(&status)?,
                added_at,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut statement = connection
        .prepare("SELECT id, name FROM collections ORDER BY position, id")
        .map_err(database_error)?;
    let mut collections = statement
        .query_map([], |row| {
            Ok(Collection {
                id: row.get(0)?,
                name: row.get(1)?,
                game_ids: Vec::new(),
            })
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    let mut members = connection
        .prepare(
            "SELECT game_id FROM collection_games WHERE collection_id = ?1 ORDER BY position, game_id",
        )
        .map_err(database_error)?;
    for collection in &mut collections {
        collection.game_ids = members
            .query_map([&collection.id], |row| row.get(0))
            .map_err(database_error)?
            .collect::<Result<_, _>>()
            .map_err(database_error)?;
    }
    Ok(LibraryOrganization { games, collections })
}

fn validate_collection_name(value: &str) -> Result<String, String> {
    let name = value.trim();
    let length = name.chars().count();
    if length == 0 || length > MAX_COLLECTION_NAME_CHARS {
        return Err(format!(
            "Collection name must be between 1 and {MAX_COLLECTION_NAME_CHARS} characters"
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("Collection name cannot contain control characters".to_owned());
    }
    Ok(name.to_owned())
}

fn ensure_unique_name(
    connection: &Connection,
    name: &str,
    except: Option<&str>,
) -> Result<(), String> {
    let mut statement = connection
        .prepare("SELECT id, name FROM collections")
        .map_err(database_error)?;
    let folded = name.to_lowercase();
    let duplicate = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?
        .into_iter()
        .any(|(id, existing)| Some(id.as_str()) != except && existing.to_lowercase() == folded);
    if duplicate {
        return Err("A collection with this name already exists".to_owned());
    }
    Ok(())
}

fn parse_game_ids(values: &[String]) -> Result<Vec<String>, String> {
    if values.is_empty() || values.len() > MAX_GAME_IDS {
        return Err(format!("Select between 1 and {MAX_GAME_IDS} games"));
    }
    let mut ids = Vec::with_capacity(values.len());
    for value in values {
        let id = parse_game_id(value)?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

fn parse_collection_id(value: &str) -> Result<String, String> {
    Uuid::parse_str(value)
        .map(|id| id.to_string())
        .map_err(|_| "collection id is invalid".to_owned())
}

fn parse_collection_ids(values: &[String], reject_duplicates: bool) -> Result<Vec<String>, String> {
    if values.len() > MAX_COLLECTIONS {
        return Err(format!(
            "A library can have at most {MAX_COLLECTIONS} collections"
        ));
    }
    let mut ids = Vec::with_capacity(values.len());
    for value in values {
        let id = parse_collection_id(value)?;
        if ids.contains(&id) {
            if reject_duplicates {
                return Err("The new order must list every collection exactly once".to_owned());
            }
            continue;
        }
        ids.push(id);
    }
    Ok(ids)
}

fn require_games(connection: &Connection, ids: &[String]) -> Result<(), String> {
    let placeholders = vec!["?"; ids.len()].join(",");
    let found: i64 = connection
        .query_row(
            &format!("SELECT COUNT(*) FROM games WHERE id IN ({placeholders})"),
            params_from_iter(ids),
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if usize::try_from(found).ok() != Some(ids.len()) {
        return Err("game was not found".to_owned());
    }
    Ok(())
}

fn require_collection(connection: &Connection, id: &str) -> Result<(), String> {
    connection
        .query_row("SELECT 1 FROM collections WHERE id = ?1", [id], |_| Ok(()))
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| "collection was not found".to_owned())
}

fn collection_ids(connection: &Connection) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare("SELECT id FROM collections")
        .map_err(database_error)?;
    statement
        .query_map([], |row| row.get(0))
        .map_err(database_error)?
        .collect::<Result<_, _>>()
        .map_err(database_error)
}

fn member_collections(connection: &Connection, game_id: &str) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare("SELECT collection_id FROM collection_games WHERE game_id = ?1")
        .map_err(database_error)?;
    statement
        .query_map([game_id], |row| row.get(0))
        .map_err(database_error)?
        .collect::<Result<_, _>>()
        .map_err(database_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::CreateGameInput;
    use std::fs;

    struct Fixture(std::path::PathBuf);

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn open() -> (Fixture, Database) {
        let directory = std::env::temp_dir().join(format!("legio-organization-{}", Uuid::new_v4()));
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

    fn entry<'a>(organization: &'a LibraryOrganization, id: &str) -> &'a GameOrganization {
        organization
            .games
            .iter()
            .find(|game| game.game_id == id)
            .unwrap()
    }

    #[test]
    fn favorites_and_status_apply_to_every_listed_game() {
        let (_fixture, database) = open();
        let first = game(&database, "First");
        let second = game(&database, "Second");
        let organization = database.library_organization().unwrap();
        assert!(!entry(&organization, &first).favorite);
        assert_eq!(entry(&organization, &first).play_status, PlayStatus::None);
        assert!(entry(&organization, &first).added_at.is_some());

        let ids = [first.clone(), second.clone()];
        database.set_games_favorite(&ids, true).unwrap();
        let organization = database
            .set_games_play_status(&ids, PlayStatus::OnHold)
            .unwrap();
        for id in &ids {
            assert!(entry(&organization, id).favorite);
            assert_eq!(entry(&organization, id).play_status, PlayStatus::OnHold);
        }
        assert_eq!(
            database
                .set_games_favorite(&[first.clone(), Uuid::new_v4().to_string()], false)
                .unwrap_err(),
            "game was not found"
        );
        assert!(entry(&database.library_organization().unwrap(), &first).favorite);
        assert!(database.set_games_favorite(&[], true).is_err());
        assert!(
            database
                .set_games_favorite(&["not-an-id".to_owned()], true)
                .is_err()
        );
    }

    #[test]
    fn collection_names_are_trimmed_unique_and_bounded() {
        let (_fixture, database) = open();
        let organization = database.create_collection("  Co-op  ").unwrap();
        assert_eq!(organization.collections[0].name, "Co-op");
        assert_eq!(
            database.create_collection("CO-OP").unwrap_err(),
            "A collection with this name already exists"
        );
        assert!(database.create_collection("   ").is_err());
        assert!(database.create_collection("Bad\nname").is_err());
        assert!(
            database
                .create_collection(&"x".repeat(MAX_COLLECTION_NAME_CHARS + 1))
                .is_err()
        );
        database
            .create_collection(&"é".repeat(MAX_COLLECTION_NAME_CHARS))
            .unwrap();

        let id = organization.collections[0].id.clone();
        let renamed = database.rename_collection(&id, "co-op").unwrap();
        assert_eq!(renamed.collections[0].name, "co-op");
        let other = database.create_collection("Other").unwrap().collections[2]
            .id
            .clone();
        assert!(database.rename_collection(&other, "Co-op").is_err());
        assert!(
            database
                .rename_collection(&Uuid::new_v4().to_string(), "Missing")
                .is_err()
        );
    }

    #[test]
    fn membership_keeps_order_and_cascades_on_removal() {
        let (_fixture, database) = open();
        let first = game(&database, "First");
        let second = game(&database, "Second");
        database.create_collection("A").unwrap();
        let organization = database.create_collection("B").unwrap();
        let a = organization.collections[0].id.clone();
        let b = organization.collections[1].id.clone();

        database
            .set_games_collections(std::slice::from_ref(&second), &[a.clone(), b.clone()])
            .unwrap();
        let organization = database
            .set_games_collections(std::slice::from_ref(&first), std::slice::from_ref(&a))
            .unwrap();
        assert_eq!(
            organization.collections[0].game_ids,
            [second.clone(), first.clone()]
        );
        assert_eq!(organization.collections[1].game_ids, [second.clone()]);

        // Keeping a membership keeps its position.
        let organization = database
            .set_games_collections(std::slice::from_ref(&second), std::slice::from_ref(&a))
            .unwrap();
        assert_eq!(
            organization.collections[0].game_ids,
            [second.clone(), first.clone()]
        );
        assert!(organization.collections[1].game_ids.is_empty());
        assert!(
            database
                .set_games_collections(std::slice::from_ref(&first), &[Uuid::new_v4().to_string()])
                .is_err()
        );

        database.remove_game(&second).unwrap();
        let organization = database.library_organization().unwrap();
        assert_eq!(organization.collections[0].game_ids, [first.clone()]);
        assert!(organization.games.iter().all(|game| game.game_id != second));

        let organization = database.delete_collection(&a).unwrap();
        assert_eq!(organization.collections.len(), 1);
        assert!(database.delete_collection(&a).is_err());
        assert!(database.library_organization().unwrap().games.len() == 1);
    }

    #[test]
    fn reorder_requires_every_collection_once() {
        let (_fixture, database) = open();
        for name in ["A", "B", "C"] {
            database.create_collection(name).unwrap();
        }
        let ids: Vec<String> = database
            .library_organization()
            .unwrap()
            .collections
            .into_iter()
            .map(|collection| collection.id)
            .collect();
        let reversed: Vec<String> = ids.iter().rev().cloned().collect();
        let organization = database.reorder_collections(&reversed).unwrap();
        let names: Vec<&str> = organization
            .collections
            .iter()
            .map(|collection| collection.name.as_str())
            .collect();
        assert_eq!(names, ["C", "B", "A"]);
        assert!(database.reorder_collections(&ids[..2]).is_err());
        assert!(
            database
                .reorder_collections(&[ids[0].clone(), ids[0].clone(), ids[1].clone()])
                .is_err()
        );
    }

    #[test]
    fn play_status_serializes_in_camel_case() {
        assert_eq!(
            serde_json::to_string(&PlayStatus::OnHold).unwrap(),
            "\"onHold\""
        );
        for status in [
            PlayStatus::None,
            PlayStatus::Backlog,
            PlayStatus::Playing,
            PlayStatus::Completed,
            PlayStatus::OnHold,
            PlayStatus::Dropped,
        ] {
            assert_eq!(PlayStatus::parse(status.as_str()).unwrap(), status);
        }
    }
}
