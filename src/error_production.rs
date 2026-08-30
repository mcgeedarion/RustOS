//! Production-Ready Error Handling Framework
//!
//! This module provides comprehensive error handling for the kernel,
//! replacing panic-prone patterns with proper Result propagation.
//!
//! ## Design Goals
//!
//! - No unwrap()/expect() in production code paths
//! - Automatic errno conversion for syscalls
//! - Rich error context for debugging
//! - Zero-cost abstractions where possible
//!
//! ## Usage
//!
//! ```rust
//! use crate::error::prelude::*;
//!
//! fn example_function(ptr: usize) -> KernelResult<i32> {
//!     let value = try_read_user(ptr)?;
//!     Ok(value * 2)
//! }
//! ```

#![no_std]

extern crate alloc;

use core::fmt;
use core::num::TryFromIntError;
use alloc::string::String;

/// Prelude module for convenient imports
pub mod prelude {
    pub use super::{KernelError, KernelResult, ErrorContext};
}

/// Comprehensive kernel error types with errno mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum KernelError {
    /// Success (should not appear in Err variants)
    Success = 0,
    /// Operation not permitted
    PermissionDenied = 1,
    /// No such file or directory
    NotFound = 2,
    /// No such process
    NoSuchProcess = 3,
    /// Interrupted system call
    Interrupted = 4,
    /// I/O error
    Io = 5,
    /// No such device or address
    NoDevice = 6,
    /// Argument list too long
    TooLong = 7,
    /// Exec format error
    BadFormat = 8,
    /// Bad file descriptor
    BadFd = 9,
    /// No child processes
    NoChild = 10,
    /// Resource temporarily unavailable
    WouldBlock = 11,
    /// Out of memory
    OutOfMemory = 12,
    /// Permission denied (alternative)
    AccessDenied = 13,
    /// Bad address
    Fault = 14,
    /// Block device required
    NotBlockDev = 15,
    /// Device or resource busy
    Busy = 16,
    /// File exists
    AlreadyExists = 17,
    /// Cross-device link
    CrossDevice = 18,
    /// No such device
    NoSuchDevice = 19,
    /// Not a directory
    NotADirectory = 20,
    /// Is a directory
    IsADirectory = 21,
    /// Invalid argument
    InvalidArg = 22,
    /// File table overflow
    FileTableOverflow = 23,
    /// Too many open files
    TooManyOpenFiles = 24,
    /// Not a typewriter
    NotATty = 25,
    /// Text file busy
    TextFileBusy = 26,
    /// File too large
    FileTooLarge = 27,
    /// No space left on device
    NoSpace = 28,
    /// Illegal seek
    IllegalSeek = 29,
    /// Read-only file system
    ReadOnly = 30,
    /// Too many links
    TooManyLinks = 31,
    /// Broken pipe
    BrokenPipe = 32,
    /// Numerical argument out of domain
    MathDomain = 33,
    /// Numerical result out of range
    MathRange = 34,
    /// Protocol wrong type for socket
    WrongProtocol = 35,
    /// Destination address required
    AddrRequired = 36,
    /// Message too long
    MsgTooLong = 37,
    /// Protocol not available
    ProtocolNotAvailable = 38,
    /// Protocol not supported
    ProtocolNotSupported = 39,
    /// Socket type not supported
    SocketNotSupported = 40,
    /// Operation not supported
    NotSupported = 95,
    /// Connection timed out
    TimedOut = 110,
    /// Connection refused
    ConnectionRefused = 111,
    /// Host unreachable
    HostUnreachable = 113,
    /// Connection already established
    AlreadyConnected = 114,
    /// Not yet implemented
    NotImplemented = 999,
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KernelError::Success => write!(f, "Success"),
            KernelError::PermissionDenied => write!(f, "Operation not permitted"),
            KernelError::NotFound => write!(f, "No such file or directory"),
            KernelError::NoSuchProcess => write!(f, "No such process"),
            KernelError::Interrupted => write!(f, "Interrupted system call"),
            KernelError::Io => write!(f, "I/O error"),
            KernelError::NoDevice => write!(f, "No such device or address"),
            KernelError::TooLong => write!(f, "Argument list too long"),
            KernelError::BadFormat => write!(f, "Exec format error"),
            KernelError::BadFd => write!(f, "Bad file descriptor"),
            KernelError::NoChild => write!(f, "No child processes"),
            KernelError::WouldBlock => write!(f, "Resource temporarily unavailable"),
            KernelError::OutOfMemory => write!(f, "Out of memory"),
            KernelError::AccessDenied => write!(f, "Permission denied"),
            KernelError::Fault => write!(f, "Bad address"),
            KernelError::NotBlockDev => write!(f, "Block device required"),
            KernelError::Busy => write!(f, "Device or resource busy"),
            KernelError::AlreadyExists => write!(f, "File exists"),
            KernelError::CrossDevice => write!(f, "Cross-device link"),
            KernelError::NoSuchDevice => write!(f, "No such device"),
            KernelError::NotADirectory => write!(f, "Not a directory"),
            KernelError::IsADirectory => write!(f, "Is a directory"),
            KernelError::InvalidArg => write!(f, "Invalid argument"),
            KernelError::FileTableOverflow => write!(f, "File table overflow"),
            KernelError::TooManyOpenFiles => write!(f, "Too many open files"),
            KernelError::NotATty => write!(f, "Not a typewriter"),
            KernelError::TextFileBusy => write!(f, "Text file busy"),
            KernelError::FileTooLarge => write!(f, "File too large"),
            KernelError::NoSpace => write!(f, "No space left on device"),
            KernelError::IllegalSeek => write!(f, "Illegal seek"),
            KernelError::ReadOnly => write!(f, "Read-only file system"),
            KernelError::TooManyLinks => write!(f, "Too many links"),
            KernelError::BrokenPipe => write!(f, "Broken pipe"),
            KernelError::MathDomain => write!(f, "Numerical argument out of domain"),
            KernelError::MathRange => write!(f, "Numerical result out of range"),
            KernelError::WrongProtocol => write!(f, "Protocol wrong type for socket"),
            KernelError::AddrRequired => write!(f, "Destination address required"),
            KernelError::MsgTooLong => write!(f, "Message too long"),
            KernelError::ProtocolNotAvailable => write!(f, "Protocol not available"),
            KernelError::ProtocolNotSupported => write!(f, "Protocol not supported"),
            KernelError::SocketNotSupported => write!(f, "Socket type not supported"),
            KernelError::NotSupported => write!(f, "Operation not supported"),
            KernelError::TimedOut => write!(f, "Connection timed out"),
            KernelError::ConnectionRefused => write!(f, "Connection refused"),
            KernelError::HostUnreachable => write!(f, "Host unreachable"),
            KernelError::AlreadyConnected => write!(f, "Connection already established"),
            KernelError::NotImplemented => write!(f, "Not yet implemented"),
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

