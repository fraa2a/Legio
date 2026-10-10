use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::watch;

#[derive(Clone)]
enum Status {
    Pending,
    Ready,
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

    fn finish(&self, result: Result<(), String>) {
        self.status.send_replace(match result {
            Ok(()) => Status::Ready,
            Err(error) => Status::Failed(format!("Startup recovery failed: {error}")),
        });
    }

    fn require_ready(&self) -> Result<(), String> {
        match &*self.status.borrow() {
            Status::Ready => Ok(()),
            Status::Pending => Err("Startup recovery is still in progress".to_owned()),
            Status::Failed(error) => Err(error.clone()),
        }
    }

    async fn wait(&self) -> Result<(), String> {
        let mut status = self.status.subscribe();
        loop {
            let current = status.borrow_and_update().clone();
            match current {
                Status::Ready => return Ok(()),
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
    match app.try_state::<StartupRecovery>() {
        Some(state) => state.wait().await,
        #[cfg(test)]
        None => Ok(()),
        #[cfg(not(test))]
        None => Err("Startup recovery state is unavailable".to_owned()),
    }
}

pub(crate) fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let worker = app.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            crate::game_transfer::recover(
                worker
                    .state::<crate::database::DatabaseState>()
                    .database()?,
            )
        })
        .await
        .map_err(|error| format!("Recovery task failed: {error}"))
        .and_then(|result| result)
        .and_then(|()| crate::download_queue::start(app.clone()));
        if let Err(error) = &result {
            crate::application_log::failure("startup_recovery", error);
        }
        app.state::<StartupRecovery>().finish(result);
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
            recovery.finish(Ok(()));
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
