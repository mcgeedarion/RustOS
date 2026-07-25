extern crate alloc;

use alloc::{string::String, vec::Vec};

const MAX_CSTR_LEN: usize = 4096;

/// Copy a NUL-terminated userspace string into a kernel-owned `String`.
///
/// Returns `None` when the pointer is null, unmapped, not NUL-terminated
/// within the bounded scan, or not valid UTF-8.
pub fn read_cstr_safe(addr: usize) -> Option<String> {
    if addr == 0 {
        return None;
    }

    let mut bytes = Vec::new();
    for offset in 0..MAX_CSTR_LEN {
        let mut byte = 0u8;
        if crate::kernel::uaccess::copy_from_user(&mut byte as *mut u8, addr + offset, 1).is_err() {
            return None;
        }
        if byte == 0 {
            return String::from_utf8(bytes).ok();
        }
        bytes.push(byte);
    }

    None
}

/// Minimal execve syscall surface used until full image replacement is wired.
///
/// Bad path pointers must report `EFAULT`; otherwise return `ENOSYS` instead
/// of panicking or pretending exec succeeded.
pub fn sys_execve(path_va: usize, _argv_va: usize, _envp_va: usize) -> isize {
    match read_cstr_safe(path_va) {
        Some(_) => -38, // ENOSYS
        None => -14,    // EFAULT
    }
}

/// Full ELF userspace spawning is not available in this build graph yet.
pub fn spawn_user_process(_path: &str, _argv: &[&str], _envp: &[&str]) -> bool {
    false
}
