//! Disk space a download needs: forecast before it is queued and checked again
//! before extraction. The archive stays in the download directory until the install
//! finishes, extraction stages a full copy beside it and finalization copies that into
//! the library, so the download directory needs archive plus extracted size and the
//! library needs the extracted size. On one volume the two add up.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::{
    archive_install::FREE_SPACE_MARGIN,
    database::{Database, DatabaseState, database_error},
};

/// Extracted bytes per archive byte assumed before an archive can be read. Game
/// archives hold mostly already compressed assets, so twice the archive size covers
/// nearly every release; the archive size itself is the lower bound.
pub(crate) const ESTIMATED_EXPANSION: u64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SpaceVerdict {
    /// The estimate fits with the safety margin to spare.
    Enough,
    /// The lower bound fits, the estimate with its margin may not.
    Tight,
    /// Even the lower bound does not fit.
    Insufficient,
    /// The size or the free space of a volume is unknown.
    Unknown,
}

impl SpaceVerdict {
    const fn severity(self) -> u8 {
        match self {
            Self::Enough => 0,
            Self::Tight => 1,
            Self::Unknown => 2,
            Self::Insufficient => 3,
        }
    }
}

/// Bytes still to be written, as a lower bound and an estimate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Need {
    pub minimum: u64,
    pub estimate: u64,
}

impl Need {
    fn exact(bytes: u64) -> Self {
        Self {
            minimum: bytes,
            estimate: bytes,
        }
    }

    fn extracted(archive_bytes: u64) -> Self {
        Self {
            minimum: archive_bytes,
            estimate: archive_bytes.saturating_mul(ESTIMATED_EXPANSION),
        }
    }

    fn plus(self, other: Self) -> Self {
        Self {
            minimum: self.minimum.saturating_add(other.minimum),
            estimate: self.estimate.saturating_add(other.estimate),
        }
    }
}

/// What a job still writes to the download directory and to its library.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Demand {
    pub downloads: Need,
    pub library: Need,
}

