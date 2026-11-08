//! Function graphing: window settings, zoom, free cursor, TRACE and the
//! CALC menu operations (value, zero, min/max, intersect, dy/dx, ∫).

use super::editor::{wrap_with_cursor, LineEditor};
use super::menus::Cmd;
use super::view::*;
use super::{Calc, ErrFrom, Mods, Screen};
use crate::engine::format::{format_num, NumFormat};
use crate::engine::{parse_expr, CalcError, Env, Tok, YVARS};
use crate::keys::Key;

#[derive(Debug, Clone, PartialEq)]
pub struct WindowVals {
    pub xmin: f64,
    pub xmax: f64,
    pub xscl: f64,
    pub ymin: f64,
    pub ymax: f64,
    pub yscl: f64,
    pub xres: f64,
}

impl WindowVals {
    pub fn standard() -> Self {
        WindowVals { xmin: -10.0, xmax: 10.0, xscl: 1.0, ymin: -10.0, ymax: 10.0, yscl: 1.0, xres: 1.0 }
    }

    pub fn dx(&self) -> f64 {
        (self.xmax - self.xmin) / (GRAPH_W as f64 - 1.0)
    }

    pub fn dy(&self) -> f64 {
        (self.ymax - self.ymin) / (GRAPH_H as f64 - 1.0)
    }

    pub fn px(&self, x: f64) -> f64 {
        (x - self.xmin) / self.dx()
    }

    pub fn py(&self, y: f64) -> f64 {
        (self.ymax - y) / self.dy()
    }

    fn center(&self) -> (f64, f64) {
        ((self.xmin + self.xmax) / 2.0, (self.ymin + self.ymax) / 2.0)
    }

    fn scale_about(&mut self, cx: f64, cy: f64, f: f64) {
        let hw = (self.xmax - self.xmin) / 2.0 * f;
        let hh = (self.ymax - self.ymin) / 2.0 * f;
        self.xmin = cx - hw;
        self.xmax = cx + hw;
        self.ymin = cy - hh;
        self.ymax = cy + hh;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Value,
    Zero,
    Min,
    Max,
    Intersect,
    Deriv,
    Integral,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub kind: TaskKind,
    pub stage: u8,
    pub left: f64,
    pub right: f64,
    pub f1: usize,
}

#[derive(Debug, Clone)]
pub struct GResult {
    pub title: Option<&'static str>,
    pub line: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GMode {
    None,
    Free { px: i32, py: i32 },
    Trace { func: usize, x: f64 },
}

#[derive(Debug, Clone)]
pub struct GraphState {
    pub mode: GMode,
    pub task: Option<Task>,
    pub input: Option<LineEditor>,
    pub result: Option<GResult>,
    /// Shading from the last ∫f(x)dx, kept until the graph is redrawn.
    pub shade: Option<(usize, f64, f64)>,
}

impl Default for GraphState {
    fn default() -> Self {
        GraphState { mode: GMode::None, task: None, input: None, result: None, shade: None }
    }
}

impl GraphState {
    pub fn reset_mode(&mut self) {
        self.mode = GMode::None;
        self.task = None;
        self.input = None;
        self.result = None;
        self.shade = None;
    }
}

fn short(x: f64) -> String {
    crate::engine::format::fit_width(x, 11)
}

fn xy_row(x: f64, y: Option<f64>) -> Row {
    let xs = format!("X={}", short(x));
    let ys = match y {
        Some(y) => format!("Y={}", short(y)),
        None => "Y=".to_string(),
    };
    let pad = 16usize.saturating_sub(xs.chars().count());
    plain(format!("{xs}{}{ys}", " ".repeat(pad)))
}

fn find_zero(f: &mut dyn FnMut(f64) -> Option<f64>, a: f64, b: f64, guess: f64) -> Option<f64> {
    let n = 400;
    let h = (b - a) / n as f64;
    let mut best: Option<(f64, f64, f64)> = None; // (dist, lo, hi)
    let mut prev: Option<(f64, f64)> = None;
    for k in 0..=n {
        let x = a + k as f64 * h;
        let y = f(x);
        if let Some(y) = y {
            if y == 0.0 {
                let d = (x - guess).abs();
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, x, x));
                }
            }
            if let Some((px, py)) = prev {
                if py.signum() != y.signum() && py != 0.0 && y != 0.0 {
                    let d = ((px + x) / 2.0 - guess).abs();
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, px, x));
                    }
                }
            }
            prev = Some((x, y));
        } else {
            prev = None;
        }
    }
    let (_, mut lo, mut hi) = best?;
    if lo == hi {
        return Some(lo);
    }
    let mut flo = f(lo)?;
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        let fm = f(mid)?;
        if fm == 0.0 {
            return Some(mid);
        }
        if fm.signum() == flo.signum() {
            lo = mid;
            flo = fm;
        } else {
            hi = mid;
        }
    }
    // Reject asymptotes (sign change through a pole).
    let x = (lo + hi) / 2.0;
    let y = f(x)?;
    if y.abs() > 1e-6 * (1.0 + flo.abs()) && y.abs() > 1e-3 {
        return None;
    }
    Some(x)
}

