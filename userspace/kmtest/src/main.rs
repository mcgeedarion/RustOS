//! rustos-kmtest — Userspace runner for the RustOS kernel test harness
//!
//! Usage:
//!   kmtest                  -- run all kernel tests
//!   kmtest <name> [name...] -- run only the named tests
//!
//! Exit status:
//!   0   all tests passed
//!   1   one or more tests failed
//!   2   usage / syscall error
//!
//! The runner communicates with the kernel via two private syscalls:
//!
//!   SYS_KMTEST_LIST (0x80000000)
//!     a0 = 0, a1 = 0   -> returns total test count
//!     a0 = buf, a1 = len -> fills buf with NUL-terminated names
//!
//!   SYS_KMTEST_RUN (0x80000001)
//!     a0 = index        -> run one test by index; returns 0=pass, 1=fail
//!     a0 = ~0UL         -> run all tests; returns failure count

#![no_std]
#![no_main]

use core::arch::asm;
use core::ffi::{c_char, c_long};

// ── Private syscall numbers ────────────────────────────────────────────────

const SYS_KMTEST_LIST: usize = 0x80000000;
const SYS_KMTEST_RUN: usize = 0x80000001;

/// Run all tests when passed as the index argument to SYS_KMTEST_RUN.
const KMTEST_RUN_ALL: usize = !0usize;

/// Maximum number of tests we support listing.
const MAX_TESTS: usize = 512;
/// Maximum length of a single test name (NUL-terminated).
const NAME_MAX_LEN: usize = 128;
/// Total name buffer: worst case all tests have 127-char names.
const NAME_BUF_SIZE: usize = MAX_TESTS * NAME_MAX_LEN;

// ── Raw syscall wrappers ───────────────────────────────────────────────────

#[inline(always)]
unsafe fn sys_kmtest_list(buf: *mut u8, len: usize) -> c_long {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: c_long;
        asm!(
            "syscall",
            in("rax") SYS_KMTEST_LIST,
            in("rdi") buf,
            in("rsi") len,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        let ret: c_long;
        asm!(
            "ecall",
            in("a7") SYS_KMTEST_LIST,
            in("a0") buf,
            in("a1") len,
            lateout("a0") ret,
            options(nostack),
        );
        ret
    }
}

#[inline(always)]
unsafe fn sys_kmtest_run(idx: usize) -> c_long {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: c_long;
        asm!(
            "syscall",
            in("rax") SYS_KMTEST_RUN,
            in("rdi") idx,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        let ret: c_long;
        asm!(
            "ecall",
            in("a7") SYS_KMTEST_RUN,
            in("a0") idx,
            lateout("a0") ret,
            options(nostack),
        );
        ret
    }
}

#[inline(always)]
unsafe fn sys_exit_group(status: usize) -> ! {
    #[cfg(target_arch = "x86_64")]
    const SYS_EXIT_GROUP: usize = 231;
    #[cfg(target_arch = "aarch64")]
    const SYS_EXIT_GROUP: usize = 94;
    #[cfg(target_arch = "riscv64")]
    const SYS_EXIT_GROUP: usize = 94;

    #[cfg(target_arch = "x86_64")]
    asm!(
        "syscall",
        in("rax") SYS_EXIT_GROUP,
        in("rdi") status,
        options(noreturn),
    );
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    asm!(
        "ecall",
        in("a7") SYS_EXIT_GROUP,
        in("a0") status,
        options(noreturn),
    );
}

#[inline(always)]
unsafe fn sys_write(fd: usize, buf: *const u8, count: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    const SYS_WRITE: usize = 1;
    #[cfg(target_arch = "aarch64")]
    const SYS_WRITE: usize = 64;
    #[cfg(target_arch = "riscv64")]
    const SYS_WRITE: usize = 64;

    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        asm!(
            "syscall",
            in("rax") SYS_WRITE,
            in("rdi") fd,
            in("rsi") buf,
            in("rdx") count,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        let ret: isize;
        asm!(
            "ecall",
            in("a7") SYS_WRITE,
            in("a0") fd,
            in("a1") buf,
            in("a2") count,
            lateout("a0") ret,
            options(nostack),
        );
        ret
    }
}

// ── Helper functions ───────────────────────────────────────────────────────

fn write_all(fd: usize, msg: &[u8]) {
    let mut written = 0;
    while written < msg.len() {
        let n = unsafe { sys_write(fd, msg[written..].as_ptr(), msg.len() - written) };
        if n <= 0 {
            unsafe { sys_exit_group(1) };
        }
        written += n as usize;
    }
}

fn write_str(fd: usize, s: &str) {
    write_all(fd, s.as_bytes());
}

fn write_line(fd: usize, s: &str) {
    write_str(fd, s);
    write_str(fd, "\n");
}

