//! Tree-walking interpreter for the Python subset.

use super::parser::{BOp, COp, Ex, FPart, FuncDef, Stmt, Target, SK};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering as AO};
use std::sync::Arc;

pub struct FuncObj {
    pub def: Rc<FuncDef>,
    pub defaults: Vec<Val>,
}

#[derive(Clone)]
pub enum Val {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Rc<str>),
    List(Rc<RefCell<Vec<Val>>>),
    Tuple(Rc<Vec<Val>>),
    Dict(Rc<RefCell<Vec<(Val, Val)>>>),
    Range(i64, i64, i64),
    Func(Rc<FuncObj>),
    Builtin(&'static str),
    Method(Rc<(Val, Rc<str>)>),
    Module(&'static str),
    Exc(Rc<(String, String)>),
}

pub struct Exc {
    pub kind: String,
    pub msg: String,
    pub line: usize,
}

pub enum Flow {
    Exc(Exc),
    Break,
    Continue,
    Return(Val),
}

impl From<Exc> for Flow {
    fn from(e: Exc) -> Self {
        Flow::Exc(e)
    }
}

type R<T> = Result<T, Exc>;

fn err<T>(kind: &str, msg: impl Into<String>) -> R<T> {
    Err(Exc { kind: kind.to_string(), msg: msg.into(), line: 0 })
}

fn list(v: Vec<Val>) -> Val {
    Val::List(Rc::new(RefCell::new(v)))
}

fn s(v: &str) -> Val {
    Val::Str(v.into())
}

const BUILTINS: &[&str] = &[
    "print", "len", "range", "int", "float", "str", "bool", "abs", "round", "min", "max",
    "sum", "list", "tuple", "dict", "sorted", "reversed", "enumerate", "zip", "chr", "ord",
    "divmod", "pow", "type", "input", "hex", "bin", "oct", "map", "filter", "any", "all",
    "repr", "isinstance", "Exception", "ValueError", "TypeError", "ZeroDivisionError",
    "IndexError", "KeyError", "NameError", "RuntimeError", "AssertionError", "OverflowError",
    "StopIteration", "help", "exit", "quit",
];

const EXC_TYPES: &[&str] = &[
    "Exception", "ValueError", "TypeError", "ZeroDivisionError", "IndexError", "KeyError",
    "NameError", "RuntimeError", "AssertionError", "OverflowError", "StopIteration",
];

const MATH_FNS: &[&str] = &[
    "math.sqrt", "math.sin", "math.cos", "math.tan", "math.asin", "math.acos", "math.atan",
    "math.atan2", "math.exp", "math.log", "math.log10", "math.log2", "math.floor",
    "math.ceil", "math.fabs", "math.pow", "math.degrees", "math.radians", "math.factorial",
    "math.gcd", "math.trunc", "math.hypot", "math.sinh", "math.cosh", "math.tanh",
    "math.isclose", "math.fmod", "math.copysign", "math.isqrt", "math.comb", "math.perm",
];

const RANDOM_FNS: &[&str] = &[
    "random.random", "random.randint", "random.uniform", "random.choice", "random.seed",
    "random.randrange", "random.shuffle",
];

const TIME_FNS: &[&str] = &["time.sleep", "time.monotonic", "time.time"];

const MODULES: &[&str] = &["math", "random", "time"];

fn module_fns(m: &str) -> &'static [&'static str] {
    match m {
        "math" => MATH_FNS,
        "random" => RANDOM_FNS,
        "time" => TIME_FNS,
        _ => &[],
    }
}

fn module_attr(m: &str, name: &str) -> Option<Val> {
    if m == "math" {
        match name {
            "pi" => return Some(Val::Float(std::f64::consts::PI)),
            "e" => return Some(Val::Float(std::f64::consts::E)),
            "tau" => return Some(Val::Float(std::f64::consts::TAU)),
            "inf" => return Some(Val::Float(f64::INFINITY)),
            "nan" => return Some(Val::Float(f64::NAN)),
            _ => {}
        }
    }
    module_fns(m)
        .iter()
        .find(|f| f.split('.').nth(1) == Some(name))
        .map(|f| Val::Builtin(f))
}

pub fn type_name(v: &Val) -> &'static str {
    match v {
        Val::None => "NoneType",
        Val::Bool(_) => "bool",
        Val::Int(_) => "int",
        Val::Float(_) => "float",
        Val::Str(_) => "str",
        Val::List(_) => "list",
        Val::Tuple(_) => "tuple",
        Val::Dict(_) => "dict",
        Val::Range(..) => "range",
        Val::Func(_) => "function",
        Val::Builtin(_) | Val::Method(_) => "builtin_function_or_method",
        Val::Module(_) => "module",
        Val::Exc(_) => "Exception",
    }
}

pub fn float_repr(f: f64) -> String {
    if f.is_nan() {
        return "nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let e = format!("{:e}", f);
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if (-4..16).contains(&exp) {
        let s = format!("{}", f);
        if s.contains('.') {
            s
        } else {
            format!("{s}.0")
        }
    } else {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{mant}e{sign}{:02}", exp.abs())
    }
}

pub fn repr(v: &Val) -> String {
    match v {
        Val::Str(x) => {
            let q = if x.contains('\'') && !x.contains('"') { '"' } else { '\'' };
            let mut out = String::new();
            out.push(q);
            for c in x.chars() {
                match c {
                    '\n' => out.push_str("\\n"),
                    '\t' => out.push_str("\\t"),
                    '\\' => out.push_str("\\\\"),
                    c if c == q => {
                        out.push('\\');
                        out.push(c);
                    }
                    c => out.push(c),
                }
            }
            out.push(q);
            out
        }
        _ => to_str(v),
    }
}

pub fn to_str(v: &Val) -> String {
    match v {
        Val::None => "None".into(),
        Val::Bool(b) => if *b { "True" } else { "False" }.into(),
        Val::Int(i) => i.to_string(),
        Val::Float(f) => float_repr(*f),
        Val::Str(x) => x.to_string(),
        Val::List(l) => {
            let items: Vec<String> = l.borrow().iter().map(repr).collect();
            format!("[{}]", items.join(", "))
        }
        Val::Tuple(t) => {
            let items: Vec<String> = t.iter().map(repr).collect();
            if items.len() == 1 {
                format!("({},)", items[0])
            } else {
                format!("({})", items.join(", "))
            }
        }
        Val::Dict(d) => {
            let items: Vec<String> =
                d.borrow().iter().map(|(k, v)| format!("{}: {}", repr(k), repr(v))).collect();
            format!("{{{}}}", items.join(", "))
        }
        Val::Range(a, b, st) => {
            if *st == 1 {
                format!("range({a}, {b})")
            } else {
                format!("range({a}, {b}, {st})")
            }
        }
        Val::Func(f) => format!("<function {}>", f.def.name),
        Val::Builtin(n) => format!("<function {}>", n),
        Val::Method(m) => format!("<bound_method {}>", m.1),
        Val::Module(m) => format!("<module '{m}'>"),
        Val::Exc(e) => e.1.clone(),
    }
}

pub fn truthy(v: &Val) -> bool {
    match v {
        Val::None => false,
        Val::Bool(b) => *b,
        Val::Int(i) => *i != 0,
        Val::Float(f) => *f != 0.0,
        Val::Str(x) => !x.is_empty(),
        Val::List(l) => !l.borrow().is_empty(),
        Val::Tuple(t) => !t.is_empty(),
        Val::Dict(d) => !d.borrow().is_empty(),
        Val::Range(a, b, st) => range_len(*a, *b, *st) > 0,
        _ => true,
    }
}

fn range_len(a: i64, b: i64, st: i64) -> i64 {
    if st > 0 && b > a {
        (b - a + st - 1) / st
    } else if st < 0 && a > b {
        (a - b - st - 1) / (-st)
    } else {
        0
    }
}

fn as_int(v: &Val) -> Option<i64> {
    match v {
        Val::Int(i) => Some(*i),
        Val::Bool(b) => Some(*b as i64),
        _ => None,
    }
}

fn as_f64(v: &Val) -> Option<f64> {
    match v {
        Val::Int(i) => Some(*i as f64),
        Val::Bool(b) => Some(*b as i64 as f64),
        Val::Float(f) => Some(*f),
        _ => None,
    }
}

fn need_f64(v: &Val) -> R<f64> {
    as_f64(v).map_or_else(
        || err("TypeError", format!("can't convert {} to float", type_name(v))),
        Ok,
    )
}

fn need_int(v: &Val) -> R<i64> {
    as_int(v).map_or_else(
        || err("TypeError", format!("'{}' object cannot be interpreted as an integer", type_name(v))),
        Ok,
    )
}

pub fn py_eq(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::None, Val::None) => true,
        (Val::Str(x), Val::Str(y)) => x == y,
        (Val::List(x), Val::List(y)) => {
            Rc::ptr_eq(x, y) || {
                let (x, y) = (x.borrow(), y.borrow());
                x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| py_eq(p, q))
            }
        }
        (Val::Tuple(x), Val::Tuple(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| py_eq(p, q))
        }
        (Val::Dict(x), Val::Dict(y)) => {
            let (x, y) = (x.borrow(), y.borrow());
            x.len() == y.len()
                && x.iter().all(|(k, v)| y.iter().any(|(k2, v2)| py_eq(k, k2) && py_eq(v, v2)))
        }
        (Val::Range(a1, b1, c1), Val::Range(a2, b2, c2)) => (a1, b1, c1) == (a2, b2, c2),
        (Val::Builtin(x), Val::Builtin(y)) => x == y,
        (Val::Module(x), Val::Module(y)) => x == y,
        (Val::Func(x), Val::Func(y)) => Rc::ptr_eq(x, y),
        _ => match (as_f64(a), as_f64(b)) {
            (Some(x), Some(y)) => {
                if let (Some(i), Some(j)) = (as_int(a), as_int(b)) {
                    i == j
                } else {
                    x == y
                }
            }
            _ => false,
        },
    }
}

