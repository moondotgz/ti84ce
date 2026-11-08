//! Token-based single-line editor (overwrite by default, like the real
//! calculator), plus helpers for wrapping text with a visible cursor.

use super::view::{color, Row, Span, COLS};
use crate::engine::{tokens_to_string, Tok};

#[derive(Debug, Clone, Default)]
pub struct LineEditor {
    pub toks: Vec<Tok>,
    pub cur: usize,
    pub insert: bool,
}

impl LineEditor {
    pub fn from_toks(toks: Vec<Tok>) -> Self {
        LineEditor { toks, cur: 0, insert: false }
    }

    pub fn is_empty(&self) -> bool {
        self.toks.is_empty()
    }

    pub fn clear(&mut self) {
        self.toks.clear();
        self.cur = 0;
        self.insert = false;
    }

    pub fn set(&mut self, toks: Vec<Tok>) {
        self.cur = toks.len();
        self.toks = toks;
        self.insert = false;
    }

    pub fn put(&mut self, t: Tok) {
        if self.insert || self.cur >= self.toks.len() {
            self.toks.insert(self.cur, t);
        } else {
            self.toks[self.cur] = t;
        }
        self.cur += 1;
    }

    pub fn put_all(&mut self, ts: &[Tok]) {
        // Multi-token pastes always insert.
        let ins = self.insert;
        if ts.len() > 1 {
            self.insert = true;
        }
        for t in ts {
            self.put(t);
        }
        self.insert = ins;
    }

    pub fn del(&mut self) {
        if self.cur < self.toks.len() {
            self.toks.remove(self.cur);
        } else if self.cur > 0 {
            // At the end of the line, delete the last token for convenience.
            self.cur -= 1;
            self.toks.remove(self.cur);
        }
    }

    pub fn backspace(&mut self) {
        if self.cur > 0 {
            self.cur -= 1;
            self.toks.remove(self.cur);
        }
    }

    pub fn left(&mut self) {
        self.insert = false;
        self.cur = self.cur.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.insert = false;
        if self.cur < self.toks.len() {
            self.cur += 1;
        }
    }

    pub fn home(&mut self) {
        self.cur = 0;
    }

    pub fn end(&mut self) {
        self.cur = self.toks.len();
    }

    pub fn text(&self) -> String {
        tokens_to_string(&self.toks)
    }

    /// Character offset of the cursor within `text()`.
    pub fn char_pos(&self) -> usize {
        self.toks[..self.cur].iter().map(|t| t.chars().count()).sum()
    }

    /// Move the cursor up or down a wrapped row of the given width
    /// (with `first` characters of prefix on the first row).
    pub fn vertical(&mut self, delta: i32, width: usize, first: usize) -> bool {
        let pos = self.char_pos() + first;
        let target = pos as i64 + delta as i64 * width as i64;
        let total = self.text().chars().count() + first;
        if target < first as i64 || (delta > 0 && (pos / width) == (total / width)) {
            return false;
        }
        let target = (target as usize).min(total) - first;
        // Find the token boundary at or before the target char offset.
        let mut acc = 0;
        let mut idx = 0;
        for (i, t) in self.toks.iter().enumerate() {
            let n = t.chars().count();
            if acc + n > target {
                idx = i;
                break;
            }
            acc += n;
            idx = i + 1;
        }
        self.cur = idx;
        true
    }
}

/// After typing `(` on the computer keyboard, turn a run of letter tokens
/// like `S I N (` into the function token `sin(`. Returns true if folded.
pub fn fold_function_name(ed: &mut LineEditor) -> bool {
    if ed.cur == 0 || ed.toks[ed.cur - 1] != "(" {
        return false;
    }
    let end = ed.cur - 1;
    let mut start = end;
    while start > 0 && end - start < 10 {
        let t = ed.toks[start - 1];
        if t.len() == 1 && t.as_bytes()[0].is_ascii_uppercase() {
            start -= 1;
        } else {
            break;
        }
    }
    const ALIASES: &[(&str, Tok)] = &[
        ("sqrt", "√("),
        ("asin", "sin⁻¹("),
        ("acos", "cos⁻¹("),
        ("atan", "tan⁻¹("),
        ("exp", "ℯ^("),
        ("cbrt", "³√("),
    ];
    for s in start..end {
        let name: String = ed.toks[s..end].concat().to_lowercase();
        let found = ALIASES.iter().find(|(a, _)| *a == name).map(|(_, t)| *t).or_else(|| {
            crate::engine::FUNCS
                .iter()
                .copied()
                .find(|f| f.trim_end_matches('(').to_lowercase() == name)
        });
        if let Some(f) = found {
            ed.toks.drain(s..=end);
            ed.toks.insert(s, f);
            ed.cur = s + 1;
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    Block,
    Second,
    Alpha,
    Lower,
    Insert,
}

impl CursorKind {
    fn glyph(self, under: char) -> char {
        match self {
            CursorKind::Block | CursorKind::Insert => under,
            CursorKind::Second => '↑',
            CursorKind::Alpha => 'A',
            CursorKind::Lower => 'a',
        }
    }
}

/// Wrap `text` (with an optional colored prefix on the first row) into rows of
/// `COLS` characters, drawing a cursor at char offset `cursor` when visible.
pub fn wrap_with_cursor(
    prefix: &[Span],
    text: &str,
    cursor: Option<(usize, CursorKind, bool)>,
    fg: u32,
) -> Vec<Row> {
    let prefix_len: usize = prefix.iter().map(|s| s.text.chars().count()).sum();
    let mut chars: Vec<char> = text.chars().collect();
    let cur_pos = cursor.map(|(p, _, _)| p + prefix_len);
    // Make room for a cursor sitting at the end.
    if let Some(p) = cur_pos {
        if p >= prefix_len + chars.len() {
            chars.push(' ');
        }
    }
    let total = prefix_len + chars.len();
    let nrows = total.div_ceil(COLS).max(1);
    let mut rows = Vec::with_capacity(nrows);
    for r in 0..nrows {
        let mut row: Row = Vec::new();
        let start = r * COLS;
        let end = ((r + 1) * COLS).min(total);
        let mut buf = String::new();
        for pos in start..end {
            if pos < prefix_len {
                continue;
            }
            let c = chars[pos - prefix_len];
            if Some(pos) == cur_pos {
                let (_, kind, on) = cursor.unwrap();
                if !buf.is_empty() {
                    row.push(Span::new(std::mem::take(&mut buf)).fg(fg));
                }
                if on {
                    match kind {
                        CursorKind::Insert => row.push(Span::new("_").fg(color::BLACK).bg(0xB0B0B0)),
                        k => row.push(Span::new(k.glyph(c).to_string()).fg(color::WHITE).bg(color::BLACK)),
                    }
                } else {
                    row.push(Span::new(c.to_string()).fg(fg));
                }
            } else {
                buf.push(c);
            }
        }
        if !buf.is_empty() {
            row.push(Span::new(buf).fg(fg));
        }
        if r == 0 {
            let mut full = prefix.to_vec();
            full.extend(row);
            rows.push(full);
        } else {
            rows.push(row);
        }
    }
    rows
}
