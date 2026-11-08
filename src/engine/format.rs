//! TI-style number formatting: 10 significant digits, leading zeros dropped,
//! `⁻` for negatives and `ᴇ` for exponents.

use super::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notation {
    Normal,
    Sci,
    Eng,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumFormat {
    pub notation: Notation,
    /// `None` = FLOAT, `Some(n)` = FIX n.
    pub fix: Option<u8>,
}

impl Default for NumFormat {
    fn default() -> Self {
        NumFormat { notation: Notation::Normal, fix: None }
    }
}

fn trim_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

fn exp_str(e: i32) -> String {
    if e < 0 {
        format!("ᴇ⁻{}", -e)
    } else {
        format!("ᴇ{}", e)
    }
}

fn drop_leading_zero(s: String) -> String {
    match s.strip_prefix("0.") {
        Some(rest) => format!(".{rest}"),
        None => s,
    }
}

/// Format a mantissa/exponent pair with the given number of decimals (or
/// trimmed float precision).
fn sci(x: f64, fix: Option<u8>, eng: bool) -> String {
    let digits = fix.map(|f| f as usize).unwrap_or(9);
    let mut e = x.abs().log10().floor() as i32;
    let mut m = x.abs() / 10f64.powi(e);
    // Re-normalize after rounding (e.g. 9.9999999999 -> 10.0).
    let rounded: f64 = format!("{:.*}", digits, m).parse().unwrap_or(m);
    if rounded >= 10.0 {
        e += 1;
        m = x.abs() / 10f64.powi(e);
    }
    if eng {
        let shift = e.rem_euclid(3);
        e -= shift;
        m = x.abs() / 10f64.powi(e);
    }
    let int_digits = if eng { (m.log10().floor() as usize) + 1 } else { 1 };
    let dec = if fix.is_some() { digits } else { 10usize.saturating_sub(int_digits) };
    let mut ms = format!("{:.*}", dec, m);
    if fix.is_none() {
        ms = trim_zeros(&ms);
    }
    format!("{}{}", ms, exp_str(e))
}

pub fn format_num(x: f64, fmt: NumFormat) -> String {
    let neg = x < 0.0;
    let ax = x.abs();
    let body = if ax == 0.0 {
        match (fmt.notation, fmt.fix) {
            (Notation::Normal, Some(f)) if f > 0 => drop_leading_zero(format!("{:.*}", f as usize, 0.0)),
            (Notation::Normal, _) => "0".to_string(),
            (_, Some(f)) if f > 0 => format!("{:.*}ᴇ0", f as usize, 0.0),
            _ => "0ᴇ0".to_string(),
        }
    } else {
        match fmt.notation {
            Notation::Sci => sci(ax, fmt.fix, false),
            Notation::Eng => sci(ax, fmt.fix, true),
            Notation::Normal => {
                // Round to 10 significant digits first.
                let r: f64 = format!("{:.9e}", ax).parse().unwrap_or(ax);
                let e = r.log10().floor() as i32;
                if e >= 10 || (fmt.fix.is_none() && e < -3) {
                    sci(ax, fmt.fix, false)
                } else {
                    let s = match fmt.fix {
                        Some(f) => {
                            let s = format!("{:.*}", f as usize, r);
                            // FIX can't show more than 10 digits total.
                            let max_dec = (9 - e.max(0)) as usize;
                            if f as usize > max_dec {
                                trim_zeros(&format!("{:.*}", max_dec, r))
                            } else {
                                s
                            }
                        }
                        None => trim_zeros(&format!("{:.*}", (9 - e).max(0) as usize, r)),
                    };
                    if fmt.fix.is_some() && s.chars().all(|c| c == '0' || c == '.') && ax != 0.0 {
                        // Tiny value under FIX: fall back to scientific.
                        sci(ax, fmt.fix, false)
                    } else {
                        drop_leading_zero(s)
                    }
                }
            }
        }
    };
    if neg && body.chars().any(|c| c.is_ascii_digit() && c != '0') {
        format!("⁻{body}")
    } else {
        body
    }
}

/// Format with as many digits as fit in `width` characters, dropping decimals
/// before falling back to scientific notation.
pub fn fit_width(x: f64, width: usize) -> String {
    let s = format_num(x, NumFormat::default());
    if s.chars().count() <= width {
        return s;
    }
    let ax = x.abs();
    if ax >= 1e-3 && ax < 1e10 {
        for d in (0..=9).rev() {
            let t = format_num(x, NumFormat { notation: Notation::Normal, fix: Some(d) });
            let t = trim_zeros(&t);
            if t.chars().count() <= width {
                return t;
            }
        }
    }
    for d in (0..=8).rev() {
        let t = format_num(x, NumFormat { notation: Notation::Sci, fix: Some(d) });
        let t = match t.split_once('ᴇ') {
            Some((m, e)) => format!("{}ᴇ{e}", trim_zeros(m)),
            None => t,
        };
        if t.chars().count() <= width {
            return t;
        }
    }
    s.chars().take(width).collect()
}

/// Best rational approximation with denominator ≤ 9999, if it is accurate.
pub fn to_fraction(x: f64) -> Option<(i64, i64)> {
    if !x.is_finite() || x.abs() >= 1e10 {
        return None;
    }
    let (mut h0, mut h1) = (0i64, 1i64);
    let (mut k0, mut k1) = (1i64, 0i64);
    let mut v = x.abs();
    for _ in 0..40 {
        let a = v.floor();
        if a > 1e12 {
            break;
        }
        let a = a as i64;
        let h2 = a.checked_mul(h1)?.checked_add(h0)?;
        let k2 = a.checked_mul(k1)?.checked_add(k0)?;
        if k2 > 9999 {
            break;
        }
        h0 = h1;
        h1 = h2;
        k0 = k1;
        k1 = k2;
        let approx = h1 as f64 / k1 as f64;
        if (approx - x.abs()).abs() <= 1e-10 * x.abs().max(1.0) {
            let sign = if x < 0.0 { -1 } else { 1 };
            return Some((sign * h1, k1));
        }
        let f = v - v.floor();
        if f < 1e-14 {
            break;
        }
        v = 1.0 / f;
    }
    None
}

fn format_frac_or_num(x: f64, fmt: NumFormat) -> String {
    match to_fraction(x) {
        Some((p, 1)) => format_num(p as f64, fmt),
        Some((p, q)) => {
            let sign = if p < 0 { "⁻" } else { "" };
            format!("{sign}{}/{q}", p.abs())
        }
        None => format_num(x, fmt),
    }
}

pub fn format_value(v: &Value, fmt: NumFormat, frac: bool) -> String {
    let f = |x: f64| if frac { format_frac_or_num(x, fmt) } else { format_num(x, fmt) };
    match v {
        Value::Num(x) => f(*x),
        Value::List(xs) => {
            let items: Vec<String> = xs.iter().map(|&x| f(x)).collect();
            format!("{{{}}}", items.join(" "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(x: f64) -> String {
        format_num(x, NumFormat::default())
    }

    #[test]
    fn float_format() {
        assert_eq!(f(0.0), "0");
        assert_eq!(f(0.5), ".5");
        assert_eq!(f(-0.5), "⁻.5");
        assert_eq!(f(2.0), "2");
        assert_eq!(f(1.0 / 3.0), ".3333333333");
        assert_eq!(f(2.0 / 3.0), ".6666666667");
        assert_eq!(f(std::f64::consts::PI), "3.141592654");
        assert_eq!(f(1e10), "1ᴇ10");
        assert_eq!(f(12345678901.0), "1.23456789ᴇ10");
        assert_eq!(f(9999999999.0), "9999999999");
        assert_eq!(f(0.001), ".001");
        assert_eq!(f(0.0001), "1ᴇ⁻4");
        assert_eq!(f(0.1 + 0.2), ".3");
        assert_eq!(f(-1234.5), "⁻1234.5");
    }

    #[test]
    fn sci_and_fix() {
        let sci = NumFormat { notation: Notation::Sci, fix: None };
        assert_eq!(format_num(1234.0, sci), "1.234ᴇ3");
        let eng = NumFormat { notation: Notation::Eng, fix: None };
        assert_eq!(format_num(12345.0, eng), "12.345ᴇ3");
        let fix2 = NumFormat { notation: Notation::Normal, fix: Some(2) };
        assert_eq!(format_num(3.14159, fix2), "3.14");
        assert_eq!(format_num(0.5, fix2), ".50");
    }

    #[test]
    fn fractions() {
        assert_eq!(to_fraction(0.75), Some((3, 4)));
        assert_eq!(to_fraction(-1.0 / 3.0), Some((-1, 3)));
        assert_eq!(to_fraction(std::f64::consts::PI), None);
    }
}