fn find_extremum(f: &mut dyn FnMut(f64) -> Option<f64>, a: f64, b: f64, max: bool) -> Option<f64> {
    let sign = if max { -1.0 } else { 1.0 };
    let n = 400;
    let h = (b - a) / n as f64;
    let mut best: Option<(f64, f64)> = None;
    for k in 0..=n {
        let x = a + k as f64 * h;
        if let Some(y) = f(x) {
            let v = sign * y;
            if best.is_none_or(|b| v < b.1) {
                best = Some((x, v));
            }
        }
    }
    let (bx, _) = best?;
    let (mut lo, mut hi) = ((bx - h).max(a), (bx + h).min(b));
    let g = (5f64.sqrt() - 1.0) / 2.0;
    for _ in 0..200 {
        let x1 = hi - g * (hi - lo);
        let x2 = lo + g * (hi - lo);
        let (f1, f2) = (f(x1).map(|y| sign * y), f(x2).map(|y| sign * y));
        match (f1, f2) {
            (Some(f1), Some(f2)) if f1 < f2 => hi = x2,
            (Some(_), Some(_)) => lo = x1,
            _ => break,
        }
    }
    Some((lo + hi) / 2.0)
}

pub fn integrate(f: &mut dyn FnMut(f64) -> Option<f64>, a: f64, b: f64) -> Option<f64> {
    const XS: [f64; 5] = [0.0, -0.5384693101056831, 0.5384693101056831, -0.906179845938664, 0.906179845938664];
    const WS: [f64; 5] = [
        0.5688888888888889, 0.47862867049936647, 0.47862867049936647, 0.23692688505618908, 0.23692688505618908,
    ];
    let n = 128;
    let h = (b - a) / n as f64;
    let mut total = 0.0;
    for k in 0..n {
        let mid = a + (k as f64 + 0.5) * h;
        for j in 0..5 {
            total += WS[j] * f(mid + XS[j] * h / 2.0)?;
        }
    }
    Some(total * h / 2.0)
}

fn eval_y(env: &mut Env, i: usize, x: f64) -> Option<f64> {
    env.eval_y(i, x).ok().filter(|y| y.is_finite())
}

impl Calc {
    /// Switch to the graph screen, reporting Y= syntax errors first.
    pub fn show_graph(&mut self) {
        for i in 0..10 {
            if self.yeq.enabled[i] && !self.yeq.funcs[i].is_empty() {
                if let Err(e) = parse_expr(&self.yeq.funcs[i].toks) {
                    self.show_error(e, ErrFrom::YEq(i));
                    return;
                }
            }
        }
        self.screen = Screen::Graph;
    }

    pub(super) fn start_trace(&mut self) {
        let funcs = self.active_funcs();
        let (cx, _) = self.window.center();
        let x = self.snap_x(cx);
        match funcs.first() {
            Some(&f) => self.graph.mode = GMode::Trace { func: f, x },
            None => {
                self.graph.mode = GMode::Free { px: (GRAPH_W / 2.0) as i32, py: (GRAPH_H / 2.0) as i32 };
            }
        }
    }

    /// Snap an x-value to the nearest pixel column.
    fn snap_x(&self, x: f64) -> f64 {
        let w = &self.window;
        let k = ((x - w.xmin) / w.dx()).round();
        crate::engine::eval::clean(w.xmin + k * w.dx())
    }

