//! The home screen: entry line, history, ENTRY recall and statistics commands.

use super::editor::{wrap_with_cursor, LineEditor};
use super::view::{color, right, Row, Span, COLS, ROWS};
use super::{Answers, Calc, ErrFrom, Mods, Screen};
use crate::engine::eval::Value;
use crate::engine::format::format_num;
use crate::engine::{format_value, parse_expr, parse_line, string_to_tokens, CResult, CalcError, Expr, Tok};
use crate::keys::Key;

#[derive(Debug, Clone)]
pub struct HistItem {
    pub input: Option<Vec<Tok>>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Home {
    pub history: Vec<HistItem>,
    pub editor: LineEditor,
    pub entries: Vec<Vec<Tok>>,
    pub entry_idx: Option<usize>,
    pub hist_sel: Option<usize>,
}

impl Home {
    pub fn push_message(&mut self, msg: &str) {
        self.history.push(HistItem { input: None, outputs: vec![msg.to_string()] });
    }

    /// Flattened list of selectable history pieces: (item, None=input / Some(k)=output k).
    fn selectables(&self) -> Vec<(usize, Option<usize>)> {
        let mut out = Vec::new();
        for (i, h) in self.history.iter().enumerate() {
            if h.input.is_some() {
                out.push((i, None));
                for k in 0..h.outputs.len() {
                    out.push((i, Some(k)));
                }
            }
        }
        out
    }
}

/// Tokens that, typed first on an empty line, implicitly operate on Ans.
const ANS_PREFIX: &[Tok] = &[
    "+", "-", "*", "/", "^", "²", "³", "⁻¹", "→", "!", " nPr ", " nCr ", "=", "≠", ">", "≥", "<",
    "≤", " and ", " or ", " xor ", "►Frac", "►Dec", "°", "ʳ",
];

const STAT_CMDS: &[Tok] = &["1-Var Stats ", "2-Var Stats ", "LinReg(ax+b) "];

fn split_args(toks: &[Tok]) -> Vec<&[Tok]> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, t) in toks.iter().enumerate() {
        if matches!(*t, "(" | "{" | "[") || crate::engine::is_func(t) {
            depth += 1;
        } else if matches!(*t, ")" | "}" | "]") {
            depth -= 1;
        } else if *t == "," && depth == 0 {
            out.push(&toks[start..i]);
            start = i + 1;
        }
    }
    if start < toks.len() {
        out.push(&toks[start..]);
    }
    out
}

fn quartiles(sorted: &[f64]) -> (f64, f64, f64) {
    fn median(s: &[f64]) -> f64 {
        let n = s.len();
        if n == 0 {
            return f64::NAN;
        }
        if n % 2 == 1 {
            s[n / 2]
        } else {
            (s[n / 2 - 1] + s[n / 2]) / 2.0
        }
    }
    let n = sorted.len();
    let med = median(sorted);
    if n < 2 {
        return (med, med, med);
    }
    let lower = &sorted[..n / 2];
    let upper = &sorted[n.div_ceil(2)..];
    (median(lower), med, median(upper))
}

impl Calc {
    pub(super) fn home_insert(&mut self, toks: &[Tok]) {
        self.home.hist_sel = None;
        self.home.entry_idx = None;
        let ed = &mut self.home.editor;
        if ed.is_empty() && toks.len() == 1 && ANS_PREFIX.contains(&toks[0]) {
            ed.put("Ans");
        }
        ed.put_all(toks);
    }

