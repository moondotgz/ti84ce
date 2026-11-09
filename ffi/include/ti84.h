// C interface to the TI-84 Plus CE-T calculator core (Rust, crate ti84-ffi).
// All functions must be called from one thread (the UI thread).
#ifndef TI84_H
#define TI84_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct Ti84 Ti84;

/// Create a calculator. `python_dir` is where Python scripts are stored
/// (UTF-8 path, created if missing); pass NULL to keep them in memory.
Ti84 *ti84_new(const char *python_dir);
void ti84_free(Ti84 *calc);

/// Press a keypad key by its code (see the "code" fields of ti84_keypad_json).
void ti84_press(Ti84 *calc, uint32_t key_code);
/// Type UTF-8 text from a hardware keyboard.
void ti84_type(Ti84 *calc, const char *utf8);
/// Paste UTF-8 text (multi-line pastes work in the Python editor).
void ti84_paste(Ti84 *calc, const char *utf8);
/// Hardware-keyboard backspace (delete left of the cursor).
void ti84_backspace(Ti84 *calc);

/// Poll background work (Python output). Call ~20x per second; returns true
/// when the screen changed and should be redrawn.
bool ti84_tick(Ti84 *calc);
/// Show or hide the blinking cursor (toggle every 0.5 s).
void ti84_set_blink(Ti84 *calc, bool on);

/// The current screen as JSON. Free the result with ti84_string_free.
char *ti84_view_json(Ti84 *calc);
/// The keypad layout as JSON. Free the result with ti84_string_free.
char *ti84_keypad_json(void);
void ti84_string_free(char *s);

#ifdef __cplusplus
}
#endif

#endif
