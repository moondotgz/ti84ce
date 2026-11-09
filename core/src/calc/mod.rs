//! The calculator's operating system: screens, key handling and state.

pub mod editor;
pub mod graph;
pub mod home;
pub mod menus;
pub mod pyapp;
pub mod screens;
pub mod table;
pub mod view;

#[cfg(test)]
mod tests;

use crate::engine::format::{NumFormat, Notation};
use crate::engine::{parse_expr, CalcError, Env, Tok};
use crate::keys::Key;
use editor::{CursorKind, LineEditor};
use graph::{GraphState, WindowVals};
use home::Home;
use menus::{Act, Cmd, Menu, MenuResult};
use pyapp::PyApp;
use screens::{FieldKind, FieldScreen, OptScreen};
use table::{ListEditor, TableState};
use view::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaState {
    Off,
    Once,
    Lock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mods {
    pub second: bool,
    pub alpha: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answers {
    Auto,
    Dec,
    Frac,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub fmt: NumFormat,
    pub thick: bool,
    pub dots: bool,
    pub answers: Answers,
    pub simul: bool,
    pub stat_diag: bool,
    pub coord_on: bool,
    pub grid: Grid,
    pub axes_on: bool,
    pub labels_on: bool,
    pub expr_on: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            fmt: NumFormat::default(),
            thick: true,
            dots: false,
            answers: Answers::Auto,
            simul: false,
            stat_diag: false,
            coord_on: true,
            grid: Grid::Off,
            axes_on: true,
            labels_on: false,
            expr_on: true,
        }
    }
}

/// Where an error's "Goto" option should return.
#[derive(Debug, Clone)]
pub enum ErrFrom {
    Home(Vec<Tok>),
    YEq(usize),
    Field(FieldKind, usize, Vec<Tok>),
    None,
}

#[derive(Debug, Clone)]
pub struct ErrState {
    pub err: CalcError,
    pub from: ErrFrom,
    pub sel: usize,
}

#[derive(Debug, Clone)]
pub enum Screen {
    Home,
    Mode(OptScreen),
    Format(OptScreen),
    YEq,
    Fields(FieldScreen),
    Graph,
    Table,
    StatEdit,
    Output { title: String, lines: Vec<String>, scroll: usize },
    Error(ErrState),
    About,
    Python,
}

pub struct YEqState {
    pub funcs: Vec<LineEditor>,
    pub enabled: [bool; 10],
    pub cur: usize,
    pub on_eq: bool,
    pub scroll: usize,
}

pub struct Calc {
    pub env: Env,
    pub settings: Settings,
    pub second: bool,
    pub alpha: AlphaState,
    pub screen: Screen,
    pub menu: Option<Menu>,
    pub home: Home,
    pub yeq: YEqState,
    pub window: WindowVals,
    pub zoom_prev: Option<WindowVals>,
    pub zoom_saved: Option<WindowVals>,
    pub graph: GraphState,
    pub table: TableState,
    pub lists: ListEditor,
    pub py: PyApp,
    pub off: bool,
    pub blink: bool,
}

impl Default for Calc {
    fn default() -> Self {
        Self::new()
    }
}

impl Calc {
    pub fn new() -> Self {
        Self::with_python(PyApp::new())
    }

    /// A calculator whose Python scripts live in `dir` (e.g. an iOS app's
    /// Documents folder). `None` keeps scripts in memory only.
    pub fn with_python_dir(dir: Option<std::path::PathBuf>) -> Self {
        Self::with_python(PyApp::new_in(dir))
    }

    fn with_python(py: PyApp) -> Self {
        Calc {
            env: Env::default(),
            settings: Settings::default(),
            second: false,
            alpha: AlphaState::Off,
            screen: Screen::Home,
            menu: None,
            home: Home::default(),
            yeq: YEqState {
                funcs: vec![LineEditor::default(); 10],
                enabled: [true; 10],
                cur: 0,
                on_eq: false,
                scroll: 0,
            },
            window: WindowVals::standard(),
            zoom_prev: None,
            zoom_saved: None,
            graph: GraphState::default(),
            table: TableState::default(),
            lists: ListEditor::default(),
            py,
            off: false,
            blink: true,
        }
    }

