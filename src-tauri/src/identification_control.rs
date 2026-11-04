use std::{collections::HashMap, sync::Mutex};

use tokio::sync::watch;

/// Stop switches of the game identifications in progress, keyed by operation id.
#[derive(Default)]
pub(crate) struct IdentificationControl {
    active: Mutex<HashMap<String, watch::Sender<bool>>>,
}

pub(crate) struct IdentificationSession<'a> {
    control: &'a IdentificationControl,
    key: String,
    stop: watch::Receiver<bool>,
}

impl IdentificationControl {
    pub(crate) fn begin(&self, operation_id: &str) -> Result<IdentificationSession<'_>, String> {
        let (sender, stop) = watch::channel(false);
        self.lock()?.insert(operation_id.to_owned(), sender);
        Ok(IdentificationSession {
            control: self,
            key: operation_id.to_owned(),
            stop,
        })
    }

    /// Stops every identification in progress. Their matches found so far are kept.
    pub(crate) fn stop_all(&self) -> Result<(), String> {
        for sender in self.lock()?.values() {
            sender.send_replace(true);
        }
        Ok(())
    }

    fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<String, watch::Sender<bool>>>, String> {
        self.active
            .lock()
            .map_err(|_| "Identification lock was poisoned".to_owned())
    }
}

impl IdentificationSession<'_> {
    /// Resolves once the identification is stopped.
    pub(crate) async fn stopped(&mut self) {
        // A closed channel cannot happen while the session is registered; wait forever then.
        if self.stop.wait_for(|stopped| *stopped).await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

impl Drop for IdentificationSession<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.control.lock() {
            active.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stop_reaches_running_sessions_only() {
        let control = IdentificationControl::default();
        let mut first = control.begin("a").unwrap();
        let mut second = control.begin("b").unwrap();
        control.stop_all().unwrap();
        first.stopped().await;
        second.stopped().await;
        drop(first);
        drop(second);
        let mut later = control.begin("c").unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), later.stopped())
                .await
                .is_err()
        );
        assert_eq!(control.lock().unwrap().len(), 1);
    }
}
