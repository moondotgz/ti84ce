//! Tabbed, numbered menus (MATH, TEST, ZOOM, CALC, …) and the CATALOG.

use super::view::{color, Row, Span, COLS, ROWS};
use crate::engine::{Tok, FUNCS, LETTERS, LISTS, YVARS};
use crate::keys::Key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    ZoomIn,
    ZoomOut,
    ZDecimal,
    ZSquare,
    ZStandard,
    ZTrig,
    ZInteger,
    ZoomFit,
    ZPrevious,
    ZoomSto,
    ZoomRcl,
    CalcValue,
    CalcZero,
    CalcMin,
    CalcMax,
    CalcIntersect,
    CalcDeriv,
    CalcIntegral,
    YVarsFunction,
    PythonApp,
    About,
    ClearEntries,
    ClrAllLists,
    ResetMenu,
    ResetAll,
    Cancel,
    StatEdit,
    // Python app
    PyReinit,
    PyClearShell,
    PyIndentIn,
    PyIndentOut,
    PyInsertLineAbove,
    PyCutLine,
    PyPasteLine,
    PyDeleteScript,
}

#[derive(Debug, Clone)]
pub enum Act {
    Insert(Vec<Tok>),
    Py(&'static str),
    Cmd(Cmd),
}

#[derive(Debug, Clone)]
pub struct Item {
    pub label: String,
    pub act: Act,
}

#[derive(Debug, Clone)]
pub struct Tab {
    pub title: &'static str,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    Normal,
    Catalog,
}

#[derive(Debug, Clone)]
pub struct Menu {
    pub kind: MenuKind,
    pub tabs: Vec<Tab>,
    pub tab: usize,
    pub sel: usize,
    pub scroll: usize,
    /// The key that opened the menu; pressing it again closes the menu.
    pub opener: Option<(Key, bool)>,
}

pub enum MenuResult {
    None,
    Close,
    Select(Act),
}

fn ins(label: &str, toks: &[Tok]) -> Item {
    Item { label: label.to_string(), act: Act::Insert(toks.to_vec()) }
}

fn tok(t: Tok) -> Item {
    ins(t.trim(), &[t])
}

fn cmd(label: &str, c: Cmd) -> Item {
    Item { label: label.to_string(), act: Act::Cmd(c) }
}

fn py(label: &str, text: &'static str) -> Item {
    Item { label: label.to_string(), act: Act::Py(text) }
}

fn index_char(i: usize) -> char {
    match i {
        0..=8 => (b'1' + i as u8) as char,
        9 => '0',
        _ => (b'A' + (i - 10) as u8) as char,
    }
}

impl Menu {
    pub fn new(tabs: Vec<Tab>) -> Self {
        Menu { kind: MenuKind::Normal, tabs, tab: 0, sel: 0, scroll: 0, opener: None }
    }

    fn items(&self) -> &[Item] {
        &self.tabs[self.tab].items
    }

    fn visible(&self) -> usize {
        ROWS - 1
    }

    fn fix_scroll(&mut self) {
        let vis = self.visible();
        if self.sel < self.scroll {
            self.scroll = self.sel;
        } else if self.sel >= self.scroll + vis {
            self.scroll = self.sel + 1 - vis;
        }
    }

