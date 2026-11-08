//! Expression evaluator over real numbers and lists.

use super::parse::{Expr, Line, Op, Post, Target};
use super::{CResult, CalcError, Tok};
use std::f64::consts::PI;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Num(f64),
    List(Vec<f64>),
}

impl Value {
    pub fn num(&self) -> CResult<f64> {
        match self {
            Value::Num(x) => Ok(*x),
            Value::List(_) => Err(CalcError::DataType),
        }
    }
}

pub const X_VAR: usize = 23;

/// Variable storage and evaluation state.
pub struct Env {
    pub vars: [f64; 27],
    pub ans: Value,
    pub lists: [Vec<f64>; 6],
    pub degree: bool,
    pub yfuncs: [Option<Expr>; 10],
    depth: u32,
    rng: u64,
}

impl Default for Env {
    fn default() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15)
            | 1;
        Env {
            vars: [0.0; 27],
            ans: Value::Num(0.0),
            lists: Default::default(),
            degree: false,
            yfuncs: Default::default(),
            depth: 0,
            rng: seed,
        }
    }
}

fn check(x: f64) -> CResult<f64> {
    if x.is_nan() {
        Err(CalcError::Domain)
    } else if !x.is_finite() || x.abs() >= 1e100 {
        Err(CalcError::Overflow)
    } else if x.abs() < 1e-99 {
        Ok(0.0)
    } else {
        Ok(x)
    }
}

/// Snap tiny floating-point noise (e.g. sin(π)) to exact values, the way the
/// calculator's 14-digit BCD arithmetic would produce them.
fn snap(x: f64) -> f64 {
    if x.abs() < 1e-14 {
        0.0
    } else if (x.abs() - 1.0).abs() < 1e-15 {
        x.signum()
    } else {
        x
    }
}

/// Round to 14 significant digits like the calculator's internal precision.
pub fn clean(x: f64) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let s = format!("{:.13e}", x);
    s.parse().unwrap_or(x)
}

fn is_int(x: f64) -> bool {
    x.fract() == 0.0
}

fn truthy(x: f64) -> f64 {
    if x != 0.0 {
        1.0
    } else {
        0.0
    }
}

fn map1(v: Value, f: impl Fn(f64) -> CResult<f64>) -> CResult<Value> {
    match v {
        Value::Num(x) => Ok(Value::Num(check(f(x)?)?)),
        Value::List(xs) => Ok(Value::List(
            xs.into_iter()
                .map(|x| f(x).and_then(check))
                .collect::<CResult<_>>()?,
        )),
    }
}

fn map2(a: Value, b: Value, f: impl Fn(f64, f64) -> CResult<f64>) -> CResult<Value> {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => Ok(Value::Num(check(f(x, y)?)?)),
        (Value::List(xs), Value::Num(y)) => Ok(Value::List(
            xs.into_iter()
                .map(|x| f(x, y).and_then(check))
                .collect::<CResult<_>>()?,
        )),
        (Value::Num(x), Value::List(ys)) => Ok(Value::List(
            ys.into_iter()
                .map(|y| f(x, y).and_then(check))
                .collect::<CResult<_>>()?,
        )),
        (Value::List(xs), Value::List(ys)) => {
            if xs.len() != ys.len() {
                return Err(CalcError::DimMismatch);
            }
            Ok(Value::List(
                xs.into_iter()
                    .zip(ys)
                    .map(|(x, y)| f(x, y).and_then(check))
                    .collect::<CResult<_>>()?,
            ))
        }
    }
}

pub fn factorial(x: f64) -> CResult<f64> {
    if x < 0.0 || !is_int(x) {
        return Err(CalcError::Domain);
    }
    if x > 69.0 {
        return Err(CalcError::Overflow);
    }
    Ok((1..=x as u64).fold(1.0, |acc, k| acc * k as f64))
}

fn npr(n: f64, r: f64) -> CResult<f64> {
    if !is_int(n) || !is_int(r) || n < 0.0 || r < 0.0 {
        return Err(CalcError::Domain);
    }
    if r > n {
        return Ok(0.0);
    }
    let mut acc = 1.0;
    let mut k = n;
    while k > n - r {
        acc *= k;
        k -= 1.0;
    }
    Ok(acc)
}

