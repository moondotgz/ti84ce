use super::view::{Body, Row};
use super::*;

fn text(rows: &[Row]) -> Vec<String> {
    rows.iter().map(|r| r.iter().map(|s| s.text.as_str()).collect::<String>().trim().to_string()).collect()
}

fn screen_text(c: &mut Calc) -> Vec<String> {
    c.blink = false;
    match c.view().body {
        Body::Text(rows) => text(&rows),
        Body::Graph(g) => text(&g.bottom),
        Body::Off => vec![],
    }
}

fn calc(script: &str) -> Calc {
    let mut c = Calc::new();
    c.run_script(script);
    c
}

#[test]
fn home_evaluates_and_chains_ans() {
    let mut c = calc("2 + 3 enter * 4 enter");
    let t = screen_text(&mut c);
    assert!(t.contains(&"5".to_string()), "{t:?}");
    assert!(t.contains(&"Ans*4".to_string()), "{t:?}");
    assert!(t.contains(&"20".to_string()), "{t:?}");
}

#[test]
fn keyboard_function_names_fold_into_tokens() {
    let c = calc("\"sqrt(16)+sin(0)");
    assert_eq!(c.home.editor.text(), "√(16)+sin(0)");
}

#[test]
fn errors_offer_goto() {
    let mut c = calc("2 + + 3 enter");
    assert!(matches!(c.screen, Screen::Error(_)));
    c.press(Key::D2);
    assert!(matches!(c.screen, Screen::Home));
    assert_eq!(c.home.editor.text(), "2++3");
    assert_eq!(c.home.editor.cur, 2);
}

#[test]
fn entry_recall_and_empty_enter_repeat() {
    let mut c = calc("\"1+1 enter 2nd enter");
    assert_eq!(c.home.editor.text(), "1+1");
    c.press(Key::Clear);
    c.press(Key::Enter);
    assert_eq!(c.home.history.len(), 2);
}

#[test]
fn one_var_stats() {
    let mut c = calc("\"{1,2,3,4}→L1 enter");
    // Keyboard can't type L₁; use the 2nd-1 key instead.
    assert!(matches!(c.screen, Screen::Error(_)) || c.env.lists[0].is_empty());
    c = calc("\"{1,2,3,4} sto 2nd 1 enter stat right 1");
    assert_eq!(c.env.lists[0], vec![1.0, 2.0, 3.0, 4.0]);
    c.press(Key::Enter);
    let t = screen_text(&mut c);
    assert!(t.contains(&"x̄=2.5".to_string()), "{t:?}");
    assert!(t.contains(&"n=4".to_string()), "{t:?}");
}

#[test]
fn linreg_stores_equation() {
    let mut c = calc("\"{1,2,3} sto 2nd 1 enter \"{3,5,7} sto 2nd 2 enter stat right 3 2nd 1 , 2nd 2 , vars enter enter enter");
    let t = screen_text(&mut c);
    assert!(t.contains(&"a=2".to_string()), "{t:?}");
    assert!(t.contains(&"b=1".to_string()), "{t:?}");
    assert_eq!(c.env.eval_y(0, 10.0).unwrap(), 21.0);
}

#[test]
fn table_and_zero() {
    let mut c = calc("y= \"X^2-4 2nd graph");
    let t = screen_text(&mut c);
    assert!(t.iter().any(|l| l.starts_with('0') && l.ends_with("⁻4")), "{t:?}");
    c.run_script("2nd trace 2 \"0 enter \"5 enter enter");
    let t = screen_text(&mut c);
    assert!(t.contains(&"Zero".to_string()), "{t:?}");
    assert!(t.iter().any(|l| l.starts_with("X=2 ")), "{t:?}");
}

#[test]
fn integral_and_intersect() {
    let mut c = calc("y= \"X enter \"4-X 2nd trace 5 enter enter enter");
    let t = screen_text(&mut c);
    assert!(t.contains(&"Intersection".to_string()), "{t:?}");
    assert!(t.iter().any(|l| l.starts_with("X=2 ")), "{t:?}");
    c.run_script("2nd trace 7 \"0 enter \"2 enter");
    let t = screen_text(&mut c);
    assert!(t.contains(&"∫f(x)dx=2".to_string()), "{t:?}");
}

#[test]
fn window_editor_validates() {
    let mut c = calc("window \"5 enter");
    assert_eq!(c.window.xmin, 5.0);
    c.run_script("up \"-3 enter");
    assert_eq!(c.window.xmin, -3.0);
    c = calc("window \"50 enter");
    assert!(matches!(c.screen, Screen::Error(_)));
}

#[test]
fn mode_changes_angle_and_format() {
    let mut c = calc("mode down down right enter down down down down down down right enter 2nd mode \"sin(90)+1/4 enter");
    assert!(c.env.degree);
    let t = screen_text(&mut c);
    assert!(t.contains(&"5/4".to_string()), "{t:?}");
}

#[test]
fn list_editor_edits_cells() {
    let c = calc("stat enter \"7 enter \"8 enter right \"1 enter");
    assert_eq!(c.env.lists[0], vec![7.0, 8.0]);
    assert_eq!(c.env.lists[1], vec![1.0]);
}

#[test]
fn python_shell_runs_code() {
    let mut c = calc("apps enter trace \"x_=_6*7 enter wait \"print(x) enter wait");
    let t = screen_text(&mut c);
    assert!(t.contains(&"42".to_string()), "{t:?}");
}
