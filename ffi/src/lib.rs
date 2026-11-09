//! C ABI over `ti84-core` for native front ends. The screen is exchanged as
//! JSON so the front end only needs a decoder, not mirrored Rust types.
//! See `include/ti84.h` for the C declarations.

use serde_json::{json, Value};
use std::ffi::{c_char, CStr, CString};
use std::path::PathBuf;
use ti84_core::calc::view::{Body, CursorStyle, Grid, Indicator, Row, ViewModel};
use ti84_core::calc::Calc;
use ti84_core::keys::{Key, KeyDef, KeyStyle, GRID, TOP_ROW, UPPER_LEFT};

/// Opaque handle given to C.
pub struct Ti84 {
    calc: Calc,
}

unsafe fn str_arg<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    // SAFETY: the caller passes a valid NUL-terminated string.
    unsafe { CStr::from_ptr(p) }.to_str().ok()
}

fn into_c(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

/// # Safety
/// `python_dir` must be NULL or a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ti84_new(python_dir: *const c_char) -> *mut Ti84 {
    let dir = unsafe { str_arg(python_dir) }.map(PathBuf::from);
    Box::into_raw(Box::new(Ti84 { calc: Calc::with_python_dir(dir) }))
}

/// # Safety
/// `calc` must come from `ti84_new` and not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ti84_free(calc: *mut Ti84) {
    if !calc.is_null() {
        drop(unsafe { Box::from_raw(calc) });
    }
}

/// # Safety
/// `calc` must be a live handle from `ti84_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ti84_press(calc: *mut Ti84, key_code: u32) {
    let Some(t) = (unsafe { calc.as_mut() }) else { return };
    if let Some(k) = Key::from_code(key_code) {
        t.calc.press(k);
    }
}

/// # Safety
/// `calc` must be live; `utf8` a valid NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ti84_type(calc: *mut Ti84, utf8: *const c_char) {
    let (Some(t), Some(s)) = (unsafe { calc.as_mut() }, unsafe { str_arg(utf8) }) else { return };
    for c in s.chars() {
        match c {
            '\n' | '\r' => t.calc.press(Key::Enter),
            c => t.calc.type_char(c),
        }
    }
}

/// # Safety
/// `calc` must be live; `utf8` a valid NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ti84_paste(calc: *mut Ti84, utf8: *const c_char) {
    let (Some(t), Some(s)) = (unsafe { calc.as_mut() }, unsafe { str_arg(utf8) }) else { return };
    t.calc.paste(s);
}

/// # Safety
/// `calc` must be a live handle from `ti84_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ti84_backspace(calc: *mut Ti84) {
    if let Some(t) = unsafe { calc.as_mut() } {
        t.calc.keyboard_backspace();
    }
}

/// # Safety
/// `calc` must be a live handle from `ti84_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ti84_tick(calc: *mut Ti84) -> bool {
    unsafe { calc.as_mut() }.is_some_and(|t| t.calc.tick())
}

/// # Safety
/// `calc` must be a live handle from `ti84_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ti84_set_blink(calc: *mut Ti84, on: bool) {
    if let Some(t) = unsafe { calc.as_mut() } {
        t.calc.blink = on;
    }
}

