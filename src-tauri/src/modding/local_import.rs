//! Installs a mod from an archive the user picked, outside Thunderstore.

use std::path::Path;

use serde::Serialize;

use super::archive_cache::{package_dir, publish_directory, stage_archive};
use super::installer::{
    InstallError, InstallStage, ModdingContext, ProgressSink, install_cached_package,
    report_progress,
};
use super::package_id::{PackageName, PackageVersionId, Version};
use super::package_manifest::{PackageManifest, read_manifest, write_manifest};
use super::profiles::profile_with_directory;
use crate::database::ProfileMod;

/// Owner of every package imported from a local archive.
const LOCAL_OWNER: &str = "Local";
const MAX_NAME_CHARS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalImportOutcome {
    pub(crate) installed: ProfileMod,
    /// Dependencies from the archive's manifest that the profile does not
    /// have. They are not installed here.
    pub(crate) missing_dependencies: Vec<String>,
}

/// Extracts the archive, installs it as `Local-<name>` and records it as a
/// local (not online) mod. A `manifest.json` in the archive supplies the name,
/// version and dependencies; without one the name comes from the file name and
/// the version is 0.0.0. Importing the same name again replaces the mod.
pub(crate) fn import_local_mod(
    ctx: &ModdingContext<'_>,
    profile_id: &str,
    archive: &Path,
    progress: ProgressSink<'_>,
) -> Result<LocalImportOutcome, InstallError> {
    let (profile, profile_dir) = profile_with_directory(ctx.database, ctx.paths, profile_id)?;
    let _guard = ctx.locks.lock(&profile.id)?;
    report_progress(
        progress,
        &profile.id,
        None,
        InstallStage::Extracting,
        (1, 1),
    );

    let stage = stage_archive(ctx.paths, archive)?;
    let manifest = read_manifest(stage.path())?;
    let id = identify(manifest.as_ref(), archive)?;
    let manifest = match manifest {
        Some(manifest) => manifest,
        None => {
            let manifest = PackageManifest {
                name: Some(id.package.name.clone()),
                version_number: Some(id.version.to_string()),
                dependencies: Vec::new(),
            };
            write_manifest(stage.path(), &manifest)?;
            manifest
        }
    };
    let target = package_dir(ctx.paths, &id, false);
    publish_directory(stage, &target, true)?;

    report_progress(
        progress,
        &profile.id,
        Some(&id),
        InstallStage::Installing,
        (1, 1),
    );
    let installed = install_cached_package(ctx, &profile, &profile_dir, &id, &target, false)?;

    let present: Vec<String> = ctx
        .database
        .profile_mods(&profile.id)?
        .into_iter()
        .map(|record| record.full_name.to_lowercase())
        .collect();
    // Entries that are not Owner-Name-Version strings cannot be installed
    // anyway and are left out.
    let missing_dependencies = manifest
        .dependencies
        .iter()
        .filter_map(|dependency| dependency.parse::<PackageVersionId>().ok())
        .filter(|dependency| !present.contains(&dependency.package.to_string().to_lowercase()))
        .map(|dependency| dependency.to_string())
        .collect();
    report_progress(progress, &profile.id, None, InstallStage::Done, (1, 1));
    Ok(LocalImportOutcome {
        installed,
        missing_dependencies,
    })
}

fn identify(
    manifest: Option<&PackageManifest>,
    archive: &Path,
) -> Result<PackageVersionId, String> {
    let stem = archive
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned());
    let name = manifest
        .and_then(|manifest| manifest.name.as_deref())
        .into_iter()
        .chain(stem.as_deref())
        .find_map(sanitize_name)
        .ok_or_else(|| "The archive has no usable name for the mod".to_owned())?;
    let version = match manifest.and_then(|manifest| manifest.version_number.as_deref()) {
        Some(version) => version
            .parse::<Version>()
            .map_err(|error| format!("The version in manifest.json is invalid: {error}"))?,
        None => Version::new(0, 0, 0),
    };
    Ok(PackageVersionId::new(
        PackageName::new(LOCAL_OWNER, &name)?,
        version,
    ))
}

