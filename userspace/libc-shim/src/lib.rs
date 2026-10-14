//! rustos-libc-shim — Minimal libc shim for RustOS userspace
//!
//! This crate provides `extern "C"` FFI wrappers around raw syscalls,
//! allowing Rust programs to use familiar POSIX-like function names
//! while remaining fully #![no_std].
//!
//! Compile with:
//!   cargo build --release --target x86_64-unknown-none

#![no_std]

use core::ffi::{c_char, c_int, c_long, c_uint, c_void, size_t};

// ── Syscall numbers ────────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
const SYS_READ: c_long = 0;
#[cfg(target_arch = "x86_64")]
const SYS_WRITE: c_long = 1;
#[cfg(target_arch = "x86_64")]
const SYS_OPEN: c_long = 2;
#[cfg(target_arch = "x86_64")]
const SYS_CLOSE: c_long = 3;
#[cfg(target_arch = "x86_64")]
const SYS_EXIT: c_long = 60;
#[cfg(target_arch = "x86_64")]
const SYS_FORK: c_long = 57;
#[cfg(target_arch = "x86_64")]
const SYS_EXECVE: c_long = 59;
#[cfg(target_arch = "x86_64")]
const SYS_WAIT4: c_long = 61;
#[cfg(target_arch = "x86_64")]
const SYS_NANOSLEEP: c_long = 35;
#[cfg(target_arch = "x86_64")]
const SYS_SCHED_YIELD: c_long = 24;
#[cfg(target_arch = "x86_64")]
const SYS_GETCWD: c_long = 79;
#[cfg(target_arch = "x86_64")]
const SYS_CHDIR: c_long = 80;

#[cfg(target_arch = "aarch64")]
const SYS_READ: c_long = 63;
#[cfg(target_arch = "aarch64")]
const SYS_WRITE: c_long = 64;
#[cfg(target_arch = "aarch64")]
const SYS_OPENAT: c_long = 56;
#[cfg(target_arch = "aarch64")]
const SYS_CLOSE: c_long = 57;
#[cfg(target_arch = "aarch64")]
const SYS_EXIT: c_long = 93;
#[cfg(target_arch = "aarch64")]
const SYS_FORK: c_long = -1; // ENOSYS
#[cfg(target_arch = "aarch64")]
const SYS_EXECVE: c_long = 221;
#[cfg(target_arch = "aarch64")]
const SYS_WAIT4: c_long = 260;
#[cfg(target_arch = "aarch64")]
const SYS_NANOSLEEP: c_long = 212;
#[cfg(target_arch = "aarch64")]
const SYS_SCHED_YIELD: c_long = 24;
#[cfg(target_arch = "aarch64")]
const SYS_GETCWD: c_long = 17;
#[cfg(target_arch = "aarch64")]
const SYS_CHDIR: c_long = 49;

#[cfg(target_arch = "riscv64")]
const SYS_READ: c_long = 63;
#[cfg(target_arch = "riscv64")]
const SYS_WRITE: c_long = 64;
#[cfg(target_arch = "riscv64")]
const SYS_OPENAT: c_long = 56;
#[cfg(target_arch = "riscv64")]
const SYS_CLOSE: c_long = 57;
#[cfg(target_arch = "riscv64")]
const SYS_EXIT: c_long = 93;
#[cfg(target_arch = "riscv64")]
const SYS_FORK: c_long = 220;
#[cfg(target_arch = "riscv64")]
const SYS_EXECVE: c_long = 221;
#[cfg(target_arch = "riscv64")]
const SYS_WAIT4: c_long = 260;
#[cfg(target_arch = "riscv64")]
const SYS_NANOSLEEP: c_long = 212;
#[cfg(target_arch = "riscv64")]
const SYS_SCHED_YIELD: c_long = 24;
#[cfg(target_arch = "riscv64")]
const SYS_GETCWD: c_long = 17;
#[cfg(target_arch = "riscv64")]
const SYS_CHDIR: c_long = 49;

// ── Raw syscall ────────────────────────────────────────────────────────────

#[inline(always)]
unsafe fn shim_syscall(
    nr: c_long,
    a1: c_long,
    a2: c_long,
    a3: c_long,
    a4: c_long,
    a5: c_long,
    a6: c_long,
) -> c_long {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: c_long;
        core::arch::asm!(
            "syscall",
            in("rax") nr,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            in("r10") a4,
            in("r8") a5,
            in("r9") a6,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: c_long;
        core::arch::asm!(
            "svc #0",
            in("x8") nr,
            in("x0") a1,
            in("x1") a2,
            in("x2") a3,
            in("x3") a4,
            in("x4") a5,
            in("x5") a6,
            lateout("x0") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: c_long;
        core::arch::asm!(
            "ecall",
            in("a7") nr,
            in("a0") a1,
            in("a1") a2,
            in("a2") a3,
            in("a3") a4,
            in("a4") a5,
            in("a5") a6,
            lateout("a0") ret,
            options(nostack),
        );
        ret
    }
}

// ── POSIX-compatible wrappers ──────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn write(fd: c_int, buf: *const c_void, n: size_t) -> isize {
    unsafe { shim_syscall(SYS_WRITE, fd as c_long, buf as c_long, n as c_long, 0, 0, 0) }
}

#[no_mangle]
pub extern "C" fn read(fd: c_int, buf: *mut c_void, n: size_t) -> isize {
    unsafe { shim_syscall(SYS_READ, fd as c_long, buf as c_long, n as c_long, 0, 0, 0) }
}