fn py_cmp(a: &Val, b: &Val) -> R<Ordering> {
    match (a, b) {
        (Val::Str(x), Val::Str(y)) => Ok(x.cmp(y)),
        (Val::List(_), Val::List(_)) | (Val::Tuple(_), Val::Tuple(_)) => {
            let xs = seq_items(a);
            let ys = seq_items(b);
            for (p, q) in xs.iter().zip(ys.iter()) {
                if !py_eq(p, q) {
                    return py_cmp(p, q);
                }
            }
            Ok(xs.len().cmp(&ys.len()))
        }
        _ => {
            if let (Some(i), Some(j)) = (as_int(a), as_int(b)) {
                return Ok(i.cmp(&j));
            }
            match (as_f64(a), as_f64(b)) {
                (Some(x), Some(y)) => Ok(x.partial_cmp(&y).unwrap_or(Ordering::Equal)),
                _ => err(
                    "TypeError",
                    format!("unsupported types for comparison: '{}', '{}'", type_name(a), type_name(b)),
                ),
            }
        }
    }
}

fn seq_items(v: &Val) -> Vec<Val> {
    match v {
        Val::List(l) => l.borrow().clone(),
        Val::Tuple(t) => t.as_ref().clone(),
        _ => vec![],
    }
}

fn norm_index(i: i64, len: usize) -> Option<usize> {
    let i = if i < 0 { i + len as i64 } else { i };
    if i >= 0 && (i as usize) < len {
        Some(i as usize)
    } else {
        None
    }
}

fn slice_indices(len: usize, a: Option<i64>, b: Option<i64>, st: Option<i64>) -> R<Vec<usize>> {
    let st = st.unwrap_or(1);
    if st == 0 {
        return err("ValueError", "slice step cannot be zero");
    }
    let len = len as i64;
    let clamp = |x: i64, lo: i64, hi: i64| x.max(lo).min(hi);
    let fix = |x: i64| if x < 0 { x + len } else { x };
    let mut out = Vec::new();
    if st > 0 {
        let start = clamp(a.map(fix).unwrap_or(0), 0, len);
        let stop = clamp(b.map(fix).unwrap_or(len), 0, len);
        let mut i = start;
        while i < stop {
            out.push(i as usize);
            i += st;
        }
    } else {
        let start = clamp(a.map(fix).unwrap_or(len - 1), -1, len - 1);
        let stop = clamp(b.map(fix).unwrap_or(-1), -1, len - 1);
        let mut i = start;
        while i > stop {
            out.push(i as usize);
            i += st;
        }
    }
    Ok(out)
}

fn round_half_even(x: f64) -> f64 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 {
        2.0 * (x / 2.0).round()
    } else {
        r
    }
}

/// Apply a Python format spec like `>8.2f` to a value.
pub fn format_spec(v: &Val, spec: &str) -> R<String> {
    let chars: Vec<char> = spec.chars().collect();
    let mut i = 0;
    let mut fill = ' ';
    let mut align = None;
    if chars.len() >= 2 && matches!(chars[1], '<' | '>' | '^') {
        fill = chars[0];
        align = Some(chars[1]);
        i = 2;
    } else if !chars.is_empty() && matches!(chars[0], '<' | '>' | '^') {
        align = Some(chars[0]);
        i = 1;
    }
    let mut plus = false;
    if i < chars.len() && chars[i] == '+' {
        plus = true;
        i += 1;
    }
    if i < chars.len() && chars[i] == '0' && align.is_none() {
        fill = '0';
        align = Some('=');
        i += 1;
    }
    let mut width = 0usize;
    while i < chars.len() && chars[i].is_ascii_digit() {
        width = width * 10 + chars[i].to_digit(10).unwrap() as usize;
        i += 1;
    }
    let mut comma = false;
    if i < chars.len() && chars[i] == ',' {
        comma = true;
        i += 1;
    }
    let mut prec = None;
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        let mut p = 0;
        while i < chars.len() && chars[i].is_ascii_digit() {
            p = p * 10 + chars[i].to_digit(10).unwrap() as usize;
            i += 1;
        }
        prec = Some(p);
    }
    let ty = chars.get(i).copied();
    let mut body = match ty {
        Some('f') | Some('F') => format!("{:.*}", prec.unwrap_or(6), need_f64(v)?),
        Some('e') | Some('E') => {
            let s = format!("{:.*e}", prec.unwrap_or(6), need_f64(v)?);
            let (m, e) = s.split_once('e').unwrap();
            let e: i32 = e.parse().unwrap();
            format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
        }
        Some('%') => format!("{:.*}%", prec.unwrap_or(6), need_f64(v)? * 100.0),
        Some('d') => need_int(v)?.to_string(),
        Some('x') => format!("{:x}", need_int(v)?),
        Some('X') => format!("{:X}", need_int(v)?),
        Some('b') => format!("{:b}", need_int(v)?),
        Some('g') | Some('G') => {
            let f = need_f64(v)?;
            let p = prec.unwrap_or(6).max(1);
            let e = if f == 0.0 { 0 } else { f.abs().log10().floor() as i32 };
            if e < -4 || e >= p as i32 {
                let s = format!("{:.*e}", p - 1, f);
                let (m, ex) = s.split_once('e').unwrap();
                let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
                let ex: i32 = ex.parse().unwrap();
                format!("{m}e{}{:02}", if ex < 0 { '-' } else { '+' }, ex.abs())
            } else {
                let s = format!("{:.*}", (p as i32 - 1 - e).max(0) as usize, f);
                if s.contains('.') {
                    s.trim_end_matches('0').trim_end_matches('.').to_string()
                } else {
                    s
                }
            }
        }
        _ => match (v, prec) {
            (Val::Float(f), Some(p)) => format!("{:.*}", p, f),
            (Val::Str(x), Some(p)) => x.chars().take(p).collect(),
            _ => to_str(v),
        },
    };
    if comma {
        let (int_part, rest) = match body.find('.') {
            Some(p) => (body[..p].to_string(), body[p..].to_string()),
            None => (body.clone(), String::new()),
        };
        let neg = int_part.starts_with('-');
        let digits: Vec<char> = int_part.trim_start_matches('-').chars().collect();
        let mut grouped = String::new();
        for (k, c) in digits.iter().enumerate() {
            if k > 0 && (digits.len() - k) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(*c);
        }
        body = format!("{}{grouped}{rest}", if neg { "-" } else { "" });
    }
    if plus && as_f64(v).is_some_and(|x| x >= 0.0) {
        body = format!("+{body}");
    }
    let len = body.chars().count();
    if len < width {
        let pad = width - len;
        let is_num = matches!(v, Val::Int(_) | Val::Float(_));
        let align = align.unwrap_or(if is_num { '>' } else { '<' });
        let fill_s = |n: usize| std::iter::repeat_n(fill, n).collect::<String>();
        body = match align {
            '<' => format!("{body}{}", fill_s(pad)),
            '^' => format!("{}{body}{}", fill_s(pad / 2), fill_s(pad - pad / 2)),
            '=' => {
                if let Some(rest) = body.strip_prefix('-') {
                    format!("-{}{rest}", fill_s(pad))
                } else {
                    format!("{}{body}", fill_s(pad))
                }
            }
            _ => format!("{}{body}", fill_s(pad)),
        };
    }
    Ok(body)
}

/// printf-style `%` formatting.
fn percent_format(fmt: &str, args: &Val) -> R<String> {
    let args: Vec<Val> = match args {
        Val::Tuple(t) => t.as_ref().clone(),
        other => vec![other.clone()],
    };
    let chars: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut k = 0;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '%' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        i += 1;
        if i < chars.len() && chars[i] == '%' {
            out.push('%');
            i += 1;
            continue;
        }
        let mut spec = String::new();
        while i < chars.len() && (chars[i].is_ascii_digit() || matches!(chars[i], '.' | '-' | '+' | ' ')) {
            spec.push(chars[i]);
            i += 1;
        }
        let ty = chars.get(i).copied().unwrap_or('s');
        i += 1;
        let arg = args.get(k).cloned().ok_or(Exc {
            kind: "TypeError".into(),
            msg: "not enough arguments for format string".into(),
            line: 0,
        })?;
        k += 1;
        let left = spec.starts_with('-');
        let spec = spec.trim_start_matches('-');
        let py_spec = match ty {
            's' => format!("{}{spec}", if left { "<" } else { ">" }),
            'r' => {
                let r = repr(&arg);
                out.push_str(&r);
                continue;
            }
            'i' | 'u' => format!("{}{spec}d", if left { "<" } else { "" }),
            c => format!("{}{spec}{c}", if left { "<" } else { "" }),
        };
        let arg = if ty == 'd' || ty == 'i' {
            match arg {
                Val::Float(f) => Val::Int(f.trunc() as i64),
                a => a,
            }
        } else {
            arg
        };
        out.push_str(&format_spec(&arg, &py_spec)?);
    }
    Ok(out)
}

struct Frame {
    locals: HashMap<Rc<str>, Val>,
    globals: HashSet<Rc<str>>,
}

pub struct Interp {
    pub globals: HashMap<Rc<str>, Val>,
    frames: Vec<Frame>,
    pub output: Box<dyn FnMut(&str)>,
    pub input: Box<dyn FnMut(&str) -> Option<String>>,
    pub interrupt: Arc<AtomicBool>,
    steps: u64,
    line: usize,
    rng: u64,
    start: std::time::Instant,
}

impl Interp {
    pub fn new(
        output: Box<dyn FnMut(&str)>,
        input: Box<dyn FnMut(&str) -> Option<String>>,
        interrupt: Arc<AtomicBool>,
    ) -> Self {
        Interp {
            globals: HashMap::new(),
            frames: Vec::new(),
            output,
            input,
            interrupt,
            steps: 0,
            line: 0,
            rng: 0x2545F4914F6CDD1D,
            start: std::time::Instant::now(),
        }
    }

    pub fn reset(&mut self) {
        self.globals.clear();
        self.frames.clear();
    }

    fn print(&mut self, s: &str) {
        (self.output)(s);
    }