    pub(super) fn home_key(&mut self, key: Key, m: Mods) {
        let fmt = self.fmt();
        if let Some(sel) = self.home.hist_sel {
            let items = self.home.selectables();
            match key {
                Key::Up => {
                    self.home.hist_sel = Some(sel.saturating_sub(1));
                    return;
                }
                Key::Down => {
                    self.home.hist_sel = if sel + 1 < items.len() { Some(sel + 1) } else { None };
                    return;
                }
                Key::Enter => {
                    let (i, which) = items[sel];
                    let h = &self.home.history[i];
                    let toks = match which {
                        None => h.input.clone().unwrap_or_default(),
                        Some(k) => {
                            let text = h.outputs[k].clone();
                            let text = if text.starts_with('{') { text.replace(' ', ",") } else { text };
                            string_to_tokens(&text)
                        }
                    };
                    self.home.hist_sel = None;
                    self.home.editor.put_all(&toks);
                    return;
                }
                Key::Clear => {
                    self.home.hist_sel = None;
                    return;
                }
                _ => self.home.hist_sel = None,
            }
        }
        let _ = fmt;
        match key {
            Key::Enter if m.second => {
                let n = self.home.entries.len();
                if n == 0 {
                    return;
                }
                let idx = match self.home.entry_idx {
                    None => n - 1,
                    Some(0) => n - 1,
                    Some(i) => i - 1,
                };
                self.home.entry_idx = Some(idx);
                let toks = self.home.entries[idx].clone();
                self.home.editor.set(toks);
            }
            Key::Enter => self.home_execute(),
            Key::Clear => {
                if self.home.editor.is_empty() {
                    self.home.history.clear();
                } else {
                    self.home.editor.clear();
                }
                self.home.entry_idx = None;
            }
            Key::Del => {
                if m.second {
                    self.home.editor.insert = !self.home.editor.insert;
                } else {
                    self.home.editor.del();
                }
            }
            Key::Left => {
                if m.second {
                    self.home.editor.home()
                } else {
                    self.home.editor.left()
                }
            }
            Key::Right => {
                if m.second {
                    self.home.editor.end()
                } else {
                    self.home.editor.right()
                }
            }
            Key::Up => {
                if !self.home.editor.vertical(-1, COLS, 0) {
                    let n = self.home.selectables().len();
                    if n > 0 {
                        self.home.hist_sel = Some(n - 1);
                    }
                }
            }
            Key::Down => {
                self.home.editor.vertical(1, COLS, 0);
            }
            _ => {
                if let Some(t) = Self::key_token(key, m) {
                    self.home_insert(&[t]);
                }
            }
        }
    }

    pub(super) fn home_execute(&mut self) {
        let toks = if self.home.editor.is_empty() {
            match self.home.entries.last() {
                Some(t) => t.clone(),
                None => return,
            }
        } else {
            self.home.editor.toks.clone()
        };
        if toks.is_empty() {
            return;
        }
        self.home.entries.push(toks.clone());
        if self.home.entries.len() > 50 {
            self.home.entries.remove(0);
        }
        self.home.entry_idx = None;
        if toks == ["ClrHome"] {
            self.home.history.clear();
            self.home.editor.clear();
            return;
        }
        if STAT_CMDS.contains(&toks[0]) {
            match self.run_stats(&toks) {
                Ok((title, lines)) => {
                    self.home.history.push(HistItem { input: Some(toks), outputs: vec!["Done".into()] });
                    self.home.editor.clear();
                    self.screen = Screen::Output { title, lines, scroll: 0 };
                }
                Err(e) => self.show_error(e, ErrFrom::Home(toks)),
            }
            return;
        }
        let result = parse_line(&toks).and_then(|line| self.env.run_line(&line));
        match result {
            Ok((v, frac)) => {
                let frac = match self.settings.answers {
                    Answers::Frac => true,
                    _ => frac,
                };
                let out = format_value(&v, self.fmt(), frac);
                self.home.history.push(HistItem { input: Some(toks), outputs: vec![out] });
                if self.home.history.len() > 100 {
                    self.home.history.remove(0);
                }
                self.home.editor.clear();
            }
            Err(e) => self.show_error(e, ErrFrom::Home(toks)),
        }
    }

    fn eval_list_arg(&mut self, toks: &[Tok]) -> CResult<Vec<f64>> {
        let e = parse_expr(toks)?;
        match self.env.eval(&e)? {
            Value::List(xs) => Ok(xs),
            Value::Num(_) => Err(CalcError::DataType),
        }
    }

