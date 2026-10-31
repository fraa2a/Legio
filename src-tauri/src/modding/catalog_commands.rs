//! Tauri commands for choosing moddable games and browsing the Thunderstore
//! catalog.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::{
    ecosystem::{EcosystemSchema, PackageLoader},
    ecosystem_store::EcosystemStore,
    package_cache::{CatalogStatus, PackageCache, PackageDetails, PackagePage, PackageQuery},
    package_id::PackageName,
    thunderstore_http::ThunderstoreHttp,
};
use crate::{
    database::{Database, DatabaseState, GameModding},
    game_lifecycle::GameLaunchManager,
};

const PROGRESS_EVENT: &str = "modding-catalog-progress";

/// Mods are only installed for BepInEx games in this version.
fn supports_loader(loader: PackageLoader) -> bool {
    loader == PackageLoader::Bepinex
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModdableGame {
    game_id: String,
    name: String,
    steam_app_id: Option<u32>,
    /// The bound Thunderstore game, or the one matching the Steam app.
    ecosystem_game_id: Option<String>,
    display_name: Option<String>,
    package_loader: Option<PackageLoader>,
    supported: bool,
    /// Set once the game was bound; carries the active profile.
    binding: Option<GameModding>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EcosystemGameSummary {
    id: String,
    display_name: String,
    package_loader: PackageLoader,
    supported: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogProgress<'a> {
    community: &'a str,
    chunks_done: u32,
    chunks_total: u32,
}

#[tauri::command]
pub async fn list_moddable_games(
    state: State<'_, DatabaseState>,
    ecosystem: State<'_, EcosystemStore>,
) -> Result<Vec<ModdableGame>, String> {
    let schema = ecosystem.schema().await?;
    let database = state.shared_database()?;
    tauri::async_runtime::spawn_blocking(move || moddable_games(&database, &schema))
        .await
        .map_err(|error| format!("Moddable games task failed: {error}"))?
}

#[tauri::command]
pub async fn list_ecosystem_games(
    ecosystem: State<'_, EcosystemStore>,
) -> Result<Vec<EcosystemGameSummary>, String> {
    let schema = ecosystem.schema().await?;
    Ok(ecosystem_games(&schema))
}

#[tauri::command]
pub async fn bind_game_to_ecosystem(
    app: AppHandle,
    state: State<'_, DatabaseState>,
    ecosystem: State<'_, EcosystemStore>,
    game_id: String,
    ecosystem_game_id: String,
) -> Result<GameModding, String> {
    let schema = ecosystem.schema().await?;
    require_package_index(&schema, &ecosystem_game_id)?;
    let database = state.shared_database()?;
    let binding = tauri::async_runtime::spawn_blocking(move || {
        let is_rebinding = database
            .game_modding(&game_id)?
            .is_some_and(|binding| binding.ecosystem_game_id != ecosystem_game_id);
        if is_rebinding {
            app.state::<GameLaunchManager>().require_idle(&game_id)?;
        }
        database.set_game_modding(&game_id, &ecosystem_game_id)
    })
    .await
    .map_err(|error| format!("Game binding task failed: {error}"))??;
    crate::application_log::state_change(
        "game_modding_bound",
        &format!("game bound to {}", binding.ecosystem_game_id),
    );
    Ok(binding)
}

#[tauri::command]
pub async fn refresh_mod_catalog(
    app: AppHandle,
    ecosystem: State<'_, EcosystemStore>,
    packages: State<'_, PackageCache>,
    http: State<'_, ThunderstoreHttp>,
    ecosystem_game_id: String,
    force: bool,
) -> Result<CatalogStatus, String> {
    let schema = ecosystem.schema().await?;
    let index_url = require_package_index(&schema, &ecosystem_game_id)?;
    let status = packages
        .sync(
            &http,
            &ecosystem_game_id,
            &index_url,
            force,
            |done, total| {
                let progress = CatalogProgress {
                    community: &ecosystem_game_id,
                    chunks_done: done,
                    chunks_total: total,
                };
                if let Err(error) = app.emit(PROGRESS_EVENT, progress) {
                    crate::application_log::failure("publish_mod_catalog_progress", error);
                }
            },
        )
        .await;
    match status {
        Ok(status) => {
            crate::application_log::state_change(
                "mod_catalog_synced",
                &format!("{ecosystem_game_id}: {} packages", status.package_count),
            );
            Ok(status)
        }
        Err(error) => {
            crate::application_log::failure("mod_catalog_sync", &error);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn search_mods(
    packages: State<'_, PackageCache>,
    ecosystem_game_id: String,
    query: PackageQuery,
) -> Result<PackagePage, String> {
    packages
        .run_blocking(move |cache| cache.search(&ecosystem_game_id, &query))
        .await
}

#[tauri::command]
pub async fn get_mod_package(
    packages: State<'_, PackageCache>,
    ecosystem_game_id: String,
    full_name: String,
) -> Result<Option<PackageDetails>, String> {
    let name: PackageName = full_name.parse()?;
    packages
        .run_blocking(move |cache| cache.package(&ecosystem_game_id, &name))
        .await
}

#[tauri::command]
pub async fn list_mod_categories(
    packages: State<'_, PackageCache>,
    ecosystem_game_id: String,
) -> Result<Vec<String>, String> {
    packages
        .run_blocking(move |cache| cache.categories(&ecosystem_game_id))
        .await
}

/// The package index URL of a playable game in the schema.
fn require_package_index(
    schema: &EcosystemSchema,
    ecosystem_game_id: &str,
) -> Result<String, String> {
    schema
        .games
        .get(ecosystem_game_id)
        .and_then(|game| game.game_entry())
        .map(|entry| entry.package_index.clone())
        .ok_or_else(|| format!("\"{ecosystem_game_id}\" is not a known Thunderstore game"))
}

fn ecosystem_games(schema: &EcosystemSchema) -> Vec<EcosystemGameSummary> {
    let mut games: Vec<_> = schema
        .games
        .iter()
        .filter_map(|(id, game)| {
            let entry = game.game_entry()?;
            Some(EcosystemGameSummary {
                id: id.clone(),
                display_name: game.meta.display_name.clone(),
                package_loader: entry.package_loader,
                supported: supports_loader(entry.package_loader),
            })
        })
        .collect();
    games.sort_by_cached_key(|game| (game.display_name.to_lowercase(), game.id.clone()));
    games
}

fn moddable_games(
    database: &Database,
    schema: &EcosystemSchema,
) -> Result<Vec<ModdableGame>, String> {
    database
        .visible_games()?
        .into_iter()
        .map(|game| {
            let binding = database.game_modding(&game.id)?;
            let ecosystem_game_id = match &binding {
                Some(binding) => Some(binding.ecosystem_game_id.as_str()),
                None => game
                    .steam_app_id
                    .and_then(|app_id| schema.find_by_steam_app(app_id).first().copied())
                    .map(|found| found.ecosystem_game_id),
            };
            let ecosystem_game = ecosystem_game_id.and_then(|id| {
                let game = schema.games.get(id)?;
                Some((id, game, game.game_entry()?))
            });
            Ok(ModdableGame {
                game_id: game.id,
                name: game.name,
                steam_app_id: game.steam_app_id,
                ecosystem_game_id: ecosystem_game.map(|(id, _, _)| id.to_owned()),
                display_name: ecosystem_game.map(|(_, game, _)| game.meta.display_name.clone()),
                package_loader: ecosystem_game.map(|(_, _, entry)| entry.package_loader),
                supported: ecosystem_game
                    .is_some_and(|(_, _, entry)| supports_loader(entry.package_loader)),
                binding,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{database::CreateGameInput, modding::test_support::TempDir};

    const FIXTURE: &str = include_str!("../../test-fixtures/modding/ecosystem-schema.json");

    fn schema() -> EcosystemSchema {
        serde_json::from_str(FIXTURE).unwrap()
    }

    fn game(database: &Database, name: &str, steam_app_id: Option<u32>) -> String {
        database
            .create_game(CreateGameInput {
                name: name.to_owned(),
                steam_app_id,
            })
            .unwrap()
            .id
    }

    #[test]
    fn matches_library_games_to_the_ecosystem() {
        let directory = TempDir::new();
        let database = Database::open(directory.path()).unwrap();
        let lethal = game(&database, "Lethal Company", Some(1966720));
        let ror2 = game(&database, "Risk of Rain 2", Some(632360));
        let unknown = game(&database, "Unknown", Some(1));
        let manual = game(&database, "Manual", None);
        database.set_game_modding(&manual, "backpack-hero").unwrap();
        database.set_game_modding(&unknown, "no-such-game").unwrap();

        let games = moddable_games(&database, &schema()).unwrap();
        let find = |id: &str| games.iter().find(|game| game.game_id == id).unwrap();

        let lethal = find(&lethal);
        assert_eq!(lethal.ecosystem_game_id.as_deref(), Some("lethal-company"));
        assert_eq!(lethal.display_name.as_deref(), Some("Lethal Company"));
        assert_eq!(lethal.package_loader, Some(PackageLoader::Bepinex));
        assert!(lethal.supported);
        assert_eq!(lethal.binding, None);

        let ror2 = find(&ror2);
        assert_eq!(ror2.ecosystem_game_id.as_deref(), Some("riskofrain2"));
        assert!(ror2.supported);

        let manual = find(&manual);
        assert_eq!(manual.ecosystem_game_id.as_deref(), Some("backpack-hero"));
        assert_eq!(manual.package_loader, Some(PackageLoader::Melonloader));
        assert!(!manual.supported);
        assert_eq!(manual.binding.as_ref().unwrap().active_profile_id, None);

        let unknown = find(&unknown);
        assert_eq!(unknown.ecosystem_game_id, None);
        assert!(!unknown.supported);
        assert_eq!(
            unknown.binding.as_ref().unwrap().ecosystem_game_id,
            "no-such-game"
        );
    }

    #[test]
    fn a_binding_overrides_the_steam_match() {
        let directory = TempDir::new();
        let database = Database::open(directory.path()).unwrap();
        let lethal = game(&database, "Lethal Company", Some(1966720));
        database.set_game_modding(&lethal, "riskofrain2").unwrap();
        let games = moddable_games(&database, &schema()).unwrap();
        assert_eq!(games[0].ecosystem_game_id.as_deref(), Some("riskofrain2"));
    }

    #[test]
    fn lists_playable_ecosystem_games_by_name() {
        let games = ecosystem_games(&schema());
        let ids: Vec<_> = games.iter().map(|game| game.id.as_str()).collect();
        assert_eq!(ids, ["backpack-hero", "lethal-company", "riskofrain2"]);
        assert!(
            games
                .iter()
                .find(|game| game.id == "lethal-company")
                .unwrap()
                .supported
        );
        assert!(
            !games
                .iter()
                .find(|game| game.id == "backpack-hero")
                .unwrap()
                .supported
        );
    }

    #[test]
    fn validates_ecosystem_ids() {
        let schema = schema();
        assert!(
            require_package_index(&schema, "lethal-company")
                .unwrap()
                .contains("lethal-company")
        );
        assert!(require_package_index(&schema, "nope").is_err());
    }

    #[test]
    fn serializes_camel_case() {
        let value = serde_json::to_value(&ecosystem_games(&schema())[0]).unwrap();
        assert_eq!(value["displayName"], "Backpack Hero");
        assert_eq!(value["packageLoader"], "melonloader");
        assert_eq!(value["supported"], false);
    }
}
