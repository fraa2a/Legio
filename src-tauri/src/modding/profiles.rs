//! Mod profile lifecycle: database rows plus the profile directory on disk.

use std::{
    collections::HashSet,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{Condvar, Mutex, MutexGuard, PoisonError},
};

use serde::Serialize;

use super::paths::ModdingPaths;
use crate::database::{Database, GameModding, ModProfile, now_milliseconds};

/// Serializes operations on one profile. Install, uninstall, enable, disable,
/// duplicate and delete all hold the lock of the profile they change, and a
/// launch holds it while the loader files are copied into the game folder.
/// Clearing a cache takes every profile at once.
#[derive(Default)]
pub(crate) struct ProfileLocks {
    state: Mutex<LockState>,
    released: Condvar,
}

#[derive(Default)]
struct LockState {
    busy: HashSet<String>,
    exclusive: bool,
}

pub(crate) struct ProfileGuard<'a> {
    locks: &'a ProfileLocks,
    profile_id: String,
}

/// Holds every profile: no operation can start until it is dropped.
pub(crate) struct ExclusiveGuard<'a> {
    locks: &'a ProfileLocks,
}

impl ProfileLocks {
    /// Takes the profile if no other operation holds it.
    pub(crate) fn try_lock(&self, profile_id: &str) -> Result<Option<ProfileGuard<'_>>, String> {
        let key = profile_id.to_lowercase();
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if state.exclusive || state.busy.contains(&key) {
            return Ok(None);
        }
        state.busy.insert(key.clone());
        Ok(Some(ProfileGuard {
            locks: self,
            profile_id: key,
        }))
    }

    /// Takes every profile if no operation holds any.
    pub(crate) fn try_lock_all(&self) -> Result<Option<ExclusiveGuard<'_>>, String> {
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if state.exclusive || !state.busy.is_empty() {
            return Ok(None);
        }
        state.exclusive = true;
        Ok(Some(ExclusiveGuard { locks: self }))
    }

    /// True while a guard from [`Self::try_lock_all`] is alive.
    pub(crate) fn is_exclusive(&self) -> Result<bool, String> {
        Ok(self.state.lock().map_err(|_| poisoned())?.exclusive)
    }

    /// Waits until no other operation holds the profile.
    pub(crate) fn lock(&self, profile_id: &str) -> Result<ProfileGuard<'_>, String> {
        let key = profile_id.to_lowercase();
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        while state.exclusive || state.busy.contains(&key) {
            state = self.released.wait(state).map_err(|_| poisoned())?;
        }
        state.busy.insert(key.clone());
        Ok(ProfileGuard {
            locks: self,
            profile_id: key,
        })
    }

    fn release(&self, update: impl FnOnce(&mut LockState)) {
        let mut state: MutexGuard<'_, LockState> =
            self.state.lock().unwrap_or_else(PoisonError::into_inner);
        update(&mut state);
        self.released.notify_all();
    }
}

impl Drop for ProfileGuard<'_> {
    fn drop(&mut self) {
        self.locks.release(|state| {
            state.busy.remove(&self.profile_id);
        });
    }
}

impl Drop for ExclusiveGuard<'_> {
    fn drop(&mut self) {
        self.locks.release(|state| state.exclusive = false);
    }
}

fn poisoned() -> String {
    "Mod profile lock was poisoned".to_owned()
}

/// Result of deleting a profile. The profile is gone from the database even
/// when its directory could not be fully removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeleteProfileOutcome {
    pub(crate) directory_error: Option<String>,
}

/// The stored profile and its directory.
pub(crate) fn profile_with_directory(
    database: &Database,
    paths: &ModdingPaths,
    profile_id: &str,
) -> Result<(ModProfile, PathBuf), String> {
    let profile = database
        .mod_profile(profile_id)?
        .ok_or_else(|| "Mod profile was not found".to_owned())?;
    let directory = paths.profile_dir(&profile.game_id, &profile.id)?;
    Ok((profile, directory))
}

/// The profile of `game_id` and its directory.
pub(crate) fn profile_for_game(
    database: &Database,
    paths: &ModdingPaths,
    game_id: &str,
    profile_id: &str,
) -> Result<(ModProfile, PathBuf), String> {
    let (profile, directory) = profile_with_directory(database, paths, profile_id)?;
    if !profile.game_id.eq_ignore_ascii_case(game_id) {
        return Err("Mod profile does not belong to this game".to_owned());
    }
    Ok((profile, directory))
}

