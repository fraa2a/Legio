use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PublicationMode {
    Display,
    NativeOnly,
}
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PublicationOutcome {
    Displayed,
    Abandoned,
    Rejected,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationResult {
    pub cache_warning: Option<String>,
}
#[derive(Clone, Default)]
pub struct ArtworkPublication {
    pending: Arc<Mutex<HashMap<String, Pending>>>,
}
struct Pending {
    stage: PathBuf,
    destination: PathBuf,
    created: Instant,
    bytes: u64,
}
impl ArtworkPublication {
    pub fn register(&self, stage: PathBuf, destination: PathBuf) -> Result<String, String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "Artwork publication unavailable")?;
        let expired = pending
            .iter()
            .filter(|(_, item)| item.created.elapsed() >= Duration::from_secs(300))
            .map(|(token, _)| token.clone())
            .collect::<Vec<_>>();
        for token in expired {
            if let Some(item) = pending.remove(&token) {
                remove_stage(&item.stage);
            }
        }
        let bytes = fs::metadata(&stage)
            .map_err(|error| format!("Could not inspect staged artwork: {error}"))?
            .len();
        if pending.len() >= 128
            || bytes + pending.values().map(|item| item.bytes).sum::<u64>() > 32 * 1024 * 1024
        {
            return Err("Artwork publication capacity reached".into());
        }
        let token = uuid::Uuid::new_v4().to_string();
        pending.insert(
            token.clone(),
            Pending {
                stage,
                destination,
                created: Instant::now(),
                bytes,
            },
        );
        Ok(token)
    }
    pub fn finish(
        &self,
        token: &str,
        outcome: PublicationOutcome,
    ) -> Result<PublicationResult, String> {
        uuid::Uuid::parse_str(token).map_err(|_| "Invalid artwork publication token")?;
        let item = self
            .pending
            .lock()
            .map_err(|_| "Artwork publication unavailable")?
            .remove(token)
            .ok_or("Artwork publication token is no longer registered")?;
        let cache_warning = if outcome == PublicationOutcome::Rejected {
            None
        } else {
            fs::rename(&item.stage, &item.destination)
                .err()
                .map(|error| format!("Artwork is ready, but could not be saved locally: {error}"))
        };
        if outcome == PublicationOutcome::Rejected || cache_warning.is_some() {
            remove_stage(&item.stage);
        }
        Ok(PublicationResult { cache_warning })
    }
    pub fn protected_destinations(&self) -> Result<HashSet<PathBuf>, String> {
        Ok(self
            .pending
            .lock()
            .map_err(|_| "Artwork publication unavailable")?
            .values()
            .map(|item| item.destination.clone())
            .collect())
    }
    pub fn cleanup_orphans(&self, directory: &Path) -> Result<(), String> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| "Artwork publication unavailable")?;
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("Could not inspect artwork stages: {error}")),
        };
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "stage")
                && !pending.values().any(|item| item.stage == path)
            {
                remove_stage(&path);
            }
        }
        Ok(())
    }
}
fn remove_stage(path: &Path) {
    if let Err(error) = fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        crate::application_log::failure("remove_staged_artwork", error);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn paths() -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("legio-publication-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let old = root.join("400-hero.asset");
        let stage = root.join("new.stage");
        std::fs::write(&old, b"old").unwrap();
        std::fs::write(&stage, b"new").unwrap();
        (root, old, stage)
    }
    #[test]
    fn rejected_replacement_preserves_disk_generation() {
        let (root, old, stage) = paths();
        let p = ArtworkPublication::default();
        let token = p.register(stage.clone(), old.clone()).unwrap();
        assert_eq!(std::fs::read(&old).unwrap(), b"old");
        p.finish(&token, PublicationOutcome::Rejected).unwrap();
        assert_eq!(std::fs::read(&old).unwrap(), b"old");
        assert!(!stage.exists());
        assert!(p.finish(&token, PublicationOutcome::Displayed).is_err());
        assert!(
            p.finish("../400-hero.asset", PublicationOutcome::Displayed)
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn abandoned_valid_replacement_finishes_as_prefetch() {
        let (root, old, stage) = paths();
        let p = ArtworkPublication::default();
        let token = p.register(stage.clone(), old.clone()).unwrap();
        p.finish(&token, PublicationOutcome::Abandoned).unwrap();
        assert_eq!(std::fs::read(&old).unwrap(), b"new");
        assert!(!stage.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn staged_replacement_survives_write_failure() {
        let (root, old, stage) = paths();
        let p = ArtworkPublication::default();
        let token = p.register(stage.clone(), old.clone()).unwrap();
        std::fs::remove_file(&stage).unwrap();
        assert!(
            p.finish(&token, PublicationOutcome::Displayed)
                .unwrap()
                .cache_warning
                .is_some()
        );
        assert_eq!(std::fs::read(&old).unwrap(), b"old");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn restart_removes_orphans_but_preserves_registered_stages() {
        let (root, old, stage) = paths();
        let p = ArtworkPublication::default();
        let token = p.register(stage.clone(), old.clone()).unwrap();
        p.cleanup_orphans(&root).unwrap();
        assert!(stage.exists());
        assert!(p.protected_destinations().unwrap().contains(&old));
        p.finish(&token, PublicationOutcome::Rejected).unwrap();
        std::fs::write(&stage, b"orphan").unwrap();
        p.cleanup_orphans(&root).unwrap();
        assert!(!stage.exists());
        assert!(old.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
