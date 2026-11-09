//! The TABLE screen and the STAT list editor.

use super::editor::{wrap_with_cursor, LineEditor};
use super::view::*;
use super::{Calc, ErrFrom, Mods, Screen};
use crate::engine::format::format_num;
use crate::engine::{parse_expr, Tok, LISTS, YVARS};
use crate::keys::Key;

const DATA_ROWS: usize = ROWS - 2;
const XW: usize = 9;
const CW: usize = 10;

#[derive(Debug, Clone)]
pub struct TableState {
    pub start: f64,
    pub step: f64,
    pub top: i64,
    pub row: usize,
    pub col: usize,
    pub hscroll: usize,
}

impl Default for TableState {
    fn default() -> Self {
        TableState { start: 0.0, step: 1.0, top: 0, row: 0, col: 0, hscroll: 0 }
    }
}

/// Format a number to fit in `width` characters.
pub fn fit(x: f64, width: usize) -> String {
    crate::engine::format::fit_width(x, width)
}

fn pad(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w {
        s.chars().take(w).collect()
    } else {
        // Right-align numbers within the cell.
        format!("{}{}", " ".repeat(w - n), s)
    }
}

fn center(s: &str, w: usize) -> String {
    let n = s.chars().count().min(w);
    let left = (w - n) / 2;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(w - n - left))
}

#[derive(Debug, Clone, Default)]
pub struct ListEditor {
    pub col: usize,
    pub row: usize,
    pub scroll: usize,
    pub hscroll: usize,
    pub ed: LineEditor,
    pub editing: bool,
}

impl ListEditor {
    pub fn enter(&mut self) {
        self.editing = false;
        self.ed.clear();
    }
}

impl Calc {
    pub fn show_table(&mut self) {
        self.show_graph();
        if matches!(self.screen, Screen::Graph) {
            self.screen = Screen::Table;
        }
    }

    fn table_x(&self, r: i64) -> f64 {
        crate::engine::eval::clean(self.table.start + r as f64 * self.table.step)
    }

    pub(super) fn table_key(&mut self, key: Key, _m: Mods) {
        let ncols = self.active_funcs().len() + 1;
        let t = &mut self.table;
        match key {
            Key::Up => {
                if t.row > 0 {
                    t.row -= 1;
                } else {
                    t.top -= 1;
                }
            }
            Key::Down => {
                if t.row + 1 < DATA_ROWS {
                    t.row += 1;
                } else {
                    t.top += 1;
                }
            }
            Key::Left => t.col = t.col.saturating_sub(1),
            Key::Right => t.col = (t.col + 1).min(ncols - 1),
            Key::Clear => {
                self.goto_home();
                return;
            }
            _ => {}
        }
        let t = &mut self.table;
        if t.col >= 1 {
            if t.col - 1 < t.hscroll {
                t.hscroll = t.col - 1;
            } else if t.col - 1 >= t.hscroll + 2 {
                t.hscroll = t.col - 2;
            }
        }
    }

    pub(super) fn table_rows(&mut self) -> Vec<Row> {
        let funcs = self.active_funcs();
        let t = self.table.clone();
        let shown: Vec<usize> = funcs.iter().skip(t.hscroll).take(2).copied().collect();
        let mut rows = Vec::new();
        let mut header = vec![Span::new(center("X", XW))];
        for &f in &shown {
            header.push(Span::new("│").fg(color::GRAY));
            header.push(Span::new(center(YVARS[f], CW)).fg(Y_COLORS[f]));
        }
        rows.push(header);
        let mut detail = String::new();
        for r in 0..DATA_ROWS {
            let x = self.table_x(t.top + r as i64);
            let mut row: Row = Vec::new();
            let xs = pad(&fit(x, XW), XW);
            let sel_x = r == t.row && t.col == 0;
            row.push(if sel_x { Span::inverse(xs) } else { Span::new(xs) });
            if sel_x {
                detail = format!("X={}", format_num(x, self.fmt()));
            }
            for (ci, &f) in shown.iter().enumerate() {
                row.push(Span::new("│").fg(color::GRAY));
                let v = self.env.eval_y(f, x);
                let cell = match v {
                    Ok(y) => fit(y, CW),
                    Err(_) => "ERROR".to_string(),
                };
                let sel = r == t.row && t.col == t.hscroll + ci + 1;
                if sel {
                    detail = match v {
                        Ok(y) => format!("{}={}", YVARS[f], format_num(y, self.fmt())),
                        Err(_) => format!("{}=ERROR", YVARS[f]),
                    };
                }
                let cell = pad(&cell, CW);
                row.push(if sel { Span::inverse(cell) } else { Span::new(cell) });
            }
            rows.push(row);
        }
        rows.push(plain(detail));
        rows
    }