    pub(super) fn zoom(&mut self, c: Cmd) {
        let prev = self.window.clone();
        let w = &mut self.window;
        let degree = self.env.degree;
        let focus = match self.graph.mode {
            GMode::Trace { x, func } => {
                let y = eval_y(&mut self.env, func, x).unwrap_or(w.center().1);
                Some((x, y))
            }
            GMode::Free { px, py } => Some((w.xmin + px as f64 * w.dx(), w.ymax - py as f64 * w.dy())),
            GMode::None => None,
        };
        match c {
            Cmd::ZStandard => *w = WindowVals::standard(),
            Cmd::ZDecimal => {
                *w = WindowVals { xmin: -15.9, xmax: 16.0, xscl: 1.0, ymin: -10.4, ymax: 10.5, yscl: 1.0, xres: w.xres };
            }
            Cmd::ZSquare => {
                let (cx, _) = w.center();
                let half = w.dy() * (GRAPH_W as f64 - 1.0) / 2.0;
                w.xmin = cx - half;
                w.xmax = cx + half;
            }
            Cmd::ZTrig => {
                let (x, s) = if degree { (352.5, 90.0) } else { (47.0 * std::f64::consts::PI / 24.0, std::f64::consts::FRAC_PI_2) };
                *w = WindowVals { xmin: -x, xmax: x, xscl: s, ymin: -4.0, ymax: 4.0, yscl: 1.0, xres: w.xres };
            }
            Cmd::ZInteger => {
                let (cx, cy) = focus.unwrap_or(w.center());
                let (cx, cy) = (cx.round(), cy.round());
                *w = WindowVals {
                    xmin: cx - 159.0,
                    xmax: cx + 160.0,
                    xscl: 10.0,
                    ymin: cy - 104.0,
                    ymax: cy + 105.0,
                    yscl: 10.0,
                    xres: w.xres,
                };
            }
            Cmd::ZoomIn | Cmd::ZoomOut => {
                let (cx, cy) = focus.unwrap_or(w.center());
                w.scale_about(cx, cy, if c == Cmd::ZoomIn { 0.25 } else { 4.0 });
            }
            Cmd::ZoomFit => {
                let (xmin, dx) = (w.xmin, w.dx());
                let mut lo = f64::INFINITY;
                let mut hi = f64::NEG_INFINITY;
                for f in self.active_funcs() {
                    for k in 0..GRAPH_W as usize {
                        if let Some(y) = eval_y(&mut self.env, f, xmin + k as f64 * dx) {
                            lo = lo.min(y);
                            hi = hi.max(y);
                        }
                    }
                }
                let w = &mut self.window;
                if lo.is_finite() && hi.is_finite() {
                    if hi - lo < 1e-9 {
                        lo -= 1.0;
                        hi += 1.0;
                    }
                    w.ymin = lo;
                    w.ymax = hi;
                }
            }
            Cmd::ZPrevious => {
                if let Some(p) = self.zoom_prev.take() {
                    self.window = p;
                }
            }
            Cmd::ZoomRcl => {
                if let Some(s) = self.zoom_saved.clone() {
                    self.window = s;
                }
            }
            _ => {}
        }
        if c != Cmd::ZPrevious {
            self.zoom_prev = Some(prev);
        }
        self.graph.reset_mode();
    }

    pub(super) fn start_calc_task(&mut self, c: Cmd) {
        self.show_graph();
        if !matches!(self.screen, Screen::Graph) {
            return;
        }
        let funcs = self.active_funcs();
        if funcs.is_empty() {
            self.show_error(CalcError::Invalid, ErrFrom::None);
            return;
        }
        let kind = match c {
            Cmd::CalcValue => TaskKind::Value,
            Cmd::CalcZero => TaskKind::Zero,
            Cmd::CalcMin => TaskKind::Min,
            Cmd::CalcMax => TaskKind::Max,
            Cmd::CalcIntersect => TaskKind::Intersect,
            Cmd::CalcDeriv => TaskKind::Deriv,
            _ => TaskKind::Integral,
        };
        self.graph.reset_mode();
        self.start_trace();
        self.graph.task = Some(Task { kind, stage: 0, left: 0.0, right: 0.0, f1: funcs[0] });
        if kind == TaskKind::Value {
            self.graph.input = Some(LineEditor::default());
        }
    }