    pub fn fmt(&self) -> NumFormat {
        self.settings.fmt
    }

    /// The cursor style implied by the current modifier state.
    pub fn cursor_kind(&self, insert: bool) -> CursorKind {
        if self.second {
            CursorKind::Second
        } else if self.alpha != AlphaState::Off {
            CursorKind::Alpha
        } else if insert {
            CursorKind::Insert
        } else {
            CursorKind::Block
        }
    }

    /// Feed a physical key press to the calculator.
    pub fn press(&mut self, key: Key) {
        self.blink = true;
        if self.off {
            if key == Key::On {
                self.off = false;
                self.second = false;
            }
            return;
        }
        if key == Key::Second {
            self.second = !self.second;
            return;
        }
        if key == Key::Alpha {
            if matches!(self.screen, Screen::Python) && self.menu.is_none() {
                self.py_alpha_key();
                return;
            }
            if self.second {
                self.second = false;
                self.alpha = AlphaState::Lock;
            } else {
                self.alpha = match self.alpha {
                    AlphaState::Off => AlphaState::Once,
                    _ => AlphaState::Off,
                };
            }
            return;
        }
        let m = Mods { second: self.second, alpha: self.alpha != AlphaState::Off && !self.second };
        self.second = false;
        if self.alpha == AlphaState::Once {
            self.alpha = AlphaState::Off;
        }
        // Arrow keys and editing keys cancel a one-shot alpha but keep lock.
        if self.alpha == AlphaState::Lock
            && matches!(key, Key::Enter | Key::Clear | Key::Mode | Key::YEq | Key::Window | Key::Zoom | Key::Trace | Key::Graph)
            && !matches!(self.screen, Screen::Python)
        {
            self.alpha = AlphaState::Off;
        }

        if key == Key::On {
            if m.second {
                self.off = true;
                self.menu = None;
            } else if matches!(self.screen, Screen::Python) {
                self.py.interrupt();
            }
            return;
        }

        if self.menu.is_some() {
            self.menu_key(key, m);
            return;
        }

        if matches!(self.screen, Screen::Python) {
            if key == Key::Mode && m.second {
                self.leave_python();
            } else {
                self.py_key(key, m);
            }
            return;
        }

        if let Screen::Error(_) = self.screen {
            self.error_key(key, m);
            return;
        }

        if self.global_key(key, m) {
            return;
        }

        match self.screen.clone() {
            Screen::Home => self.home_key(key, m),
            Screen::Mode(_) | Screen::Format(_) => self.opt_key(key, m),
            Screen::YEq => self.yeq_key(key, m),
            Screen::Fields(_) => self.field_key(key, m),
            Screen::Graph => self.graph_key(key, m),
            Screen::Table => self.table_key(key, m),
            Screen::StatEdit => self.statedit_key(key, m),
            Screen::Output { .. } => self.output_key(key, m),
            Screen::About => {
                if matches!(key, Key::Clear | Key::Enter) {
                    self.goto_home();
                }
            }
            Screen::Error(_) | Screen::Python => {}
        }
    }

