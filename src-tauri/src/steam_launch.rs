use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::database::{DatabaseState, Game};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamLaunchResult {
    pub game_id: String,
    pub steam_app_id: u32,
}

pub fn launch(app: &AppHandle, game_id: &str) -> Result<SteamLaunchResult, String> {
    let game = app.state::<DatabaseState>().database()?.game(game_id)?;
    let (steam_app_id, uri) = launch_uri(&game)?;
    app.opener()
        .open_url(uri, None::<&str>)
        .map_err(|error| format!("Could not ask Steam to launch the game: {error}"))?;
    Ok(SteamLaunchResult {
        game_id: game.id,
        steam_app_id,
    })
}

fn launch_uri(game: &Game) -> Result<(u32, String), String> {
    if game.steam_account_id.is_some() {
        return Err("Account-specific launch is unavailable because Legio cannot verify the active Steam account. Clear the account preference to launch with Steam's current account.".to_owned());
    }
    let app_id = game
        .steam_app_id
        .filter(|id| *id > 0)
        .ok_or_else(|| "This game has no valid Steam App ID".to_owned())?;
    let path = game
        .steam_install_path
        .as_deref()
        .ok_or_else(|| "This game has no detected Steam installation".to_owned())?;
    if !Path::new(path).is_dir() {
        return Err(
            "Steam installation directory is missing. Rescan Steam games and retry.".to_owned(),
        );
    }
    Ok((app_id, format!("steam://run/{app_id}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Game {
        Game {
            id: "9717c6f1-3d4b-49b5-903b-c07d898c333c".to_owned(),
            steam_app_id: Some(400),
            automatic_name: Some("Portal".to_owned()),
            name_override: None,
            name: "Portal".to_owned(),
            steam_install_path: Some(std::env::temp_dir().to_string_lossy().into_owned()),
            steam_account_id: None,
        }
    }

    #[test]
    fn constructs_valid_steam_uri() {
        assert_eq!(
            launch_uri(&game()).unwrap(),
            (400, "steam://run/400".to_owned())
        );
    }

    #[test]
    fn rejects_game_without_steam_id_or_installation() {
        let mut game = game();
        game.steam_app_id = None;
        assert!(launch_uri(&game).unwrap_err().contains("Steam App ID"));
        game.steam_app_id = Some(400);
        game.steam_install_path = None;
        assert!(launch_uri(&game).unwrap_err().contains("installation"));
    }

    #[test]
    fn rejects_selected_account_before_launch() {
        let mut game = game();
        game.steam_account_id = Some("76561198000000001".to_owned());
        assert!(launch_uri(&game).unwrap_err().contains("cannot verify"));
    }
}
