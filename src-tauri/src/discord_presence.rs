use std::{io, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::Manager;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::watch,
    time::{Instant, timeout},
};

use crate::{
    database::{Database, DatabaseState},
    game_lifecycle::{GameLaunchManager, GameStatus},
};

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_INTERVAL: Duration = Duration::from_secs(15);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const MAX_FRAME_BYTES: usize = 64 * 1024;

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
    let selected = summaries
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
        });
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

#[cfg(target_os = "linux")]
type Stream = tokio::net::UnixStream;
#[cfg(windows)]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;

struct Connection {
    stream: Stream,
}

impl Connection {
    async fn connect(application_id: &str) -> io::Result<Self> {
        timeout(IO_TIMEOUT, async {
            let mut connection = Self {
                stream: connect_stream().await?,
            };
            handshake(&mut connection.stream, application_id).await?;
            Ok(connection)
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Discord handshake timed out"))?
    }

    async fn set_activity(&mut self, activity: Option<Value>) -> io::Result<()> {
        timeout(IO_TIMEOUT, exchange_activity(&mut self.stream, activity))
            .await
            .map_err(|_| {
                io::Error::new(io::ErrorKind::TimedOut, "Discord activity update timed out")
            })?
    }
}

async fn handshake<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    application_id: &str,
) -> io::Result<()> {
    write_frame(stream, 0, &json!({ "v": 1, "client_id": application_id })).await?;
    let (opcode, response) = read_frame(stream).await?;
    if opcode != 1 || response["evt"] != "READY" {
        return Err(io::Error::other("Discord rejected the IPC handshake"));
    }
    Ok(())
}

async fn exchange_activity<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    activity: Option<Value>,
) -> io::Result<()> {
    let nonce = uuid::Uuid::new_v4().to_string();
    write_frame(stream, 1, &json!({ "cmd": "SET_ACTIVITY", "args": { "pid": std::process::id(), "activity": activity }, "nonce": nonce })).await?;
    loop {
        let (opcode, response) = read_frame(stream).await?;
        match opcode {
            2 => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Discord closed the IPC connection",
                ));
            }
            3 => write_frame(stream, 4, &response).await?,
            1 if response["nonce"] == nonce => {
                if response["evt"] == "ERROR" || response["cmd"] != "SET_ACTIVITY" {
                    return Err(io::Error::other("Discord rejected the activity update"));
                }
                return Ok(());
            }
            1 | 4 => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid Discord IPC opcode",
                ));
            }
        }
    }
}

async fn write_frame<S: AsyncWrite + Unpin>(
    stream: &mut S,
    opcode: u32,
    payload: &Value,
) -> io::Result<()> {
    let bytes = serde_json::to_vec(payload)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Discord IPC frame is too large",
        ));
    }
    stream.write_all(&opcode.to_le_bytes()).await?;
    stream
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .await?;
    stream.write_all(&bytes).await?;
    stream.flush().await
}

async fn read_frame<S: AsyncRead + Unpin>(stream: &mut S) -> io::Result<(u32, Value)> {
    let opcode = stream.read_u32_le().await?;
    let length = stream.read_u32_le().await? as usize;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Discord IPC frame is too large",
        ));
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    Ok((opcode, serde_json::from_slice(&bytes)?))
}

#[cfg(target_os = "linux")]
async fn connect_stream() -> io::Result<Stream> {
    let roots = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .chain(std::iter::once(std::path::PathBuf::from("/tmp")));
    for root in roots {
        if !root.is_absolute() {
            continue;
        }
        for subdirectory in [
            "",
            "app/com.discordapp.Discord",
            "app/dev.vencord.Vesktop",
            ".flatpak/com.discordapp.Discord/xdg-run",
            ".flatpak/dev.vencord.Vesktop/xdg-run",
            "snap.discord",
            "snap.discord-canary",
        ] {
            for index in 0..10 {
                let path = root.join(subdirectory).join(format!("discord-ipc-{index}"));
                if let Ok(stream) = Stream::connect(path).await {
                    return Ok(stream);
                }
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Discord desktop IPC is not available",
    ))
}

#[cfg(windows)]
async fn connect_stream() -> io::Result<Stream> {
    for index in 0..10 {
        if let Ok(stream) = tokio::net::windows::named_pipe::ClientOptions::new()
            .open(format!(r"\\.\pipe\discord-ipc-{index}"))
        {
            return Ok(stream);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Discord desktop IPC is not available",
    ))
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

    #[tokio::test]
    async fn handshake_requires_ready_and_activity_detects_disconnects() {
        for ready in [true, false] {
            let (mut client, mut server) = tokio::io::duplex(4096);
            let peer = tokio::spawn(async move {
                let (opcode, request) = read_frame(&mut server).await.unwrap();
                assert_eq!(opcode, 0);
                assert_eq!(request["client_id"], "123456789012345678");
                write_frame(
                    &mut server,
                    1,
                    &json!({"evt": if ready { "READY" } else { "ERROR" }}),
                )
                .await
                .unwrap();
            });
            assert_eq!(
                handshake(&mut client, "123456789012345678").await.is_ok(),
                ready
            );
            peer.await.unwrap();
        }
        let (mut client, server) = tokio::io::duplex(4096);
        drop(server);
        assert!(exchange_activity(&mut client, None).await.is_err());
    }

    #[tokio::test]
    async fn acknowledges_updates_handles_ping_and_clears_presence() {
        let (mut client, mut server) = tokio::io::duplex(4096);
        let peer = tokio::spawn(async move {
            for expected in [Some(json!({"details": "A game"})), None] {
                let (opcode, request) = read_frame(&mut server).await.unwrap();
                assert_eq!(opcode, 1);
                assert_eq!(request["args"]["activity"], json!(expected));
                write_frame(&mut server, 3, &json!({"ping": true}))
                    .await
                    .unwrap();
                assert_eq!(read_frame(&mut server).await.unwrap().0, 4);
                write_frame(
                    &mut server,
                    1,
                    &json!({"cmd": "SET_ACTIVITY", "nonce": request["nonce"]}),
                )
                .await
                .unwrap();
            }
        });
        exchange_activity(&mut client, Some(json!({"details": "A game"})))
            .await
            .unwrap();
        exchange_activity(&mut client, None).await.unwrap();
        peer.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_oversized_frames_before_allocating_and_rejected_activities() {
        let (mut client, mut server) = tokio::io::duplex(4096);
        server.write_u32_le(1).await.unwrap();
        server
            .write_u32_le((MAX_FRAME_BYTES + 1) as u32)
            .await
            .unwrap();
        assert_eq!(
            read_frame(&mut client).await.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        let peer = tokio::spawn(async move {
            let (_, request) = read_frame(&mut server).await.unwrap();
            write_frame(
                &mut server,
                1,
                &json!({"cmd": "SET_ACTIVITY", "evt": "ERROR", "nonce": request["nonce"]}),
            )
            .await
            .unwrap();
        });
        assert!(exchange_activity(&mut client, None).await.is_err());
        peer.await.unwrap();
    }
}
