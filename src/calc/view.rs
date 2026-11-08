//! A renderer-agnostic description of what the LCD shows. The calculator
//! state produces a `ViewModel`; the GPUI layer only draws it.

pub const COLS: usize = 32;
pub const ROWS: usize = 10;
/// LCD size in logical pixels.
pub const LCD_W: f32 = 320.0;
pub const LCD_H: f32 = 240.0;
pub const STATUS_H: f32 = 30.0;
/// The content area below the status bar.
pub const GRAPH_W: f32 = 320.0;
pub const GRAPH_H: f32 = 210.0;

pub mod color {
    pub const BLACK: u32 = 0x000000;
    pub const WHITE: u32 = 0xFFFFFF;
    pub const BLUE: u32 = 0x0000FF;
    pub const RED: u32 = 0xFF0000;
    pub const MAGENTA: u32 = 0xFF00FF;
    pub const GREEN: u32 = 0x00A000;
    pub const ORANGE: u32 = 0xFF8C00;
    pub const BROWN: u32 = 0x8B4513;
    pub const NAVY: u32 = 0x000080;
    pub const LTBLUE: u32 = 0x1E90FF;
    pub const YELLOW: u32 = 0xD4C000;
    pub const GRAY: u32 = 0x808080;
    pub const LTGRAY: u32 = 0xC8C8C8;
    pub const DKGRAY: u32 = 0x505050;
    /// Selection highlight used by menus and settings screens.
    pub const SEL: u32 = 0x2D4E78;
    pub const STATUS_BG: u32 = 0xD8DEE6;
    pub const STATUS_FG: u32 = 0x2D4E78;
}

pub const Y_COLORS: [u32; 10] = [
    color::BLUE,
    color::RED,
    color::BLACK,
    color::MAGENTA,
    color::GREEN,
    color::ORANGE,
    color::BROWN,
    color::NAVY,
    color::LTBLUE,
    color::YELLOW,
];

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub fg: u32,
    pub bg: Option<u32>,
}

impl Span {
    pub fn new(text: impl Into<String>) -> Self {
        Span { text: text.into(), fg: color::BLACK, bg: None }
    }
    pub fn fg(mut self, c: u32) -> Self {
        self.fg = c;
        self
    }
    pub fn bg(mut self, c: u32) -> Self {
        self.bg = Some(c);
        self
    }
    pub fn inverse(text: impl Into<String>) -> Self {
        Span { text: text.into(), fg: color::WHITE, bg: Some(color::SEL) }
    }
}

pub type Row = Vec<Span>;

pub fn plain(text: impl Into<String>) -> Row {
    vec![Span::new(text)]
}

/// Right-align plain text in a row.
pub fn right(text: &str) -> Row {
    let len = text.chars().count();
    if len >= COLS {
        return plain(text);
    }
    plain(format!("{}{}", " ".repeat(COLS - len), text))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indicator {
    None,
    Second,
    Alpha,
    AlphaLock,
    Lower,
    LowerLock,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub text: String,
    pub indicator: Indicator,
    pub busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    /// Free-moving crosshair.
    Cross,
    /// Trace cursor on a curve.
    Trace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grid {
    Off,
    Dot,
    Line,
}

#[derive(Debug, Clone, Default)]
pub struct Curve {
    pub color: u32,
    pub thick: bool,
    pub dots: bool,
    pub segments: Vec<Vec<(f32, f32)>>,
}

#[derive(Debug, Clone)]
pub struct GraphModel {
    pub curves: Vec<Curve>,
    /// Pixel row of the x-axis and pixel column of the y-axis, if visible.
    pub x_axis: Option<f32>,
    pub y_axis: Option<f32>,
    pub x_ticks: Vec<f32>,
    pub y_ticks: Vec<f32>,
    pub grid: Grid,
    pub cursor: Option<(f32, f32, CursorStyle, u32)>,
    pub marks: Vec<(f32, f32)>,
    pub shade: Option<(u32, Vec<(f32, f32)>)>,
    pub top: Option<Row>,
    pub bottom: Vec<Row>,
}

#[derive(Debug, Clone)]
pub enum Body {
    Text(Vec<Row>),
    Graph(GraphModel),
    Off,
}

#[derive(Debug, Clone)]
pub struct ViewModel {
    pub status: Status,
    pub body: Body,
    /// Python app soft-key labels shown along the bottom of the screen.
    pub softkeys: Option<[&'static str; 5]>,
}
