//! `core` — the zero-dependency kernel foundation.
//!
//! Every other subsystem **may** import from here; nothing here imports
//! from any other kernel subsystem.  Keep it that way.
//!
//! Sub-modules
//! -----------
//! * [`cfg_aliases`] — centralized cfg aliases for cleaner conditional compilation
//! * [`collections`] — intrusive linked-list and ring-buffer
//! * [`cpu_local`]   — per-CPU variable accessor
//! * [`error`]       — [`KernelError`] enum and [`KResult`] alias
//! * [`fast_hash`]   — fast maps for trusted kernel-internal keys
//! * [`panic`]       — kernel panic handler

#![allow(dead_code)]

pub mod cfg_aliases;
pub mod collections;
pub mod cpu_local;
pub mod error;
pub mod fast_hash;
pub mod panic;

pub use error::{KResult, KernelError};
