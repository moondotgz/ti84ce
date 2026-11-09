//! Recursive-descent parser for the Python subset.

use super::lexer::{Token, T};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum COp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    NotIn,
    Is,
    IsNot,
}

#[derive(Debug, Clone)]
pub enum Ex {
    Int(i64),
    Float(f64),
    Str(Rc<str>),
    Bool(bool),
    None,
    Name(Rc<str>),
    List(Vec<Ex>),
    Tuple(Vec<Ex>),
    Dict(Vec<(Ex, Ex)>),
    Bin(BOp, Box<Ex>, Box<Ex>),
    Neg(Box<Ex>),
    Pos(Box<Ex>),
    Invert(Box<Ex>),
    Not(Box<Ex>),
    And(Box<Ex>, Box<Ex>),
    Or(Box<Ex>, Box<Ex>),
    Cmp(Box<Ex>, Vec<(COp, Ex)>),
    IfExp(Box<Ex>, Box<Ex>, Box<Ex>),
    Call(Box<Ex>, Vec<Ex>, Vec<(Rc<str>, Ex)>),
    Index(Box<Ex>, Box<Ex>),
    Slice(Box<Ex>, Option<Box<Ex>>, Option<Box<Ex>>, Option<Box<Ex>>),
    Attr(Box<Ex>, Rc<str>),
    ListComp(Box<Ex>, Box<Target>, Box<Ex>, Option<Box<Ex>>),
    Lambda(Rc<FuncDef>),
    FStr(Vec<FPart>),
}

#[derive(Debug, Clone)]
pub enum FPart {
    Lit(Rc<str>),
    Expr(Ex, Option<Rc<str>>),
}

#[derive(Debug, Clone)]
pub enum Target {
    Name(Rc<str>),
    Index(Ex, Ex),
    Attr(Rc<str>),
    Tuple(Vec<Target>),
}

#[derive(Debug)]
pub struct FuncDef {
    pub name: Rc<str>,
    pub params: Vec<Rc<str>>,
    pub defaults: Vec<Ex>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub line: usize,
    pub kind: SK,
}

#[derive(Debug, Clone)]
pub enum SK {
    Expr(Ex),
    Assign(Vec<Target>, Ex),
    AugAssign(Target, BOp, Ex),
    If(Vec<(Ex, Vec<Stmt>)>, Option<Vec<Stmt>>),
    While(Ex, Vec<Stmt>, Option<Vec<Stmt>>),
    For(Target, Ex, Vec<Stmt>, Option<Vec<Stmt>>),
    Def(Rc<FuncDef>),
    Return(Option<Ex>),
    Break,
    Continue,
    Pass,
    Import(Rc<str>, Option<Rc<str>>),
    FromImport(Rc<str>, Vec<Rc<str>>),
    Global(Vec<Rc<str>>),
    Del(Vec<Target>),
    Assert(Ex, Option<Ex>),
    Raise(Option<Ex>),
    Try(Vec<Stmt>, Vec<(Option<Rc<str>>, Option<Rc<str>>, Vec<Stmt>)>, Option<Vec<Stmt>>),
}

pub type PErr = (usize, String);

struct P {
    toks: Vec<Token>,
    i: usize,
}

pub fn parse(toks: Vec<Token>) -> Result<Vec<Stmt>, PErr> {
    let mut p = P { toks, i: 0 };
    let mut out = Vec::new();
    while !p.check(&T::Eof) {
        if p.eat(&T::Newline) {
            continue;
        }
        out.extend(p.statement()?);
    }
    Ok(out)
}

fn syntax(line: usize) -> PErr {
    (line, "SyntaxError: invalid syntax".to_string())
}

const KEYWORDS: &[&str] = &[
    "and", "or", "not", "if", "elif", "else", "while", "for", "in", "def", "return", "break",
    "continue", "pass", "import", "from", "as", "global", "True", "False", "None", "is",
    "lambda", "del", "assert", "raise", "try", "except", "finally", "nonlocal", "class",
];

impl P {
    fn peek(&self) -> &T {
        &self.toks[self.i.min(self.toks.len() - 1)].t
    }

    fn peek_at(&self, k: usize) -> &T {
        &self.toks[(self.i + k).min(self.toks.len() - 1)].t
    }

    fn line(&self) -> usize {
        self.toks[self.i.min(self.toks.len() - 1)].line
    }

    fn check(&self, t: &T) -> bool {
        self.peek() == t
    }