    /// Run source text. `repl` echoes the repr of top-level expression statements.
    /// Errors are printed as a traceback to the output.
    pub fn run(&mut self, src: &str, file: &str, repl: bool) -> bool {
        self.interrupt.store(false, AO::SeqCst);
        let parsed = super::lexer::tokenize(src).and_then(super::parser::parse);
        let prog = match parsed {
            Ok(p) => p,
            Err((line, msg)) => {
                self.print(&format!("Traceback (most recent call last):\n  File \"{file}\", line {line}\n{msg}\n"));
                return false;
            }
        };
        self.frames.clear();
        for stmt in &prog {
            let r = if repl {
                if let SK::Expr(e) = &stmt.kind {
                    self.line = stmt.line;
                    match self.eval(e) {
                        Ok(Val::None) => Ok(()),
                        Ok(v) => {
                            let r = repr(&v);
                            self.print(&format!("{r}\n"));
                            Ok(())
                        }
                        Err(e) => Err(Flow::Exc(e)),
                    }
                } else {
                    self.exec(stmt)
                }
            } else {
                self.exec(stmt)
            };
            match r {
                Ok(()) => {}
                Err(Flow::Exc(e)) => {
                    let line = if e.line == 0 { self.line } else { e.line };
                    let msg = if e.msg.is_empty() { e.kind.clone() } else { format!("{}: {}", e.kind, e.msg) };
                    self.print(&format!(
                        "Traceback (most recent call last):\n  File \"{file}\", line {line}, in <module>\n{msg}\n"
                    ));
                    self.frames.clear();
                    return false;
                }
                Err(Flow::Break) | Err(Flow::Continue) => {
                    self.print("SyntaxError: 'break' outside loop\n");
                    return false;
                }
                Err(Flow::Return(_)) => {
                    self.print("SyntaxError: 'return' outside function\n");
                    return false;
                }
            }
        }
        true
    }

    fn tick(&mut self) -> R<()> {
        self.steps += 1;
        if self.steps % 512 == 0 && self.interrupt.load(AO::Relaxed) {
            return err("KeyboardInterrupt", "");
        }
        Ok(())
    }

    fn exec_block(&mut self, body: &[Stmt]) -> Result<(), Flow> {
        for s in body {
            self.exec(s)?;
        }
        Ok(())
    }

    fn lookup(&self, name: &str) -> R<Val> {
        if let Some(f) = self.frames.last() {
            if let Some(v) = f.locals.get(name) {
                return Ok(v.clone());
            }
        }
        if let Some(v) = self.globals.get(name) {
            return Ok(v.clone());
        }
        if let Some(b) = BUILTINS.iter().find(|&&b| b == name) {
            return Ok(Val::Builtin(b));
        }
        err("NameError", format!("name '{name}' isn't defined"))
    }

    fn set_name(&mut self, name: &Rc<str>, v: Val) {
        if let Some(f) = self.frames.last_mut() {
            if !f.globals.contains(name) {
                f.locals.insert(name.clone(), v);
                return;
            }
        }
        self.globals.insert(name.clone(), v);
    }

    fn del_name(&mut self, name: &Rc<str>) -> R<()> {
        if let Some(f) = self.frames.last_mut() {
            if !f.globals.contains(name) && f.locals.remove(name).is_some() {
                return Ok(());
            }
        }
        if self.globals.remove(name).is_some() {
            Ok(())
        } else {
            err("NameError", format!("name '{name}' isn't defined"))
        }
    }

    fn assign(&mut self, t: &Target, v: Val) -> R<()> {
        match t {
            Target::Name(n) => {
                self.set_name(n, v);
                Ok(())
            }
            Target::Index(obj, idx) => {
                let o = self.eval(obj)?;
                let i = self.eval(idx)?;
                self.set_item(&o, &i, v)
            }
            Target::Attr(n) => err("AttributeError", format!("can't set attribute '{n}'")),
            Target::Tuple(ts) => {
                let items = self.iterate(&v)?;
                if items.len() != ts.len() {
                    return err(
                        "ValueError",
                        if items.len() > ts.len() {
                            format!("too many values to unpack (expected {})", ts.len())
                        } else {
                            format!("need more than {} values to unpack", items.len())
                        },
                    );
                }
                for (t, item) in ts.iter().zip(items) {
                    self.assign(t, item)?;
                }
                Ok(())
            }
        }
    }

    fn set_item(&mut self, o: &Val, i: &Val, v: Val) -> R<()> {
        match o {
            Val::List(l) => {
                let len = l.borrow().len();
                let idx = norm_index(need_int(i)?, len)
                    .ok_or_else(|| Exc { kind: "IndexError".into(), msg: "list index out of range".into(), line: 0 })?;
                l.borrow_mut()[idx] = v;
                Ok(())
            }
            Val::Dict(d) => {
                let mut d = d.borrow_mut();
                if let Some(e) = d.iter_mut().find(|(k, _)| py_eq(k, i)) {
                    e.1 = v;
                } else {
                    d.push((i.clone(), v));
                }
                Ok(())
            }
            _ => err("TypeError", format!("'{}' object doesn't support item assignment", type_name(o))),
        }
    }

    fn exec(&mut self, stmt: &Stmt) -> Result<(), Flow> {
        self.line = stmt.line;
        let r = self.exec_inner(stmt);
        if let Err(Flow::Exc(mut e)) = r {
            if e.line == 0 {
                e.line = stmt.line;
            }
            return Err(Flow::Exc(e));
        }
        r
    }

