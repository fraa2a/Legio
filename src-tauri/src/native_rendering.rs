use gtk::glib::translate::{ToGlibPtr, from_glib_borrow};
use gtk::prelude::*;

pub(crate) fn configure(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    window.with_webview(|platform| {
        let view = platform.inner();
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
    })
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
        eprintln!("Could not expand transparent window damage: {error}");
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
    use gtk::cairo::{Context, Format, ImageSurface, Operator, RectangleInt, Region};

    fn draw_frame(surface: &mut ImageSurface, partial_damage: bool) -> Vec<u8> {
        let damage = Region::create_rectangle(&RectangleInt::new(
            0,
            0,
            if partial_damage { 2 } else { 12 },
            if partial_damage { 2 } else { 12 },
        ));
        full_damage(&damage, 12, 12).expect("expand damage");
        let context = Context::new(&*surface).expect("image context");
        context.add_region(&damage);
        context.clip();
        context.set_operator(Operator::Clear);
        context.paint().expect("clear GTK damage region");
        context.set_operator(Operator::Over);
        context.set_source_rgba(0.15, 0.15, 0.15, 0.4);
        context.paint().expect("draw translucent background");
        context.set_source_rgba(1.0, 1.0, 1.0, 0.6);
        context.rectangle(6.0, 6.0, 4.0, 4.0);
        context.fill().expect("draw antialiased text pixels");
        drop(context);
        surface.flush();
        surface.data().expect("frame pixels").to_vec()
    }

    #[test]
    fn repeated_partial_updates_do_not_accumulate_translucent_text_pixels() {
        let mut surface = ImageSurface::create(Format::ARgb32, 12, 12).expect("image surface");
        let first = draw_frame(&mut surface, false);
        for _ in 0..20 {
            assert_eq!(draw_frame(&mut surface, true), first);
        }
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