    pub fn handle(&mut self, key: Key, second: bool, alpha: bool) -> MenuResult {
        let n = self.items().len();
        if let Some(op) = self.opener {
            if op == (key, second) {
                return MenuResult::Close;
            }
        }
        match key {
            Key::Clear => return MenuResult::Close,
            Key::Mode if second => return MenuResult::Close,
            Key::Up if n > 0 => {
                self.sel = if self.sel == 0 { n - 1 } else { self.sel - 1 };
            }
            Key::Down if n > 0 => {
                self.sel = (self.sel + 1) % n;
            }
            Key::Left | Key::Right if self.tabs.len() > 1 => {
                let t = self.tabs.len();
                self.tab = if key == Key::Left { (self.tab + t - 1) % t } else { (self.tab + 1) % t };
                self.sel = 0;
                self.scroll = 0;
            }
            Key::Enter if n > 0 => return MenuResult::Select(self.items()[self.sel].act.clone()),
            _ => {
                if self.kind == MenuKind::Catalog {
                    if let Some(letter) = key.alpha_char() {
                        let letter = letter.to_lowercase();
                        if let Some(pos) = self.items().iter().position(|it| {
                            it.label.trim_start_matches(|c: char| !c.is_alphanumeric()).to_lowercase().starts_with(&letter)
                        }) {
                            self.sel = pos;
                            self.scroll = pos;
                        }
                    }
                    return MenuResult::None;
                }
                let ch = if alpha {
                    key.alpha_char().and_then(|c| c.chars().next())
                } else {
                    key.digit().map(|d| (b'0' + d) as char)
                };
                if let Some(c) = ch {
                    if let Some(i) = (0..n).find(|&i| index_char(i) == c) {
                        self.sel = i;
                        return MenuResult::Select(self.items()[i].act.clone());
                    }
                }
            }
        }
        self.fix_scroll();
        MenuResult::None
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut header: Row = Vec::new();
        let mut used = 0;
        for (i, t) in self.tabs.iter().enumerate() {
            if i > 0 {
                header.push(Span::new(" "));
                used += 1;
            }
            used += t.title.chars().count();
            if i == self.tab {
                header.push(Span::inverse(t.title));
            } else {
                header.push(Span::new(t.title));
            }
        }
        let _ = used;
        rows.push(header);
        let items = self.items();
        let vis = self.visible();
        for (row, i) in (self.scroll..items.len().min(self.scroll + vis)).enumerate() {
            let it = &items[i];
            let selected = i == self.sel;
            if self.kind == MenuKind::Catalog {
                let pointer = if selected { "▸" } else { " " };
                rows.push(vec![Span::new(pointer).fg(color::SEL), Span::new(it.label.clone())]);
                continue;
            }
            let more_below = row == vis - 1 && i + 1 < items.len();
            let more_above = row == 0 && self.scroll > 0;
            let sep = if more_below {
                '↓'
            } else if more_above {
                '↑'
            } else {
                ':'
            };
            let num = format!("{}{}", index_char(i), sep);
            let label: String = it.label.chars().take(COLS - 2).collect();
            if selected {
                rows.push(vec![Span::inverse(num), Span::new(label)]);
            } else {
                rows.push(vec![Span::new(num), Span::new(label)]);
            }
        }
        rows
    }
}

pub fn math() -> Menu {
    Menu::new(vec![
        Tab {
            title: "MATH",
            items: vec![
                tok("►Frac"),
                tok("►Dec"),
                tok("³"),
                tok("³√("),
                ins("ˣ√", &["ˣ√"]),
                tok("nDeriv("),
                tok("fnInt("),
                tok("logBASE("),
            ],
        },
        Tab {
            title: "NUM",
            items: vec![
                tok("abs("),
                tok("round("),
                tok("iPart("),
                tok("fPart("),
                tok("int("),
                tok("min("),
                tok("max("),
                tok("lcm("),
                tok("gcd("),
                tok("remainder("),
                tok("floor("),
                tok("ceiling("),
            ],
        },
        Tab {
            title: "PROB",
            items: vec![tok("rand"), tok(" nPr "), tok(" nCr "), tok("!"), tok("randInt(")],
        },
    ])
}

pub fn test() -> Menu {
    Menu::new(vec![
        Tab {
            title: "TEST",
            items: vec![tok("="), tok("≠"), tok(">"), tok("≥"), tok("<"), tok("≤")],
        },
        Tab {
            title: "LOGIC",
            items: vec![tok(" and "), tok(" or "), tok(" xor "), tok("not(")],
        },
    ])
}

pub fn angle() -> Menu {
    Menu::new(vec![Tab { title: "ANGLE", items: vec![tok("°"), tok("ʳ")] }])
}

pub fn list() -> Menu {
    Menu::new(vec![
        Tab {
            title: "NAMES",
            items: LISTS.iter().map(|&l| tok(l)).collect(),
        },
        Tab {
            title: "OPS",
            items: vec![
                tok("SortA("),
                tok("SortD("),
                tok("dim("),
                tok("seq("),
                tok("cumSum("),
                tok("ΔList("),
                tok("augment("),
            ],
        },
        Tab {
            title: "MATH",
            items: vec![
                tok("min("),
                tok("max("),
                tok("mean("),
                tok("median("),
                tok("sum("),
                tok("prod("),
                tok("stdDev("),
                tok("variance("),
            ],
        },
    ])
}

pub fn distr() -> Menu {
    Menu::new(vec![Tab {
        title: "DISTR",
        items: vec![
            tok("normalpdf("),
            tok("normalcdf("),
            tok("invNorm("),
            tok("binompdf("),
            tok("binomcdf("),
            tok("poissonpdf("),
            tok("poissoncdf("),
            tok("geometpdf("),
            tok("geometcdf("),
        ],
    }])
}

pub fn vars() -> Menu {
    Menu::new(vec![Tab { title: "Y-VARS", items: vec![cmd("Function…", Cmd::YVarsFunction)] }])
}

pub fn yvars_function() -> Menu {
    Menu::new(vec![Tab { title: "FUNCTION", items: YVARS.iter().map(|&y| tok(y)).collect() }])
}

pub fn zoom() -> Menu {
    Menu::new(vec![
        Tab {
            title: "ZOOM",
            items: vec![
                cmd("Zoom In", Cmd::ZoomIn),
                cmd("Zoom Out", Cmd::ZoomOut),
                cmd("ZDecimal", Cmd::ZDecimal),
                cmd("ZSquare", Cmd::ZSquare),
                cmd("ZStandard", Cmd::ZStandard),
                cmd("ZTrig", Cmd::ZTrig),
                cmd("ZInteger", Cmd::ZInteger),
                cmd("ZoomFit", Cmd::ZoomFit),
            ],
        },
        Tab {
            title: "MEMORY",
            items: vec![
                cmd("ZPrevious", Cmd::ZPrevious),
                cmd("ZoomSto", Cmd::ZoomSto),
                cmd("ZoomRcl", Cmd::ZoomRcl),
            ],
        },
    ])
}

pub fn calc() -> Menu {
    Menu::new(vec![Tab {
        title: "CALCULATE",
        items: vec![
            cmd("value", Cmd::CalcValue),
            cmd("zero", Cmd::CalcZero),
            cmd("minimum", Cmd::CalcMin),
            cmd("maximum", Cmd::CalcMax),
            cmd("intersect", Cmd::CalcIntersect),
            cmd("dy/dx", Cmd::CalcDeriv),
            cmd("∫f(x)dx", Cmd::CalcIntegral),
        ],
    }])
}

pub fn apps() -> Menu {
    Menu::new(vec![Tab { title: "APPLICATIONS", items: vec![cmd("Python App", Cmd::PythonApp)] }])
}

pub fn mem() -> Menu {
    Menu::new(vec![Tab {
        title: "MEMORY",
        items: vec![
            cmd("About", Cmd::About),
            cmd("Clear Entries", Cmd::ClearEntries),
            cmd("ClrAllLists", Cmd::ClrAllLists),
            cmd("Reset…", Cmd::ResetMenu),
        ],
    }])
}

pub fn reset() -> Menu {
    Menu::new(vec![Tab {
        title: "RESET RAM",
        items: vec![cmd("No", Cmd::Cancel), cmd("Reset", Cmd::ResetAll)],
    }])
}

pub fn stat() -> Menu {
    Menu::new(vec![
        Tab {
            title: "EDIT",
            items: vec![cmd("Edit…", Cmd::StatEdit), tok("SortA("), tok("SortD(")],
        },
        Tab {
            title: "CALC",
            items: vec![
                ins("1-Var Stats", &["1-Var Stats "]),
                ins("2-Var Stats", &["2-Var Stats "]),
                ins("LinReg(ax+b)", &["LinReg(ax+b) "]),
            ],
        },
    ])
}

pub fn catalog() -> Menu {
    let mut items: Vec<Item> = FUNCS
        .iter()
        .map(|&f| tok(f))
        .chain(
            [
                "Ans", "►Frac", "►Dec", " and ", " or ", " xor ", " nPr ", " nCr ", "π", "ℯ",
                "rand", "1-Var Stats ", "2-Var Stats ", "LinReg(ax+b) ", "ClrHome",
            ]
            .iter()
            .map(|&t| tok(t)),
        )
        .chain(LISTS.iter().map(|&l| tok(l)))
        .collect();
    let _ = LETTERS;
    items.sort_by_key(|it| {
        it.label
            .trim_start_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase()
    });
    let mut m = Menu::new(vec![Tab { title: "CATALOG", items }]);
    m.kind = MenuKind::Catalog;
    m
}

pub fn py_fns() -> Menu {
    Menu::new(vec![
        Tab {
            title: "Built-ins",
            items: vec![
                py("print()", "print("),
                py("input()", "input("),
                py("int()", "int("),
                py("float()", "float("),
                py("str()", "str("),
                py("len()", "len("),
                py("range()", "range("),
                py("round()", "round("),
                py("abs()", "abs("),
                py("min()", "min("),
                py("max()", "max("),
                py("sum()", "sum("),
            ],
        },
        Tab {
            title: "Ctl",
            items: vec![
                py("if ..:", "if :"),
                py("elif ..:", "elif :"),
                py("else:", "else:"),
                py("for i in range():", "for i in range():"),
                py("for i in list:", "for i in :"),
                py("while ..:", "while :"),
                py("def f():", "def ():"),
                py("return", "return "),
                py("break", "break"),
                py("try:/except:", "try:"),
            ],
        },
        Tab {
            title: "Ops",
            items: vec![
                py("==", "=="),
                py("!=", "!="),
                py(">", ">"),
                py(">=", ">="),
                py("<", "<"),
                py("<=", "<="),
                py("and", " and "),
                py("or", " or "),
                py("not", "not "),
                py("in", " in "),
                py("%", "%"),
                py("//", "//"),
            ],
        },
        Tab {
            title: "List",
            items: vec![
                py("[]", "[]"),
                py(".append(x)", ".append("),
                py(".pop()", ".pop()"),
                py(".sort()", ".sort()"),
                py("sorted()", "sorted("),
                py("list()", "list("),
                py("enumerate()", "enumerate("),
            ],
        },
        Tab {
            title: "Mod",
            items: vec![
                py("from math import *", "from math import *"),
                py("from random import *", "from random import *"),
                py("import time", "import time"),
                py("sqrt()", "sqrt("),
                py("pi", "pi"),
                py("randint(,)", "randint("),
            ],
        },
    ])
}

pub fn py_test() -> Menu {
    Menu::new(vec![Tab {
        title: "TEST",
        items: vec![
            py("==", "=="),
            py("!=", "!="),
            py(">", ">"),
            py(">=", ">="),
            py("<", "<"),
            py("<=", "<="),
            py("and", " and "),
            py("or", " or "),
            py("not", "not "),
            py("True", "True"),
            py("False", "False"),
        ],
    }])
}

pub fn py_shell_tools() -> Menu {
    Menu::new(vec![Tab {
        title: "Tools",
        items: vec![cmd("Clear Screen", Cmd::PyClearShell), cmd("Reinitialize Shell", Cmd::PyReinit)],
    }])
}

pub fn py_editor_tools() -> Menu {
    Menu::new(vec![Tab {
        title: "Tools",
        items: vec![
            cmd("Indent ▸", Cmd::PyIndentIn),
            cmd("Indent ◂", Cmd::PyIndentOut),
            cmd("Insert Line Above", Cmd::PyInsertLineAbove),
            cmd("Cut Line", Cmd::PyCutLine),
            cmd("Paste Line Below", Cmd::PyPasteLine),
        ],
    }])
}

pub fn py_manage() -> Menu {
    Menu::new(vec![Tab {
        title: "Manage",
        items: vec![cmd("Delete Script", Cmd::PyDeleteScript), cmd("Cancel", Cmd::Cancel)],
    }])
}