    fn eat(&mut self, t: &T) -> bool {
        if self.check(t) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), T::Op(o) if *o == op)
    }

    fn eat_op(&mut self, op: &str) -> bool {
        if self.is_op(op) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expect_op(&mut self, op: &str) -> Result<(), PErr> {
        if self.eat_op(op) {
            Ok(())
        } else {
            Err(syntax(self.line()))
        }
    }

    fn is_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), T::Name(n) if n == kw)
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.is_kw(kw) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expect_kw(&mut self, kw: &str) -> Result<(), PErr> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            Err(syntax(self.line()))
        }
    }

    fn name(&mut self) -> Result<Rc<str>, PErr> {
        match self.peek().clone() {
            T::Name(n) if !KEYWORDS.contains(&n.as_str()) => {
                self.i += 1;
                Ok(n.into())
            }
            _ => Err(syntax(self.line())),
        }
    }

    fn end_simple(&mut self) -> Result<(), PErr> {
        if self.eat(&T::Newline) || self.check(&T::Eof) || self.check(&T::Dedent) {
            Ok(())
        } else if self.is_op(";") {
            Ok(())
        } else {
            Err(syntax(self.line()))
        }
    }

    fn block(&mut self) -> Result<Vec<Stmt>, PErr> {
        self.expect_op(":")?;
        if !self.eat(&T::Newline) {
            // Single-line suite: `if x: y`
            return self.simple_line();
        }
        if !self.eat(&T::Indent) {
            return Err((self.line(), "IndentationError: expected an indented block".into()));
        }
        let mut body = Vec::new();
        while !self.eat(&T::Dedent) {
            if self.check(&T::Eof) {
                break;
            }
            if self.eat(&T::Newline) {
                continue;
            }
            body.extend(self.statement()?);
        }
        Ok(body)
    }

    fn statement(&mut self) -> Result<Vec<Stmt>, PErr> {
        let line = self.line();
        let kw = match self.peek() {
            T::Name(n) => n.clone(),
            _ => String::new(),
        };
        let kind = match kw.as_str() {
            "if" => {
                self.i += 1;
                let mut arms = Vec::new();
                let cond = self.expr()?;
                let body = self.block()?;
                arms.push((cond, body));
                let mut els = None;
                loop {
                    if self.eat_kw("elif") {
                        let c = self.expr()?;
                        let b = self.block()?;
                        arms.push((c, b));
                    } else if self.eat_kw("else") {
                        els = Some(self.block()?);
                        break;
                    } else {
                        break;
                    }
                }
                SK::If(arms, els)
            }
            "while" => {
                self.i += 1;
                let cond = self.expr()?;
                let body = self.block()?;
                let els = if self.eat_kw("else") { Some(self.block()?) } else { None };
                SK::While(cond, body, els)
            }
            "for" => {
                self.i += 1;
                let target = self.target_list()?;
                self.expect_kw("in")?;
                let iter = self.expr_list()?;
                let body = self.block()?;
                let els = if self.eat_kw("else") { Some(self.block()?) } else { None };
                SK::For(target, iter, body, els)
            }
            "def" => {
                self.i += 1;
                let name = self.name()?;
                self.expect_op("(")?;
                let (params, defaults) = self.params(")")?;
                self.expect_op(")")?;
                if self.eat_op("->") {
                    self.expr()?;
                }
                let body = self.block()?;
                SK::Def(Rc::new(FuncDef { name, params, defaults, body }))
            }
            "try" => {
                self.i += 1;
                let body = self.block()?;
                let mut handlers = Vec::new();
                while self.eat_kw("except") {
                    let mut ty = None;
                    let mut alias = None;
                    if !self.is_op(":") {
                        ty = Some(self.name()?);
                        if self.eat_kw("as") {
                            alias = Some(self.name()?);
                        }
                    }
                    let b = self.block()?;
                    handlers.push((ty, alias, b));
                }
                let fin = if self.eat_kw("finally") { Some(self.block()?) } else { None };
                if handlers.is_empty() && fin.is_none() {
                    return Err(syntax(self.line()));
                }
                SK::Try(body, handlers, fin)
            }
            "class" => {
                return Err((line, "SyntaxError: classes are not supported".into()));
            }
            _ => return self.simple_line(),
        };
        Ok(vec![Stmt { line, kind }])
    }

    fn simple_line(&mut self) -> Result<Vec<Stmt>, PErr> {
        let mut out = vec![self.simple()?];
        while self.eat_op(";") {
            if self.check(&T::Newline) || self.check(&T::Eof) {
                break;
            }
            out.push(self.simple()?);
        }
        self.end_simple()?;
        Ok(out)
    }

    fn params(&mut self, close: &str) -> Result<(Vec<Rc<str>>, Vec<Ex>), PErr> {
        let mut params = Vec::new();
        let mut defaults = Vec::new();
        while !self.is_op(close) {
            params.push(self.name()?);
            if self.eat_op("=") {
                defaults.push(self.expr()?);
            } else if !defaults.is_empty() {
                return Err((self.line(), "SyntaxError: non-default argument follows default argument".into()));
            }
            if !self.eat_op(",") {
                break;
            }
        }
        Ok((params, defaults))
    }

    fn simple(&mut self) -> Result<Stmt, PErr> {
        let line = self.line();
        let kw = match self.peek() {
            T::Name(n) => n.clone(),
            _ => String::new(),
        };
        let kind = match kw.as_str() {
            "pass" => {
                self.i += 1;
                SK::Pass
            }
            "break" => {
                self.i += 1;
                SK::Break
            }
            "continue" => {
                self.i += 1;
                SK::Continue
            }
            "return" => {
                self.i += 1;
                if self.check(&T::Newline) || self.check(&T::Eof) || self.is_op(";") || self.check(&T::Dedent) {
                    SK::Return(None)
                } else {
                    SK::Return(Some(self.expr_list()?))
                }
            }
            "import" => {
                self.i += 1;
                let m = self.name()?;
                let alias = if self.eat_kw("as") { Some(self.name()?) } else { None };
                SK::Import(m, alias)
            }
            "from" => {
                self.i += 1;
                let m = self.name()?;
                self.expect_kw("import")?;
                let mut names = Vec::new();
                if self.eat_op("*") {
                    names.push("*".into());
                } else {
                    loop {
                        names.push(self.name()?);
                        if !self.eat_op(",") {
                            break;
                        }
                    }
                }
                SK::FromImport(m, names)
            }
            "global" | "nonlocal" => {
                self.i += 1;
                let mut names = vec![self.name()?];
                while self.eat_op(",") {
                    names.push(self.name()?);
                }
                SK::Global(names)
            }
            "del" => {
                self.i += 1;
                let first = self.postfix_expr()?;
                let mut ts = vec![self.to_target(first)?];
                while self.eat_op(",") {
                    let e = self.postfix_expr()?;
                    ts.push(self.to_target(e)?);
                }
                SK::Del(ts)
            }
            "assert" => {
                self.i += 1;
                let c = self.expr()?;
                let msg = if self.eat_op(",") { Some(self.expr()?) } else { None };
                SK::Assert(c, msg)
            }
            "raise" => {
                self.i += 1;
                if self.check(&T::Newline) || self.check(&T::Eof) {
                    SK::Raise(None)
                } else {
                    SK::Raise(Some(self.expr()?))
                }
            }
            _ => {
                let first = self.expr_list()?;
                let aug = match self.peek() {
                    T::Op("+=") => Some(BOp::Add),
                    T::Op("-=") => Some(BOp::Sub),
                    T::Op("*=") => Some(BOp::Mul),
                    T::Op("/=") => Some(BOp::Div),
                    T::Op("//=") => Some(BOp::FloorDiv),
                    T::Op("%=") => Some(BOp::Mod),
                    T::Op("**=") => Some(BOp::Pow),
                    _ => None,
                };
                if let Some(op) = aug {
                    self.i += 1;
                    let rhs = self.expr_list()?;
                    SK::AugAssign(self.to_target(first)?, op, rhs)
                } else if self.is_op("=") {
                    let mut targets = vec![self.to_target(first)?];
                    let mut value;
                    loop {
                        self.expect_op("=")?;
                        value = self.expr_list()?;
                        if self.is_op("=") {
                            targets.push(self.to_target(value)?);
                        } else {
                            break;
                        }
                    }
                    SK::Assign(targets, value)
                } else {
                    SK::Expr(first)
                }
            }
        };
        Ok(Stmt { line, kind })
    }

    fn to_target(&self, e: Ex) -> Result<Target, PErr> {
        match e {
            Ex::Name(n) => Ok(Target::Name(n)),
            Ex::Index(a, b) => Ok(Target::Index(*a, *b)),
            Ex::Attr(_, n) => Ok(Target::Attr(n)),
            Ex::Tuple(items) | Ex::List(items) => Ok(Target::Tuple(
                items.into_iter().map(|i| self.to_target(i)).collect::<Result<_, _>>()?,
            )),
            _ => Err((self.line(), "SyntaxError: can't assign to expression".into())),
        }
    }

    fn target_list(&mut self) -> Result<Target, PErr> {
        let mut items = vec![self.postfix_expr()?];
        let mut tuple = false;
        while self.eat_op(",") {
            tuple = true;
            if self.is_kw("in") {
                break;
            }
            items.push(self.postfix_expr()?);
        }
        if tuple {
            self.to_target(Ex::Tuple(items))
        } else {
            self.to_target(items.pop().unwrap())
        }
    }

    /// Comma-separated expressions forming an implicit tuple.
    fn expr_list(&mut self) -> Result<Ex, PErr> {
        let first = self.expr()?;
        if !self.is_op(",") {
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat_op(",") {
            if self.at_expr_end() {
                break;
            }
            items.push(self.expr()?);
        }
        Ok(Ex::Tuple(items))
    }

    fn at_expr_end(&self) -> bool {
        matches!(self.peek(), T::Newline | T::Eof | T::Dedent)
            || matches!(self.peek(), T::Op(o) if matches!(*o, "=" | ")" | "]" | "}" | ":" | ";"))
    }

    pub fn expr(&mut self) -> Result<Ex, PErr> {
        if self.eat_kw("lambda") {
            let (params, defaults) = self.params(":")?;
            self.expect_op(":")?;
            let line = self.line();
            let body = self.expr()?;
            return Ok(Ex::Lambda(Rc::new(FuncDef {
                name: "<lambda>".into(),
                params,
                defaults,
                body: vec![Stmt { line, kind: SK::Return(Some(body)) }],
            })));
        }
        let e = self.or_expr()?;
        if self.is_kw("if") {
            // Only treat as a conditional expression if an `else` follows.
            let save = self.i;
            self.i += 1;
            let cond = self.or_expr()?;
            if self.eat_kw("else") {
                let other = self.expr()?;
                return Ok(Ex::IfExp(Box::new(cond), Box::new(e), Box::new(other)));
            }
            self.i = save;
        }
        Ok(e)
    }

    fn or_expr(&mut self) -> Result<Ex, PErr> {
        let mut l = self.and_expr()?;
        while self.eat_kw("or") {
            let r = self.and_expr()?;
            l = Ex::Or(Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn and_expr(&mut self) -> Result<Ex, PErr> {
        let mut l = self.not_expr()?;
        while self.eat_kw("and") {
            let r = self.not_expr()?;
            l = Ex::And(Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn not_expr(&mut self) -> Result<Ex, PErr> {
        if self.eat_kw("not") {
            let e = self.not_expr()?;
            return Ok(Ex::Not(Box::new(e)));
        }
        self.comparison()
    }

    fn comparison(&mut self) -> Result<Ex, PErr> {
        let first = self.bitor()?;
        let mut rest = Vec::new();
        loop {
            let op = match self.peek() {
                T::Op("==") => COp::Eq,
                T::Op("!=") => COp::Ne,
                T::Op("<") => COp::Lt,
                T::Op("<=") => COp::Le,
                T::Op(">") => COp::Gt,
                T::Op(">=") => COp::Ge,
                T::Name(n) if n == "in" => COp::In,
                T::Name(n) if n == "is" => {
                    if matches!(self.peek_at(1), T::Name(m) if m == "not") {
                        self.i += 1;
                        COp::IsNot
                    } else {
                        COp::Is
                    }
                }
                T::Name(n) if n == "not" && matches!(self.peek_at(1), T::Name(m) if m == "in") => {
                    self.i += 1;
                    COp::NotIn
                }
                _ => break,
            };
            self.i += 1;
            rest.push((op, self.bitor()?));
        }
        if rest.is_empty() {
            Ok(first)
        } else {
            Ok(Ex::Cmp(Box::new(first), rest))
        }
    }

    fn bitor(&mut self) -> Result<Ex, PErr> {
        let mut l = self.bitxor()?;
        while self.eat_op("|") {
            let r = self.bitxor()?;
            l = Ex::Bin(BOp::BitOr, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn bitxor(&mut self) -> Result<Ex, PErr> {
        let mut l = self.bitand()?;
        while self.eat_op("^") {
            let r = self.bitand()?;
            l = Ex::Bin(BOp::BitXor, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn bitand(&mut self) -> Result<Ex, PErr> {
        let mut l = self.arith()?;
        while self.eat_op("&") {
            let r = self.arith()?;
            l = Ex::Bin(BOp::BitAnd, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn arith(&mut self) -> Result<Ex, PErr> {
        let mut l = self.term()?;
        loop {
            let op = if self.eat_op("+") {
                BOp::Add
            } else if self.eat_op("-") {
                BOp::Sub
            } else {
                break;
            };
            let r = self.term()?;
            l = Ex::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn term(&mut self) -> Result<Ex, PErr> {
        let mut l = self.unary()?;
        loop {
            let op = match self.peek() {
                T::Op("*") => BOp::Mul,
                T::Op("/") => BOp::Div,
                T::Op("//") => BOp::FloorDiv,
                T::Op("%") => BOp::Mod,
                _ => break,
            };
            self.i += 1;
            let r = self.unary()?;
            l = Ex::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn unary(&mut self) -> Result<Ex, PErr> {
        if self.eat_op("-") {
            return Ok(Ex::Neg(Box::new(self.unary()?)));
        }
        if self.eat_op("+") {
            return Ok(Ex::Pos(Box::new(self.unary()?)));
        }
        if self.eat_op("~") {
            return Ok(Ex::Invert(Box::new(self.unary()?)));
        }
        self.power()
    }

    fn power(&mut self) -> Result<Ex, PErr> {
        let base = self.postfix_expr()?;
        if self.eat_op("**") {
            let exp = self.unary()?;
            return Ok(Ex::Bin(BOp::Pow, Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }

    fn call_args(&mut self) -> Result<(Vec<Ex>, Vec<(Rc<str>, Ex)>), PErr> {
        let mut args = Vec::new();
        let mut kwargs = Vec::new();
        while !self.is_op(")") {
            if let (T::Name(n), T::Op("=")) = (self.peek().clone(), self.peek_at(1)) {
                self.i += 2;
                kwargs.push((n.into(), self.expr()?));
            } else {
                let e = self.expr()?;
                if self.is_kw("for") {
                    // Generator expression as sole argument: sum(x for x in y)
                    let comp = self.comprehension(e)?;
                    args.push(comp);
                } else {
                    args.push(e);
                }
            }
            if !self.eat_op(",") {
                break;
            }
        }
        self.expect_op(")")?;
        Ok((args, kwargs))
    }

    fn comprehension(&mut self, elt: Ex) -> Result<Ex, PErr> {
        self.expect_kw("for")?;
        let target = self.target_list()?;
        self.expect_kw("in")?;
        let iter = self.or_expr()?;
        let cond = if self.eat_kw("if") { Some(Box::new(self.or_expr()?)) } else { None };
        Ok(Ex::ListComp(Box::new(elt), Box::new(target), Box::new(iter), cond))
    }

    fn postfix_expr(&mut self) -> Result<Ex, PErr> {
        let mut e = self.atom()?;
        loop {
            if self.eat_op("(") {
                let (args, kwargs) = self.call_args()?;
                e = Ex::Call(Box::new(e), args, kwargs);
            } else if self.eat_op("[") {
                let mut parts: Vec<Option<Ex>> = vec![None];
                let mut is_slice = false;
                loop {
                    if self.eat_op(":") {
                        is_slice = true;
                        parts.push(None);
                        continue;
                    }
                    if self.is_op("]") {
                        break;
                    }
                    let x = self.expr_list()?;
                    *parts.last_mut().unwrap() = Some(x);
                    if !self.is_op(":") {
                        break;
                    }
                }
                self.expect_op("]")?;
                if is_slice {
                    if parts.len() > 3 {
                        return Err(syntax(self.line()));
                    }
                    let mut it = parts.into_iter();
                    let a = it.next().flatten().map(Box::new);
                    let b = it.next().flatten().map(Box::new);
                    let c = it.next().flatten().map(Box::new);
                    e = Ex::Slice(Box::new(e), a, b, c);
                } else {
                    let idx = parts.pop().flatten().ok_or_else(|| syntax(self.line()))?;
                    e = Ex::Index(Box::new(e), Box::new(idx));
                }
            } else if self.eat_op(".") {
                let n = self.name()?;
                e = Ex::Attr(Box::new(e), n);
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn atom(&mut self) -> Result<Ex, PErr> {
        let line = self.line();
        let t = self.peek().clone();
        self.i += 1;
        match t {
            T::Int(v) => Ok(Ex::Int(v)),
            T::Float(v) => Ok(Ex::Float(v)),
            T::Str(s) => {
                let mut s = s;
                // Adjacent string literal concatenation.
                while let T::Str(more) = self.peek().clone() {
                    self.i += 1;
                    s.push_str(&more);
                }
                Ok(Ex::Str(s.into()))
            }
            T::FStr(s) => parse_fstring(&s, line),
            T::Name(n) => match n.as_str() {
                "True" => Ok(Ex::Bool(true)),
                "False" => Ok(Ex::Bool(false)),
                "None" => Ok(Ex::None),
                _ if KEYWORDS.contains(&n.as_str()) => Err(syntax(line)),
                _ => Ok(Ex::Name(n.into())),
            },
            T::Op("(") => {
                if self.eat_op(")") {
                    return Ok(Ex::Tuple(vec![]));
                }
                let first = self.expr()?;
                if self.is_kw("for") {
                    let c = self.comprehension(first)?;
                    self.expect_op(")")?;
                    return Ok(c);
                }
                if self.eat_op(")") {
                    return Ok(first);
                }
                let mut items = vec![first];
                while self.eat_op(",") {
                    if self.is_op(")") {
                        break;
                    }
                    items.push(self.expr()?);
                }
                self.expect_op(")")?;
                Ok(Ex::Tuple(items))
            }
            T::Op("[") => {
                if self.eat_op("]") {
                    return Ok(Ex::List(vec![]));
                }
                let first = self.expr()?;
                if self.is_kw("for") {
                    let c = self.comprehension(first)?;
                    self.expect_op("]")?;
                    return Ok(c);
                }
                let mut items = vec![first];
                while self.eat_op(",") {
                    if self.is_op("]") {
                        break;
                    }
                    items.push(self.expr()?);
                }
                self.expect_op("]")?;
                Ok(Ex::List(items))
            }
            T::Op("{") => {
                let mut items = Vec::new();
                while !self.is_op("}") {
                    let k = self.expr()?;
                    self.expect_op(":")?;
                    let v = self.expr()?;
                    items.push((k, v));
                    if !self.eat_op(",") {
                        break;
                    }
                }
                self.expect_op("}")?;
                Ok(Ex::Dict(items))
            }
            _ => Err(syntax(line)),
        }
    }
}

fn parse_fstring(s: &str, line: usize) -> Result<Ex, PErr> {
    let chars: Vec<char> = s.chars().collect();
    let mut parts = Vec::new();
    let mut lit = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '{' && chars.get(i + 1) == Some(&'{') {
            lit.push('{');
            i += 2;
            continue;
        }
        if c == '}' && chars.get(i + 1) == Some(&'}') {
            lit.push('}');
            i += 2;
            continue;
        }
        if c == '{' {
            if !lit.is_empty() {
                parts.push(FPart::Lit(std::mem::take(&mut lit).into()));
            }
            let mut depth = 0;
            let mut j = i + 1;
            let mut colon = None;
            while j < chars.len() {
                match chars[j] {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' => depth -= 1,
                    '}' if depth == 0 => break,
                    '}' => depth -= 1,
                    ':' if depth == 0 && colon.is_none() => colon = Some(j),
                    _ => {}
                }
                j += 1;
            }
            if j >= chars.len() {
                return Err((line, "SyntaxError: f-string: expecting '}'".into()));
            }
            let expr_end = colon.unwrap_or(j);
            let mut src: String = chars[i + 1..expr_end].iter().collect();
            if src.ends_with("!r") || src.ends_with("!s") {
                src.truncate(src.len() - 2);
            }
            let spec = colon.map(|c| chars[c + 1..j].iter().collect::<String>().into());
            let toks = super::lexer::tokenize(&src).map_err(|(_, m)| (line, m))?;
            let mut p = P { toks, i: 0 };
            let e = p.expr().map_err(|(_, m)| (line, m))?;
            parts.push(FPart::Expr(e, spec));
            i = j + 1;
            continue;
        }
        lit.push(c);
        i += 1;
    }
    if !lit.is_empty() {
        parts.push(FPart::Lit(lit.into()));
    }
    Ok(Ex::FStr(parts))
}
