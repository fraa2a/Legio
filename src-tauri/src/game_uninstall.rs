use crate::database::{Database, DatabaseState, database_error};
use crate::game_lifecycle::GameLaunchManager;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};

fn checked_target(database: &Database, id: &str, protected: &[PathBuf]) -> Result<PathBuf, String> {
    let game = database.game(id)?;
    if game.steam_install_path.is_some() {
        return Err("Uninstall Steam-managed games through Steam".into());
    }
    let recorded = Path::new(
        game.installation_root
            .as_deref()
            .ok_or("The game has no recorded installation directory")?,
    );
    if !recorded.is_absolute()
        || fs::symlink_metadata(recorded)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err(
            "The installation directory must be an absolute path without a symbolic link".into(),
        );
    }
    let executable = dunce::canonicalize(
        game.executable_path
            .as_deref()
            .ok_or("Select a game executable first")?,
    )
    .map_err(|e| format!("Could not inspect game executable: {e}"))?;
    let root =
        crate::game_library::installation_root(Some(&recorded.to_string_lossy()), &executable)?;
    if root.parent().is_none() || protected.iter().any(|path| path.starts_with(&root)) {
        return Err("This directory contains protected application or user files".into());
    }
    #[cfg(target_os = "linux")]
    if [
        "/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/boot", "/proc", "/sys", "/dev",
        "/run", "/var",
    ]
    .iter()
    .any(|path| root.starts_with(path))
        || ["/opt", "/tmp", "/home", "/mnt", "/media"]
            .iter()
            .any(|path| root == Path::new(path))
    {
        return Err("System directories cannot be uninstalled by Legio".into());
    }
    #[cfg(windows)]
    if std::env::var_os("SystemRoot").is_some_and(|path| root.starts_with(Path::new(&path))) {
        return Err("System directories cannot be uninstalled by Legio".into());
    }
    for other in database.games()? {
        if other.id == id {
            continue;
        }
        for path in [
            other.installation_root,
            other.steam_install_path,
            other.executable_path,
        ]
        .into_iter()
        .flatten()
        {
            let path = match dunce::canonicalize(&path) {
                Ok(path) => path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => PathBuf::from(path),
                Err(error) => {
                    return Err(format!("Could not inspect another game's path: {error}"));
                }
            };
            if root.starts_with(&path) || path.starts_with(&root) {
                return Err("Another library game shares this installation directory".into());
            }
        }
    }
    database.with_connection(|connection| {
        let busy: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM downloads WHERE (id=?1 OR target_game_id=?1) AND status NOT IN ('installed','cancelled')) OR EXISTS(SELECT 1 FROM game_transfers WHERE game_id=?1) OR EXISTS(SELECT 1 FROM game_update_journals WHERE game_id=?1)", [id], |row| row.get(0)).map_err(database_error)?;
        if busy { return Err("Finish or cancel this game's download, update or transfer before uninstalling".into()); }
        let mut query = connection.prepare("SELECT root FROM game_libraries").map_err(database_error)?;
        for library in query.query_map([], |row| row.get::<_, String>(0)).map_err(database_error)? {
            let library = dunce::canonicalize(library.map_err(database_error)?).map_err(|e| e.to_string())?;
            if library.starts_with(&root) { return Err("A game library directory cannot be uninstalled".into()); }
        }
        Ok(())
    })?;
    if crate::download_policy::external_busy(&root)? {
        return Err("Close the game before uninstalling its files".into());
    }
    Ok(root)
}

fn remove_installation(
    database: &Database,
    id: &str,
    expected: &Path,
    protected: &[PathBuf],
) -> Result<(), String> {
    let root = checked_target(database, id, protected)?;
    if root != expected {
        return Err(
            "The installation directory changed. Close the dialog and confirm the new directory"
                .into(),
        );
    }
    fs::remove_dir_all(&root)
        .map_err(|e| format!("Could not remove game files. The library entry was kept: {e}"))?;
    database.remove_game(id)
}

fn protected_directories(app: &AppHandle) -> Result<Vec<PathBuf>, String> {
    let mut paths: Vec<PathBuf> = [
        dirs::home_dir(),
        dirs::desktop_dir(),
        dirs::document_dir(),
        dirs::download_dir(),
        dirs::audio_dir(),
        dirs::picture_dir(),
        dirs::video_dir(),
        dirs::data_dir(),
        dirs::config_dir(),
        dirs::cache_dir(),
    ]
    .into_iter()
    .flatten()
    .collect();
    paths.push(app.path().app_data_dir().map_err(|e| e.to_string())?);
    paths.push(std::env::current_exe().map_err(|e| e.to_string())?);
    #[cfg(windows)]
    for name in ["ProgramFiles", "ProgramFiles(x86)", "ProgramData", "PUBLIC"] {
        if let Some(path) = std::env::var_os(name) {
            paths.push(path.into());
        }
    }
    paths
        .into_iter()
        .filter(|path| path.exists())
        .map(|path| dunce::canonicalize(path).map_err(|e| e.to_string()))
        .collect()
}

