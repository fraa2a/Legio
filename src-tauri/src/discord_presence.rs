use std::{io, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod ipc;
use ipc::Connection;
use tauri::Manager;
use tokio::{sync::watch, time::Instant};

use crate::{
    database::{Database, DatabaseState},
    game_artwork::{ArtworkKind, GameArtworkStore},
    game_lifecycle::{GameLaunchManager, GameStatus},
    steam_assets::AssetCacheState,
};

const APPLICATION_ID: &str = "1557120430475575366";
// Discord only loads external images from public https URLs.
const LEGIO_ICON_URL: &str =
    "https://raw.githubusercontent.com/fraa2a/Legio/main/src-tauri/icons/icon.png";
const MAX_IMAGE_URL_LENGTH: usize = 256;

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const RETRY_INTERVAL: Duration = Duration::from_secs(15);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PresenceSettings {
    pub enabled: bool,
}

impl Default for PresenceSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

pub(crate) struct PresenceService {
    refresh: watch::Sender<()>,
    worker: tauri::async_runtime::JoinHandle<()>,
}

impl PresenceService {
    pub(crate) fn start(app: &tauri::AppHandle) -> io::Result<Self> {
        let database = app
            .state::<DatabaseState>()
            .shared_database()
            .map_err(io::Error::other)?;
        let manager = app.state::<GameLaunchManager>().inner().clone();
        let artwork = app.state::<GameArtworkStore>().inner().clone();
        let assets = app.state::<AssetCacheState>().inner().clone();
        let lifecycle = manager.subscribe_changes();
        let (refresh, receiver) = watch::channel(());
        let worker = tauri::async_runtime::spawn(run(
            database, manager, artwork, assets, receiver, lifecycle,
        ));
        Ok(Self { refresh, worker })
    }

    pub(crate) fn refresh(&self) {
        self.refresh.send_replace(());
    }

    pub(crate) fn shutdown(&self) {
        self.worker.abort();
    }
}

impl Drop for PresenceService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Activity {
    name: Option<String>,
    started_at: Option<i64>,
    icon_url: Option<String>,
}

impl Activity {
    fn payload(&self) -> Value {
        let mut activity = json!({
            "details": self.name.as_deref().unwrap_or("Browsing the library"),
            "state": if self.name.is_some() { "Playing with Legio" } else { "Legio Launcher" },
        });
        if let Some(start) = self.started_at {
            activity["timestamps"] = json!({ "start": start });
        }
        if self.name.is_some()
            && let Some(icon_url) = self.icon_url.as_deref().filter(|url| is_https_url(url))
        {
            activity["assets"] = json!({
                "large_image": icon_url,
                "small_image": LEGIO_ICON_URL,
                "small_text": "Legio Launcher",
            });
        }
        activity
    }
}

fn is_https_url(url: &str) -> bool {
    url.len() <= MAX_IMAGE_URL_LENGTH
        && reqwest::Url::parse(url)
            .is_ok_and(|url| url.scheme() == "https" && url.host_str().is_some())
}

struct Snapshot {
    settings: PresenceSettings,
    activity: Activity,
    // Steam app whose public icon can represent the running game.
    icon_app_id: Option<u32>,
    warning: Option<String>,
}

fn snapshot(
    database: &Database,
    manager: &GameLaunchManager,
    artwork: &GameArtworkStore,
) -> Result<Snapshot, String> {
    let settings = database.settings()?.discord_presence;
    let mut snapshot = Snapshot {
        settings,
        activity: Activity {
            name: None,
            started_at: None,
            icon_url: None,
        },
        icon_app_id: None,
        warning: None,
    };
    if !snapshot.settings.enabled {
        return Ok(snapshot);
    }
    let states = manager.list()?;
    let running: Vec<_> = states
        .iter()
        .filter(|state| state.status == GameStatus::Running)
        .map(|state| state.game_id.as_str())
        .collect();
    if let Some(game) = database.running_game_presence(&running)? {
        snapshot.activity.name = Some(limit_text(&game.name));
        snapshot.activity.started_at = game.started_at.map(|milliseconds| milliseconds / 1000);
        // A custom icon exists only on this device, so it cannot be shared.
        match artwork.path(&game.game_id, ArtworkKind::Icon) {
            Ok(None) => snapshot.icon_app_id = game.steam_app_id,
            Ok(Some(_)) => {}
            Err(error) => snapshot.warning = Some(error),
        }
    }
    Ok(snapshot)
}

fn limit_text(text: &str) -> String {
    let mut value = String::new();
    for character in text.chars().filter(|character| !character.is_control()) {
        if value.len() + character.len_utf8() > 128 {
            break;
        }
        value.push(character);
    }
    if value.trim().is_empty() {
        "Unknown game".to_owned()
    } else {
        value
    }
}

async fn run(
    database: Arc<Database>,
    manager: GameLaunchManager,
    artwork: GameArtworkStore,
    assets: AssetCacheState,
    mut refresh: watch::Receiver<()>,
    mut lifecycle: watch::Receiver<()>,
) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut client: Option<Connection> = None;
    let mut last_activity = None;
    let mut last_sent = Instant::now();
    let mut retry_at = Instant::now();
    let mut last_error = None;
    let mut enabled = true;
    loop {
        tokio::select! {
            _ = interval.tick(), if enabled => {},
            changed = lifecycle.changed(), if enabled => {
                if changed.is_err() { break; }
            },
            changed = refresh.changed() => {
                if changed.is_err() { break; }
                retry_at = Instant::now();
            },
        }
        let database = Arc::clone(&database);
        let manager = manager.clone();
        let artwork = artwork.clone();
        let result =
            tauri::async_runtime::spawn_blocking(move || snapshot(&database, &manager, &artwork))
                .await;
        let Snapshot {
            settings,
            mut activity,
            icon_app_id,
            warning,
        } = match result {
            Ok(Ok(snapshot)) => snapshot,
            Ok(Err(error)) => {
                report_error(&mut last_error, error);
                client = None;
                last_activity = None;
                continue;
            }
            Err(error) => {
                report_error(&mut last_error, error.to_string());
                client = None;
                last_activity = None;
                continue;
            }
        };
        if let Some(warning) = warning {
            report_error(&mut last_error, warning);
        }
        if let Some(app_id) = icon_app_id {
            match assets.cached_icon_url(app_id).await {
                Ok(url) => activity.icon_url = url,
                Err(error) => report_error(&mut last_error, error),
            }
        }
        enabled = settings.enabled;
        if !settings.enabled
            && let Some(mut connection) = client.take()
        {
            if let Err(error) = connection.set_activity(None).await {
                report_error(&mut last_error, error.to_string());
            }
            last_activity = None;
        }
        if !settings.enabled || Instant::now() < retry_at {
            continue;
        }
        if client.is_none() {
            match Connection::connect(APPLICATION_ID).await {
                Ok(connection) => {
                    client = Some(connection);
                    last_activity = None;
                }
                Err(error) => {
                    report_error(&mut last_error, error.to_string());
                    retry_at = Instant::now() + RETRY_INTERVAL;
                    continue;
                }
            }
        }
        if last_activity.as_ref() == Some(&activity) && last_sent.elapsed() < HEARTBEAT_INTERVAL {
            continue;
        }
        if let Some(connection) = client.as_mut() {
            match connection.set_activity(Some(activity.payload())).await {
                Ok(()) => {
                    last_activity = Some(activity);
                    last_sent = Instant::now();
                    last_error = None;
                }
                Err(error) => {
                    report_error(&mut last_error, error.to_string());
                    client = None;
                    last_activity = None;
                    retry_at = Instant::now() + RETRY_INTERVAL;
                }
            }
        }
    }
}