    /// Type a character from the computer keyboard.
    pub fn type_char(&mut self, c: char) {
        if self.off || self.menu.is_some() {
            return;
        }
        if matches!(self.screen, Screen::Python) {
            self.py_type(&c.to_string());
            return;
        }
        let tok: Option<Tok> = match c {
            'a'..='z' | 'A'..='Z' => crate::engine::intern(&c.to_ascii_uppercase().to_string()),
            '0'..='9' | '.' | '+' | '*' | '/' | '^' | '(' | ')' | ',' | '{' | '}' | '=' | '<' | '>' | '!' | ':' | '"' | '?' => {
                crate::engine::intern(&c.to_string())
            }
            ' ' => Some(" "),
            '-' => {
                // Negation at the start of an operand, subtraction otherwise.
                let prev = self.active_editor().and_then(|e| e.toks[..e.cur].last().copied());
                let operand_start = match prev {
                    None => !matches!(self.screen, Screen::Home),
                    Some(t) => {
                        matches!(t, "(" | "," | "{" | "+" | "-" | "*" | "/" | "^" | "=" | "→" | "⁻" | ":")
                            || crate::engine::is_func(t)
                    }
                };
                Some(if operand_start { "⁻" } else { "-" })
            }
            '~' => Some("⁻"),
            '[' => Some("["),
            ']' => Some("]"),
            _ => None,
        };
        let Some(tok) = tok else { return };
        self.second = false;
        if self.alpha == AlphaState::Once {
            self.alpha = AlphaState::Off;
        }
        if let Screen::Graph = self.screen {
            self.graph_type(tok);
            return;
        }
        if let Screen::Table = self.screen {
            return;
        }
        if matches!(self.screen, Screen::Error(_) | Screen::About | Screen::Output { .. }) {
            if let Some(d) = c.to_digit(10) {
                self.press(Key::from_digit(d as u8));
            }
            return;
        }
        if self.active_editor().is_none() {
            self.goto_home();
        }
        self.insert_tokens(&[tok]);
        if tok == "(" {
            if let Some(ed) = self.active_editor() {
                editor::fold_function_name(ed);
            }
            if matches!(self.screen, Screen::YEq) {
                self.sync_y(self.yeq.cur);
            }
        }
    }

    /// Backspace from the computer keyboard: delete the token left of the cursor.
    pub fn keyboard_backspace(&mut self) {
        if self.off || self.menu.is_some() {
            return;
        }
        if matches!(self.screen, Screen::Python) {
            self.press(Key::Del);
            return;
        }
        if let Screen::Graph = self.screen {
            if let Some(ed) = self.graph.input.as_mut() {
                ed.backspace();
            }
            return;
        }
        if let Screen::Fields(f) = &mut self.screen {
            f.fresh = false;
        }
        if let Screen::StatEdit = self.screen {
            if !self.lists.editing {
                return;
            }
        }
        let yeq = matches!(self.screen, Screen::YEq);
        if let Some(ed) = self.active_editor() {
            ed.backspace();
            if yeq {
                self.sync_y(self.yeq.cur);
            }
        } else {
            self.press(Key::Del);
        }
    }

    /// Paste text from the system clipboard.
    pub fn paste(&mut self, text: &str) {
        if matches!(self.screen, Screen::Python) {
            self.py_paste(text);
            return;
        }
        for c in text.chars() {
            match c {
                '\n' | '\r' => {}
                c => self.type_char(c),
            }
        }
    }

    pub fn active_editor(&mut self) -> Option<&mut LineEditor> {
        match &mut self.screen {
            Screen::Home => Some(&mut self.home.editor),
            Screen::YEq => {
                if self.yeq.on_eq {
                    None
                } else {
                    Some(&mut self.yeq.funcs[self.yeq.cur])
                }
            }
            Screen::Fields(f) => Some(&mut f.ed),
            Screen::StatEdit => Some(&mut self.lists.ed),
            _ => None,
        }
    }

    pub fn insert_tokens(&mut self, toks: &[Tok]) {
        match &mut self.screen {
            Screen::Home => self.home_insert(toks),
            Screen::Fields(f) => {
                if f.fresh {
                    f.ed.clear();
                    f.fresh = false;
                }
                f.ed.put_all(toks);
            }
            Screen::StatEdit => self.lists_insert(toks),
            Screen::YEq => {
                if self.yeq.on_eq {
                    self.yeq.on_eq = false;
                    self.yeq.funcs[self.yeq.cur].home();
                }
                self.yeq.funcs[self.yeq.cur].put_all(toks);
                self.sync_y(self.yeq.cur);
            }
            _ => {}
        }
    }