pub(crate) fn create_profile(
    database: &Database,
    paths: &ModdingPaths,
    game_id: &str,
    name: &str,
) -> Result<ModProfile, String> {
    let profile = database.create_mod_profile(game_id, name, now_milliseconds()?)?;
    let created = paths
        .profile_dir(&profile.game_id, &profile.id)
        .and_then(|directory| {
            fs::create_dir_all(&directory)
                .map_err(|error| format!("Cannot create the profile folder: {error}"))
        });
    if let Err(error) = created {
        discard_profile(database, &profile.id);
        return Err(error);
    }
    Ok(profile)
}

/// Copies the profile directory and its database records under a new name.
/// Disabled mods stay disabled in the copy.
pub(crate) fn duplicate_profile(
    database: &Database,
    paths: &ModdingPaths,
    locks: &ProfileLocks,
    profile_id: &str,
    name: &str,
) -> Result<ModProfile, String> {
    let (source, source_directory) = profile_with_directory(database, paths, profile_id)?;
    let _guard = locks.lock(&source.id)?;
    let mods = database.profile_mods(&source.id)?;
    let dependencies = database.profile_mod_dependencies(&source.id)?;
    let mut files = Vec::with_capacity(mods.len());
    for profile_mod in &mods {
        files.push(database.profile_mod_files(&source.id, &profile_mod.full_name)?);
    }

    let copy = database.create_mod_profile(&source.game_id, name, now_milliseconds()?)?;
    let copied = paths
        .profile_dir(&copy.game_id, &copy.id)
        .and_then(|directory| {
            if source_directory.is_dir() {
                copy_directory(&source_directory, &directory)?;
            } else {
                fs::create_dir_all(&directory)
                    .map_err(|error| format!("Cannot create the profile folder: {error}"))?;
            }
            for (profile_mod, tracked) in mods.iter().zip(&files) {
                let mut record = profile_mod.clone();
                record.profile_id.clone_from(&copy.id);
                database.upsert_profile_mod(&record)?;
                database.replace_profile_mod_files(&copy.id, &record.full_name, tracked)?;
                if let Some(declared) = dependencies.get(&record.full_name.to_lowercase()) {
                    database.set_profile_mod_dependencies(
                        &copy.id,
                        &record.full_name,
                        Some(declared),
                    )?;
                }
            }
            Ok(())
        });
    if let Err(error) = copied {
        let cleanup = remove_profile_data(database, paths, &copy);
        return Err(match cleanup {
            Some(cleanup) => format!("{error}; {cleanup}"),
            None => error,
        });
    }
    Ok(copy)
}

/// Deletes the profile records and directory. A game that used the profile
/// falls back to vanilla.
pub(crate) fn delete_profile(
    database: &Database,
    paths: &ModdingPaths,
    locks: &ProfileLocks,
    profile_id: &str,
) -> Result<DeleteProfileOutcome, String> {
    let (profile, directory) = profile_with_directory(database, paths, profile_id)?;
    let _guard = locks.lock(&profile.id)?;
    database.delete_mod_profile(&profile.id)?;
    let directory_error = match fs::remove_dir_all(&directory) {
        Ok(()) => None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            let message = format!(
                "The profile was deleted but its folder could not be removed ({}): {error}",
                directory.display()
            );
            crate::application_log::failure("delete_mod_profile_directory", &message);
            Some(message)
        }
    };
    Ok(DeleteProfileOutcome { directory_error })
}

/// Selects the profile launched with the game; `None` selects vanilla.
pub(crate) fn set_active_profile(
    database: &Database,
    game_id: &str,
    profile_id: Option<&str>,
) -> Result<GameModding, String> {
    database.set_active_mod_profile(game_id, profile_id)
}

/// Copies a file, replacing the target if it exists.
pub(crate) fn copy_file(source: &Path, target: &Path) -> Result<(), String> {
    let mut input =
        File::open(source).map_err(|error| format!("Cannot read {}: {error}", source.display()))?;
    let mut output = File::options()
        .write(true)
        .create(true)
        .truncate(true)
        .open(target)
        .map_err(|error| format!("Cannot write {}: {error}", target.display()))?;
    crate::filesystem_copy::copy(&mut input, &mut output)
        .map(|_| ())
        .map_err(|error| {
            format!(
                "Cannot copy {} to {}: {error}",
                source.display(),
                target.display()
            )
        })
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|error| format!("Cannot create {}: {error}", destination.display()))?;
    for item in fs::read_dir(source)
        .map_err(|error| format!("Cannot read {}: {error}", source.display()))?
    {
        let item = item.map_err(|error| format!("Cannot read {}: {error}", source.display()))?;
        let file_type = item
            .file_type()
            .map_err(|error| format!("Cannot inspect {}: {error}", item.path().display()))?;
        let target = destination.join(item.file_name());
        if file_type.is_dir() {
            copy_directory(&item.path(), &target)?;
        } else if file_type.is_file() {
            copy_file(&item.path(), &target)?;
        } else {
            return Err(format!(
                "Cannot copy {}: links and special files are not supported",
                item.path().display()
            ));
        }
    }
    Ok(())
}