/// Reduces free text to a Thunderstore package name: letters, digits and
/// underscores. Separators become underscores and other characters are
/// dropped.
fn sanitize_name(raw: &str) -> Option<String> {
    let name: String = raw
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character)
            } else if matches!(character, '_' | '-' | '.') || character.is_whitespace() {
                Some('_')
            } else {
                None
            }
        })
        .collect();
    let name: String = name
        .trim_matches('_')
        .chars()
        .take(MAX_NAME_CHARS)
        .collect();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modding::dependencies::ResolveMode;
    use crate::modding::installer::{set_mod_enabled, uninstall_mod};
    use crate::modding::test_support::{Env, FakeRepo, write_zip};
    use std::{fs, path::PathBuf};

    fn archive(env: &Env, file_name: &str, entries: &[(&str, &str)]) -> PathBuf {
        let path = env.directory.join(file_name);
        write_zip(&path, entries);
        path
    }

    fn import(env: &Env, path: &Path) -> Result<LocalImportOutcome, InstallError> {
        import_local_mod(&env.ctx(), &env.profile.id, path, &|_| {})
    }

    fn temp_entries(env: &Env) -> usize {
        fs::read_dir(env.paths.temp_dir()).map_or(0, Iterator::count)
    }

    #[test]
    fn imports_an_archive_with_a_manifest() {
        let env = Env::new();
        let path = archive(
            &env,
            "download.zip",
            &[
                (
                    "manifest.json",
                    r#"{"name":"Cool Mod","version_number":"1.2.3",
                    "dependencies":["BepInEx-BepInExPack-5.4.2100","Other-Lib-1.0.0","bad entry"]}"#,
                ),
                ("CoolMod.dll", "dll"),
            ],
        );
        let outcome = import(&env, &path).unwrap();
        assert_eq!(outcome.installed.full_name, "Local-Cool_Mod");
        assert_eq!(outcome.installed.version, "1.2.3");
        assert!(!outcome.installed.online_source);
        assert!(outcome.installed.enabled);
        assert_eq!(
            outcome.missing_dependencies,
            ["BepInEx-BepInExPack-5.4.2100", "Other-Lib-1.0.0"]
        );
        assert_eq!(
            fs::read(
                env.profile_dir()
                    .join("BepInEx/plugins/Local-Cool_Mod/CoolMod.dll")
            )
            .unwrap(),
            b"dll"
        );
        let cache = env.paths.root().join("local-cache/Local-Cool_Mod/1.2.3");
        assert!(cache.join("CoolMod.dll").is_file());
        assert!(!env.paths.root().join("cache").exists());
        assert_eq!(temp_entries(&env), 0);
        assert_eq!(env.mods(), [outcome.installed]);
    }

    #[test]
    fn derives_the_name_from_the_file_name_without_a_manifest() {
        let env = Env::new();
        let path = archive(
            &env,
            "My Cool-Mod (v2).zip",
            &[("plugins/Thing.dll", "dll")],
        );
        let outcome = import(&env, &path).unwrap();
        assert_eq!(outcome.installed.full_name, "Local-My_Cool_Mod_v2");
        assert_eq!(outcome.installed.version, "0.0.0");
        let cache = env
            .paths
            .root()
            .join("local-cache/Local-My_Cool_Mod_v2/0.0.0");
        let manifest = read_manifest(&cache).unwrap().unwrap();
        assert_eq!(manifest.version_number.as_deref(), Some("0.0.0"));
        assert!(env.exists("BepInEx/plugins/Local-My_Cool_Mod_v2/Thing.dll"));
    }

    #[test]
    fn importing_again_replaces_the_mod() {
        let env = Env::new();
        let first = archive(
            &env,
            "mod.zip",
            &[
                (
                    "manifest.json",
                    r#"{"name":"Mod","version_number":"1.0.0"}"#,
                ),
                ("Old.dll", "old"),
            ],
        );
        import(&env, &first).unwrap();
        let second = archive(
            &env,
            "mod2.zip",
            &[
                (
                    "manifest.json",
                    r#"{"name":"Mod","version_number":"1.1.0"}"#,
                ),
                ("New.dll", "new"),
            ],
        );
        import(&env, &second).unwrap();
        assert!(!env.exists("BepInEx/plugins/Local-Mod/Old.dll"));
        assert!(env.exists("BepInEx/plugins/Local-Mod/New.dll"));
        let mods = env.mods();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].version, "1.1.0");

        // Same version again refreshes the cached files.
        let third = archive(
            &env,
            "mod3.zip",
            &[
                (
                    "manifest.json",
                    r#"{"name":"Mod","version_number":"1.1.0"}"#,
                ),
                ("Newer.dll", "newer"),
            ],
        );
        import(&env, &third).unwrap();
        let cache = env.paths.root().join("local-cache/Local-Mod/1.1.0");
        assert!(cache.join("Newer.dll").is_file());
        assert!(!cache.join("New.dll").exists());
    }

    #[test]
    fn skips_dependencies_the_profile_already_has() {
        let env = Env::new();
        let mut repo = FakeRepo::default();
        repo.add("Other-Lib-1.0.0", &[], &[("Lib.dll", "lib")]);
        env.install(&repo, &["Other-Lib"], ResolveMode::Latest)
            .unwrap();
        let path = archive(
            &env,
            "mod.zip",
            &[
                (
                    "manifest.json",
                    r#"{"name":"Mod","version_number":"1.0.0","dependencies":["other-lib-0.9.0"]}"#,
                ),
                ("Mod.dll", "dll"),
            ],
        );
        assert!(import(&env, &path).unwrap().missing_dependencies.is_empty());
    }

    #[test]
    fn local_mods_can_be_disabled_enabled_and_uninstalled() {
        let env = Env::new();
        let path = archive(
            &env,
            "mod.zip",
            &[
                (
                    "manifest.json",
                    r#"{"name":"Mod","version_number":"1.0.0"}"#,
                ),
                ("Mod.dll", "dll"),
            ],
        );
        import(&env, &path).unwrap();
        set_mod_enabled(&env.ctx(), &env.profile.id, "Local-Mod", false).unwrap();
        assert!(!env.exists("BepInEx/plugins/Local-Mod"));
        set_mod_enabled(&env.ctx(), &env.profile.id, "Local-Mod", true).unwrap();
        assert!(env.exists("BepInEx/plugins/Local-Mod/Mod.dll"));

        fs::remove_dir_all(env.paths.root().join("local-cache")).unwrap();
        set_mod_enabled(&env.ctx(), &env.profile.id, "Local-Mod", false).unwrap();
        let error = set_mod_enabled(&env.ctx(), &env.profile.id, "Local-Mod", true).unwrap_err();
        assert!(matches!(
            error,
            InstallError::CacheMissing {
                online_source: false,
                ..
            }
        ));
        uninstall_mod(&env.ctx(), &env.profile.id, "Local-Mod").unwrap();
        assert!(env.mods().is_empty());
    }

    #[test]
    fn rejects_bad_archives_without_leaving_anything() {
        let env = Env::new();
        let cases = [
            ("traversal.zip", vec![("..\\..\\escape.dll", "x")]),
            (
                "version.zip",
                vec![("manifest.json", r#"{"name":"Mod","version_number":"1.0"}"#)],
            ),
            ("json.zip", vec![("manifest.json", "{ not json")]),
        ];
        for (file_name, entries) in cases {
            let path = archive(&env, file_name, &entries);
            assert!(import(&env, &path).is_err(), "{file_name}");
        }
        let text = env.directory.join("notes.zip");
        fs::write(&text, "this is not an archive").unwrap();
        assert!(import(&env, &text).is_err());
        assert!(import(&env, &env.directory.join("missing.zip")).is_err());

        assert!(env.mods().is_empty());
        assert!(!env.directory.join("escape.dll").exists());
        assert!(!env.paths.root().join("local-cache").exists());
        assert_eq!(temp_entries(&env), 0);
        assert_eq!(fs::read_dir(env.profile_dir()).unwrap().count(), 0);
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_name("Cool Mod").as_deref(), Some("Cool_Mod"));
        assert_eq!(sanitize_name("  --a.b--  ").as_deref(), Some("a_b"));
        assert_eq!(sanitize_name("Mod\u{e9}!").as_deref(), Some("Mod"));
        assert_eq!(sanitize_name("../../x").as_deref(), Some("x"));
        assert_eq!(sanitize_name("***"), None);
        assert_eq!(sanitize_name(""), None);
        assert_eq!(
            sanitize_name(&"a".repeat(300)).unwrap().len(),
            MAX_NAME_CHARS
        );
    }

    #[test]
    fn a_manifest_name_that_cannot_be_used_falls_back_to_the_file_name() {
        let manifest = PackageManifest {
            name: Some("***".to_owned()),
            version_number: None,
            dependencies: Vec::new(),
        };
        let id = identify(Some(&manifest), Path::new("dir/Fallback Mod.zip")).unwrap();
        assert_eq!(id.to_string(), "Local-Fallback_Mod-0.0.0");
        assert!(identify(None, Path::new("***.zip")).is_err());
    }
}
