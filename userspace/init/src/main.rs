//! rustos-init — minimal PID-1 userspace process (no libc)
//!
//! This binary is the first end-to-end validation that the kernel can:
//!   1. Load a static ELF into userspace
//!   2. Deliver control to _start at ring 3
//!   3. Service write() and exit_group() syscalls
//!
//! Acceptance criteria (Phase 2):
//!   * Prints "[init] Hello from userspace!" to stdout
//!   * Exercises write(), brk(), exit_group()
//!   * Exits with code 0 — no kernel panic
//!
//! Compile:
//!   cargo build --release --target x86_64-unknown-linux-musl
//!   # or via xtask:
//!   cargo xtask build-init

#![no_std]
#![no_main]
#![allow(clippy::missing_safety_doc)]

use core::arch::{asm, naked_asm};

// ── Linux syscall numbers ─────────────────────────────────────────────────
#[cfg(target_arch = "x86_64")]
const SYS_READ: usize = 0;
#[cfg(target_arch = "x86_64")]
const SYS_WRITE: usize = 1;
#[cfg(target_arch = "x86_64")]
const SYS_OPEN: usize = 2;
#[cfg(target_arch = "x86_64")]
const SYS_CLOSE: usize = 3;
#[cfg(target_arch = "x86_64")]
const SYS_BRK: usize = 12;
#[cfg(target_arch = "x86_64")]
const SYS_FORK: usize = 57;
#[cfg(target_arch = "x86_64")]
const SYS_EXECVE: usize = 59;
#[cfg(target_arch = "x86_64")]
const SYS_WAIT4: usize = 61;
#[cfg(target_arch = "x86_64")]
const SYS_EXIT_GROUP: usize = 231;

#[cfg(target_arch = "aarch64")]
const SYS_READ: usize = 63;
#[cfg(target_arch = "aarch64")]
const SYS_WRITE: usize = 64;
#[cfg(target_arch = "aarch64")]
const SYS_OPEN: usize = 56;
#[cfg(target_arch = "aarch64")]
const SYS_CLOSE: usize = 57;
#[cfg(target_arch = "aarch64")]
const SYS_BRK: usize = 214;
#[cfg(target_arch = "aarch64")]
const SYS_FORK: usize = 220; // Translated to RustOS fork semantics by syscall dispatch.
#[cfg(target_arch = "aarch64")]
const SYS_EXECVE: usize = 221;
#[cfg(target_arch = "aarch64")]
const SYS_WAIT4: usize = 260;
#[cfg(target_arch = "aarch64")]
const SYS_EXIT_GROUP: usize = 94;

#[cfg(target_arch = "riscv64")]
const SYS_READ: usize = 63;
#[cfg(target_arch = "riscv64")]
const SYS_WRITE: usize = 64;
#[cfg(target_arch = "riscv64")]
const SYS_OPEN: usize = 56; // openat
#[cfg(target_arch = "riscv64")]
const SYS_CLOSE: usize = 57;
#[cfg(target_arch = "riscv64")]
const SYS_BRK: usize = 214;
#[cfg(target_arch = "riscv64")]
const SYS_FORK: usize = 220;
#[cfg(target_arch = "riscv64")]
const SYS_EXECVE: usize = 221;
#[cfg(target_arch = "riscv64")]
const SYS_WAIT4: usize = 260;
#[cfg(target_arch = "riscv64")]
const SYS_EXIT_GROUP: usize = 94;

// ── Raw syscall wrappers ───────────────────────────────────────────────────

/// `write(fd, buf, count)` → bytes written (or negative errno)
#[inline(always)]
unsafe fn sys_write(fd: usize, buf: *const u8, count: usize) -> isize {
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
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        asm!(
            "svc #0",
            in("x8") SYS_WRITE,
            in("x0") fd,
            in("x1") buf,
            in("x2") count,
            lateout("x0") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        asm!("ecall", in("a7") SYS_WRITE, in("a0") fd, in("a1") buf, in("a2") count,
             lateout("a0") ret, options(nostack));
        ret
    }

}