    fn exec_inner(&mut self, stmt: &Stmt) -> Result<(), Flow> {
        self.tick()?;
        match &stmt.kind {
            SK::Expr(e) => {
                self.eval(e)?;
            }
            SK::Assign(targets, e) => {
                let v = self.eval(e)?;
                for t in targets {
                    self.assign(t, v.clone())?;
                }
            }
            SK::AugAssign(t, op, e) => {
                let cur = match t {
                    Target::Name(n) => self.lookup(n)?,
                    Target::Index(o, i) => {
                        let o = self.eval(o)?;
                        let i = self.eval(i)?;
                        self.get_item(&o, &i)?
                    }
                    _ => return Err(Flow::Exc(Exc { kind: "SyntaxError".into(), msg: "invalid target".into(), line: 0 })),
                };
                let rhs = self.eval(e)?;
                // In-place list extension keeps aliasing semantics.
                if let (BOp::Add, Val::List(l)) = (op, &cur) {
                    let items = self.iterate(&rhs)?;
                    l.borrow_mut().extend(items);
                    self.assign(t, cur.clone())?;
                    return Ok(());
                }
                let v = self.binop(*op, &cur, &rhs)?;
                self.assign(t, v)?;
            }
            SK::If(arms, els) => {
                for (c, body) in arms {
                    let cv = self.eval(c)?;
                    if truthy(&cv) {
                        return self.exec_block(body);
                    }
                }
                if let Some(b) = els {
                    return self.exec_block(b);
                }
            }
            SK::While(c, body, els) => {
                loop {
                    let cv = self.eval(c)?;
                    if !truthy(&cv) {
                        if let Some(b) = els {
                            self.exec_block(b)?;
                        }
                        break;
                    }
                    self.tick()?;
                    match self.exec_block(body) {
                        Ok(()) | Err(Flow::Continue) => {}
                        Err(Flow::Break) => break,
                        Err(e) => return Err(e),
                    }
                }
            }
            SK::For(t, it, body, els) => {
                let iv = self.eval(it)?;
                let mut broke = false;
                if let Val::Range(a, b, st) = iv {
                    let n = range_len(a, b, st);
                    for k in 0..n {
                        self.assign(t, Val::Int(a + k * st))?;
                        self.tick()?;
                        match self.exec_block(body) {
                            Ok(()) | Err(Flow::Continue) => {}
                            Err(Flow::Break) => {
                                broke = true;
                                break;
                            }
                            Err(e) => return Err(e),
                        }
                    }
                } else {
                    let items = self.iterate(&iv)?;
                    for item in items {
                        self.assign(t, item)?;
                        self.tick()?;
                        match self.exec_block(body) {
                            Ok(()) | Err(Flow::Continue) => {}
                            Err(Flow::Break) => {
                                broke = true;
                                break;
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
                if !broke {
                    if let Some(b) = els {
                        self.exec_block(b)?;
                    }
                }
            }
            SK::Def(def) => {
                let mut defaults = Vec::new();
                for d in &def.defaults {
                    defaults.push(self.eval(d)?);
                }
                let f = Val::Func(Rc::new(FuncObj { def: def.clone(), defaults }));
                self.set_name(&def.name, f);
            }
            SK::Return(e) => {
                let v = match e {
                    Some(e) => self.eval(e)?,
                    None => Val::None,
                };
                return Err(Flow::Return(v));
            }
            SK::Break => return Err(Flow::Break),
            SK::Continue => return Err(Flow::Continue),
            SK::Pass => {}
            SK::Import(m, alias) => {
                let name = MODULES.iter().find(|&&x| x == m.as_ref()).ok_or(Exc {
                    kind: "ImportError".into(),
                    msg: format!("no module named '{m}'"),
                    line: 0,
                })?;
                let bind = alias.clone().unwrap_or_else(|| m.clone());
                self.set_name(&bind, Val::Module(name));
            }
            SK::FromImport(m, names) => {
                let module = MODULES.iter().find(|&&x| x == m.as_ref()).ok_or(Exc {
                    kind: "ImportError".into(),
                    msg: format!("no module named '{m}'"),
                    line: 0,
                })?;
                if names.len() == 1 && names[0].as_ref() == "*" {
                    for f in module_fns(module) {
                        let short: Rc<str> = f.split('.').nth(1).unwrap().into();
                        self.set_name(&short, Val::Builtin(f));
                    }
                    if *module == "math" {
                        for c in ["pi", "e", "tau", "inf", "nan"] {
                            let v = module_attr("math", c).unwrap();
                            self.set_name(&c.into(), v);
                        }
                    }
                } else {
                    for n in names {
                        let v = module_attr(module, n).ok_or(Exc {
                            kind: "ImportError".into(),
                            msg: format!("can't import name {n}"),
                            line: 0,
                        })?;
                        self.set_name(n, v);
                    }
                }
            }
            SK::Global(names) => {
                if let Some(f) = self.frames.last_mut() {
                    for n in names {
                        f.globals.insert(n.clone());
                    }
                }
            }
            SK::Del(ts) => {
                for t in ts {
                    match t {
                        Target::Name(n) => self.del_name(n)?,
                        Target::Index(o, i) => {
                            let o = self.eval(o)?;
                            let i = self.eval(i)?;
                            match &o {
                                Val::List(l) => {
                                    let len = l.borrow().len();
                                    let idx = norm_index(need_int(&i)?, len).ok_or(Exc {
                                        kind: "IndexError".into(),
                                        msg: "list index out of range".into(),
                                        line: 0,
                                    })?;
                                    l.borrow_mut().remove(idx);
                                }
                                Val::Dict(d) => {
                                    let pos = d.borrow().iter().position(|(k, _)| py_eq(k, &i));
                                    match pos {
                                        Some(p) => {
                                            d.borrow_mut().remove(p);
                                        }
                                        None => return Err(Flow::Exc(Exc { kind: "KeyError".into(), msg: repr(&i), line: 0 })),
                                    }
                                }
                                _ => return Err(Flow::Exc(Exc { kind: "TypeError".into(), msg: "object doesn't support item deletion".into(), line: 0 })),
                            }
                        }
                        _ => {}
                    }
                }
            }
            SK::Assert(c, msg) => {
                let v = self.eval(c)?;
                if !truthy(&v) {
                    let m = match msg {
                        Some(m) => to_str(&self.eval(m)?),
                        None => String::new(),
                    };
                    return Err(Flow::Exc(Exc { kind: "AssertionError".into(), msg: m, line: 0 }));
                }
            }
            SK::Raise(e) => {
                let v = match e {
                    Some(e) => self.eval(e)?,
                    None => return Err(Flow::Exc(Exc { kind: "RuntimeError".into(), msg: "No active exception to reraise".into(), line: 0 })),
                };
                let (kind, msg) = match v {
                    Val::Exc(e) => (e.0.clone(), e.1.clone()),
                    Val::Builtin(b) if EXC_TYPES.contains(&b) => (b.to_string(), String::new()),
                    _ => ("TypeError".to_string(), "exceptions must derive from BaseException".to_string()),
                };
                return Err(Flow::Exc(Exc { kind, msg, line: 0 }));
            }
            SK::Try(body, handlers, fin) => {
                let r = self.exec_block(body);
                let r = match r {
                    Err(Flow::Exc(e)) if e.kind != "KeyboardInterrupt" => {
                        let h = handlers.iter().find(|(ty, _, _)| match ty {
                            None => true,
                            Some(t) => t.as_ref() == "Exception" || t.as_ref() == e.kind,
                        });
                        match h {
                            Some((_, alias, hbody)) => {
                                if let Some(a) = alias {
                                    self.set_name(a, Val::Exc(Rc::new((e.kind.clone(), e.msg.clone()))));
                                }
                                self.exec_block(hbody)
                            }
                            None => Err(Flow::Exc(e)),
                        }
                    }
                    other => other,
                };
                if let Some(f) = fin {
                    self.exec_block(f)?;
                }
                return r;
            }
        }
        Ok(())
    }

    fn iterate(&mut self, v: &Val) -> R<Vec<Val>> {
        match v {
            Val::List(l) => Ok(l.borrow().clone()),
            Val::Tuple(t) => Ok(t.as_ref().clone()),
            Val::Str(x) => Ok(x.chars().map(|c| s(&c.to_string())).collect()),
            Val::Range(a, b, st) => {
                let n = range_len(*a, *b, *st);
                if n > 10_000_000 {
                    return err("MemoryError", "range too large");
                }
                Ok((0..n).map(|k| Val::Int(a + k * st)).collect())
            }
            Val::Dict(d) => Ok(d.borrow().iter().map(|(k, _)| k.clone()).collect()),
            _ => err("TypeError", format!("'{}' object isn't iterable", type_name(v))),
        }
    }

    fn get_item(&mut self, o: &Val, i: &Val) -> R<Val> {
        match o {
            Val::List(_) | Val::Tuple(_) | Val::Str(_) | Val::Range(..) => {
                let idx = need_int(i)?;
                let oob = |what: &str| Exc { kind: "IndexError".into(), msg: format!("{what} index out of range"), line: 0 };
                match o {
                    Val::List(l) => {
                        let l = l.borrow();
                        norm_index(idx, l.len()).map(|k| l[k].clone()).ok_or_else(|| oob("list"))
                    }
                    Val::Tuple(t) => norm_index(idx, t.len()).map(|k| t[k].clone()).ok_or_else(|| oob("tuple")),
                    Val::Str(x) => {
                        let cs: Vec<char> = x.chars().collect();
                        norm_index(idx, cs.len()).map(|k| s(&cs[k].to_string())).ok_or_else(|| oob("string"))
                    }
                    Val::Range(a, b, st) => {
                        let n = range_len(*a, *b, *st);
                        norm_index(idx, n as usize).map(|k| Val::Int(a + k as i64 * st)).ok_or_else(|| oob("range"))
                    }
                    _ => unreachable!(),
                }
            }
            Val::Dict(d) => d
                .borrow()
                .iter()
                .find(|(k, _)| py_eq(k, i))
                .map(|(_, v)| v.clone())
                .ok_or(Exc { kind: "KeyError".into(), msg: repr(i), line: 0 }),
            _ => err("TypeError", format!("'{}' object isn't subscriptable", type_name(o))),
        }
    }

    fn eval_opt_int(&mut self, e: &Option<Box<Ex>>) -> R<Option<i64>> {
        match e {
            Some(e) => {
                let v = self.eval(e)?;
                match v {
                    Val::None => Ok(None),
                    v => Ok(Some(need_int(&v)?)),
                }
            }
            None => Ok(None),
        }
    }

    pub fn eval(&mut self, e: &Ex) -> R<Val> {
        match e {
            Ex::Int(i) => Ok(Val::Int(*i)),
            Ex::Float(f) => Ok(Val::Float(*f)),
            Ex::Str(x) => Ok(Val::Str(x.clone())),
            Ex::Bool(b) => Ok(Val::Bool(*b)),
            Ex::None => Ok(Val::None),
            Ex::Name(n) => self.lookup(n),
            Ex::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    out.push(self.eval(it)?);
                }
                Ok(list(out))
            }
            Ex::Tuple(items) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    out.push(self.eval(it)?);
                }
                Ok(Val::Tuple(Rc::new(out)))
            }
            Ex::Dict(items) => {
                let mut out: Vec<(Val, Val)> = Vec::new();
                for (k, v) in items {
                    let k = self.eval(k)?;
                    let v = self.eval(v)?;
                    if let Some(e) = out.iter_mut().find(|(k2, _)| py_eq(k2, &k)) {
                        e.1 = v;
                    } else {
                        out.push((k, v));
                    }
                }
                Ok(Val::Dict(Rc::new(RefCell::new(out))))
            }
            Ex::FStr(parts) => {
                let mut out = String::new();
                for p in parts {
                    match p {
                        FPart::Lit(l) => out.push_str(l),
                        FPart::Expr(e, spec) => {
                            let v = self.eval(e)?;
                            match spec {
                                Some(sp) => out.push_str(&format_spec(&v, sp)?),
                                None => out.push_str(&to_str(&v)),
                            }
                        }
                    }
                }
                Ok(s(&out))
            }
            Ex::Bin(op, a, b) => {
                let va = self.eval(a)?;
                let vb = self.eval(b)?;
                self.binop(*op, &va, &vb)
            }
            Ex::Neg(a) => match self.eval(a)? {
                Val::Int(i) => i.checked_neg().map(Val::Int).ok_or(Exc { kind: "OverflowError".into(), msg: "integer overflow".into(), line: 0 }),
                Val::Bool(b) => Ok(Val::Int(-(b as i64))),
                Val::Float(f) => Ok(Val::Float(-f)),
                v => err("TypeError", format!("unsupported type for -: '{}'", type_name(&v))),
            },
            Ex::Pos(a) => {
                let v = self.eval(a)?;
                need_f64(&v)?;
                Ok(v)
            }
            Ex::Invert(a) => {
                let v = self.eval(a)?;
                Ok(Val::Int(!need_int(&v)?))
            }
            Ex::Not(a) => {
                let v = self.eval(a)?;
                Ok(Val::Bool(!truthy(&v)))
            }
            Ex::And(a, b) => {
                let va = self.eval(a)?;
                if !truthy(&va) {
                    return Ok(va);
                }
                self.eval(b)
            }
            Ex::Or(a, b) => {
                let va = self.eval(a)?;
                if truthy(&va) {
                    return Ok(va);
                }
                self.eval(b)
            }
            Ex::Cmp(first, rest) => {
                let mut left = self.eval(first)?;
                for (op, re) in rest {
                    let right = self.eval(re)?;
                    let ok = match op {
                        COp::Eq => py_eq(&left, &right),
                        COp::Ne => !py_eq(&left, &right),
                        COp::Lt => py_cmp(&left, &right)? == Ordering::Less,
                        COp::Le => py_cmp(&left, &right)? != Ordering::Greater,
                        COp::Gt => py_cmp(&left, &right)? == Ordering::Greater,
                        COp::Ge => py_cmp(&left, &right)? != Ordering::Less,
                        COp::In => self.contains(&right, &left)?,
                        COp::NotIn => !self.contains(&right, &left)?,
                        COp::Is => is_same(&left, &right),
                        COp::IsNot => !is_same(&left, &right),
                    };
                    if !ok {
                        return Ok(Val::Bool(false));
                    }
                    left = right;
                }
                Ok(Val::Bool(true))
            }
            Ex::IfExp(c, a, b) => {
                let cv = self.eval(c)?;
                if truthy(&cv) {
                    self.eval(a)
                } else {
                    self.eval(b)
                }
            }
            Ex::Call(f, args, kwargs) => {
                let fv = self.eval(f)?;
                let mut av = Vec::with_capacity(args.len());
                for a in args {
                    av.push(self.eval(a)?);
                }
                let mut kv = Vec::with_capacity(kwargs.len());
                for (k, v) in kwargs {
                    kv.push((k.clone(), self.eval(v)?));
                }
                self.call(&fv, av, kv)
            }
            Ex::Index(o, i) => {
                let ov = self.eval(o)?;
                let iv = self.eval(i)?;
                self.get_item(&ov, &iv)
            }
            Ex::Slice(o, a, b, st) => {
                let ov = self.eval(o)?;
                let a = self.eval_opt_int(a)?;
                let b = self.eval_opt_int(b)?;
                let st = self.eval_opt_int(st)?;
                match &ov {
                    Val::List(l) => {
                        let l = l.borrow();
                        let idx = slice_indices(l.len(), a, b, st)?;
                        Ok(list(idx.into_iter().map(|k| l[k].clone()).collect()))
                    }
                    Val::Tuple(t) => {
                        let idx = slice_indices(t.len(), a, b, st)?;
                        Ok(Val::Tuple(Rc::new(idx.into_iter().map(|k| t[k].clone()).collect())))
                    }
                    Val::Str(x) => {
                        let cs: Vec<char> = x.chars().collect();
                        let idx = slice_indices(cs.len(), a, b, st)?;
                        Ok(s(&idx.into_iter().map(|k| cs[k]).collect::<String>()))
                    }
                    _ => err("TypeError", format!("'{}' object isn't subscriptable", type_name(&ov))),
                }
            }
            Ex::Attr(o, name) => {
                let ov = self.eval(o)?;
                self.get_attr(&ov, name)
            }
            Ex::ListComp(elt, t, it, cond) => {
                let iv = self.eval(it)?;
                let items = self.iterate(&iv)?;
                let mut out = Vec::new();
                for item in items {
                    self.tick()?;
                    self.assign(t, item)?;
                    if let Some(c) = cond {
                        let cv = self.eval(c)?;
                        if !truthy(&cv) {
                            continue;
                        }
                    }
                    out.push(self.eval(elt)?);
                }
                Ok(list(out))
            }
            Ex::Lambda(def) => {
                let mut defaults = Vec::new();
                for d in &def.defaults {
                    defaults.push(self.eval(d)?);
                }
                Ok(Val::Func(Rc::new(FuncObj { def: def.clone(), defaults })))
            }
        }
    }

