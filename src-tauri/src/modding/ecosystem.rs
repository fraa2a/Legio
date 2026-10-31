//! Subset of the Thunderstore ecosystem schema
//! (`https://thunderstore.io/api/experimental/schema/dev/latest/`) used by Legio.
//! The schema keys games by Thunderstore community id; that id is the
//! `ecosystem_game_id` stored in `game_modding`.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EcosystemSchema {
    pub(crate) games: BTreeMap<String, EcosystemGame>,
    #[serde(default)]
    pub(crate) communities: BTreeMap<String, Community>,
    #[serde(default)]
    pub(crate) modloader_packages: Vec<ModloaderPackage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EcosystemGame {
    pub(crate) uuid: String,
    /// Same value as the key in `EcosystemSchema::games`.
    pub(crate) label: String,
    pub(crate) meta: GameMeta,
    /// Entries Thunderstore mod managers expose for this game. Games without
    /// mod manager support have none.
    #[serde(default, deserialize_with = "null_as_default")]
    pub(crate) r2modman: Vec<GameEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GameMeta {
    pub(crate) display_name: String,
    /// Path relative to the Thunderstore image host.
    #[serde(default)]
    pub(crate) icon_url: Option<String>,
}

/// One mod manager entry of a game (the game itself or a dedicated server).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GameEntry {
    pub(crate) meta: GameMeta,
    pub(crate) internal_folder_name: String,
    #[serde(default)]
    pub(crate) data_folder_name: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(crate) distributions: Vec<Distribution>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(crate) exe_names: Vec<String>,
    /// URL of the community package-listing-index.
    pub(crate) package_index: String,
    pub(crate) package_loader: PackageLoader,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(crate) install_rules: Vec<InstallRule>,
    /// File names mod managers skip when installing package contents.
    #[serde(default, deserialize_with = "null_as_default")]
    pub(crate) relative_file_exclusions: Vec<String>,
    #[serde(default)]
    pub(crate) game_instance_type: GameInstanceType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Distribution {
    /// `steam`, `epic-games-store`, `xbox-game-pass`, `other`, ...
    pub(crate) platform: String,
    pub(crate) identifier: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum GameInstanceType {
    #[default]
    Game,
    Server,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PackageLoader {
    Bepinex,
    Melonloader,
    RecursiveMelonloader,
    Lovely,
    Godotml,
    ReturnOfModding,
    Northstar,
    Shimloader,
    Gdweave,
    Umm,
    Bepisloader,
    Rivet,
    None,
    /// Loader added to the schema after this build.
    #[serde(other)]
    Unknown,
}

/// A node of the install rule tree. `route` is relative to the profile root
/// for top-level rules and to the parent route for `sub_routes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstallRule {
    pub(crate) route: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(crate) default_file_extensions: Vec<String>,
    pub(crate) tracking_method: TrackingMethod,
    #[serde(default)]
    pub(crate) is_default_location: bool,
    #[serde(default, deserialize_with = "null_as_default")]
    pub(crate) sub_routes: Vec<InstallRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TrackingMethod {
    Subdir,
    SubdirNoFlatten,
    State,
    PackageZip,
    #[serde(rename = "none")]
    Untracked,
    /// Method added to the schema after this build.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Community {
    pub(crate) display_name: String,
}

/// A mod loader package, installed from `root_folder` of its archive into the
/// profile root. The same package id can appear once per loader kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModloaderPackage {
    pub(crate) package_id: String,
    pub(crate) root_folder: String,
    pub(crate) loader: PackageLoader,
}

/// A playable mod manager entry matched to a store identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EcosystemMatch<'a> {
    pub(crate) ecosystem_game_id: &'a str,
    pub(crate) game: &'a EcosystemGame,
    pub(crate) entry: &'a GameEntry,
}

impl EcosystemSchema {
    /// Playable game entries distributed on Steam under `app_id`. Dedicated
    /// server entries are skipped. Results follow the schema's key order.
    pub(crate) fn find_by_steam_app(&self, app_id: u32) -> Vec<EcosystemMatch<'_>> {
        let identifier = app_id.to_string();
        self.games
            .iter()
            .flat_map(|(id, game)| {
                let identifier = identifier.as_str();
                game.r2modman
                    .iter()
                    .filter(|entry| entry.game_instance_type == GameInstanceType::Game)
                    .filter(move |entry| {
                        entry.distributions.iter().any(|distribution| {
                            distribution.platform == "steam"
                                && distribution.identifier == identifier
                        })
                    })
                    .map(move |entry| EcosystemMatch {
                        ecosystem_game_id: id,
                        game,
                        entry,
                    })
            })
            .collect()
    }
}

