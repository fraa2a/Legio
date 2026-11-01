//! Dependency resolution for package installs. Port of the intent of
//! r2modmanPlus `DependencyUtils.ts` with a real topological order.

use std::collections::{HashMap, VecDeque};

use super::package_id::{PackageName, PackageVersionId, Version};

const MAX_RESOLUTION_STEPS: usize = 20_000;

/// A package version known to the package source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedPackage {
    pub(crate) id: PackageVersionId,
    pub(crate) dependencies: Vec<PackageVersionId>,
    pub(crate) download_url: String,
    pub(crate) file_size: u64,
}

/// Where package metadata comes from. Implemented over the package database
/// by the Thunderstore wiring and by fakes in tests.
pub(crate) trait PackageSource {
    fn latest(&self, package: &PackageName) -> Result<Option<ResolvedPackage>, String>;
    fn exact(&self, id: &PackageVersionId) -> Result<Option<ResolvedPackage>, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolveMode {
    /// Dependencies resolve to their latest version and packages already
    /// installed in the profile are left alone, so version-pinned mods are not
    /// updated behind the user's back.
    Latest,
    /// Dependencies resolve to the exact versions their dependents list, as
    /// profile imports need.
    Exact,
}

/// A package the user asked for. Without a version the latest one is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstallRequest {
    pub(crate) package: PackageName,
    pub(crate) version: Option<Version>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstallPlan {
    /// Dependencies come before the packages that need them.
    pub(crate) packages: Vec<ResolvedPackage>,
    pub(crate) warnings: Vec<String>,
}

/// Package names compare ignoring case, as on Thunderstore.
pub(crate) fn package_key(package: &PackageName) -> String {
    package.to_string().to_lowercase()
}

enum Want {
    Latest {
        package: PackageName,
        /// Listed by a dependent; used when the source has no latest version.
        fallback: Option<PackageVersionId>,
    },
    Exact(PackageVersionId),
}

impl Want {
    fn package(&self) -> &PackageName {
        match self {
            Self::Latest { package, .. } => package,
            Self::Exact(id) => &id.package,
        }
    }
}

/// Resolves `requests` into the packages to install. `installed` maps package
/// keys to the version already in the profile.
pub(crate) fn resolve_plan(
    requests: &[InstallRequest],
    source: &dyn PackageSource,
    mode: ResolveMode,
    installed: &HashMap<String, Version>,
) -> Result<InstallPlan, String> {
    let mut chosen: HashMap<String, ResolvedPackage> = HashMap::new();
    let mut queue: VecDeque<(Want, bool)> = requests
        .iter()
        .map(|request| {
            let want = match request.version {
                Some(version) => {
                    Want::Exact(PackageVersionId::new(request.package.clone(), version))
                }
                None => Want::Latest {
                    package: request.package.clone(),
                    fallback: None,
                },
            };
            (want, true)
        })
        .collect();

    let mut steps = 0usize;
    while let Some((want, is_root)) = queue.pop_front() {
        steps += 1;
        if steps > MAX_RESOLUTION_STEPS {
            return Err("The dependency tree is too large to resolve".to_owned());
        }
        let key = package_key(want.package());
        if want.package().is_mod_manager() {
            return Err(format!(
                "{} is a mod manager application, not a mod, and cannot be installed",
                want.package()
            ));
        }
        if !is_root && !needs_resolution(&want, &key, mode, &chosen, installed) {
            continue;
        }
        let resolved = look_up(source, &want)?;
        if package_key(&resolved.id.package) != key {
            return Err(format!(
                "The package source returned {} for {}",
                resolved.id,
                want.package()
            ));
        }
        let replaces = chosen
            .get(&key)
            .is_none_or(|existing| resolved.id.version > existing.id.version);
        if !replaces {
            continue;
        }
        for dependency in &resolved.dependencies {
            let want = match mode {
                ResolveMode::Latest => Want::Latest {
                    package: dependency.package.clone(),
                    fallback: Some(dependency.clone()),
                },
                ResolveMode::Exact => Want::Exact(dependency.clone()),
            };
            queue.push_back((want, false));
        }
        chosen.insert(key, resolved);
    }

    let roots: Vec<String> = requests
        .iter()
        .map(|request| package_key(&request.package))
        .collect();
    Ok(order_dependencies_first(&roots, chosen))
}

fn needs_resolution(
    want: &Want,
    key: &str,
    mode: ResolveMode,
    chosen: &HashMap<String, ResolvedPackage>,
    installed: &HashMap<String, Version>,
) -> bool {
    match (mode, want) {
        (ResolveMode::Latest, _) | (_, Want::Latest { .. }) => {
            !chosen.contains_key(key) && !installed.contains_key(key)
        }
        (ResolveMode::Exact, Want::Exact(id)) => {
            chosen
                .get(key)
                .is_none_or(|existing| existing.id.version < id.version)
                && installed.get(key) != Some(&id.version)
        }
    }
}

fn look_up(source: &dyn PackageSource, want: &Want) -> Result<ResolvedPackage, String> {
    let (found, label) = match want {
        Want::Exact(id) => (source.exact(id)?, id.to_string()),
        Want::Latest { package, fallback } => {
            let mut found = source.latest(package)?;
            if found.is_none()
                && let Some(fallback) = fallback
            {
                found = source.exact(fallback)?;
            }
            (found, package.to_string())
        }
    };
    found.ok_or_else(|| format!("Package {label} was not found on Thunderstore"))
}

enum Visit {
    InProgress,
    Done,
}

/// Depth-first post-order from the roots, so a package follows everything it
/// depends on. A dependency cycle is broken at the edge that closes it and
/// reported as a warning. Packages no root reaches (left over from versions
/// that were replaced) are dropped.
fn order_dependencies_first(
    roots: &[String],
    mut chosen: HashMap<String, ResolvedPackage>,
) -> InstallPlan {
    let mut state: HashMap<String, Visit> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut warnings = Vec::new();

    for root in roots {
        if state.contains_key(root) || !chosen.contains_key(root) {
            continue;
        }
        state.insert(root.clone(), Visit::InProgress);
        let mut stack: Vec<(String, usize)> = vec![(root.clone(), 0)];
        while let Some((key, index)) = stack.last().cloned() {
            let next = chosen[&key]
                .dependencies
                .get(index)
                .map(|dependency| package_key(&dependency.package));
            let Some(dependency_key) = next else {
                state.insert(key.clone(), Visit::Done);
                order.push(key);
                stack.pop();
                continue;
            };
            if let Some(top) = stack.last_mut() {
                top.1 += 1;
            }
            if !chosen.contains_key(&dependency_key) {
                continue;
            }
            match state.get(&dependency_key) {
                Some(Visit::Done) => {}
                Some(Visit::InProgress) => {
                    let start = stack
                        .iter()
                        .position(|(entry, _)| *entry == dependency_key)
                        .unwrap_or(0);
                    let mut cycle: Vec<String> = stack[start..]
                        .iter()
                        .map(|(entry, _)| chosen[entry].id.package.to_string())
                        .collect();
                    cycle.push(chosen[&dependency_key].id.package.to_string());
                    warnings.push(format!("Dependency cycle: {}", cycle.join(" -> ")));
                }
                None => {
                    state.insert(dependency_key.clone(), Visit::InProgress);
                    stack.push((dependency_key, 0));
                }
            }
        }
    }

    InstallPlan {
        packages: order
            .into_iter()
            .filter_map(|key| chosen.remove(&key))
            .collect(),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    fn id(value: &str) -> PackageVersionId {
        value.parse().unwrap()
    }

    fn name(value: &str) -> PackageName {
        value.parse().unwrap()
    }

    /// Packages by id with their dependency strings.
    struct FakeSource {
        packages: BTreeMap<PackageVersionId, Vec<PackageVersionId>>,
        calls: RefCell<Vec<String>>,
    }

    impl FakeSource {
        fn new(packages: &[(&str, &[&str])]) -> Self {
            Self {
                packages: packages
                    .iter()
                    .map(|(package, dependencies)| {
                        (id(package), dependencies.iter().map(|d| id(d)).collect())
                    })
                    .collect(),
                calls: RefCell::default(),
            }
        }

        fn resolved(&self, id: &PackageVersionId) -> ResolvedPackage {
            ResolvedPackage {
                id: id.clone(),
                dependencies: self.packages[id].clone(),
                download_url: format!("https://example.invalid/{id}.zip"),
                file_size: 1,
            }
        }
    }

    impl PackageSource for FakeSource {
        fn latest(&self, package: &PackageName) -> Result<Option<ResolvedPackage>, String> {
            self.calls.borrow_mut().push(format!("latest {package}"));
            Ok(self
                .packages
                .keys()
                .filter(|candidate| candidate.package == *package)
                .max_by_key(|candidate| candidate.version)
                .map(|found| self.resolved(found)))
        }

        fn exact(&self, id: &PackageVersionId) -> Result<Option<ResolvedPackage>, String> {
            self.calls.borrow_mut().push(format!("exact {id}"));
            Ok(self.packages.contains_key(id).then(|| self.resolved(id)))
        }
    }

    fn request(value: &str) -> InstallRequest {
        match value.parse::<PackageVersionId>() {
            Ok(id) => InstallRequest {
                package: id.package,
                version: Some(id.version),
            },
            Err(_) => InstallRequest {
                package: name(value),
                version: None,
            },
        }
    }

    fn plan_ids(
        source: &FakeSource,
        requests: &[&str],
        mode: ResolveMode,
        installed: &[&str],
    ) -> Result<Vec<String>, String> {
        let installed = installed
            .iter()
            .map(|value| {
                let id = id(value);
                (package_key(&id.package), id.version)
            })
            .collect();
        let requests: Vec<_> = requests.iter().map(|value| request(value)).collect();
        resolve_plan(&requests, source, mode, &installed)
            .map(|plan| plan.packages.iter().map(|p| p.id.to_string()).collect())
    }

    fn chain() -> FakeSource {
        FakeSource::new(&[
            ("A-App-1.0.0", &["B-Lib-1.0.0", "C-Core-1.0.0"]),
            ("B-Lib-1.0.0", &["C-Core-1.0.0"]),
            ("B-Lib-1.1.0", &["C-Core-1.0.0"]),
            ("C-Core-1.0.0", &[]),
            ("C-Core-2.0.0", &[]),
        ])
    }

    #[test]
    fn orders_dependencies_before_dependents() {
        let source = chain();
        assert_eq!(
            plan_ids(&source, &["A-App"], ResolveMode::Latest, &[]).unwrap(),
            ["C-Core-2.0.0", "B-Lib-1.1.0", "A-App-1.0.0"]
        );
    }

    #[test]
    fn refuses_mod_manager_applications() {
        let source = FakeSource::new(&[
            ("ebkr-r2modman-3.2.21", &[]),
            ("A-Pack-1.0.0", &["kesomannen-GaleModManager-1.0.0"]),
            ("Kesomannen-GaleModManager-1.0.0", &[]),
        ]);
        for request in ["ebkr-r2modman", "A-Pack"] {
            let error = plan_ids(&source, &[request], ResolveMode::Latest, &[]).unwrap_err();
            assert!(error.contains("mod manager application"), "{error}");
        }
    }

    #[test]
    fn exact_mode_keeps_listed_versions() {
        let source = chain();
        assert_eq!(
            plan_ids(&source, &["A-App-1.0.0"], ResolveMode::Exact, &[]).unwrap(),
            ["C-Core-1.0.0", "B-Lib-1.0.0", "A-App-1.0.0"]
        );
    }

    #[test]
    fn highest_exact_version_wins_and_replaced_dependencies_are_dropped() {
        let source = FakeSource::new(&[
            ("A-App-1.0.0", &["B-Lib-1.0.0", "C-Core-1.0.0"]),
            ("B-Lib-1.0.0", &["C-Core-2.0.0"]),
            ("C-Core-1.0.0", &["D-Old-1.0.0"]),
            ("C-Core-2.0.0", &[]),
            ("D-Old-1.0.0", &[]),
        ]);
        let plan = plan_ids(&source, &["A-App-1.0.0"], ResolveMode::Exact, &[]).unwrap();
        assert_eq!(plan, ["C-Core-2.0.0", "B-Lib-1.0.0", "A-App-1.0.0"]);
    }

    #[test]
    fn highest_requested_root_version_wins() {
        let source = chain();
        let plan = plan_ids(
            &source,
            &["C-Core-1.0.0", "C-Core-2.0.0", "C-Core-1.0.0"],
            ResolveMode::Latest,
            &[],
        )
        .unwrap();
        assert_eq!(plan, ["C-Core-2.0.0"]);
    }

    #[test]
    fn requested_version_is_not_replaced_by_a_newer_dependency() {
        let source = chain();
        let plan = plan_ids(
            &source,
            &["A-App-1.0.0", "C-Core-1.0.0"],
            ResolveMode::Latest,
            &[],
        )
        .unwrap();
        assert_eq!(plan, ["C-Core-1.0.0", "B-Lib-1.1.0", "A-App-1.0.0"]);
    }

    #[test]
    fn installed_dependencies_are_left_alone_in_latest_mode() {
        let source = chain();
        let plan = plan_ids(
            &source,
            &["A-App"],
            ResolveMode::Latest,
            &["B-Lib-1.0.0", "c-core-1.0.0"],
        )
        .unwrap();
        assert_eq!(plan, ["A-App-1.0.0"]);
        // The installed packages were not even looked up.
        assert_eq!(source.calls.borrow().as_slice(), ["latest A-App"]);
    }

    #[test]
    fn requested_packages_are_planned_even_when_installed() {
        let source = chain();
        let plan = plan_ids(&source, &["B-Lib"], ResolveMode::Latest, &["B-Lib-1.0.0"]).unwrap();
        assert_eq!(plan, ["C-Core-2.0.0", "B-Lib-1.1.0"]);
    }

    #[test]
    fn exact_mode_skips_dependencies_installed_at_that_version() {
        let source = chain();
        let plan = plan_ids(
            &source,
            &["A-App-1.0.0"],
            ResolveMode::Exact,
            &["C-Core-1.0.0", "B-Lib-1.1.0"],
        )
        .unwrap();
        // B-Lib-1.1.0 differs from the listed 1.0.0 and is replaced.
        assert_eq!(plan, ["B-Lib-1.0.0", "A-App-1.0.0"]);
    }

    #[test]
    fn breaks_dependency_cycles() {
        let source = FakeSource::new(&[
            ("A-One-1.0.0", &["B-Two-1.0.0"]),
            ("B-Two-1.0.0", &["C-Three-1.0.0"]),
            ("C-Three-1.0.0", &["A-One-1.0.0"]),
        ]);
        let requests = [request("A-One")];
        let plan = resolve_plan(&requests, &source, ResolveMode::Latest, &HashMap::new()).unwrap();
        let ids: Vec<_> = plan.packages.iter().map(|p| p.id.to_string()).collect();
        assert_eq!(ids, ["C-Three-1.0.0", "B-Two-1.0.0", "A-One-1.0.0"]);
        assert_eq!(
            plan.warnings,
            ["Dependency cycle: A-One -> B-Two -> C-Three -> A-One"]
        );
    }

    #[test]
    fn handles_a_package_depending_on_itself() {
        let source = FakeSource::new(&[("A-One-1.0.0", &["A-One-1.0.0"])]);
        let requests = [request("A-One")];
        let plan = resolve_plan(&requests, &source, ResolveMode::Exact, &HashMap::new()).unwrap();
        assert_eq!(plan.packages.len(), 1);
        assert_eq!(plan.warnings.len(), 1);
    }

    #[test]
    fn deduplicates_diamond_dependencies() {
        let source = FakeSource::new(&[
            ("A-Top-1.0.0", &["B-Left-1.0.0", "C-Right-1.0.0"]),
            ("B-Left-1.0.0", &["D-Base-1.0.0"]),
            ("C-Right-1.0.0", &["D-Base-1.0.0"]),
            ("D-Base-1.0.0", &[]),
        ]);
        assert_eq!(
            plan_ids(&source, &["A-Top"], ResolveMode::Latest, &[]).unwrap(),
            [
                "D-Base-1.0.0",
                "B-Left-1.0.0",
                "C-Right-1.0.0",
                "A-Top-1.0.0"
            ]
        );
    }

    #[test]
    fn reports_missing_packages() {
        let source = chain();
        assert!(
            plan_ids(&source, &["Z-Missing"], ResolveMode::Latest, &[])
                .unwrap_err()
                .contains("Z-Missing")
        );
        assert!(plan_ids(&source, &["A-App-9.9.9"], ResolveMode::Exact, &[]).is_err());
    }

    #[test]
    fn falls_back_to_the_listed_version_when_latest_is_unknown() {
        struct NoLatest(FakeSource);
        impl PackageSource for NoLatest {
            fn latest(&self, _: &PackageName) -> Result<Option<ResolvedPackage>, String> {
                Ok(None)
            }
            fn exact(&self, id: &PackageVersionId) -> Result<Option<ResolvedPackage>, String> {
                self.0.exact(id)
            }
        }
        let source = NoLatest(FakeSource::new(&[
            ("A-App-1.0.0", &["B-Lib-1.0.0"]),
            ("B-Lib-1.0.0", &[]),
        ]));
        let requests = [request("A-App-1.0.0")];
        let plan = resolve_plan(&requests, &source, ResolveMode::Latest, &HashMap::new()).unwrap();
        assert_eq!(plan.packages.len(), 2);
    }

    #[test]
    fn rejects_a_source_that_answers_with_another_package() {
        struct Liar;
        impl PackageSource for Liar {
            fn latest(&self, _: &PackageName) -> Result<Option<ResolvedPackage>, String> {
                Ok(Some(ResolvedPackage {
                    id: id("Evil-Other-1.0.0"),
                    dependencies: Vec::new(),
                    download_url: String::new(),
                    file_size: 0,
                }))
            }
            fn exact(&self, _: &PackageVersionId) -> Result<Option<ResolvedPackage>, String> {
                Ok(None)
            }
        }
        let requests = [request("A-App")];
        assert!(resolve_plan(&requests, &Liar, ResolveMode::Latest, &HashMap::new()).is_err());
    }

    #[test]
    fn stops_on_endless_version_chains() {
        struct Endless;
        impl PackageSource for Endless {
            fn latest(&self, package: &PackageName) -> Result<Option<ResolvedPackage>, String> {
                Ok(Some(ResolvedPackage {
                    id: PackageVersionId::new(package.clone(), Version::new(1, 0, 0)),
                    dependencies: vec![id("A-Next-1.0.0")],
                    download_url: String::new(),
                    file_size: 0,
                }))
            }
            fn exact(&self, id: &PackageVersionId) -> Result<Option<ResolvedPackage>, String> {
                Ok(Some(ResolvedPackage {
                    id: id.clone(),
                    dependencies: vec![PackageVersionId::new(
                        id.package.clone(),
                        Version::new(id.version.major + 1, 0, 0),
                    )],
                    download_url: String::new(),
                    file_size: 0,
                }))
            }
        }
        let requests = [request("A-Next-1.0.0")];
        let error =
            resolve_plan(&requests, &Endless, ResolveMode::Exact, &HashMap::new()).unwrap_err();
        assert!(error.contains("too large"), "{error}");
    }
}
