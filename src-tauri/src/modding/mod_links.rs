//! Thunderstore "Install with Mod Manager" links,
//! `ror2mm://v1/install/thunderstore.io/<owner>/<name>/<version>/`: parsing,
//! the queue the frontend drains, the opt-in scheme registration and the
//! lookup of the library games a link can be installed into.

use std::{collections::HashMap, ffi::OsString, sync::Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_deep_link::DeepLinkExt;

use super::{
    catalog_commands::supports_loader,
    ecosystem::EcosystemSchema,
    ecosystem_store::EcosystemStore,
    package_cache::{PackageCache, PackageVersionRecord},
    package_id::{PackageName, PackageVersionId, Version},
};
use crate::database::{Database, DatabaseState};

pub(crate) const SCHEME: &str = "ror2mm";
const LINK_EVENT: &str = "legio:mod-link";
const PREFIX: &str = "ror2mm://v1/install/";
const MAX_LINK_BYTES: usize = 512;
const MAX_PENDING: usize = 16;

/// The package a link asks for.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModLinkTarget {
    full_name: String,
    owner: String,
    name: String,
    version: String,
}

impl From<&PackageVersionId> for ModLinkTarget {
    fn from(id: &PackageVersionId) -> Self {
        Self {
            full_name: id.package.to_string(),
            owner: id.package.owner.clone(),
            name: id.package.name.clone(),
            version: id.version.to_string(),
        }
    }
}

/// A queued link: either the requested package or why the link was refused.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModLinkRequest {
    target: Option<ModLinkTarget>,
    error: Option<String>,
}

#[derive(Default)]
pub(crate) struct ModLinks {
    requests: Mutex<Vec<ModLinkRequest>>,
}

fn parse(value: &str) -> Result<PackageVersionId, String> {
    let invalid =
        || "Expected ror2mm://v1/install/thunderstore.io/<owner>/<name>/<version>/".to_owned();
    if value.len() > MAX_LINK_BYTES || value.chars().any(char::is_control) {
        return Err(invalid());
    }
    let rest = value
        .get(..PREFIX.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(PREFIX))
        .map(|_| &value[PREFIX.len()..])
        .ok_or_else(invalid)?;
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    let parts: Vec<&str> = rest.split('/').collect();
    let [host, owner, name, version] = parts.as_slice() else {
        return Err(invalid());
    };
    if !is_thunderstore_host(host) {
        return Err(invalid());
    }
    Ok(PackageVersionId::new(
        PackageName::new(owner, name)?,
        version.parse::<Version>()?,
    ))
}

/// `thunderstore.io`, or the same under one community label as old links had.
fn is_thunderstore_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let community = host.strip_suffix("thunderstore.io");
    match community {
        Some("") => true,
        Some(label) => label.strip_suffix('.').is_some_and(|label| {
            !label.is_empty() && label.bytes().all(|byte| byte.is_ascii_alphanumeric())
        }),
        None => false,
    }
}

impl ModLinks {
    /// Queues the `ror2mm://` arguments and reports whether any was found.
    pub(crate) fn enqueue(
        &self,
        arguments: impl IntoIterator<Item = OsString>,
    ) -> Result<bool, String> {
        let mut requests = self
            .requests
            .lock()
            .map_err(|_| "Mod links are unavailable".to_owned())?;
        let mut added = false;
        for argument in arguments {
            let Some(value) = argument.to_str() else {
                continue;
            };
            if !value
                .get(..SCHEME.len() + 1)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("ror2mm:"))
            {
                continue;
            }
            if requests.len() >= MAX_PENDING {
                return Err("Too many pending mod links".to_owned());
            }
            requests.push(match parse(value) {
                Ok(id) => ModLinkRequest {
                    target: Some(ModLinkTarget::from(&id)),
                    error: None,
                },
                Err(error) => ModLinkRequest {
                    target: None,
                    error: Some(error),
                },
            });
            added = true;
        }
        Ok(added)
    }

    fn take(&self) -> Result<Vec<ModLinkRequest>, String> {
        Ok(std::mem::take(
            &mut *self
                .requests
                .lock()
                .map_err(|_| "Mod links are unavailable".to_owned())?,
        ))
    }
}

/// Queues links forwarded by a second instance and tells the frontend.
pub(crate) fn forward(app: &AppHandle, arguments: &[String]) -> Result<(), String> {
    if app
        .state::<ModLinks>()
        .enqueue(arguments.iter().skip(1).map(OsString::from))?
    {
        app.emit(LINK_EVENT, ())
            .map_err(|error| format!("Could not publish mod link: {error}"))?;
    }
    Ok(())
}

