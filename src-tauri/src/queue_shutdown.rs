use std::{
    process::Command,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::{database::DatabaseState, download_queue::DownloadQueueState};

const COUNTDOWN: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_secs(2);
#[cfg(target_os = "linux")]
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Off,
    Waiting,
    Countdown { ends_at: Instant },
}

#[derive(Debug, PartialEq, Eq)]
enum Step {
    Idle,
    Started,
    Interrupted,
    Fire,
}

struct Schedule {
    phase: Phase,
    error: Option<String>,
}

impl Schedule {
    fn arm(&mut self, busy: bool) -> Result<(), String> {
        if !busy {
            return Err("There are no downloads waiting to complete".to_owned());
        }
        self.phase = Phase::Waiting;
        self.error = None;
        Ok(())
    }

    fn tick(&mut self, busy: bool, now: Instant) -> Step {
        match (self.phase, busy) {
            (Phase::Off, _) => Step::Idle,
            (Phase::Waiting, true) => Step::Idle,
            (Phase::Waiting, false) => {
                self.phase = Phase::Countdown {
                    ends_at: now + COUNTDOWN,
                };
                Step::Started
            }
            (Phase::Countdown { .. }, true) => {
                self.phase = Phase::Waiting;
                Step::Interrupted
            }
            (Phase::Countdown { ends_at }, false) if now >= ends_at => {
                self.phase = Phase::Off;
                Step::Fire
            }
            (Phase::Countdown { .. }, false) => Step::Idle,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QueueShutdownStatus {
    phase: &'static str,
    remaining_seconds: Option<u64>,
    error: Option<String>,
}

pub struct QueueShutdownState(Mutex<Schedule>);

impl Default for QueueShutdownState {
    fn default() -> Self {
        Self(Mutex::new(Schedule {
            phase: Phase::Off,
            error: None,
        }))
    }
}

impl QueueShutdownState {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Schedule>, String> {
        self.0
            .lock()
            .map_err(|_| "Shutdown schedule lock was poisoned".to_owned())
    }

    fn status(&self) -> Result<QueueShutdownStatus, String> {
        let schedule = self.lock()?;
        let now = Instant::now();
        Ok(match schedule.phase {
            Phase::Off => QueueShutdownStatus {
                phase: "off",
                remaining_seconds: None,
                error: schedule.error.clone(),
            },
            Phase::Waiting => QueueShutdownStatus {
                phase: "waiting",
                remaining_seconds: None,
                error: None,
            },
            Phase::Countdown { ends_at } => QueueShutdownStatus {
                phase: "countdown",
                remaining_seconds: Some(
                    ends_at.saturating_duration_since(now).as_secs_f64().ceil() as u64,
                ),
                error: None,
            },
        })
    }
}

/// True while downloads can still progress or install, or a game is starting or running.
fn is_busy(app: &AppHandle) -> Result<bool, String> {
    let database = app.state::<DatabaseState>();
    if crate::download_queue::has_unfinished_work(
        &app.state::<DownloadQueueState>(),
        database.database()?,
    )? {
        return Ok(true);
    }
    Ok(
        match app.try_state::<crate::game_lifecycle::GameLaunchManager>() {
            Some(manager) => manager
                .list()?
                .iter()
                .any(|state| state.status != crate::game_lifecycle::GameStatus::Idle),
            None => false,
        },
    )
}

fn publish(app: &AppHandle) {
    let result = app
        .state::<QueueShutdownState>()
        .status()
        .and_then(|status| {
            app.emit("queue-shutdown-changed", status)
                .map_err(|error| error.to_string())
        });
    if let Err(error) = result {
        crate::application_log::failure("publish_queue_shutdown", error);
    }
}

fn notify_countdown(app: &AppHandle) {
    let language = match app
        .state::<DatabaseState>()
        .database()
        .and_then(|database| database.settings())
    {
        Ok(settings) => settings.language,
        Err(error) => {
            crate::application_log::failure("read_shutdown_notification_language", error);
            return;
        }
    };
    if let Err(error) = app
        .notification()
        .builder()
        .title(language.text("Spegnimento automatico", "Automatic shutdown"))
        .body(language.text(
            "Il computer si spegnerà tra 30 secondi.",
            "The computer will shut down in 30 seconds.",
        ))
        .show()
    {
        crate::application_log::failure("show_shutdown_notification", error);
    }
}

#[cfg(target_os = "linux")]
const POWER_OFF_COMMANDS: &[&[&str]] = &[&["systemctl", "poweroff"], &["shutdown", "-P", "now"]];
#[cfg(target_os = "windows")]
const POWER_OFF_COMMANDS: &[&[&str]] = &[&["shutdown", "/s", "/t", "0"]];
#[cfg(target_os = "macos")]
const POWER_OFF_COMMANDS: &[&[&str]] = &[&[
    "osascript",
    "-e",
    "tell application \"System Events\" to shut down",
]];

#[cfg(target_os = "linux")]
fn run(request: &mut Command) -> Result<std::process::Output, String> {
    crate::process_output::capture(request, COMMAND_TIMEOUT).map_err(str::to_owned)
}

// The Windows and macOS commands return as soon as the shutdown is requested.
#[cfg(not(target_os = "linux"))]
fn run(request: &mut Command) -> Result<std::process::Output, String> {
    request.output().map_err(|error| error.to_string())
}

fn power_off() -> Result<(), String> {
    let mut last = "No shutdown command is available".to_owned();
    for command in POWER_OFF_COMMANDS {
        let Some((program, arguments)) = command.split_first() else {
            continue;
        };
        let mut request = Command::new(program);
        request.args(arguments);
        match run(&mut request) {
            Ok(output) if output.status.success() => return Ok(()),
            Ok(output) => {
                last = format!(
                    "{program} exited with {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
            Err(error) => last = format!("{program} failed: {error}"),
        }
    }
    Err(last)
}

/// One evaluation under the schedule lock, so a concurrent disable or enable is
/// ordered with the decision to power off.
fn step(app: &AppHandle) -> Result<Step, String> {
    let state = app.state::<QueueShutdownState>();
    let mut schedule = state.lock()?;
    if schedule.phase == Phase::Off {
        return Ok(Step::Idle);
    }
    // A failed read keeps the machine on.
    let busy = is_busy(app).unwrap_or_else(|error| {
        crate::application_log::failure("check_queue_shutdown", error);
        true
    });
    Ok(schedule.tick(busy, Instant::now()))
}

async fn evaluate(app: &AppHandle) {
    let worker = app.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || step(&worker)).await;
    match outcome {
        Ok(Ok(Step::Idle)) => {}
        Ok(Ok(Step::Started)) => {
            crate::application_log::state_change("queue_shutdown", "countdown started");
            notify_countdown(app);
            publish(app);
        }
        Ok(Ok(Step::Interrupted)) => {
            crate::application_log::state_change(
                "queue_shutdown",
                "countdown interrupted by new work",
            );
            publish(app);
        }
        Ok(Ok(Step::Fire)) => {
            crate::application_log::state_change("queue_shutdown", "powering off");
            let result = tauri::async_runtime::spawn_blocking(power_off)
                .await
                .unwrap_or_else(|error| Err(format!("Shutdown task failed: {error}")));
            if let Err(error) = result {
                crate::application_log::failure("queue_shutdown_power_off", &error);
                if let Ok(mut schedule) = app.state::<QueueShutdownState>().lock() {
                    schedule.error = Some(error);
                }
            }
            publish(app);
        }
        Ok(Err(error)) => crate::application_log::failure("queue_shutdown", error),
        Err(error) => crate::application_log::failure("queue_shutdown", error),
    }
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            evaluate(&app).await;
        }
    });
}

#[tauri::command]
pub async fn get_queue_shutdown(app: AppHandle) -> Result<QueueShutdownStatus, String> {
    app.state::<QueueShutdownState>().status()
}

#[tauri::command]
pub async fn set_queue_shutdown(
    app: AppHandle,
    enabled: bool,
) -> Result<QueueShutdownStatus, String> {
    let worker = app.clone();
    let status = tauri::async_runtime::spawn_blocking(move || {
        let state = worker.state::<QueueShutdownState>();
        let mut schedule = state.lock()?;
        if enabled {
            schedule.arm(is_busy(&worker)?)?;
        } else {
            schedule.phase = Phase::Off;
            schedule.error = None;
        }
        drop(schedule);
        state.status()
    })
    .await
    .map_err(|error| format!("Shutdown schedule task failed: {error}"))??;
    crate::application_log::state_change(
        "queue_shutdown",
        if enabled { "enabled" } else { "disabled" },
    );
    publish(&app);
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schedule() -> Schedule {
        Schedule {
            phase: Phase::Off,
            error: None,
        }
    }

    #[test]
    fn refuses_to_arm_without_pending_work() {
        let mut schedule = schedule();
        assert!(schedule.arm(false).is_err());
        assert_eq!(schedule.phase, Phase::Off);
    }

    #[test]
    fn disarmed_schedule_never_fires() {
        let mut schedule = schedule();
        let start = Instant::now();
        assert_eq!(schedule.tick(false, start), Step::Idle);
        assert_eq!(schedule.tick(false, start + COUNTDOWN * 2), Step::Idle);
    }

    #[test]
    fn fires_only_after_a_full_idle_countdown() {
        let mut schedule = schedule();
        let start = Instant::now();
        schedule.arm(true).unwrap();
        assert_eq!(schedule.tick(true, start), Step::Idle);
        assert_eq!(schedule.tick(false, start), Step::Started);
        assert_eq!(
            schedule.tick(false, start + COUNTDOWN - Duration::from_secs(1)),
            Step::Idle
        );
        assert_eq!(schedule.tick(false, start + COUNTDOWN), Step::Fire);
        assert_eq!(schedule.phase, Phase::Off);
        assert_eq!(schedule.tick(false, start + COUNTDOWN * 2), Step::Idle);
    }

    #[test]
    fn new_work_restarts_the_wait_with_a_fresh_countdown() {
        let mut schedule = schedule();
        let start = Instant::now();
        schedule.arm(true).unwrap();
        assert_eq!(schedule.tick(false, start), Step::Started);
        assert_eq!(
            schedule.tick(true, start + Duration::from_secs(10)),
            Step::Interrupted
        );
        let resumed = start + Duration::from_secs(20);
        assert_eq!(schedule.tick(false, resumed), Step::Started);
        assert_eq!(schedule.tick(false, start + COUNTDOWN), Step::Idle);
        assert_eq!(schedule.tick(false, resumed + COUNTDOWN), Step::Fire);
    }

    #[test]
    fn rearming_clears_a_previous_error() {
        let mut schedule = schedule();
        schedule.error = Some("failed".to_owned());
        schedule.arm(true).unwrap();
        assert_eq!(schedule.error, None);
    }
}
