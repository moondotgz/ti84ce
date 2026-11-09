//! Per-character syntax coloring for Python source shown on the LCD.

pub const DEFAULT: u32 = 0x000000;
pub const KEYWORD: u32 = 0x0033B3;
pub const BUILTIN: u32 = 0x8A2BB8;
pub const STRING: u32 = 0x18803A;
pub const NUMBER: u32 = 0xC2410C;
pub const COMMENT: u32 = 0x8A8A8A;
pub const DEFNAME: u32 = 0x00707F;

const KEYWORDS: &[&str] = &[
    "and", "as", "assert", "break", "class", "continue", "def", "del", "elif", "else", "except",
    "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "nonlocal", "not",
    "or", "pass", "raise", "return", "try", "while", "with", "yield", "True", "False", "None",
];

const BUILTINS: &[&str] = &[
    "print", "input", "len", "range", "int", "float", "str", "bool", "abs", "round", "min",
    "max", "sum", "list", "tuple", "dict", "sorted", "reversed", "enumerate", "zip", "chr",
    "ord", "divmod", "pow", "type", "hex", "bin", "oct", "map", "filter", "any", "all", "repr",
    "isinstance", "math", "random", "time", "Exception", "ValueError", "TypeError",
    "ZeroDivisionError", "IndexError", "KeyError", "NameError",
];

/// Lexer state carried between lines (an open triple-quoted string).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct State {
    pub triple: Option<char>,
}

/// Colors for each character of `line`, updating `state` for the next line.
pub fn highlight_line(line: &str, state: &mut State) -> Vec<u32> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = vec![DEFAULT; chars.len()];
    let mut i = 0;
    let mut prev_word = String::new();
    while i < chars.len() {
        // Inside a triple-quoted string continued from an earlier line.
        if let Some(q) = state.triple {
            let start = i;
            while i < chars.len() {
                if chars[i] == q && chars.get(i + 1) == Some(&q) && chars.get(i + 2) == Some(&q) {
                    i += 3;
                    state.triple = None;
                    break;
                }
                i += 1;
            }
            out[start..i].fill(STRING);
            continue;
        }
        let c = chars[i];
        if c == '#' {
            out[i..].fill(COMMENT);
            break;
        }
        if c == '"' || c == '\'' {
            let start = i;
            // String prefix letters (f, r, b) are colored with the string.
            let mut s = start;
            while s > 0 && matches!(chars[s - 1], 'f' | 'F' | 'r' | 'R' | 'b' | 'B') && out[s - 1] == DEFAULT {
                s -= 1;
            }
            if chars.get(i + 1) == Some(&c) && chars.get(i + 2) == Some(&c) {
                i += 3;
                state.triple = Some(c);
                while i < chars.len() {
                    if chars[i] == c && chars.get(i + 1) == Some(&c) && chars.get(i + 2) == Some(&c) {
                        i += 3;
                        state.triple = None;
                        break;
                    }
                    i += 1;
                }
            } else {
                i += 1;
                while i < chars.len() && chars[i] != c {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i = (i + 1).min(chars.len());
            }
            out[s..i].fill(STRING);
            prev_word.clear();
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '_') {
                i += 1;
            }
            out[start..i].fill(NUMBER);
            prev_word.clear();
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            // Don't swallow a string prefix like f"..."; the quote branch colors it.
            let next = chars.get(i).copied();
            let color = if matches!(word.as_str(), "f" | "F" | "r" | "R" | "b" | "B") && matches!(next, Some('"') | Some('\'')) {
                DEFAULT
            } else if prev_word == "def" {
                DEFNAME
            } else if KEYWORDS.contains(&word.as_str()) {
                KEYWORD
            } else if BUILTINS.contains(&word.as_str()) && !(start > 0 && chars[start - 1] == '.') {
                BUILTIN
            } else {
                DEFAULT
            };
            out[start..i].fill(color);
            prev_word = word;
            continue;
        }
        if !c.is_whitespace() {
            prev_word.clear();
        }
        i += 1;
    }
    out
}

/// Highlight a single line with no carried-over state.
pub fn highlight(line: &str) -> Vec<u32> {
    highlight_line(line, &mut State::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(line: &str) -> Vec<(String, u32)> {
        let colors = highlight(line);
        let mut out: Vec<(String, u32)> = Vec::new();
        for (c, col) in line.chars().zip(colors) {
            match out.last_mut() {
                Some((s, k)) if *k == col => s.push(c),
                _ => out.push((c.to_string(), col)),
            }
        }
        out.into_iter().filter(|(s, _)| !s.trim().is_empty()).collect()
    }

    #[test]
    fn colors_tokens() {
        let r = runs("def fib(n):  # comment");
        assert_eq!(r[0], ("def".into(), KEYWORD));
        assert_eq!(r[1], ("fib".into(), DEFNAME));
        assert_eq!(r.last().unwrap(), &("# comment".into(), COMMENT));
        let r = runs("print(f\"x={x}\", 3.5)");
        assert_eq!(r[0], ("print".into(), BUILTIN));
        assert!(r.contains(&("f\"x={x}\"".into(), STRING)));
        assert!(r.contains(&("3.5".into(), NUMBER)));
        // Keywords inside strings and attribute names aren't colored.
        let r = runs("s = 'for if' ; x.len");
        assert!(r.contains(&("'for if'".into(), STRING)));
        assert!(r.iter().all(|(t, c)| !(t.contains("len") && *c == BUILTIN)));
    }

    #[test]
    fn triple_quotes_span_lines() {
        let mut st = State::default();
        let a = highlight_line("x = \"\"\"start", &mut st);
        assert_eq!(*a.last().unwrap(), STRING);
        assert!(st.triple.is_some());
        let b = highlight_line("end\"\"\" + 1", &mut st);
        assert_eq!(b[0], STRING);
        assert_eq!(*b.last().unwrap(), NUMBER);
        assert!(st.triple.is_none());
    }
}