/// Claims or releases the `ror2mm` scheme for this app. Another manager's
/// registration is left alone when releasing.
pub(crate) fn set_registered(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let deep_link = app.deep_link();
    if enabled {
        return deep_link
            .register(SCHEME)
            .map_err(|error| format!("Could not register Thunderstore links: {error}"));
    }
    if deep_link.is_registered(SCHEME).unwrap_or(false) {
        deep_link
            .unregister(SCHEME)
            .map_err(|error| format!("Could not release Thunderstore links: {error}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn take_mod_links(state: State<'_, ModLinks>) -> Result<Vec<ModLinkRequest>, String> {
    state.take()
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModLinkPackage {
    description: String,
    icon: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModLinkProfile {
    id: String,
    name: String,
    /// The version the profile has of the package, if any.
    installed_version: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModLinkCandidate {
    game_id: String,
    name: String,
    ecosystem_game_id: String,
    active_profile_id: Option<String>,
    profiles: Vec<ModLinkProfile>,
}

/// A bound game whose catalog was never downloaded, so it cannot be searched.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnsyncedGame {
    game_id: String,
    name: String,
    ecosystem_game_id: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModLinkResolution {
    package: Option<ModLinkPackage>,
    candidates: Vec<ModLinkCandidate>,
    unsynced: Vec<UnsyncedGame>,
}

enum Lookup {
    Unsynced,
    Missing,
    Found(PackageVersionRecord),
}

fn catalog_lookup(
    cache: &PackageCache,
    community: &str,
    id: &PackageVersionId,
) -> Result<Lookup, String> {
    if cache.status(community)?.is_none() {
        return Ok(Lookup::Unsynced);
    }
    Ok(match cache.version(community, id)? {
        Some(record) => Lookup::Found(record),
        None => Lookup::Missing,
    })
}

/// Which moddable library games have `id` in their synced catalog.
fn resolve(
    database: &Database,
    schema: &EcosystemSchema,
    id: &PackageVersionId,
    lookup: impl Fn(&str, &PackageVersionId) -> Result<Lookup, String>,
) -> Result<ModLinkResolution, String> {
    let mut resolution = ModLinkResolution {
        package: None,
        candidates: Vec::new(),
        unsynced: Vec::new(),
    };
    let mut lookups: HashMap<String, Lookup> = HashMap::new();
    for game in database.visible_games()? {
        let Some(binding) = database.game_modding(&game.id)? else {
            continue;
        };
        let community = binding.ecosystem_game_id;
        let supported = schema
            .games
            .get(&community)
            .and_then(|entry| entry.game_entry())
            .is_some_and(|entry| supports_loader(entry.package_loader));
        if !supported {
            continue;
        }
        if !lookups.contains_key(&community) {
            let found = lookup(&community, id)?;
            lookups.insert(community.clone(), found);
        }
        match lookups.get(&community) {
            Some(Lookup::Unsynced) => resolution.unsynced.push(UnsyncedGame {
                game_id: game.id,
                name: game.name,
                ecosystem_game_id: community,
            }),
            Some(Lookup::Found(record)) => {
                resolution.package.get_or_insert_with(|| ModLinkPackage {
                    description: record.description.clone(),
                    icon: record.icon.clone(),
                });
                resolution.candidates.push(ModLinkCandidate {
                    profiles: link_profiles(database, &game.id, id)?,
                    game_id: game.id,
                    name: game.name,
                    ecosystem_game_id: community,
                    active_profile_id: binding.active_profile_id,
                });
            }
            Some(Lookup::Missing) | None => {}
        }
    }
    resolution
        .candidates
        .sort_by_cached_key(|candidate| candidate.name.to_lowercase());
    Ok(resolution)
}

fn link_profiles(
    database: &Database,
    game_id: &str,
    id: &PackageVersionId,
) -> Result<Vec<ModLinkProfile>, String> {
    let full_name = id.package.to_string();
    let mut profiles = database
        .mod_profiles(game_id)?
        .into_iter()
        .map(|profile| {
            let installed_version = database
                .profile_mods(&profile.id)?
                .into_iter()
                .find(|installed| installed.full_name.eq_ignore_ascii_case(&full_name))
                .map(|installed| installed.version);
            Ok(ModLinkProfile {
                id: profile.id,
                name: profile.name,
                installed_version,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    profiles.sort_by_cached_key(|profile| profile.name.to_lowercase());
    Ok(profiles)
}

#[tauri::command]
pub async fn resolve_mod_link(
    state: State<'_, DatabaseState>,
    ecosystem: State<'_, EcosystemStore>,
    packages: State<'_, PackageCache>,
    full_name: String,
    version: String,
) -> Result<ModLinkResolution, String> {
    let id = PackageVersionId::new(full_name.parse()?, version.parse()?);
    let schema = ecosystem.schema().await?;
    let database = state.shared_database()?;
    packages
        .run_blocking(move |cache| {
            resolve(&database, &schema, &id, |community, id| {
                catalog_lookup(cache, community, id)
            })
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{database::CreateGameInput, modding::test_support::TempDir};

    const FIXTURE: &str = include_str!("../../test-fixtures/modding/ecosystem-schema.json");

    fn id(value: &str) -> PackageVersionId {
        value.parse().unwrap()
    }

    #[test]
    fn parses_install_links() {
        for value in [
            "ror2mm://v1/install/thunderstore.io/Owner/Name_1/1.2.3/",
            "ror2mm://v1/install/thunderstore.io/Owner/Name_1/1.2.3",
            "ROR2MM://V1/Install/Thunderstore.IO/Owner/Name_1/1.2.3/",
            "ror2mm://v1/install/valheim.thunderstore.io/Owner/Name_1/1.2.3/",
        ] {
            assert_eq!(parse(value).unwrap(), id("Owner-Name_1-1.2.3"), "{value}");
        }
    }

    #[test]
    fn rejects_other_links() {
        for value in [
            "",
            "ror2mm://",
            "ror2mm://v2/install/thunderstore.io/Owner/Name/1.2.3/",
            "ror2mm://v1/uninstall/thunderstore.io/Owner/Name/1.2.3/",
            "ror2mm://v1/install/example.com/Owner/Name/1.2.3/",
            "ror2mm://v1/install/evilthunderstore.io/Owner/Name/1.2.3/",
            "ror2mm://v1/install/a.b.thunderstore.io/Owner/Name/1.2.3/",
            "ror2mm://v1/install/.thunderstore.io/Owner/Name/1.2.3/",
            "ror2mm://v1/install/thunderstore.io/Owner/Name/",
            "ror2mm://v1/install/thunderstore.io/Owner/Name/1.2/",
            "ror2mm://v1/install/thunderstore.io/Owner/Name/1.2.3/extra/",
            "ror2mm://v1/install/thunderstore.io/Owner/Name/1.2.3//",
            "ror2mm://v1/install/thunderstore.io/Own-er/Name/1.2.3/",
            "ror2mm://v1/install/thunderstore.io/Owner/Na%6De/1.2.3/",
            "ror2mm://v1/install/thunderstore.io/Owner/Name/1.2.3/?a=1",
            "ror2mm://v1/install/thunderstore.io/Owner/Name/1.2.3/#x",
            "ror2mm://v1/install/thunderstore.io/../Name/1.2.3/",
            "ror2mm://v1/install/thunderstore.io/Owner/Name/1.2.3/\n",
            "legio://add-source?url=https://example.invalid/a.json",
        ] {
            assert!(parse(value).is_err(), "{value:?}");
        }
        let long = format!(
            "ror2mm://v1/install/thunderstore.io/Owner/{}/1.0.0/",
            "a".repeat(600)
        );
        assert!(parse(&long).is_err());
    }

    #[test]
    fn startup_and_forwarded_links_are_drained_once() {
        let links = ModLinks::default();
        assert!(
            !links
                .enqueue([
                    OsString::from("--minimized"),
                    OsString::from("legio://add-source?url=https://example.invalid/a.json")
                ])
                .unwrap()
        );
        assert!(
            links
                .enqueue([
                    OsString::from("ror2mm://v1/install/thunderstore.io/Owner/Name/1.2.3/"),
                    OsString::from("ror2mm://bad"),
                ])
                .unwrap()
        );
        let pending = links.take().unwrap();
        assert_eq!(pending.len(), 2);
        let target = pending[0].target.as_ref().unwrap();
        assert_eq!(target.full_name, "Owner-Name");
        assert_eq!(target.owner, "Owner");
        assert_eq!(target.name, "Name");
        assert_eq!(target.version, "1.2.3");
        assert!(pending[0].error.is_none());
        assert!(pending[1].target.is_none());
        assert!(pending[1].error.is_some());
        assert!(links.take().unwrap().is_empty());
    }

    #[test]
    fn limits_pending_links() {
        let links = ModLinks::default();
        let link = OsString::from("ror2mm://v1/install/thunderstore.io/Owner/Name/1.2.3/");
        for _ in 0..MAX_PENDING {
            links.enqueue([link.clone()]).unwrap();
        }
        assert!(links.enqueue([link]).is_err());
    }

    fn record(id: &PackageVersionId) -> PackageVersionRecord {
        PackageVersionRecord {
            id: id.clone(),
            download_url: String::new(),
            file_size: 0,
            dependencies: Vec::new(),
            description: "A mod".to_owned(),
            icon: "https://gcdn.thunderstore.io/icon.png".to_owned(),
        }
    }

    fn add_game(database: &Database, name: &str, ecosystem: Option<&str>) -> String {
        let game = database
            .create_game(CreateGameInput {
                name: name.to_owned(),
                steam_app_id: None,
            })
            .unwrap()
            .id;
        if let Some(ecosystem) = ecosystem {
            database.set_game_modding(&game, ecosystem).unwrap();
        }
        game
    }

    #[test]
    fn resolves_bound_games_with_the_package() {
        let directory = TempDir::new();
        let database = Database::open(directory.path()).unwrap();
        let schema: EcosystemSchema = serde_json::from_str(FIXTURE).unwrap();
        let wanted = id("Owner-Name-1.2.3");

        let lethal = add_game(&database, "Lethal Company", Some("lethal-company"));
        let ror2 = add_game(&database, "Risk of Rain 2", Some("riskofrain2"));
        add_game(&database, "Unbound", None);
        add_game(&database, "Unsupported", Some("backpack-hero"));
        let first = database.create_mod_profile(&lethal, "Zeta", 1).unwrap();
        let second = database.create_mod_profile(&lethal, "Alpha", 2).unwrap();
        database
            .upsert_profile_mod(&crate::database::ProfileMod {
                profile_id: first.id.clone(),
                full_name: "owner-name".to_owned(),
                version: "1.0.0".to_owned(),
                enabled: true,
                online_source: true,
                installed_at: 0,
                install_order: 0,
            })
            .unwrap();
        database
            .set_active_mod_profile(&lethal, Some(&second.id))
            .unwrap();

        let resolution = resolve(&database, &schema, &wanted, |community, id| {
            Ok(match community {
                "lethal-company" => Lookup::Found(record(id)),
                "riskofrain2" => Lookup::Unsynced,
                other => panic!("unexpected lookup for {other}"),
            })
        })
        .unwrap();

        assert_eq!(resolution.package.unwrap().description, "A mod");
        assert_eq!(resolution.candidates.len(), 1);
        let candidate = &resolution.candidates[0];
        assert_eq!(candidate.game_id, lethal);
        assert_eq!(candidate.ecosystem_game_id, "lethal-company");
        assert_eq!(
            candidate.active_profile_id.as_deref(),
            Some(second.id.as_str())
        );
        let profiles: Vec<_> = candidate
            .profiles
            .iter()
            .map(|profile| (profile.name.as_str(), profile.installed_version.as_deref()))
            .collect();
        assert_eq!(profiles, [("Alpha", None), ("Zeta", Some("1.0.0"))]);
        assert_eq!(
            resolution.unsynced,
            [UnsyncedGame {
                game_id: ror2,
                name: "Risk of Rain 2".to_owned(),
                ecosystem_game_id: "riskofrain2".to_owned(),
            }]
        );
    }

    #[test]
    fn omits_games_whose_catalog_lacks_the_package() {
        let directory = TempDir::new();
        let database = Database::open(directory.path()).unwrap();
        let schema: EcosystemSchema = serde_json::from_str(FIXTURE).unwrap();
        add_game(&database, "Lethal Company", Some("lethal-company"));
        let resolution = resolve(&database, &schema, &id("Owner-Name-1.2.3"), |_, _| {
            Ok(Lookup::Missing)
        })
        .unwrap();
        assert_eq!(resolution.package, None);
        assert!(resolution.candidates.is_empty());
        assert!(resolution.unsynced.is_empty());
    }

    #[test]
    fn serializes_camel_case() {
        let value = serde_json::to_value(ModLinkRequest {
            target: Some(ModLinkTarget::from(&id("Owner-Name-1.2.3"))),
            error: None,
        })
        .unwrap();
        assert_eq!(value["target"]["fullName"], "Owner-Name");
        assert_eq!(value["error"], serde_json::Value::Null);
    }
}