    /// Token typed by a key in a calculator-mode editor.
    pub fn key_token(key: Key, m: Mods) -> Option<Tok> {
        use Key::*;
        if m.alpha {
            return key.alpha_char().and_then(crate::engine::intern);
        }
        if m.second {
            return Some(match key {
                Sin => "sin⁻¹(",
                Cos => "cos⁻¹(",
                Tan => "tan⁻¹(",
                Pow => "π",
                Sq => "√(",
                Comma => "ᴇ",
                LParen => "{",
                RParen => "}",
                Div => "ℯ",
                Log => "10^(",
                Mul => "[",
                Ln => "ℯ^(",
                Sub => "]",
                D1 => "L₁",
                D2 => "L₂",
                D3 => "L₃",
                D4 => "L₄",
                D5 => "L₅",
                D6 => "L₆",
                Dot => "i",
                Neg => "Ans",
                _ => return None,
            });
        }
        if let Some(d) = key.digit() {
            return Some(crate::engine::DIGITS[d as usize]);
        }
        Some(match key {
            Dot => ".",
            Neg => "⁻",
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Pow => "^",
            Sq => "²",
            Inv => "⁻¹",
            Sin => "sin(",
            Cos => "cos(",
            Tan => "tan(",
            Log => "log(",
            Ln => "ln(",
            LParen => "(",
            RParen => ")",
            Comma => ",",
            Sto => "→",
            Xtn => "X",
            _ => return None,
        })
    }

    fn has_editor(&self) -> bool {
        matches!(self.screen, Screen::Home | Screen::YEq | Screen::Fields(_) | Screen::StatEdit)
    }

    fn open_menu(&mut self, mut m: Menu, key: Key, second: bool, needs_editor: bool) {
        if needs_editor && !self.has_editor() {
            self.goto_home();
        }
        m.opener = Some((key, second));
        self.menu = Some(m);
    }

    /// Keys that switch screens or open menus from (almost) anywhere.
    fn global_key(&mut self, key: Key, m: Mods) -> bool {
        use Key::*;
        if m.alpha {
            return false;
        }
        match (key, m.second) {
            (YEq, false) => {
                self.leave_screen();
                self.screen = Screen::YEq;
            }
            (Window, false) => {
                self.leave_screen();
                self.screen = Screen::Fields(FieldScreen::new(FieldKind::Window, self));
            }
            (Window, true) => {
                self.leave_screen();
                self.screen = Screen::Fields(FieldScreen::new(FieldKind::TblSet, self));
            }
            (Zoom, false) => {
                self.leave_screen();
                self.open_menu(menus::zoom(), key, false, false);
            }
            (Zoom, true) => {
                self.leave_screen();
                self.screen = Screen::Format(OptScreen::default());
            }
            (Trace, false) => {
                self.leave_screen();
                self.show_graph();
                if matches!(self.screen, Screen::Graph) {
                    self.start_trace();
                }
            }
            (Trace, true) => {
                self.leave_screen();
                self.open_menu(menus::calc(), key, true, false);
            }
            (Graph, false) => {
                self.leave_screen();
                self.show_graph();
            }
            (Graph, true) => {
                self.leave_screen();
                self.show_table();
            }
            (Mode, false) => {
                self.leave_screen();
                self.screen = Screen::Mode(OptScreen::default());
            }
            (Mode, true) => {
                self.goto_home();
            }
            (Stat, false) => self.open_menu(menus::stat(), key, false, false),
            (Stat, true) => self.open_menu(menus::list(), key, true, true),
            (Math, false) => self.open_menu(menus::math(), key, false, true),
            (Math, true) => self.open_menu(menus::test(), key, true, true),
            (Apps, false) => self.open_menu(menus::apps(), key, false, false),
            (Apps, true) => self.open_menu(menus::angle(), key, true, true),
            (Vars, false) => self.open_menu(menus::vars(), key, false, true),
            (Vars, true) => self.open_menu(menus::distr(), key, true, true),
            (Add, true) => self.open_menu(menus::mem(), key, true, false),
            (D0, true) => self.open_menu(menus::catalog(), key, true, true),
            _ => return false,
        }
        true
    }

