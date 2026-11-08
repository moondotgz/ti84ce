//! The physical keypad: key identities, printed labels and layout.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    YEq,
    Window,
    Zoom,
    Trace,
    Graph,
    Second,
    Mode,
    Del,
    Up,
    Down,
    Left,
    Right,
    Alpha,
    Xtn,
    Stat,
    Math,
    Apps,
    Prgm,
    Vars,
    Clear,
    Inv,
    Sin,
    Cos,
    Tan,
    Pow,
    Sq,
    Comma,
    LParen,
    RParen,
    Div,
    Log,
    D7,
    D8,
    D9,
    Mul,
    Ln,
    D4,
    D5,
    D6,
    Sub,
    Sto,
    D1,
    D2,
    D3,
    Add,
    On,
    D0,
    Dot,
    Neg,
    Enter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStyle {
    /// The five white keys directly under the screen.
    Top,
    /// Black function keys.
    Func,
    /// Light-gray number keys.
    Number,
    /// Light-gray operator keys on the right column.
    Op,
    Second,
    Alpha,
    Enter,
}

pub struct KeyDef {
    pub key: Key,
    pub label: &'static str,
    pub second: &'static str,
    pub alpha: &'static str,
    pub style: KeyStyle,
}

const fn k(key: Key, label: &'static str, second: &'static str, alpha: &'static str, style: KeyStyle) -> KeyDef {
    KeyDef { key, label, second, alpha, style }
}

use Key::*;
use KeyStyle as S;

/// Top row (below the screen).
pub const TOP_ROW: [KeyDef; 5] = [
    k(YEq, "y=", "stat plot", "f1", S::Top),
    k(Window, "window", "tblset", "f2", S::Top),
    k(Zoom, "zoom", "format", "f3", S::Top),
    k(Trace, "trace", "calc", "f4", S::Top),
    k(Graph, "graph", "table", "f5", S::Top),
];

/// The two rows beside the arrow pad.
pub const UPPER_LEFT: [[KeyDef; 3]; 2] = [
    [
        k(Second, "2nd", "", "", S::Second),
        k(Mode, "mode", "quit", "", S::Func),
        k(Del, "del", "ins", "", S::Func),
    ],
    [
        k(Alpha, "alpha", "A-lock", "", S::Alpha),
        k(Xtn, "X,T,θ,n", "link", "", S::Func),
        k(Stat, "stat", "list", "", S::Func),
    ],
];

/// The main 5-column grid, top to bottom.
pub const GRID: [[KeyDef; 5]; 7] = [
    [
        k(Math, "math", "test", "A", S::Func),
        k(Apps, "apps", "angle", "B", S::Func),
        k(Prgm, "prgm", "draw", "C", S::Func),
        k(Vars, "vars", "distr", "", S::Func),
        k(Clear, "clear", "", "", S::Func),
    ],
    [
        k(Inv, "x⁻¹", "matrix", "D", S::Func),
        k(Sin, "sin", "sin⁻¹", "E", S::Func),
        k(Cos, "cos", "cos⁻¹", "F", S::Func),
        k(Tan, "tan", "tan⁻¹", "G", S::Func),
        k(Pow, "^", "π", "H", S::Op),
    ],
    [
        k(Sq, "x²", "√", "I", S::Func),
        k(Comma, ",", "EE", "J", S::Func),
        k(LParen, "(", "{", "K", S::Func),
        k(RParen, ")", "}", "L", S::Func),
        k(Div, "÷", "e", "M", S::Op),
    ],
    [
        k(Log, "log", "10ˣ", "N", S::Func),
        k(D7, "7", "u", "O", S::Number),
        k(D8, "8", "v", "P", S::Number),
        k(D9, "9", "w", "Q", S::Number),
        k(Mul, "×", "[", "R", S::Op),
    ],
    [
        k(Ln, "ln", "eˣ", "S", S::Func),
        k(D4, "4", "L4", "T", S::Number),
        k(D5, "5", "L5", "U", S::Number),
        k(D6, "6", "L6", "V", S::Number),
        k(Sub, "−", "]", "W", S::Op),
    ],
    [
        k(Sto, "sto→", "rcl", "X", S::Func),
        k(D1, "1", "L1", "Y", S::Number),
        k(D2, "2", "L2", "Z", S::Number),
        k(D3, "3", "L3", "θ", S::Number),
        k(Add, "+", "mem", "\"", S::Op),
    ],
    [
        k(On, "on", "off", "", S::Func),
        k(D0, "0", "catalog", "␣", S::Number),
        k(Dot, ".", "i", ":", S::Number),
        k(Neg, "(−)", "ans", "?", S::Number),
        k(Enter, "enter", "entry", "solve", S::Enter),
    ],
];

impl Key {
    pub fn digit(self) -> Option<u8> {
        Some(match self {
            D0 => 0,
            D1 => 1,
            D2 => 2,
            D3 => 3,
            D4 => 4,
            D5 => 5,
            D6 => 6,
            D7 => 7,
            D8 => 8,
            D9 => 9,
            _ => return None,
        })
    }

    /// The letter this key types in alpha mode.
    pub fn alpha_char(self) -> Option<&'static str> {
        Some(match self {
            Math => "A",
            Apps => "B",
            Prgm => "C",
            Inv => "D",
            Sin => "E",
            Cos => "F",
            Tan => "G",
            Pow => "H",
            Sq => "I",
            Comma => "J",
            LParen => "K",
            RParen => "L",
            Div => "M",
            Log => "N",
            D7 => "O",
            D8 => "P",
            D9 => "Q",
            Mul => "R",
            Ln => "S",
            D4 => "T",
            D5 => "U",
            D6 => "V",
            Sub => "W",
            Sto => "X",
            D1 => "Y",
            D2 => "Z",
            D3 => "θ",
            Add => "\"",
            D0 => " ",
            Dot => ":",
            Neg => "?",
            _ => return None,
        })
    }

    pub fn from_digit(d: u8) -> Key {
        [D0, D1, D2, D3, D4, D5, D6, D7, D8, D9][d as usize % 10]
    }
}

/// Look up a key by its printed label (e.g. "2nd", "enter", "sin"), or an
/// arrow name ("up", "down", "left", "right").
pub fn key_by_name(name: &str) -> Option<Key> {
    match name {
        "up" => return Some(Up),
        "down" => return Some(Down),
        "left" => return Some(Left),
        "right" => return Some(Right),
        "-" => return Some(Sub),
        "*" => return Some(Mul),
        "/" => return Some(Div),
        "neg" => return Some(Neg),
        "sto" => return Some(Sto),
        "x" => return Some(Xtn),
        "x^2" => return Some(Sq),
        "x^-1" => return Some(Inv),
        _ => {}
    }
    TOP_ROW
        .iter()
        .chain(UPPER_LEFT.iter().flatten())
        .chain(GRID.iter().flatten())
        .find(|d| d.label == name)
        .map(|d| d.key)
}
