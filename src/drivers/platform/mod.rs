//! Platform, bus, and peripheral drivers.
//!
//! ## Modules
//!   gpio   — GPIO stub
//!   pcie   — PCIe ECAM enumeration
//!   tty    — TTY driver shim (real impl in shell/tty.rs)

pub mod gpio;
pub mod pcie;
pub mod tty;
