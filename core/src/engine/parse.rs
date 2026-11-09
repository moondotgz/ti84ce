//! Parser for TI-style tokenized expressions, following the TI-84 EOS
//! order of operations (left-associative powers, implied multiplication,
//! optional closing parentheses, and so on).

use super::{is_func, letter_index, list_index, yvar_index, CResult, CalcError, Tok};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Root,
    NPr,
    NCr,
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    And,
    Or,
    Xor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Post {
    Sq,
    Cube,
    Inv,
    Fact,
    Deg,
    Rad,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    Var(usize),
    Ans,
    Rand,
    ListLit(Vec<Expr>),
    ListVar(usize),
    ListElem(usize, Box<Expr>),
    YVar(usize),
    YCall(usize, Box<Expr>),
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    Post(Post, Box<Expr>),
    Call(Tok, Vec<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Var(usize),
    List(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conv {
    Frac,
    Dec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub expr: Expr,
    pub store: Option<Target>,
    pub conv: Option<Conv>,
}

pub type Line = Vec<Stmt>;

#[derive(Debug, Clone, Copy, PartialEq)]
enum L {
    Num(f64),
    T(Tok),
}

struct Parser {
    toks: Vec<(L, usize)>,
    i: usize,
    end: usize,
}

fn lex(src: &[Tok]) -> CResult<Vec<(L, usize)>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < src.len() {
        let t = src[i];
        let is_digit = |t: Tok| t.len() == 1 && t.as_bytes()[0].is_ascii_digit();
        if is_digit(t) || t == "." || t == "ᴇ" {
            let start = i;
            let mut mant = String::new();
            let mut dots = 0;
            while i < src.len() && (is_digit(src[i]) || src[i] == ".") {
                if src[i] == "." {
                    dots += 1;
                }
                mant.push_str(src[i]);
                i += 1;
            }
            if dots > 1 || mant == "." {
                return Err(CalcError::Syntax(start));
            }
            if mant.is_empty() {
                mant.push('1');
            }
            let mut text = mant;
            if i < src.len() && src[i] == "ᴇ" {
                i += 1;
                let mut exp = String::new();
                if i < src.len() && (src[i] == "⁻" || src[i] == "-") {
                    exp.push('-');
                    i += 1;
                }
                let exp_start = i;
                while i < src.len() && is_digit(src[i]) {
                    exp.push_str(src[i]);
                    i += 1;
                }
                if i == exp_start || i - exp_start > 2 {
                    return Err(CalcError::Syntax(i.min(src.len().saturating_sub(1))));
                }
                text.push('e');
                text.push_str(&exp);
            }
            let v: f64 = text.parse().map_err(|_| CalcError::Syntax(start))?;
            out.push((L::Num(v), start));
            continue;
        }
        if t == " " {
            i += 1;
            continue;
        }
        out.push((L::T(t), i));
        i += 1;
    }
    Ok(out)
}

/// Parse a full home-screen line: statements separated by ':', each with an
/// optional `→var` store and an optional `►Frac`/`►Dec` conversion.
pub fn parse_line(src: &[Tok]) -> CResult<Line> {
    let toks = lex(src)?;
    let mut p = Parser { toks, i: 0, end: src.len() };
    let mut stmts = Vec::new();
    loop {
        if p.at_end() {
            break;
        }
        if p.peek_t() == Some(":") {
            p.i += 1;
            continue;
        }
        let expr = p.parse_or()?;
        let mut conv = None;
        let mut store = None;
        if p.eat("►Frac") {
            conv = Some(Conv::Frac);
        } else if p.eat("►Dec") {
            conv = Some(Conv::Dec);
        }
        if p.eat("→") {
            let pos = p.pos();
            match p.next() {
                Some(L::T(t)) if letter_index(t).is_some() => {
                    store = Some(Target::Var(letter_index(t).unwrap()))
                }
                Some(L::T(t)) if list_index(t).is_some() => {
                    store = Some(Target::List(list_index(t).unwrap()))
                }
                _ => return Err(CalcError::Syntax(pos)),
            }
        }
        if !p.at_end() && p.peek_t() != Some(":") {
            return Err(CalcError::Syntax(p.pos()));
        }
        stmts.push(Stmt { expr, store, conv });
    }
    Ok(stmts)
}

/// Parse a single expression (e.g. a Y= function or a window value).
pub fn parse_expr(src: &[Tok]) -> CResult<Expr> {
    let toks = lex(src)?;
    let mut p = Parser { toks, i: 0, end: src.len() };
    if p.at_end() {
        return Err(CalcError::Syntax(0));
    }
    let e = p.parse_or()?;
    if !p.at_end() {
        return Err(CalcError::Syntax(p.pos()));
    }
    Ok(e)
}

impl Parser {
    fn at_end(&self) -> bool {
        self.i >= self.toks.len()
    }

    fn pos(&self) -> usize {
        self.toks.get(self.i).map(|t| t.1).unwrap_or(self.end)
    }

    fn peek(&self) -> Option<L> {
        self.toks.get(self.i).map(|t| t.0)
    }

    fn peek_t(&self) -> Option<Tok> {
        match self.peek() {
            Some(L::T(t)) => Some(t),
            _ => None,
        }
    }

    fn next(&mut self) -> Option<L> {
        let t = self.peek();
        if t.is_some() {
            self.i += 1;
        }
        t
    }

    fn eat(&mut self, t: Tok) -> bool {
        if self.peek_t() == Some(t) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    /// Closing delimiters may be omitted before the end of a statement.
    fn close(&mut self, t: Tok) -> CResult<()> {
        if self.eat(t) {
            return Ok(());
        }
        match self.peek_t() {
            None if self.at_end() => Ok(()),
            Some(":") | Some("→") | Some("►Frac") | Some("►Dec") => Ok(()),
            _ => Err(CalcError::Syntax(self.pos())),
        }
    }

    fn parse_or(&mut self) -> CResult<Expr> {
        let mut lhs = self.parse_and()?;
        loop {
            let op = match self.peek_t() {
                Some(" or ") => Op::Or,
                Some(" xor ") => Op::Xor,
                _ => break,
            };
            self.i += 1;
            let rhs = self.parse_and()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> CResult<Expr> {
        let mut lhs = self.parse_rel()?;
        while self.eat(" and ") {
            let rhs = self.parse_rel()?;
            lhs = Expr::Bin(Op::And, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_rel(&mut self) -> CResult<Expr> {
        let mut lhs = self.parse_sum()?;
        loop {
            let op = match self.peek_t() {
                Some("=") => Op::Eq,
                Some("≠") => Op::Ne,
                Some(">") => Op::Gt,
                Some("≥") => Op::Ge,
                Some("<") => Op::Lt,
                Some("≤") => Op::Le,
                _ => break,
            };
            self.i += 1;
            let rhs = self.parse_sum()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_sum(&mut self) -> CResult<Expr> {
        let mut lhs = self.parse_term()?;
        loop {
            let op = match self.peek_t() {
                Some("+") => Op::Add,
                Some("-") => Op::Sub,
                _ => break,
            };
            self.i += 1;
            let rhs = self.parse_term()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn starts_atom(&self) -> bool {
        match self.peek() {
            Some(L::Num(_)) => true,
            Some(L::T(t)) => {
                letter_index(t).is_some()
                    || list_index(t).is_some()
                    || yvar_index(t).is_some()
                    || is_func(t)
                    || matches!(t, "(" | "{" | "π" | "ℯ" | "Ans" | "rand" | "⁻")
            }
            None => false,
        }
    }

    fn parse_term(&mut self) -> CResult<Expr> {
        let mut lhs = self.parse_perm()?;
        loop {
            let op = match self.peek_t() {
                Some("*") => {
                    self.i += 1;
                    Op::Mul
                }
                Some("/") => {
                    self.i += 1;
                    Op::Div
                }
                _ if self.starts_atom() => Op::Mul,
                _ => break,
            };
            let rhs = self.parse_perm()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_perm(&mut self) -> CResult<Expr> {
        let mut lhs = self.parse_neg()?;
        loop {
            let op = match self.peek_t() {
                Some(" nPr ") => Op::NPr,
                Some(" nCr ") => Op::NCr,
                _ => break,
            };
            self.i += 1;
            let rhs = self.parse_neg()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_neg(&mut self) -> CResult<Expr> {
        if self.eat("⁻") {
            let e = self.parse_neg()?;
            return Ok(Expr::Neg(Box::new(e)));
        }
        self.parse_power()
    }

    fn parse_power(&mut self) -> CResult<Expr> {
        let mut lhs = self.parse_postfix()?;
        loop {
            let op = match self.peek_t() {
                Some("^") => Op::Pow,
                Some("ˣ√") => Op::Root,
                _ => break,
            };
            self.i += 1;
            let rhs = self.parse_power_rhs()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_power_rhs(&mut self) -> CResult<Expr> {
        if self.eat("⁻") {
            let e = self.parse_power_rhs()?;
            return Ok(Expr::Neg(Box::new(e)));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> CResult<Expr> {
        let mut e = self.parse_atom()?;
        loop {
            let p = match self.peek_t() {
                Some("²") => Post::Sq,
                Some("³") => Post::Cube,
                Some("⁻¹") => Post::Inv,
                Some("!") => Post::Fact,
                Some("°") => Post::Deg,
                Some("ʳ") => Post::Rad,
                _ => break,
            };
            self.i += 1;
            e = Expr::Post(p, Box::new(e));
        }
        Ok(e)
    }

    fn parse_args(&mut self) -> CResult<Vec<Expr>> {
        let mut args = Vec::new();
        if self.eat(")") {
            return Ok(args);
        }
        loop {
            args.push(self.parse_or()?);
            if !self.eat(",") {
                break;
            }
        }
        self.close(")")?;
        Ok(args)
    }

    fn parse_atom(&mut self) -> CResult<Expr> {
        let pos = self.pos();
        let t = match self.next() {
            Some(L::Num(v)) => return Ok(Expr::Num(v)),
            Some(L::T(t)) => t,
            None => return Err(CalcError::Syntax(pos)),
        };
        if let Some(i) = letter_index(t) {
            return Ok(Expr::Var(i));
        }
        if let Some(i) = list_index(t) {
            if self.eat("(") {
                let idx = self.parse_or()?;
                self.close(")")?;
                return Ok(Expr::ListElem(i, Box::new(idx)));
            }
            return Ok(Expr::ListVar(i));
        }
        if let Some(i) = yvar_index(t) {
            if self.eat("(") {
                let arg = self.parse_or()?;
                self.close(")")?;
                return Ok(Expr::YCall(i, Box::new(arg)));
            }
            return Ok(Expr::YVar(i));
        }
        if is_func(t) {
            let args = self.parse_args()?;
            return Ok(Expr::Call(t, args));
        }
        match t {
            "(" => {
                let e = self.parse_or()?;
                self.close(")")?;
                Ok(e)
            }
            "{" => {
                let mut items = Vec::new();
                if !self.eat("}") {
                    loop {
                        items.push(self.parse_or()?);
                        if !self.eat(",") {
                            break;
                        }
                    }
                    self.close("}")?;
                }
                Ok(Expr::ListLit(items))
            }
            "π" => Ok(Expr::Num(std::f64::consts::PI)),
            "ℯ" => Ok(Expr::Num(std::f64::consts::E)),
            "Ans" => Ok(Expr::Ans),
            "rand" => Ok(Expr::Rand),
            _ => Err(CalcError::Syntax(pos)),
        }
    }
}