    fn contains(&mut self, container: &Val, item: &Val) -> R<bool> {
        match container {
            Val::Str(x) => match item {
                Val::Str(y) => Ok(x.contains(y.as_ref())),
                _ => err("TypeError", "'in <string>' requires string as left operand"),
            },
            Val::Range(a, b, st) => {
                let Some(i) = as_int(item) else { return Ok(false) };
                let n = range_len(*a, *b, *st);
                Ok(n > 0 && (i - a) % st == 0 && (i - a) / st >= 0 && (i - a) / st < n)
            }
            _ => {
                let items = self.iterate(container)?;
                Ok(items.iter().any(|x| py_eq(x, item)))
            }
        }
    }

    fn get_attr(&mut self, o: &Val, name: &Rc<str>) -> R<Val> {
        match o {
            Val::Module(m) => module_attr(m, name).ok_or(Exc {
                kind: "AttributeError".into(),
                msg: format!("'module' object has no attribute '{name}'"),
                line: 0,
            }),
            Val::List(_) | Val::Str(_) | Val::Dict(_) | Val::Tuple(_) => {
                Ok(Val::Method(Rc::new((o.clone(), name.clone()))))
            }
            Val::Exc(e) if name.as_ref() == "args" => Ok(Val::Tuple(Rc::new(vec![s(&e.1)]))),
            _ => err(
                "AttributeError",
                format!("'{}' object has no attribute '{name}'", type_name(o)),
            ),
        }
    }

