//! Maps the files of an extracted Thunderstore package to profile locations
//! using the ecosystem install rules. Port of r2modmanPlus
//! `InstallationRules.ts` and `InstallRulePluginInstaller.ts`; nothing here
//! touches the profile, the caller copies the planned files.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use super::ecosystem::{InstallRule, TrackingMethod};
use super::package_id::PackageName;

/// One file to copy from an extracted package into a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedFile {
    pub(crate) source: PathBuf,
    /// `/`-separated path relative to the profile directory.
    pub(crate) destination: String,
    /// False for files left in place on uninstall (the `none` tracking
    /// method, used for configuration files).
    pub(crate) tracked: bool,
}

struct ManagedRule<'a> {
    route: String,
    tracking: TrackingMethod,
    extensions: &'a [String],
    is_default_location: bool,
}

struct Entry {
    path: PathBuf,
    name: String,
    /// `/`-separated path relative to the package root.
    relative: String,
}

#[derive(Default)]
struct Intents {
    order: Vec<usize>,
    sources: HashMap<usize, Vec<Entry>>,
}

impl Intents {
    fn push(&mut self, rule: usize, entry: Entry) {
        if !self.sources.contains_key(&rule) {
            self.order.push(rule);
        }
        self.sources.entry(rule).or_default().push(entry);
    }
}

/// Plans the installation of the package extracted at `package_root`.
pub(crate) fn plan_install(
    rules: &[InstallRule],
    exclusions: &[String],
    package: &PackageName,
    package_root: &Path,
) -> Result<Vec<PlannedFile>, String> {
    let mut managed = Vec::new();
    flatten_rules(rules, "", &mut managed)?;
    if managed.is_empty() {
        return Err("This game has no install rules for mods".to_owned());
    }
    let mut intents = Intents::default();
    assign_entries(&managed, package_root, "", &mut intents)?;

    let mut planned = Vec::new();
    for rule_index in &intents.order {
        let rule = &managed[*rule_index];
        let entries = &intents.sources[rule_index];
        expand_rule(rule, entries, exclusions, package, &mut planned)?;
    }
    Ok(dedupe_destinations(planned))
}

/// Every route of the rule tree with the parent routes joined in, as
/// `getAllManagedPaths` does.
fn flatten_rules<'a>(
    rules: &'a [InstallRule],
    parent: &str,
    out: &mut Vec<ManagedRule<'a>>,
) -> Result<(), String> {
    for rule in rules {
        let route = join_route(parent, &rule.route)?;
        out.push(ManagedRule {
            route: route.clone(),
            tracking: rule.tracking_method,
            extensions: &rule.default_file_extensions,
            is_default_location: rule.is_default_location,
        });
        flatten_rules(&rule.sub_routes, &route, out)?;
    }
    Ok(())
}

fn join_route(parent: &str, route: &str) -> Result<String, String> {
    let mut parts: Vec<&str> = parent.split('/').filter(|part| !part.is_empty()).collect();
    for part in route.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".."
            || part.chars().any(char::is_control)
            || (parts.is_empty() && part.contains(':'))
        {
            return Err(format!("Install rule route \"{route}\" is not allowed"));
        }
        parts.push(part);
    }
    Ok(parts.join("/"))
}

