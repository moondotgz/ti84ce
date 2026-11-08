//! GPUI front end: the calculator body, LCD renderer and clickable keypad.

use crate::calc::view::{self as v, Body, CursorStyle, GraphModel, Grid, Indicator, Row, ViewModel};
use crate::calc::Calc;
use crate::keys::{Key, KeyDef, KeyStyle, GRID, TOP_ROW, UPPER_LEFT};
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::time::Duration;

/// Design size of the whole calculator in logical units; everything scales with the window.
pub const BASE_W: f32 = 440.0;
pub const BASE_H: f32 = 960.0;
const LCD_SCALE: f32 = 1.15;
const MONO: &str = "DejaVu Sans Mono";
const SANS: &str = "DejaVu Sans";

const SECOND_LABEL: u32 = 0x6EC3F0;
const ALPHA_LABEL: u32 = 0x8CCB5E;

pub struct CalcView {
    calc: Calc,
    focus: FocusHandle,
    ticks: u32,
    _timer: Task<()>,
}

impl CalcView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus);
        let timer = cx.spawn(async move |this, cx| loop {
            Timer::after(Duration::from_millis(50)).await;
            if this.update(cx, |view, cx| view.on_tick(cx)).is_err() {
                break;
            }
        });
        let mut calc = Calc::new();
        // Debug hook: replay key presses, e.g. TI84CE_SCRIPT='2 + 3 enter "abc'.
        if let Ok(script) = std::env::var("TI84CE_SCRIPT") {
            calc.run_script(&script);
        }
        CalcView { calc, focus, ticks: 0, _timer: timer }
    }

    fn on_tick(&mut self, cx: &mut Context<Self>) {
        self.ticks += 1;
        let mut changed = self.calc.tick();
        if self.ticks % 10 == 0 {
            self.calc.blink = !self.calc.blink;
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    fn press(&mut self, key: Key, cx: &mut Context<Self>) {
        self.calc.press(key);
        self.ticks = 0;
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = ks.modifiers;
        if m.control || m.platform {
            match ks.key.as_str() {
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                        self.calc.paste(&text);
                    }
                }
                "q" => cx.quit(),
                _ => {}
            }
            self.ticks = 0;
            self.calc.blink = true;
            cx.notify();
            return;
        }
        let key = match ks.key.as_str() {
            "enter" => Some(Key::Enter),
            "escape" => Some(Key::Clear),
            "delete" => Some(Key::Del),
            "up" => Some(Key::Up),
            "down" => Some(Key::Down),
            "left" => Some(Key::Left),
            "right" => Some(Key::Right),
            "home" => {
                self.calc.press(Key::Second);
                Some(Key::Left)
            }
            "end" => {
                self.calc.press(Key::Second);
                Some(Key::Right)
            }
            "f1" => Some(Key::YEq),
            "f2" => Some(Key::Window),
            "f3" => Some(Key::Zoom),
            "f4" => Some(Key::Trace),
            "f5" => Some(Key::Graph),
            "f6" => Some(Key::Mode),
            "f7" => Some(Key::Math),
            "f8" => Some(Key::Apps),
            "f9" => Some(Key::Stat),
            "f10" => Some(Key::Vars),
            "backspace" => {
                self.calc.keyboard_backspace();
                None
            }
            "tab" => {
                self.calc.paste("  ");
                None
            }
            _ => {
                if let Some(text) = &ks.key_char {
                    for c in text.chars() {
                        self.calc.type_char(c);
                    }
                }
                None
            }
        };
        if let Some(k) = key {
            // Shift acts as [2nd] for arrows and function keys.
            if m.shift && !matches!(k, Key::Enter | Key::Del) {
                self.calc.press(Key::Second);
            }
            self.calc.press(k);
        }
        self.ticks = 0;
        self.calc.blink = true;
        cx.notify();
    }

    fn key_colors(style: KeyStyle) -> (u32, u32, u32) {
        // (cap, text, pressed)
        match style {
            KeyStyle::Top => (0xE9EAEC, 0x1B1C1E, 0xC5C7CA),
            KeyStyle::Func => (0x34373C, 0xF2F2F2, 0x24262A),
            KeyStyle::Number => (0xEDEDED, 0x111111, 0xC9C9C9),
            KeyStyle::Op => (0xD3D5D8, 0x111111, 0xB0B2B5),
            KeyStyle::Second => (0x2F9BD6, 0xFFFFFF, 0x2380B3),
            KeyStyle::Alpha => (0x4E9F3D, 0xFFFFFF, 0x3D8030),
            KeyStyle::Enter => (0xD3D5D8, 0x111111, 0xB0B2B5),
        }
    }

    fn key(&self, def: &KeyDef, col_w: f32, cap_h: f32, u: f32, cx: &mut Context<Self>) -> AnyElement {
        let (bg, fg, pressed) = Self::key_colors(def.style);
        let key = def.key;
        let cap_w = col_w - 12.0;
        let small = def.label.chars().count() > 4;
        let label_size = if def.style == KeyStyle::Top || small { 11.0 } else { 14.5 };
        div()
            .flex()
            .flex_col()
            .items_center()
            .w(px(col_w * u))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .w(px((cap_w + 4.0) * u))
                    .h(px(13.0 * u))
                    .text_size(px(9.0 * u))
                    .whitespace_nowrap()
                    .child(div().text_color(rgb(SECOND_LABEL)).child(def.second))
                    .child(div().text_color(rgb(ALPHA_LABEL)).child(def.alpha)),
            )
            .child(
                div()
                    .id(("key", key as usize))
                    .w(px(cap_w * u))
                    .h(px(cap_h * u))
                    .rounded(px(7.0 * u))
                    .bg(rgb(bg))
                    .border_b(px(2.5 * u))
                    .border_color(rgba(0x00000066))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(fg))
                    .text_size(px(label_size * u))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .active(move |s| s.bg(rgb(pressed)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _ev, window, cx| {
                            window.focus(&this.focus);
                            this.press(key, cx);
                        }),
                    )
                    .child(def.label),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn arrow(&self, key: Key, glyph: &'static str, x: f32, y: f32, w: f32, h: f32, u: f32, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id(("arrow", key as usize))
            .absolute()
            .left(px(x * u))
            .top(px(y * u))
            .w(px(w * u))
            .h(px(h * u))
            .rounded(px(10.0 * u))
            .bg(rgb(0x3B3E44))
            .border_b(px(2.5 * u))
            .border_color(rgba(0x00000066))
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(0xE8E8E8))
            .text_size(px(13.0 * u))
            .cursor_pointer()
            .active(|s| s.bg(rgb(0x2A2C30)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _ev, window, cx| {
                    window.focus(&this.focus);
                    this.press(key, cx);
                }),
            )
            .child(glyph)
            .into_any_element()
    }

    fn keypad(&self, u: f32, cx: &mut Context<Self>) -> AnyElement {
        let col = 76.0;
        let mut top_keys = Vec::new();
        for d in TOP_ROW.iter() {
            top_keys.push(self.key(d, col, 24.0, u, cx));
        }
        let top = div().flex().flex_row().children(top_keys);
        let mut ul_rows = Vec::new();
        for row in UPPER_LEFT.iter() {
            let mut keys = Vec::new();
            for d in row.iter() {
                keys.push(self.key(d, col, 30.0, u, cx));
            }
            ul_rows.push(div().flex().flex_row().children(keys));
        }
        let upper_left = div().flex().flex_col().gap(px(4.0 * u)).children(ul_rows);
        let dpad = div()
            .relative()
            .w(px(152.0 * u))
            .h(px(98.0 * u))
            .child(
                div()
                    .absolute()
                    .left(px(54.0 * u))
                    .top(px(32.0 * u))
                    .w(px(44.0 * u))
                    .h(px(34.0 * u))
                    .rounded(px(17.0 * u))
                    .bg(rgb(0x18191B)),
            )
            .child(self.arrow(Key::Up, "▲", 51.0, 2.0, 50.0, 30.0, u, cx))
            .child(self.arrow(Key::Down, "▼", 51.0, 66.0, 50.0, 30.0, u, cx))
            .child(self.arrow(Key::Left, "◀", 8.0, 29.0, 44.0, 40.0, u, cx))
            .child(self.arrow(Key::Right, "▶", 100.0, 29.0, 44.0, 40.0, u, cx));
        let upper = div().flex().flex_row().items_center().child(upper_left).child(dpad);
        let mut grid_rows = Vec::new();
        for row in GRID.iter() {
            let mut keys = Vec::new();
            for d in row.iter() {
                keys.push(self.key(d, col, 32.0, u, cx));
            }
            grid_rows.push(div().flex().flex_row().children(keys));
        }
        let grid = div().flex().flex_col().gap(px(4.0 * u)).children(grid_rows);
        div()
            .flex()
            .flex_col()
            .gap(px(6.0 * u))
            .w(px(380.0 * u))
            .child(top)
            .child(upper)
            .child(grid)
            .into_any_element()
    }

    fn text_rows(rows: &[Row], max_rows: usize, s: f32) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .font_family(MONO)
            .text_size(px(16.6 * s))
            .line_height(px(21.0 * s))
            .children(rows.iter().take(max_rows).map(|row| {
                div().h(px(21.0 * s)).flex().flex_row().whitespace_nowrap().overflow_hidden().children(row.iter().map(|span| {
                    div()
                        .text_color(rgb(span.fg))
                        .when_some(span.bg, |d, bg| d.bg(rgb(bg)))
                        .child(span.text.clone())
                }))
            }))
    }

    fn status_bar(model: &ViewModel, s: f32) -> impl IntoElement {
        let st = &model.status;
        let ind = match st.indicator {
            Indicator::None => None,
            Indicator::Second => Some(("2ND", 0x2F9BD6)),
            Indicator::Alpha => Some(("A", 0x4E9F3D)),
            Indicator::AlphaLock => Some(("A▪", 0x4E9F3D)),
            Indicator::Lower => Some(("a", 0x4E9F3D)),
            Indicator::LowerLock => Some(("a▪", 0x4E9F3D)),
        };
        div()
            .h(px(v::STATUS_H * s))
            .flex()
            .flex_row()
            .items_center()
            .px(px(4.0 * s))
            .gap(px(4.0 * s))
            .bg(rgb(v::color::STATUS_BG))
            .border_b(px(1.0 * s))
            .border_color(rgb(0xA8B0BA))
            .font_family(SANS)
            .text_size(px(10.0 * s))
            .text_color(rgb(v::color::STATUS_FG))
            .child(
                div()
                    .w(px(30.0 * s))
                    .h(px(16.0 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when_some(ind, |d, (t, c)| {
                        d.bg(rgb(c)).rounded(px(3.0 * s)).text_color(rgb(0xFFFFFF)).font_weight(FontWeight::BOLD).child(t)
                    }),
            )
            .child(div().flex_1().whitespace_nowrap().overflow_hidden().font_weight(FontWeight::SEMIBOLD).child(st.text.clone()))
            .when(st.busy, |d| {
                d.child(div().text_color(rgb(0xC03030)).font_weight(FontWeight::BOLD).child("BUSY"))
            })
            .child(
                // Battery icon.
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(
                        div()
                            .w(px(20.0 * s))
                            .h(px(10.0 * s))
                            .border(px(1.0 * s))
                            .border_color(rgb(v::color::STATUS_FG))
                            .p(px(1.0 * s))
                            .child(div().size_full().bg(rgb(0x3BA55C))),
                    )
                    .child(div().w(px(2.0 * s)).h(px(4.0 * s)).bg(rgb(v::color::STATUS_FG))),
            )
    }

    fn softkeys(labels: [&'static str; 5], s: f32) -> impl IntoElement {
        div()
            .h(px(21.0 * s))
            .flex()
            .flex_row()
            .gap(px(2.0 * s))
            .px(px(1.0 * s))
            .font_family(SANS)
            .text_size(px(10.5 * s))
            .children(labels.iter().map(move |l| {
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_t(px(4.0 * s))
                    .when(!l.is_empty(), |d| d.bg(rgb(v::color::SEL)))
                    .text_color(rgb(0xFFFFFF))
                    .whitespace_nowrap()
                    .child(*l)
            }))
    }

    fn lcd(&self, model: ViewModel, s: f32) -> impl IntoElement {
        let body_h = v::GRAPH_H * s;
        let screen = div()
            .w(px(v::LCD_W * s))
            .h(px(v::LCD_H * s))
            .bg(rgb(0xFFFFFF))
            .overflow_hidden()
            .flex()
            .flex_col();
        if let Body::Off = model.body {
            return screen.bg(rgb(0x0B0D0B));
        }
        let status = Self::status_bar(&model, s);
        let content = match &model.body {
            Body::Text(rows) => {
                let max = if model.softkeys.is_some() { v::ROWS - 1 } else { v::ROWS };
                div()
                    .h(px(body_h))
                    .flex()
                    .flex_col()
                    .justify_between()
                    .child(Self::text_rows(rows, max, s))
                    .when_some(model.softkeys, |d, keys| d.child(Self::softkeys(keys, s)))
                    .into_any_element()
            }
            Body::Graph(g) => Self::graph(g.clone(), s).into_any_element(),
            Body::Off => div().into_any_element(),
        };
        screen.child(status).child(content)
    }

    fn graph(g: GraphModel, s: f32) -> impl IntoElement {
        let top = g.top.clone();
        let bottom = g.bottom.clone();
        let paint_model = g;
        div()
            .relative()
            .w(px(v::GRAPH_W * s))
            .h(px(v::GRAPH_H * s))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| paint_graph(&paint_model, bounds, s, window),
                )
                .absolute()
                .size_full(),
            )
            .when_some(top, |d, row| {
                d.child(div().absolute().top(px(1.0 * s)).left(px(0.0)).bg(rgba(0xFFFFFFC8)).child(Self::text_rows(&[row], 1, s)))
            })
            .when(!bottom.is_empty(), |d| {
                let n = bottom.len();
                d.child(
                    div()
                        .absolute()
                        .bottom(px(0.0))
                        .left(px(0.0))
                        .bg(rgba(0xFFFFFFC8))
                        .child(Self::text_rows(&bottom, n, s)),
                )
            })
    }
}

