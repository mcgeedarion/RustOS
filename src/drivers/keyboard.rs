//! Compatibility keyboard facade.
//!
//! The active keyboard driver lives at `drivers::input::keyboard` and exposes a
//! blocking byte-oriented API.  Older TTY code expects a non-blocking
//! `drivers::keyboard::read_char() -> Option<char>` poll hook, so provide a
//! no-key facade until the input line-discipline path is reconciled.

pub fn read_char() -> Option<char> {
    None
}