    fn trace_y(&mut self) -> Option<(usize, f64, Option<f64>)> {
        if let GMode::Trace { func, x } = self.graph.mode {
            Some((func, x, eval_y(&mut self.env, func, x)))
        } else {
            None
        }
    }

    fn set_trace_x(&mut self, x: f64) {
        if let GMode::Trace { func, .. } = self.graph.mode {
            self.graph.mode = GMode::Trace { func, x };
            let w = &mut self.window;
            if x < w.xmin || x > w.xmax {
                let shift = x - (w.xmin + w.xmax) / 2.0;
                w.xmin += shift;
                w.xmax += shift;
            }
        }
    }

    /// A character typed from the keyboard while on the graph screen.
    pub(super) fn graph_type(&mut self, tok: Tok) {
        if matches!(self.graph.mode, GMode::Trace { .. }) {
            let numeric = tok.len() == 1 && tok.as_bytes()[0].is_ascii_digit() || matches!(tok, "." | "⁻" | "-" | "π" | "ᴇ");
            if self.graph.input.is_none() && numeric {
                self.graph.input = Some(LineEditor::default());
            }
            if let Some(ed) = self.graph.input.as_mut() {
                ed.put(if tok == "-" { "⁻" } else { tok });
            }
        }
    }

    fn commit_input(&mut self) {
        let Some(ed) = self.graph.input.take() else { return };
        if ed.is_empty() {
            return;
        }
        match parse_expr(&ed.toks).and_then(|e| self.env.eval_num(&e)) {
            Ok(x) => {
                self.set_trace_x(x);
                if let Some(t) = &self.graph.task {
                    if t.kind == TaskKind::Value {
                        self.graph.task = None;
                    } else {
                        self.advance_task();
                    }
                }
            }
            Err(e) => self.show_error(e, ErrFrom::None),
        }
    }

