use std::{io, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod ipc;
use ipc::Connection;
use tauri::Manager;
use tokio::{sync::watch, time::Instant};

use crate::{
    database::{Database, DatabaseState, PlaytimeSummary},
    game_lifecycle::{GameLaunchManager, GameLaunchState, GameStatus},
};

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const RETRY_INTERVAL: Duration = Duration::from_secs(15);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PresenceSettings {
    pub enabled: bool,
    pub application_id: String,
}

impl PresenceSettings {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.application_id.is_empty() && !self.enabled {
            return Ok(());
        }
        if !(17..=20).contains(&self.application_id.len())
            || !self
                .application_id
                .bytes()
                .all(|byte| byte.is_ascii_digit())
            || !self.application_id.parse::<u64>().is_ok_and(|id| id > 0)
        {
            return Err("Discord Application ID must be a valid numeric application ID".to_owned());
        }
        Ok(())
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
        let (refresh, receiver) = watch::channel(());
        let worker = tauri::async_runtime::spawn(run(database, manager, receiver));
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
    settings.validate()?;
    let mut activity = Activity {
        name: None,
        started_at: None,
    };
    if !settings.enabled {
        return Ok((settings, activity));
    }
    let states = manager.list()?;
    let summaries = database.playtime_summaries(crate::database::now_milliseconds()?)?;
    let selected = select_running_game(&summaries, &states);
    if let Some(summary) = selected {
        let game = database.game(&summary.game_id)?;
        activity.name = Some(limit_text(&game.name));
        activity.started_at = if summary.active_sessions > 0 {
            summary
                .last_played_at
                .map(|milliseconds| milliseconds / 1000)
        } else {
            None
        };
    }
    Ok((settings, activity))
}

fn select_running_game<'a>(
    summaries: &'a [PlaytimeSummary],
    states: &[GameLaunchState],
) -> Option<&'a PlaytimeSummary> {
    summaries
        .iter()
        .filter(|summary| {
            states.iter().any(|state| {
                state.game_id == summary.game_id && state.status == GameStatus::Running
            })
        })
        .max_by(|left, right| {
            left.last_played_at
                .cmp(&right.last_played_at)
                .then_with(|| left.game_id.cmp(&right.game_id))
        })
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
) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut client: Option<(String, Connection)> = None;
    let mut last_activity = None;
    let mut last_sent = Instant::now();
    let mut retry_at = Instant::now();
    let mut last_error = None;
    loop {
        tokio::select! {
            _ = interval.tick() => {},
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
        if client
            .as_ref()
            .is_some_and(|(id, _)| !settings.enabled || *id != settings.application_id)
        {
            if let Some((_, mut connection)) = client.take()
                && let Err(error) = connection.set_activity(None).await
            {
                report_error(&mut last_error, error.to_string());
            }
            last_activity = None;
        }
        if !settings.enabled || Instant::now() < retry_at {
            continue;
        }
        if client.is_none() {
            match Connection::connect(&settings.application_id).await {
                Ok(connection) => {
                    client = Some((settings.application_id, connection));
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
        if let Some((_, connection)) = client.as_mut() {
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
    fn validates_ids_and_defaults_old_settings_to_disabled() {
        let defaults: PresenceSettings = serde_json::from_str("{}").unwrap();
        assert!(!defaults.enabled);
        assert!(defaults.validate().is_ok());
        for id in [
            "",
            "abc",
            "123",
            "18446744073709551616",
            " 123456789012345678",
        ] {
            assert!(
                PresenceSettings {
                    enabled: true,
                    application_id: id.into()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            PresenceSettings {
                enabled: true,
                application_id: "123456789012345678".into()
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn selects_latest_running_game_and_ignores_launching_or_stopped_games() {
        let summaries: Vec<_> = [("older", 10), ("newer", 20), ("launching", 30)]
            .into_iter()
            .map(|(id, start)| PlaytimeSummary {
                game_id: id.to_owned(),
                total_milliseconds: 0,
                active_sessions: 1,
                last_played_at: Some(start),
            })
            .collect();
        let mut states: Vec<_> = [
            ("older", GameStatus::Running),
            ("newer", GameStatus::Running),
            ("launching", GameStatus::Launching),
        ]
        .into_iter()
        .map(|(id, status)| GameLaunchState {
            game_id: id.to_owned(),
            status,
            error: None,
            compatibility_options: None,
            compatibility_log_path: None,
            compatibility_log_error: None,
            compatibility_log_truncated: false,
            runner_exit_code: None,
        })
        .collect();
        assert_eq!(
            select_running_game(&summaries, &states).unwrap().game_id,
            "newer"
        );
        states[1].status = GameStatus::Idle;
        assert_eq!(
            select_running_game(&summaries, &states).unwrap().game_id,
            "older"
        );
        states[0].status = GameStatus::Idle;
        assert!(select_running_game(&summaries, &states).is_none());
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
