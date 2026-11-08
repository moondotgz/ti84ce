//! The Python App: script browser, interactive shell and script editor.

use super::editor::{wrap_with_cursor, CursorKind};
use super::menus::{self, Cmd};
use super::view::*;
use super::{AlphaState, Calc, Mods, Screen};
use crate::keys::Key;
use crate::python::{needs_more, PyEvent, PyWorker};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyScreen {
    Files,
    Shell,
    Editor,
    NewName,
}

#[derive(Debug, Clone)]
pub struct Script {
    pub name: String,
    pub lines: Vec<String>,
}

pub struct PyApp {
    worker: Option<PyWorker>,
    pub screen: PyScreen,
    pub scripts: Vec<Script>,
    pub file_sel: usize,
    shell: Vec<String>,
    /// The last shell line has no trailing newline yet.
    shell_open: bool,
    input: String,
    icur: usize,
    pending: Vec<String>,
    history: Vec<String>,
    hist_idx: Option<usize>,
    awaiting_input: bool,
    edit_idx: usize,
    row: usize,
    col: usize,
    scroll: usize,
    clip: Option<String>,
    pub lower: bool,
    name: String,
    dir: Option<PathBuf>,
}

const DEMO: &str = "\
from math import *
# Fibonacci and primes demo
def fib(n):
  a, b = 0, 1
  for i in range(n):
    a, b = b, a + b
  return a

def is_prime(n):
  if n < 2:
    return False
  for d in range(2, int(sqrt(n)) + 1):
    if n % d == 0:
      return False
  return True

