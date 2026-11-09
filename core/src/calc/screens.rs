//! Settings screens: MODE and FORMAT option grids, plus the WINDOW and
//! TBLSET value editors.

use super::editor::{wrap_with_cursor, LineEditor};
use super::view::{color, plain, Grid, Row, Span};
use super::{Answers, Calc, ErrFrom, Mods, Screen};
use crate::engine::format::{format_num, Notation, NumFormat};
use crate::engine::{parse_expr, string_to_tokens, CalcError, Tok};
use crate::keys::Key;

#[derive(Debug, Clone, Default)]
pub struct OptScreen {
    pub row: usize,
    pub col: usize,
}

const MODE_ROWS: &[(&str, &[&str])] = &[
    ("", &["NORMAL", "SCI", "ENG"]),
    ("", &["FLOAT", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]),
    ("", &["RADIAN", "DEGREE"]),
    ("", &["FUNCTION", "PARAMETRIC", "POLAR", "SEQ"]),
    ("", &["THICK", "DOT-THICK", "THIN", "DOT-THIN"]),
    ("", &["SEQUENTIAL", "SIMUL"]),
    ("", &["REAL", "a+bi", "re^θi"]),
    ("", &["FULL", "HORIZONTAL", "GRAPH-TABLE"]),
    ("ANSWERS: ", &["AUTO", "DEC", "FRAC"]),
    ("STAT DIAGNOSTICS: ", &["OFF", "ON"]),
];

const FORMAT_ROWS: &[(&str, &[&str])] = &[
    ("", &["RectGC", "PolarGC"]),
    ("", &["CoordOn", "CoordOff"]),
    ("", &["GridOff", "GridDot", "GridLine"]),
    ("", &["AxesOn", "AxesOff"]),
    ("", &["LabelOff", "LabelOn"]),
    ("", &["ExprOn", "ExprOff"]),
];

impl Calc {
    fn is_mode(&self) -> bool {
        matches!(self.screen, Screen::Mode(_))
    }