    /// Commit any in-progress edits before switching screens.
    fn leave_screen(&mut self) {
        match &self.screen {
            Screen::YEq => {
                for i in 0..10 {
                    self.sync_y(i);
                }
            }
            Screen::Fields(_) => {
                self.field_commit(false);
            }
            Screen::StatEdit => self.lists_commit(),
            _ => {}
        }
        self.graph.reset_mode();
    }

    pub fn goto_home(&mut self) {
        if !matches!(self.screen, Screen::Home | Screen::Error(_)) {
            self.leave_screen();
        }
        self.menu = None;
        self.screen = Screen::Home;
        self.home.hist_sel = None;
    }

    fn menu_key(&mut self, key: Key, m: Mods) {
        let Some(menu) = self.menu.as_mut() else { return };
        match menu.handle(key, m.second, m.alpha || menu.kind == menus::MenuKind::Catalog) {
            MenuResult::None => {}
            MenuResult::Close => self.menu = None,
            MenuResult::Select(act) => {
                self.menu = None;
                match act {
                    Act::Insert(toks) => {
                        if !self.has_editor() {
                            self.goto_home();
                        }
                        self.insert_tokens(&toks);
                    }
                    Act::Py(text) => self.py_type(text),
                    Act::Cmd(c) => self.run_cmd(c),
                }
            }
        }
    }

    fn run_cmd(&mut self, c: Cmd) {
        match c {
            Cmd::ZoomIn
            | Cmd::ZoomOut
            | Cmd::ZDecimal
            | Cmd::ZSquare
            | Cmd::ZStandard
            | Cmd::ZTrig
            | Cmd::ZInteger
            | Cmd::ZoomFit
            | Cmd::ZPrevious
            | Cmd::ZoomRcl => {
                self.zoom(c);
                self.show_graph();
            }
            Cmd::ZoomSto => {
                self.zoom_saved = Some(self.window.clone());
                self.show_graph();
            }
            Cmd::CalcValue
            | Cmd::CalcZero
            | Cmd::CalcMin
            | Cmd::CalcMax
            | Cmd::CalcIntersect
            | Cmd::CalcDeriv
            | Cmd::CalcIntegral => self.start_calc_task(c),
            Cmd::YVarsFunction => {
                let mut m = menus::yvars_function();
                m.opener = None;
                self.menu = Some(m);
            }
            Cmd::PythonApp => self.enter_python(),
            Cmd::About => self.screen = Screen::About,
            Cmd::ClearEntries => {
                self.home.entries.clear();
                self.goto_home();
                self.home.push_message("Done");
            }
            Cmd::ClrAllLists => {
                for l in self.env.lists.iter_mut() {
                    l.clear();
                }
                self.goto_home();
                self.home.push_message("Done");
            }
            Cmd::ResetMenu => self.menu = Some(menus::reset()),
            Cmd::ResetAll => {
                let py = std::mem::replace(&mut self.py, PyApp::placeholder());
                *self = Calc::new_with_py(py);
                self.py.reset();
                self.home.push_message("RAM Cleared");
            }
            Cmd::Cancel => {}
            Cmd::StatEdit => {
                self.leave_screen();
                self.lists.enter();
                self.screen = Screen::StatEdit;
            }
            Cmd::PyReinit
            | Cmd::PyClearShell
            | Cmd::PyIndentIn
            | Cmd::PyIndentOut
            | Cmd::PyInsertLineAbove
            | Cmd::PyCutLine
            | Cmd::PyPasteLine
            | Cmd::PyDeleteScript => self.py_cmd(c),
        }
    }

    fn new_with_py(py: PyApp) -> Self {
        let mut c = Calc::new();
        c.py = py;
        c
    }

    pub fn show_error(&mut self, err: CalcError, from: ErrFrom) {
        self.menu = None;
        self.graph.reset_mode();
        self.screen = Screen::Error(ErrState { err, from, sel: 0 });
    }