    // ----- STAT list editor -----

    pub(super) fn lists_insert(&mut self, toks: &[Tok]) {
        let le = &mut self.lists;
        if !le.editing {
            le.editing = true;
            le.ed.clear();
        }
        le.ed.put_all(toks);
    }

    pub(super) fn lists_commit(&mut self) {
        let le = &mut self.lists;
        if !le.editing {
            return;
        }
        le.editing = false;
        if le.ed.is_empty() {
            return;
        }
        let toks = le.ed.toks.clone();
        let (col, row) = (le.col, le.row);
        match parse_expr(&toks).and_then(|e| self.env.eval_num(&e)) {
            Ok(v) => {
                let v = crate::engine::eval::clean(v);
                let list = &mut self.env.lists[col];
                if row < list.len() {
                    list[row] = v;
                } else {
                    list.push(v);
                }
            }
            Err(e) => self.show_error(e, ErrFrom::None),
        }
    }

    pub(super) fn statedit_key(&mut self, key: Key, m: Mods) {
        match key {
            Key::Up | Key::Down | Key::Enter | Key::Left | Key::Right => {
                self.lists_commit();
                if !matches!(self.screen, Screen::StatEdit) {
                    return;
                }
                let le = &mut self.lists;
                let len = self.env.lists[le.col].len();
                match key {
                    Key::Up => le.row = le.row.saturating_sub(1),
                    Key::Down | Key::Enter => le.row = (le.row + 1).min(len),
                    Key::Left => le.col = le.col.saturating_sub(1),
                    _ => le.col = (le.col + 1).min(5),
                }
                le.row = le.row.min(self.env.lists[le.col].len());
                if le.col < le.hscroll {
                    le.hscroll = le.col;
                } else if le.col >= le.hscroll + 3 {
                    le.hscroll = le.col - 2;
                }
                if le.row < le.scroll {
                    le.scroll = le.row;
                } else if le.row >= le.scroll + DATA_ROWS {
                    le.scroll = le.row + 1 - DATA_ROWS;
                }
            }
            Key::Del => {
                let le = &mut self.lists;
                if le.editing {
                    le.ed.del();
                } else if le.row < self.env.lists[le.col].len() {
                    self.env.lists[le.col].remove(le.row);
                }
            }
            Key::Clear => {
                let le = &mut self.lists;
                if le.editing {
                    le.ed.clear();
                } else {
                    le.editing = true;
                    le.ed.clear();
                }
            }
            _ => {
                if let Some(t) = Self::key_token(key, m) {
                    if t != "→" {
                        self.lists_insert(&[t]);
                    }
                }
            }
        }
    }

    pub(super) fn statedit_rows(&self) -> Vec<Row> {
        let le = &self.lists;
        let mut rows = Vec::new();
        let cols: Vec<usize> = (le.hscroll..(le.hscroll + 3).min(6)).collect();
        let mut header: Row = Vec::new();
        for (i, &c) in cols.iter().enumerate() {
            if i > 0 {
                header.push(Span::new("│").fg(color::GRAY));
            }
            header.push(Span::new(center(LISTS[c], CW)).fg(color::NAVY));
        }
        rows.push(header);
        for r in 0..DATA_ROWS {
            let idx = le.scroll + r;
            let mut row: Row = Vec::new();
            for (i, &c) in cols.iter().enumerate() {
                if i > 0 {
                    row.push(Span::new("│").fg(color::GRAY));
                }
                let list = &self.env.lists[c];
                let text = if idx < list.len() {
                    pad(&fit(list[idx], CW), CW)
                } else if idx == list.len() {
                    pad("------", CW)
                } else {
                    " ".repeat(CW)
                };
                let sel = c == le.col && idx == le.row;
                row.push(if sel { Span::inverse(text) } else { Span::new(text) });
            }
            rows.push(row);
        }
        let name = LISTS[le.col];
        let prefix = vec![Span::new(format!("{name}({})=", le.row + 1))];
        if le.editing {
            let cursor = Some((le.ed.char_pos(), self.cursor_kind(le.ed.insert), self.blink));
            rows.extend(wrap_with_cursor(&prefix, &le.ed.text(), cursor, color::BLACK).into_iter().take(1));
        } else {
            let list = &self.env.lists[le.col];
            let v = list.get(le.row).map(|&v| format_num(v, self.fmt())).unwrap_or_default();
            rows.push(wrap_with_cursor(&prefix, &v, None, color::BLACK).remove(0));
        }
        rows
    }
}