    fn opt_rows_def(&self) -> &'static [(&'static str, &'static [&'static str])] {
        if self.is_mode() {
            MODE_ROWS
        } else {
            FORMAT_ROWS
        }
    }

    fn opt_disabled(&self, row: usize, opt: usize) -> bool {
        if self.is_mode() {
            matches!((row, opt), (3, 1..) | (6, 1..) | (7, 1..))
        } else {
            matches!((row, opt), (0, 1))
        }
    }

    fn opt_get(&self, row: usize) -> usize {
        let s = &self.settings;
        if self.is_mode() {
            match row {
                0 => s.fmt.notation as usize,
                1 => s.fmt.fix.map(|f| f as usize + 1).unwrap_or(0),
                2 => self.env.degree as usize,
                4 => match (s.thick, s.dots) {
                    (true, false) => 0,
                    (true, true) => 1,
                    (false, false) => 2,
                    (false, true) => 3,
                },
                5 => s.simul as usize,
                8 => s.answers as usize,
                9 => s.stat_diag as usize,
                _ => 0,
            }
        } else {
            match row {
                1 => !s.coord_on as usize,
                2 => s.grid as usize,
                3 => !s.axes_on as usize,
                4 => s.labels_on as usize,
                5 => !s.expr_on as usize,
                _ => 0,
            }
        }
    }

    fn opt_set(&mut self, row: usize, opt: usize) {
        if self.opt_disabled(row, opt) {
            return;
        }
        let mode = self.is_mode();
        let s = &mut self.settings;
        if mode {
            match row {
                0 => s.fmt.notation = [Notation::Normal, Notation::Sci, Notation::Eng][opt],
                1 => s.fmt.fix = if opt == 0 { None } else { Some(opt as u8 - 1) },
                2 => self.env.degree = opt == 1,
                4 => {
                    s.thick = opt < 2;
                    s.dots = opt % 2 == 1;
                }
                5 => s.simul = opt == 1,
                8 => s.answers = [Answers::Auto, Answers::Dec, Answers::Frac][opt],
                9 => s.stat_diag = opt == 1,
                _ => {}
            }
        } else {
            match row {
                1 => s.coord_on = opt == 0,
                2 => s.grid = [Grid::Off, Grid::Dot, Grid::Line][opt],
                3 => s.axes_on = opt == 0,
                4 => s.labels_on = opt == 1,
                5 => s.expr_on = opt == 0,
                _ => {}
            }
        }
    }

    pub(super) fn opt_key(&mut self, key: Key, _m: Mods) {
        let rows = self.opt_rows_def();
        let (mut row, mut col) = match &self.screen {
            Screen::Mode(o) | Screen::Format(o) => (o.row, o.col),
            _ => return,
        };
        match key {
            Key::Up => {
                row = if row == 0 { rows.len() - 1 } else { row - 1 };
                col = col.min(rows[row].1.len() - 1);
            }
            Key::Down => {
                row = (row + 1) % rows.len();
                col = col.min(rows[row].1.len() - 1);
            }
            Key::Left => col = if col == 0 { rows[row].1.len() - 1 } else { col - 1 },
            Key::Right => col = (col + 1) % rows[row].1.len(),
            Key::Enter => self.opt_set(row, col),
            Key::Clear => {
                self.goto_home();
                return;
            }
            _ => {}
        }
        if let Screen::Mode(o) | Screen::Format(o) = &mut self.screen {
            o.row = row;
            o.col = col;
        }
    }

    fn opt_rows(&self, o: &OptScreen) -> Vec<Row> {
        let mut out = Vec::new();
        for (r, (label, opts)) in self.opt_rows_def().iter().enumerate() {
            let mut row: Row = Vec::new();
            if !label.is_empty() {
                row.push(Span::new(*label));
            }
            let sel = self.opt_get(r);
            for (i, opt) in opts.iter().enumerate() {
                if i > 0 {
                    row.push(Span::new(" "));
                }
                let span = if r == o.row && i == o.col && self.blink {
                    Span::new(*opt).fg(color::WHITE).bg(color::BLACK)
                } else if i == sel {
                    Span::inverse(*opt)
                } else if self.opt_disabled(r, i) {
                    Span::new(*opt).fg(color::LTGRAY)
                } else {
                    Span::new(*opt)
                };
                row.push(span);
            }
            out.push(row);
        }
        out
    }

    pub(super) fn mode_rows(&self, o: &OptScreen) -> Vec<Row> {
        self.opt_rows(o)
    }

    pub(super) fn format_rows(&self, o: &OptScreen) -> Vec<Row> {
        self.opt_rows(o)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Window,
    TblSet,
}

#[derive(Debug, Clone)]
pub struct FieldScreen {
    pub kind: FieldKind,
    pub cur: usize,
    pub ed: LineEditor,
    /// The field was just entered: typing replaces the whole value.
    pub fresh: bool,
}

fn field_names(kind: FieldKind) -> &'static [&'static str] {
    match kind {
        FieldKind::Window => &["Xmin", "Xmax", "Xscl", "Ymin", "Ymax", "Yscl", "Xres"],
        FieldKind::TblSet => &["TblStart", "ΔTbl"],
    }
}

fn value_toks(x: f64) -> Vec<Tok> {
    string_to_tokens(&format_num(x, NumFormat::default()))
}

impl FieldScreen {
    pub fn new(kind: FieldKind, calc: &Calc) -> Self {
        let v = calc.field_get(kind, 0);
        FieldScreen { kind, cur: 0, ed: LineEditor::from_toks(value_toks(v)), fresh: true }
    }
}

impl Calc {
    pub(super) fn field_get(&self, kind: FieldKind, i: usize) -> f64 {
        let w = &self.window;
        match kind {
            FieldKind::Window => [w.xmin, w.xmax, w.xscl, w.ymin, w.ymax, w.yscl, w.xres][i],
            FieldKind::TblSet => [self.table.start, self.table.step][i],
        }
    }

    fn field_set(&mut self, kind: FieldKind, i: usize, v: f64) -> Result<(), CalcError> {
        match kind {
            FieldKind::Window => {
                let mut w = self.window.clone();
                match i {
                    0 => w.xmin = v,
                    1 => w.xmax = v,
                    2 => w.xscl = v,
                    3 => w.ymin = v,
                    4 => w.ymax = v,
                    5 => w.yscl = v,
                    _ => {
                        if v.fract() != 0.0 || !(1.0..=8.0).contains(&v) {
                            return Err(CalcError::Domain);
                        }
                        w.xres = v
                    }
                }
                if (i <= 1 && w.xmin >= w.xmax) || ((3..=4).contains(&i) && w.ymin >= w.ymax) {
                    return Err(CalcError::WindowRange);
                }
                self.window = w;
            }
            FieldKind::TblSet => {
                if i == 0 {
                    self.table.start = v;
                } else {
                    if v == 0.0 {
                        return Err(CalcError::Domain);
                    }
                    self.table.step = v;
                }
                self.table.top = 0;
            }
        }
        Ok(())
    }