#[no_mangle]
pub extern "C" fn open(path: *const c_char, flags: c_int, mode: c_uint) -> c_int {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe { shim_syscall(SYS_OPEN, path as c_long, flags as c_long, mode as c_long, 0, 0, 0) as c_int }
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        // Use openat with AT_FDCWD (-100)
        unsafe { shim_syscall(SYS_OPENAT, -100, path as c_long, flags as c_long, mode as c_long, 0, 0) as c_int }
    }
}

#[no_mangle]
pub extern "C" fn close(fd: c_int) -> c_int {
    unsafe { shim_syscall(SYS_CLOSE, fd as c_long, 0, 0, 0, 0, 0) as c_int }
}

#[no_mangle]
pub extern "C" fn exit(code: c_int) -> ! {
    unsafe {
        shim_syscall(SYS_EXIT, code as c_long, 0, 0, 0, 0, 0);
        core::hint::unreachable_unchecked();
    }
}

#[no_mangle]
pub extern "C" fn fork() -> c_int {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe { shim_syscall(SYS_FORK, 0, 0, 0, 0, 0, 0) as c_int }
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        -1 // ENOSYS on architectures where fork is not implemented
    }
}

#[no_mangle]
pub extern "C" fn execve(path: *const c_char, argv: *const *mut c_char, envp: *const *mut c_char) -> c_int {
    unsafe { shim_syscall(SYS_EXECVE, path as c_long, argv as c_long, envp as c_long, 0, 0, 0) as c_int }
}

/// waitpid — implemented via wait4(pid, status, options, NULL).
#[no_mangle]
pub extern "C" fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int {
    unsafe { shim_syscall(SYS_WAIT4, pid as c_long, status as c_long, options as c_long, 0, 0, 0) as c_int }
}

#[no_mangle]
pub extern "C" fn sched_yield() -> c_int {
    unsafe { shim_syscall(SYS_SCHED_YIELD, 0, 0, 0, 0, 0, 0) as c_int }
}

#[no_mangle]
pub extern "C" fn getcwd(buf: *mut c_char, size: size_t) -> *mut c_char {
    let r = unsafe { shim_syscall(SYS_GETCWD, buf as c_long, size as c_long, 0, 0, 0, 0) };
    if r < 0 {
        core::ptr::null_mut()
    } else {
        buf
    }
}

#[no_mangle]
pub extern "C" fn chdir(path: *const c_char) -> c_int {
    unsafe { shim_syscall(SYS_CHDIR, path as c_long, 0, 0, 0, 0, 0) as c_int }
}

// ── String primitives ──────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn strlen(s: *const c_char) -> size_t {
    let mut p = s;
    unsafe {
        while *p != 0 {
            p = p.add(1);
        }
        p.offset_from(s) as size_t
    }
}

#[no_mangle]
pub extern "C" fn strcmp(a: *const c_char, b: *const c_char) -> c_int {
    unsafe {
        let mut i = 0;
        loop {
            let ai = *a.add(i) as u8;
            let bi = *b.add(i) as u8;
            if ai != bi {
                return (ai as c_int) - (bi as c_int);
            }
            if ai == 0 {
                break;
            }
            i += 1;
        }
        0
    }
}

#[no_mangle]
pub extern "C" fn memcpy(dst: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void {
    unsafe {
        let d = dst as *mut u8;
        let s = src as *const u8;
        for i in 0..n {
            *d.add(i) = *s.add(i);
        }
        dst
    }
}

#[no_mangle]
pub extern "C" fn memset(dst: *mut c_void, c: c_int, n: size_t) -> *mut c_void {
    unsafe {
        let d = dst as *mut u8;
        for i in 0..n {
            *d.add(i) = c as u8;
        }
        dst
    }
}

// ── Native process entry point (optional) ──────────────────────────────────

/// Provide a freestanding startup entry for programs linking this shim with
/// provide_start. Decode the initial argc/argv/envp stack using each
/// architecture's native ABI, call main, then exit with its return status.
#[cfg(all(target_arch = "x86_64", feature = "provide_start"))]
#[no_mangle]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "xor rbp, rbp",
        "mov rdi, [rsp]",
        "lea rsi, [rsp + 8]",
        "lea rdx, [rsi + rdi*8 + 8]",
        "and rsp, -16",
        "call main",
        "mov rdi, rax",
        "mov rax, 60",
        "syscall",
        "ud2",
    );
}

#[cfg(all(target_arch = "aarch64", feature = "provide_start"))]
#[no_mangle]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "mov x29, xzr",
        "ldr x0, [sp]",
        "add x1, sp, #8",
        "add x2, x1, x0, lsl #3",
        "add x2, x2, #8",
        "mov x9, sp",
        "and x9, x9, #-16",
        "mov sp, x9",
        "bl main",
        "mov x8, #93",
        "svc #0",
        "brk #0",
    );
}

#[cfg(all(target_arch = "riscv64", feature = "provide_start"))]
#[no_mangle]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "mv s0, zero",
        "ld a0, 0(sp)",
        "addi a1, sp, 8",
        "addi t0, a0, 1",
        "slli t0, t0, 3",
        "add a2, a1, t0",
        "andi sp, sp, -16",
        "call main",
        "li a7, 93",
        "ecall",
        "unimp",
    );
}
