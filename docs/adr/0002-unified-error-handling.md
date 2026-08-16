# ADR 0002: Unified Error Handling

## Status
Accepted

## Context
Error handling across RustOS was inconsistent:
- Different subsystems used different error types
- Syscall return values mixed positive values and negative errno
- No automatic conversion between error types and errno
- Difficult to trace error sources across module boundaries

## Decision
Implement a unified error handling system with:

### Core Components
1. **KernelError enum**: Standardized error type with errno mapping
2. **Automatic conversions**: From<KernelError> for isize/i32/u32
3. **Syscall wrapper macros**: Automatic errno conversion
4. **Result type aliases**: KernelResult<T>, SyscallResult<T>

### Error Type Design
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelError {
    NotFound = 2,        // ENOENT
    Io = 5,              // EIO
    BadFd = 9,           // EBADF
    OutOfMemory = 12,    // ENOMEM
    PermissionDenied = 13, // EACCES
    InvalidArg = 22,     // EINVAL
    // ... etc
}
```

### Usage Pattern
```rust
// Before
fn open_file(path: &str) -> Result<File, i32> {
    if !exists(path) { return Err(-2); }
    // ...
}

// After  
fn open_file(path: &str) -> KernelResult<File> {
    if !exists(path) { return Err(KernelError::NotFound); }
    // ...
}

// Syscall wrapper
#[no_mangle]
pub fn sys_open(path: *const u8) -> isize {
    syscall_wrapper!(open_file(unsafe { read_string(path) }))
}
```

## Consequences
### Positive
- Consistent error handling across all subsystems
- Automatic errno conversion reduces boilerplate
- Better error tracing and debugging
- Type-safe error propagation

### Negative
- Migration effort for existing code
- Some performance overhead from enum conversions

## Implementation
See `src/error.rs` for the complete implementation.
