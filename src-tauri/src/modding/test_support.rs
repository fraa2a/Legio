//! Zip fixtures, a fake package source and a database-backed profile for modding tests.

use std::{
    cell::Cell,
    collections::BTreeMap,
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

use zip::{ZipWriter, write::SimpleFileOptions};

use super::dependencies::{InstallRequest, PackageSource, ResolveMode, ResolvedPackage};
use super::ecosystem::EcosystemSchema;
use super::installer::{
    InstallError, InstallOutcome, ModdingContext, ProgressSink, install_packages,
};
use super::package_id::{PackageName, PackageVersionId};
use super::paths::ModdingPaths;
use super::profiles::ProfileLocks;
use crate::database::{CreateGameInput, Database, ModProfile, ProfileMod};

const FIXTURE: &str = include_str!("../../test-fixtures/modding/ecosystem-schema.json");

pub(super) fn zip_bytes(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(content.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

pub(super) fn write_zip(path: &Path, entries: &[(&str, &str)]) {
    std::fs::write(path, zip_bytes(entries)).unwrap();
}

struct FakePackage {
    dependencies: Vec<PackageVersionId>,
    files: Vec<(String, String)>,
}

/// In-memory package repository: metadata for the source side and zip
/// archives for the fetch side.
#[derive(Default)]
pub(super) struct FakeRepo {
    packages: BTreeMap<PackageVersionId, FakePackage>,
    fetches: Cell<usize>,
}

impl FakeRepo {
    pub(super) fn add(&mut self, id: &str, dependencies: &[&str], files: &[(&str, &str)]) {
        self.packages.insert(
            id.parse().unwrap(),
            FakePackage {
                dependencies: dependencies
                    .iter()
                    .map(|value| value.parse().unwrap())
                    .collect(),
                files: files
                    .iter()
                    .map(|(name, content)| ((*name).to_owned(), (*content).to_owned()))
                    .collect(),
            },
        );
    }

    pub(super) fn fetch(
        &self,
        package: &ResolvedPackage,
        destination: &Path,
    ) -> Result<(), String> {
        self.fetches.set(self.fetches.get() + 1);
        let found = self
            .packages
            .get(&package.id)
            .ok_or_else(|| format!("{} is not in the fake repository", package.id))?;
        let entries: Vec<(&str, &str)> = found
            .files
            .iter()
            .map(|(name, content)| (name.as_str(), content.as_str()))
            .collect();
        write_zip(destination, &entries);
        Ok(())
    }

    pub(super) fn fetch_count(&self) -> usize {
        self.fetches.get()
    }

    fn resolved(&self, id: &PackageVersionId) -> ResolvedPackage {
        ResolvedPackage {
            id: id.clone(),
            dependencies: self.packages[id].dependencies.clone(),
            download_url: format!("https://example.invalid/{id}.zip"),
            file_size: 1,
        }
    }
}

impl PackageSource for FakeRepo {
    fn latest(&self, package: &PackageName) -> Result<Option<ResolvedPackage>, String> {
        Ok(self
            .packages
            .keys()
            .filter(|candidate| candidate.package == *package)
            .max_by_key(|candidate| candidate.version)
            .map(|found| self.resolved(found)))
    }

    fn exact(&self, id: &PackageVersionId) -> Result<Option<ResolvedPackage>, String> {
        Ok(self.packages.contains_key(id).then(|| self.resolved(id)))
    }
}

/// A game with one profile in a temporary data directory.
pub(super) struct Env {
    pub(super) database: Database,
    pub(super) paths: ModdingPaths,
    pub(super) locks: ProfileLocks,
    pub(super) schema: EcosystemSchema,
    pub(super) game_key: &'static str,
    pub(super) game_id: String,
    pub(super) profile: ModProfile,
    pub(super) directory: TempDir,
}

impl Env {
    pub(super) fn new() -> Self {
        Self::for_game("lethal-company")
    }

    pub(super) fn for_game(game_key: &'static str) -> Self {
        let directory =
            std::env::temp_dir().join(format!("legio-install-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let database = Database::open(&directory).unwrap();
        let game_id = database
            .create_game(CreateGameInput {
                name: "Game".to_owned(),
                steam_app_id: None,
            })
            .unwrap()
            .id;
        database.set_game_modding(&game_id, game_key).unwrap();
        let paths = ModdingPaths::new(&directory);
        let profile =
            crate::modding::profiles::create_profile(&database, &paths, &game_id, "Main").unwrap();
        Self {
            database,
            paths,
            locks: ProfileLocks::default(),
            schema: serde_json::from_str(FIXTURE).unwrap(),
            game_key,
            game_id,
            profile,
            directory: TempDir(directory),
        }
    }

    pub(super) fn ctx(&self) -> ModdingContext<'_> {
        ModdingContext {
            database: &self.database,
            paths: &self.paths,
            locks: &self.locks,
            game: self.schema.games[self.game_key].game_entry().unwrap(),
            loaders: &self.schema.modloader_packages,
        }
    }

    pub(super) fn profile_dir(&self) -> PathBuf {
        self.paths
            .profile_dir(&self.game_id, &self.profile.id)
            .unwrap()
    }

    pub(super) fn install(
        &self,
        repo: &FakeRepo,
        requests: &[&str],
        mode: ResolveMode,
    ) -> Result<InstallOutcome, InstallError> {
        self.install_with(repo, requests, mode, &|_| {})
    }

    pub(super) fn install_with(
        &self,
        repo: &FakeRepo,
        requests: &[&str],
        mode: ResolveMode,
        progress: ProgressSink<'_>,
    ) -> Result<InstallOutcome, InstallError> {
        let requests: Vec<_> = requests.iter().map(|value| request(value)).collect();
        install_packages(
            &self.ctx(),
            &self.profile.id,
            &requests,
            repo,
            &|package, destination| repo.fetch(package, destination),
            mode,
            progress,
        )
    }

    pub(super) fn mods(&self) -> Vec<ProfileMod> {
        self.database.profile_mods(&self.profile.id).unwrap()
    }

    pub(super) fn files_of(&self, name: &str) -> Vec<String> {
        self.database
            .profile_mod_files(&self.profile.id, name)
            .unwrap()
    }

    pub(super) fn exists(&self, relative: &str) -> bool {
        self.profile_dir().join(relative).exists()
    }
}

/// Removes the directory when dropped. Declared after the database in
/// [`Env`] so the database file is closed first.
pub(super) struct TempDir(pub(super) PathBuf);

impl std::ops::Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

pub(super) fn request(value: &str) -> InstallRequest {
    match value.parse::<PackageVersionId>() {
        Ok(id) => InstallRequest {
            package: id.package,
            version: Some(id.version),
        },
        Err(_) => InstallRequest {
            package: value.parse().unwrap(),
            version: None,
        },
    }
}
