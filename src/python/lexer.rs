//! Tokenizer for the Python subset, with INDENT/DEDENT tracking.

#[derive(Debug, Clone, PartialEq)]
pub enum T {
    Int(i64),
    Float(f64),
    Str(String),
    FStr(String),
    Name(String),
    Op(&'static str),
    Newline,
    Indent,
    Dedent,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub t: T,
    pub line: usize,
}

const OPS: &[&str] = &[
    "**=", "//=", "**", "//", "==", "!=", "<=", ">=", "+=", "-=", "*=", "/=", "%=", "->", "(",
    ")", "[", "]", "{", "}", ",", ":", ".", "=", "+", "-", "*", "/", "%", "<", ">", ";", "&",
    "|", "^", "~",
];

pub fn tokenize(src: &str) -> Result<Vec<Token>, (usize, String)> {
    let mut out = Vec::new();
    let mut indents = vec![0usize];
    let mut depth = 0i32;
    let mut fstring_next = false;
    let lines: Vec<&str> = src.split('\n').collect();
    let mut li = 0;
    while li < lines.len() {
        let raw = lines[li];
        let line_no = li + 1;
        li += 1;
        let chars: Vec<char> = raw.chars().collect();
        let mut i = 0;
        if depth == 0 {
            let mut col = 0;
            while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
                col += if chars[i] == '\t' { 8 - col % 8 } else { 1 };
                i += 1;
            }
            if i >= chars.len() || chars[i] == '#' {
                continue; // blank or comment-only line
            }
            let cur = *indents.last().unwrap();
            if col > cur {
                indents.push(col);
                out.push(Token { t: T::Indent, line: line_no });
            } else {
                while col < *indents.last().unwrap() {
                    indents.pop();
                    out.push(Token { t: T::Dedent, line: line_no });
                }
                if col != *indents.last().unwrap() {
                    return Err((line_no, "IndentationError: unindent doesn't match any outer indent level".into()));
                }
            }
        }
        while i < chars.len() {
            let c = chars[i];
            if c == ' ' || c == '\t' || c == '\r' {
                i += 1;
                continue;
            }
            if c == '#' {
                break;
            }
            if c == '\\' && i == chars.len() - 1 {
                i += 1;
                continue;
            }
            if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
                let start = i;
                if c == '0' && matches!(chars.get(i + 1), Some('x') | Some('X') | Some('b') | Some('B')) {
                    let radix = if matches!(chars[i + 1], 'x' | 'X') { 16 } else { 2 };
                    i += 2;
                    let s = i;
                    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    let text: String = chars[s..i].iter().filter(|&&c| c != '_').collect();
                    let v = i64::from_str_radix(&text, radix)
                        .map_err(|_| (line_no, "SyntaxError: invalid syntax for number".to_string()))?;
                    out.push(Token { t: T::Int(v), line: line_no });
                    continue;
                }
                let mut is_float = false;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                    i += 1;
                }
                if i < chars.len() && chars[i] == '.' {
                    is_float = true;
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                    let save = i;
                    i += 1;
                    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                        i += 1;
                    }
                    if i < chars.len() && chars[i].is_ascii_digit() {
                        is_float = true;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    } else {
                        i = save;
                    }
                }
                let text: String = chars[start..i].iter().filter(|&&c| c != '_').collect();
                let t = if is_float {
                    T::Float(text.parse().map_err(|_| (line_no, "SyntaxError: invalid number".to_string()))?)
                } else {
                    match text.parse::<i64>() {
                        Ok(v) => T::Int(v),
                        Err(_) => T::Float(text.parse().unwrap_or(f64::INFINITY)),
                    }
                };
                out.push(Token { t, line: line_no });
                continue;
            }
            if c.is_alphabetic() || c == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let name: String = chars[start..i].iter().collect();
                // String prefixes: f"..." becomes an f-string, r/b are plain.
                if matches!(name.as_str(), "f" | "F" | "r" | "b")
                    && i < chars.len()
                    && (chars[i] == '"' || chars[i] == '\'')
                {
                    if name == "f" || name == "F" {
                        fstring_next = true;
                    }
                    continue;
                }
                out.push(Token { t: T::Name(name), line: line_no });
                continue;
            }
            if c == '"' || c == '\'' {
                let q = c;
                // Triple-quoted strings may span lines.
                let triple = chars.get(i + 1) == Some(&q) && chars.get(i + 2) == Some(&q);
                if triple {
                    let mut s = String::new();
                    let mut rest: Vec<char> = chars[i + 3..].to_vec();
                    loop {
                        let text: String = rest.iter().collect();
                        let pat: String = std::iter::repeat_n(q, 3).collect();
                        if let Some(pos) = text.find(&pat) {
                            s.push_str(&text[..pos]);
                            let consumed = text[..pos + 3].chars().count();
                            let remaining: Vec<char> = rest[consumed..].to_vec();
                            out.push(Token { t: T::Str(s), line: line_no });
                            // Continue lexing the remainder of this physical line.
                            let rem: String = remaining.iter().collect();
                            let sub = tokenize_fragment(&rem, line_no)?;
                            out.extend(sub);
                            break;
                        }
                        s.push_str(&text);
                        s.push('\n');
                        if li >= lines.len() {
                            return Err((line_no, "SyntaxError: unterminated triple-quoted string".into()));
                        }
                        rest = lines[li].chars().collect();
                        li += 1;
                    }
                    i = chars.len();
                    continue;
                }
                i += 1;
                let mut s = String::new();
                loop {
                    if i >= chars.len() {
                        return Err((line_no, "SyntaxError: unterminated string".into()));
                    }
                    let ch = chars[i];
                    if ch == q {
                        i += 1;
                        break;
                    }
                    if ch == '\\' && i + 1 < chars.len() {
                        i += 1;
                        let esc = match chars[i] {
                            'n' => '\n',
                            't' => '\t',
                            '\\' => '\\',
                            '\'' => '\'',
                            '"' => '"',
                            '0' => '\0',
                            other => {
                                s.push('\\');
                                other
                            }
                        };
                        s.push(esc);
                        i += 1;
                        continue;
                    }
                    s.push(ch);
                    i += 1;
                }
                let t = if std::mem::take(&mut fstring_next) { T::FStr(s) } else { T::Str(s) };
                out.push(Token { t, line: line_no });
                continue;
            }
            let rest: String = chars[i..chars.len().min(i + 3)].iter().collect();
            let op = OPS.iter().find(|op| rest.starts_with(**op));
            match op {
                Some(op) => {
                    match *op {
                        "(" | "[" | "{" => depth += 1,
                        ")" | "]" | "}" => depth = (depth - 1).max(0),
                        _ => {}
                    }
                    out.push(Token { t: T::Op(op), line: line_no });
                    i += op.chars().count();
                }
                None => {
                    return Err((line_no, format!("SyntaxError: invalid character '{c}'")));
                }
            }
        }
        if depth == 0 && !matches!(out.last().map(|t| &t.t), Some(T::Newline) | None) {
            out.push(Token { t: T::Newline, line: line_no });
        }
    }
    let last = lines.len();
    if !matches!(out.last().map(|t| &t.t), Some(T::Newline) | None) {
        out.push(Token { t: T::Newline, line: last });
    }
    while indents.len() > 1 {
        indents.pop();
        out.push(Token { t: T::Dedent, line: last });
    }
    out.push(Token { t: T::Eof, line: last });
    Ok(out)
}

/// Tokenize the remainder of a line (used after a triple-quoted string closes).
fn tokenize_fragment(s: &str, line: usize) -> Result<Vec<Token>, (usize, String)> {
    let mut toks = tokenize(s)?;
    toks.retain(|t| !matches!(t.t, T::Newline | T::Eof | T::Indent | T::Dedent));
    for t in &mut toks {
        t.line = line;
    }
    Ok(toks)
}
