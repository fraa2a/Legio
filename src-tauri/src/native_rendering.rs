use gtk::prelude::*;

pub(crate) fn configure(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    window.with_webview(|platform| {
        let view = platform.inner();
        view.connect_draw(|view, context| {
            if let Err(error) =
                prepare_frame(context, view.allocated_width(), view.allocated_height())
            {
                eprintln!("Could not prepare transparent window frame: {error}");
            }
            gtk::glib::Propagation::Proceed
        });
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

fn prepare_frame(
    context: &gtk::cairo::Context,
    width: i32,
    height: i32,
) -> Result<(), gtk::cairo::Error> {
    context.reset_clip();
    context.new_path();
    context.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
    context.clip();
    // GTK clears only its damage region; the expanded clip must start transparent too.
    let operator = context.operator();
    context.set_operator(gtk::cairo::Operator::Clear);
    let result = context.paint();
    context.set_operator(operator);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::cairo::{Context, Format, ImageSurface, Operator};

    fn draw_frame(surface: &mut ImageSurface, partial_damage: bool) -> Vec<u8> {
        let context = Context::new(&*surface).expect("image context");
        if partial_damage {
            context.rectangle(0.0, 0.0, 2.0, 2.0);
            context.clip();
            context.set_operator(Operator::Clear);
            context.paint().expect("clear GTK damage region");
            context.set_operator(Operator::Over);
        }
        prepare_frame(&context, 12, 12).expect("prepare frame");
        assert_eq!(context.operator(), Operator::Over);
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
    fn full_clear_stays_within_the_translated_webview_allocation() {
        let mut surface = ImageSurface::create(Format::ARgb32, 16, 16).expect("image surface");
        let context = Context::new(&surface).expect("image context");
        context.set_source_rgb(0.0, 1.0, 0.0);
        context.paint().expect("draw surrounding widgets");
        context.translate(4.0, 4.0);
        context.rectangle(0.0, 0.0, 2.0, 2.0);
        context.clip();
        prepare_frame(&context, 8, 8).expect("prepare frame");
        drop(context);
        surface.flush();
        let stride = usize::try_from(surface.stride()).expect("positive stride");
        let data = surface.data().expect("frame pixels");
        let surrounding = data[..4].to_vec();
        for y in 0..16 {
            for x in 0..16 {
                let offset = y * stride + x * 4;
                let expected: &[u8] = if (4..12).contains(&x) && (4..12).contains(&y) {
                    &[0, 0, 0, 0]
                } else {
                    &surrounding
                };
                assert_eq!(&data[offset..offset + 4], expected);
            }
        }
    }
}
