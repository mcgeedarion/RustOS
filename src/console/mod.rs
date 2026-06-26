//! Console subsystem — printk-style output to serial (and optionally VGA).
//!
//! ## Usage
//!
//! ```rust
//! use crate::console;
//! console::print("hello\n");
//! console::println!("world");
//! ```
//!
//! Internally routes to the arch-specific serial driver.

mod console;
pub use console::print;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::console::print(&format_args!($($arg)*).to_string()));
}

#[macro_export]
macro_rules! println {
    () => ($crate::console::print("\n"));
    ($($arg:tt)*) => ($crate::console::print(&format!("{}", format_args!($($arg)*))));
}
