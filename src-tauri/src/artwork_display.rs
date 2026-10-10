use std::sync::atomic::{AtomicU32, Ordering};

use tauri::Manager;

const DEFAULT_WIDTH: u32 = 1920;
const MAX_HERO_WIDTH: u32 = 3840;

pub(crate) struct ArtworkDisplayState(AtomicU32);

impl Default for ArtworkDisplayState {
    fn default() -> Self {
        Self(AtomicU32::new(DEFAULT_WIDTH))
    }
}

fn largest_width(monitors: impl IntoIterator<Item = (u32, u32)>) -> u32 {
    monitors
        .into_iter()
        .filter(|(width, height)| *width > 0 && *height > 0)
        .map(|(width, height)| width.max(height))
        .max()
        .unwrap_or(DEFAULT_WIDTH)
        .min(MAX_HERO_WIDTH)
}

pub(crate) fn width<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> u32 {
    app.try_state::<ArtworkDisplayState>()
        .map_or(DEFAULT_WIDTH, |state| state.0.load(Ordering::Relaxed))
}

pub(crate) fn refresh<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> u32 {
    match app.available_monitors() {
        Ok(monitors) => {
            let width = largest_width(monitors.iter().map(|monitor| {
                let size = monitor.size();
                (size.width, size.height)
            }));
            if let Some(state) = app.try_state::<ArtworkDisplayState>() {
                state.0.store(width, Ordering::Relaxed);
            }
            width
        }
        Err(error) => {
            crate::application_log::failure("inspect_artwork_resolution", error);
            width(app)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_highest_physical_resolution_across_all_monitors() {
        assert_eq!(
            largest_width([(1280, 720), (3840, 2160), (1280, 720)]),
            3840
        );
        assert_eq!(largest_width([(1920, 1080), (2560, 1440)]), 2560);
        assert_eq!(largest_width([(1280, 720)]), 1280);
        assert_eq!(largest_width([(1440, 2560), (1920, 1080)]), 2560);
        assert_eq!(largest_width([(7680, 4320)]), 3840);
        assert_eq!(largest_width([(0, 0)]), DEFAULT_WIDTH);
        assert_eq!(largest_width([]), DEFAULT_WIDTH);
    }
}