/// # Safety
/// `calc` must be a live handle from `ti84_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ti84_view_json(calc: *mut Ti84) -> *mut c_char {
    let Some(t) = (unsafe { calc.as_mut() }) else { return std::ptr::null_mut() };
    into_c(view_json(&t.calc.view()).to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn ti84_keypad_json() -> *mut c_char {
    into_c(keypad_json().to_string())
}

/// # Safety
/// `s` must come from a `ti84_*_json` call and not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ti84_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

// ----- JSON encoding -----

fn row_json(row: &Row) -> Value {
    Value::Array(row.iter().map(|s| json!({ "t": s.text, "fg": s.fg, "bg": s.bg })).collect())
}

fn pts(p: &[(f32, f32)]) -> Value {
    Value::Array(p.iter().map(|&(x, y)| json!([x, y])).collect())
}

pub fn view_json(v: &ViewModel) -> Value {
    let indicator = match v.status.indicator {
        Indicator::None => "none",
        Indicator::Second => "second",
        Indicator::Alpha => "alpha",
        Indicator::AlphaLock => "alphaLock",
        Indicator::Lower => "lower",
        Indicator::LowerLock => "lowerLock",
    };
    let body = match &v.body {
        Body::Off => json!({ "kind": "off" }),
        Body::Text(rows) => json!({ "kind": "text", "rows": rows.iter().map(row_json).collect::<Vec<_>>() }),
        Body::Graph(g) => json!({
            "kind": "graph",
            "curves": g.curves.iter().map(|c| json!({
                "color": c.color,
                "thick": c.thick,
                "dots": c.dots,
                "segments": c.segments.iter().map(|s| pts(s)).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "xAxis": g.x_axis,
            "yAxis": g.y_axis,
            "xTicks": g.x_ticks,
            "yTicks": g.y_ticks,
            "grid": match g.grid { Grid::Off => "off", Grid::Dot => "dot", Grid::Line => "line" },
            "cursor": g.cursor.map(|(x, y, style, color)| json!({
                "x": x,
                "y": y,
                "style": match style { CursorStyle::Cross => "cross", CursorStyle::Trace => "trace" },
                "color": color,
            })),
            "marks": pts(&g.marks),
            "shade": g.shade.as_ref().map(|(c, p)| json!({ "color": c, "points": pts(p) })),
            "top": g.top.as_ref().map(row_json),
            "bottom": g.bottom.iter().map(row_json).collect::<Vec<_>>(),
        }),
    };
    json!({
        "status": { "text": v.status.text, "indicator": indicator, "busy": v.status.busy },
        "softkeys": v.softkeys,
        "body": body,
    })
}

fn key_json(d: &KeyDef) -> Value {
    let style = match d.style {
        KeyStyle::Top => "top",
        KeyStyle::Func => "func",
        KeyStyle::Number => "number",
        KeyStyle::Op => "op",
        KeyStyle::Second => "second",
        KeyStyle::Alpha => "alpha",
        KeyStyle::Enter => "enter",
    };
    json!({ "code": d.key.code(), "label": d.label, "second": d.second, "alpha": d.alpha, "style": style })
}

pub fn keypad_json() -> Value {
    json!({
        "top": TOP_ROW.iter().map(key_json).collect::<Vec<_>>(),
        "upperLeft": UPPER_LEFT.iter().map(|r| r.iter().map(key_json).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "grid": GRID.iter().map(|r| r.iter().map(key_json).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "arrows": {
            "up": Key::Up.code(),
            "down": Key::Down.code(),
            "left": Key::Left.code(),
            "right": Key::Right.code(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_through_c_api() {
        unsafe {
            let c = ti84_new(std::ptr::null());
            let two = CString::new("2+3\n").unwrap();
            ti84_type(c, two.as_ptr());
            ti84_set_blink(c, false);
            let s = ti84_view_json(c);
            let v: Value = serde_json::from_str(CStr::from_ptr(s).to_str().unwrap()).unwrap();
            ti84_string_free(s);
            assert_eq!(v["body"]["kind"], "text");
            let rows = v["body"]["rows"].as_array().unwrap();
            let text: Vec<String> = rows
                .iter()
                .map(|r| r.as_array().unwrap().iter().map(|s| s["t"].as_str().unwrap()).collect::<String>())
                .collect();
            assert_eq!(text[1].trim(), "5");
            // Graph screen encodes curves.
            ti84_press(c, Key::YEq.code());
            let f = CString::new("X^2").unwrap();
            ti84_type(c, f.as_ptr());
            ti84_press(c, Key::Graph.code());
            let s = ti84_view_json(c);
            let v: Value = serde_json::from_str(CStr::from_ptr(s).to_str().unwrap()).unwrap();
            ti84_string_free(s);
            assert_eq!(v["body"]["kind"], "graph");
            assert!(!v["body"]["curves"][0]["segments"][0].as_array().unwrap().is_empty());
            ti84_free(c);
        }
    }

    #[test]
    fn keypad_lists_every_key_once() {
        let k = keypad_json();
        let mut codes: Vec<u64> = Vec::new();
        let mut collect = |v: &Value| codes.push(v["code"].as_u64().unwrap());
        k["top"].as_array().unwrap().iter().for_each(&mut collect);
        for r in k["upperLeft"].as_array().unwrap().iter().chain(k["grid"].as_array().unwrap()) {
            r.as_array().unwrap().iter().for_each(&mut collect);
        }
        for a in ["up", "down", "left", "right"] {
            codes.push(k["arrows"][a].as_u64().unwrap());
        }
        codes.sort();
        assert_eq!(codes, (0..50).collect::<Vec<u64>>());
    }
}