    fn error_key(&mut self, key: Key, _m: Mods) {
        let Screen::Error(st) = &mut self.screen else { return };
        let can_goto = !matches!(st.from, ErrFrom::None);
        let n = if can_goto { 2 } else { 1 };
        let choice = match key {
            Key::Up => {
                st.sel = (st.sel + n - 1) % n;
                None
            }
            Key::Down => {
                st.sel = (st.sel + 1) % n;
                None
            }
            Key::Enter => Some(st.sel),
            Key::D1 => Some(0),
            Key::D2 if can_goto => Some(1),
            Key::Clear => Some(0),
            _ => None,
        };
        let Some(choice) = choice else { return };
        let st = st.clone();
        if choice == 0 {
            self.screen = Screen::Home;
            return;
        }
        let pos = st.err.goto_pos();
        match st.from {
            ErrFrom::Home(toks) => {
                self.screen = Screen::Home;
                self.home.editor.set(toks);
                if let Some(p) = pos {
                    self.home.editor.cur = p.min(self.home.editor.toks.len());
                }
            }
            ErrFrom::YEq(i) => {
                self.screen = Screen::YEq;
                self.yeq.cur = i;
                self.yeq.on_eq = false;
                let ed = &mut self.yeq.funcs[i];
                ed.cur = pos.unwrap_or(0).min(ed.toks.len());
            }
            ErrFrom::Field(kind, i, toks) => {
                let mut fs = FieldScreen::new(kind, self);
                fs.cur = i;
                fs.ed.set(toks);
                fs.fresh = false;
                if let Some(p) = pos {
                    fs.ed.cur = p.min(fs.ed.toks.len());
                }
                self.screen = Screen::Fields(fs);
            }
            ErrFrom::None => self.screen = Screen::Home,
        }
    }

    // ----- Y= editor -----

    pub fn sync_y(&mut self, i: usize) {
        let toks = &self.yeq.funcs[i].toks;
        self.env.yfuncs[i] = if toks.is_empty() { None } else { parse_expr(toks).ok() };
    }

    /// Enabled, non-empty function indices.
    pub fn active_funcs(&self) -> Vec<usize> {
        (0..10).filter(|&i| self.yeq.enabled[i] && !self.yeq.funcs[i].is_empty()).collect()
    }

    fn yeq_key(&mut self, key: Key, m: Mods) {
        let y = &mut self.yeq;
        match key {
            Key::Up => {
                let ed = &mut y.funcs[y.cur];
                if y.on_eq || !ed.vertical(-1, COLS, 4) {
                    if y.cur > 0 {
                        self.sync_y(self.yeq.cur);
                        self.yeq.cur -= 1;
                        let ed = &mut self.yeq.funcs[self.yeq.cur];
                        ed.cur = ed.cur.min(ed.toks.len());
                    }
                }
            }
            Key::Down | Key::Enter => {
                let ed = &mut y.funcs[y.cur];
                if key == Key::Enter || y.on_eq || !ed.vertical(1, COLS, 4) {
                    if key == Key::Enter && y.on_eq {
                        y.enabled[y.cur] = !y.enabled[y.cur];
                        return;
                    }
                    if y.cur < 9 {
                        self.sync_y(self.yeq.cur);
                        self.yeq.cur += 1;
                        self.yeq.on_eq = false;
                        let ed = &mut self.yeq.funcs[self.yeq.cur];
                        ed.cur = 0;
                    }
                }
            }
            Key::Left => {
                if m.second {
                    y.funcs[y.cur].home();
                } else if y.on_eq {
                } else if y.funcs[y.cur].cur == 0 {
                    y.on_eq = true;
                } else {
                    y.funcs[y.cur].left();
                }
            }
            Key::Right => {
                if y.on_eq {
                    y.on_eq = false;
                    y.funcs[y.cur].cur = 0;
                } else if m.second {
                    y.funcs[y.cur].end();
                } else {
                    y.funcs[y.cur].right();
                }
            }
            Key::Clear => {
                y.funcs[y.cur].clear();
                y.on_eq = false;
                self.sync_y(self.yeq.cur);
            }
            Key::Del => {
                if m.second {
                    let ed = &mut y.funcs[y.cur];
                    ed.insert = !ed.insert;
                } else if !y.on_eq {
                    y.funcs[y.cur].del();
                    self.sync_y(self.yeq.cur);
                }
            }
            _ => {
                if let Some(t) = Self::key_token(key, m) {
                    if t == "→" {
                        return;
                    }
                    self.insert_tokens(&[t]);
                }
            }
        }
    }

