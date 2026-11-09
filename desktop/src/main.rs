//! A TI-84 Plus CE-T Python Edition clone written in Rust with GPUI.

mod ui;

use gpui::*;

fn main() {
    Application::new().run(|cx: &mut App| {
        let display_h = cx
            .primary_display()
            .map(|d| f32::from(d.bounds().size.height))
            .unwrap_or(1080.0);
        let h = (display_h * 0.9).min(ui::BASE_H);
        let w = h * ui::BASE_W / ui::BASE_H;
        let bounds = Bounds::centered(None, size(px(w), px(h)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("TI-84 Plus CE-T Python Edition".into()),
                    ..Default::default()
                }),
                app_id: Some("ti84ce".into()),
                window_min_size: Some(size(px(220.0), px(480.0))),
                window_decorations: Some(WindowDecorations::Server),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| ui::CalcView::new(window, cx)),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