print(\"fib(30) =\", fib(30))
primes = [p for p in range(50) if is_prime(p)]
print(\"primes:\", primes)
";

const GUESS: &str = "\
from random import *
secret = randint(1, 20)
tries = 0
while True:
  g = int(input(\"Guess 1-20: \"))
  tries += 1
  if g < secret:
    print(\"Higher\")
  elif g > secret:
    print(\"Lower\")
  else:
    print(\"Got it in\", tries, \"tries!\")
    break
";

fn data_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("ti84ce").join("python"))
}

fn indent_of(s: &str) -> usize {
    s.chars().take_while(|&c| c == ' ').count()
}

fn byte_idx(s: &str, ch: usize) -> usize {
    s.char_indices().nth(ch).map(|(b, _)| b).unwrap_or(s.len())
}

impl PyApp {
    pub fn new() -> Self {
        let dir = data_dir();
        let mut scripts = Vec::new();
        if let Some(d) = &dir {
            if let Ok(rd) = std::fs::read_dir(d) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().is_some_and(|x| x == "py") {
                        if let (Some(stem), Ok(text)) = (p.file_stem(), std::fs::read_to_string(&p)) {
                            scripts.push(Script {
                                name: stem.to_string_lossy().to_string(),
                                lines: text.trim_end_matches('\n').split('\n').map(String::from).collect(),
                            });
                        }
                    }
                }
            }
        }
        let first_run = scripts.is_empty();
        if first_run {
            for (name, src) in [("DEMO", DEMO), ("GUESS", GUESS)] {
                scripts.push(Script { name: name.into(), lines: src.trim_end().split('\n').map(String::from).collect() });
            }
        }
        scripts.sort_by(|a, b| a.name.cmp(&b.name));
        let app = PyApp {
            worker: None,
            screen: PyScreen::Files,
            scripts,
            file_sel: 0,
            shell: Vec::new(),
            shell_open: false,
            input: String::new(),
            icur: 0,
            pending: Vec::new(),
            history: Vec::new(),
            hist_idx: None,
            awaiting_input: false,
            edit_idx: 0,
            row: 0,
            col: 0,
            scroll: 0,
            clip: None,
            lower: true,
            name: String::new(),
            dir,
        };
        if first_run {
            for i in 0..app.scripts.len() {
                app.save(i);
            }
        }
        app
    }

    pub fn placeholder() -> Self {
        PyApp::new()
    }

    pub fn busy(&self) -> bool {
        self.worker.as_ref().is_some_and(|w| w.busy)
    }

    pub fn interrupt(&self) {
        if let Some(w) = &self.worker {
            if w.busy {
                w.interrupt();
                if self.awaiting_input {
                    w.send_input(None);
                }
            }
        }
    }

    pub fn reset(&mut self) {
        if let Some(w) = &self.worker {
            w.reset();
        }
        self.shell.clear();
        self.shell_open = false;
    }

    fn worker(&mut self) -> &mut PyWorker {
        self.worker.get_or_insert_with(PyWorker::spawn)
    }

    fn save(&self, i: usize) {
        let (Some(dir), Some(s)) = (&self.dir, self.scripts.get(i)) else { return };
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(dir.join(format!("{}.py", s.name)), s.lines.join("\n") + "\n");
    }

    fn delete_file(&self, name: &str) {
        if let Some(dir) = &self.dir {
            let _ = std::fs::remove_file(dir.join(format!("{name}.py")));
        }
    }

    pub fn title(&self) -> String {
        match self.screen {
            PyScreen::Files => "PYTHON APP".into(),
            PyScreen::Shell => "PYTHON SHELL".into(),
            PyScreen::Editor => format!("EDITOR: {}", self.scripts.get(self.edit_idx).map(|s| s.name.as_str()).unwrap_or("")),
            PyScreen::NewName => "NEW SCRIPT".into(),
        }
    }

    pub fn softkeys(&self) -> [&'static str; 5] {
        match self.screen {
            PyScreen::Files => ["Run", "Edit", "New", "Shell", "Manage"],
            PyScreen::Shell => ["Fns…", "a A #", "Tools", "Editor", "Files"],
            PyScreen::Editor => ["Fns…", "a A #", "Tools", "Run", "Files"],
            PyScreen::NewName => ["", "", "", "Ok", "Esc"],
        }
    }

    fn append(&mut self, piece: &str) {
        if piece.is_empty() {
            return;
        }
        if self.shell_open {
            if let Some(l) = self.shell.last_mut() {
                l.push_str(piece);
            }
        } else {
            self.shell.push(piece.to_string());
            self.shell_open = true;
        }
    }

    fn print(&mut self, text: &str) {
        let mut pieces = text.split('\n');
        if let Some(first) = pieces.next() {
            self.append(first);
        }
        for p in pieces {
            if !self.shell_open {
                self.shell.push(String::new());
            }
            self.shell_open = false;
            self.append(p);
        }
        if self.shell.len() > 400 {
            let drop = self.shell.len() - 400;
            self.shell.drain(..drop);
        }
    }

    fn run_source(&mut self, src: String, file: &str, repl: bool) {
        self.worker().run(src, file, repl);
    }

    fn run_script(&mut self, idx: usize) {
        let Some(s) = self.scripts.get(idx).cloned() else { return };
        self.save(idx);
        self.worker().reset();
        self.shell_open = false;
        self.print("*** Shell Reinitialized ***\n");
        self.print(&format!(">>> # Running {}\n", s.name));
        self.screen = PyScreen::Shell;
        self.input.clear();
        self.icur = 0;
        self.pending.clear();
        let file = format!("{}.py", s.name);
        self.run_source(s.lines.join("\n"), &file, false);
    }

    fn insert_text(&mut self, text: &str) {
        match self.screen {
            PyScreen::Shell => {
                if self.busy() && !self.awaiting_input {
                    return;
                }
                let b = byte_idx(&self.input, self.icur);
                self.input.insert_str(b, text);
                self.icur += text.chars().count();
            }
            PyScreen::Editor => {
                let Some(s) = self.scripts.get_mut(self.edit_idx) else { return };
                let line = &mut s.lines[self.row];
                let b = byte_idx(line, self.col);
                line.insert_str(b, text);
                self.col += text.chars().count();
            }
            PyScreen::NewName => {
                for c in text.chars() {
                    if c.is_ascii_alphanumeric() && self.name.chars().count() < 8 {
                        if self.name.is_empty() && c.is_ascii_digit() {
                            continue;
                        }
                        self.name.push(c.to_ascii_uppercase());
                    }
                }
            }
            PyScreen::Files => {}
        }
    }

    fn shell_enter(&mut self) {
        if self.busy() {
            if self.awaiting_input {
                let text = std::mem::take(&mut self.input);
                self.icur = 0;
                self.print(&format!("{text}\n"));
                self.awaiting_input = false;
                if let Some(w) = &self.worker {
                    w.send_input(Some(text));
                }
            }
            return;
        }
        let line = std::mem::take(&mut self.input);
        self.icur = 0;
        self.hist_idx = None;
        let prompt = if self.pending.is_empty() { ">>> " } else { "... " };
        self.print(&format!("{prompt}{line}\n"));
        let run = if self.pending.is_empty() {
            if line.trim().is_empty() {
                return;
            }
            if needs_more(&line) {
                self.pending.push(line.clone());
                None
            } else {
                Some(line)
            }
        } else if line.trim().is_empty() {
            let src = self.pending.join("\n");
            self.pending.clear();
            Some(src)
        } else {
            self.pending.push(line.clone());
            None
        };
        match run {
            Some(src) => {
                self.history.push(src.clone());
                self.run_source(src, "<stdin>", true);
            }
            None => {
                // Auto-indent the continuation line.
                let last = self.pending.last().cloned().unwrap_or_default();
                let mut ind = indent_of(&last);
                if last.trim_end().ends_with(':') {
                    ind += 2;
                }
                self.input = " ".repeat(ind);
                self.icur = ind;
            }
        }
    }

    fn editor_newline(&mut self) {
        let Some(s) = self.scripts.get_mut(self.edit_idx) else { return };
        let line = s.lines[self.row].clone();
        let b = byte_idx(&line, self.col);
        let (left, right) = line.split_at(b);
        let mut ind = indent_of(left);
        if left.trim_end().ends_with(':') {
            ind += 2;
        }
        s.lines[self.row] = left.to_string();
        s.lines.insert(self.row + 1, format!("{}{}", " ".repeat(ind), right.trim_start()));
        self.row += 1;
        self.col = ind;
    }

    fn editor_backspace(&mut self) {
        let Some(s) = self.scripts.get_mut(self.edit_idx) else { return };
        if self.col > 0 {
            let line = &mut s.lines[self.row];
            let b = byte_idx(line, self.col - 1);
            line.remove(b);
            self.col -= 1;
        } else if self.row > 0 {
            let cur = s.lines.remove(self.row);
            self.row -= 1;
            self.col = s.lines[self.row].chars().count();
            s.lines[self.row].push_str(&cur);
        }
    }

    fn fix_editor_scroll(&mut self) {
        let vis = ROWS - 1;
        if self.row < self.scroll {
            self.scroll = self.row;
        } else if self.row >= self.scroll + vis {
            self.scroll = self.row + 1 - vis;
        }
    }

    pub fn rows(&self, blink: bool, kind: CursorKind) -> Vec<Row> {
        let kind = if self.lower && kind == CursorKind::Alpha { CursorKind::Lower } else { kind };
        let vis = ROWS - 1;
        match self.screen {
            PyScreen::Files => {
                let mut rows = Vec::new();
                if self.scripts.is_empty() {
                    rows.push(plain("No scripts yet."));
                    rows.push(plain("Press [New] to create one."));
                }
                let start = self.file_sel.saturating_sub(vis - 1);
                for (i, s) in self.scripts.iter().enumerate().skip(start).take(vis) {
                    let size: usize = s.lines.iter().map(|l| l.len() + 1).sum();
                    let label = format!("{:<12}{:>8} B", s.name, size);
                    if i == self.file_sel {
                        rows.push(vec![Span::new("▸").fg(color::SEL), Span::inverse(label)]);
                    } else {
                        rows.push(vec![Span::new(" "), Span::new(label)]);
                    }
                }
                rows
            }
            PyScreen::NewName => {
                let cursor = Some((self.name.chars().count(), CursorKind::Alpha, blink));
                let mut rows = vec![plain("New Script"), plain("")];
                rows.extend(wrap_with_cursor(&[Span::new("Name=")], &self.name, cursor, color::BLACK));
                rows.push(plain(""));
                rows.push(vec![Span::new("Up to 8 letters or digits.").fg(color::GRAY)]);
                rows
            }
            PyScreen::Shell => {
                let mut rows: Vec<Row> = Vec::new();
                let mut lines: Vec<&String> = self.shell.iter().collect();
                let mut open_prefix = String::new();
                let show_input = !self.busy() || self.awaiting_input;
                if self.awaiting_input && self.shell_open {
                    if let Some(l) = lines.pop() {
                        open_prefix = l.clone();
                    }
                }
                for l in lines {
                    rows.extend(wrap_with_cursor(&[], l, None, color::BLACK));
                }
                if show_input {
                    let prompt = if self.awaiting_input {
                        open_prefix
                    } else if self.pending.is_empty() {
                        ">>> ".to_string()
                    } else {
                        "... ".to_string()
                    };
                    let cursor = Some((self.icur, kind, blink));
                    rows.extend(wrap_with_cursor(&[Span::new(prompt).fg(color::NAVY)], &self.input, cursor, color::BLACK));
                }
                let n = rows.len();
                rows.into_iter().skip(n.saturating_sub(vis)).collect()
            }
            PyScreen::Editor => {
                let Some(s) = self.scripts.get(self.edit_idx) else { return vec![] };
                let hs = self.col.saturating_sub(COLS - 2);
                let mut rows = Vec::new();
                for (i, line) in s.lines.iter().enumerate().skip(self.scroll).take(vis) {
                    let chars: Vec<char> = line.chars().collect();
                    let ind = indent_of(line);
                    let mut row: Row = Vec::new();
                    let mut buf = String::new();
                    let end = (hs + COLS).min(chars.len() + 1);
                    for (c_idx, pos) in (hs..end).enumerate() {
                        let _ = c_idx;
                        let ch = chars.get(pos).copied().unwrap_or(' ');
                        let is_cur = i == self.row && pos == self.col;
                        if is_cur || pos < ind {
                            if !buf.is_empty() {
                                row.push(Span::new(std::mem::take(&mut buf)));
                            }
                            if is_cur && blink {
                                let g = match kind {
                                    CursorKind::Second => '↑',
                                    CursorKind::Alpha => 'A',
                                    CursorKind::Lower => 'a',
                                    _ => ch,
                                };
                                row.push(Span::new(g.to_string()).fg(color::WHITE).bg(color::BLACK));
                            } else if pos < ind {
                                row.push(Span::new("·").fg(color::LTGRAY));
                            } else {
                                row.push(Span::new(ch.to_string()));
                            }
                        } else if pos < chars.len() {
                            buf.push(ch);
                        }
                    }
                    if !buf.is_empty() {
                        row.push(Span::new(buf));
                    }
                    rows.push(row);
                }
                rows
            }
        }
    }
}

impl Calc {
    pub(super) fn enter_python(&mut self) {
        self.leave_screen();
        self.screen = Screen::Python;
        self.py.screen = PyScreen::Files;
        self.alpha = AlphaState::Off;
    }

    pub(super) fn leave_python(&mut self) {
        if self.py.screen == PyScreen::Editor {
            self.py.save(self.py.edit_idx);
        }
        self.py.interrupt();
        self.alpha = AlphaState::Off;
        self.screen = Screen::Home;
    }

    pub(super) fn py_alpha_key(&mut self) {
        if self.second {
            self.second = false;
            self.alpha = AlphaState::Lock;
        } else {
            self.alpha = match self.alpha {
                AlphaState::Off => AlphaState::Once,
                _ => AlphaState::Off,
            };
        }
    }

    fn py_key_text(&self, key: Key, m: Mods) -> Option<&'static str> {
        use Key::*;
        if m.alpha {
            let c = key.alpha_char()?;
            if c == "θ" {
                return None;
            }
            if self.py.lower && c.len() == 1 && c.as_bytes()[0].is_ascii_uppercase() {
                const LOWER: [&str; 26] = [
                    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r",
                    "s", "t", "u", "v", "w", "x", "y", "z",
                ];
                return Some(LOWER[(c.as_bytes()[0] - b'A') as usize]);
            }
            return Some(c);
        }
        if m.second {
            return Some(match key {
                Sin => "asin(",
                Cos => "acos(",
                Tan => "atan(",
                Pow => "pi",
                Sq => "sqrt(",
                Comma => "e",
                LParen => "{",
                RParen => "}",
                Div => "e",
                Log => "10**",
                Mul => "[",
                Ln => "exp(",
                Sub => "]",
                Dot => ":",
                Neg => "_",
                Sto => "\\",
                _ => return None,
            });
        }
        if let Some(d) = key.digit() {
            return Some(["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"][d as usize]);
        }
        Some(match key {
            Dot => ".",
            Neg => "-",
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Pow => "**",
            Sq => "**2",
            Inv => "**-1",
            Sin => "sin(",
            Cos => "cos(",
            Tan => "tan(",
            Log => "log10(",
            Ln => "log(",
            LParen => "(",
            RParen => ")",
            Comma => ",",
            Sto => "=",
            Xtn => "x",
            _ => return None,
        })
    }

    pub(super) fn py_type(&mut self, text: &str) {
        self.py.insert_text(text);
        if self.py.screen == PyScreen::Editor {
            self.py.fix_editor_scroll();
        }
    }

    fn py_softkey(&mut self, n: usize) {
        let py = &mut self.py;
        match (py.screen, n) {
            (PyScreen::Files, 0) => {
                if !py.scripts.is_empty() {
                    let i = py.file_sel;
                    py.run_script(i);
                }
            }
            (PyScreen::Files, 1) => {
                if !py.scripts.is_empty() {
                    py.edit_idx = py.file_sel;
                    py.row = 0;
                    py.col = 0;
                    py.scroll = 0;
                    py.screen = PyScreen::Editor;
                }
            }
            (PyScreen::Files, 2) => {
                py.name.clear();
                py.screen = PyScreen::NewName;
                self.alpha = AlphaState::Lock;
            }
            (PyScreen::Files, 3) => py.screen = PyScreen::Shell,
            (PyScreen::Files, 4) => {
                if !py.scripts.is_empty() {
                    self.menu = Some(menus::py_manage());
                }
            }
            (PyScreen::Shell | PyScreen::Editor, 0) => self.menu = Some(menus::py_fns()),
            (PyScreen::Shell | PyScreen::Editor, 1) => {
                // Cycle: lowercase lock → uppercase lock → off.
                if self.alpha == AlphaState::Lock && py.lower {
                    py.lower = false;
                } else if self.alpha == AlphaState::Lock {
                    self.alpha = AlphaState::Off;
                    py.lower = true;
                } else {
                    self.alpha = AlphaState::Lock;
                    py.lower = true;
                }
            }
            (PyScreen::Shell, 2) => self.menu = Some(menus::py_shell_tools()),
            (PyScreen::Editor, 2) => self.menu = Some(menus::py_editor_tools()),
            (PyScreen::Shell, 3) => {
                if !py.scripts.is_empty() {
                    py.edit_idx = py.edit_idx.min(py.scripts.len() - 1);
                    py.screen = PyScreen::Editor;
                    py.fix_editor_scroll();
                }
            }
            (PyScreen::Editor, 3) => {
                let i = py.edit_idx;
                py.run_script(i);
            }
            (PyScreen::Shell, 4) => py.screen = PyScreen::Files,
            (PyScreen::Editor, 4) => {
                py.save(py.edit_idx);
                py.screen = PyScreen::Files;
            }
            (PyScreen::NewName, 3) => self.py_new_ok(),
            (PyScreen::NewName, 4) => {
                py.screen = PyScreen::Files;
                self.alpha = AlphaState::Off;
            }
            _ => {}
        }
    }

    fn py_new_ok(&mut self) {
        let py = &mut self.py;
        if py.name.is_empty() {
            return;
        }
        let name = py.name.clone();
        let idx = match py.scripts.iter().position(|s| s.name == name) {
            Some(i) => i,
            None => {
                py.scripts.push(Script { name: name.clone(), lines: vec![String::new()] });
                py.scripts.sort_by(|a, b| a.name.cmp(&b.name));
                py.scripts.iter().position(|s| s.name == name).unwrap()
            }
        };
        py.save(idx);
        py.file_sel = idx;
        py.edit_idx = idx;
        py.row = 0;
        py.col = 0;
        py.scroll = 0;
        py.screen = PyScreen::Editor;
        self.alpha = AlphaState::Off;
    }

    pub(super) fn py_cmd(&mut self, c: Cmd) {
        let py = &mut self.py;
        match c {
            Cmd::PyReinit => {
                if let Some(w) = &py.worker {
                    w.reset();
                }
                py.shell_open = false;
                py.print("*** Shell Reinitialized ***\n");
            }
            Cmd::PyClearShell => {
                py.shell.clear();
                py.shell_open = false;
            }
            Cmd::PyDeleteScript => {
                if py.file_sel < py.scripts.len() {
                    let s = py.scripts.remove(py.file_sel);
                    py.delete_file(&s.name);
                    py.file_sel = py.file_sel.min(py.scripts.len().saturating_sub(1));
                }
            }
            _ => {
                let Some(s) = py.scripts.get_mut(py.edit_idx) else { return };
                match c {
                    Cmd::PyIndentIn => {
                        s.lines[py.row].insert_str(0, "  ");
                        py.col += 2;
                    }
                    Cmd::PyIndentOut => {
                        let n = indent_of(&s.lines[py.row]).min(2);
                        s.lines[py.row].drain(..n);
                        py.col = py.col.saturating_sub(n);
                    }
                    Cmd::PyInsertLineAbove => {
                        let ind = indent_of(&s.lines[py.row]);
                        s.lines.insert(py.row, " ".repeat(ind));
                        py.col = ind;
                    }
                    Cmd::PyCutLine => {
                        py.clip = Some(s.lines.remove(py.row));
                        if s.lines.is_empty() {
                            s.lines.push(String::new());
                        }
                        py.row = py.row.min(s.lines.len() - 1);
                        py.col = 0;
                    }
                    Cmd::PyPasteLine => {
                        if let Some(l) = py.clip.clone() {
                            s.lines.insert(py.row + 1, l);
                            py.row += 1;
                            py.col = 0;
                        }
                    }
                    _ => {}
                }
                py.fix_editor_scroll();
                py.save(py.edit_idx);
            }
        }
    }

    pub(super) fn py_key(&mut self, key: Key, m: Mods) {
        let soft = [Key::YEq, Key::Window, Key::Zoom, Key::Trace, Key::Graph];
        if let Some(n) = soft.iter().position(|&k| k == key) {
            if !m.second {
                self.py_softkey(n);
            }
            return;
        }
        if key == Key::Math && !m.alpha {
            if matches!(self.py.screen, PyScreen::Shell | PyScreen::Editor) {
                self.menu = Some(if m.second { menus::py_test() } else { menus::py_fns() });
            }
            return;
        }
        let py_screen = self.py.screen;
        match py_screen {
            PyScreen::Files => {
                let py = &mut self.py;
                let n = py.scripts.len();
                match key {
                    Key::Up if n > 0 => py.file_sel = (py.file_sel + n - 1) % n,
                    Key::Down if n > 0 => py.file_sel = (py.file_sel + 1) % n,
                    Key::Enter => self.py_softkey(1),
                    Key::Clear => self.leave_python(),
                    _ => {}
                }
            }
            PyScreen::NewName => match key {
                Key::Enter => self.py_new_ok(),
                Key::Del => {
                    self.py.name.pop();
                }
                Key::Clear => self.py.name.clear(),
                _ => {
                    let m2 = Mods { second: m.second, alpha: m.alpha || self.alpha != AlphaState::Off };
                    let saved = self.py.lower;
                    self.py.lower = false;
                    if let Some(t) = self.py_key_text(key, m2) {
                        self.py_type(t);
                    }
                    self.py.lower = saved;
                }
            },
            PyScreen::Shell => {
                let py = &mut self.py;
                match key {
                    Key::Enter => py.shell_enter(),
                    Key::Del => {
                        if py.icur > 0 {
                            let b = byte_idx(&py.input, py.icur - 1);
                            py.input.remove(b);
                            py.icur -= 1;
                        }
                    }
                    Key::Clear => {
                        py.input.clear();
                        py.icur = 0;
                    }
                    Key::Left => {
                        py.icur = if m.second { 0 } else { py.icur.saturating_sub(1) };
                    }
                    Key::Right => {
                        let n = py.input.chars().count();
                        py.icur = if m.second { n } else { (py.icur + 1).min(n) };
                    }
                    Key::Up | Key::Down if !py.busy() && !py.history.is_empty() => {
                        let n = py.history.len();
                        let idx = match (py.hist_idx, key) {
                            (None, Key::Up) => Some(n - 1),
                            (None, _) => None,
                            (Some(0), Key::Up) => Some(0),
                            (Some(i), Key::Up) => Some(i - 1),
                            (Some(i), _) if i + 1 < n => Some(i + 1),
                            _ => None,
                        };
                        py.hist_idx = idx;
                        py.input = idx.map(|i| py.history[i].replace('\n', " ")).unwrap_or_default();
                        // Multi-line history entries are re-run as a block.
                        if let Some(i) = idx {
                            if py.history[i].contains('\n') {
                                py.input = py.history[i].lines().next().unwrap_or("").to_string();
                            }
                        }
                        py.icur = py.input.chars().count();
                    }
                    _ => {
                        if let Some(t) = self.py_key_text(key, m) {
                            self.py_type(t);
                        }
                    }
                }
            }
            PyScreen::Editor => {
                let py = &mut self.py;
                let Some(s) = py.scripts.get(py.edit_idx) else { return };
                let nlines = s.lines.len();
                let len = |py: &PyApp, r: usize| py.scripts[py.edit_idx].lines[r].chars().count();
                match key {
                    Key::Enter => py.editor_newline(),
                    Key::Del => py.editor_backspace(),
                    Key::Clear => {
                        let ind = indent_of(&py.scripts[py.edit_idx].lines[py.row]);
                        py.scripts[py.edit_idx].lines[py.row] = " ".repeat(ind);
                        py.col = ind;
                    }
                    Key::Up => {
                        if m.second {
                            py.row = py.row.saturating_sub(ROWS - 1);
                        } else if py.row > 0 {
                            py.row -= 1;
                        }
                        py.col = py.col.min(len(py, py.row));
                    }
                    Key::Down => {
                        if m.second {
                            py.row = (py.row + ROWS - 1).min(nlines - 1);
                        } else if py.row + 1 < nlines {
                            py.row += 1;
                        }
                        py.col = py.col.min(len(py, py.row));
                    }
                    Key::Left => {
                        if m.second {
                            py.col = 0;
                        } else if py.col > 0 {
                            py.col -= 1;
                        } else if py.row > 0 {
                            py.row -= 1;
                            py.col = len(py, py.row);
                        }
                    }
                    Key::Right => {
                        let l = len(py, py.row);
                        if m.second {
                            py.col = l;
                        } else if py.col < l {
                            py.col += 1;
                        } else if py.row + 1 < nlines {
                            py.row += 1;
                            py.col = 0;
                        }
                    }
                    _ => {
                        if let Some(t) = self.py_key_text(key, m) {
                            self.py_type(t);
                        }
                    }
                }
                self.py.fix_editor_scroll();
                if matches!(key, Key::Enter | Key::Del | Key::Clear) {
                    self.py.save(self.py.edit_idx);
                }
            }
        }
    }

    /// Paste (possibly multi-line) text into the shell or editor verbatim.
    pub(super) fn py_paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\t', "    ");
        let lines: Vec<&str> = text.split('\n').collect();
        match self.py.screen {
            PyScreen::Editor => {
                for (i, l) in lines.iter().enumerate() {
                    if i > 0 {
                        // Split without auto-indent: the pasted text carries its own.
                        let py = &mut self.py;
                        let s = &mut py.scripts[py.edit_idx];
                        let line = s.lines[py.row].clone();
                        let b = byte_idx(&line, py.col);
                        s.lines[py.row] = line[..b].to_string();
                        s.lines.insert(py.row + 1, line[b..].to_string());
                        py.row += 1;
                        py.col = 0;
                    }
                    self.py.insert_text(l);
                }
                self.py.fix_editor_scroll();
                self.py.save(self.py.edit_idx);
            }
            PyScreen::Shell => {
                for (i, l) in lines.iter().enumerate() {
                    if i > 0 {
                        self.py.shell_enter();
                        self.py.input.clear();
                        self.py.icur = 0;
                    }
                    self.py.insert_text(l);
                }
            }
            _ => self.py.insert_text(&lines.join("")),
        }
    }

    pub(super) fn py_poll(&mut self) -> bool {
        let Some(w) = self.py.worker.as_mut() else { return false };
        let events = w.poll();
        if events.is_empty() {
            return false;
        }
        for e in events {
            match e {
                PyEvent::Output(s) => self.py.print(&s),
                PyEvent::InputRequest(prompt) => {
                    self.py.print(&prompt);
                    if !self.py.shell_open {
                        self.py.shell.push(String::new());
                        self.py.shell_open = true;
                    }
                    self.py.awaiting_input = true;
                    self.py.input.clear();
                    self.py.icur = 0;
                }
                PyEvent::Done => {
                    self.py.awaiting_input = false;
                    if self.py.shell_open {
                        self.py.shell_open = false;
                    }
                }
            }
        }
        true
    }
}
