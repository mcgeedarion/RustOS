//! rustos-hello — minimal hello world binary for RustOS
//!
//! Writes "Hello from rustos userspace!" to stdout and exits.
//! Used to verify that:
//!   - elf64::load() maps PT_LOAD segments correctly
//!   - auxv / initial stack is set up correctly by write_initial_stack()
//!   - SYS_write and SYS_exit syscalls are dispatched correctly
//!
//! Build:
//!   cargo build --release --target x86_64-unknown-none

#![no_std]
#![no_main]

use core::arch::asm;

// ── Linux syscall numbers ─────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
const SYS_WRITE: usize = 1;
#[cfg(target_arch = "x86_64")]
const SYS_EXIT: usize = 60;

#[cfg(target_arch = "aarch64")]
const SYS_WRITE: usize = 64;
#[cfg(target_arch = "aarch64")]
const SYS_EXIT: usize = 93;

#[cfg(target_arch = "riscv64")]
const SYS_WRITE: usize = 64;
#[cfg(target_arch = "riscv64")]
const SYS_EXIT: usize = 93;

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

/// `exit(status)` — terminates the process. Never returns.
#[inline(always)]
unsafe fn sys_exit(status: usize) -> ! {
    #[cfg(target_arch = "x86_64")]
    asm!(
        "syscall",
        in("rax") SYS_EXIT,
        in("rdi") status,
        options(noreturn),
    );
    #[cfg(target_arch = "aarch64")]
    asm!(
        "svc #0",
        in("x8") SYS_EXIT,
        in("x0") status,
        options(noreturn),
    );
    #[cfg(target_arch = "riscv64")]
    asm!(
        "ecall",
        in("a7") SYS_EXIT,
        in("a0") status,
        options(noreturn),
    );
}

// ── Convenience write helper ───────────────────────────────────────────────

/// Write all bytes in `msg` to `fd`; retry on short writes.
fn write_all(fd: usize, msg: &[u8]) {
    let mut written = 0;
    while written < msg.len() {
        let n = unsafe { sys_write(fd, msg[written..].as_ptr(), msg.len() - written) };
        if n <= 0 {
            // write failed — cannot do much without a heap; just exit.
            unsafe { sys_exit(1) };
        }
        written += n as usize;
    }
}

// ── Entry point ────────────────────────────────────────────────────────────

/// Main entry point called from `_start` below.
fn hello_main() -> ! {
    const STDOUT: usize = 1;
    const MSG: &[u8] = b"Hello from rustos userspace!\n";

    write_all(STDOUT, MSG);
    unsafe { sys_exit(0) }
}

// ── Low-level entry stub ───────────────────────────────────────────────────

/// `_start` — the ELF entry point.
#[no_mangle]
#[unsafe(naked)]
unsafe extern "C" fn _start() -> ! {
    #[cfg(target_arch = "x86_64")]
    {
        asm!(
            "and rsp, -16",
            "xor rbp, rbp",
            "call {entry}",
            entry = sym hello_main,
            options(noreturn),
        )
    }
    #[cfg(target_arch = "aarch64")]
    {
        asm!(
            "mov x29, #0",
            "and sp, sp, #-16",
            "bl {entry}",
            entry = sym hello_main,
            options(noreturn),
        )
    }
    #[cfg(target_arch = "riscv64")]
    {
        asm!(
            "mv s0, zero",
            "and sp, sp, -16",
            "call {entry}",
            entry = sym hello_main,
            options(noreturn),
        )
    }
}

// ── Panic handler (required for no_std) ───────────────────────────────────

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { sys_exit(101) }
}