    fn advance_task(&mut self) {
        let Some(mut task) = self.graph.task.clone() else { return };
        let Some((func, x, _)) = self.trace_y() else { return };
        let fmt = NumFormat::default();
        match task.kind {
            TaskKind::Value => {
                self.graph.task = None;
                return;
            }
            TaskKind::Deriv => {
                let h = 1e-3;
                let d = match (eval_y(&mut self.env, func, x + h), eval_y(&mut self.env, func, x - h)) {
                    (Some(a), Some(b)) => Some((a - b) / (2.0 * h)),
                    _ => None,
                };
                self.graph.task = None;
                self.graph.result = Some(GResult {
                    title: None,
                    line: match d {
                        Some(d) => format!("dy/dx={}", format_num(crate::engine::eval::clean(d), fmt)),
                        None => "dy/dx=undefined".into(),
                    } });
                return;
            }
            TaskKind::Intersect => {
                match task.stage {
                    0 => {
                        task.f1 = func;
                        task.stage = 1;
                        // Move to the next curve for the second selection.
                        let funcs = self.active_funcs();
                        if let Some(p) = funcs.iter().position(|&f| f == func) {
                            let next = funcs[(p + 1) % funcs.len()];
                            self.graph.mode = GMode::Trace { func: next, x };
                        }
                    }
                    1 => {
                        task.left = func as f64;
                        task.stage = 2;
                    }
                    _ => {
                        let (f1, f2) = (task.f1, task.left as usize);
                        let (a, b) = (self.window.xmin, self.window.xmax);
                        let env = &mut self.env;
                        let mut g = |t: f64| Some(eval_y(env, f1, t)? - eval_y(env, f2, t)?);
                        // Search near the guess first, then the whole window.
                        let span = (b - a) / 4.0;
                        let root = find_zero(&mut g, (x - span).max(a), (x + span).min(b), x).or_else(|| find_zero(&mut g, a, b, x));
                        self.graph.task = None;
                        match root {
                            Some(r) => {
                                let r = crate::engine::eval::clean(r);
                                self.graph.mode = GMode::Trace { func: f1, x: r };
                                let y = eval_y(&mut self.env, f1, r);
                                self.graph.result = Some(GResult {
                                    title: Some("Intersection"),
                                    line: row_text(&xy_row(r, y)) });
                            }
                            None => self.show_error(CalcError::NoSignChange, ErrFrom::None),
                        }
                        return;
                    }
                }
                self.graph.task = Some(task);
                return;
            }
            _ => {}
        }
        match task.stage {
            0 => {
                task.left = x;
                task.stage = 1;
                self.graph.task = Some(task);
            }
            1 if task.kind == TaskKind::Integral => {
                let (a, b) = (task.left, x);
                let env = &mut self.env;
                let (lo, hi) = (a.min(b), a.max(b));
                let scale = integrate(&mut |t| eval_y(env, func, t).map(f64::abs), lo, hi).unwrap_or(0.0);
                let r = integrate(&mut |t| eval_y(env, func, t), lo, hi)
                    .map(|v| if v.abs() <= 1e-11 * scale { 0.0 } else { v })
                    .map(|v| if b < a { -v } else { v });
                self.graph.task = None;
                self.graph.shade = Some((func, a.min(b), a.max(b)));
                self.graph.result = Some(GResult {
                    title: None,
                    line: match r {
                        Some(v) => format!("∫f(x)dx={}", format_num(crate::engine::eval::clean(v), fmt)),
                        None => "∫f(x)dx=undefined".into(),
                    } });
            }
            1 => {
                if x <= task.left {
                    self.show_error(CalcError::Invalid, ErrFrom::None);
                    return;
                }
                task.right = x;
                task.stage = 2;
                self.graph.task = Some(task);
            }
            _ => {
                let (a, b) = (task.left, task.right);
                let env = &mut self.env;
                let mut f = |t: f64| eval_y(env, func, t);
                let found = match task.kind {
                    TaskKind::Zero => find_zero(&mut f, a, b, x),
                    TaskKind::Min => find_extremum(&mut f, a, b, false),
                    _ => find_extremum(&mut f, a, b, true),
                };
                self.graph.task = None;
                match found {
                    Some(r) => {
                        let r = crate::engine::eval::clean(r);
                        self.graph.mode = GMode::Trace { func, x: r };
                        let mut y = eval_y(&mut self.env, func, r);
                        if task.kind == TaskKind::Zero {
                            y = y.map(|v| if v.abs() < 1e-10 { 0.0 } else { v });
                        }
                        let title = match task.kind {
                            TaskKind::Zero => "Zero",
                            TaskKind::Min => "Minimum",
                            _ => "Maximum",
                        };
                        self.graph.result = Some(GResult { title: Some(title), line: row_text(&xy_row(r, y)) });
                    }
                    None => self.show_error(CalcError::NoSignChange, ErrFrom::None),
                }
            }
        }
    }