fn report_error(previous: &mut Option<String>, error: String) {
    if previous.as_ref() != Some(&error) {
        crate::application_log::failure("discord_presence", &error);
        *previous = Some(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::RunningGamePresence;

    #[test]
    fn defaults_to_enabled_and_ignores_legacy_application_ids() {
        let defaults: PresenceSettings = serde_json::from_str("{}").unwrap();
        assert!(defaults.enabled);
        let disabled: PresenceSettings = serde_json::from_str(r#"{"enabled":false}"#).unwrap();
        assert!(!disabled.enabled);
        for enabled in [true, false] {
            for id in ["123456789012345678", "invalid", ""] {
                let legacy: PresenceSettings = serde_json::from_value(json!({
                    "enabled": enabled,
                    "applicationId": id,
                }))
                .unwrap();
                assert_eq!(legacy.enabled, enabled);
                assert_eq!(
                    serde_json::to_value(legacy).unwrap(),
                    json!({ "enabled": enabled })
                );
            }
        }
    }

    fn presence(game_id: &str, name: &str, started_at: Option<i64>) -> RunningGamePresence {
        RunningGamePresence {
            game_id: game_id.into(),
            name: name.into(),
            started_at,
            steam_app_id: None,
        }
    }

    #[test]
    fn presence_reads_only_running_games_and_keeps_the_latest_session_time() {
        let directory =
            std::env::temp_dir().join(format!("legio-presence-{}", uuid::Uuid::new_v4()));
        let database = Database::open(&directory).unwrap();
        let older = database
            .create_game(crate::database::CreateGameInput {
                name: "Older".into(),
                steam_app_id: None,
            })
            .unwrap();
        let newer = database
            .create_game(crate::database::CreateGameInput {
                name: "Newer".into(),
                steam_app_id: None,
            })
            .unwrap();
        database.start_game_session(&older.id, 10).unwrap();
        database.start_game_session(&newer.id, 20).unwrap();
        assert_eq!(
            database
                .running_game_presence(&[&older.id, &newer.id])
                .unwrap(),
            Some(presence(&newer.id, "Newer", Some(20)))
        );
        assert_eq!(
            database.running_game_presence(&[&older.id]).unwrap(),
            Some(presence(&older.id, "Older", Some(10)))
        );
        database.end_game_session(&older.id, 30).unwrap();
        assert_eq!(
            database.running_game_presence(&[&older.id]).unwrap(),
            Some(presence(&older.id, "Older", None))
        );
        assert!(database.running_game_presence(&[]).unwrap().is_none());
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn activity_limits_utf8_and_preserves_session_time() {
        let name = limit_text(&"🦈".repeat(100));
        assert_eq!(name.len(), 128);
        let prefix = "a".repeat(127);
        assert_eq!(limit_text(&format!("{prefix}é")), prefix);
        let activity = Activity {
            name: Some(name.clone()),
            started_at: Some(1234),
            icon_url: None,
        }
        .payload();
        assert_eq!(activity["details"], name);
        assert_eq!(activity["timestamps"]["start"], 1234);
        assert_eq!(limit_text("\n\t"), "Unknown game");
        assert!(
            Activity {
                name: None,
                started_at: None,
                icon_url: None,
            }
            .payload()
            .get("timestamps")
            .is_none()
        );
    }

    #[test]
    fn payload_shows_the_game_icon_only_while_playing() {
        let icon = "https://shared.fastly.steamstatic.com/community_assets/images/apps/400/a.jpg";
        let playing = Activity {
            name: Some("Game".into()),
            started_at: None,
            icon_url: Some(icon.into()),
        }
        .payload();
        assert_eq!(playing["assets"]["large_image"], icon);
        assert_eq!(playing["assets"]["small_image"], LEGIO_ICON_URL);
        let without_icon = Activity {
            name: Some("Game".into()),
            started_at: None,
            icon_url: None,
        }
        .payload();
        assert!(without_icon.get("assets").is_none());
        let idle = Activity {
            name: None,
            started_at: None,
            icon_url: Some(icon.into()),
        }
        .payload();
        assert!(idle.get("assets").is_none());
    }

    #[test]
    fn payload_rejects_icons_that_discord_cannot_load() {
        let too_long = format!("https://example.com/{}", "a".repeat(MAX_IMAGE_URL_LENGTH));
        for url in [
            "http://example.com/a.jpg",
            "asset://localhost/a.jpg",
            "file:///C:/a.jpg",
            "C:\\a.jpg",
            "https://",
            too_long.as_str(),
        ] {
            let payload = Activity {
                name: Some("Game".into()),
                started_at: None,
                icon_url: Some(url.into()),
            }
            .payload();
            assert!(payload.get("assets").is_none(), "{url}");
        }
    }
}
