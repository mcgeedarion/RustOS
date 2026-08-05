//! Syscall-path fault injection integration.
//!
//! Call [`check_resource`] at the top of any syscall handler that touches
//! kernel resources (memory allocation, descriptor tables, etc.).
//! Call [`check_fdtable`] inside the file-descriptor allocation path.
//!
//! # Integration example
//!
//! ```rust,ignore
//! pub fn sys_open(path: &str, flags: u32) -> Result<Fd, SyscallError> {
//!     #[cfg(feature = "fault-inject")]
//!     crate::syscall::fault::check_resource()?;
//!
//!     // ... real open logic ...
//!
//!     let fd = fdtable.insert(file_obj).map_err(|_| {
//!         #[cfg(feature = "fault-inject")]
//!         { let _ = crate::syscall::fault::check_fdtable(); }
//!         SyscallError::TooManyFiles
//!     })?;
//!     Ok(fd)
//! }
//! ```

#![cfg(feature = "fault-inject")]

use crate::fault_inject::points::{FAULT_SYSCALL_FDTABLE, FAULT_SYSCALL_RESOURCE};

/// Errno codes used by the fault injection layer.
///
/// These mirror POSIX values; adjust to match your kernel's `SyscallError`
/// / `Errno` type as needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultErrno {
    /// Generic resource exhaustion (`EAGAIN` / `ENOMEM`).
    ResourceExhausted,
    /// Per-process file-descriptor limit (`EMFILE`).
    TooManyFiles,
}

impl core::fmt::Display for FaultErrno {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ResourceExhausted => write!(f, "EAGAIN (injected resource exhaustion)"),
            Self::TooManyFiles => write!(f, "EMFILE (injected fd-table exhaustion)"),
        }
    }
}

/// Check the generic resource-exhaustion fault point.
///
/// Returns `Err(FaultErrno::ResourceExhausted)` if the fault is armed and
/// should fire, `Ok(())` otherwise.
#[inline(always)]
pub fn check_resource() -> Result<(), FaultErrno> {
    if FAULT_SYSCALL_RESOURCE.check() {
        log::warn!("[fault-inject] FAULT_SYSCALL_RESOURCE fired — injecting EAGAIN");
        Err(FaultErrno::ResourceExhausted)
    } else {
        Ok(())
    }
}

/// Check the fd-table-insertion fault point.
///
/// Returns `Err(FaultErrno::TooManyFiles)` when the fault fires.
#[inline(always)]
pub fn check_fdtable() -> Result<(), FaultErrno> {
    if FAULT_SYSCALL_FDTABLE.check() {
        log::warn!("[fault-inject] FAULT_SYSCALL_FDTABLE fired — injecting EMFILE");
        Err(FaultErrno::TooManyFiles)
    } else {
        Ok(())
    }
}
