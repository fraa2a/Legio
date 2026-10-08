use std::{io, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod ipc;
use ipc::Connection;
use tauri::Manager;
use tokio::{sync::watch, time::Instant};

use crate::{
    database::{Database, DatabaseState},
    game_lifecycle::{GameLaunchManager, GameStatus},
};

const APPLICATION_ID: &str = "1557120430475575366";

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
        let lifecycle = manager.subscribe_changes();
        let (refresh, receiver) = watch::channel(());
        let worker = tauri::async_runtime::spawn(run(database, manager, receiver, lifecycle));
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
        activity
    }
}

fn snapshot(
    database: &Database,
    manager: &GameLaunchManager,
) -> Result<(PresenceSettings, Activity), String> {
    let settings = database.settings()?.discord_presence;
    let mut activity = Activity {
        name: None,
        started_at: None,
    };
    if !settings.enabled {
        return Ok((settings, activity));
    }
    let states = manager.list()?;
    let running: Vec<_> = states
        .iter()
        .filter(|state| state.status == GameStatus::Running)
        .map(|state| state.game_id.as_str())
        .collect();
    if let Some((name, started_at)) = database.running_game_presence(&running)? {
        activity.name = Some(limit_text(&name));
        activity.started_at = started_at.map(|milliseconds| milliseconds / 1000);
    }
    Ok((settings, activity))
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
        let result =
            tauri::async_runtime::spawn_blocking(move || snapshot(&database, &manager)).await;
        let (settings, activity) = match result {
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
        eprintln!("Discord Rich Presence unavailable: {error}");
        *previous = Some(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            Some(("Newer".into(), Some(20)))
        );
        assert_eq!(
            database.running_game_presence(&[&older.id]).unwrap(),
            Some(("Older".into(), Some(10)))
        );
        database.end_game_session(&older.id, 30).unwrap();
        assert_eq!(
            database.running_game_presence(&[&older.id]).unwrap(),
            Some(("Older".into(), None))
        );
        assert!(database.running_game_presence(&[]).unwrap().is_none());
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn activity_limits_utf8_and_preserves_session_time() {
        let name = limit_text(&"🦈".repeat(100));
        assert_eq!(name.len(), 128);
        let activity = Activity {
            name: Some(name.clone()),
            started_at: Some(1234),
        }
        .payload();
        assert_eq!(activity["details"], name);
        assert_eq!(activity["timestamps"]["start"], 1234);
        assert_eq!(limit_text("\n\t"), "Unknown game");
        assert!(
            Activity {
                name: None,
                started_at: None
            }
            .payload()
            .get("timestamps")
            .is_none()
        );
    }
}
