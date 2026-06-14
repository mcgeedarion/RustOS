//! Core kernel utilities with no single subsystem home.
//!
//! ## Modules
//!
//!   `panic`   — Kernel panic handler: prints a backtrace and halts all CPUs.
//!   `rand`    — Cryptographically-seeded PRNG (ChaCha20); used by ASLR and
//!               stack-canary generation in `crate::security`.
//!   `uaccess` — Safe helpers for copying data between kernel and user space
//!               (`copy_from_user`, `copy_to_user`, `get_user`, `put_user`).
//!   `utils`   — Small cross-cutting helpers (alignment, bit ops, etc.).
//!   `architecture` — Hybrid-kernel architecture contract and diagnostics.

#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod architecture;
pub mod panic;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod rand;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod uaccess;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod utils;