fn paint_graph(g: &GraphModel, bounds: Bounds<Pixels>, s: f32, window: &mut Window) {
    let o = bounds.origin;
    let pt = |x: f32, y: f32| point(o.x + px(x * s), o.y + px(y * s));
    let rect = |x: f32, y: f32, w: f32, h: f32| Bounds::new(pt(x, y), size(px(w * s), px(h * s)));
    window.paint_quad(fill(bounds, rgb(0xFFFFFF)));

    let line = |window: &mut Window, a: (f32, f32), b: (f32, f32), w: f32, color: Rgba| {
        let mut pb = PathBuilder::stroke(px(w * s));
        pb.move_to(pt(a.0, a.1));
        pb.line_to(pt(b.0, b.1));
        if let Ok(path) = pb.build() {
            window.paint_path(path, color);
        }
    };

    // Grid.
    let (xa, ya) = (g.x_axis, g.y_axis);
    let mut xs: Vec<f32> = g.x_ticks.clone();
    let mut ys: Vec<f32> = g.y_ticks.clone();
    if let Some(y) = ya {
        xs.push(y);
    }
    if let Some(x) = xa {
        ys.push(x);
    }
    match g.grid {
        Grid::Line => {
            for &x in &xs {
                window.paint_quad(fill(rect(x, 0.0, 0.6, v::GRAPH_H), rgb(0xD5D9DE)));
            }
            for &y in &ys {
                window.paint_quad(fill(rect(0.0, y, v::GRAPH_W, 0.6), rgb(0xD5D9DE)));
            }
        }
        Grid::Dot => {
            for &x in &xs {
                for &y in &ys {
                    window.paint_quad(fill(rect(x - 0.6, y - 0.6, 1.2, 1.2), rgb(0x8A9099)));
                }
            }
        }
        Grid::Off => {}
    }

    // Shaded integral.
    if let Some((c, pts)) = &g.shade {
        if pts.len() >= 3 {
            let mut pb = PathBuilder::fill();
            pb.move_to(pt(pts[0].0, pts[0].1));
            for p in &pts[1..] {
                pb.line_to(pt(p.0, p.1));
            }
            pb.close();
            if let Ok(path) = pb.build() {
                let mut col = rgb(*c);
                col.a = 0.3;
                window.paint_path(path, col);
            }
        }
    }

    // Axes and tick marks.
    let axis = rgb(0x000000);
    if let Some(y) = xa {
        window.paint_quad(fill(rect(0.0, y - 0.5, v::GRAPH_W, 1.0), axis));
        for &x in &g.x_ticks {
            window.paint_quad(fill(rect(x - 0.5, y - 2.5, 1.0, 5.0), axis));
        }
    }
    if let Some(x) = ya {
        window.paint_quad(fill(rect(x - 0.5, 0.0, 1.0, v::GRAPH_H), axis));
        for &y in &g.y_ticks {
            window.paint_quad(fill(rect(x - 2.5, y - 0.5, 5.0, 1.0), axis));
        }
    }

    // Curves.
    let mask = bounds;
    window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
        for c in &g.curves {
            let color = rgb(c.color);
            let w = if c.thick { 2.2 } else { 1.2 };
            for seg in &c.segments {
                if c.dots || seg.len() == 1 {
                    for &(x, y) in seg {
                        if (0.0..=v::GRAPH_H).contains(&y) {
                            window.paint_quad(fill(rect(x - w / 2.0, y - w / 2.0, w, w), color));
                        }
                    }
                    continue;
                }
                let mut pb = PathBuilder::stroke(px(w * s));
                pb.move_to(pt(seg[0].0, seg[0].1));
                for &(x, y) in &seg[1..] {
                    pb.line_to(pt(x, y));
                }
                if let Ok(path) = pb.build() {
                    window.paint_path(path, color);
                }
            }
        }

        // Bound markers.
        for &(x, _) in &g.marks {
            let mut pb = PathBuilder::fill();
            pb.move_to(pt(x - 4.0, 0.0));
            pb.line_to(pt(x + 4.0, 0.0));
            pb.line_to(pt(x, 7.0));
            pb.close();
            if let Ok(path) = pb.build() {
                window.paint_path(path, rgb(0x000000));
            }
        }

        // Cursor.
        if let Some((x, y, style, c)) = g.cursor {
            let col = rgb(c);
            match style {
                CursorStyle::Cross => {
                    let k = rgb(0x000000);
                    line(window, (x - 7.0, y), (x - 2.0, y), 1.2, k);
                    line(window, (x + 2.0, y), (x + 7.0, y), 1.2, k);
                    line(window, (x, y - 7.0), (x, y - 2.0), 1.2, k);
                    line(window, (x, y + 2.0), (x, y + 7.0), 1.2, k);
                }
                CursorStyle::Trace => {
                    let r = 4.5;
                    line(window, (x - r, y - r), (x + r, y - r), 1.4, col);
                    line(window, (x + r, y - r), (x + r, y + r), 1.4, col);
                    line(window, (x + r, y + r), (x - r, y + r), 1.4, col);
                    line(window, (x - r, y + r), (x - r, y - r), 1.4, col);
                    line(window, (x - r - 3.0, y), (x - r, y), 1.2, col);
                    line(window, (x + r, y), (x + r + 3.0, y), 1.2, col);
                    line(window, (x, y - r - 3.0), (x, y - r), 1.2, col);
                    line(window, (x, y + r), (x, y + r + 3.0), 1.2, col);
                }
            }
        }
    });
}

