//! Dependencies between the mods installed in a profile, as their manifests
//! declare them. Names match case-insensitively and a mod naming itself is
//! ignored.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::database::ProfileMod;

/// How the declared dependencies of one installed mod stand.
#[derive(Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DependencyState {
    /// Declared packages that are not installed.
    pub(crate) missing_dependencies: Vec<String>,
    /// Installed dependencies that are off while the mod is on.
    pub(crate) disabled_dependencies: Vec<String>,
    /// Installed mods that declare this one, enabled or not.
    pub(crate) dependents: Vec<String>,
    /// Enabled mods that need this one, directly or through other enabled
    /// mods, the most dependent first.
    pub(crate) enabled_dependents: Vec<String>,
}

pub(crate) struct DependencyGraph<'a> {
    mods: &'a [ProfileMod],
    by_key: HashMap<String, &'a ProfileMod>,
    /// Declared packages keyed by lowercase `full_name`.
    declared: &'a HashMap<String, Vec<String>>,
}

impl<'a> DependencyGraph<'a> {
    pub(crate) fn new(mods: &'a [ProfileMod], declared: &'a HashMap<String, Vec<String>>) -> Self {
        let by_key = mods
            .iter()
            .map(|record| (record.full_name.to_lowercase(), record))
            .collect();
        Self {
            mods,
            by_key,
            declared,
        }
    }

