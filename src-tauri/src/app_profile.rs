pub(crate) fn configure(config: &mut tauri::Config, development: bool) {
    if development {
        config.identifier.push_str(".dev");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::Manager;

    fn app(identifier: &str, development: bool) -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = identifier.to_owned();
        configure(context.config_mut(), development);
        tauri::test::mock_builder().build(context).unwrap()
    }

    #[test]
    fn development_and_installed_apps_have_separate_persistent_paths() {
        let identifier = format!("org.legio.test.{}", uuid::Uuid::new_v4());
        let installed = app(&identifier, false);
        let development = app(&identifier, true);
        assert_eq!(installed.config().identifier, identifier);
        assert_eq!(development.config().identifier, format!("{identifier}.dev"));
        let installed_paths = [
            installed.path().app_data_dir().unwrap(),
            installed.path().app_local_data_dir().unwrap(),
            installed.path().app_config_dir().unwrap(),
            installed.path().app_cache_dir().unwrap(),
            installed.path().app_log_dir().unwrap(),
        ];
        let development_paths = [
            development.path().app_data_dir().unwrap(),
            development.path().app_local_data_dir().unwrap(),
            development.path().app_config_dir().unwrap(),
            development.path().app_cache_dir().unwrap(),
            development.path().app_log_dir().unwrap(),
        ];
        for (installed, development) in installed_paths.iter().zip(&development_paths) {
            assert!(!installed.starts_with(development));
            assert!(!development.starts_with(installed));
        }
    }

    #[test]
    fn development_database_opens_without_modifying_a_newer_installed_database() {
        let identifier = format!("org.legio.test.{}", uuid::Uuid::new_v4());
        let installed = app(&identifier, false);
        let development = app(&identifier, true);
        let installed_directory = installed.path().app_data_dir().unwrap();
        let development_directory = development.path().app_data_dir().unwrap();
        std::fs::create_dir_all(&installed_directory).unwrap();
        let installed_file = installed_directory.join("legio.sqlite3");
        let connection = rusqlite::Connection::open(&installed_file).unwrap();
        connection
            .pragma_update(None, "user_version", 1_000_000)
            .unwrap();
        drop(connection);
        let original = std::fs::read(&installed_file).unwrap();
        let database = crate::database::Database::open(&development_directory).unwrap();
        database
            .save_settings(crate::database::Settings {
                theme: crate::database::Theme::Light,
                ..crate::database::Settings::default()
            })
            .unwrap();
        drop(database);
        assert_eq!(std::fs::read(&installed_file).unwrap(), original);
        let reopened = crate::database::Database::open(&development_directory).unwrap();
        assert_eq!(
            reopened.settings().unwrap().theme,
            crate::database::Theme::Light
        );
        drop(reopened);
        std::fs::remove_dir_all(installed_directory).unwrap();
        std::fs::remove_dir_all(development_directory).unwrap();
    }
}
