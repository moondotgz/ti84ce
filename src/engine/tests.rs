use super::*;

fn run(env: &mut Env, s: &str) -> Result<String, CalcError> {
    let toks = string_to_tokens(s);
    let line = parse_line(&toks)?;
    let (v, frac) = env.run_line(&line)?;
    Ok(format_value(&v, NumFormat::default(), frac))
}

fn ev(s: &str) -> String {
    let mut env = Env::default();
    match run(&mut env, s) {
        Ok(v) => v,
        Err(e) => format!("ERR:{}", e.title()),
    }
}

#[test]
fn arithmetic() {
    assert_eq!(ev("1+2*3"), "7");
    assert_eq!(ev("(1+2)*3"), "9");
    assert_eq!(ev("2^3^2"), "64");
    assert_eq!(ev("⁻2²"), "⁻4");
    assert_eq!(ev("2(3+4"), "14");
    assert_eq!(ev("2π"), "6.283185307");
    assert_eq!(ev("1/0"), "ERR:DIVIDE BY 0");
    assert_eq!(ev("5!"), "120");
    assert_eq!(ev("5 nCr 2"), "10");
    assert_eq!(ev("5 nPr 2"), "20");
    assert_eq!(ev("3ˣ√8"), "2");
    assert_eq!(ev("√(16"), "4");
    assert_eq!(ev("√(⁻1)"), "ERR:NONREAL ANSWERS");
    assert_eq!(ev("2⁻¹"), ".5");
    assert_eq!(ev("1.5ᴇ3"), "1500");
    assert_eq!(ev("ᴇ⁻2"), ".01");
    assert_eq!(ev("1+"), "ERR:SYNTAX");
    assert_eq!(ev("0.75►Frac"), "3/4");
    assert_eq!(ev("1/3+1/6►Frac"), "1/2");
    assert_eq!(ev("2>1 and 3<2"), "0");
    assert_eq!(ev("not(0)"), "1");
}

#[test]
fn trig_and_logs() {
    assert_eq!(ev("sin(π)"), "0");
    assert_eq!(ev("cos(π/3"), ".5");
    assert_eq!(ev("ln(ℯ)"), "1");
    assert_eq!(ev("log(1000)"), "3");
    assert_eq!(ev("tan⁻¹(1)"), ".7853981634");
    assert_eq!(ev("logBASE(8,2)"), "3");
    let mut env = Env::default();
    env.degree = true;
    assert_eq!(run(&mut env, "sin(30)").unwrap(), ".5");
    assert_eq!(run(&mut env, "tan(45)").unwrap(), "1");
    assert!(run(&mut env, "tan(90)").is_err());
}

#[test]
fn variables_and_lists() {
    let mut env = Env::default();
    assert_eq!(run(&mut env, "5→A").unwrap(), "5");
    assert_eq!(run(&mut env, "2A+1").unwrap(), "11");
    assert_eq!(run(&mut env, "Ans*2").unwrap(), "22");
    assert_eq!(run(&mut env, "{1,2,3}→L₁").unwrap(), "{1 2 3}");
    assert_eq!(run(&mut env, "L₁²").unwrap(), "{1 4 9}");
    assert_eq!(run(&mut env, "sum(L₁)").unwrap(), "6");
    assert_eq!(run(&mut env, "L₁(2)").unwrap(), "2");
    assert_eq!(run(&mut env, "seq(X²,X,1,4)").unwrap(), "{1 4 9 16}");
    assert_eq!(run(&mut env, "3→B:B+1").unwrap(), "4");
    assert_eq!(run(&mut env, "{1,2}+{1,2,3}").unwrap_err(), CalcError::DimMismatch);
}

#[test]
fn calculus_and_stats() {
    assert_eq!(ev("fnInt(X²,X,0,3)"), "9");
    assert_eq!(ev("fnInt(X³,X,⁻1,1)"), "0");
    assert_eq!(ev("nDeriv(X³,X,2)"), "12.000001");
    assert_eq!(ev("normalcdf(⁻1,1)"), ".6826894921");
    assert_eq!(ev("invNorm(.975)"), "1.959963985");
    assert_eq!(ev("binompdf(10,.5,5)"), ".24609375");
    assert_eq!(ev("gcd(12,18)"), "6");
    assert_eq!(ev("lcm(4,6)"), "12");
}

#[test]
fn y_functions() {
    let mut env = Env::default();
    env.yfuncs[0] = Some(parse_expr(&string_to_tokens("X²+1")).unwrap());
    assert_eq!(run(&mut env, "Y₁(3)").unwrap(), "10");
    assert_eq!(env.eval_y(0, 2.0).unwrap(), 5.0);
}