/// `read(fd, buf, count)` → bytes read (or negative errno)
#[inline(always)]
#[allow(dead_code)]
unsafe fn sys_read(fd: usize, buf: *mut u8, count: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        asm!(
            "syscall",
            in("rax") SYS_READ,
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
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        asm!(
            "svc #0",
            in("x8") SYS_READ,
            in("x0") fd,
            in("x1") buf,
            in("x2") count,
            lateout("x0") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        asm!("ecall", in("a7") SYS_READ, in("a0") fd, in("a1") buf, in("a2") count,
             lateout("a0") ret, options(nostack));
        ret
    }

}

/// `open(path, flags, mode)` → fd (or negative errno).
#[inline(always)]
unsafe fn sys_open(path: *const u8, flags: usize, mode: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        asm!(
            "syscall",
            in("rax") SYS_OPEN,
            in("rdi") path,
            in("rsi") flags,
            in("rdx") mode,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        asm!(
            "svc #0",
            in("x8") SYS_OPEN,
            in("x0") -100isize, // AT_FDCWD; aarch64 exposes openat, not open.
            in("x1") path,
            in("x2") flags,
            in("x3") mode,
            lateout("x0") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        asm!("ecall", in("a7") SYS_OPEN, in("a0") -100isize, in("a1") path,
             in("a2") flags, in("a3") mode, lateout("a0") ret, options(nostack));
        ret
    }

}

/// `close(fd)` → 0 or negative errno.
#[inline(always)]
unsafe fn sys_close(fd: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        asm!(
            "syscall",
            in("rax") SYS_CLOSE,
            in("rdi") fd,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        asm!(
            "svc #0",
            in("x8") SYS_CLOSE,
            in("x0") fd,
            lateout("x0") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        asm!("ecall", in("a7") SYS_CLOSE, in("a0") fd, lateout("a0") ret, options(nostack));
        ret
    }

}

/// `brk(addr)` — returns current program break; brk(0) is a harmless probe.
#[inline(always)]
unsafe fn sys_brk(addr: usize) -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: usize;
        asm!(
            "syscall",
            in("rax") SYS_BRK,
            in("rdi") addr,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: usize;
        asm!(
            "svc #0",
            in("x8") SYS_BRK,
            in("x0") addr,
            lateout("x0") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        asm!("ecall", in("a7") SYS_BRK, in("a0") addr, lateout("a0") ret, options(nostack));
        ret
    }

}

#[inline(always)]
unsafe fn sys_fork() -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        asm!(
            "syscall",
            in("rax") SYS_FORK,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        asm!("svc #0", in("x8") SYS_FORK, lateout("x0") ret, options(nostack));
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        asm!("ecall", in("a7") SYS_FORK, lateout("a0") ret, options(nostack));
        ret
    }
}

#[inline(always)]
unsafe fn sys_wait4(pid: isize, status: *mut i32, options: usize, rusage: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        asm!(
            "syscall",
            in("rax") SYS_WAIT4,
            in("rdi") pid,
            in("rsi") status,
            in("rdx") options,
            in("r10") rusage,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        asm!(
            "svc #0",
            in("x8") SYS_WAIT4,
            in("x0") pid,
            in("x1") status,
            in("x2") options,
            in("x3") rusage,
            lateout("x0") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        asm!("ecall", in("a7") SYS_WAIT4, in("a0") pid, in("a1") status, in("a2") options,
             in("a3") rusage, lateout("a0") ret, options(nostack));
        ret
    }

}

#[inline(always)]
unsafe fn sys_execve(path: *const u8, argv: *const usize, envp: *const usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        asm!(
            "syscall",
            in("rax") SYS_EXECVE,
            in("rdi") path,
            in("rsi") argv,
            in("rdx") envp,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        asm!(
            "svc #0",
            in("x8") SYS_EXECVE,
            in("x0") path,
            in("x1") argv,
            in("x2") envp,
            lateout("x0") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        asm!("ecall", in("a7") SYS_EXECVE, in("a0") path, in("a1") argv, in("a2") envp,
             lateout("a0") ret, options(nostack));
        ret
    }

}

/// `exit_group(status)` — terminates every thread in the thread group.
/// Never returns; used as the clean PID-1 exit path.
#[inline(always)]
unsafe fn sys_exit_group(status: usize) -> ! {
    #[cfg(target_arch = "x86_64")]
    asm!(
        "syscall",
        in("rax") SYS_EXIT_GROUP,
        in("rdi") status,
        options(noreturn),
    );
    #[cfg(target_arch = "aarch64")]
    asm!(
        "svc #0",
        in("x8") SYS_EXIT_GROUP,
        in("x0") status,
        options(noreturn),
    );
    #[cfg(target_arch = "riscv64")]
    asm!("ecall", in("a7") SYS_EXIT_GROUP, in("a0") status, options(noreturn));

}

// ── Convenience write helper ───────────────────────────────────────────────

/// Write all bytes in `msg` to `fd`; retry on short writes.
/// Panics (aborts) on write error — appropriate for PID 1 init.
fn write_all(fd: usize, msg: &[u8]) {
    let mut written = 0;
    while written < msg.len() {
        let n = unsafe { sys_write(fd, msg[written..].as_ptr(), msg.len() - written) };
        if n <= 0 {
            // write failed — cannot do much without a heap; just abort.
            unsafe { sys_exit_group(1) };
        }
        written += n as usize;
    }
}

/// Write a decimal integer without a trailing newline.
fn write_usize_inline(fd: usize, value: usize) {
    let mut buf = [0u8; 24];
    let mut i = buf.len();
    let mut v = value;
    if v == 0 {
        i -= 1;
        buf[i] = b'0';
    } else {
        while v > 0 {
            i -= 1;
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
    }
    write_all(fd, &buf[i..]);
}

/// Write a decimal integer followed by a newline to `fd`.
fn write_usize(fd: usize, prefix: &[u8], value: usize) {
    let mut buf = [0u8; 24]; // max usize decimal digits + newline
    let mut i = buf.len();
    let mut v = value;
    if v == 0 {
        i -= 1;
        buf[i] = b'0';
    } else {
        while v > 0 {
            i -= 1;
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
    }
    write_all(fd, prefix);
    write_all(fd, &buf[i..]);
    write_all(fd, b"\n");
}

fn expect_eq(name: &[u8], got: isize, want: isize) {
    if got != want {
        write_all(2, b"[init] TEST FAIL: ");
        write_all(2, name);
        write_all(2, b"\n");
        unsafe { sys_exit_group(1) };
    }
    write_all(1, b"[init] TEST PASS: ");
    write_all(1, name);
    write_all(1, b"\n");
}

fn expect_nonnegative(name: &[u8], got: isize) -> usize {
    if got < 0 {
        write_all(2, b"[init] TEST FAIL: ");
        write_all(2, name);
        write_all(2, b"\n");
        unsafe { sys_exit_group(1) };
    }
    write_all(1, b"[init] TEST PASS: ");
    write_all(1, name);
    write_all(1, b"\n");
    got as usize
}

fn run_syscall_tests() {
    expect_eq(
        b"write invalid fd returns EBADF",
        unsafe { sys_write(99, b"x".as_ptr(), 1) },
        -9,
    );
    expect_eq(
        b"write invalid pointer returns EFAULT",
        unsafe { sys_write(1, 1 as *const u8, 1) },
        -14,
    );
    expect_eq(
        b"execve invalid pointer returns EFAULT",
        unsafe { sys_execve(1 as *const u8, core::ptr::null(), core::ptr::null()) },
        -14,
    );

    let null_fd = expect_nonnegative(b"open /dev/null", unsafe {
        sys_open(c"/dev/null".as_ptr() as *const u8, 1, 0)
    });
    expect_eq(
        b"write /dev/null",
        unsafe { sys_write(null_fd, b"discard".as_ptr(), 7) },
        7,
    );
    expect_eq(b"close /dev/null", unsafe { sys_close(null_fd) }, 0);

    let zero_fd = expect_nonnegative(b"open /dev/zero", unsafe {
        sys_open(c"/dev/zero".as_ptr() as *const u8, 0, 0)
    });
    let mut zeros = [0xAAu8; 8];
    expect_eq(
        b"read /dev/zero",
        unsafe { sys_read(zero_fd, zeros.as_mut_ptr(), zeros.len()) },
        zeros.len() as isize,
    );
    for byte in zeros {
        if byte != 0 {
            write_all(2, b"[init] TEST FAIL: /dev/zero returned nonzero byte\n");
            unsafe { sys_exit_group(1) };
        }
    }
    expect_eq(b"close /dev/zero", unsafe { sys_close(zero_fd) }, 0);

    #[cfg(target_arch = "x86_64")]
    {
        let child = unsafe { sys_fork() };
        if child == 0 {
            write_all(1, b"[init-child] exiting with status 42\n");
            unsafe { sys_exit_group(42) };
        }
        let child_pid = expect_nonnegative(b"fork child", child);
        let mut status = 0i32;
        expect_eq(
            b"wait4 child",
            unsafe { sys_wait4(child_pid as isize, &mut status as *mut i32, 0, 0) },
            child_pid as isize,
        );
        if ((status >> 8) & 0xff) != 42 {
            write_all(2, b"[init] TEST FAIL: child exit status mismatch\n");
            unsafe { sys_exit_group(1) };
        }
        write_all(1, b"[init] TEST PASS: child exit status 42\n");
    }
}

/// Execute the real /smoke ELF as a child, then reap the same PID and
/// require normal exit with status zero. A returning execve is a failure.
fn run_exec_smoke_child() {
    const STDOUT: usize = 1;
    const STDERR: usize = 2;
    let child = unsafe { sys_fork() };

    if child == 0 {
        let path = c"/smoke";
        let argv = [path.as_ptr() as usize, 0usize];
        let envp = [0usize];
        let result = unsafe {
            sys_execve(path.as_ptr() as *const u8, argv.as_ptr(), envp.as_ptr())
        };
        write_all(STDERR, b"RUSTOS_EXEC marker=smoke_execve_failed errno=");
        write_usize_inline(STDERR, result.wrapping_neg() as usize);
        write_all(STDERR, b"\n");
        unsafe { sys_exit_group(127) };
    }

    if child < 0 {
        write_all(STDERR, b"RUSTOS_EXEC marker=smoke_fork_failed errno=");
        write_usize_inline(STDERR, child.wrapping_neg() as usize);
        write_all(STDERR, b"\n");
        unsafe { sys_exit_group(1) };
    }

    let mut status = 0i32;
    let waited = unsafe { sys_wait4(child, &mut status as *mut i32, 0, 0) };
    if waited != child {
        write_all(STDERR, b"RUSTOS_EXEC marker=smoke_wait_failed\n");
        unsafe { sys_exit_group(1) };
    }

    let exited_normally = (status & 0x7f) == 0;
    let exit_code = (status >> 8) & 0xff;
    if !exited_normally || exit_code != 0 {
        write_all(STDERR, b"RUSTOS_EXEC marker=smoke_child_failed pid=");
        write_usize_inline(STDERR, child as usize);
        write_all(STDERR, b" status=");
        write_usize_inline(STDERR, status as usize);
        write_all(STDERR, b" exit=");
        write_usize_inline(STDERR, exit_code as usize);
        write_all(STDERR, b"\n");
        unsafe { sys_exit_group(1) };
    }

    write_all(STDOUT, b"RUSTOS_EXEC marker=smoke_child_reaped pid=");
    write_usize_inline(STDOUT, child as usize);
    write_all(STDOUT, b" exit=0\n");
}

/// Run the fixed-iteration syscall benchmark as a separate userspace child.
/// Its JSON record and completion marker are written to the inherited console;
/// PID 1 still verifies that the specific benchmark child exited normally.
fn run_exec_benchmark_child() {
    const STDOUT: usize = 1;
    const STDERR: usize = 2;
    let child = unsafe { sys_fork() };

    if child == 0 {
        let path = c"/bench";
        let argv = [path.as_ptr() as usize, 0usize];
        let envp = [0usize];
        let result = unsafe {
            sys_execve(path.as_ptr() as *const u8, argv.as_ptr(), envp.as_ptr())
        };
        write_all(STDERR, b"RUSTOS_EXEC marker=bench_execve_failed errno=");
        write_usize_inline(STDERR, result.wrapping_neg() as usize);
        write_all(STDERR, b"\n");
        unsafe { sys_exit_group(127) };
    }

    if child < 0 {
        write_all(STDERR, b"RUSTOS_EXEC marker=bench_fork_failed errno=");
        write_usize_inline(STDERR, child.wrapping_neg() as usize);
        write_all(STDERR, b"\n");
        unsafe { sys_exit_group(1) };
    }

    let mut status = 0i32;
    let waited = unsafe { sys_wait4(child, &mut status as *mut i32, 0, 0) };
    if waited != child {
        write_all(STDERR, b"RUSTOS_EXEC marker=bench_wait_failed\n");
        unsafe { sys_exit_group(1) };
    }

    let exited_normally = (status & 0x7f) == 0;
    let exit_code = (status >> 8) & 0xff;
    if !exited_normally || exit_code != 0 {
        write_all(STDERR, b"RUSTOS_EXEC marker=bench_child_failed pid=");
        write_usize_inline(STDERR, child as usize);
        write_all(STDERR, b" status=");
        write_usize_inline(STDERR, status as usize);
        write_all(STDERR, b" exit=");
        write_usize_inline(STDERR, exit_code as usize);
        write_all(STDERR, b"\n");
        unsafe { sys_exit_group(1) };
    }

    write_all(STDOUT, b"RUSTOS_EXEC marker=bench_child_reaped pid=");
    write_usize_inline(STDOUT, child as usize);
    write_all(STDOUT, b" exit=0\n");
}

// ── Entry point ────────────────────────────────────────────────────────────

/// Rust entry point called from `_start` below.
///
/// Responsible for:
///   1. Printing the boot banner to stdout
///   2. Exercising brk() as a minimal heap syscall check
///   3. Printing a PASS marker (scanned by CI / QEMU smoke runner)
///   4. Exiting cleanly with status 0
fn init_main() -> ! {
    const STDOUT: usize = 1;
    const STDERR: usize = 2;

    // ── 1. Boot banner ────────────────────────────────────────────────────
    write_all(STDOUT, b"[init] Hello from userspace!\n");
    write_all(STDOUT, b"[init] rustos PID 1 running (no-libc Rust init)\n");

    // ── 2. Syscall exercise: brk(0) probes the data segment end ──────────
    let brk_addr = unsafe { sys_brk(0) };
    write_usize(STDOUT, b"[init] brk(0) -> 0x", brk_addr);
    // A non-zero brk return means the kernel handled the syscall.
    if brk_addr == 0 {
        write_all(
            STDERR,
            b"[init] WARNING: brk(0) returned 0 - heap syscall may be unimplemented\n",
        );
    }

    run_syscall_tests();
    write_all(STDOUT, b"RUSTOS_EXEC marker=init_started\n");
    run_exec_smoke_child();
    run_exec_benchmark_child();

    // Completion is emitted only after successful smoke/benchmark execve,
    // exact-PID wait/reap, normal termination, and exit status zero.
    write_all(STDOUT, b"RUSTOS_EXEC marker=init_complete exit=0\n");
    write_all(STDOUT, b"SMOKE OK: userspace_init\n");
    write_all(STDOUT, b"[init] TEST PASS: userspace_init\n");
    write_all(STDOUT, b"FULL_OS_USERSPACE_OK\n");

    // ── 4. Clean exit ─────────────────────────────────────────────────────
    // Using exit_group (not exit) so the kernel's PID-1 reaper path is
    // exercised correctly. The kernel must not panic on PID 1 exit.
    unsafe { sys_exit_group(0) }
}

// ── Low-level entry stub ───────────────────────────────────────────────────

/// `_start` — the ELF entry point.
///
/// The kernel jumps here after loading the ELF and setting up the initial
/// stack. The System V AMD64 ABI defines:
///   [rsp]     = argc
///   [rsp+8]   = argv[0..argc]
///   following = envp[], auxv[]
///
/// We ignore all of those for now (init takes no arguments) and simply
/// call into `init_main()`.
#[cfg(target_arch = "x86_64")]
#[no_mangle]
#[unsafe(naked)]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "and rsp, -16",
        "xor rbp, rbp",
        "call {entry}",
        "mov rdi, 1",
        "mov rax, 231",
        "syscall",
        entry = sym init_main,
    );
}

#[cfg(target_arch = "aarch64")]
#[no_mangle]
#[unsafe(naked)]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "mov x29, xzr",
        "and sp, sp, #-16",
        "bl {entry}",
        "mov x0, #1",
        "mov x8, #94",
        "svc #0",
        entry = sym init_main,
    );
}

#[cfg(target_arch = "riscv64")]
#[no_mangle]
#[unsafe(naked)]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "mv s0, zero",
        "andi sp, sp, -16",
        "call {entry}",
        "li a0, 1",
        "li a7, 94",
        "ecall",
        entry = sym init_main,
    );
}

// ── Panic handler (required for no_std) ───────────────────────────────────

/// Minimal panic handler — prints a message and calls exit_group(101).
/// Panics in /init most likely indicate a bug in the init logic itself
/// or a kernel ABI mismatch; exiting with a non-zero code makes it
/// visible in QEMU output and the smoke-test harness.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    const STDERR: usize = 2;
    write_all(STDERR, b"[init] PANIC: ");
    if let Some(loc) = info.location() {
        // We can't use format! without alloc, so write file/line manually.
        write_all(STDERR, loc.file().as_bytes());
        write_all(STDERR, b":");
        write_usize(STDERR, b"", loc.line() as usize);
    } else {
        write_all(STDERR, b"(unknown location)\n");
    }
    unsafe { sys_exit_group(101) }
}
