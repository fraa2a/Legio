//! Exporting Legio games as Steam non-Steam shortcuts for one Steam account.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{
    database::{Database, Game},
    game_artwork::{ArtworkKind, GameArtworkStore},
    steam_shortcuts::{self as shortcuts, Action, Change, ExportContext, ExportGame},
};

const STEAM_ID64_BASE: u64 = 76_561_197_960_265_728;
const MAX_GAMES: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutAccount {
    steam_id: String,
    display_name: String,
    legio_shortcuts: usize,
    total_shortcuts: usize,
    /// Why this account cannot be edited, such as an unreadable shortcuts file.
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutAccounts {
    accounts: Vec<ShortcutAccount>,
    steam_running: bool,
    diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewItem {
    game_id: String,
    name: String,
    action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPreview {
    items: Vec<PreviewItem>,
    steam_running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    added: usize,
    updated: usize,
    unchanged: usize,
    artwork_written: usize,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalResult {
    removed: usize,
}

fn account_id(steam_id: &str) -> Result<u32, String> {
    let invalid = || "The Steam account ID is invalid".to_owned();
    if steam_id.is_empty() || !steam_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid());
    }
    let id = steam_id.parse::<u64>().map_err(|_| invalid())?;
    let account = id.checked_sub(STEAM_ID64_BASE).ok_or_else(invalid)?;
    u32::try_from(account)
        .ok()
        .filter(|account| *account != 0)
        .ok_or_else(invalid)
}

fn steam_is_running() -> Result<bool, String> {
    crate::steam_process::is_running()
}

fn canonical_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for root in crate::steam_local::default_steam_roots().unwrap_or_default() {
        if let Ok(root) = dunce::canonicalize(root)
            && !roots.contains(&root)
        {
            roots.push(root);
        }
    }
    roots
}

fn locate_config(roots: &[PathBuf], steam_id: &str) -> Result<PathBuf, String> {
    let account = account_id(steam_id)?;
    let mut last_error = None;
    for root in roots {
        match shortcuts::config_directory(root, account) {
            Ok(config) => return Ok(config),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| "Steam was not found on this device".to_owned()))
}

fn require_saved_account(steam_id: &str) -> Result<(), String> {
    crate::steam_local::scan_saved_accounts()
        .accounts
        .iter()
        .any(|account| account.steam_id == steam_id)
        .then_some(())
        .ok_or_else(|| "Steam account is not in the local saved account list".to_owned())
}

pub fn list_accounts() -> Result<ShortcutAccounts, String> {
    let launcher = crate::desktop_shortcuts::launcher_executable()?;
    let saved = crate::steam_local::scan_saved_accounts();
    let roots = canonical_roots();
    let mut diagnostics = saved.diagnostics;
    let mut accounts = Vec::new();
    for account in saved.accounts {
        let Ok(config) = locate_config(&roots, &account.steam_id) else {
            continue;
        };
        let (legio_shortcuts, total_shortcuts, error) = match shortcuts::read_shortcuts(&config) {
            Ok((file, _)) => (
                file.owned_game_ids(&launcher).len(),
                file.shortcut_count(),
                None,
            ),
            Err(error) => (0, 0, Some(error)),
        };
        accounts.push(ShortcutAccount {
            steam_id: account.steam_id,
            display_name: account.display_name,
            legio_shortcuts,
            total_shortcuts,
            error,
        });
    }
    let steam_running = match steam_is_running() {
        Ok(running) => running,
        Err(error) => {
            diagnostics.push(format!("Could not tell whether Steam is running: {error}"));
            false
        }
    };
    Ok(ShortcutAccounts {
        accounts,
        steam_running,
        diagnostics,
    })
}

fn is_exportable(game: &Game) -> bool {
    let steam_game = game.steam_install_path.is_some() && game.steam_app_id.is_some();
    (steam_game || game.executable_path.is_some()) && !steam_name(&game.name).is_empty()
}

fn steam_name(name: &str) -> String {
    name.chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .to_owned()
}

fn export_games(
    database: &Database,
    artwork: &GameArtworkStore,
    game_ids: &[String],
) -> Result<Vec<ExportGame>, String> {
    if game_ids.is_empty() {
        return Err("Select at least one game".to_owned());
    }
    if game_ids.len() > MAX_GAMES {
        return Err("Too many games selected".to_owned());
    }
    let mut games: Vec<ExportGame> = Vec::with_capacity(game_ids.len());
    for id in game_ids {
        let game = database.game(id)?;
        if !is_exportable(&game) {
            return Err(format!(
                "{} cannot be launched from Steam: it has no installation or executable",
                game.name
            ));
        }
        if games.iter().any(|known| known.game_id == game.id) {
            continue;
        }
        games.push(ExportGame {
            has_icon: artwork.path(&game.id, ArtworkKind::Icon)?.is_some(),
            name: steam_name(&game.name),
            game_id: game.id,
        });
    }
    Ok(games)
}

fn preview_items(changes: Vec<Change>) -> Vec<PreviewItem> {
    changes
        .into_iter()
        .map(|change| PreviewItem {
            game_id: change.game_id,
            name: change.name,
            action: change.action,
        })
        .collect()
}

pub fn preview(
    database: &Database,
    artwork: &GameArtworkStore,
    steam_id: &str,
    game_ids: &[String],
) -> Result<ExportPreview, String> {
    let launcher = crate::desktop_shortcuts::launcher_executable()?;
    let config = locate_config(&canonical_roots(), steam_id)?;
    let games = export_games(database, artwork, game_ids)?;
    let (file, _) = shortcuts::read_shortcuts(&config)?;
    let grid = shortcuts::grid_directory(&config);
    let context = ExportContext {
        launcher: &launcher,
        grid_dir: &grid,
    };
    let changes = shortcuts::plan_export(&file, &context, &games).map_err(|e| e.to_string())?;
    Ok(ExportPreview {
        items: preview_items(changes),
        steam_running: steam_is_running()?,
    })
}

pub fn export(
    database: &Database,
    artwork: &GameArtworkStore,
    steam_id: &str,
    game_ids: &[String],
) -> Result<ExportResult, String> {
    require_saved_account(steam_id)?;
    let launcher = crate::desktop_shortcuts::launcher_executable()?;
    let config = locate_config(&canonical_roots(), steam_id)?;
    let games = export_games(database, artwork, game_ids)?;
    let grid = shortcuts::grid_directory(&config);
    let context = ExportContext {
        launcher: &launcher,
        grid_dir: &grid,
    };
    let changes = shortcuts::modify_shortcuts(&config, &steam_is_running, |file| {
        let changes = shortcuts::plan_export(file, &context, &games)?;
        shortcuts::apply_export(file, &context, &changes);
        Ok(changes)
    })?;
    let mut result = ExportResult {
        added: count(&changes, Action::Add),
        updated: count(&changes, Action::Update),
        unchanged: count(&changes, Action::Unchanged),
        artwork_written: 0,
        warnings: Vec::new(),
    };
    write_artwork(artwork, &config, &changes, &mut result);
    Ok(result)
}

pub fn remove(steam_id: &str, game_ids: Option<&[String]>) -> Result<RemovalResult, String> {
    require_saved_account(steam_id)?;
    let launcher = crate::desktop_shortcuts::launcher_executable()?;
    let config = locate_config(&canonical_roots(), steam_id)?;
    let removed = shortcuts::modify_shortcuts(&config, &steam_is_running, |file| {
        let removals = shortcuts::plan_removal(file, &launcher, game_ids);
        shortcuts::apply_removal(file, &removals);
        Ok(removals.len())
    })?;
    Ok(RemovalResult { removed })
}

fn count(changes: &[Change], action: Action) -> usize {
    changes
        .iter()
        .filter(|change| change.action == action)
        .count()
}

// Artwork is a convenience: failures become warnings and existing files are kept,
// since Steam writes the same file names when a person picks artwork in its own UI.
fn write_artwork(
    store: &GameArtworkStore,
    config: &Path,
    changes: &[Change],
    result: &mut ExportResult,
) {
    let mut grid: Option<PathBuf> = None;
    for change in changes {
        let targets = [
            (
                ArtworkKind::Icon,
                vec![format!("{}_icon.png", change.app_id)],
            ),
            (
                ArtworkKind::Banner,
                vec![
                    format!("{}_hero.png", change.app_id),
                    format!("{}.png", change.app_id),
                ],
            ),
        ];
        for (kind, names) in targets {
            let bytes = match store.get(&change.game_id, kind) {
                Ok(Some(artwork)) => artwork.into_bytes(),
                Ok(None) => continue,
                Err(error) => {
                    result.warnings.push(format!("{}: {error}", change.name));
                    continue;
                }
            };
            let png = match crate::image_trim::png_asset(&bytes) {
                Ok(png) => png,
                Err(error) => {
                    result
                        .warnings
                        .push(format!("{}: artwork was skipped: {error}", change.name));
                    continue;
                }
            };
            for name in names {
                if grid.is_none() {
                    match shortcuts::create_grid_directory(config) {
                        Ok(directory) => grid = Some(directory),
                        Err(error) => {
                            result.warnings.push(error);
                            return;
                        }
                    }
                }
                let Some(directory) = grid.as_deref() else {
                    return;
                };
                match shortcuts::write_new_file(&directory.join(&name), &png) {
                    Ok(true) => result.artwork_written += 1,
                    Ok(false) => {}
                    Err(error) => result
                        .warnings
                        .push(format!("{}: could not write {name}: {error}", change.name)),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_id_converts_steam_id64_and_rejects_bad_input() {
        assert_eq!(account_id("76561198000000001"), Ok(39_734_273));
        for bad in [
            "",
            "abc",
            "../1",
            "76561197960265728",
            "12",
            "-1",
            "1e5",
            "99999999999999999999",
        ] {
            assert!(account_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn locate_config_requires_a_real_account_folder() {
        let root = std::env::temp_dir().join(format!("legio-steam-root-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("userdata/39734273")).unwrap();
        let roots = [root.clone()];
        assert_eq!(
            locate_config(&roots, "76561198000000001"),
            Ok(root.join("userdata/39734273/config"))
        );
        assert!(locate_config(&roots, "76561198000000002").is_err());
        assert!(locate_config(&[], "76561198000000001").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn steam_names_drop_control_characters() {
        assert_eq!(steam_name("  A\u{0}B\n "), "AB");
    }
}