    fn yeq_rows(&mut self) -> Vec<Row> {
        let mut all: Vec<Row> = Vec::new();
        let mut cursor_row = 0;
        let kind = self.cursor_kind(self.yeq.funcs[self.yeq.cur].insert);
        for i in 0..10 {
            let ed = &self.yeq.funcs[i];
            let enabled = self.yeq.enabled[i];
            let is_cur = i == self.yeq.cur;
            let eq_cursor = is_cur && self.yeq.on_eq;
            let eq_span = if eq_cursor && self.blink {
                Span::new("=").fg(color::WHITE).bg(color::BLACK)
            } else if enabled && !ed.is_empty() {
                Span::new("=").fg(color::WHITE).bg(Y_COLORS[i])
            } else {
                Span::new("=")
            };
            let prefix = vec![
                Span::new("▪").fg(Y_COLORS[i]),
                Span::new(crate::engine::YVARS[i]).fg(color::BLACK),
                eq_span,
            ];
            let cursor = if is_cur && !self.yeq.on_eq { Some((ed.char_pos(), kind, self.blink)) } else { None };
            let rows = editor::wrap_with_cursor(&prefix, &ed.text(), cursor, color::BLACK);
            if is_cur {
                cursor_row = all.len() + if self.yeq.on_eq { 0 } else { (ed.char_pos() + 4) / COLS };
            }
            all.extend(rows);
        }
        let vis = ROWS - 1;
        if cursor_row < self.yeq.scroll {
            self.yeq.scroll = cursor_row;
        } else if cursor_row >= self.yeq.scroll + vis {
            self.yeq.scroll = cursor_row + 1 - vis;
        }
        let mut out = vec![vec![
            Span::new(" Plot1").fg(color::GRAY),
            Span::new("  Plot2").fg(color::GRAY),
            Span::new("  Plot3").fg(color::GRAY),
        ]];
        out.extend(all.into_iter().skip(self.yeq.scroll).take(vis));
        out
    }

    // ----- output screens -----

    fn output_key(&mut self, key: Key, _m: Mods) {
        let Screen::Output { lines, scroll, .. } = &mut self.screen else { return };
        let vis = ROWS - 1;
        match key {
            Key::Up => *scroll = scroll.saturating_sub(1),
            Key::Down => {
                if *scroll + vis < lines.len() {
                    *scroll += 1;
                }
            }
            Key::Clear | Key::Enter => self.goto_home(),
            _ => {}
        }
    }

    // ----- view -----

    pub fn status(&self) -> Status {
        let fmt = &self.settings.fmt;
        let notation = match fmt.notation {
            Notation::Normal => "NORMAL",
            Notation::Sci => "SCI",
            Notation::Eng => "ENG",
        };
        let float = match fmt.fix {
            None => "FLOAT".to_string(),
            Some(n) => format!("FIX{n}"),
        };
        let ans = match self.settings.answers {
            Answers::Auto => "AUTO",
            Answers::Dec => "DEC",
            Answers::Frac => "FRAC",
        };
        let angle = if self.env.degree { "DEGREE" } else { "RADIAN" };
        let text = if matches!(self.screen, Screen::Python) {
            self.py.title()
        } else {
            format!("{notation} {float} {ans} REAL {angle} CL")
        };
        let python = matches!(self.screen, Screen::Python);
        let indicator = if self.second {
            Indicator::Second
        } else {
            match (self.alpha, python && self.py.lower) {
                (AlphaState::Off, _) => Indicator::None,
                (AlphaState::Once, false) => Indicator::Alpha,
                (AlphaState::Lock, false) => Indicator::AlphaLock,
                (AlphaState::Once, true) => Indicator::Lower,
                (AlphaState::Lock, true) => Indicator::LowerLock,
            }
        };
        Status { text, indicator, busy: python && self.py.busy() }
    }