    fn find(&self, full_name: &str) -> Option<&'a ProfileMod> {
        self.by_key.get(&full_name.to_lowercase()).copied()
    }

    fn dependencies_of(&self, record: &ProfileMod) -> Vec<&'a str> {
        self.declared
            .get(&record.full_name.to_lowercase())
            .into_iter()
            .flatten()
            .filter(|package| !package.eq_ignore_ascii_case(&record.full_name))
            .map(String::as_str)
            .collect()
    }

    /// Installed mods other than `target` that declare it.
    pub(crate) fn dependents(&self, target: &ProfileMod) -> Vec<&'a ProfileMod> {
        self.mods
            .iter()
            .filter(|other| !other.full_name.eq_ignore_ascii_case(&target.full_name))
            .filter(|other| {
                self.dependencies_of(other)
                    .iter()
                    .any(|package| package.eq_ignore_ascii_case(&target.full_name))
            })
            .collect()
    }

    /// A disabled mod needs nothing, so it only reports missing packages.
    pub(crate) fn state(&self, record: &'a ProfileMod) -> DependencyState {
        let mut state = DependencyState::default();
        for package in self.dependencies_of(record) {
            match self.find(package) {
                None => state.missing_dependencies.push(package.to_owned()),
                Some(dependency) if record.enabled && !dependency.enabled => state
                    .disabled_dependencies
                    .push(dependency.full_name.clone()),
                Some(_) => {}
            }
        }
        state.dependents = names(self.dependents(record));
        state.enabled_dependents = names(self.enabled_dependents(record));
        state
    }

    /// Disabled mods to enable, dependencies first, so that `targets` end
    /// up enabled with everything they need.
    pub(crate) fn enable_order(&self, targets: &[&'a ProfileMod]) -> Vec<&'a ProfileMod> {
        let mut visited = HashSet::new();
        let mut order = Vec::new();
        for target in targets {
            self.visit_dependencies(target, &mut visited, &mut order);
        }
        order
    }

    fn visit_dependencies(
        &self,
        record: &'a ProfileMod,
        visited: &mut HashSet<String>,
        order: &mut Vec<&'a ProfileMod>,
    ) {
        if !visited.insert(record.full_name.to_lowercase()) {
            return;
        }
        for package in self.dependencies_of(record) {
            if let Some(dependency) = self.find(package) {
                self.visit_dependencies(dependency, visited, order);
            }
        }
        if !record.enabled {
            order.push(record);
        }
    }

    /// Enabled mods that need `target`, directly or through other enabled
    /// mods, the most dependent first. Disabled mods do not pass the need on.
    pub(crate) fn enabled_dependents(&self, target: &'a ProfileMod) -> Vec<&'a ProfileMod> {
        let mut visited = HashSet::from([target.full_name.to_lowercase()]);
        let mut order = Vec::new();
        self.visit_dependents(target, &mut visited, &mut order);
        order
    }

    fn visit_dependents(
        &self,
        record: &'a ProfileMod,
        visited: &mut HashSet<String>,
        order: &mut Vec<&'a ProfileMod>,
    ) {
        for dependent in self.dependents(record) {
            if !dependent.enabled || !visited.insert(dependent.full_name.to_lowercase()) {
                continue;
            }
            self.visit_dependents(dependent, visited, order);
            order.push(dependent);
        }
    }

    /// Enabled mods to disable among `targets`, each preceded by its enabled
    /// dependents when `with_dependents` is set.
    pub(crate) fn disable_order(
        &self,
        targets: &[&'a ProfileMod],
        with_dependents: bool,
    ) -> Vec<&'a ProfileMod> {
        let mut seen = HashSet::new();
        let mut order = Vec::new();
        for target in targets {
            let dependents = if with_dependents {
                self.enabled_dependents(target)
            } else {
                Vec::new()
            };
            for record in dependents.into_iter().chain([*target]) {
                if record.enabled && seen.insert(record.full_name.to_lowercase()) {
                    order.push(record);
                }
            }
        }
        order
    }
}

fn names(mods: Vec<&ProfileMod>) -> Vec<String> {
    mods.into_iter()
        .map(|record| record.full_name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(full_name: &str, enabled: bool) -> ProfileMod {
        ProfileMod {
            profile_id: "p".to_owned(),
            full_name: full_name.to_owned(),
            version: "1.0.0".to_owned(),
            enabled,
            online_source: true,
            installed_at: 0,
            install_order: 0,
            local_cache_key: None,
        }
    }

    fn declared(entries: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
        entries
            .iter()
            .map(|(name, packages)| {
                (
                    name.to_lowercase(),
                    packages
                        .iter()
                        .map(|package| (*package).to_owned())
                        .collect(),
                )
            })
            .collect()
    }

    fn full_names(mods: Vec<&ProfileMod>) -> Vec<&str> {
        mods.into_iter()
            .map(|record| record.full_name.as_str())
            .collect()
    }

    #[test]
    fn reports_missing_and_disabled_dependencies() {
        let mods = [
            installed("A-Core", true),
            installed("B-Lib", false),
            installed("C-Mid", false),
            installed("D-Top", true),
        ];
        // D-Top names itself and spells C-Mid in another case.
        let declared = declared(&[
            ("B-Lib", &["A-Core", "Gone-Missing"]),
            ("C-Mid", &["b-lib"]),
            ("D-Top", &["c-mid", "A-Core", "D-Top"]),
        ]);
        let graph = DependencyGraph::new(&mods, &declared);
        let lib = graph.state(&mods[1]);
        assert_eq!(lib.missing_dependencies, ["Gone-Missing"]);
        assert!(lib.disabled_dependencies.is_empty());
        let top = graph.state(&mods[3]);
        assert!(top.missing_dependencies.is_empty());
        assert_eq!(top.disabled_dependencies, ["C-Mid"]);
        assert!(top.dependents.is_empty());
        let core = graph.state(&mods[0]);
        assert_eq!(core.dependents, ["B-Lib", "D-Top"]);
        assert_eq!(core.enabled_dependents, ["D-Top"]);
    }

    #[test]
    fn enables_dependencies_first() {
        let mods = [
            installed("A-Core", true),
            installed("B-Lib", false),
            installed("C-Mid", false),
            installed("D-Top", true),
        ];
        let declared = declared(&[
            ("B-Lib", &["A-CORE"]),
            ("C-Mid", &["B-Lib"]),
            ("D-Top", &["C-Mid", "D-Top"]),
        ]);
        let graph = DependencyGraph::new(&mods, &declared);
        assert_eq!(
            full_names(graph.enable_order(&[&mods[2]])),
            ["B-Lib", "C-Mid"]
        );
        assert_eq!(
            full_names(graph.enable_order(&[&mods[3]])),
            ["B-Lib", "C-Mid"]
        );
        assert!(graph.enable_order(&[&mods[0]]).is_empty());
    }

    #[test]
    fn plans_survive_cycles() {
        let mods = [installed("P-One", false), installed("P-Two", false)];
        let declared = declared(&[("P-One", &["p-two"]), ("P-Two", &["P-One"])]);
        let graph = DependencyGraph::new(&mods, &declared);
        assert_eq!(
            full_names(graph.enable_order(&[&mods[0]])),
            ["P-Two", "P-One"]
        );
        let enabled = [installed("P-One", true), installed("P-Two", true)];
        let graph = DependencyGraph::new(&enabled, &declared);
        assert_eq!(full_names(graph.enabled_dependents(&enabled[0])), ["P-Two"]);
        assert_eq!(
            full_names(graph.disable_order(&[&enabled[0]], true)),
            ["P-Two", "P-One"]
        );
    }

    #[test]
    fn disables_the_most_dependent_first() {
        let mods = [
            installed("X-Base", true),
            installed("Y-Mid", true),
            installed("Z-Top", true),
            installed("W-Off", false),
        ];
        let declared = declared(&[
            ("Y-Mid", &["x-base"]),
            ("Z-Top", &["Y-Mid"]),
            ("W-Off", &["X-Base"]),
        ]);
        let graph = DependencyGraph::new(&mods, &declared);
        assert_eq!(
            full_names(graph.disable_order(&[&mods[0]], true)),
            ["Z-Top", "Y-Mid", "X-Base"]
        );
        assert_eq!(
            full_names(graph.disable_order(&[&mods[0]], false)),
            ["X-Base"]
        );
        assert!(graph.disable_order(&[&mods[3]], true).is_empty());
    }
}
