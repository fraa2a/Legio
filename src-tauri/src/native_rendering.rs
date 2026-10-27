use gtk::glib::translate::{ToGlibPtr, from_glib_borrow};
use gtk::prelude::*;

pub(crate) fn configure(
    window: &tauri::WebviewWindow,
    log: crate::application_log::ApplicationLog,
    transparent: bool,
) -> tauri::Result<()> {
    window.with_webview(move |platform| {
        use webkit2gtk::{SettingsExt, WebViewExt};

        let view = platform.inner();
        let version = crate::webkit_features::version();
        let skia_override = environment_switch("WEBKIT_USE_SKIA_FOR_COMPOSITION", &log);
        reject_live_damage_override(
            std::env::var_os("LEGIO_WEBKIT_DAMAGE_COMPOSITING").is_some(),
            &log,
        );
        let mut skia = None;
        let mut damage = None;
        let mut policy = String::from("unavailable");
        if let Some(settings) = WebViewExt::settings(&view) {
            policy = format!("{:?}", settings.hardware_acceleration_policy());
            if version >= (2, 54, 0) {
                match crate::webkit_features::Features::load() {
                    Ok(features) => {
                        skia = features.value(&settings, c"UseSkiaForComposition");
                        damage = features.value(&settings, c"UseDamagingInformationForCompositing");
                    }
                    Err(error) => {
                        log.record("warn", "linux_rendering_configuration", Some(error), None, false);
                    }
                }
            } else {
                log.record(
                    "warn",
                    "linux_rendering_configuration",
                    Some("Legacy WebKit runtime: Skia composition requires WebKitGTK 2.54 or newer"),
                    None,
                    false,
                );

            }
        } else {
            log.record("error", "linux_rendering_configuration", Some("WebKit view settings unavailable"), None, false);
        }
        let legacy_damage = legacy_damage_required(version, skia_override.or(skia), transparent);
        if legacy_damage {
            install_damage_handler(&view);
            view.connect_realize(install_damage_handler);
            view.connect_focus_in_event(|view, _| {
                view.queue_draw();
                gtk::glib::Propagation::Proceed
            });
            view.connect_focus_out_event(|view, _| {
                view.queue_draw();
                gtk::glib::Propagation::Proceed
            });
        }
        let backend = view.display().type_().name().to_owned();
        let disable_dmabuf = environment_switch("WEBKIT_DISABLE_DMABUF_RENDERER", &log);
        let force_shm = environment_switch("WEBKIT_DMABUF_RENDERER_FORCE_SHM", &log);
        log.record(
            "info",
            "linux_renderer",
            Some(&format!(
                "WebKitGTK={}.{}.{}; backend={backend}; skia_preference={skia:?}; damage_preference={damage:?}; skia_override={skia_override:?}; native_alpha={transparent}; legacy_gtk_damage={legacy_damage}; acceleration_policy={policy}; disable_dmabuf_override={disable_dmabuf:?}; force_shm_override={force_shm:?}",
                version.0, version.1, version.2,
            )),
            None,
            true,
        );
        view.connect_web_process_terminated(move |_, reason| {
            log.record("error", "linux_web_process_terminated", Some(&format!("reason={reason:?}")), None, true);
        });
    })
}

fn reject_live_damage_override(present: bool, log: &crate::application_log::ApplicationLog) {
    if present {
        log.record(
            "warn",
            "linux_rendering_configuration",
            Some("Live damage override unsupported: use the native rendering probe to configure composition before page creation"),
            None,
            false,
        );
    }
}

fn parse_switch(value: Option<&str>) -> Result<Option<bool>, &'static str> {
    match value {
        None => Ok(None),
        Some("0") => Ok(Some(false)),
        Some("1") => Ok(Some(true)),
        Some(_) => Err("Expected 0 or 1"),
    }
}

fn environment_switch(
    name: &'static str,
    log: &crate::application_log::ApplicationLog,
) -> Option<bool> {
    match std::env::var(name) {
        Ok(value) => match parse_switch(Some(&value)) {
            Ok(value) => value,
            Err(error) => {
                log.record(
                    "warn",
                    "linux_rendering_configuration",
                    Some(&format!("{name}: {error}")),
                    None,
                    false,
                );
                None
            }
        },
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            log.record(
                "warn",
                "linux_rendering_configuration",
                Some(&format!("{name}: expected 0 or 1")),
                None,
                false,
            );
            None
        }
    }
}