fn fmt_ul(buf: &mut [u8; 24], mut n: usize) -> &str {
    let mut i = buf.len();
    if n == 0 {
        i -= 1;
        buf[i] = b'0';
    } else {
        while n > 0 {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("0")
}

// ── Core logic ─────────────────────────────────────────────────────────────

/// Query the kernel for how many tests exist and fill names[] with their
/// NUL-terminated name strings. Returns the test count, or -1 on error.
fn list_tests(names: &mut [[u8; NAME_MAX_LEN]]) -> Result<usize, &'static str> {
    // Step 1: get the count.
    let count = unsafe { sys_kmtest_list(core::ptr::null_mut(), 0) };
    if count < 0 {
        write_line(2, "kmtest: SYS_KMTEST_LIST failed");
        return Err("SYS_KMTEST_LIST failed");
    }
    if count == 0 {
        write_line(2, "kmtest: no tests registered in kernel");
        return Ok(0);
    }
    let count = core::cmp::min(count as usize, MAX_TESTS);

    // Step 2: fetch names into a flat byte buffer.
    static mut RAW: [u8; NAME_BUF_SIZE] = [0u8; NAME_BUF_SIZE];
    let written = unsafe { sys_kmtest_list(RAW.as_mut_ptr(), RAW.len()) };
    if written < 0 {
        write_line(2, "kmtest: SYS_KMTEST_LIST (names) failed");
        return Err("SYS_KMTEST_LIST (names) failed");
    }

    // Parse NUL-terminated strings out of the flat buffer.
    let raw = unsafe { &RAW[..written as usize] };
    let mut p = 0;
    let mut i = 0;
    while i < count && p < raw.len() {
        // Find the NUL terminator
        let start = p;
        while p < raw.len() && raw[p] != 0 {
            p += 1;
        }
        let len = core::cmp::min(p - start, NAME_MAX_LEN - 1);
        names[i][..len].copy_from_slice(&raw[start..start + len]);
        names[i][len] = 0; // Ensure NUL termination
        i += 1;
        if p < raw.len() {
            p += 1; // Skip the NUL
        }
    }
    Ok(i)
}

/// Find the index of a test by name. Returns None if not found.
fn find_test(names: &[[u8; NAME_MAX_LEN]], count: usize, target: &str) -> Option<usize> {
    for i in 0..count {
        let name_len = names[i].iter().position(|&b| b == 0).unwrap_or(NAME_MAX_LEN);
        if let Ok(name_str) = core::str::from_utf8(&names[i][..name_len]) {
            if name_str == target {
                return Some(i);
            }
        }
    }
    None
}

/// Run a single test by index. Prints one PASS/FAIL line to stdout.
/// Returns 0 on pass, 1 on fail.
fn run_one(idx: usize, name: &[u8]) -> i32 {
    let ret = unsafe { sys_kmtest_run(idx) };
    
    let name_str = core::str::from_utf8(name).unwrap_or("<invalid utf8>");
    if ret == 0 {
        write_str(1, "  PASS  ");
        write_line(1, name_str);
        0
    } else {
        write_str(1, "  FAIL  ");
        write_line(1, name_str);
        1
    }
}

// ── Entry point ────────────────────────────────────────────────────────────

fn kmtest_main() -> ! {
    static mut NAMES: [[u8; NAME_MAX_LEN]; MAX_TESTS] = [[0u8; NAME_MAX_LEN]; MAX_TESTS];
    let names = unsafe { &mut NAMES };

    let count = match list_tests(names) {
        Ok(c) => c,
        Err(_) => unsafe { sys_exit_group(2) },
    };

    if count == 0 {
        unsafe { sys_exit_group(0) };
    }

    // Print header.
    let mut numbuf = [0u8; 24];
    write_str(1, "kmtest: ");
    write_str(1, fmt_ul(&mut numbuf, count));
    write_line(1, " test(s) registered");
    write_line(1, "----");

    let mut failures: i32 = 0;
    let mut ran: i32 = 0;

    // For now, always run all tests (argv parsing omitted for no_std simplicity)
    for i in 0..count {
        let name_len = names[i].iter().position(|&b| b == 0).unwrap_or(NAME_MAX_LEN);
        failures += run_one(i, &names[i][..name_len]);
        ran += 1;
    }

    // Summary line.
    write_line(1, "----");
    let passed = ran - failures;
    let mut buf = [0u8; 64];
    let passed_str = fmt_ul(&mut buf, passed as usize);
    let ran_str = fmt_ul(&mut buf, ran as usize);
    write_str(1, "kmtest: ");
    write_str(1, passed_str);
    write_str(1, "/");
    write_str(1, ran_str);
    write_line(1, " passed");

    if failures > 0 {
        unsafe { sys_exit_group(1) };
    } else {
        unsafe { sys_exit_group(0) };
    }
}

#[no_mangle]
#[unsafe(naked)]
unsafe extern "C" fn _start() -> ! {
    #[cfg(target_arch = "x86_64")]
    {
        asm!(
            "and rsp, -16",
            "xor rbp, rbp",
            "call {entry}",
            entry = sym kmtest_main,
            options(noreturn),
        )
    }
    #[cfg(target_arch = "aarch64")]
    {
        asm!(
            "mov x29, #0",
            "and sp, sp, #-16",
            "bl {entry}",
            entry = sym kmtest_main,
            options(noreturn),
        )
    }
    #[cfg(target_arch = "riscv64")]
    {
        asm!(
            "mv s0, zero",
            "and sp, sp, -16",
            "call {entry}",
            entry = sym kmtest_main,
            options(noreturn),
        )
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { sys_exit_group(101) }
}