fn assign_entries(
    rules: &[ManagedRule<'_>],
    directory: &Path,
    relative: &str,
    intents: &mut Intents,
) -> Result<(), String> {
    let (files, directories) = read_directory(directory, relative)?;
    for file in files {
        if let Some(rule) = rule_for_file(rules, &file.name) {
            intents.push(rule, file);
        }
    }
    for subdirectory in directories {
        match rule_for_directory(rules, &subdirectory) {
            Some(rule) => intents.push(rule, subdirectory),
            None => assign_entries(rules, &subdirectory.path, &subdirectory.relative, intents)?,
        }
    }
    Ok(())
}

/// Extension rules win over the default location; a longer matching
/// extension (`.mm.dll`) beats a shorter one (`.dll`), and on equal length the
/// later rule wins.
fn rule_for_file(rules: &[ManagedRule<'_>], name: &str) -> Option<usize> {
    let lower = name.to_lowercase();
    let mut best: Option<(usize, usize)> = None;
    for (index, rule) in rules.iter().enumerate() {
        let matching = rule
            .extensions
            .iter()
            .find(|extension| !extension.is_empty() && lower.ends_with(&extension.to_lowercase()));
        if let Some(extension) = matching
            && !best.is_some_and(|(_, length)| length > extension.len())
        {
            best = Some((index, extension.len()));
        }
    }
    best.map(|(index, _)| index)
        .or_else(|| rules.iter().position(|rule| rule.is_default_location))
}

/// Rules whose last route segment equals the directory name, compared case
/// insensitively. With several, the one sharing the most trailing path
/// segments with the directory wins and the first wins ties.
fn rule_for_directory(rules: &[ManagedRule<'_>], directory: &Entry) -> Option<usize> {
    let name = directory.name.to_lowercase();
    let candidates: Vec<usize> = rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| route_basename(&rule.route).to_lowercase() == name)
        .map(|(index, _)| index)
        .collect();
    let directory_parts: Vec<&str> = directory.relative.rsplit('/').collect();
    let mut best: Option<(usize, usize)> = None;
    for index in candidates {
        let matches = rules[index]
            .route
            .rsplit('/')
            .zip(&directory_parts)
            .filter(|(rule_part, directory_part)| rule_part == *directory_part)
            .count();
        if !best.is_some_and(|(_, count)| count >= matches) {
            best = Some((index, matches));
        }
    }
    best.map(|(index, _)| index)
}

fn route_basename(route: &str) -> &str {
    route.rsplit('/').next().unwrap_or(route)
}

fn expand_rule(
    rule: &ManagedRule<'_>,
    entries: &[Entry],
    exclusions: &[String],
    package: &PackageName,
    out: &mut Vec<PlannedFile>,
) -> Result<(), String> {
    let subdirectory = join_destination(&rule.route, &package.to_string());
    match rule.tracking {
        TrackingMethod::Subdir => {
            for entry in entries {
                place(entry, &subdirectory, Placement::Flatten, true, out)?;
            }
        }
        TrackingMethod::SubdirNoFlatten => {
            for entry in entries {
                place(entry, &subdirectory, Placement::KeepPath, true, out)?;
            }
        }
        TrackingMethod::Untracked => {
            for entry in entries {
                place(entry, &rule.route, Placement::Flatten, false, out)?;
            }
        }
        TrackingMethod::State => {
            for entry in entries {
                let mut placed = Vec::new();
                place(entry, &rule.route, Placement::Flatten, true, &mut placed)?;
                out.extend(
                    placed
                        .into_iter()
                        .filter(|file| !is_excluded(file, &rule.route, entry, exclusions)),
                );
            }
        }
        TrackingMethod::PackageZip => {
            return Err(format!(
                "Install route \"{}\" repackages the mod as a zip, which is not supported yet",
                rule.route
            ));
        }
        TrackingMethod::Unknown => {
            return Err(format!(
                "Install route \"{}\" uses a tracking method this version does not know",
                rule.route
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Placement {
    /// A file lands directly in the destination; a directory contributes its
    /// contents.
    Flatten,
    /// A file keeps its path relative to the package root.
    KeepPath,
}

fn place(
    entry: &Entry,
    destination: &str,
    placement: Placement,
    tracked: bool,
    out: &mut Vec<PlannedFile>,
) -> Result<(), String> {
    if entry.path.is_dir() {
        for (relative, source) in walk_files(&entry.path)? {
            out.push(PlannedFile {
                source,
                destination: join_destination(destination, &relative),
                tracked,
            });
        }
        return Ok(());
    }
    let relative = match placement {
        Placement::Flatten => &entry.name,
        Placement::KeepPath => &entry.relative,
    };
    out.push(PlannedFile {
        source: entry.path.clone(),
        destination: join_destination(destination, relative),
        tracked,
    });
    Ok(())
}

/// `relative_file_exclusions` apply to the state tracking method only, as in
/// r2modman. A file is excluded by its name or, inside a directory entry, by
/// its path within that directory.
fn is_excluded(file: &PlannedFile, route: &str, entry: &Entry, exclusions: &[String]) -> bool {
    let candidate = if entry.path.is_dir() {
        file.destination
            .strip_prefix(route)
            .map_or(file.destination.as_str(), |rest| {
                rest.trim_start_matches('/')
            })
    } else {
        entry.name.as_str()
    };
    exclusions
        .iter()
        .any(|exclusion| exclusion.to_lowercase() == candidate.to_lowercase())
}

fn join_destination(base: &str, relative: &str) -> String {
    if base.is_empty() {
        relative.to_owned()
    } else {
        format!("{base}/{relative}")
    }
}

/// A later file replaces an earlier one with the same destination, as the
/// second copy overwrites the first in r2modman. Names compare ignoring case
/// because the profile may live on a case-insensitive file system.
fn dedupe_destinations(planned: Vec<PlannedFile>) -> Vec<PlannedFile> {
    let mut positions: HashMap<String, usize> = HashMap::new();
    let mut result: Vec<PlannedFile> = Vec::new();
    for file in planned {
        let key = file.destination.to_lowercase();
        match positions.get(&key) {
            Some(position) => result[*position] = file,
            None => {
                positions.insert(key, result.len());
                result.push(file);
            }
        }
    }
    result
}

fn read_directory(directory: &Path, relative: &str) -> Result<(Vec<Entry>, Vec<Entry>), String> {
    let mut files = Vec::new();
    let mut directories = Vec::new();
    for item in fs::read_dir(directory)
        .map_err(|error| format!("Cannot read {}: {error}", directory.display()))?
    {
        let item = item.map_err(|error| format!("Cannot read {}: {error}", directory.display()))?;
        let name = item
            .file_name()
            .into_string()
            .map_err(|name| format!("Package contains a file name that is not text: {name:?}"))?;
        let file_type = item
            .file_type()
            .map_err(|error| format!("Cannot inspect {}: {error}", item.path().display()))?;
        let entry = Entry {
            path: item.path(),
            relative: join_destination(relative, &name),
            name,
        };
        if file_type.is_file() {
            files.push(entry);
        } else if file_type.is_dir() {
            directories.push(entry);
        } else {
            return Err(format!(
                "Package contains a link or special file: {}",
                entry.relative
            ));
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    directories.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((files, directories))
}

/// Every file below `directory` as (path relative to it, absolute path),
/// sorted for stable results.
pub(crate) fn walk_files(directory: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut result = Vec::new();
    collect_files(directory, "", &mut result)?;
    Ok(result)
}

fn collect_files(
    directory: &Path,
    relative: &str,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    let (files, directories) = read_directory(directory, relative)?;
    out.extend(files.into_iter().map(|file| (file.relative, file.path)));
    for subdirectory in directories {
        collect_files(&subdirectory.path, &subdirectory.relative, out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modding::ecosystem::EcosystemSchema;

    const FIXTURE: &str = include_str!("../../test-fixtures/modding/ecosystem-schema.json");

    fn rules_of(game: &str) -> (Vec<InstallRule>, Vec<String>) {
        let schema: EcosystemSchema = serde_json::from_str(FIXTURE).unwrap();
        let entry = schema.games[game].game_entry().unwrap();
        (
            entry.install_rules.clone(),
            entry.relative_file_exclusions.clone(),
        )
    }

    fn package() -> PackageName {
        "Owner-Mod".parse().unwrap()
    }

    fn tree(files: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("legio-rules-{}", uuid::Uuid::new_v4()));
        for file in files {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, file).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        root
    }

    /// Sorted (destination, tracked) pairs, with sources checked to exist.
    fn plan(game: &str, files: &[&str]) -> Vec<(String, bool)> {
        let root = tree(files);
        let (rules, exclusions) = rules_of(game);
        let planned = plan_install(&rules, &exclusions, &package(), &root).unwrap();
        for file in &planned {
            assert!(file.source.starts_with(&root) && file.source.is_file());
        }
        let mut pairs: Vec<_> = planned
            .into_iter()
            .map(|file| (file.destination, file.tracked))
            .collect();
        pairs.sort();
        fs::remove_dir_all(root).unwrap();
        pairs
    }

    fn tracked(path: &str) -> (String, bool) {
        (path.to_owned(), true)
    }

    fn untracked(path: &str) -> (String, bool) {
        (path.to_owned(), false)
    }

    #[test]
    fn loose_dll_goes_to_the_plugins_subdirectory() {
        assert_eq!(
            plan("lethal-company", &["Mod.dll"]),
            [tracked("BepInEx/plugins/Owner-Mod/Mod.dll")]
        );
    }

    #[test]
    fn extension_matching_ignores_case() {
        assert_eq!(
            plan("lethal-company", &["MOD.DLL"]),
            [tracked("BepInEx/plugins/Owner-Mod/MOD.DLL")]
        );
    }

    #[test]
    fn longer_extension_wins() {
        assert_eq!(
            plan("lethal-company", &["Patch.mm.dll", "Plain.dll"]),
            [
                tracked("BepInEx/monomod/Owner-Mod/Patch.mm.dll"),
                tracked("BepInEx/plugins/Owner-Mod/Plain.dll"),
            ]
        );
    }

    #[test]
    fn unmatched_files_use_the_default_location() {
        assert_eq!(
            plan(
                "lethal-company",
                &["manifest.json", "icon.png", "README.md"]
            ),
            [
                tracked("BepInEx/plugins/Owner-Mod/README.md"),
                tracked("BepInEx/plugins/Owner-Mod/icon.png"),
                tracked("BepInEx/plugins/Owner-Mod/manifest.json"),
            ]
        );
    }

    #[test]
    fn plugins_folder_keeps_its_inner_structure() {
        assert_eq!(
            plan(
                "lethal-company",
                &["plugins/Mod.dll", "plugins/assets/sound.ogg"]
            ),
            [
                tracked("BepInEx/plugins/Owner-Mod/Mod.dll"),
                tracked("BepInEx/plugins/Owner-Mod/assets/sound.ogg"),
            ]
        );
    }

    #[test]
    fn nested_bepinex_plugins_folder_is_found() {
        assert_eq!(
            plan(
                "lethal-company",
                &["BepInEx/plugins/Mod.dll", "BepInEx/plugins/lang/en.json"]
            ),
            [
                tracked("BepInEx/plugins/Owner-Mod/Mod.dll"),
                tracked("BepInEx/plugins/Owner-Mod/lang/en.json"),
            ]
        );
    }

    #[test]
    fn folder_names_match_ignoring_case() {
        assert_eq!(
            plan("lethal-company", &["Plugins/Mod.dll", "PATCHERS/Patch.dll"]),
            [
                tracked("BepInEx/patchers/Owner-Mod/Patch.dll"),
                tracked("BepInEx/plugins/Owner-Mod/Mod.dll"),
            ]
        );
    }

    #[test]
    fn patchers_and_core_get_their_own_subdirectories() {
        assert_eq!(
            plan(
                "lethal-company",
                &["patchers/Patch.dll", "core/Core.dll", "Mod.dll"]
            ),
            [
                tracked("BepInEx/core/Owner-Mod/Core.dll"),
                tracked("BepInEx/patchers/Owner-Mod/Patch.dll"),
                tracked("BepInEx/plugins/Owner-Mod/Mod.dll"),
            ]
        );
    }

    #[test]
    fn config_files_are_copied_untracked() {
        assert_eq!(
            plan(
                "lethal-company",
                &["config/mod.cfg", "config/sub/extra.cfg", "Mod.dll"]
            ),
            [
                untracked("BepInEx/config/mod.cfg"),
                untracked("BepInEx/config/sub/extra.cfg"),
                tracked("BepInEx/plugins/Owner-Mod/Mod.dll"),
            ]
        );
    }

    #[test]
    fn loose_files_in_unknown_folders_are_flattened_by_extension() {
        assert_eq!(
            plan(
                "lethal-company",
                &[
                    "Wrapper/Mod.dll",
                    "Wrapper/deep/Other.dll",
                    "Wrapper/notes.txt"
                ]
            ),
            [
                tracked("BepInEx/plugins/Owner-Mod/Mod.dll"),
                tracked("BepInEx/plugins/Owner-Mod/Other.dll"),
                tracked("BepInEx/plugins/Owner-Mod/notes.txt"),
            ]
        );
    }

    #[test]
    fn deeper_folders_are_searched_for_matching_names() {
        assert_eq!(
            plan(
                "lethal-company",
                &["Wrapper/plugins/Mod.dll", "Wrapper/plugins/data/a.bin"]
            ),
            [
                tracked("BepInEx/plugins/Owner-Mod/Mod.dll"),
                tracked("BepInEx/plugins/Owner-Mod/data/a.bin"),
            ]
        );
    }

    #[test]
    fn mixed_package_layouts_are_split_across_routes() {
        assert_eq!(
            plan(
                "riskofrain2",
                &[
                    "manifest.json",
                    "BepInEx/plugins/Mod.dll",
                    "BepInEx/config/Mod.cfg",
                    "BepInEx/patchers/Patch.dll",
                    "Other.dll",
                ]
            ),
            [
                untracked("BepInEx/config/Mod.cfg"),
                tracked("BepInEx/patchers/Owner-Mod/Patch.dll"),
                tracked("BepInEx/plugins/Owner-Mod/Mod.dll"),
                tracked("BepInEx/plugins/Owner-Mod/Other.dll"),
                tracked("BepInEx/plugins/Owner-Mod/manifest.json"),
            ]
        );
    }

    #[test]
    fn same_flattened_name_keeps_the_later_file() {
        let root = tree(&["a/Mod.dll", "b/Mod.dll"]);
        let (rules, exclusions) = rules_of("lethal-company");
        let planned = plan_install(&rules, &exclusions, &package(), &root).unwrap();
        assert_eq!(planned.len(), 1);
        assert!(planned[0].source.ends_with("Mod.dll"));
        assert!(planned[0].source.starts_with(root.join("b")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_package_plans_nothing() {
        assert_eq!(plan("lethal-company", &[]), []);
    }

    #[test]
    fn state_rules_keep_paths_and_honor_exclusions() {
        assert_eq!(
            plan(
                "backpack-hero",
                &[
                    "Mods/Mod.dll",
                    "Mods/data/a.bin",
                    "Mods/manifest.json",
                    "UserLibs/Lib.lib.dll",
                    "MelonLoader/Managed/Thing.managed.dll",
                    "MelonLoader/Libs/native.bin",
                    "Loose.dll",
                    "manifest.json",
                    "LICENCE",
                ]
            ),
            [
                tracked("MelonLoader/Libs/native.bin"),
                tracked("MelonLoader/Managed/Thing.managed.dll"),
                tracked("Mods/Loose.dll"),
                tracked("Mods/Mod.dll"),
                tracked("Mods/data/a.bin"),
                tracked("UserData/ModManager/Owner-Mod/LICENCE"),
                tracked("UserData/ModManager/Owner-Mod/manifest.json"),
                tracked("UserLibs/Lib.lib.dll"),
            ]
        );
    }

    #[test]
    fn no_flatten_keeps_the_relative_path() {
        let rules = vec![InstallRule {
            route: "Data".to_owned(),
            default_file_extensions: Vec::new(),
            tracking_method: TrackingMethod::SubdirNoFlatten,
            is_default_location: true,
            sub_routes: Vec::new(),
        }];
        let root = tree(&["Wrapper/deep/a.txt", "b.txt"]);
        let mut destinations: Vec<_> = plan_install(&rules, &[], &package(), &root)
            .unwrap()
            .into_iter()
            .map(|file| file.destination)
            .collect();
        destinations.sort();
        assert_eq!(
            destinations,
            ["Data/Owner-Mod/Wrapper/deep/a.txt", "Data/Owner-Mod/b.txt"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn matching_directories_pick_the_best_route_suffix() {
        let leaf = |route: &str| InstallRule {
            route: route.to_owned(),
            default_file_extensions: Vec::new(),
            tracking_method: TrackingMethod::Untracked,
            is_default_location: false,
            sub_routes: Vec::new(),
        };
        let rules = vec![
            leaf("A/Data"),
            leaf("B/Data"),
            InstallRule {
                is_default_location: true,
                ..leaf("Fallback")
            },
        ];
        let root = tree(&["B/Data/x.txt", "Data/y.txt"]);
        let mut planned: Vec<_> = plan_install(&rules, &[], &package(), &root)
            .unwrap()
            .into_iter()
            .map(|file| file.destination)
            .collect();
        planned.sort();
        // B/Data shares two trailing segments with the route B/Data; the
        // top-level Data folder ties both routes and the first one wins.
        assert_eq!(planned, ["A/Data/y.txt", "B/Data/x.txt"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_zip_routes_are_unsupported() {
        let rules = vec![InstallRule {
            route: "Mods".to_owned(),
            default_file_extensions: Vec::new(),
            tracking_method: TrackingMethod::PackageZip,
            is_default_location: true,
            sub_routes: Vec::new(),
        }];
        let root = tree(&["a.zip"]);
        let error = plan_install(&rules, &[], &package(), &root).unwrap_err();
        assert!(error.contains("not supported"), "{error}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_unsafe_routes_and_empty_rule_sets() {
        let root = tree(&["a.dll"]);
        for route in ["../x", "a/../../x", "C:/x"] {
            let rules = vec![InstallRule {
                route: route.to_owned(),
                default_file_extensions: Vec::new(),
                tracking_method: TrackingMethod::Untracked,
                is_default_location: true,
                sub_routes: Vec::new(),
            }];
            assert!(
                plan_install(&rules, &[], &package(), &root).is_err(),
                "{route}"
            );
        }
        assert!(plan_install(&[], &[], &package(), &root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn routes_accept_backslashes_and_join_nested_routes() {
        let (rules, _) = rules_of("backpack-hero");
        let mut managed = Vec::new();
        flatten_rules(&rules, "", &mut managed).unwrap();
        let routes: Vec<_> = managed.iter().map(|rule| rule.route.as_str()).collect();
        assert!(routes.contains(&"MelonLoader/Managed"));
        assert_eq!(join_route("A", "b\\c").unwrap(), "A/b/c");
    }
}
