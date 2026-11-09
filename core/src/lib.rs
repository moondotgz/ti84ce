//! Platform-independent core of the TI-84 Plus CE-T Python Edition clone:
//! the math engine, the Python interpreter and the calculator OS. Front ends
//! (GPUI desktop, SwiftUI iOS) feed it key presses and draw its `ViewModel`.

pub mod calc;
pub mod engine;
pub mod keys;
pub mod python;
