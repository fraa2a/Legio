use crate::application_log;
use crate::database::{self, Database, DatabaseState, Game, PlaytimeSummary};
use crate::locale::LanguagePreference;
use std::collections::HashSet;
use std::time::Duration;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Manager};

const RECENT_LIMIT: usize = 5;
const LABEL_LIMIT: usize = 32;
const RECENT_PREFIX: &str = "recent:";
const PAUSE_DOWNLOADS: &str = "downloads-pause";
const RESUME_DOWNLOADS: &str = "downloads-resume";
const REFRESH_DELAY: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecentEntry {
    id: String,
    label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DownloadsAction {
    Pause,
    Resume,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrayMenuModel {
    language: LanguagePreference,
    recents: Vec<RecentEntry>,
    downloads: Option<DownloadsAction>,
}

impl TrayMenuModel {
    pub(crate) fn initial(language: LanguagePreference) -> Self {
        Self {
            language,
            recents: Vec::new(),
            downloads: None,
        }
    }
}

fn truncate_name(name: &str) -> String {
    let name = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        // A single ampersand would be read as a mnemonic marker.
        .replace('&', "&&");
    if name.chars().count() <= LABEL_LIMIT {
        return name;
    }
    let mut shortened: String = name.chars().take(LABEL_LIMIT - 3).collect();
    // Cutting inside an escaped pair would leave a lone marker.
    let trailing = shortened.chars().rev().take_while(|c| *c == '&').count();
    if trailing % 2 == 1 {
        shortened.pop();
    }
    shortened.push_str("...");
    shortened
}

fn recent_entries(games: &[Game], summaries: &[PlaytimeSummary]) -> Vec<RecentEntry> {
    let mut played: Vec<(i64, &Game)> = games
        .iter()
        .filter_map(|game| {
            summaries
                .iter()
                .find(|summary| summary.game_id == game.id)
                .and_then(|summary| summary.last_played_at)
                .map(|at| (at, game))
        })
        .collect();
    played.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    // The same Steam game can exist in several libraries; the home view lists it once.
    let mut seen = HashSet::new();
    played
        .into_iter()
        .filter(|(_, game)| {
            seen.insert(
                game.steam_app_id
                    .map_or_else(|| game.id.clone(), |app_id| format!("steam:{app_id}")),
            )
        })
        .take(RECENT_LIMIT)
        .map(|(_, game)| RecentEntry {
            id: game.id.clone(),
            label: truncate_name(&game.name),
        })
        .collect()
}

fn downloads_action(active: bool, user_paused: bool) -> Option<DownloadsAction> {
    if active {
        Some(DownloadsAction::Pause)
    } else if user_paused {
        Some(DownloadsAction::Resume)
    } else {
        None
    }
}

fn load_model(database: &Database) -> Result<TrayMenuModel, String> {
    let settings = database.settings()?;
    let games = database.visible_games()?;
    let summaries = database.playtime_summaries(database::now_milliseconds()?)?;
    let (active, user_paused) = crate::download_queue::pause_overview(database)?;
    Ok(TrayMenuModel {
        language: settings.language,
        recents: recent_entries(&games, &summaries),
        downloads: downloads_action(active, user_paused),
    })
}

pub(crate) fn build(app: &AppHandle, model: &TrayMenuModel) -> tauri::Result<Menu<tauri::Wry>> {
    let language = model.language;
    let mut items: Vec<Box<dyn IsMenuItem<tauri::Wry>>> = vec![Box::new(MenuItem::with_id(
        app,
        "open",
        language.text("Apri", "Open"),
        true,
        None::<&str>,
    )?)];
    if !model.recents.is_empty() {
        items.push(Box::new(PredefinedMenuItem::separator(app)?));
        items.push(Box::new(MenuItem::with_id(
            app,
            "recent-heading",
            language.text("Giochi recenti", "Recent games"),
            false,
            None::<&str>,
        )?));
        for entry in &model.recents {
            items.push(Box::new(MenuItem::with_id(
                app,
                format!("{RECENT_PREFIX}{}", entry.id),
                &entry.label,
                true,
                None::<&str>,
            )?));
        }
    }
    if let Some(action) = model.downloads {
        let (id, label) = match action {
            DownloadsAction::Pause => (
                PAUSE_DOWNLOADS,
                language.text("Metti in pausa i download", "Pause downloads"),
            ),
            DownloadsAction::Resume => (
                RESUME_DOWNLOADS,
                language.text("Riprendi i download", "Resume downloads"),
            ),
        };
        items.push(Box::new(PredefinedMenuItem::separator(app)?));
        items.push(Box::new(MenuItem::with_id(
            app,
            id,
            label,
            true,
            None::<&str>,
        )?));
    }
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(
        app,
        "quit",
        language.text("Chiudi", "Quit"),
        true,
        None::<&str>,
    )?));
    let references: Vec<&dyn IsMenuItem<tauri::Wry>> = items.iter().map(Box::as_ref).collect();
    Menu::with_items(app, &references)
}

pub(crate) fn handle_event(app: &AppHandle, id: &str) {
    if let Some(game_id) = id.strip_prefix(RECENT_PREFIX) {
        crate::launch_shortcut(app, game_id.to_owned());
        return;
    }
    let app = app.clone();
    match id {
        PAUSE_DOWNLOADS => {
            tauri::async_runtime::spawn(async move {
                if let Err(error) = crate::download_queue::pause_all(app).await {
                    application_log::failure("tray_pause_downloads", error);
                }
            });
        }
        RESUME_DOWNLOADS => {
            tauri::async_runtime::spawn(async move {
                if let Err(error) = crate::download_queue::resume_all(app).await {
                    application_log::failure("tray_resume_downloads", error);
                }
            });
        }
        _ => {}
    }
}