pub fn ncr(n: f64, r: f64) -> CResult<f64> {
    if !is_int(n) || !is_int(r) || n < 0.0 || r < 0.0 {
        return Err(CalcError::Domain);
    }
    if r > n {
        return Ok(0.0);
    }
    let r = r.min(n - r);
    let mut acc = 1.0;
    for i in 0..r as u64 {
        acc = acc * (n - i as f64) / (i as f64 + 1.0);
    }
    Ok(acc.round())
}

fn gcd(a: f64, b: f64) -> CResult<f64> {
    if !is_int(a) || !is_int(b) || a < 0.0 || b < 0.0 {
        return Err(CalcError::Domain);
    }
    let (mut a, mut b) = (a as u64, b as u64);
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    Ok(a as f64)
}

/// Standard normal CDF (W. J. Cody / Hart double-precision approximation).
pub fn norm_cdf(x: f64) -> f64 {
    let xabs = x.abs();
    let c = if xabs > 37.0 {
        0.0
    } else {
        let e = (-xabs * xabs / 2.0).exp();
        if xabs < 7.07106781186547 {
            let mut b = 3.52624965998911e-02 * xabs + 0.700383064443688;
            b = b * xabs + 6.37396220353165;
            b = b * xabs + 33.912866078383;
            b = b * xabs + 112.079291497871;
            b = b * xabs + 221.213596169931;
            b = b * xabs + 220.206867912376;
            let c = e * b;
            let mut b = 8.83883476483184e-02 * xabs + 1.75566716318264;
            b = b * xabs + 16.064177579207;
            b = b * xabs + 86.7807322029461;
            b = b * xabs + 296.564248779674;
            b = b * xabs + 637.333633378831;
            b = b * xabs + 793.826512519948;
            b = b * xabs + 440.413735824752;
            c / b
        } else {
            let mut b = xabs + 0.65;
            b = xabs + 4.0 / b;
            b = xabs + 3.0 / b;
            b = xabs + 2.0 / b;
            b = xabs + 1.0 / b;
            e / b / 2.506628274631
        }
    };
    if x > 0.0 {
        1.0 - c
    } else {
        c
    }
}