/// Trait for adding context to errors
pub trait ErrorContext<T> {
    fn context<C>(self, context: C) -> Result<T, KernelError>
    where
        C: Into<&'static str>;
    
    fn with_context<C, F>(self, f: F) -> Result<T, KernelError>
    where
        C: Into<&'static str>,
        F: FnOnce() -> C;
}

impl<T, E> ErrorContext<T> for Result<T, E>
where
    E: Into<KernelError>,
{
    #[inline]
    fn context<C>(self, _context: C) -> Result<T, KernelError>
    where
        C: Into<&'static str>,
    {
        // In production, we could log the context here
        self.map_err(|e| e.into())
    }
    
    #[inline]
    fn with_context<C, F>(self, _f: F) -> Result<T, KernelError>
    where
        C: Into<&'static str>,
        F: FnOnce() -> C,
    {
        self.map_err(|e| e.into())
    }
}

/// Helper trait for converting various error types to KernelError
pub trait IntoKernelError {
    fn into_kernel_error(self) -> KernelError;
}

impl IntoKernelError for KernelError {
    #[inline]
    fn into_kernel_error(self) -> KernelError {
        self
    }
}

impl IntoKernelError for isize {
    #[inline]
    fn into_kernel_error(self) -> KernelError {
        if self >= 0 {
            KernelError::Io
        } else {
            match -self {
                1 => KernelError::PermissionDenied,
                2 => KernelError::NotFound,
                3 => KernelError::NoSuchProcess,
                4 => KernelError::Interrupted,
                5 => KernelError::Io,
                9 => KernelError::BadFd,
                11 => KernelError::WouldBlock,
                12 => KernelError::OutOfMemory,
                13 => KernelError::AccessDenied,
                14 => KernelError::Fault,
                16 => KernelError::Busy,
                17 => KernelError::AlreadyExists,
                20 => KernelError::NotADirectory,
                21 => KernelError::IsADirectory,
                22 => KernelError::InvalidArg,
                28 => KernelError::NoSpace,
                30 => KernelError::ReadOnly,
                38 => KernelError::NotSupported,
                95 => KernelError::NotSupported,
                110 => KernelError::TimedOut,
                _ => KernelError::Io,
            }
        }
    }
}

/// Check if a syscall result is an error
#[inline]
pub const fn is_syscall_error(result: isize) -> bool {
    result < 0
}

/// Get errno from syscall result
#[inline]
pub fn syscall_errno(result: isize) -> KernelError {
    (-result).into_kernel_error()
}

/// Macro for syscall wrapper that automatically converts errors to negative errno
#[macro_export]
macro_rules! syscall_wrapper_safe {
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

/// Macro for safe unwrapping with error propagation
#[macro_export]
macro_rules! try_opt {
    ($expr:expr, $err:expr) => {
        match $expr {
            Some(val) => val,
            None => return Err($err),
        }
    };
}

/// Macro for safe result conversion
#[macro_export]
macro_rules! try_result {
    ($expr:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => return Err(e.into()),
        }
    };
    ($expr:expr, $ctx:expr) => {
        match $expr {
            Ok(val) => val,
            Err(_) => return Err($ctx.into()),
        }
    };
}

/// Convert TryFromIntError to KernelError
impl From<TryFromIntError> for KernelError {
    fn from(_err: TryFromIntError) -> Self {
        KernelError::InvalidArg
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
        assert_eq!(u32::from(err), 2);
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
    
    #[test]
    fn test_is_syscall_error() {
        assert!(!is_syscall_error(42));
        assert!(!is_syscall_error(0));
        assert!(is_syscall_error(-1));
        assert!(is_syscall_error(-2));
    }
    
    #[test]
    fn test_error_display() {
        assert_eq!(format!("{}", KernelError::NotFound), "No such file or directory");
        assert_eq!(format!("{}", KernelError::OutOfMemory), "Out of memory");
    }
}