impl EcosystemGame {
    /// The playable game entry, as opposed to a dedicated server entry.
    pub(crate) fn game_entry(&self) -> Option<&GameEntry> {
        self.r2modman
            .iter()
            .find(|entry| entry.game_instance_type == GameInstanceType::Game)
    }
}

fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../test-fixtures/modding/ecosystem-schema.json");

    fn schema() -> EcosystemSchema {
        serde_json::from_str(FIXTURE).unwrap()
    }

    #[test]
    fn parses_the_schema_fixture() {
        let schema = schema();
        assert_eq!(schema.games.len(), 3);
        assert_eq!(
            schema.communities["lethal-company"].display_name,
            "Lethal Company"
        );
        assert!(schema.modloader_packages.iter().any(|package| {
            package.package_id == "BepInEx-BepInExPack"
                && package.root_folder == "BepInExPack"
                && package.loader == PackageLoader::Bepinex
        }));

        let lethal = schema.games["lethal-company"].game_entry().unwrap();
        assert_eq!(lethal.internal_folder_name, "LethalCompany");
        assert_eq!(lethal.data_folder_name, "Lethal Company_Data");
        assert_eq!(lethal.exe_names, ["Lethal Company.exe"]);
        assert_eq!(lethal.package_loader, PackageLoader::Bepinex);
        assert_eq!(lethal.meta.display_name, "Lethal Company");
        assert!(lethal.relative_file_exclusions.is_empty());
        let plugins = &lethal.install_rules[0];
        assert_eq!(plugins.route, "BepInEx/plugins");
        assert_eq!(plugins.tracking_method, TrackingMethod::Subdir);
        assert_eq!(plugins.default_file_extensions, [".dll"]);
        assert!(plugins.is_default_location);
        assert_eq!(
            lethal.install_rules.last().unwrap().tracking_method,
            TrackingMethod::Untracked
        );
    }

    #[test]
    fn parses_nested_rules_and_exclusions() {
        let schema = schema();
        let entry = schema.games["backpack-hero"].game_entry().unwrap();
        assert_eq!(entry.package_loader, PackageLoader::Melonloader);
        assert_eq!(entry.relative_file_exclusions[0], "manifest.json");
        let rule = entry
            .install_rules
            .iter()
            .find(|rule| !rule.sub_routes.is_empty())
            .unwrap();
        assert_eq!(rule.tracking_method, TrackingMethod::State);
        assert_eq!(rule.sub_routes[0].route, "Managed");
    }

    #[test]
    fn finds_games_by_steam_app() {
        let schema = schema();
        let matches = schema.find_by_steam_app(1966720);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].ecosystem_game_id, "lethal-company");
        assert_eq!(matches[0].entry.internal_folder_name, "LethalCompany");
        assert_eq!(
            schema.find_by_steam_app(632360)[0].ecosystem_game_id,
            "riskofrain2"
        );
        assert!(schema.find_by_steam_app(1).is_empty());
    }

    #[test]
    fn skips_dedicated_server_entries() {
        // 1180760 is the Risk of Rain 2 dedicated server.
        assert!(schema().find_by_steam_app(1180760).is_empty());
    }

    #[test]
    fn tolerates_null_fields_and_unknown_variants() {
        let schema: EcosystemSchema = serde_json::from_str(
            r#"{"games":{
            "a":{"uuid":"u","label":"a","meta":{"displayName":"A"},"r2modman":null},
            "b":{"uuid":"v","label":"b","meta":{"displayName":"B","iconUrl":"b/i.webp"},
            "r2modman":[{"meta":{"displayName":"B"},"internalFolderName":"B",
            "packageIndex":"https://thunderstore.io/c/b/api/v1/package-listing-index/",
            "packageLoader":"future-loader","gameInstanceType":"future",
            "installRules":[{"route":"x","trackingMethod":"future"}],
            "relativeFileExclusions":null}]}}}"#,
        )
        .unwrap();
        assert!(schema.games["a"].r2modman.is_empty());
        assert!(schema.games["a"].game_entry().is_none());
        let entry = &schema.games["b"].r2modman[0];
        assert_eq!(entry.package_loader, PackageLoader::Unknown);
        assert_eq!(entry.game_instance_type, GameInstanceType::Unknown);
        assert_eq!(
            entry.install_rules[0].tracking_method,
            TrackingMethod::Unknown
        );
        assert!(entry.relative_file_exclusions.is_empty());
    }
}