fn legacy_damage_required(version: (u32, u32, u32), skia: Option<bool>, transparent: bool) -> bool {
    transparent && (version < (2, 54, 0) || skia == Some(false))
}

fn install_damage_handler(view: &impl IsA<gtk::Widget>) {
    if let Some(window) = view.window() {
        // SAFETY: GTK owns the live window; the static callback retains no borrowed state.
        unsafe {
            gtk::gdk::ffi::gdk_window_set_invalidate_handler(
                window.to_glib_none().0,
                Some(expand_damage),
            );
        }
    }
}

unsafe extern "C" fn expand_damage(
    window: *mut gtk::gdk::ffi::GdkWindow,
    damage: *mut gtk::cairo::ffi::cairo_region_t,
) {
    // SAFETY: GDK supplies a live window and mutable damage region for this callback.
    let (window, damage) = unsafe {
        (
            from_glib_borrow::<_, gtk::gdk::Window>(window),
            gtk::cairo::Region::from_raw_none(damage),
        )
    };
    if let Err(error) = full_damage(&damage, window.width(), window.height()) {
        crate::application_log::failure("expand_window_damage", error);
    }
}

fn full_damage(
    damage: &gtk::cairo::Region,
    width: i32,
    height: i32,
) -> Result<(), gtk::cairo::Error> {
    // Expand before GDK prepares both its Cairo and GL paint regions.
    damage.union_rectangle(&gtk::cairo::RectangleInt::new(0, 0, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::cairo::{RectangleInt, Region};

    #[test]
    fn skia_runtime_does_not_install_legacy_damage_expansion() {
        assert!(!legacy_damage_required((2, 54, 0), None, true));
        assert!(!legacy_damage_required((2, 54, 1), Some(true), true));
        assert!(!legacy_damage_required((2, 56, 0), None, true));
        assert!(!legacy_damage_required((3, 0, 0), None, true));
    }

    #[test]
    fn legacy_damage_expansion_is_limited_to_transparent_legacy_views() {
        assert!(legacy_damage_required((2, 52, 6), None, true));
        assert!(legacy_damage_required((2, 54, 1), Some(false), true));
        assert!(!legacy_damage_required((2, 52, 6), None, false));
        assert!(!legacy_damage_required((2, 54, 1), Some(false), false));
    }

    #[test]
    fn diagnostic_switch_rejects_invalid_values_without_changing_defaults() {
        assert_eq!(parse_switch(None), Ok(None));
        assert_eq!(parse_switch(Some("0")), Ok(Some(false)));
        assert_eq!(parse_switch(Some("1")), Ok(Some(true)));
        assert!(parse_switch(Some("false")).is_err());
        assert!(parse_switch(Some("")).is_err());
        assert!(parse_switch(Some("2")).is_err());
    }

    #[test]
    fn unsupported_live_damage_override_is_reported_instead_of_claimed_applied() {
        let directory =
            std::env::temp_dir().join(format!("legio-rendering-log-{}", uuid::Uuid::new_v4()));
        let log = crate::application_log::ApplicationLog::new(directory.clone(), true);
        reject_live_damage_override(false, &log);
        assert!(!directory.join("app.txt").exists());
        reject_live_damage_override(true, &log);
        let text = std::fs::read_to_string(directory.join("app.txt")).unwrap();
        assert!(text.contains("linux_rendering_configuration"));
        assert!(text.contains("before page creation"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn damage_tracks_the_current_window_size_and_preserves_existing_regions() {
        let damage = Region::create_rectangle(&RectangleInt::new(20, 20, 2, 2));
        full_damage(&damage, 8, 8).expect("initial allocation");
        assert!(damage.contains_point(7, 7));
        assert!(!damage.contains_point(10, 10));
        assert!(damage.contains_point(21, 21));
        full_damage(&damage, 16, 16).expect("resized allocation");
        assert!(damage.contains_point(15, 15));
        assert!(damage.contains_point(21, 21));
        assert!(!damage.contains_point(18, 18));
    }
}