impl Render for CalcView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let vp = window.viewport_size();
        let u = (f32::from(vp.width) / BASE_W).min(f32::from(vp.height) / BASE_H);
        let s = LCD_SCALE * u;
        let model = self.calc.view();
        let header = div()
            .id("header")
            .w(px(372.0 * u))
            .h(px(30.0 * u))
            .flex()
            .flex_row()
            .items_end()
            .justify_between()
            .pb(px(4.0 * u))
            .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            .child(
                div()
                    .text_color(rgb(0xF4F4F4))
                    .text_size(px(15.0 * u))
                    .font_weight(FontWeight::BOLD)
                    .child("TI-84 Plus CE-T"),
            )
            .child(
                div()
                    .text_color(rgb(0xF2C744))
                    .text_size(px(11.0 * u))
                    .font_weight(FontWeight::BOLD)
                    .child("PYTHON EDITION"),
            );
        let bezel = div()
            .p(px(12.0 * u))
            .rounded(px(12.0 * u))
            .bg(rgb(0x0B0B0C))
            .border(px(1.0 * u))
            .border_color(rgb(0x2E3034))
            .child(self.lcd(model, s));
        let body = div()
            .w(px(420.0 * u))
            .h(px(944.0 * u))
            .rounded(px(38.0 * u))
            .bg(rgb(0x1F2125))
            .border(px(2.0 * u))
            .border_color(rgb(0x3A3D42))
            .shadow_lg()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(10.0 * u))
            .gap(px(4.0 * u))
            .child(header)
            .child(bezel)
            .child(div().h(px(14.0 * u)))
            .child(self.keypad(u, cx));
        div()
            .size_full()
            .bg(rgb(0x14161A))
            .flex()
            .items_center()
            .justify_center()
            .font_family(SANS)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .child(body)
    }
}