    fn run_stats(&mut self, toks: &[Tok]) -> CResult<(String, Vec<String>)> {
        let cmd = toks[0];
        let args = split_args(&toks[1..]);
        let fmtv = self.fmt();
        let f = move |x: f64| format_num(x, fmtv);
        let list_or = |c: &mut Calc, i: usize, default: usize| -> CResult<Vec<f64>> {
            match args.get(i) {
                Some(a) => c.eval_list_arg(a),
                None => {
                    let l = c.env.lists[default].clone();
                    if l.is_empty() {
                        Err(CalcError::InvalidDim)
                    } else {
                        Ok(l)
                    }
                }
            }
        };
        match cmd {
            "1-Var Stats " => {
                let xs = list_or(self, 0, 0)?;
                let freq = match args.get(1) {
                    Some(a) => self.eval_list_arg(a)?,
                    None => vec![1.0; xs.len()],
                };
                if freq.len() != xs.len() {
                    return Err(CalcError::DimMismatch);
                }
                let n: f64 = freq.iter().sum();
                if n <= 0.0 {
                    return Err(CalcError::InvalidDim);
                }
                let sx: f64 = xs.iter().zip(&freq).map(|(x, w)| x * w).sum();
                let sx2: f64 = xs.iter().zip(&freq).map(|(x, w)| x * x * w).sum();
                let mean = sx / n;
                let ss: f64 = xs.iter().zip(&freq).map(|(x, w)| (x - mean).powi(2) * w).sum();
                let mut expanded = Vec::new();
                for (x, w) in xs.iter().zip(&freq) {
                    for _ in 0..(w.round() as usize) {
                        expanded.push(*x);
                    }
                }
                expanded.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let (q1, med, q3) = quartiles(&expanded);
                let mut lines = vec![
                    format!("x̄={}", f(mean)),
                    format!("Σx={}", f(sx)),
                    format!("Σx²={}", f(sx2)),
                ];
                if n > 1.0 {
                    lines.push(format!("Sx={}", f((ss / (n - 1.0)).sqrt())));
                } else {
                    lines.push("Sx=".into());
                }
                lines.extend([
                    format!("σx={}", f((ss / n).sqrt())),
                    format!("n={}", f(n)),
                    format!("minX={}", f(expanded.first().copied().unwrap_or(0.0))),
                    format!("Q₁={}", f(q1)),
                    format!("Med={}", f(med)),
                    format!("Q₃={}", f(q3)),
                    format!("maxX={}", f(expanded.last().copied().unwrap_or(0.0))),
                ]);
                Ok(("1-Var Stats".into(), lines))
            }
            "2-Var Stats " | "LinReg(ax+b) " => {
                let xs = list_or(self, 0, 0)?;
                let ys = list_or(self, 1, 1)?;
                if xs.len() != ys.len() {
                    return Err(CalcError::DimMismatch);
                }
                if xs.len() < 2 {
                    return Err(CalcError::InvalidDim);
                }
                let n = xs.len() as f64;
                let sx: f64 = xs.iter().sum();
                let sy: f64 = ys.iter().sum();
                let sx2: f64 = xs.iter().map(|x| x * x).sum();
                let sy2: f64 = ys.iter().map(|y| y * y).sum();
                let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| x * y).sum();
                let (mx, my) = (sx / n, sy / n);
                let sxx = sx2 - n * mx * mx;
                let syy = sy2 - n * my * my;
                let sxy_c = sxy - n * mx * my;
                if cmd == "2-Var Stats " {
                    let minmax = |v: &[f64]| {
                        (v.iter().cloned().fold(f64::INFINITY, f64::min), v.iter().cloned().fold(f64::NEG_INFINITY, f64::max))
                    };
                    let (minx, maxx) = minmax(&xs);
                    let (miny, maxy) = minmax(&ys);
                    let lines = vec![
                        format!("x̄={}", f(mx)),
                        format!("Σx={}", f(sx)),
                        format!("Σx²={}", f(sx2)),
                        format!("Sx={}", f((sxx / (n - 1.0)).sqrt())),
                        format!("σx={}", f((sxx / n).sqrt())),
                        format!("ȳ={}", f(my)),
                        format!("Σy={}", f(sy)),
                        format!("Σy²={}", f(sy2)),
                        format!("Sy={}", f((syy / (n - 1.0)).sqrt())),
                        format!("σy={}", f((syy / n).sqrt())),
                        format!("Σxy={}", f(sxy)),
                        format!("n={}", f(n)),
                        format!("minX={}", f(minx)),
                        format!("maxX={}", f(maxx)),
                        format!("minY={}", f(miny)),
                        format!("maxY={}", f(maxy)),
                    ];
                    return Ok(("2-Var Stats".into(), lines));
                }
                if sxx == 0.0 {
                    return Err(CalcError::DivideBy0);
                }
                let a = sxy_c / sxx;
                let b = my - a * mx;
                let r = if syy == 0.0 { 0.0 } else { sxy_c / (sxx * syy).sqrt() };
                // Optional third argument: a Y-var to store the regression equation in.
                if let Some(arg) = args.get(2) {
                    match parse_expr(arg)? {
                        Expr::YVar(i) => {
                            let text = format!("{}X+{}", fmt_plain(a), fmt_plain(b)).replace("+⁻", "-");
                            let toks = string_to_tokens(&text);
                            self.yeq.funcs[i].set(toks);
                            self.yeq.enabled[i] = true;
                            self.sync_y(i);
                        }
                        _ => return Err(CalcError::Argument),
                    }
                }
                let lines = vec![
                    "y=ax+b".into(),
                    format!("a={}", f(a)),
                    format!("b={}", f(b)),
                    format!("r²={}", f(r * r)),
                    format!("r={}", f(r)),
                ];
                Ok(("LinReg".into(), lines))
            }
            _ => Err(CalcError::Syntax(0)),
        }
    }

    pub(super) fn home_rows(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = Vec::new();
        let sel = self.home.hist_sel.map(|s| self.home.selectables()[s]);
        let highlight = |row: Row| -> Row {
            row.into_iter().map(|s| Span { text: s.text, fg: color::WHITE, bg: Some(color::SEL) }).collect()
        };
        for (i, h) in self.home.history.iter().enumerate() {
            if let Some(input) = &h.input {
                let text = crate::engine::tokens_to_string(input);
                let mut r = wrap_with_cursor(&[], &text, None, color::BLACK);
                if sel == Some((i, None)) {
                    r = r.into_iter().map(highlight).collect();
                }
                rows.extend(r);
            }
            for (k, o) in h.outputs.iter().enumerate() {
                let mut r: Vec<Row> = if o.chars().count() <= COLS {
                    vec![right(o)]
                } else {
                    wrap_with_cursor(&[], o, None, color::BLACK)
                };
                if sel == Some((i, Some(k))) {
                    r = r
                        .into_iter()
                        .map(|row| {
                            // Highlight only the text, not the padding.
                            let text = row.iter().map(|s| s.text.clone()).collect::<String>();
                            let trimmed = text.trim_start();
                            let pad = text.chars().count() - trimmed.chars().count();
                            vec![Span::new(" ".repeat(pad)), Span::inverse(trimmed)]
                        })
                        .collect();
                }
                rows.extend(r);
            }
        }
        let ed = &self.home.editor;
        let cursor = if self.home.hist_sel.is_none() {
            Some((ed.char_pos(), self.cursor_kind(ed.insert), self.blink))
        } else {
            None
        };
        rows.extend(wrap_with_cursor(&[], &ed.text(), cursor, color::BLACK));
        let n = rows.len();
        // Keep the selected history row visible when scrolled up.
        if n > ROWS {
            if let Some((i, which)) = sel {
                let mut idx = 0;
                let mut target = 0;
                for (j, h) in self.home.history.iter().enumerate() {
                    if let Some(input) = &h.input {
                        let len = crate::engine::tokens_to_string(input).chars().count().max(1).div_ceil(COLS);
                        if j == i && which.is_none() {
                            target = idx;
                        }
                        idx += len;
                    }
                    for (k, o) in h.outputs.iter().enumerate() {
                        if j == i && which == Some(k) {
                            target = idx;
                        }
                        idx += o.chars().count().max(1).div_ceil(COLS);
                    }
                }
                let start = target.min(n - ROWS);
                return rows.into_iter().skip(start).take(ROWS).collect();
            }
            return rows.into_iter().skip(n - ROWS).collect();
        }
        rows
    }
}

fn fmt_plain(x: f64) -> String {
    format_num(x, crate::engine::NumFormat::default())
}