    pub(super) fn graph_key(&mut self, key: Key, m: Mods) {
        // Typing a value while tracing (or for CALC value).
        if let Some(ed) = self.graph.input.as_mut() {
            match key {
                Key::Enter => self.commit_input(),
                Key::Clear => {
                    if ed.is_empty() {
                        self.graph.input = None;
                        if self.graph.task.as_ref().is_some_and(|t| t.kind == TaskKind::Value) {
                            self.graph.task = None;
                        }
                    } else {
                        ed.clear();
                    }
                }
                Key::Del => ed.backspace(),
                Key::Left => ed.left(),
                Key::Right => ed.right(),
                _ => {
                    if let Some(t) = Self::key_token(key, m) {
                        ed.put(t);
                    }
                }
            }
            return;
        }
        self.graph.result = None;
        let dx = self.window.dx();
        match self.graph.mode {
            GMode::Trace { func, x } => match key {
                Key::Left => self.set_trace_x(crate::engine::eval::clean(x - dx)),
                Key::Right => self.set_trace_x(crate::engine::eval::clean(x + dx)),
                Key::Up | Key::Down => {
                    let funcs = self.active_funcs();
                    if let Some(p) = funcs.iter().position(|&f| f == func) {
                        let n = funcs.len();
                        let next = if key == Key::Up { funcs[(p + n - 1) % n] } else { funcs[(p + 1) % n] };
                        self.graph.mode = GMode::Trace { func: next, x };
                    }
                }
                Key::Enter => {
                    if self.graph.task.is_some() {
                        self.advance_task();
                    } else if let Some(y) = eval_y(&mut self.env, func, x) {
                        // Quick zoom: re-center the window on the cursor.
                        let w = &mut self.window;
                        let (cx, cy) = w.center();
                        w.xmin += x - cx;
                        w.xmax += x - cx;
                        w.ymin += y - cy;
                        w.ymax += y - cy;
                    }
                }
                Key::Clear => {
                    self.graph.mode = GMode::None;
                    self.graph.task = None;
                }
                _ => {
                    if let Some(t) = Self::key_token(key, m) {
                        self.graph_type(t);
                    }
                }
            },
            GMode::Free { px, py } => {
                let (nx, ny) = match key {
                    Key::Left => (px - 1, py),
                    Key::Right => (px + 1, py),
                    Key::Up => (px, py - 1),
                    Key::Down => (px, py + 1),
                    Key::Clear => {
                        self.graph.mode = GMode::None;
                        return;
                    }
                    _ => return,
                };
                self.graph.mode = GMode::Free {
                    px: nx.clamp(0, GRAPH_W as i32 - 1),
                    py: ny.clamp(0, GRAPH_H as i32 - 1),
                };
            }
            GMode::None => match key {
                Key::Left | Key::Right | Key::Up | Key::Down => {
                    self.graph.mode = GMode::Free { px: (GRAPH_W / 2.0) as i32, py: (GRAPH_H / 2.0) as i32 };
                }
                Key::Clear => self.goto_home(),
                _ => {}
            },
        }
    }

