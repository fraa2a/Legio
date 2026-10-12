use std::sync::Mutex;

use tauri::WindowEvent;

pub(crate) fn install(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let last_zoom = Mutex::new(None);
    update(window, &last_zoom)?;
    let webview = window.clone();
    window.on_window_event(move |event| {
        if matches!(
            event,
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }
        ) && let Err(error) = update(&webview, &last_zoom)
        {
            crate::application_log::failure("resize_interface", error);
        }
    });
    Ok(())
}

fn update(window: &tauri::WebviewWindow, last_zoom: &Mutex<Option<f64>>) -> tauri::Result<()> {
    let size = window.inner_size()?;
    let Some(zoom) = zoom_for_size(size.width, size.height, window.scale_factor()?) else {
        return Ok(());
    };
    let mut last_zoom = last_zoom.lock().map_err(|_| {
        tauri::Error::Io(std::io::Error::other("Interface scaling state unavailable"))
    })?;
    if *last_zoom != Some(zoom) {
        window.set_zoom(zoom)?;
        *last_zoom = Some(zoom);
    }
    Ok(())
}

pub(crate) fn zoom_for_size(width: u32, height: u32, display_scale: f64) -> Option<f64> {
    if width == 0 || height == 0 || !display_scale.is_finite() || display_scale <= 0.0 {
        return None;
    }
    Some(
        (f64::from(width) / display_scale / 1280.0)
            .min(f64::from(height) / display_scale / 720.0)
            .min(1.0),
    )
}

#[cfg(test)]
mod tests {
    use super::zoom_for_size;

    #[test]
    fn compact_windows_preserve_the_full_layout() {
        for (width, height, expected) in [
            (960, 540, 0.75),
            (1120, 630, 0.875),
            (1280, 720, 1.0),
            (1920, 1080, 1.0),
        ] {
            let zoom = zoom_for_size(width, height, 1.0).unwrap();
            assert_eq!(zoom, expected);
            assert!(f64::from(width) / zoom >= 1280.0);
            assert!(f64::from(height) / zoom >= 720.0);
        }
    }

    #[test]
    fn either_dimension_can_limit_the_scale() {
        assert_eq!(zoom_for_size(1280, 540, 1.0), Some(0.75));
        assert_eq!(zoom_for_size(960, 900, 1.0), Some(0.75));
    }

    #[test]
    fn display_scaling_is_applied_once() {
        assert_eq!(zoom_for_size(1200, 675, 1.25), Some(0.75));
        assert_eq!(zoom_for_size(1920, 1080, 1.5), Some(1.0));
    }

    #[test]
    fn minimized_or_invalid_sizes_do_not_change_zoom() {
        for (width, height, scale) in [
            (0, 0, 1.0),
            (960, 0, 1.0),
            (960, 540, 0.0),
            (960, 540, f64::NAN),
        ] {
            assert_eq!(zoom_for_size(width, height, scale), None);
        }
    }
}
