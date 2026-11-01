//! Installation plan for mod loader packages. Port of r2modmanPlus
//! `BepInExInstaller.ts`: the contents of the package's root folder are
//! copied into the profile root.

use std::path::Path;

use super::dependencies::{InstallRequest, PackageSource};
use super::ecosystem::PackageLoader;
use super::install_rules::{PlannedFile, walk_files};
use super::installer::ModdingContext;
use super::package_id::PackageName;

/// Package files that describe the package and are not part of the loader.
const PACKAGE_FILES: [&str; 3] = ["manifest.json", "readme.md", "icon.png"];

pub(crate) fn unsupported_loader_message(loader: PackageLoader) -> String {
    format!("The {loader:?} mod loader is not supported yet")
}

/// The request that installs the newest version of the game's mod loader:
/// the first loader package of the schema that the catalog has.
pub(crate) fn plan_loader_request(
    ctx: &ModdingContext<'_>,
    source: &dyn PackageSource,
) -> Result<InstallRequest, String> {
    let loader = ctx.game.package_loader;
    if loader != PackageLoader::Bepinex {
        return Err(unsupported_loader_message(loader));
    }
    for candidate in ctx
        .loaders
        .iter()
        .filter(|candidate| candidate.loader == loader)
    {
        let package: PackageName = candidate.package_id.parse()?;
        if source.latest(&package)?.is_some() {
            return Ok(InstallRequest {
                package,
                version: None,
            });
        }
    }
    Err("The mod loader of this game is not in the mod catalog".to_owned())
}

/// Plans the copy of `<package_root>/<root_folder>` into the profile root.
/// An empty `root_folder` selects the package root itself.
pub(crate) fn plan_loader_install(
    package_root: &Path,
    root_folder: &str,
) -> Result<Vec<PlannedFile>, String> {
    let mut loader_root = package_root.to_path_buf();
    for part in root_folder
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
    {
        if part == ".." || part == "." || part.contains(':') {
            return Err(format!(
                "Loader root folder \"{root_folder}\" is not allowed"
            ));
        }
        loader_root.push(part);
    }
    if !loader_root.is_dir() {
        return Err(format!(
            "The loader package has no \"{root_folder}\" folder"
        ));
    }
    Ok(walk_files(&loader_root)?
        .into_iter()
        .filter(|(relative, _)| {
            relative.contains('/') || !PACKAGE_FILES.contains(&relative.to_lowercase().as_str())
        })
        .map(|(relative, source)| PlannedFile {
            source,
            destination: relative,
            tracked: true,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modding::test_support::{Env, FakeRepo};
    use std::{fs, path::PathBuf};

    fn tree(files: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("legio-loader-{}", uuid::Uuid::new_v4()));
        for file in files {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, file).unwrap();
        }
        root
    }

    fn destinations(root: &Path, folder: &str) -> Vec<String> {
        let mut result: Vec<_> = plan_loader_install(root, folder)
            .unwrap()
            .into_iter()
            .map(|file| file.destination)
            .collect();
        result.sort();
        result
    }

    #[test]
    fn copies_the_root_folder_contents_to_the_profile_root() {
        let root = tree(&[
            "manifest.json",
            "icon.png",
            "README.md",
            "BepInExPack/winhttp.dll",
            "BepInExPack/doorstop_config.ini",
            "BepInExPack/BepInEx/core/BepInEx.dll",
            "BepInExPack/BepInEx/config/.keep",
        ]);
        assert_eq!(
            destinations(&root, "BepInExPack"),
            [
                "BepInEx/config/.keep",
                "BepInEx/core/BepInEx.dll",
                "doorstop_config.ini",
                "winhttp.dll",
            ]
        );
        assert!(
            plan_loader_install(&root, "BepInExPack")
                .unwrap()
                .iter()
                .all(|file| file.tracked && file.source.is_file())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn skips_package_files_at_the_loader_root_only() {
        let root = tree(&[
            "Pack/Manifest.JSON",
            "Pack/readme.md",
            "Pack/ICON.png",
            "Pack/BepInEx/README.md",
            "Pack/run.sh",
        ]);
        assert_eq!(destinations(&root, "Pack"), ["BepInEx/README.md", "run.sh"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_root_folder_uses_the_package_root() {
        let root = tree(&["manifest.json", "winhttp.dll", "BepInEx/core/a.dll"]);
        assert_eq!(
            destinations(&root, ""),
            ["BepInEx/core/a.dll", "winhttp.dll"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn requests_the_first_loader_package_the_catalog_has() {
        let env = Env::new();
        let mut repo = FakeRepo::default();
        repo.add(
            "BepInEx-BepInExPack-5.4.2100",
            &[],
            &[("manifest.json", "{}")],
        );
        let request = plan_loader_request(&env.ctx(), &repo).unwrap();
        assert_eq!(request.package.to_string(), "BepInEx-BepInExPack");
        assert_eq!(request.version, None);

        repo.add("bbepis-BepInExPack-5.4.1", &[], &[("manifest.json", "{}")]);
        let request = plan_loader_request(&env.ctx(), &repo).unwrap();
        assert_eq!(request.package.to_string(), "bbepis-BepInExPack");
    }

    #[test]
    fn loader_request_fails_without_a_catalog_package_or_support() {
        let env = Env::new();
        let error = plan_loader_request(&env.ctx(), &FakeRepo::default()).unwrap_err();
        assert!(error.contains("not in the mod catalog"), "{error}");

        let env = Env::for_game("backpack-hero");
        let mut repo = FakeRepo::default();
        repo.add("LavaGang-MelonLoader-0.7.0", &[], &[("version.dll", "x")]);
        let error = plan_loader_request(&env.ctx(), &repo).unwrap_err();
        assert!(error.contains("not supported"), "{error}");
    }

    #[test]
    fn rejects_missing_and_unsafe_root_folders() {
        let root = tree(&["a.txt"]);
        assert!(plan_loader_install(&root, "Missing").is_err());
        assert!(plan_loader_install(&root, "../x").is_err());
        assert!(plan_loader_install(&root, "C:/x").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
