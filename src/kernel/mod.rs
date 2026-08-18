//! Core kernel utilities with no single subsystem home.
//!
//! ## Modules
//!
//!   `panic`   — Kernel panic handler: prints a backtrace and halts all CPUs.
//!   `rand`    — Hardware-seeded PRNG with xorshift; used by ASLR and
//!               stack-canary generation in `crate::security`.
//!   `uaccess` — Safe helpers for copying data between kernel and user space
//!               (`copy_from_user`, `copy_to_user`, `get_user`, `put_user`).
//!   `utils`   — Alignment, bit manipulation, and power-of-two utilities.
//!   `architecture` — Hybrid-kernel architecture contract and diagnostics.
//!
//! ## See also
//! - [`crate::arch`] for architecture-specific implementations
//! - [`crate::mm`] for memory management

// Internal macro to reduce feature flag repetition
macro_rules! full_kernel_only {
    ($($item:item)*) => { $($item)* };
}

#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod architecture;
pub mod panic;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod rand;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod uaccess;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod utils;

// Re-export commonly used types at module level
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub use uaccess::{UaccessError, UaccessResult, UserPtr, UserSlice};
pub use panic::set_fault_addr;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub use architecture::{KernelArchitecture, HYBRID_KERNEL_CONTRACT, KERNEL_ARCHITECTURE};