#[tauri::command]
pub async fn get_game_uninstall_target(app: AppHandle, id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::startup_recovery::require_ready(&app)?;
        let manager = app.state::<GameLaunchManager>();
        let _operation = manager.operation()?;
        manager.require_idle(&id)?;
        checked_target(
            app.state::<DatabaseState>().database()?,
            &id,
            &protected_directories(&app)?,
        )
        .map(|root| root.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| format!("Could not inspect game installation: {e}"))?
}

#[tauri::command]
pub async fn uninstall_game(
    app: AppHandle,
    id: String,
    installation_root: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::startup_recovery::require_ready(&app)?;
        let manager = app.state::<GameLaunchManager>();
        let _operation = manager.operation()?;
        manager.require_idle(&id)?;
        remove_installation(
            app.state::<DatabaseState>().database()?,
            &id,
            Path::new(&installation_root),
            &protected_directories(&app)?,
        )?;
        crate::game_lifecycle::cleanup_removed_game(&app, &id)?;
        crate::game_library::remove_default(&app, &id)
    })
    .await
    .map_err(|e| format!("Game uninstall task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{CreateGameInput, database_error};
    use rusqlite::params;

    struct Fixture {
        root: PathBuf,
        directory: PathBuf,
        database: Database,
        id: String,
    }

    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("legio-uninstall-{}", uuid::Uuid::new_v4()));
            let directory = root.join("Game");
            fs::create_dir_all(directory.join("bin")).unwrap();
            fs::write(directory.join("bin/game.exe"), b"game").unwrap();
            let database = Database::open(&root.join("data")).unwrap();
            let game = database
                .create_game(CreateGameInput {
                    name: "Game".into(),
                    steam_app_id: Some(400),
                })
                .unwrap();
            database
                .with_connection(|connection| {
                    connection
                        .execute(
                            "UPDATE games SET executable_path=?2, installation_root=?3 WHERE id=?1",
                            params![
                                game.id,
                                directory.join("bin/game.exe").to_string_lossy(),
                                directory.to_string_lossy()
                            ],
                        )
                        .map_err(database_error)?;
                    Ok(())
                })
                .unwrap();
            Self {
                root,
                directory,
                database,
                id: game.id,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn uninstall_removes_only_the_confirmed_game_and_library_entry() {
        let f = Fixture::new();
        fs::write(f.root.join("keep.txt"), b"keep").unwrap();
        let target = checked_target(&f.database, &f.id, &[]).unwrap();
        remove_installation(&f.database, &f.id, &target, &[]).unwrap();
        assert!(!f.directory.exists());
        assert!(f.database.game(&f.id).is_err());
        assert!(f.root.join("keep.txt").is_file());
    }

    #[test]
    fn stale_confirmation_cannot_delete_a_different_directory() {
        let f = Fixture::new();
        assert!(remove_installation(&f.database, &f.id, &f.root, &[]).is_err());
        assert!(f.directory.is_dir());
        assert!(f.database.game(&f.id).is_ok());
    }

    #[test]
    fn steam_managed_and_protected_directories_are_rejected() {
        let f = Fixture::new();
        assert!(checked_target(&f.database, &f.id, std::slice::from_ref(&f.directory)).is_err());
        f.database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE games SET steam_install_path=?2 WHERE id=?1",
                        params![f.id, f.directory.to_string_lossy()],
                    )
                    .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        assert!(checked_target(&f.database, &f.id, &[]).is_err());
        assert!(f.directory.is_dir());
    }

    #[test]
    fn shared_installations_are_rejected() {
        let f = Fixture::new();
        let other = f
            .database
            .create_game(CreateGameInput {
                name: "Other".into(),
                steam_app_id: None,
            })
            .unwrap();
        f.database
            .with_connection(|connection| {
                connection
                    .execute(
                        "UPDATE games SET installation_root=?2 WHERE id=?1",
                        params![other.id, f.directory.join("bin").to_string_lossy()],
                    )
                    .map_err(database_error)?;
                Ok(())
            })
            .unwrap();
        assert!(checked_target(&f.database, &f.id, &[]).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn nested_symlinks_never_delete_files_outside_the_installation() {
        let f = Fixture::new();
        let outside = f.root.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep.txt"), b"keep").unwrap();
        std::os::unix::fs::symlink(&outside, f.directory.join("linked")).unwrap();
        let target = checked_target(&f.database, &f.id, &[]).unwrap();
        remove_installation(&f.database, &f.id, &target, &[]).unwrap();
        assert!(outside.join("keep.txt").is_file());
    }
}
