use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::watch;

#[derive(Clone)]
enum Status {
    Pending,
    Ready(Vec<String>),
    Failed(String),
}

pub(crate) struct StartupRecovery {
    status: watch::Sender<Status>,
}

impl StartupRecovery {
    pub(crate) fn new() -> Self {
        Self {
            status: watch::channel(Status::Pending).0,
        }
    }

    fn finish(&self, result: Result<Vec<String>, String>) {
        self.status.send_replace(match result {
            Ok(warnings) => Status::Ready(warnings),
            Err(error) => Status::Failed(format!("Startup recovery failed: {error}")),
        });
    }

    fn require_ready(&self) -> Result<(), String> {
        match &*self.status.borrow() {
            Status::Ready(_) => Ok(()),
            Status::Pending => Err("Startup recovery is still in progress".to_owned()),
            Status::Failed(error) => Err(error.clone()),
        }
    }

    async fn wait(&self) -> Result<Vec<String>, String> {
        let mut status = self.status.subscribe();
        loop {
            let current = status.borrow_and_update().clone();
            match current {
                Status::Ready(warnings) => return Ok(warnings),
                Status::Failed(error) => return Err(error),
                Status::Pending => {}
            }
            status
                .changed()
                .await
                .map_err(|_| "Startup recovery notifications stopped".to_owned())?;
        }
    }
}

pub(crate) fn require_ready<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    match app.try_state::<StartupRecovery>() {
        Some(state) => state.require_ready(),
        #[cfg(test)]
        None => Ok(()),
        #[cfg(not(test))]
        None => Err("Startup recovery state is unavailable".to_owned()),
    }
}

pub(crate) async fn wait<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    warnings(app).await.map(|_| ())
}

/// Waits for recovery and returns the warnings of the steps that failed.
pub(crate) async fn warnings<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<String>, String> {
    match app.try_state::<StartupRecovery>() {
        Some(state) => state.wait().await,
        #[cfg(test)]
        None => Ok(Vec::new()),
        #[cfg(not(test))]
        None => Err("Startup recovery state is unavailable".to_owned()),
    }
}

fn record(warnings: &mut Vec<String>, step: &str, result: Result<(), String>) {
    if let Err(error) = result {
        warnings.push(format!("{step}: {error}"));
    }
}

// Only an unavailable database is fatal; every other step runs and reports its failure.
fn recover(app: &AppHandle) -> Result<Vec<String>, String> {
    let state = app.state::<crate::database::DatabaseState>();
    let database = state.database()?;
    let mut warnings = Vec::new();
    record(
        &mut warnings,
        "Game transfer recovery",
        crate::game_transfer::recover(database),
    );
    record(
        &mut warnings,
        "Game folder adoption",
        crate::game_library::adopt_existing(database),
    );
    record(
        &mut warnings,
        "Game library loading",
        app.path()
            .app_data_dir()
            .map_err(|error| error.to_string())
            .and_then(|data| database.storage_root(&data))
            .map(|storage| crate::game_library::initialize(database, &storage)),
    );
    record(
        &mut warnings,
        "Download recovery",
        crate::download_queue::start(app.clone()),
    );
    Ok(warnings)
}

pub(crate) fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let worker = app.clone();
        let result = tauri::async_runtime::spawn_blocking(move || recover(&worker))
            .await
            .map_err(|error| format!("Recovery task failed: {error}"))
            .and_then(|result| result);
        match &result {
            Ok(warnings) => {
                for warning in warnings {
                    crate::application_log::failure("startup_recovery", warning);
                }
            }
            Err(error) => crate::application_log::failure("startup_recovery", error),
        }
        let ready = result.is_ok();
        app.state::<StartupRecovery>().finish(result);
        if ready {
            crate::game_updates::start(app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn pending_recovery_blocks_operations_and_releases_all_waiters_when_ready() {
        let recovery = StartupRecovery::new();
        assert!(
            recovery
                .require_ready()
                .unwrap_err()
                .contains("in progress")
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(20), recovery.wait())
                .await
                .is_err()
        );
        let complete = async {
            tokio::task::yield_now().await;
            recovery.finish(Ok(Vec::new()));
        };
        let (first, second, ()) = tokio::time::timeout(Duration::from_secs(1), async {
            tokio::join!(recovery.wait(), recovery.wait(), complete)
        })
        .await
        .unwrap();
        first.unwrap();
        second.unwrap();
        recovery.require_ready().unwrap();
        recovery.wait().await.unwrap();
    }

    #[tokio::test]
    async fn failed_steps_become_warnings_and_operations_stay_available() {
        let recovery = StartupRecovery::new();
        let mut warnings = Vec::new();
        record(&mut warnings, "Transfer", Err("drive offline".to_owned()));
        record(&mut warnings, "Adoption", Ok(()));
        record(&mut warnings, "Downloads", Err("stage locked".to_owned()));
        recovery.finish(Ok(warnings));
        recovery.require_ready().unwrap();
        assert_eq!(
            recovery.wait().await.unwrap(),
            ["Transfer: drive offline", "Downloads: stage locked"]
        );
    }

    #[tokio::test]
    async fn recovery_failure_is_retained_for_operations_and_late_waiters() {
        let recovery = StartupRecovery::new();
        recovery.finish(Err("marker conflict".to_owned()));
        assert_eq!(
            recovery.wait().await.unwrap_err(),
            "Startup recovery failed: marker conflict"
        );
        assert_eq!(
            recovery.require_ready().unwrap_err(),
            "Startup recovery failed: marker conflict"
        );
    }
}