impl Demand {
    /// A job in `status` with `size` archive bytes of which `downloaded` are on disk.
    pub(crate) fn for_job(status: &str, size: u64, downloaded: u64) -> Self {
        let extracted = Need::extracted(size);
        match status {
            "queued" | "downloading" | "waiting" | "paused" => Self {
                downloads: Need::exact(size.saturating_sub(downloaded)).plus(extracted),
                library: extracted,
            },
            "downloaded" | "staging" => Self {
                downloads: extracted,
                library: extracted,
            },
            "staged" | "finalizing" => Self {
                downloads: Need::default(),
                library: extracted,
            },
            _ => Self::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VolumeSpace {
    /// A folder on the volume, for display.
    pub path: String,
    pub free_bytes: Option<u64>,
    /// Estimated bytes that jobs already in the queue will still write here.
    pub reserved_bytes: u64,
    pub minimum_bytes: u64,
    pub estimated_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SpaceForecast {
    pub verdict: SpaceVerdict,
    pub size_bytes: Option<u64>,
    pub volumes: Vec<VolumeSpace>,
}

fn volume_verdict(volume: &VolumeSpace) -> SpaceVerdict {
    let Some(free) = volume.free_bytes else {
        return SpaceVerdict::Unknown;
    };
    let available = free.saturating_sub(volume.reserved_bytes);
    if available < volume.minimum_bytes {
        SpaceVerdict::Insufficient
    } else if available < volume.estimated_bytes.saturating_add(FREE_SPACE_MARGIN) {
        SpaceVerdict::Tight
    } else {
        SpaceVerdict::Enough
    }
}

/// The most severe verdict of all volumes. Insufficient outranks unknown, so one
/// volume that clearly lacks space is reported even when another cannot be read.
pub(crate) fn verdict(volumes: &[VolumeSpace]) -> SpaceVerdict {
    volumes
        .iter()
        .map(volume_verdict)
        .max_by_key(|verdict| verdict.severity())
        .unwrap_or(SpaceVerdict::Unknown)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Volume {
    pub id: Option<String>,
    pub path: PathBuf,
    pub free_bytes: Option<u64>,
}

/// Identity of the filesystem holding `path`. Unknown identities never match, so
/// two folders whose volume cannot be read are treated as separate.
#[cfg(unix)]
fn volume_id(path: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .ok()
        .map(|metadata| metadata.dev().to_string())
}

#[cfg(windows)]
fn volume_id(path: &Path) -> Option<String> {
    dunce::canonicalize(path)
        .ok()?
        .components()
        .next()
        .and_then(|component| match component {
            std::path::Component::Prefix(prefix) => {
                Some(prefix.as_os_str().to_string_lossy().to_uppercase())
            }
            _ => None,
        })
}

pub(crate) fn volume(path: &Path) -> Volume {
    let free_bytes = crate::installed_folder::disk_space(path)
        .inspect_err(|error| crate::application_log::failure("forecast_disk_space", error))
        .ok()
        .map(|(free, _)| free);
    Volume {
        id: volume_id(path),
        path: path.to_path_buf(),
        free_bytes,
    }
}

fn same_volume(left: &Volume, right: &Volume) -> bool {
    left.id.is_some() && left.id == right.id
}

/// A queued job: its demand and the library it installs into.
pub(crate) struct Reservation {
    pub demand: Demand,
    pub library: Volume,
}

/// Lays `demand` out on the download and library volumes and adds what queued jobs
/// still need on each of them.
pub(crate) fn forecast(
    size_bytes: Option<u64>,
    downloads: &Volume,
    library: &Volume,
    reservations: &[Reservation],
) -> SpaceForecast {
    let demand = size_bytes.map(|size| Demand::for_job("queued", size, 0));
    let mut volumes = vec![(downloads, demand.map(|demand| demand.downloads), 0u64)];
    if same_volume(downloads, library) {
        volumes[0].1 = demand.map(|demand| demand.downloads.plus(demand.library));
    } else {
        volumes.push((library, demand.map(|demand| demand.library), 0));
    }
    for reservation in reservations {
        volumes[0].2 = volumes[0]
            .2
            .saturating_add(reservation.demand.downloads.estimate);
        let target = volumes
            .iter_mut()
            .find(|(volume, _, _)| same_volume(volume, &reservation.library));
        if let Some(target) = target {
            target.2 = target.2.saturating_add(reservation.demand.library.estimate);
        }
    }
    let volumes: Vec<VolumeSpace> = volumes
        .into_iter()
        .map(|(volume, need, reserved)| {
            let need = need.unwrap_or_default();
            VolumeSpace {
                path: volume.path.to_string_lossy().into_owned(),
                free_bytes: volume.free_bytes,
                reserved_bytes: reserved,
                minimum_bytes: need.minimum,
                estimated_bytes: need.estimate,
            }
        })
        .collect();
    SpaceForecast {
        verdict: if size_bytes.is_some() {
            verdict(&volumes)
        } else {
            SpaceVerdict::Unknown
        },
        size_bytes,
        volumes,
    }
}

fn gigabytes(bytes: u64) -> String {
    // Binary units, like the frontend formatBytes.
    format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

pub(crate) fn extraction_shortfall(needed: u64, available: u64) -> String {
    format!(
        "Not enough free disk space to extract the archive: it needs {} and {} are free. Free disk space and retry",
        gigabytes(needed),
        gigabytes(available)
    )
}

/// Checked before extraction: `extracted` bytes are staged in the download directory
/// and then copied into the library. Unknown free space is left to the extraction limit.
pub(crate) fn check_install_space(
    extracted: u64,
    downloads: &Volume,
    library: Option<&Volume>,
) -> Result<(), String> {
    let needed = match library {
        Some(library) if same_volume(downloads, library) => {
            vec![(downloads, extracted.saturating_mul(2))]
        }
        Some(library) => vec![(downloads, extracted), (library, extracted)],
        None => vec![(downloads, extracted)],
    };
    for (volume, bytes) in needed {
        if let Some(free) = volume.free_bytes
            && free < bytes
        {
            return Err(format!(
                "Not enough free disk space to install the game in {}: it needs {} and {} are free. Free disk space and retry",
                volume.path.display(),
                gigabytes(bytes),
                gigabytes(free)
            ));
        }
    }
    Ok(())
}

const RESERVING: &str =
    "'queued','downloading','waiting','paused','downloaded','staging','staged','finalizing'";

fn reservations(database: &Database, storage_root: &Path) -> Result<Vec<Reservation>, String> {
    let rows: Vec<(String, i64, i64, Option<String>)> = database.with_connection(|connection| {
        let mut statement = connection
            .prepare(&format!(
                "SELECT status, size_bytes, downloaded_bytes, library_id FROM downloads WHERE status IN ({RESERVING})"
            ))
            .map_err(database_error)?;
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .map_err(database_error)?
            .collect::<Result<_, _>>()
            .map_err(database_error)
    })?;
    let mut volumes: Vec<(Option<String>, Volume)> = Vec::new();
    let mut reserved = Vec::with_capacity(rows.len());
    for (status, size, downloaded, library) in rows {
        let library = match volumes.iter().find(|(id, _)| *id == library) {
            Some((_, volume)) => volume.clone(),
            None => {
                let root = crate::finalize_install::library_root_for(
                    database,
                    storage_root,
                    library.as_deref(),
                )?;
                let volume = volume(&root);
                volumes.push((library, volume.clone()));
                volume
            }
        };
        reserved.push(Reservation {
            demand: Demand::for_job(
                &status,
                u64::try_from(size).unwrap_or(0),
                u64::try_from(downloaded).unwrap_or(0),
            ),
            library,
        });
    }
    Ok(reserved)
}

#[tauri::command]
pub async fn forecast_download_space(
    app: AppHandle,
    size_bytes: Option<u64>,
    library_id: Option<String>,
) -> Result<SpaceForecast, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let database = app.state::<DatabaseState>();
        let database = database.database()?;
        let downloads = app
            .state::<crate::download_queue::DownloadQueueState>()
            .directory()?;
        let storage_root = downloads
            .parent()
            .ok_or("Download directory has no storage root")?
            .to_path_buf();
        let library = crate::finalize_install::library_root_for(
            database,
            &storage_root,
            library_id.as_deref(),
        )?;
        Ok(forecast(
            size_bytes,
            &volume(&downloads),
            &volume(&library),
            &reservations(database, &storage_root)?,
        ))
    })
    .await
    .map_err(|error| format!("Space forecast task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    fn at(id: &str, free: Option<u64>) -> Volume {
        Volume {
            id: Some(id.into()),
            path: PathBuf::from(format!("/{id}")),
            free_bytes: free,
        }
    }

    #[test]
    fn a_job_needs_less_as_it_progresses() {
        let queued = Demand::for_job("queued", 10, 4);
        assert_eq!(
            queued.downloads,
            Need {
                minimum: 16,
                estimate: 26
            }
        );
        assert_eq!(
            queued.library,
            Need {
                minimum: 10,
                estimate: 20
            }
        );
        let downloaded = Demand::for_job("downloaded", 10, 10);
        assert_eq!(
            downloaded.downloads,
            Need {
                minimum: 10,
                estimate: 20
            }
        );
        let staged = Demand::for_job("staged", 10, 10);
        assert_eq!(staged.downloads, Need::default());
        assert_eq!(
            staged.library,
            Need {
                minimum: 10,
                estimate: 20
            }
        );
        for status in ["installed", "failed", "cancelled"] {
            assert_eq!(
                Demand::for_job(status, 10, 0),
                Demand::default(),
                "{status}"
            );
        }
    }

    #[test]
    fn separate_volumes_are_counted_apart() {
        let forecast = forecast(
            Some(10 * GB),
            &at("a", Some(100 * GB)),
            &at("b", Some(25 * GB)),
            &[],
        );
        assert_eq!(forecast.volumes.len(), 2);
        assert_eq!(forecast.volumes[0].minimum_bytes, 20 * GB);
        assert_eq!(forecast.volumes[0].estimated_bytes, 30 * GB);
        assert_eq!(forecast.volumes[1].minimum_bytes, 10 * GB);
        assert_eq!(forecast.volumes[1].estimated_bytes, 20 * GB);
        assert_eq!(forecast.verdict, SpaceVerdict::Enough);
    }

    #[test]
    fn one_volume_adds_both_needs() {
        let disk = at("a", Some(40 * GB));
        let forecast = forecast(Some(10 * GB), &disk, &disk, &[]);
        assert_eq!(forecast.volumes.len(), 1);
        assert_eq!(forecast.volumes[0].minimum_bytes, 30 * GB);
        assert_eq!(forecast.volumes[0].estimated_bytes, 50 * GB);
        assert_eq!(forecast.verdict, SpaceVerdict::Tight);
    }

    #[test]
    fn verdicts_follow_the_bounds_and_the_margin() {
        let disk = |free| at("a", free);
        let verdict_for = |free| forecast(Some(10 * GB), &disk(free), &disk(free), &[]).verdict;
        assert_eq!(verdict_for(Some(30 * GB - 1)), SpaceVerdict::Insufficient);
        assert_eq!(verdict_for(Some(30 * GB)), SpaceVerdict::Tight);
        assert_eq!(
            verdict_for(Some(50 * GB + FREE_SPACE_MARGIN - 1)),
            SpaceVerdict::Tight
        );
        assert_eq!(
            verdict_for(Some(50 * GB + FREE_SPACE_MARGIN)),
            SpaceVerdict::Enough
        );
        assert_eq!(verdict_for(None), SpaceVerdict::Unknown);
    }

    #[test]
    fn unknown_size_is_unknown_but_insufficient_space_wins_over_unknown() {
        let unknown = forecast(
            None,
            &at("a", Some(100 * GB)),
            &at("b", Some(100 * GB)),
            &[],
        );
        assert_eq!(unknown.verdict, SpaceVerdict::Unknown);
        let mixed = forecast(Some(10 * GB), &at("a", None), &at("b", Some(GB)), &[]);
        assert_eq!(mixed.verdict, SpaceVerdict::Insufficient);
        assert_eq!(verdict(&[]), SpaceVerdict::Unknown);
    }

    #[test]
    fn queued_jobs_reserve_space_on_their_own_volumes() {
        let downloads = at("a", Some(100 * GB));
        let library = at("b", Some(100 * GB));
        let other = at("c", Some(100 * GB));
        let reservations = [
            Reservation {
                demand: Demand::for_job("queued", 10 * GB, 0),
                library: library.clone(),
            },
            Reservation {
                demand: Demand::for_job("staged", 10 * GB, 10 * GB),
                library: other,
            },
        ];
        let forecast = forecast(Some(GB), &downloads, &library, &reservations);
        // The first job downloads 10 GB and stages 20 GB, then copies 20 GB into the
        // library; the staged job only writes to another library.
        assert_eq!(forecast.volumes[0].reserved_bytes, 30 * GB);
        assert_eq!(forecast.volumes[1].reserved_bytes, 20 * GB);
        let tight = super::forecast(
            Some(30 * GB),
            &at("a", Some(100 * GB)),
            &at("a", Some(100 * GB)),
            &reservations[..1],
        );
        // 100 GB minus 50 GB reserved leaves less than the 90 GB lower bound.
        assert_eq!(tight.verdict, SpaceVerdict::Insufficient);
    }

    #[test]
    fn unknown_volume_identities_never_merge() {
        let unknown = Volume {
            id: None,
            path: PathBuf::from("/x"),
            free_bytes: Some(GB),
        };
        assert_eq!(forecast(Some(1), &unknown, &unknown, &[]).volumes.len(), 2);
    }

    #[test]
    fn install_check_fails_early_on_any_short_volume() {
        let check = |downloads: Volume, library: Option<Volume>| {
            check_install_space(10 * GB, &downloads, library.as_ref())
        };
        assert!(check(at("a", Some(20 * GB)), Some(at("a", Some(20 * GB)))).is_ok());
        let shared =
            check(at("a", Some(20 * GB - 1)), Some(at("a", Some(20 * GB - 1)))).unwrap_err();
        assert!(shared.contains("Not enough free disk space"), "{shared}");
        assert!(
            check(at("a", Some(10 * GB)), Some(at("b", Some(GB))))
                .unwrap_err()
                .contains("/b")
        );
        assert!(check(at("a", None), Some(at("b", None))).is_ok());
        assert!(check(at("a", Some(10 * GB)), None).is_ok());
        assert!(check(at("a", Some(GB)), None).is_err());
    }

    #[test]
    fn the_shortfall_message_names_both_amounts() {
        assert_eq!(
            extraction_shortfall(1_610_612_736, 322_122_547),
            "Not enough free disk space to extract the archive: it needs 1.5 GB and 0.3 GB are free. Free disk space and retry"
        );
    }
}