    pub fn view(&mut self) -> ViewModel {
        let status = self.status();
        if self.off {
            return ViewModel { status, body: Body::Off, softkeys: None };
        }
        if let Some(menu) = &self.menu {
            let softkeys = if matches!(self.screen, Screen::Python) { Some(self.py.softkeys()) } else { None };
            return ViewModel { status, body: Body::Text(menu.rows()), softkeys };
        }
        let body = match self.screen.clone() {
            Screen::Home => Body::Text(self.home_rows()),
            Screen::Mode(o) => Body::Text(self.mode_rows(&o)),
            Screen::Format(o) => Body::Text(self.format_rows(&o)),
            Screen::YEq => Body::Text(self.yeq_rows()),
            Screen::Fields(f) => Body::Text(self.field_rows(&f)),
            Screen::Graph => Body::Graph(self.graph_model()),
            Screen::Table => Body::Text(self.table_rows()),
            Screen::StatEdit => Body::Text(self.statedit_rows()),
            Screen::Output { title, lines, scroll } => {
                let mut rows = vec![plain(title)];
                for l in lines.iter().skip(scroll).take(ROWS - 1) {
                    rows.push(plain(l.clone()));
                }
                Body::Text(rows)
            }
            Screen::Error(st) => Body::Text(self.error_rows(&st)),
            Screen::About => Body::Text(vec![
                plain("TI-84 Plus CE-T"),
                plain("Python Edition"),
                plain("(a Rust + GPUI clone)"),
                plain(""),
                plain(format!("Version {}", env!("CARGO_PKG_VERSION"))),
                plain(""),
                plain("Math engine, graphing, tables,"),
                plain("lists, stats and a Python app."),
                plain(""),
                plain("Press [clear] to exit.").into_iter().map(|s| s.fg(color::GRAY)).collect(),
            ]),
            Screen::Python => {
                let rows = self.py.rows(self.blink, self.cursor_kind(false));
                return ViewModel { status, body: Body::Text(rows), softkeys: Some(self.py.softkeys()) };
            }
        };
        ViewModel { status, body, softkeys: None }
    }

    fn error_rows(&self, st: &ErrState) -> Vec<Row> {
        let mut rows = vec![vec![Span::new(format!("ERROR: {}", st.err.title())).fg(color::RED)]];
        // Word-wrap the description.
        let mut line = String::new();
        for w in st.err.description().split(' ') {
            if line.chars().count() + w.chars().count() + 1 > COLS {
                rows.push(plain(std::mem::take(&mut line)).into_iter().map(|s| s.fg(color::DKGRAY)).collect());
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(w);
        }
        if !line.is_empty() {
            rows.push(plain(line).into_iter().map(|s| s.fg(color::DKGRAY)).collect());
        }
        rows.push(plain(""));
        let opts: &[&str] = if matches!(st.from, ErrFrom::None) { &["Quit"] } else { &["Quit", "Goto"] };
        for (i, o) in opts.iter().enumerate() {
            let num = format!("{}:", i + 1);
            if i == st.sel {
                rows.push(vec![Span::inverse(num), Span::new(*o)]);
            } else {
                rows.push(vec![Span::new(num), Span::new(*o)]);
            }
        }
        rows
    }

    /// Replay a whitespace-separated list of key names (see `keys::key_by_name`).
    /// Items starting with `"` are typed as text (`_` types a space), and
    /// `wait` pauses briefly to let the Python worker catch up.
    pub fn run_script(&mut self, script: &str) {
        for item in script.split_whitespace() {
            if let Some(text) = item.strip_prefix('"') {
                for c in text.chars() {
                    self.type_char(if c == '_' { ' ' } else { c });
                }
            } else if item == "wait" {
                for _ in 0..30 {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    self.tick();
                    if !self.py.busy() {
                        break;
                    }
                }
            } else if let Some(k) = crate::keys::key_by_name(item) {
                self.press(k);
            } else {
                eprintln!("unknown key in script: {item}");
            }
        }
    }

    /// Called periodically by the UI (cursor blink, Python output).
    pub fn tick(&mut self) -> bool {
        self.py_poll()
    }
}
