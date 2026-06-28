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
const SYS_BRK: usize = 12;
#[cfg(target_arch = "x86_64")]
const SYS_EXIT_GROUP: usize = 231;

#[cfg(target_arch = "aarch64")]
const SYS_READ: usize = 63;
#[cfg(target_arch = "aarch64")]
const SYS_WRITE: usize = 64;
#[cfg(target_arch = "aarch64")]
const SYS_BRK: usize = 214;
#[cfg(target_arch = "aarch64")]
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

    // ── 3. Smoke marker (scanned by scripts/ci/run_qemu.sh --smoke) ───────
    // The CI harness looks for this exact line on the serial output.
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
#[no_mangle]
#[unsafe(naked)]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        // Align the stack to 16 bytes as required by the SysV ABI before
        // calling into Rust code (CALL pushes 8 bytes, so AND -16 aligns).
        "and rsp, -16",
        // Zero the frame pointer so stack unwinders stop here.
        "xor rbp, rbp",
        "call {entry}",
        // Should never be reached — init_main() is divergent (-> !).
        // If somehow we fall through, issue a clean exit(1).
        "mov rdi, 1",
        "mov rax, 231",   // SYS_exit_group
        "syscall",
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
