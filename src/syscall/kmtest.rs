//! Kernel-mode test syscall handlers.
//!
//! Only compiled when `feature = "kmtest"` is active.
//! Exposes two syscalls to userspace:
//!
//! | NR              | a0            | a1            | Returns |
//! |-----------------|---------------|---------------|---------|
//! | SYS_KMTEST_LIST | buf_ptr / 0   | buf_len / 0   | test count (≥ 0) or -EFAULT |
//! | SYS_KMTEST_RUN  | index / `!0`  | —             | 0 = pass, 1 = fail, -EINVAL = bad index; `!0` runs all |
//!
//! Results for every test are streamed to the serial console as lines:
//!
//! ```text
//! KMTEST  PASS  mm_map_unmap
//! KMTEST  FAIL  fs_write_read_roundtrip — readback mismatch
//! KMTEST  DONE  2/2 passed
//! ```
//!
//! This format is designed to be trivially grep-able by the QEMU runner
//! script: `grep '^KMTEST' serial.log`.

use crate::syscall::errno::{efault, einval};

/// Handler for `SYS_KMTEST_LIST`.
pub fn sys_kmtest_list(buf_ptr: usize, buf_len: usize) -> isize {
    let tests = crate::kmtest::registry_snapshot();
    if buf_ptr == 0 {
        return tests.len() as isize;
    }

    // Copy names into the user buffer as NUL-terminated strings.  Never
    // form a direct kernel slice over userspace memory; each fragment goes
    // through uaccess so a bad pointer returns EFAULT instead of faulting
    // in kernel context.
    let mut pos = 0usize;
    let mut written = 0isize;
    for entry in tests {
        let name = entry.name.as_bytes();
        let needed = name.len() + 1; // +1 for NUL
        if pos + needed > buf_len {
            break;
        }
        if crate::uaccess::copy_to_user(buf_ptr + pos, name).is_err() {
            return efault();
        }
        if crate::uaccess::copy_to_user(buf_ptr + pos + name.len(), &[0]).is_err() {
            return efault();
        }
        pos += needed;
        written += 1;
    }
    written
}

/// Handler for `SYS_KMTEST_RUN`.
pub fn sys_kmtest_run(index: usize) -> isize {
    let tests = crate::kmtest::registry_snapshot();
    if index == usize::MAX {
        run_range(&tests, 0, tests.len())
    } else {
        if index >= tests.len() {
            return einval();
        }
        run_range(&tests, index, index + 1)
    }
}

// Run tests[start..end], print results to serial, return failure count as
// isize.
fn run_range(tests: &[crate::kmtest::KmTestEntry], start: usize, end: usize) -> isize {
    let mut failures = 0usize;
    let mut total = 0usize;

    for entry in &tests[start..end] {
        let result = (entry.run)();
        total += 1;
        match result {
            Ok(()) => {
                crate::serial_println!("KMTEST  PASS  {}", entry.name);
            },
            Err(msg) => {
                crate::serial_println!("KMTEST  FAIL  {} \u{2014} {}", entry.name, msg);
                failures += 1;
            },
        }
    }

    let passed = total - failures;
    crate::serial_println!("KMTEST  DONE  {}/{} passed", passed, total);

    failures as isize
}