    /// Commit the current field. Returns false if an error screen was shown.
    pub(super) fn field_commit(&mut self, show_errors: bool) -> bool {
        let Screen::Fields(f) = &self.screen else { return true };
        let (kind, cur, toks) = (f.kind, f.cur, f.ed.toks.clone());
        if toks.is_empty() {
            return true;
        }
        let res = parse_expr(&toks).and_then(|e| self.env.eval_num(&e)).and_then(|v| self.field_set(kind, cur, v));
        match res {
            Ok(()) => true,
            Err(e) => {
                if show_errors {
                    self.show_error(e, ErrFrom::Field(kind, cur, toks));
                }
                false
            }
        }
    }

    fn field_move(&mut self, to: usize) {
        let Screen::Fields(f) = &self.screen else { return };
        let kind = f.kind;
        let v = self.field_get(kind, to);
        if let Screen::Fields(f) = &mut self.screen {
            f.cur = to;
            f.ed = LineEditor::from_toks(value_toks(v));
            f.fresh = true;
        }
    }

    pub(super) fn field_key(&mut self, key: Key, m: Mods) {
        let Screen::Fields(f) = &mut self.screen else { return };
        let n = field_names(f.kind).len();
        let cur = f.cur;
        match key {
            Key::Up | Key::Down | Key::Enter => {
                if !self.field_commit(true) {
                    return;
                }
                let to = if key == Key::Up {
                    if cur == 0 { 0 } else { cur - 1 }
                } else {
                    (cur + 1).min(n - 1)
                };
                self.field_move(to);
            }
            Key::Left => {
                f.fresh = false;
                f.ed.left();
            }
            Key::Right => {
                f.fresh = false;
                f.ed.right();
            }
            Key::Clear => {
                f.ed.clear();
                f.fresh = false;
            }
            Key::Del => {
                if m.second {
                    f.ed.insert = !f.ed.insert;
                } else {
                    f.fresh = false;
                    f.ed.del();
                }
            }
            _ => {
                if let Some(t) = Self::key_token(key, m) {
                    if t != "→" {
                        self.insert_tokens(&[t]);
                    }
                }
            }
        }
    }

    pub(super) fn field_rows(&self, f: &FieldScreen) -> Vec<Row> {
        let fmt = NumFormat::default();
        let title = match f.kind {
            FieldKind::Window => "WINDOW",
            FieldKind::TblSet => "TABLE SETUP",
        };
        let mut rows = vec![plain(title)];
        for (i, name) in field_names(f.kind).iter().enumerate() {
            let prefix = vec![Span::new(format!("{name}="))];
            if i == f.cur {
                let cursor = Some((f.ed.char_pos(), self.cursor_kind(f.ed.insert), self.blink));
                rows.extend(wrap_with_cursor(&prefix, &f.ed.text(), cursor, color::BLACK));
            } else {
                let v = format_num(self.field_get(f.kind, i), fmt);
                rows.extend(wrap_with_cursor(&prefix, &v, None, color::BLACK));
            }
        }
        match f.kind {
            FieldKind::Window => {
                let dx = self.window.dx();
                rows.push(vec![Span::new(format!("ΔX={}", format_num(dx, fmt))).fg(color::GRAY)]);
                rows.push(vec![Span::new(format!("TraceStep={}", format_num(dx, fmt))).fg(color::GRAY)]);
            }
            FieldKind::TblSet => {
                rows.push(vec![Span::new("Indpnt: "), Span::inverse("Auto"), Span::new(" Ask").fg(color::LTGRAY)]);
                rows.push(vec![Span::new("Depend: "), Span::inverse("Auto"), Span::new(" Ask").fg(color::LTGRAY)]);
            }
        }
        rows
    }
}