    fn binop(&mut self, op: BOp, a: &Val, b: &Val) -> R<Val> {
        let overflow = || Exc { kind: "OverflowError".into(), msg: "integer overflow".into(), line: 0 };
        let zero = |m: &str| Exc { kind: "ZeroDivisionError".into(), msg: m.into(), line: 0 };
        if let (Some(x), Some(y)) = (as_int(a), as_int(b)) {
            return match op {
                BOp::Add => x.checked_add(y).map(Val::Int).ok_or_else(overflow),
                BOp::Sub => x.checked_sub(y).map(Val::Int).ok_or_else(overflow),
                BOp::Mul => x.checked_mul(y).map(Val::Int).ok_or_else(overflow),
                BOp::Div => {
                    if y == 0 {
                        Err(zero("division by zero"))
                    } else {
                        Ok(Val::Float(x as f64 / y as f64))
                    }
                }
                BOp::FloorDiv => {
                    if y == 0 {
                        return Err(zero("integer division by zero"));
                    }
                    let q = x / y;
                    Ok(Val::Int(if x % y != 0 && ((x < 0) != (y < 0)) { q - 1 } else { q }))
                }
                BOp::Mod => {
                    if y == 0 {
                        return Err(zero("integer modulo by zero"));
                    }
                    let r = x % y;
                    Ok(Val::Int(if r != 0 && ((r < 0) != (y < 0)) { r + y } else { r }))
                }
                BOp::Pow => {
                    if y < 0 {
                        if x == 0 {
                            return Err(zero("0.0 cannot be raised to a negative power"));
                        }
                        Ok(Val::Float((x as f64).powf(y as f64)))
                    } else {
                        u32::try_from(y)
                            .ok()
                            .and_then(|y| x.checked_pow(y))
                            .map(Val::Int)
                            .ok_or_else(overflow)
                    }
                }
                BOp::BitAnd => Ok(Val::Int(x & y)),
                BOp::BitOr => Ok(Val::Int(x | y)),
                BOp::BitXor => Ok(Val::Int(x ^ y)),
            };
        }
        if let (Some(x), Some(y)) = (as_f64(a), as_f64(b)) {
            let r = match op {
                BOp::Add => x + y,
                BOp::Sub => x - y,
                BOp::Mul => x * y,
                BOp::Div => {
                    if y == 0.0 {
                        return Err(zero("float division by zero"));
                    }
                    x / y
                }
                BOp::FloorDiv => {
                    if y == 0.0 {
                        return Err(zero("float divmod()"));
                    }
                    (x / y).floor()
                }
                BOp::Mod => {
                    if y == 0.0 {
                        return Err(zero("float modulo"));
                    }
                    x - y * (x / y).floor()
                }
                BOp::Pow => {
                    if x == 0.0 && y < 0.0 {
                        return Err(zero("0.0 cannot be raised to a negative power"));
                    }
                    if x < 0.0 && y.fract() != 0.0 {
                        return err("ValueError", "math domain error");
                    }
                    x.powf(y)
                }
                _ => return err("TypeError", "unsupported types for bitwise operation"),
            };
            return Ok(Val::Float(r));
        }
        match (op, a, b) {
            (BOp::Add, Val::Str(x), Val::Str(y)) => Ok(s(&format!("{x}{y}"))),
            (BOp::Add, Val::List(x), Val::List(y)) => {
                let mut v = x.borrow().clone();
                v.extend(y.borrow().iter().cloned());
                Ok(list(v))
            }
            (BOp::Add, Val::Tuple(x), Val::Tuple(y)) => {
                let mut v = x.as_ref().clone();
                v.extend(y.iter().cloned());
                Ok(Val::Tuple(Rc::new(v)))
            }
            (BOp::Mul, Val::Str(x), n) | (BOp::Mul, n, Val::Str(x)) if as_int(n).is_some() => {
                Ok(s(&x.repeat(as_int(n).unwrap().max(0) as usize)))
            }
            (BOp::Mul, Val::List(x), n) | (BOp::Mul, n, Val::List(x)) if as_int(n).is_some() => {
                let k = as_int(n).unwrap().max(0) as usize;
                let src = x.borrow();
                let mut v = Vec::with_capacity(src.len() * k);
                for _ in 0..k {
                    v.extend(src.iter().cloned());
                }
                Ok(list(v))
            }
            (BOp::Mod, Val::Str(f), args) => Ok(s(&percent_format(f, args)?)),
            _ => {
                let sym = match op {
                    BOp::Add => "+",
                    BOp::Sub => "-",
                    BOp::Mul => "*",
                    BOp::Div => "/",
                    BOp::FloorDiv => "//",
                    BOp::Mod => "%",
                    BOp::Pow => "**",
                    BOp::BitAnd => "&",
                    BOp::BitOr => "|",
                    BOp::BitXor => "^",
                };
                err(
                    "TypeError",
                    format!("unsupported types for {sym}: '{}', '{}'", type_name(a), type_name(b)),
                )
            }
        }
    }

    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        let v = self.rng.wrapping_mul(0x2545F4914F6CDD1D);
        (v >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn call(&mut self, f: &Val, args: Vec<Val>, kwargs: Vec<(Rc<str>, Val)>) -> R<Val> {
        match f {
            Val::Func(fo) => self.call_func(fo, args, kwargs),
            Val::Builtin(name) => self.call_builtin(name, args, kwargs),
            Val::Method(m) => self.call_method(&m.0, &m.1, args, kwargs),
            _ => err("TypeError", format!("'{}' object isn't callable", type_name(f))),
        }
    }

    fn call_func(&mut self, fo: &Rc<FuncObj>, args: Vec<Val>, kwargs: Vec<(Rc<str>, Val)>) -> R<Val> {
        let def = &fo.def;
        if self.frames.len() >= 150 {
            return err("RuntimeError", "maximum recursion depth exceeded");
        }
        let np = def.params.len();
        if args.len() > np {
            return err(
                "TypeError",
                format!("function takes {} positional arguments but {} were given", np, args.len()),
            );
        }
        let mut locals: HashMap<Rc<str>, Val> = HashMap::new();
        let nargs = args.len();
        for (p, a) in def.params.iter().zip(args) {
            locals.insert(p.clone(), a);
        }
        for (k, v) in kwargs {
            if !def.params.contains(&k) {
                return err("TypeError", format!("unexpected keyword argument '{k}'"));
            }
            locals.insert(k, v);
        }
        let first_default = np - fo.defaults.len();
        for (i, p) in def.params.iter().enumerate().skip(nargs) {
            if !locals.contains_key(p) {
                if i >= first_default {
                    locals.insert(p.clone(), fo.defaults[i - first_default].clone());
                } else {
                    return err("TypeError", format!("function missing required positional argument '{p}'"));
                }
            }
        }
        self.frames.push(Frame { locals, globals: HashSet::new() });
        let saved_line = self.line;
        let r = self.exec_block(&def.body);
        self.frames.pop();
        self.line = saved_line;
        match r {
            Ok(()) => Ok(Val::None),
            Err(Flow::Return(v)) => Ok(v),
            Err(Flow::Exc(e)) => Err(e),
            Err(_) => err("SyntaxError", "'break' outside loop"),
        }
    }

    fn kwarg(kwargs: &[(Rc<str>, Val)], name: &str) -> Option<Val> {
        kwargs.iter().find(|(k, _)| k.as_ref() == name).map(|(_, v)| v.clone())
    }

    fn sort_vals(&mut self, items: &mut [Val], key: Option<Val>, reverse: bool) -> R<()> {
        let keys: Vec<Val> = match &key {
            Some(k) if !matches!(k, Val::None) => {
                let mut ks = Vec::with_capacity(items.len());
                for it in items.iter() {
                    ks.push(self.call(k, vec![it.clone()], vec![])?);
                }
                ks
            }
            _ => items.to_vec(),
        };
        let mut idx: Vec<usize> = (0..items.len()).collect();
        let mut error = None;
        idx.sort_by(|&a, &b| match py_cmp(&keys[a], &keys[b]) {
            Ok(o) => {
                if reverse {
                    o.reverse()
                } else {
                    o
                }
            }
            Err(e) => {
                error.get_or_insert(e);
                Ordering::Equal
            }
        });
        if let Some(e) = error {
            return Err(e);
        }
        let sorted: Vec<Val> = idx.into_iter().map(|i| items[i].clone()).collect();
        items.clone_from_slice(&sorted);
        Ok(())
    }

    fn call_builtin(&mut self, name: &str, mut args: Vec<Val>, kwargs: Vec<(Rc<str>, Val)>) -> R<Val> {
        let n = args.len();
        let arity = |lo: usize, hi: usize| -> R<()> {
            if n < lo || n > hi {
                err("TypeError", format!("function takes {lo} positional arguments but {n} were given"))
            } else {
                Ok(())
            }
        };
        if EXC_TYPES.contains(&name) {
            let msg = args.first().map(to_str).unwrap_or_default();
            return Ok(Val::Exc(Rc::new((name.to_string(), msg))));
        }
        let fnum = |v: &Val| need_f64(v);
        match name {
            "print" => {
                let sep = Self::kwarg(&kwargs, "sep").map(|v| to_str(&v)).unwrap_or(" ".into());
                let end = Self::kwarg(&kwargs, "end").map(|v| to_str(&v)).unwrap_or("\n".into());
                let parts: Vec<String> = args.iter().map(to_str).collect();
                let line = format!("{}{}", parts.join(&sep), end);
                self.print(&line);
                Ok(Val::None)
            }
            "input" => {
                let prompt = args.first().map(to_str).unwrap_or_default();
                match (self.input)(&prompt) {
                    Some(text) => Ok(s(&text)),
                    None => err("KeyboardInterrupt", ""),
                }
            }
            "len" => {
                arity(1, 1)?;
                let l = match &args[0] {
                    Val::Str(x) => x.chars().count() as i64,
                    Val::List(l) => l.borrow().len() as i64,
                    Val::Tuple(t) => t.len() as i64,
                    Val::Dict(d) => d.borrow().len() as i64,
                    Val::Range(a, b, st) => range_len(*a, *b, *st),
                    v => return err("TypeError", format!("object of type '{}' has no len()", type_name(v))),
                };
                Ok(Val::Int(l))
            }
            "range" => {
                arity(1, 3)?;
                let ints: Vec<i64> = args.iter().map(need_int).collect::<R<_>>()?;
                let (a, b, st) = match ints.len() {
                    1 => (0, ints[0], 1),
                    2 => (ints[0], ints[1], 1),
                    _ => (ints[0], ints[1], ints[2]),
                };
                if st == 0 {
                    return err("ValueError", "range() arg 3 must not be zero");
                }
                Ok(Val::Range(a, b, st))
            }
            "int" => {
                if n == 0 {
                    return Ok(Val::Int(0));
                }
                match &args[0] {
                    Val::Int(i) => Ok(Val::Int(*i)),
                    Val::Bool(b) => Ok(Val::Int(*b as i64)),
                    Val::Float(f) => {
                        if !f.is_finite() {
                            return err("OverflowError", "can't convert inf/nan to int");
                        }
                        Ok(Val::Int(f.trunc() as i64))
                    }
                    Val::Str(x) => {
                        let base = if n > 1 { need_int(&args[1])? as u32 } else { 10 };
                        i64::from_str_radix(x.trim(), base).map(Val::Int).or_else(|_| {
                            err("ValueError", format!("invalid syntax for integer with base {base}: {}", repr(&args[0])))
                        })
                    }
                    v => err("TypeError", format!("can't convert {} to int", type_name(v))),
                }
            }
            "float" => {
                if n == 0 {
                    return Ok(Val::Float(0.0));
                }
                match &args[0] {
                    Val::Str(x) => {
                        let t = x.trim().to_lowercase();
                        match t.as_str() {
                            "inf" | "infinity" => Ok(Val::Float(f64::INFINITY)),
                            "-inf" => Ok(Val::Float(f64::NEG_INFINITY)),
                            "nan" => Ok(Val::Float(f64::NAN)),
                            _ => t.parse().map(Val::Float).or_else(|_| {
                                err("ValueError", format!("could not convert string to float: {}", repr(&args[0])))
                            }),
                        }
                    }
                    v => Ok(Val::Float(fnum(v)?)),
                }
            }
            "str" => Ok(s(&args.first().map(to_str).unwrap_or_default())),
            "repr" => {
                arity(1, 1)?;
                Ok(s(&repr(&args[0])))
            }
            "bool" => Ok(Val::Bool(args.first().map(truthy).unwrap_or(false))),
            "abs" => {
                arity(1, 1)?;
                match &args[0] {
                    Val::Int(i) => Ok(Val::Int(i.abs())),
                    Val::Bool(b) => Ok(Val::Int(*b as i64)),
                    v => Ok(Val::Float(fnum(v)?.abs())),
                }
            }
            "round" => {
                arity(1, 2)?;
                let digits = if n == 2 && !matches!(args[1], Val::None) { Some(need_int(&args[1])?) } else { None };
                match (&args[0], digits) {
                    (Val::Int(i), _) => Ok(Val::Int(*i)),
                    (v, None) => Ok(Val::Int(round_half_even(fnum(v)?) as i64)),
                    (v, Some(d)) => {
                        let m = 10f64.powi(d as i32);
                        let x = fnum(v)?;
                        Ok(Val::Float(round_half_even(x * m) / m))
                    }
                }
            }
            "min" | "max" => {
                let items = if n == 1 { self.iterate(&args[0])? } else { args.clone() };
                if items.is_empty() {
                    return err("ValueError", format!("{name}() arg is an empty sequence"));
                }
                let key = Self::kwarg(&kwargs, "key");
                let mut best = items[0].clone();
                let mut best_k = match &key {
                    Some(k) => self.call(k, vec![best.clone()], vec![])?,
                    None => best.clone(),
                };
                for it in items.into_iter().skip(1) {
                    let k = match &key {
                        Some(kf) => self.call(kf, vec![it.clone()], vec![])?,
                        None => it.clone(),
                    };
                    let o = py_cmp(&k, &best_k)?;
                    if (name == "min" && o == Ordering::Less) || (name == "max" && o == Ordering::Greater) {
                        best = it;
                        best_k = k;
                    }
                }
                Ok(best)
            }
            "sum" => {
                arity(1, 2)?;
                let items = self.iterate(&args[0])?;
                let mut acc = if n == 2 { args[1].clone() } else { Val::Int(0) };
                for it in items {
                    acc = self.binop(BOp::Add, &acc, &it)?;
                }
                Ok(acc)
            }
            "list" => {
                if n == 0 {
                    return Ok(list(vec![]));
                }
                Ok(list(self.iterate(&args[0])?))
            }
            "tuple" => {
                if n == 0 {
                    return Ok(Val::Tuple(Rc::new(vec![])));
                }
                Ok(Val::Tuple(Rc::new(self.iterate(&args[0])?)))
            }
            "dict" => {
                let mut out: Vec<(Val, Val)> = Vec::new();
                if n == 1 {
                    for item in self.iterate(&args[0])? {
                        let kv = self.iterate(&item)?;
                        if kv.len() != 2 {
                            return err("ValueError", "dictionary update sequence element has wrong length");
                        }
                        out.push((kv[0].clone(), kv[1].clone()));
                    }
                }
                for (k, v) in kwargs {
                    out.push((Val::Str(k), v));
                }
                Ok(Val::Dict(Rc::new(RefCell::new(out))))
            }
            "sorted" => {
                arity(1, 1)?;
                let mut items = self.iterate(&args[0])?;
                let rev = Self::kwarg(&kwargs, "reverse").map(|v| truthy(&v)).unwrap_or(false);
                self.sort_vals(&mut items, Self::kwarg(&kwargs, "key"), rev)?;
                Ok(list(items))
            }
            "reversed" => {
                arity(1, 1)?;
                let mut items = self.iterate(&args[0])?;
                items.reverse();
                Ok(list(items))
            }
            "enumerate" => {
                arity(1, 2)?;
                let start = if n == 2 { need_int(&args[1])? } else { 0 };
                let items = self.iterate(&args[0])?;
                Ok(list(
                    items
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| Val::Tuple(Rc::new(vec![Val::Int(start + i as i64), v])))
                        .collect(),
                ))
            }
            "zip" => {
                let lists: Vec<Vec<Val>> = args.iter().map(|a| self.iterate(a)).collect::<R<_>>()?;
                let len = lists.iter().map(|l| l.len()).min().unwrap_or(0);
                Ok(list(
                    (0..len)
                        .map(|i| Val::Tuple(Rc::new(lists.iter().map(|l| l[i].clone()).collect())))
                        .collect(),
                ))
            }
            "map" => {
                if n < 2 {
                    return err("TypeError", "map() requires at least 2 arguments");
                }
                let f = args.remove(0);
                let lists: Vec<Vec<Val>> = args.iter().map(|a| self.iterate(a)).collect::<R<_>>()?;
                let len = lists.iter().map(|l| l.len()).min().unwrap_or(0);
                let mut out = Vec::new();
                for i in 0..len {
                    let a: Vec<Val> = lists.iter().map(|l| l[i].clone()).collect();
                    out.push(self.call(&f, a, vec![])?);
                }
                Ok(list(out))
            }
            "filter" => {
                arity(2, 2)?;
                let items = self.iterate(&args[1])?;
                let mut out = Vec::new();
                for it in items {
                    let keep = match &args[0] {
                        Val::None => truthy(&it),
                        f => truthy(&self.call(f, vec![it.clone()], vec![])?),
                    };
                    if keep {
                        out.push(it);
                    }
                }
                Ok(list(out))
            }
            "any" | "all" => {
                arity(1, 1)?;
                let items = self.iterate(&args[0])?;
                Ok(Val::Bool(if name == "any" {
                    items.iter().any(truthy)
                } else {
                    items.iter().all(truthy)
                }))
            }
            "chr" => {
                arity(1, 1)?;
                let c = char::from_u32(need_int(&args[0])? as u32)
                    .ok_or(Exc { kind: "ValueError".into(), msg: "chr() arg not in range".into(), line: 0 })?;
                Ok(s(&c.to_string()))
            }
            "ord" => {
                arity(1, 1)?;
                match &args[0] {
                    Val::Str(x) if x.chars().count() == 1 => Ok(Val::Int(x.chars().next().unwrap() as i64)),
                    _ => err("TypeError", "ord() expected a character"),
                }
            }
            "divmod" => {
                arity(2, 2)?;
                let q = self.binop(BOp::FloorDiv, &args[0], &args[1])?;
                let r = self.binop(BOp::Mod, &args[0], &args[1])?;
                Ok(Val::Tuple(Rc::new(vec![q, r])))
            }
            "pow" => {
                arity(2, 3)?;
                if n == 3 {
                    let (b, e, m) = (need_int(&args[0])?, need_int(&args[1])?, need_int(&args[2])?);
                    if m == 0 {
                        return err("ValueError", "pow() 3rd argument cannot be 0");
                    }
                    let mut result: i128 = 1;
                    let mut base = (b as i128).rem_euclid(m as i128);
                    let mut e = e;
                    while e > 0 {
                        if e & 1 == 1 {
                            result = result * base % m as i128;
                        }
                        base = base * base % m as i128;
                        e >>= 1;
                    }
                    return Ok(Val::Int(result as i64));
                }
                self.binop(BOp::Pow, &args[0], &args[1])
            }
            "type" => {
                arity(1, 1)?;
                Ok(s(&format!("<class '{}'>", type_name(&args[0]))))
            }
            "isinstance" => {
                arity(2, 2)?;
                let tn = type_name(&args[0]);
                let check = |t: &Val| match t {
                    Val::Builtin(b) => *b == tn || (*b == "int" && tn == "bool") || (*b == "Exception" && tn == "Exception"),
                    _ => false,
                };
                Ok(Val::Bool(match &args[1] {
                    Val::Tuple(ts) => ts.iter().any(check),
                    t => check(t),
                }))
            }
            "hex" | "bin" | "oct" => {
                arity(1, 1)?;
                let i = need_int(&args[0])?;
                let (sign, a) = if i < 0 { ("-", i.unsigned_abs()) } else { ("", i as u64) };
                Ok(s(&match name {
                    "hex" => format!("{sign}0x{a:x}"),
                    "bin" => format!("{sign}0b{a:b}"),
                    _ => format!("{sign}0o{a:o}"),
                }))
            }
            "help" => {
                self.print("Modules: math, random, time\nBuiltins: print, input, range, len, ...\n");
                Ok(Val::None)
            }
            "exit" | "quit" => err("SystemExit", ""),
            _ if name.starts_with("math.") => self.call_math(&name[5..], &args),
            _ if name.starts_with("random.") => self.call_random(&name[7..], &args),
            "time.sleep" => {
                arity(1, 1)?;
                let secs = fnum(&args[0])?.max(0.0);
                let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(secs);
                while std::time::Instant::now() < until {
                    if self.interrupt.load(AO::Relaxed) {
                        return err("KeyboardInterrupt", "");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Ok(Val::None)
            }
            "time.monotonic" | "time.time" => Ok(Val::Float(self.start.elapsed().as_secs_f64())),
            _ => err("NameError", format!("name '{name}' isn't defined")),
        }
    }

    fn call_math(&mut self, f: &str, args: &[Val]) -> R<Val> {
        let x = || -> R<f64> {
            args.first().map_or_else(|| err("TypeError", "function takes 1 positional argument"), need_f64)
        };
        let y = || -> R<f64> {
            args.get(1).map_or_else(|| err("TypeError", "function takes 2 positional arguments"), need_f64)
        };
        let domain = || err::<Val>("ValueError", "math domain error");
        let v = match f {
            "sqrt" => {
                let v = x()?;
                if v < 0.0 {
                    return domain();
                }
                v.sqrt()
            }
            "sin" => x()?.sin(),
            "cos" => x()?.cos(),
            "tan" => x()?.tan(),
            "asin" | "acos" => {
                let v = x()?;
                if v.abs() > 1.0 {
                    return domain();
                }
                if f == "asin" { v.asin() } else { v.acos() }
            }
            "atan" => x()?.atan(),
            "atan2" => x()?.atan2(y()?),
            "sinh" => x()?.sinh(),
            "cosh" => x()?.cosh(),
            "tanh" => x()?.tanh(),
            "exp" => x()?.exp(),
            "log" | "log10" | "log2" => {
                let v = x()?;
                if v <= 0.0 {
                    return domain();
                }
                match f {
                    "log10" => v.log10(),
                    "log2" => v.log2(),
                    _ if args.len() == 2 => v.ln() / y()?.ln(),
                    _ => v.ln(),
                }
            }
            "floor" | "ceil" | "trunc" => {
                if let Some(i) = args.first().and_then(as_int) {
                    return Ok(Val::Int(i));
                }
                let v = x()?;
                let r = match f {
                    "floor" => v.floor(),
                    "ceil" => v.ceil(),
                    _ => v.trunc(),
                };
                return Ok(Val::Int(r as i64));
            }
            "fabs" => x()?.abs(),
            "pow" => x()?.powf(y()?),
            "fmod" => x()? % y()?,
            "copysign" => x()?.copysign(y()?),
            "hypot" => x()?.hypot(y()?),
            "degrees" => x()?.to_degrees(),
            "radians" => x()?.to_radians(),
            "isclose" => {
                let (a, b) = (x()?, y()?);
                return Ok(Val::Bool((a - b).abs() <= 1e-9 * a.abs().max(b.abs())));
            }
            "factorial" => {
                let k = need_int(args.first().unwrap_or(&Val::None))?;
                if k < 0 {
                    return err("ValueError", "factorial() not defined for negative values");
                }
                let mut acc: i64 = 1;
                for i in 2..=k {
                    acc = acc.checked_mul(i).ok_or(Exc { kind: "OverflowError".into(), msg: "integer overflow".into(), line: 0 })?;
                }
                return Ok(Val::Int(acc));
            }
            "gcd" => {
                let (mut a, mut b) = (
                    need_int(args.first().unwrap_or(&Val::None))?.abs(),
                    need_int(args.get(1).unwrap_or(&Val::None))?.abs(),
                );
                while b != 0 {
                    let t = a % b;
                    a = b;
                    b = t;
                }
                return Ok(Val::Int(a));
            }
            "isqrt" => {
                let k = need_int(args.first().unwrap_or(&Val::None))?;
                if k < 0 {
                    return err("ValueError", "isqrt() argument must be nonnegative");
                }
                let mut r = (k as f64).sqrt() as i64;
                while r * r > k {
                    r -= 1;
                }
                while (r + 1) * (r + 1) <= k {
                    r += 1;
                }
                return Ok(Val::Int(r));
            }
            "comb" | "perm" => {
                let nn = need_int(args.first().unwrap_or(&Val::None))?;
                let k = need_int(args.get(1).unwrap_or(&Val::None))?;
                if nn < 0 || k < 0 {
                    return err("ValueError", "must be non-negative integers");
                }
                if k > nn {
                    return Ok(Val::Int(0));
                }
                let mut acc: i128 = 1;
                if f == "perm" {
                    for i in 0..k {
                        acc *= (nn - i) as i128;
                    }
                } else {
                    let k = k.min(nn - k);
                    for i in 0..k {
                        acc = acc * (nn - i) as i128 / (i + 1) as i128;
                    }
                }
                return i64::try_from(acc).map(Val::Int).or_else(|_| err("OverflowError", "integer overflow"));
            }
            _ => return err("AttributeError", format!("'module' object has no attribute '{f}'")),
        };
        Ok(Val::Float(v))
    }

    fn call_random(&mut self, f: &str, args: &[Val]) -> R<Val> {
        match f {
            "random" => Ok(Val::Float(self.rand())),
            "seed" => {
                let seed = args.first().and_then(as_int).unwrap_or(0) as u64;
                self.rng = seed.wrapping_mul(0x9E3779B97F4A7C15) | 1;
                Ok(Val::None)
            }
            "uniform" => {
                let a = need_f64(args.first().unwrap_or(&Val::None))?;
                let b = need_f64(args.get(1).unwrap_or(&Val::None))?;
                Ok(Val::Float(a + (b - a) * self.rand()))
            }
            "randint" => {
                let a = need_int(args.first().unwrap_or(&Val::None))?;
                let b = need_int(args.get(1).unwrap_or(&Val::None))?;
                if b < a {
                    return err("ValueError", "empty range for randint()");
                }
                Ok(Val::Int(a + (self.rand() * (b - a + 1) as f64) as i64))
            }
            "randrange" => {
                let ints: Vec<i64> = args.iter().map(need_int).collect::<R<_>>()?;
                let (a, b, st) = match ints.len() {
                    1 => (0, ints[0], 1),
                    2 => (ints[0], ints[1], 1),
                    3 => (ints[0], ints[1], ints[2]),
                    _ => return err("TypeError", "randrange() takes 1 to 3 arguments"),
                };
                let nn = range_len(a, b, st);
                if nn <= 0 {
                    return err("ValueError", "empty range for randrange()");
                }
                Ok(Val::Int(a + st * (self.rand() * nn as f64) as i64))
            }
            "choice" => {
                let items = self.iterate(args.first().unwrap_or(&Val::None))?;
                if items.is_empty() {
                    return err("IndexError", "list index out of range");
                }
                let k = (self.rand() * items.len() as f64) as usize;
                Ok(items[k.min(items.len() - 1)].clone())
            }
            "shuffle" => {
                if let Some(Val::List(l)) = args.first() {
                    let len = l.borrow().len();
                    for i in (1..len).rev() {
                        let j = (self.rand() * (i + 1) as f64) as usize;
                        l.borrow_mut().swap(i, j.min(i));
                    }
                    Ok(Val::None)
                } else {
                    err("TypeError", "shuffle() argument must be a list")
                }
            }
            _ => err("AttributeError", format!("'module' object has no attribute '{f}'")),
        }
    }

    fn call_method(&mut self, recv: &Val, name: &str, args: Vec<Val>, kwargs: Vec<(Rc<str>, Val)>) -> R<Val> {
        let n = args.len();
        let no_attr = || {
            err(
                "AttributeError",
                format!("'{}' object has no attribute '{name}'", type_name(recv)),
            )
        };
        match recv {
            Val::List(l) => match name {
                "append" => {
                    l.borrow_mut().push(args.into_iter().next().unwrap_or(Val::None));
                    Ok(Val::None)
                }
                "extend" => {
                    let items = self.iterate(args.first().unwrap_or(&Val::None))?;
                    l.borrow_mut().extend(items);
                    Ok(Val::None)
                }
                "insert" => {
                    if n != 2 {
                        return err("TypeError", "insert() takes 2 arguments");
                    }
                    let len = l.borrow().len() as i64;
                    let mut i = need_int(&args[0])?;
                    if i < 0 {
                        i = (i + len).max(0);
                    }
                    let i = i.min(len) as usize;
                    l.borrow_mut().insert(i, args[1].clone());
                    Ok(Val::None)
                }
                "pop" => {
                    let len = l.borrow().len();
                    if len == 0 {
                        return err("IndexError", "pop from empty list");
                    }
                    let i = if n == 1 { need_int(&args[0])? } else { -1 };
                    let idx = norm_index(i, len)
                        .ok_or(Exc { kind: "IndexError".into(), msg: "pop index out of range".into(), line: 0 })?;
                    Ok(l.borrow_mut().remove(idx))
                }
                "remove" => {
                    let target = args.first().cloned().unwrap_or(Val::None);
                    let pos = l.borrow().iter().position(|x| py_eq(x, &target));
                    match pos {
                        Some(p) => {
                            l.borrow_mut().remove(p);
                            Ok(Val::None)
                        }
                        None => err("ValueError", "object not in sequence"),
                    }
                }
                "index" => {
                    let target = args.first().cloned().unwrap_or(Val::None);
                    let pos = l.borrow().iter().position(|x| py_eq(x, &target));
                    pos.map(|p| Val::Int(p as i64))
                        .ok_or(Exc { kind: "ValueError".into(), msg: "object not in sequence".into(), line: 0 })
                }
                "count" => {
                    let target = args.first().cloned().unwrap_or(Val::None);
                    Ok(Val::Int(l.borrow().iter().filter(|x| py_eq(x, &target)).count() as i64))
                }
                "reverse" => {
                    l.borrow_mut().reverse();
                    Ok(Val::None)
                }
                "sort" => {
                    let rev = Self::kwarg(&kwargs, "reverse").map(|v| truthy(&v)).unwrap_or(false);
                    let mut items = l.borrow().clone();
                    self.sort_vals(&mut items, Self::kwarg(&kwargs, "key"), rev)?;
                    *l.borrow_mut() = items;
                    Ok(Val::None)
                }
                "copy" => Ok(list(l.borrow().clone())),
                "clear" => {
                    l.borrow_mut().clear();
                    Ok(Val::None)
                }
                _ => no_attr(),
            },
            Val::Tuple(t) => match name {
                "index" => {
                    let target = args.first().cloned().unwrap_or(Val::None);
                    t.iter()
                        .position(|x| py_eq(x, &target))
                        .map(|p| Val::Int(p as i64))
                        .ok_or(Exc { kind: "ValueError".into(), msg: "object not in sequence".into(), line: 0 })
                }
                "count" => {
                    let target = args.first().cloned().unwrap_or(Val::None);
                    Ok(Val::Int(t.iter().filter(|x| py_eq(x, &target)).count() as i64))
                }
                _ => no_attr(),
            },
            Val::Dict(d) => match name {
                "keys" => Ok(list(d.borrow().iter().map(|(k, _)| k.clone()).collect())),
                "values" => Ok(list(d.borrow().iter().map(|(_, v)| v.clone()).collect())),
                "items" => Ok(list(
                    d.borrow()
                        .iter()
                        .map(|(k, v)| Val::Tuple(Rc::new(vec![k.clone(), v.clone()])))
                        .collect(),
                )),
                "get" => {
                    let key = args.first().cloned().unwrap_or(Val::None);
                    let default = args.get(1).cloned().unwrap_or(Val::None);
                    Ok(d.borrow().iter().find(|(k, _)| py_eq(k, &key)).map(|(_, v)| v.clone()).unwrap_or(default))
                }
                "pop" => {
                    let key = args.first().cloned().unwrap_or(Val::None);
                    let pos = d.borrow().iter().position(|(k, _)| py_eq(k, &key));
                    match pos {
                        Some(p) => Ok(d.borrow_mut().remove(p).1),
                        None => args.get(1).cloned().ok_or(Exc { kind: "KeyError".into(), msg: repr(&key), line: 0 }),
                    }
                }
                "clear" => {
                    d.borrow_mut().clear();
                    Ok(Val::None)
                }
                _ => no_attr(),
            },
            Val::Str(x) => {
                let arg_str = |i: usize| -> R<String> {
                    match args.get(i) {
                        Some(Val::Str(a)) => Ok(a.to_string()),
                        _ => err("TypeError", "expected a string argument"),
                    }
                };
                match name {
                    "upper" => Ok(s(&x.to_uppercase())),
                    "lower" => Ok(s(&x.to_lowercase())),
                    "strip" => Ok(s(x.trim())),
                    "lstrip" => Ok(s(x.trim_start())),
                    "rstrip" => Ok(s(x.trim_end())),
                    "capitalize" => {
                        let mut c = x.chars();
                        Ok(s(&match c.next() {
                            Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                            None => String::new(),
                        }))
                    }
                    "title" => Ok(s(&x
                        .split(' ')
                        .map(|w| {
                            let mut c = w.chars();
                            match c.next() {
                                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                                None => String::new(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" "))),
                    "isdigit" => Ok(Val::Bool(!x.is_empty() && x.chars().all(|c| c.is_ascii_digit()))),
                    "isalpha" => Ok(Val::Bool(!x.is_empty() && x.chars().all(|c| c.is_alphabetic()))),
                    "split" => {
                        let parts: Vec<Val> = match args.first() {
                            Some(Val::Str(sep)) => x.split(sep.as_ref()).map(s).collect(),
                            _ => x.split_whitespace().map(s).collect(),
                        };
                        Ok(list(parts))
                    }
                    "join" => {
                        let items = self.iterate(args.first().unwrap_or(&Val::None))?;
                        let mut parts = Vec::new();
                        for it in items {
                            match it {
                                Val::Str(p) => parts.push(p.to_string()),
                                other => return err("TypeError", format!("join expects strings, got '{}'", type_name(&other))),
                            }
                        }
                        Ok(s(&parts.join(x)))
                    }
                    "replace" => Ok(s(&x.replace(&arg_str(0)?, &arg_str(1)?))),
                    "find" => {
                        let needle = arg_str(0)?;
                        Ok(Val::Int(match x.find(&needle) {
                            Some(b) => x[..b].chars().count() as i64,
                            None => -1,
                        }))
                    }
                    "index" => {
                        let needle = arg_str(0)?;
                        match x.find(&needle) {
                            Some(b) => Ok(Val::Int(x[..b].chars().count() as i64)),
                            None => err("ValueError", "substring not found"),
                        }
                    }
                    "count" => {
                        let needle = arg_str(0)?;
                        Ok(Val::Int(if needle.is_empty() { x.chars().count() as i64 + 1 } else { x.matches(&needle).count() as i64 }))
                    }
                    "startswith" => Ok(Val::Bool(x.starts_with(&arg_str(0)?))),
                    "endswith" => Ok(Val::Bool(x.ends_with(&arg_str(0)?))),
                    "center" | "ljust" | "rjust" | "zfill" => {
                        let w = need_int(args.first().unwrap_or(&Val::None))?.max(0) as usize;
                        let spec = match name {
                            "center" => format!("^{w}"),
                            "ljust" => format!("<{w}"),
                            "rjust" => format!(">{w}"),
                            _ => format!("0>{w}"),
                        };
                        Ok(s(&format_spec(recv, &spec)?))
                    }
                    "format" => {
                        let mut out = String::new();
                        let chars: Vec<char> = x.chars().collect();
                        let mut i = 0;
                        let mut auto = 0;
                        while i < chars.len() {
                            let c = chars[i];
                            if c == '{' && chars.get(i + 1) == Some(&'{') {
                                out.push('{');
                                i += 2;
                            } else if c == '}' && chars.get(i + 1) == Some(&'}') {
                                out.push('}');
                                i += 2;
                            } else if c == '{' {
                                let end = chars[i..].iter().position(|&c| c == '}').map(|p| p + i).ok_or(Exc {
                                    kind: "ValueError".into(),
                                    msg: "unmatched '{' in format".into(),
                                    line: 0,
                                })?;
                                let inner: String = chars[i + 1..end].iter().collect();
                                let (field, spec) = match inner.split_once(':') {
                                    Some((f, sp)) => (f.to_string(), Some(sp.to_string())),
                                    None => (inner, None),
                                };
                                let v = if field.is_empty() {
                                    auto += 1;
                                    args.get(auto - 1).cloned()
                                } else if let Ok(k) = field.parse::<usize>() {
                                    args.get(k).cloned()
                                } else {
                                    Self::kwarg(&kwargs, &field)
                                }
                                .ok_or(Exc { kind: "IndexError".into(), msg: "tuple index out of range".into(), line: 0 })?;
                                match spec {
                                    Some(sp) => out.push_str(&format_spec(&v, &sp)?),
                                    None => out.push_str(&to_str(&v)),
                                }
                                i = end + 1;
                            } else {
                                out.push(c);
                                i += 1;
                            }
                        }
                        Ok(s(&out))
                    }
                    _ => no_attr(),
                }
            }
            _ => no_attr(),
        }
    }
}

fn is_same(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::List(x), Val::List(y)) => Rc::ptr_eq(x, y),
        (Val::Dict(x), Val::Dict(y)) => Rc::ptr_eq(x, y),
        _ => py_eq(a, b),
    }
}
