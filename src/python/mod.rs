//! A small Python interpreter (lexer, parser, tree-walking evaluator) that
//! runs on a worker thread so long-running scripts don't block the UI.

pub mod highlight;
pub mod interp;
pub mod lexer;
pub mod parser;

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub enum PyCmd {
    Run { src: String, file: String, repl: bool },
    Reset,
    Input(Option<String>),
}

pub enum PyEvent {
    Output(String),
    InputRequest(String),
    Done,
}

pub struct PyWorker {
    tx: Sender<PyCmd>,
    rx: Receiver<PyEvent>,
    interrupt: Arc<AtomicBool>,
    pub busy: bool,
}

impl PyWorker {
    pub fn spawn() -> Self {
        let (cmd_tx, cmd_rx) = channel::<PyCmd>();
        let (ev_tx, ev_rx) = channel::<PyEvent>();
        let interrupt = Arc::new(AtomicBool::new(false));
        let flag = interrupt.clone();
        std::thread::Builder::new()
            .name("python".into())
            .stack_size(512 * 1024 * 1024)
            .spawn(move || {
                let cmd_rx = Rc::new(cmd_rx);
                let out_tx = ev_tx.clone();
                let in_tx = ev_tx.clone();
                let in_rx = cmd_rx.clone();
                let mut interp = interp::Interp::new(
                    Box::new(move |s: &str| {
                        let _ = out_tx.send(PyEvent::Output(s.to_string()));
                    }),
                    Box::new(move |prompt: &str| {
                        let _ = in_tx.send(PyEvent::InputRequest(prompt.to_string()));
                        loop {
                            match in_rx.recv() {
                                Ok(PyCmd::Input(v)) => return v,
                                Ok(_) => continue,
                                Err(_) => return None,
                            }
                        }
                    }),
                    flag,
                );
                while let Ok(cmd) = cmd_rx.recv() {
                    match cmd {
                        PyCmd::Run { src, file, repl } => {
                            interp.run(&src, &file, repl);
                            let _ = ev_tx.send(PyEvent::Done);
                        }
                        PyCmd::Reset => interp.reset(),
                        PyCmd::Input(_) => {}
                    }
                }
            })
            .expect("spawn python worker");
        PyWorker { tx: cmd_tx, rx: ev_rx, interrupt, busy: false }
    }

    pub fn run(&mut self, src: String, file: &str, repl: bool) {
        self.busy = true;
        let _ = self.tx.send(PyCmd::Run { src, file: file.to_string(), repl });
    }

    pub fn reset(&self) {
        let _ = self.tx.send(PyCmd::Reset);
    }

    pub fn send_input(&self, text: Option<String>) {
        let _ = self.tx.send(PyCmd::Input(text));
    }

    pub fn interrupt(&self) {
        self.interrupt.store(true, Ordering::SeqCst);
    }

    pub fn poll(&mut self) -> Vec<PyEvent> {
        let mut out = Vec::new();
        while let Ok(e) = self.rx.try_recv() {
            if matches!(e, PyEvent::Done) {
                self.busy = false;
            }
            out.push(e);
        }
        out
    }
}

/// Whether a shell entry needs more lines before it can run (open block or brackets).
pub fn needs_more(src: &str) -> bool {
    let trimmed = src.trim_end();
    if trimmed.ends_with(':') || trimmed.ends_with('\\') {
        return true;
    }
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut prev = '\0';
    for c in src.chars() {
        match quote {
            Some(q) => {
                if c == q && prev != '\\' {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '#' => {}
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            },
        }
        prev = c;
    }
    depth > 0
}

#[cfg(test)]
mod tests {
    use super::interp::Interp;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn run(src: &str) -> String {
        let out = Rc::new(RefCell::new(String::new()));
        let o = out.clone();
        let mut it = Interp::new(
            Box::new(move |s| o.borrow_mut().push_str(s)),
            Box::new(|_| Some("42".into())),
            Arc::new(AtomicBool::new(false)),
        );
        it.run(src, "<stdin>", true);
        let r = out.borrow().clone();
        r
    }

    #[test]
    fn basics() {
        assert_eq!(run("1+2"), "3\n");
        assert_eq!(run("7/2"), "3.5\n");
        assert_eq!(run("7//2, -7//2, 7%3, -7%3"), "(3, -4, 1, 2)\n");
        assert_eq!(run("2**10"), "1024\n");
        assert_eq!(run("0.1+0.2"), "0.30000000000000004\n");
        assert_eq!(run("1e20"), "1e+20\n");
        assert_eq!(run("10/5"), "2.0\n");
        assert_eq!(run("'ab'*3"), "'ababab'\n");
        assert_eq!(run("print('hi', 3)"), "hi 3\n");
        assert_eq!(run("x = [3,1,2]\nx.sort()\nx"), "[1, 2, 3]\n");
        assert_eq!(run("[i*i for i in range(5) if i%2==0]"), "[0, 4, 16]\n");
        assert_eq!(run("a, b = 1, 2\na, b = b, a\nprint(a, b)"), "2 1\n");
        assert_eq!(run("f'{3.14159:.2f} and {1+1}'"), "'3.14 and 2'\n");
        assert_eq!(run("'{} + {} = {}'.format(1, 2, 3)"), "'1 + 2 = 3'\n");
        assert_eq!(run("'%d items at %.2f' % (3, 1.5)"), "'3 items at 1.50'\n");
        assert_eq!(run("round(2.5), round(3.5), round(3.14159, 2)"), "(2, 4, 3.14)\n");
        assert_eq!(run("d = {'a': 1}\nd['b'] = 2\nd"), "{'a': 1, 'b': 2}\n");
        assert_eq!(run("s = 'Hello'\ns[::-1], s[1:3], len(s)"), "('olleH', 'el', 5)\n");
    }

    #[test]
    fn functions_and_control_flow() {
        let src = "def fib(n):\n  a, b = 0, 1\n  for _ in range(n):\n    a, b = b, a+b\n  return a\nprint(fib(30))";
        assert_eq!(run(src), "832040\n");
        let src = "def fact(n):\n  if n <= 1:\n    return 1\n  return n * fact(n-1)\nfact(10)";
        assert_eq!(run(src), "3628800\n");
        let src = "n = 0\nwhile True:\n  n += 1\n  if n > 5:\n    break\nn";
        assert_eq!(run(src), "6\n");
        let src = "from math import *\nprint(sqrt(16), floor(2.7), pi)";
        assert_eq!(run(src), "4.0 2 3.141592653589793\n");
        let src = "import math\nmath.cos(0)";
        assert_eq!(run(src), "1.0\n");
        let src = "try:\n  1/0\nexcept ZeroDivisionError as e:\n  print('caught', e)";
        assert_eq!(run(src), "caught division by zero\n");
        let src = "sq = lambda x: x*x\nlist(map(sq, [1,2,3]))";
        assert_eq!(run(src), "[1, 4, 9]\n");
        assert_eq!(run("x = input('n? ')\nint(x) + 1"), "43\n");
    }

    #[test]
    fn errors() {
        let out = run("print(y)");
        assert!(out.contains("NameError: name 'y' isn't defined"), "{out}");
        let out = run("1/0");
        assert!(out.contains("ZeroDivisionError"), "{out}");
        let out = run("[1,2][5]");
        assert!(out.contains("IndexError"), "{out}");
        let out = run("if True\n  pass");
        assert!(out.contains("SyntaxError"), "{out}");
    }

    #[test]
    fn needs_more_lines() {
        assert!(super::needs_more("def f():"));
        assert!(super::needs_more("x = (1,"));
        assert!(!super::needs_more("x = 1"));
    }
}