fn apply(app: &AppHandle, model: &TrayMenuModel) -> Result<(), String> {
    let menu_app = app.clone();
    let model = model.clone();
    app.run_on_main_thread(move || {
        let result =
            build(&menu_app, &model).and_then(|menu| match menu_app.tray_by_id("main-tray") {
                Some(tray) => tray.set_menu(Some(menu)),
                None => Ok(()),
            });
        if let Err(error) = result {
            application_log::failure("update_tray_menu", error);
        }
    })
    .map_err(|error| error.to_string())
}

/// Keeps the tray menu in step with the library, play sessions and the download queue.
pub(crate) fn start(
    app: &AppHandle,
    initial: TrayMenuModel,
) -> Result<tauri::async_runtime::JoinHandle<()>, String> {
    let state = app.state::<DatabaseState>();
    let database = state.database()?;
    let mut library = database.subscribe_library_changes();
    let mut downloads = database.subscribe_downloads_changes();
    let app = app.clone();
    Ok(tauri::async_runtime::spawn(async move {
        let mut current = initial;
        loop {
            let worker_app = app.clone();
            let loaded = tauri::async_runtime::spawn_blocking(move || {
                load_model(worker_app.state::<DatabaseState>().database()?)
            })
            .await
            .map_err(|error| format!("Tray menu task failed: {error}"))
            .and_then(|result| result);
            match loaded {
                Ok(model) if model != current => match apply(&app, &model) {
                    Ok(()) => current = model,
                    Err(error) => application_log::failure("update_tray_menu", error),
                },
                Ok(_) => {}
                Err(error) => application_log::failure("load_tray_menu", error),
            }
            tokio::select! {
                changed = library.changed() => if changed.is_err() { break },
                changed = downloads.changed() => if changed.is_err() { break },
            }
            // Download progress is persisted twice a second; the delay coalesces bursts.
            tokio::time::sleep(REFRESH_DELAY).await;
            library.borrow_and_update();
            downloads.borrow_and_update();
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(id: &str, name: &str, steam_app_id: Option<u32>) -> Game {
        Game {
            id: id.to_owned(),
            steam_app_id,
            automatic_name: None,
            name_override: None,
            name: name.to_owned(),
            steam_install_path: None,
            steam_account_id: None,
            executable_path: None,
            installation_root: None,
        }
    }

    fn summary(id: &str, last_played_at: Option<i64>) -> PlaytimeSummary {
        PlaytimeSummary {
            game_id: id.to_owned(),
            total_milliseconds: 0,
            active_sessions: 0,
            last_played_at,
        }
    }

    #[test]
    fn recents_are_newest_first_and_capped() {
        let games: Vec<Game> = (0..7)
            .map(|n| game(&format!("g{n}"), "Game", None))
            .collect();
        let summaries: Vec<_> = (0..7).map(|n| summary(&format!("g{n}"), Some(n))).collect();
        let ids: Vec<_> = recent_entries(&games, &summaries)
            .into_iter()
            .map(|entry| entry.id)
            .collect();
        assert_eq!(ids, ["g6", "g5", "g4", "g3", "g2"]);
    }

    #[test]
    fn recents_skip_unplayed_games() {
        let games = [game("a", "A", None), game("b", "B", None)];
        let summaries = [summary("a", None), summary("b", Some(1))];
        let entries = recent_entries(&games, &summaries);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "b");
    }

    #[test]
    fn recents_list_a_steam_game_once() {
        let games = [game("a", "A", Some(10)), game("b", "A", Some(10))];
        let summaries = [summary("a", Some(1)), summary("b", Some(2))];
        let entries = recent_entries(&games, &summaries);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "b");
    }

    #[test]
    fn long_names_are_truncated() {
        let label = truncate_name(&"x".repeat(60));
        assert_eq!(label.chars().count(), LABEL_LIMIT);
        assert!(label.ends_with("..."));
        assert_eq!(truncate_name("Short"), "Short");
        assert_eq!(
            truncate_name(&"x".repeat(LABEL_LIMIT)).chars().count(),
            LABEL_LIMIT
        );
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        let label = truncate_name(&"é".repeat(40));
        assert_eq!(label.chars().count(), LABEL_LIMIT);
    }

    #[test]
    fn ampersands_are_escaped_and_never_split() {
        assert_eq!(truncate_name("Tom & Jerry"), "Tom && Jerry");
        // The escaped pair starts at the last kept character.
        let name = format!("{}&tail-of-the-name", "x".repeat(LABEL_LIMIT - 4));
        let ampersands = truncate_name(&name).chars().filter(|c| *c == '&').count();
        assert_eq!(ampersands % 2, 0);
    }

    #[test]
    fn whitespace_in_names_is_normalized() {
        assert_eq!(truncate_name("  A\n  B\t"), "A B");
    }

    #[test]
    fn downloads_action_follows_queue_state() {
        assert_eq!(downloads_action(true, true), Some(DownloadsAction::Pause));
        assert_eq!(downloads_action(false, true), Some(DownloadsAction::Resume));
        assert_eq!(downloads_action(false, false), None);
    }
}
