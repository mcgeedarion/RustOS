//! Compatibility stubs for Linux memory protection key syscalls.
//!
//! RustOS does not yet implement per-thread protection keys.  Keep the syscall
//! router linkable and return `ENOSYS` until the VM subsystem grows pkey state.

const ENOSYS: isize = -38;

pub fn sys_pkey_alloc(_flags: u32, _access_rights: u32) -> isize {
    ENOSYS
}

pub fn sys_pkey_free(_pkey: u32) -> isize {
    ENOSYS
}

pub fn sys_pkey_mprotect(_addr: usize, _len: usize, _prot: i32, _pkey: i32) -> isize {
    ENOSYS
}
