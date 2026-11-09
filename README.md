# TI-84 Plus CE-T Python Edition — Rust clone for desktop and iOS

A recreation of the TI-84 Plus CE-T Python Edition graphing calculator, written
from scratch in Rust: a [GPUI](https://gpui.rs) desktop app and a SwiftUI iOS
app share one calculator core.
It doesn't use TI's ROM or OS: the math engine, grapher and Python interpreter
are all reimplemented here.

## Run (desktop)

```bash
cargo run --release
```

Linux needs Vulkan (GPUI's renderer) plus the usual Wayland/X11 dev libraries.

## iOS

The iOS app is a native SwiftUI front end (`ios/`) over the same Rust core,
linked as a static library through a small C API (`ffi/include/ti84.h`).

**From GitHub Actions:** every push builds an unsigned `.ipa`
(`.github/workflows/ios.yml`). Download it from the run's artifacts, or from
the release for a `v*` tag, and install it with a sideloading tool that
re-signs it (AltStore, SideStore, Sideloadly…).

**Locally on a Mac** (Xcode 16, [XcodeGen](https://github.com/yonaskolb/XcodeGen)):

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
ios/build-rust.sh
cd ios && xcodegen generate && open Ti84.xcodeproj
```

Python scripts are stored in the app's Documents/Python folder, which shows up
in the Files app. A hardware keyboard works the same way as on the desktop, and
long-pressing the screen offers Paste.

## What works

- **Home screen**: TI order of operations (left-associative `^`, implied
  multiplication, closing parens are optional), `Ans` chaining (an operator typed
  first acts on Ans), `→` store to A–Z/θ and L₁–L₆, `:` multi-statements, ENTRY
  recall (`2nd enter`), history selection (arrow up and press enter to paste),
  `►Frac`/`►Dec`, overwrite/insert editing, and TI-style errors with Quit/Goto.
- **Math**: trig/inverse/hyperbolic functions in radians or degrees, `log`, `ln`, `10^`, `ℯ^`, `√`, `³√`, `ˣ√`,
  `!`, nPr/nCr, abs/round/iPart/fPart/int, min/max, gcd/lcm, remainder, rand/randInt,
  lists with element-wise math, `seq`, `sum`, `mean`, `median`, `stdDev`, `cumSum`, `nDeriv`, `fnInt`,
  normal/binomial/Poisson/geometric distributions, test & logic operators.
- **Menus**: MATH, TEST, ANGLE, LIST, DISTR, VARS→Y-VARS, STAT, MEM, CATALOG
  (letter keys jump), ZOOM, CALC, APPS.
- **Graphing**: Y= editor (10 color-coded functions with on/off), WINDOW, ZOOM
  (In/Out, ZDecimal, ZSquare, ZStandard, ZTrig, ZInteger, ZoomFit, ZPrevious/Sto/Rcl),
  FORMAT (coords, grid dot/line, axes, expression), free cursor, TRACE with panning,
  and CALC value / zero / minimum / maximum / intersect / dy/dx / ∫f(x)dx (with shading).
- **TABLE / TBLSET**, **STAT** list editor (L₁–L₆), 1-Var Stats, 2-Var Stats,
  LinReg(ax+b) (an optional third argument stores the fit into a Y-var).
- **MODE**: Normal/Sci/Eng, Float/Fix 0–9, Radian/Degree, Thick/Thin/Dot graphs,
  Answers Auto/Dec/Frac.
- **Python App** (`apps` → Python App): script browser, shell, and editor with
  the soft-key bar. Python runs on a worker thread, so long loops don't freeze the
  UI, and `on` interrupts them. Supported Python: functions, lambdas, if/elif/else,
  for/while (with else), try/except, ints/floats/strings/lists/tuples/dicts, slicing,
  comprehensions, f-strings, `%` and `.format`, `input()`, and the `math`, `random`
  and `time` modules. Scripts are saved to `~/.local/share/ti84ce/python/*.py`.

## Keyboard

| PC key | Calculator |
|---|---|
| digits, letters, `+ - * / ^ ( ) , . =` | type directly (letters are variables; `sin(` etc. become function tokens) |
| Enter / Backspace / Delete / Esc | enter / delete-left / del / clear |
| Arrows, Home/End | arrows, `2nd ←` / `2nd →` |
| F1–F5 | y= window zoom trace graph (Shift = `2nd`, e.g. Shift+F5 = table) |
| F6 F7 F8 F9 F10 | mode, math, apps, stat, vars |
| Ctrl+V | paste (multi-line paste works in the Python editor) |
| Ctrl+Q | quit |

Or click the keys. The `2nd` and `alpha` modifiers work just like on the real calculator.

## Layout

```
core/src/engine/   TI-style tokenizer, parser, evaluator, number formatting
core/src/python/   Python lexer, parser, interpreter, highlighter, worker thread
core/src/calc/     calculator OS: screens, menus, graphing, table, Python app
core/src/keys.rs   keypad layout and labels
desktop/src/       GPUI front end: body, LCD (text + canvas graph), keypad
ffi/               C API (staticlib) exposing the core to native front ends
ios/               SwiftUI front end, XcodeGen project, Rust build script
```

`cargo test --workspace` runs the engine, interpreter, headless calculator and
FFI tests. Set `TI84CE_SCRIPT='2 + 3 enter'` to replay key presses when the
desktop app starts.

`vendor/xattr` patches an old transitive dependency of gpui 0.2.2 that doesn't
build against current `libc`.
