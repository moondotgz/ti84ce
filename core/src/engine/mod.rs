//! The calculator's math engine: a TI-style tokenized expression parser,
//! evaluator, and number formatter.

pub mod eval;
pub mod format;
pub mod parse;

pub use eval::{Env, Value};
pub use format::{format_value, NumFormat};
pub use parse::{parse_expr, parse_line, Expr};

/// A token in an entry line. All tokens come from a fixed static vocabulary,
/// mirroring how the real calculator stores programs and entries as tokens.
pub type Tok = &'static str;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalcError {
    Syntax(usize),
    DivideBy0,
    Domain,
    NonReal,
    Undefined,
    DimMismatch,
    InvalidDim,
    Overflow,
    Argument,
    DataType,
    WindowRange,
    NoSignChange,
    Invalid,
}

impl CalcError {
    pub fn title(&self) -> &'static str {
        match self {
            CalcError::Syntax(_) => "SYNTAX",
            CalcError::DivideBy0 => "DIVIDE BY 0",
            CalcError::Domain => "DOMAIN",
            CalcError::NonReal => "NONREAL ANSWERS",
            CalcError::Undefined => "UNDEFINED",
            CalcError::DimMismatch => "DIMENSION MISMATCH",
            CalcError::InvalidDim => "INVALID DIMENSION",
            CalcError::Overflow => "OVERFLOW",
            CalcError::Argument => "ARGUMENT",
            CalcError::DataType => "DATA TYPE",
            CalcError::WindowRange => "WINDOW RANGE",
            CalcError::NoSignChange => "NO SIGN CHANGE",
            CalcError::Invalid => "INVALID",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            CalcError::Syntax(_) => "The command contains a syntax error.",
            CalcError::DivideBy0 => "You attempted to divide by zero.",
            CalcError::Domain => "An argument is outside the valid domain.",
            CalcError::NonReal => "In REAL mode, the result is not a real number.",
            CalcError::Undefined => "You referenced a variable that is not defined.",
            CalcError::DimMismatch => "List dimensions do not match.",
            CalcError::InvalidDim => "The dimension is not valid.",
            CalcError::Overflow => "The result is too large to display.",
            CalcError::Argument => "A function has the wrong number of arguments.",
            CalcError::DataType => "You entered a value of the wrong type.",
            CalcError::WindowRange => "Xmin must be less than Xmax, and Ymin less than Ymax.",
            CalcError::NoSignChange => "No sign change was found between the bounds.",
            CalcError::Invalid => "This operation is not valid here.",
        }
    }

    pub fn goto_pos(&self) -> Option<usize> {
        match self {
            CalcError::Syntax(p) => Some(*p),
            _ => None,
        }
    }
}

pub type CResult<T> = Result<T, CalcError>;

/// Letter variables A–Z plus θ.
pub const LETTERS: [Tok; 27] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R",
    "S", "T", "U", "V", "W", "X", "Y", "Z", "θ",
];

pub const LISTS: [Tok; 6] = ["L₁", "L₂", "L₃", "L₄", "L₅", "L₆"];

/// Y-variables in Y= editor order (Y1..Y9, Y0).
pub const YVARS: [Tok; 10] = ["Y₁", "Y₂", "Y₃", "Y₄", "Y₅", "Y₆", "Y₇", "Y₈", "Y₉", "Y₀"];

pub const DIGITS: [Tok; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// Functions that take parenthesized arguments (the token includes the paren).
pub const FUNCS: &[Tok] = &[
    "sin(", "cos(", "tan(", "sin⁻¹(", "cos⁻¹(", "tan⁻¹(", "sinh(", "cosh(", "tanh(",
    "sinh⁻¹(", "cosh⁻¹(", "tanh⁻¹(", "log(", "ln(", "10^(", "ℯ^(", "√(", "³√(", "abs(",
    "round(", "iPart(", "fPart(", "int(", "min(", "max(", "lcm(", "gcd(", "remainder(",
    "randInt(", "sum(", "prod(", "mean(", "median(", "dim(", "cumSum(", "seq(", "augment(",
    "normalpdf(", "normalcdf(", "invNorm(", "binompdf(", "binomcdf(", "poissonpdf(",
    "poissoncdf(", "geometpdf(", "geometcdf(", "nDeriv(", "fnInt(", "logBASE(", "not(",
    "stdDev(", "variance(", "SortA(", "SortD(", "ΔList(", "floor(", "ceiling(",
];

pub fn letter_index(t: Tok) -> Option<usize> {
    LETTERS.iter().position(|&l| l == t)
}

pub fn list_index(t: Tok) -> Option<usize> {
    LISTS.iter().position(|&l| l == t)
}

pub fn yvar_index(t: Tok) -> Option<usize> {
    YVARS.iter().position(|&l| l == t)
}

pub fn is_func(t: Tok) -> bool {
    FUNCS.contains(&t)
}

/// Intern an arbitrary string into the static token vocabulary, if it is part of it.
pub fn intern(s: &str) -> Option<Tok> {
    const OTHERS: &[Tok] = &[
        ".", "ᴇ", "+", "-", "*", "/", "^", "⁻", "(", ")", ",", "{", "}", "[", "]", "→", ":",
        "=", "≠", ">", "≥", "<", "≤", " and ", " or ", " xor ", "²", "⁻¹", "³", "!", "°",
        "ʳ", " nPr ", " nCr ", "ˣ√", "►Frac", "►Dec", "π", "ℯ", "Ans", "rand", " ", "\"",
        "?", "i", "1-Var Stats ", "2-Var Stats ", "LinReg(ax+b) ", "ClrHome",
    ];
    LETTERS
        .iter()
        .chain(DIGITS.iter())
        .chain(LISTS.iter())
        .chain(YVARS.iter())
        .chain(FUNCS.iter())
        .chain(OTHERS.iter())
        .copied()
        .find(|&t| t == s)
}

/// Display text for a token list.
pub fn tokens_to_string(toks: &[Tok]) -> String {
    toks.concat()
}

/// Convert a plain string (e.g. one typed or a formatted result) into tokens,
/// greedily matching the longest vocabulary entry.
pub fn string_to_tokens(s: &str) -> Vec<Tok> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let mut matched = None;
        for len in (1..=12.min(chars.len() - i)).rev() {
            let cand: String = chars[i..i + len].iter().collect();
            if let Some(t) = intern(&cand) {
                matched = Some((t, len));
                break;
            }
        }
        match matched {
            Some((t, len)) => {
                out.push(t);
                i += len;
            }
            None => i += 1,
        }
    }
    out
}

#[cfg(test)]
mod tests;