    pub(super) fn graph_model(&mut self) -> GraphModel {
        let w = self.window.clone();
        let dx = w.dx();
        let mut curves = Vec::new();
        let step = w.xres.max(1.0) as usize;
        for f in self.active_funcs() {
            let mut segments: Vec<Vec<(f32, f32)>> = Vec::new();
            let mut cur: Vec<(f32, f32)> = Vec::new();
            let mut prev_py: Option<f64> = None;
            let mut k = 0;
            while k < GRAPH_W as usize {
                let x = w.xmin + k as f64 * dx;
                match eval_y(&mut self.env, f, x) {
                    Some(y) => {
                        let py = w.py(y);
                        // Break the curve across vertical asymptotes.
                        if let Some(pp) = prev_py {
                            let off_opposite = (pp < 0.0 && py > GRAPH_H as f64) || (pp > GRAPH_H as f64 && py < 0.0);
                            if off_opposite && (py - pp).abs() > GRAPH_H as f64 {
                                if !cur.is_empty() {
                                    segments.push(std::mem::take(&mut cur));
                                }
                            }
                        }
                        let clamped = py.clamp(-2.0 * GRAPH_H as f64, 3.0 * GRAPH_H as f64);
                        cur.push((k as f32, clamped as f32));
                        prev_py = Some(py);
                    }
                    None => {
                        if !cur.is_empty() {
                            segments.push(std::mem::take(&mut cur));
                        }
                        prev_py = None;
                    }
                }
                k += step;
            }
            if !cur.is_empty() {
                segments.push(cur);
            }
            curves.push(Curve { color: Y_COLORS[f], thick: self.settings.thick, dots: self.settings.dots, segments });
        }

        let (x_axis, y_axis, x_ticks, y_ticks) = if self.settings.axes_on || self.settings.grid != Grid::Off {
            let xa = (w.ymin <= 0.0 && w.ymax >= 0.0).then(|| w.py(0.0) as f32);
            let ya = (w.xmin <= 0.0 && w.xmax >= 0.0).then(|| w.px(0.0) as f32);
            let ticks = |min: f64, max: f64, scl: f64, map: &dyn Fn(f64) -> f64| -> Vec<f32> {
                if scl <= 0.0 || (max - min) / scl > 160.0 {
                    return vec![];
                }
                let start = (min / scl).ceil() as i64;
                let end = (max / scl).floor() as i64;
                (start..=end).filter(|&i| i != 0).map(|i| map(i as f64 * scl) as f32).collect()
            };
            let xt = ticks(w.xmin, w.xmax, w.xscl, &|x| w.px(x));
            let yt = ticks(w.ymin, w.ymax, w.yscl, &|y| w.py(y));
            (xa, ya, xt, yt)
        } else {
            (None, None, vec![], vec![])
        };

        let mut cursor = None;
        let mut top = None;
        let mut bottom: Vec<Row> = Vec::new();
        let mut marks = Vec::new();
        match self.graph.mode {
            GMode::Free { px, py } => {
                cursor = Some((px as f32, py as f32, CursorStyle::Cross, color::BLACK));
                if self.settings.coord_on {
                    let x = w.xmin + px as f64 * dx;
                    let y = w.ymax - py as f64 * w.dy();
                    bottom.push(xy_row(crate::engine::eval::clean(x), Some(crate::engine::eval::clean(y))));
                }
            }
            GMode::Trace { func, x } => {
                let y = eval_y(&mut self.env, func, x);
                if let Some(y) = y {
                    cursor = Some((w.px(x) as f32, w.py(y) as f32, CursorStyle::Trace, Y_COLORS[func]));
                }
                if self.settings.expr_on {
                    let text = format!("{}={}", YVARS[func], self.yeq.funcs[func].text());
                    let text: String = text.chars().take(COLS).collect();
                    top = Some(vec![Span::new(text).fg(Y_COLORS[func])]);
                }
                if let Some(task) = &self.graph.task {
                    let prompt = match (task.kind, task.stage) {
                        (TaskKind::Value, _) => None,
                        (TaskKind::Intersect, 0) => Some("First curve?"),
                        (TaskKind::Intersect, 1) => Some("Second curve?"),
                        (TaskKind::Integral, 0) => Some("Lower Limit?"),
                        (TaskKind::Integral, _) => Some("Upper Limit?"),
                        (TaskKind::Deriv, _) => None,
                        (_, 0) => Some("Left Bound?"),
                        (_, 1) => Some("Right Bound?"),
                        _ => Some("Guess?"),
                    };
                    if let Some(p) = prompt {
                        bottom.push(plain(p));
                    }
                    if matches!(task.kind, TaskKind::Zero | TaskKind::Min | TaskKind::Max | TaskKind::Integral) && task.stage >= 1 {
                        marks.push((w.px(task.left) as f32, 0.0));
                        if task.stage >= 2 {
                            marks.push((w.px(task.right) as f32, 0.0));
                        }
                    }
                }
                if let Some(ed) = &self.graph.input {
                    let cursor = Some((ed.char_pos(), self.cursor_kind(false), self.blink));
                    bottom.extend(wrap_with_cursor(&[Span::new("X=")], &ed.text(), cursor, color::BLACK));
                } else if let Some(r) = &self.graph.result {
                    if let Some(t) = r.title {
                        bottom.push(plain(t));
                    }
                    bottom.push(plain(r.line.clone()));
                } else {
                    bottom.push(xy_row(x, y));
                }
            }
            GMode::None => {}
        }
        if let (Some(r), true) = (&self.graph.result, matches!(self.graph.mode, GMode::None)) {
            bottom.push(plain(r.line.clone()));
        }

        let shade = self.graph.shade.map(|(f, a, b)| {
            let mut pts = Vec::new();
            let (pa, pb) = (w.px(a).max(0.0), w.px(b).min(GRAPH_W as f64 - 1.0));
            let base = w.py(0.0).clamp(0.0, GRAPH_H as f64);
            pts.push((pa as f32, base as f32));
            let mut p = pa;
            while p <= pb {
                let x = w.xmin + p * dx;
                let y = eval_y(&mut self.env, f, x).map(|y| w.py(y).clamp(0.0, GRAPH_H as f64)).unwrap_or(base);
                pts.push((p as f32, y as f32));
                p += 1.0;
            }
            pts.push((pb as f32, base as f32));
            let c = Y_COLORS[f];
            (c, pts)
        });

        GraphModel {
            curves,
            x_axis: if self.settings.axes_on { x_axis } else { None },
            y_axis: if self.settings.axes_on { y_axis } else { None },
            x_ticks,
            y_ticks,
            grid: self.settings.grid,
            cursor,
            marks,
            shade,
            top,
            bottom,
        }
    }
}

fn row_text(r: &Row) -> String {
    r.iter().map(|s| s.text.as_str()).collect()
}