fn discard_profile(database: &Database, profile_id: &str) {
    if let Err(error) = database.delete_mod_profile(profile_id) {
        crate::application_log::failure("discard_mod_profile", error);
    }
}

/// Removes a half-created profile. Returns a description of what could not be
/// cleaned up.
fn remove_profile_data(
    database: &Database,
    paths: &ModdingPaths,
    profile: &ModProfile,
) -> Option<String> {
    let mut problems = Vec::new();
    if let Ok(directory) = paths.profile_dir(&profile.game_id, &profile.id)
        && let Err(error) = fs::remove_dir_all(&directory)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        problems.push(format!(
            "the incomplete copy at {} could not be removed: {error}",
            directory.display()
        ));
    }
    if let Err(error) = database.delete_mod_profile(&profile.id) {
        problems.push(format!(
            "the incomplete profile could not be removed: {error}"
        ));
    }
    if problems.is_empty() {
        None
    } else {
        let message = problems.join("; ");
        crate::application_log::failure("cleanup_mod_profile_copy", &message);
        Some(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{CreateGameInput, ProfileMod};
    use std::sync::Arc;
    use uuid::Uuid;

    struct Fixture {
        database: Database,
        paths: ModdingPaths,
        locks: ProfileLocks,
        game_id: String,
        _directory: crate::modding::test_support::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!("legio-profiles-{}", Uuid::new_v4()));
            fs::create_dir_all(&directory).unwrap();
            let database = Database::open(&directory).unwrap();
            let game_id = database
                .create_game(CreateGameInput {
                    name: "Game".to_owned(),
                    steam_app_id: None,
                })
                .unwrap()
                .id;
            database
                .set_game_modding(&game_id, "lethal-company")
                .unwrap();
            Self {
                paths: ModdingPaths::new(&directory),
                database,
                locks: ProfileLocks::default(),
                game_id,
                _directory: crate::modding::test_support::TempDir(directory),
            }
        }

        fn profile(&self, name: &str) -> ModProfile {
            create_profile(&self.database, &self.paths, &self.game_id, name).unwrap()
        }

        fn directory_of(&self, profile: &ModProfile) -> PathBuf {
            self.paths.profile_dir(&self.game_id, &profile.id).unwrap()
        }

        fn add_mod(&self, profile: &ModProfile, name: &str, enabled: bool, files: &[&str]) {
            self.database
                .upsert_profile_mod(&ProfileMod {
                    profile_id: profile.id.clone(),
                    full_name: name.to_owned(),
                    version: "1.0.0".to_owned(),
                    enabled,
                    online_source: true,
                    installed_at: 1,
                    install_order: 0,
                })
                .unwrap();
            let tracked: Vec<String> = files.iter().map(|file| (*file).to_owned()).collect();
            self.database
                .replace_profile_mod_files(&profile.id, name, &tracked)
                .unwrap();
            if enabled {
                for file in files {
                    let path = self.directory_of(profile).join(file);
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(path, file).unwrap();
                }
            }
        }
    }

    #[test]
    fn create_makes_the_profile_directory() {
        let fixture = Fixture::new();
        let profile = fixture.profile("Main");
        assert!(fixture.directory_of(&profile).is_dir());
        assert_eq!(
            fixture.database.mod_profiles(&fixture.game_id).unwrap(),
            [profile]
        );
    }

    #[test]
    fn create_rejects_duplicate_names_without_leaving_a_directory() {
        let fixture = Fixture::new();
        fixture.profile("Main");
        let before = fs::read_dir(
            fixture
                .paths
                .game_dir(&fixture.game_id)
                .unwrap()
                .join("profiles"),
        )
        .unwrap()
        .count();
        assert!(
            create_profile(&fixture.database, &fixture.paths, &fixture.game_id, "main").is_err()
        );
        let after = fs::read_dir(
            fixture
                .paths
                .game_dir(&fixture.game_id)
                .unwrap()
                .join("profiles"),
        )
        .unwrap()
        .count();
        assert_eq!(before, after);
    }

    #[test]
    fn rename_keeps_the_directory_and_ignores_the_lock() {
        let fixture = Fixture::new();
        let profile = fixture.profile("Main");
        let _busy = fixture.locks.lock(&profile.id).unwrap();
        let renamed = fixture
            .database
            .rename_mod_profile(&profile.id, "Other")
            .unwrap();
        assert_eq!(renamed.name, "Other");
        assert_eq!(renamed.id, profile.id);
        assert!(fixture.directory_of(&profile).is_dir());
    }

    #[test]
    fn finds_a_profile_only_for_its_own_game() {
        let fixture = Fixture::new();
        let profile = fixture.profile("Main");
        let (found, directory) = profile_for_game(
            &fixture.database,
            &fixture.paths,
            &fixture.game_id.to_uppercase(),
            &profile.id,
        )
        .unwrap();
        assert_eq!(found, profile);
        assert_eq!(directory, fixture.directory_of(&profile));
        let other = Uuid::new_v4().to_string();
        let error =
            profile_for_game(&fixture.database, &fixture.paths, &other, &profile.id).unwrap_err();
        assert!(error.contains("does not belong"), "{error}");
        assert!(
            profile_for_game(
                &fixture.database,
                &fixture.paths,
                &fixture.game_id,
                &Uuid::new_v4().to_string()
            )
            .is_err()
        );
    }

    #[test]
    fn duplicate_copies_files_and_records() {
        let fixture = Fixture::new();
        let profile = fixture.profile("Main");
        fixture.add_mod(&profile, "A-On", true, &["BepInEx/plugins/A-On/a.dll"]);
        fixture.add_mod(&profile, "B-Off", false, &["BepInEx/plugins/B-Off/b.dll"]);
        fs::write(fixture.directory_of(&profile).join("log.txt"), "log").unwrap();
        fixture
            .database
            .set_profile_mod_dependencies(&profile.id, "B-Off", Some(&["A-On".to_owned()]))
            .unwrap();

        let copy = duplicate_profile(
            &fixture.database,
            &fixture.paths,
            &fixture.locks,
            &profile.id,
            "Copy",
        )
        .unwrap();
        assert_ne!(copy.id, profile.id);
        let directory = fixture.directory_of(&copy);
        assert_eq!(
            fs::read(directory.join("BepInEx/plugins/A-On/a.dll")).unwrap(),
            b"BepInEx/plugins/A-On/a.dll"
        );
        assert!(!directory.join("BepInEx/plugins/B-Off/b.dll").exists());
        assert!(directory.join("log.txt").is_file());

        let mods = fixture.database.profile_mods(&copy.id).unwrap();
        assert_eq!(mods.len(), 2);
        assert!(mods.iter().all(|record| record.profile_id == copy.id));
        assert!(
            !mods
                .iter()
                .find(|record| record.full_name == "B-Off")
                .unwrap()
                .enabled
        );
        assert_eq!(
            fixture
                .database
                .profile_mod_files(&copy.id, "B-Off")
                .unwrap(),
            ["BepInEx/plugins/B-Off/b.dll"]
        );
        let dependencies = fixture.database.profile_mod_dependencies(&copy.id).unwrap();
        assert_eq!(dependencies.len(), 1);
        assert_eq!(dependencies["b-off"], ["A-On"]);
        // The original is untouched.
        assert_eq!(fixture.database.profile_mods(&profile.id).unwrap().len(), 2);
        assert!(
            fixture
                .directory_of(&profile)
                .join("BepInEx/plugins/A-On/a.dll")
                .is_file()
        );
    }

    #[test]
    fn duplicate_with_a_taken_name_changes_nothing() {
        let fixture = Fixture::new();
        let profile = fixture.profile("Main");
        assert!(
            duplicate_profile(
                &fixture.database,
                &fixture.paths,
                &fixture.locks,
                &profile.id,
                "main"
            )
            .is_err()
        );
        assert_eq!(
            fixture
                .database
                .mod_profiles(&fixture.game_id)
                .unwrap()
                .len(),
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn duplicate_cleans_up_when_a_link_blocks_the_copy() {
        let fixture = Fixture::new();
        let profile = fixture.profile("Main");
        std::os::unix::fs::symlink("/etc/hostname", fixture.directory_of(&profile).join("link"))
            .unwrap();
        let error = duplicate_profile(
            &fixture.database,
            &fixture.paths,
            &fixture.locks,
            &profile.id,
            "Copy",
        )
        .unwrap_err();
        assert!(error.contains("links"), "{error}");
        assert_eq!(
            fixture
                .database
                .mod_profiles(&fixture.game_id)
                .unwrap()
                .len(),
            1
        );
        let profiles = fixture
            .paths
            .game_dir(&fixture.game_id)
            .unwrap()
            .join("profiles");
        assert_eq!(fs::read_dir(profiles).unwrap().count(), 1);
    }

    #[test]
    fn delete_removes_records_and_directory() {
        let fixture = Fixture::new();
        let profile = fixture.profile("Main");
        fixture.add_mod(&profile, "A-On", true, &["BepInEx/plugins/A-On/a.dll"]);
        fixture
            .database
            .set_active_mod_profile(&fixture.game_id, Some(&profile.id))
            .unwrap();

        let outcome = delete_profile(
            &fixture.database,
            &fixture.paths,
            &fixture.locks,
            &profile.id,
        )
        .unwrap();
        assert_eq!(outcome.directory_error, None);
        assert!(!fixture.directory_of(&profile).exists());
        assert!(fixture.database.mod_profile(&profile.id).unwrap().is_none());
        assert_eq!(
            fixture
                .database
                .game_modding(&fixture.game_id)
                .unwrap()
                .unwrap()
                .active_profile_id,
            None
        );
        assert!(
            delete_profile(
                &fixture.database,
                &fixture.paths,
                &fixture.locks,
                &profile.id
            )
            .is_err()
        );
    }

    #[test]
    fn delete_tolerates_a_missing_directory() {
        let fixture = Fixture::new();
        let profile = fixture.profile("Main");
        fs::remove_dir_all(fixture.directory_of(&profile)).unwrap();
        let outcome = delete_profile(
            &fixture.database,
            &fixture.paths,
            &fixture.locks,
            &profile.id,
        )
        .unwrap();
        assert_eq!(outcome.directory_error, None);
    }

    #[test]
    fn sets_and_clears_the_active_profile() {
        let fixture = Fixture::new();
        let profile = fixture.profile("Main");
        let active =
            set_active_profile(&fixture.database, &fixture.game_id, Some(&profile.id)).unwrap();
        assert_eq!(
            active.active_profile_id.as_deref(),
            Some(profile.id.as_str())
        );
        let cleared = set_active_profile(&fixture.database, &fixture.game_id, None).unwrap();
        assert_eq!(cleared.active_profile_id, None);
        assert!(
            set_active_profile(
                &fixture.database,
                &fixture.game_id,
                Some(&Uuid::new_v4().to_string())
            )
            .is_err()
        );
    }

    #[test]
    fn try_lock_refuses_a_busy_profile() {
        let locks = ProfileLocks::default();
        let profile = Uuid::new_v4().to_string();
        let held = locks.try_lock(&profile).unwrap().unwrap();
        assert!(locks.try_lock(&profile.to_uppercase()).unwrap().is_none());
        assert!(
            locks
                .try_lock(&Uuid::new_v4().to_string())
                .unwrap()
                .is_some()
        );
        drop(held);
        assert!(locks.try_lock(&profile).unwrap().is_some());
    }

    #[test]
    fn exclusive_lock_holds_off_every_profile() {
        let locks = Arc::new(ProfileLocks::default());
        let profile = Uuid::new_v4().to_string();
        let held = locks.try_lock(&profile).unwrap().unwrap();
        assert!(locks.try_lock_all().unwrap().is_none());
        drop(held);

        let exclusive = locks.try_lock_all().unwrap().unwrap();
        assert!(locks.is_exclusive().unwrap());
        assert!(locks.try_lock_all().unwrap().is_none());
        assert!(locks.try_lock(&profile).unwrap().is_none());
        let waiter = {
            let locks = locks.clone();
            let profile = profile.clone();
            std::thread::spawn(move || drop(locks.lock(&profile).unwrap()))
        };
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!waiter.is_finished());
        drop(exclusive);
        waiter.join().unwrap();
        assert!(!locks.is_exclusive().unwrap());
        assert!(locks.try_lock(&profile).unwrap().is_some());
    }

    #[test]
    fn locks_serialize_work_on_the_same_profile() {
        let locks = Arc::new(ProfileLocks::default());
        let profile = Uuid::new_v4().to_string();
        let counter = Arc::new(Mutex::new(Vec::new()));
        let first = locks.lock(&profile).unwrap();
        let handle = {
            let locks = locks.clone();
            let counter = counter.clone();
            let profile = profile.clone();
            std::thread::spawn(move || {
                let _guard = locks.lock(&profile).unwrap();
                counter.lock().unwrap().push("second");
            })
        };
        std::thread::sleep(std::time::Duration::from_millis(50));
        counter.lock().unwrap().push("first");
        drop(first);
        handle.join().unwrap();
        assert_eq!(*counter.lock().unwrap(), ["first", "second"]);
        // Other profiles are independent.
        let _a = locks.lock(&Uuid::new_v4().to_string()).unwrap();
        let _b = locks.lock(&Uuid::new_v4().to_string()).unwrap();
    }
}