/// Inverse standard normal CDF (Acklam's algorithm with one Halley refinement).
pub fn inv_norm(p: f64) -> CResult<f64> {
    if p <= 0.0 || p >= 1.0 {
        return Err(CalcError::Domain);
    }
    const A: [f64; 6] = [
        -3.969683028665376e+01, 2.209460984245205e+02, -2.759285104469687e+02,
        1.383577518672690e+02, -3.066479806614716e+01, 2.506628277459239e+00,
    ];
    const B: [f64; 5] = [
        -5.447609879822406e+01, 1.615858368580409e+02, -1.556989798598866e+02,
        6.680131188771972e+01, -1.328068155288572e+01,
    ];
    const C: [f64; 6] = [
        -7.784894002430293e-03, -3.223964580411365e-01, -2.400758277161838e+00,
        -2.549732539343734e+00, 4.374664141464968e+00, 2.938163982698783e+00,
    ];
    const D: [f64; 4] = [
        7.784695709041462e-03, 3.224671290700398e-01, 2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    let plow = 0.02425;
    let x = if p < plow {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= 1.0 - plow {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    let e = norm_cdf(x) - p;
    let u = e * (2.0 * PI).sqrt() * (x * x / 2.0).exp();
    Ok(x - u / (1.0 + x * u / 2.0))
}

fn list_arg(v: Value) -> CResult<Vec<f64>> {
    match v {
        Value::List(xs) => Ok(xs),
        Value::Num(_) => Err(CalcError::DataType),
    }
}

fn mean(xs: &[f64]) -> CResult<f64> {
    if xs.is_empty() {
        return Err(CalcError::InvalidDim);
    }
    Ok(xs.iter().sum::<f64>() / xs.len() as f64)
}

fn variance(xs: &[f64]) -> CResult<f64> {
    if xs.len() < 2 {
        return Err(CalcError::InvalidDim);
    }
    let m = mean(xs)?;
    Ok(xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() - 1) as f64)
}

fn pow(x: f64, y: f64) -> CResult<f64> {
    if x == 0.0 && y == 0.0 {
        return Err(CalcError::Domain);
    }
    if x == 0.0 && y < 0.0 {
        return Err(CalcError::DivideBy0);
    }
    if x < 0.0 && !is_int(y) {
        // Allow odd roots expressed as reciprocals of odd integers.
        let inv = 1.0 / y;
        if (inv - inv.round()).abs() < 1e-12 && inv.round() as i64 % 2 != 0 {
            return Ok(-(-x).powf(y));
        }
        return Err(CalcError::NonReal);
    }
    if is_int(y) && y.abs() < 1e9 {
        return Ok(x.powi(y as i32));
    }
    Ok(x.powf(y))
}

fn root(n: f64, x: f64) -> CResult<f64> {
    if n == 0.0 {
        return Err(CalcError::Domain);
    }
    if x < 0.0 {
        if is_int(n) && (n as i64) % 2 != 0 {
            return Ok(-(-x).powf(1.0 / n));
        }
        return Err(CalcError::NonReal);
    }
    Ok(x.powf(1.0 / n))
}

impl Env {
    fn next_rand(&mut self) -> f64 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        let v = self.rng.wrapping_mul(0x2545F4914F6CDD1D);
        (v >> 11) as f64 / (1u64 << 53) as f64
    }


    /// Execute a parsed home-screen line. Returns the value of the last
    /// statement and whether it requested fraction display.
    pub fn run_line(&mut self, line: &Line) -> CResult<(Value, bool)> {
        let mut last = (self.ans.clone(), false);
        for stmt in line {
            self.depth = 0;
            let mut v = self.eval(&stmt.expr)?;
            if let Value::Num(x) = v {
                v = Value::Num(clean(x));
            } else if let Value::List(xs) = v {
                v = Value::List(xs.into_iter().map(clean).collect());
            }
            match stmt.store {
                Some(Target::Var(i)) => self.vars[i] = v.num()?,
                Some(Target::List(i)) => match &v {
                    Value::List(xs) => self.lists[i] = xs.clone(),
                    Value::Num(_) => return Err(CalcError::DataType),
                },
                None => {}
            }
            self.ans = v.clone();
            last = (v, stmt.conv == Some(super::parse::Conv::Frac));
        }
        Ok(last)
    }

    pub fn eval_num(&mut self, e: &Expr) -> CResult<f64> {
        self.eval(e)?.num()
    }

    /// Evaluate a Y= function at a given X, restoring X afterwards.
    pub fn eval_y(&mut self, idx: usize, x: f64) -> CResult<f64> {
        let saved = self.vars[X_VAR];
        self.vars[X_VAR] = x;
        let r = match self.yfuncs[idx].clone() {
            Some(e) => {
                self.depth = 0;
                self.eval_num(&e)
            }
            None => Err(CalcError::Undefined),
        };
        self.vars[X_VAR] = saved;
        r
    }

    /// Evaluate an expression with a letter variable temporarily bound.
    pub fn eval_with(&mut self, e: &Expr, var: usize, x: f64) -> CResult<f64> {
        let saved = self.vars[var];
        self.vars[var] = x;
        let r = self.eval_num(e);
        self.vars[var] = saved;
        r
    }

    pub fn eval(&mut self, e: &Expr) -> CResult<Value> {
        match e {
            Expr::Num(x) => Ok(Value::Num(*x)),
            Expr::Var(i) => Ok(Value::Num(self.vars[*i])),
            Expr::Ans => Ok(self.ans.clone()),
            Expr::Rand => Ok(Value::Num(self.next_rand())),
            Expr::ListLit(items) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    out.push(self.eval_num(it)?);
                }
                Ok(Value::List(out))
            }
            Expr::ListVar(i) => {
                if self.lists[*i].is_empty() {
                    Err(CalcError::Undefined)
                } else {
                    Ok(Value::List(self.lists[*i].clone()))
                }
            }
            Expr::ListElem(i, idx) => {
                let k = self.eval_num(idx)?;
                let list = &self.lists[*i];
                if !is_int(k) || k < 1.0 || k as usize > list.len() {
                    return Err(CalcError::InvalidDim);
                }
                Ok(Value::Num(list[k as usize - 1]))
            }
            Expr::YVar(i) => {
                let x = self.vars[X_VAR];
                self.call_y(*i, Value::Num(x))
            }
            Expr::YCall(i, arg) => {
                let a = self.eval(arg)?;
                self.call_y(*i, a)
            }
            Expr::Neg(a) => {
                let v = self.eval(a)?;
                map1(v, |x| Ok(-x))
            }
            Expr::Post(p, a) => {
                let v = self.eval(a)?;
                let degree = self.degree;
                match p {
                    Post::Sq => map1(v, |x| Ok(x * x)),
                    Post::Cube => map1(v, |x| Ok(x * x * x)),
                    Post::Inv => map1(v, |x| {
                        if x == 0.0 {
                            Err(CalcError::DivideBy0)
                        } else {
                            Ok(1.0 / x)
                        }
                    }),
                    Post::Fact => map1(v, factorial),
                    Post::Deg => map1(v, |x| Ok(if degree { x } else { x * PI / 180.0 })),
                    Post::Rad => map1(v, |x| Ok(if degree { x * 180.0 / PI } else { x })),
                }
            }
            Expr::Bin(op, a, b) => {
                let va = self.eval(a)?;
                let vb = self.eval(b)?;
                match op {
                    Op::Add => map2(va, vb, |x, y| Ok(x + y)),
                    Op::Sub => map2(va, vb, |x, y| Ok(x - y)),
                    Op::Mul => map2(va, vb, |x, y| Ok(x * y)),
                    Op::Div => map2(va, vb, |x, y| {
                        if y == 0.0 {
                            Err(CalcError::DivideBy0)
                        } else {
                            Ok(x / y)
                        }
                    }),
                    Op::Pow => map2(va, vb, pow),
                    Op::Root => map2(va, vb, root),
                    Op::NPr => map2(va, vb, npr),
                    Op::NCr => map2(va, vb, ncr),
                    Op::Eq => map2(va, vb, |x, y| Ok((x == y) as u8 as f64)),
                    Op::Ne => map2(va, vb, |x, y| Ok((x != y) as u8 as f64)),
                    Op::Gt => map2(va, vb, |x, y| Ok((x > y) as u8 as f64)),
                    Op::Ge => map2(va, vb, |x, y| Ok((x >= y) as u8 as f64)),
                    Op::Lt => map2(va, vb, |x, y| Ok((x < y) as u8 as f64)),
                    Op::Le => map2(va, vb, |x, y| Ok((x <= y) as u8 as f64)),
                    Op::And => map2(va, vb, |x, y| Ok(truthy(x) * truthy(y))),
                    Op::Or => map2(va, vb, |x, y| Ok(truthy(truthy(x) + truthy(y)))),
                    Op::Xor => map2(va, vb, |x, y| Ok((truthy(x) != truthy(y)) as u8 as f64)),
                }
            }
            Expr::Call(name, args) => self.call(name, args),
        }
    }

    fn call_y(&mut self, i: usize, arg: Value) -> CResult<Value> {
        self.depth += 1;
        if self.depth > 24 {
            return Err(CalcError::Undefined);
        }
        let f = self.yfuncs[i].clone().ok_or(CalcError::Undefined)?;
        let saved = self.vars[X_VAR];
        let r = match arg {
            Value::Num(x) => {
                self.vars[X_VAR] = x;
                self.eval(&f)
            }
            Value::List(xs) => {
                let mut out = Vec::new();
                let mut err = None;
                for x in xs {
                    self.vars[X_VAR] = x;
                    match self.eval_num(&f) {
                        Ok(v) => out.push(v),
                        Err(e) => {
                            err = Some(e);
                            break;
                        }
                    }
                }
                match err {
                    Some(e) => Err(e),
                    None => Ok(Value::List(out)),
                }
            }
        };
        self.vars[X_VAR] = saved;
        self.depth -= 1;
        r
    }

    fn var_arg(e: &Expr) -> CResult<usize> {
        match e {
            Expr::Var(i) => Ok(*i),
            _ => Err(CalcError::Syntax(0)),
        }
    }

    fn integrate(&mut self, f: &Expr, var: usize, a: f64, b: f64) -> CResult<f64> {
        // Composite Gauss–Legendre (5-point) over subintervals.
        const XS: [f64; 5] = [
            0.0, -0.5384693101056831, 0.5384693101056831, -0.906179845938664,
            0.906179845938664,
        ];
        const WS: [f64; 5] = [
            0.5688888888888889, 0.47862867049936647, 0.47862867049936647,
            0.23692688505618908, 0.23692688505618908,
        ];
        let n = 64;
        let h = (b - a) / n as f64;
        let mut total = 0.0;
        let mut abs_total = 0.0;
        for k in 0..n {
            let lo = a + k as f64 * h;
            let mid = lo + h / 2.0;
            for j in 0..5 {
                let y = self.eval_with(f, var, mid + XS[j] * h / 2.0)?;
                total += WS[j] * y;
                abs_total += WS[j] * y.abs();
            }
        }
        // Cancellation noise (e.g. odd functions on symmetric bounds) is zero.
        if total.abs() <= 1e-11 * abs_total {
            return Ok(0.0);
        }
        Ok(total * h / 2.0)
    }

    fn call(&mut self, name: Tok, args: &[Expr]) -> CResult<Value> {
        let argc = |lo: usize, hi: usize| -> CResult<()> {
            if args.len() < lo || args.len() > hi {
                Err(CalcError::Argument)
            } else {
                Ok(())
            }
        };
        // Functions with unevaluated (symbolic) arguments.
        match name {
            "seq(" => {
                argc(4, 5)?;
                let var = Self::var_arg(&args[1])?;
                let start = self.eval_num(&args[2])?;
                let end = self.eval_num(&args[3])?;
                let step = if args.len() == 5 { self.eval_num(&args[4])? } else { 1.0 };
                if step == 0.0 {
                    return Err(CalcError::Domain);
                }
                let count = ((end - start) / step).floor() + 1.0;
                if count > 999.0 {
                    return Err(CalcError::InvalidDim);
                }
                let mut out = Vec::new();
                for k in 0..count.max(0.0) as usize {
                    let x = start + k as f64 * step;
                    out.push(check(self.eval_with(&args[0], var, x)?)?);
                }
                return Ok(Value::List(out));
            }
            "nDeriv(" => {
                argc(3, 4)?;
                let var = Self::var_arg(&args[1])?;
                let x = self.eval_num(&args[2])?;
                let h = if args.len() == 4 { self.eval_num(&args[3])? } else { 1e-3 };
                let fp = self.eval_with(&args[0], var, x + h)?;
                let fm = self.eval_with(&args[0], var, x - h)?;
                return Ok(Value::Num(check((fp - fm) / (2.0 * h))?));
            }
            "fnInt(" => {
                argc(4, 5)?;
                let var = Self::var_arg(&args[1])?;
                let a = self.eval_num(&args[2])?;
                let b = self.eval_num(&args[3])?;
                let r = self.integrate(&args[0], var, a, b)?;
                return Ok(Value::Num(check(r)?));
            }
            _ => {}
        }

        let mut vals = Vec::with_capacity(args.len());
        for a in args {
            vals.push(self.eval(a)?);
        }
        let degree = self.degree;
        let to_rad = move |x: f64| if degree { x * PI / 180.0 } else { x };
        let from_rad = move |x: f64| if degree { x * 180.0 / PI } else { x };
        let one = |vals: &mut Vec<Value>| -> CResult<Value> {
            if vals.len() != 1 {
                return Err(CalcError::Argument);
            }
            Ok(vals.pop().unwrap())
        };

        match name {
            "sin(" => map1(one(&mut vals)?, |x| Ok(snap(to_rad(x).sin()))),
            "cos(" => map1(one(&mut vals)?, |x| Ok(snap(to_rad(x).cos()))),
            "tan(" => map1(one(&mut vals)?, |x| {
                let r = to_rad(x);
                let (s, c) = (snap(r.sin()), snap(r.cos()));
                if c == 0.0 {
                    Err(CalcError::Domain)
                } else {
                    Ok(s / c)
                }
            }),
            "sin⁻¹(" => map1(one(&mut vals)?, |x| {
                if x.abs() > 1.0 {
                    Err(CalcError::Domain)
                } else {
                    Ok(from_rad(x.asin()))
                }
            }),
            "cos⁻¹(" => map1(one(&mut vals)?, |x| {
                if x.abs() > 1.0 {
                    Err(CalcError::Domain)
                } else {
                    Ok(from_rad(x.acos()))
                }
            }),
            "tan⁻¹(" => map1(one(&mut vals)?, |x| Ok(from_rad(x.atan()))),
            "sinh(" => map1(one(&mut vals)?, |x| Ok(x.sinh())),
            "cosh(" => map1(one(&mut vals)?, |x| Ok(x.cosh())),
            "tanh(" => map1(one(&mut vals)?, |x| Ok(x.tanh())),
            "sinh⁻¹(" => map1(one(&mut vals)?, |x| Ok(x.asinh())),
            "cosh⁻¹(" => map1(one(&mut vals)?, |x| {
                if x < 1.0 {
                    Err(CalcError::Domain)
                } else {
                    Ok(x.acosh())
                }
            }),
            "tanh⁻¹(" => map1(one(&mut vals)?, |x| {
                if x.abs() >= 1.0 {
                    Err(CalcError::Domain)
                } else {
                    Ok(x.atanh())
                }
            }),
            "log(" => map1(one(&mut vals)?, |x| {
                if x == 0.0 {
                    Err(CalcError::Domain)
                } else if x < 0.0 {
                    Err(CalcError::NonReal)
                } else {
                    Ok(x.log10())
                }
            }),
            "ln(" => map1(one(&mut vals)?, |x| {
                if x == 0.0 {
                    Err(CalcError::Domain)
                } else if x < 0.0 {
                    Err(CalcError::NonReal)
                } else {
                    Ok(x.ln())
                }
            }),
            "logBASE(" => {
                argc(2, 2)?;
                let b = vals.pop().unwrap();
                let x = vals.pop().unwrap();
                map2(x, b, |x, b| {
                    if x <= 0.0 || b <= 0.0 || b == 1.0 {
                        Err(CalcError::Domain)
                    } else {
                        Ok(x.ln() / b.ln())
                    }
                })
            }
            "10^(" => map1(one(&mut vals)?, |x| Ok(10f64.powf(x))),
            "ℯ^(" => map1(one(&mut vals)?, |x| Ok(x.exp())),
            "√(" => map1(one(&mut vals)?, |x| {
                if x < 0.0 {
                    Err(CalcError::NonReal)
                } else {
                    Ok(x.sqrt())
                }
            }),
            "³√(" => map1(one(&mut vals)?, |x| Ok(x.cbrt())),
            "abs(" => map1(one(&mut vals)?, |x| Ok(x.abs())),
            "not(" => map1(one(&mut vals)?, |x| Ok((x == 0.0) as u8 as f64)),
            "iPart(" => map1(one(&mut vals)?, |x| Ok(x.trunc())),
            "fPart(" => map1(one(&mut vals)?, |x| Ok(clean(x - x.trunc()))),
            "int(" | "floor(" => map1(one(&mut vals)?, |x| Ok(clean(x).floor())),
            "ceiling(" => map1(one(&mut vals)?, |x| Ok(clean(x).ceil())),
            "round(" => {
                argc(1, 2)?;
                let n = if vals.len() == 2 { vals.pop().unwrap().num()? } else { 9.0 };
                if !is_int(n) || !(0.0..=9.0).contains(&n) {
                    return Err(CalcError::Domain);
                }
                let m = 10f64.powi(n as i32);
                map1(vals.pop().unwrap(), |x| Ok((x * m).round() / m))
            }
            "min(" | "max(" => {
                let is_min = name == "min(";
                let pick = move |a: f64, b: f64| if is_min { a.min(b) } else { a.max(b) };
                match vals.len() {
                    1 => {
                        let xs = list_arg(vals.pop().unwrap())?;
                        xs.into_iter()
                            .reduce(pick)
                            .map(Value::Num)
                            .ok_or(CalcError::InvalidDim)
                    }
                    2 => {
                        let b = vals.pop().unwrap();
                        let a = vals.pop().unwrap();
                        map2(a, b, |x, y| Ok(pick(x, y)))
                    }
                    _ => Err(CalcError::Argument),
                }
            }
            "gcd(" | "lcm(" => {
                argc(2, 2)?;
                let b = vals.pop().unwrap();
                let a = vals.pop().unwrap();
                if name == "gcd(" {
                    map2(a, b, gcd)
                } else {
                    map2(a, b, |x, y| {
                        let g = gcd(x, y)?;
                        Ok(if g == 0.0 { 0.0 } else { x / g * y })
                    })
                }
            }
            "remainder(" => {
                argc(2, 2)?;
                let b = vals.pop().unwrap();
                let a = vals.pop().unwrap();
                map2(a, b, |x, y| {
                    if y == 0.0 {
                        Err(CalcError::DivideBy0)
                    } else {
                        Ok(x - y * (x / y).trunc())
                    }
                })
            }
            "randInt(" => {
                argc(2, 3)?;
                let n = if vals.len() == 3 { Some(vals.pop().unwrap().num()?) } else { None };
                let hi = vals.pop().unwrap().num()?;
                let lo = vals.pop().unwrap().num()?;
                if !is_int(lo) || !is_int(hi) {
                    return Err(CalcError::Domain);
                }
                let (lo, hi) = (lo.min(hi), lo.max(hi));
                let draw = |env: &mut Env| lo + (env.next_rand() * (hi - lo + 1.0)).floor();
                match n {
                    None => Ok(Value::Num(draw(self))),
                    Some(n) if is_int(n) && n >= 1.0 && n <= 999.0 => {
                        Ok(Value::List((0..n as usize).map(|_| draw(self)).collect()))
                    }
                    Some(_) => Err(CalcError::Domain),
                }
            }
            "sum(" | "prod(" | "mean(" | "median(" | "dim(" | "stdDev(" | "variance(" => {
                let xs = list_arg(one(&mut vals)?)?;
                let r = match name {
                    "sum(" => xs.iter().sum(),
                    "prod(" => xs.iter().product(),
                    "mean(" => mean(&xs)?,
                    "median(" => {
                        if xs.is_empty() {
                            return Err(CalcError::InvalidDim);
                        }
                        let mut s = xs.clone();
                        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
                        let n = s.len();
                        if n % 2 == 1 {
                            s[n / 2]
                        } else {
                            (s[n / 2 - 1] + s[n / 2]) / 2.0
                        }
                    }
                    "dim(" => xs.len() as f64,
                    "stdDev(" => variance(&xs)?.sqrt(),
                    _ => variance(&xs)?,
                };
                Ok(Value::Num(check(r)?))
            }
            "cumSum(" | "SortA(" | "SortD(" | "ΔList(" => {
                let mut xs = list_arg(one(&mut vals)?)?;
                match name {
                    "cumSum(" => {
                        let mut acc = 0.0;
                        for x in xs.iter_mut() {
                            acc += *x;
                            *x = acc;
                        }
                    }
                    "SortA(" => xs.sort_by(|a, b| a.partial_cmp(b).unwrap()),
                    "SortD(" => xs.sort_by(|a, b| b.partial_cmp(a).unwrap()),
                    _ => {
                        if xs.len() < 2 {
                            return Err(CalcError::InvalidDim);
                        }
                        xs = xs.windows(2).map(|w| w[1] - w[0]).collect();
                    }
                }
                Ok(Value::List(xs))
            }
            "augment(" => {
                argc(2, 2)?;
                let b = list_arg(vals.pop().unwrap())?;
                let mut a = list_arg(vals.pop().unwrap())?;
                a.extend(b);
                Ok(Value::List(a))
            }
            "normalpdf(" => {
                let (mu, sigma) = mu_sigma(&mut vals, 1)?;
                map1(vals.pop().unwrap(), |x| {
                    let z = (x - mu) / sigma;
                    Ok((-z * z / 2.0).exp() / (sigma * (2.0 * PI).sqrt()))
                })
            }
            "normalcdf(" => {
                let (mu, sigma) = mu_sigma(&mut vals, 2)?;
                let hi = vals.pop().unwrap().num()?;
                let lo = vals.pop().unwrap().num()?;
                let r = norm_cdf((hi - mu) / sigma) - norm_cdf((lo - mu) / sigma);
                Ok(Value::Num(check(r.max(0.0))?))
            }
            "invNorm(" => {
                let (mu, sigma) = mu_sigma(&mut vals, 1)?;
                let p = vals.pop().unwrap().num()?;
                Ok(Value::Num(check(mu + sigma * inv_norm(p)?)?))
            }
            "binompdf(" | "binomcdf(" => {
                argc(2, 3)?;
                let x = if vals.len() == 3 { Some(vals.pop().unwrap()) } else { None };
                let p = vals.pop().unwrap().num()?;
                let n = vals.pop().unwrap().num()?;
                if !is_int(n) || n < 0.0 || !(0.0..=1.0).contains(&p) {
                    return Err(CalcError::Domain);
                }
                let pdf = |k: f64| -> CResult<f64> {
                    if !is_int(k) || k < 0.0 || k > n {
                        return Ok(0.0);
                    }
                    Ok(ncr(n, k)? * p.powf(k) * (1.0 - p).powf(n - k))
                };
                let cdf = |k: f64| -> CResult<f64> {
                    let mut s = 0.0;
                    let mut i = 0.0;
                    while i <= k.floor().min(n) {
                        s += pdf(i)?;
                        i += 1.0;
                    }
                    Ok(s.min(1.0))
                };
                let is_pdf = name == "binompdf(";
                match x {
                    Some(v) => map1(v, |k| if is_pdf { pdf(k) } else { cdf(k) }),
                    None => {
                        let mut out = Vec::new();
                        for k in 0..=n as usize {
                            out.push(if is_pdf { pdf(k as f64)? } else { cdf(k as f64)? });
                        }
                        Ok(Value::List(out))
                    }
                }
            }
            "poissonpdf(" | "poissoncdf(" => {
                argc(2, 2)?;
                let x = vals.pop().unwrap();
                let mu = vals.pop().unwrap().num()?;
                if mu <= 0.0 {
                    return Err(CalcError::Domain);
                }
                let pdf = move |k: f64| -> f64 {
                    if k < 0.0 || !is_int(k) {
                        return 0.0;
                    }
                    let mut lnf = 0.0;
                    for i in 1..=k as u64 {
                        lnf += (i as f64).ln();
                    }
                    (k * mu.ln() - mu - lnf).exp()
                };
                if name == "poissonpdf(" {
                    map1(x, |k| Ok(pdf(k)))
                } else {
                    map1(x, |k| Ok((0..=k.floor().max(-1.0) as i64).map(|i| pdf(i as f64)).sum::<f64>().min(1.0)))
                }
            }
            "geometpdf(" | "geometcdf(" => {
                argc(2, 2)?;
                let x = vals.pop().unwrap();
                let p = vals.pop().unwrap().num()?;
                if p <= 0.0 || p > 1.0 {
                    return Err(CalcError::Domain);
                }
                if name == "geometpdf(" {
                    map1(x, |k| Ok(if k >= 1.0 && is_int(k) { (1.0 - p).powf(k - 1.0) * p } else { 0.0 }))
                } else {
                    map1(x, |k| Ok(if k >= 1.0 { 1.0 - (1.0 - p).powf(k.floor()) } else { 0.0 }))
                }
            }
            _ => Err(CalcError::Syntax(0)),
        }
    }
}

fn mu_sigma(vals: &mut Vec<Value>, base: usize) -> CResult<(f64, f64)> {
    // Optional trailing args are μ and σ (defaults 0 and 1).
    let (mu, sigma) = if vals.len() == base + 2 {
        let s = vals.pop().unwrap().num()?;
        let m = vals.pop().unwrap().num()?;
        (m, s)
    } else if vals.len() == base {
        (0.0, 1.0)
    } else {
        return Err(CalcError::Argument);
    };
    if sigma <= 0.0 {
        return Err(CalcError::Domain);
    }
    Ok((mu, sigma))
}
