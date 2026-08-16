//! Unified Error Handling for RustOS Kernel
//!
//! This module provides standardized error types with automatic errno conversion
//! and syscall wrapper macros for consistent error handling across the kernel.

#![no_std]

extern crate alloc;

use core::fmt;

/// Standardized kernel error type with errno mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelError {
    /// No such file or directory
    NotFound = 2,
    /// I/O error
    Io = 5,
    /// Bad file descriptor
    BadFd = 9,
    /// Resource temporarily unavailable
    WouldBlock = 11,
    /// Out of memory
    OutOfMemory = 12,
    /// Permission denied
    PermissionDenied = 13,
    /// Invalid argument
    InvalidArg = 22,
    /// File exists
    AlreadyExists = 17,
    /// Not a directory
    NotADirectory = 20,
    /// Is a directory
    IsADirectory = 21,
    /// Read-only file system
    ReadOnly = 30,
    /// Operation not supported
    NotSupported = 95,
    /// Connection timed out
    TimedOut = 110,
    /// Interrupted system call
    Interrupted = 4,
    /// Resource busy
    Busy = 16,
    /// Too many open files
    TooManyOpenFiles = 24,
    /// No space left on device
    NoSpace = 28,
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KernelError::NotFound => write!(f, "No such file or directory"),
            KernelError::Io => write!(f, "I/O error"),
            KernelError::BadFd => write!(f, "Bad file descriptor"),
            KernelError::WouldBlock => write!(f, "Resource temporarily unavailable"),
            KernelError::OutOfMemory => write!(f, "Out of memory"),
            KernelError::PermissionDenied => write!(f, "Permission denied"),
            KernelError::InvalidArg => write!(f, "Invalid argument"),
            KernelError::AlreadyExists => write!(f, "File exists"),
            KernelError::NotADirectory => write!(f, "Not a directory"),
            KernelError::IsADirectory => write!(f, "Is a directory"),
            KernelError::ReadOnly => write!(f, "Read-only file system"),
            KernelError::NotSupported => write!(f, "Operation not supported"),
            KernelError::TimedOut => write!(f, "Connection timed out"),
            KernelError::Interrupted => write!(f, "Interrupted system call"),
            KernelError::Busy => write!(f, "Resource busy"),
            KernelError::TooManyOpenFiles => write!(f, "Too many open files"),
            KernelError::NoSpace => write!(f, "No space left on device"),
        }
    }
}

impl From<KernelError> for isize {
    fn from(err: KernelError) -> Self {
        -(err as isize)
    }
}

impl From<KernelError> for i32 {
    fn from(err: KernelError) -> Self {
        err as i32
    }
}

impl From<KernelError> for u32 {
    fn from(err: KernelError) -> Self {
        err as u32
    }
}

/// Result type alias for kernel operations
pub type KernelResult<T> = Result<T, KernelError>;

/// Syscall result - returns either value or negative errno
pub type SyscallResult<T> = Result<T, isize>;

/// Convert any error type to syscall result
pub fn to_syscall_result<T, E>(result: Result<T, E>) -> SyscallResult<T>
where
    E: Into<KernelError>,
{
    result.map_err(|e| -(e.into() as isize))
}

/// Macro for syscall wrapper that automatically converts errors to negative errno
#[macro_export]
macro_rules! syscall_wrapper {
    ($expr:expr) => {{
        match $expr {
            Ok(val) => val as isize,
            Err(e) => {
                let errno: $crate::error::KernelError = e.into();
                -(errno as isize)
            }
        }
    }};
}

/// Macro for converting Result to syscall convention
#[macro_export]
macro_rules! into_syscall {
    ($result:expr) => {{
        match $result {
            Ok(v) => v as isize,
            Err(e) => {
                use $crate::error::IntoKernelError;
                -(e.into_errno() as isize)
            }
        }
    }};
}

/// Trait for converting errors to errno
pub trait IntoKernelError {
    fn into_errno(self) -> KernelError;
}

impl IntoKernelError for KernelError {
    fn into_errno(self) -> KernelError {
        self
    }
}

impl IntoKernelError for isize {
    fn into_errno(self) -> KernelError {
        match self {
            -2 => KernelError::NotFound,
            -5 => KernelError::Io,
            -9 => KernelError::BadFd,
            -11 => KernelError::WouldBlock,
            -12 => KernelError::OutOfMemory,
            -13 => KernelError::PermissionDenied,
            -22 => KernelError::InvalidArg,
            _ => KernelError::Io,
        }
    }
}

/// Helper function to check if a syscall result is an error
pub const fn is_syscall_error(result: isize) -> bool {
    result < 0
}

/// Get errno from syscall result
pub const fn syscall_errno(result: isize) -> KernelError {
    if result >= 0 {
        KernelError::Io // Should not happen
    } else {
        match result {
            -2 => KernelError::NotFound,
            -5 => KernelError::Io,
            -9 => KernelError::BadFd,
            -11 => KernelError::WouldBlock,
            -12 => KernelError::OutOfMemory,
            -13 => KernelError::PermissionDenied,
            -17 => KernelError::AlreadyExists,
            -20 => KernelError::NotADirectory,
            -21 => KernelError::IsADirectory,
            -22 => KernelError::InvalidArg,
            -30 => KernelError::ReadOnly,
            -95 => KernelError::NotSupported,
            _ => KernelError::Io,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_conversion() {
        let err = KernelError::NotFound;
        assert_eq!(isize::from(err), -2);
        assert_eq!(i32::from(err), 2);
    }
    
    #[test]
    fn test_syscall_result() {
        let ok_result: KernelResult<i32> = Ok(42);
        let syscall_ok = to_syscall_result(ok_result);
        assert_eq!(syscall_ok, Ok(42));
        
        let err_result: KernelResult<i32> = Err(KernelError::NotFound);
        let syscall_err = to_syscall_result(err_result);
        assert_eq!(syscall_err, Err(-2));
    }
}
