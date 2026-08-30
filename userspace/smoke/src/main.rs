//! rustos-smoke — Minimal userspace syscall smoke test for RustOS
//!
//! This binary performs comprehensive syscall testing:
//!   - write to stdout
//!   - open/close /dev/null
//!   - fork/wait with exit status verification
//!   - errno handling for invalid operations
//!
//! Build:
//!   cargo build --release --target x86_64-unknown-none

#![no_std]
#![no_main]

use core::arch::asm;
use core::ffi::c_int;

// ── Linux syscall numbers ─────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
const SYS_WRITE: usize = 1;
#[cfg(target_arch = "x86_64")]
const SYS_OPEN: usize = 2;
#[cfg(target_arch = "x86_64")]
const SYS_CLOSE: usize = 3;
#[cfg(target_arch = "x86_64")]
const SYS_WAIT4: usize = 61;
#[cfg(target_arch = "x86_64")]
const SYS_FORK: usize = 57;
#[cfg(target_arch = "x86_64")]
const SYS_EXIT_GROUP: usize = 231;
#[cfg(target_arch = "x86_64")]
const SYS_EXECVE: usize = 59;

#[cfg(target_arch = "aarch64")]
const SYS_WRITE: usize = 64;
#[cfg(target_arch = "aarch64")]
const SYS_OPEN: usize = usize::MAX; // Use openat
#[cfg(target_arch = "aarch64")]
const SYS_OPENAT: usize = 56;
#[cfg(target_arch = "aarch64")]
const SYS_CLOSE: usize = 57;
#[cfg(target_arch = "aarch64")]
const SYS_WAIT4: usize = 260;
#[cfg(target_arch = "aarch64")]
const SYS_FORK: usize = usize::MAX; // Not implemented
#[cfg(target_arch = "aarch64")]
const SYS_EXIT_GROUP: usize = 94;
#[cfg(target_arch = "aarch64")]
const SYS_EXECVE: usize = 221;

#[cfg(target_arch = "riscv64")]
const SYS_WRITE: usize = 64;
#[cfg(target_arch = "riscv64")]
const SYS_OPENAT: usize = 56;
#[cfg(target_arch = "riscv64")]
const SYS_CLOSE: usize = 57;
#[cfg(target_arch = "riscv64")]
const SYS_WAIT4: usize = 260;
#[cfg(target_arch = "riscv64")]
const SYS_FORK: usize = 220;
#[cfg(target_arch = "riscv64")]
const SYS_EXIT_GROUP: usize = 94;
#[cfg(target_arch = "riscv64")]
const SYS_EXECVE: usize = 221;

// Error codes
const EFAULT: isize = -14;
const EBADF: isize = -9;
const ECHILD: isize = -10;

// Open flags
const O_RDONLY: usize = 0;

// ── Raw syscall wrappers ───────────────────────────────────────────────────

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
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        let ret: isize;
        asm!(
            "ecall",
            in("a7") SYS_OPENAT,
            in("a0") -100isize, // AT_FDCWD
            in("a1") path,
            in("a2") flags,
            in("a3") mode,
            lateout("a0") ret,
            options(nostack),
        );
        ret
    }
}

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
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        let ret: isize;
        asm!(
            "ecall",
            in("a7") SYS_CLOSE,
            in("a0") fd,
            lateout("a0") ret,
            options(nostack),
        );
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
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        asm!(
            "ecall",
            in("a7") SYS_FORK,
            lateout("a0") ret,
            options(nostack),
        );
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        -38 // ENOSYS
    }
}

#[inline(always)]
unsafe fn sys_wait4(pid: isize, status: *mut c_int, options: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        asm!(
            "syscall",
            in("rax") SYS_WAIT4,
            in("rdi") pid,
            in("rsi") status,
            in("rdx") options,
            in("r10") 0, // rusage
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
            in("a7") SYS_WAIT4,
            in("a0") pid,
            in("a1") status,
            in("a2") options,
            in("a3") 0, // rusage
            lateout("a0") ret,
            options(nostack),
        );
        ret
    }
}

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
    asm!(
        "ecall",
        in("a7") SYS_EXIT_GROUP,
        in("a0") status,
        options(noreturn),
    );
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

fn check(name: &str, ok: bool) -> bool {
    if !ok {
        write_str(2, "SMOKE FAIL: ");
        write_str(2, name);
        write_str(2, "\n");
    }
    ok
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

// ── Syscall tests ──────────────────────────────────────────────────────────

fn test_errno_checks() -> bool {
    let mut pass = true;

    // Bad write returns EFAULT
    pass &= check(
        "bad write returns errno",
        unsafe { sys_write(1, 1 as *const u8, 1) } == EFAULT,
    );

    // Bad open returns EFAULT
    pass &= check(
        "bad open returns errno",
        unsafe { sys_open(1 as *const u8, O_RDONLY, 0) } == EFAULT,
    );

    // Bad close returns EBADF
    pass &= check(
        "bad close returns errno",
        unsafe { sys_close(!0usize) } == EBADF,
    );

    // Bad wait4 returns ECHILD
    pass &= check(
        "bad wait4 returns errno",
        unsafe { sys_wait4(999999, core::ptr::null_mut(), 0) } == ECHILD,
    );

    // Bad execve returns EFAULT
    pass &= check(
        "bad execve returns errno",
        unsafe { sys_execve(1 as *const u8, core::ptr::null(), core::ptr::null()) } == EFAULT,
    );

    pass
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
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        let ret: isize;
        asm!(
            "ecall",
            in("a7") SYS_EXECVE,
            in("a0") path,
            in("a1") argv,
            in("a2") envp,
            lateout("a0") ret,
            options(nostack),
        );
        ret
    }
}

fn run_tests() -> bool {
    let mut pass = true;

    // Test write to stdout
    pass &= check("write stdout", unsafe {
        sys_write(1, b"SMOKE: write\n".as_ptr(), 13)
    } == 13);

    // Test open/close /dev/null
    let fd = unsafe { sys_open(c"/dev/null".as_ptr() as *const u8, O_RDONLY, 0) };
    pass &= check("open /dev/null", fd >= 0);
    if fd >= 0 {
        pass &= check("close /dev/null", unsafe { sys_close(fd as usize) } == 0);
    }

    // Test read from /dev/null is EOF-safe
    let null_fd = unsafe { sys_open(c"/dev/null".as_ptr() as *const u8, O_RDONLY, 0) };
    if null_fd >= 0 {
        let mut byte: u8 = 0;
        let result = unsafe { sys_read(null_fd as usize, &mut byte as *mut u8, 1) };
        pass &= check("read from /dev/null is EOF-safe", result >= 0);
        unsafe { sys_close(null_fd as usize) };
    }

    // Test fork/wait on architectures that support it
    #[cfg(any(target_arch = "x86_64", target_arch = "riscv64"))]
    {
        let child = unsafe { sys_fork() };
        if child == 0 {
            // Child process
            write_str(1, "[smoke-child] exiting with status 42\n");
            unsafe { sys_exit_group(42) };
        }

        let child_pid = check("fork", child >= 0);
        if child > 0 {
            let mut status: c_int = 0;
            pass &= check(
                "wait4 child",
                unsafe { sys_wait4(child, &mut status as *mut c_int, 0) } == child,
            );
            let exit_status = ((status >> 8) & 0xff) as u8;
            pass &= check("child exit status", exit_status == 42);
        }

        // Test fork with immediate exit
        let child2 = unsafe { sys_fork() };
        if child2 == 0 {
            unsafe { sys_exit_group(42) };
        }
        if child2 > 0 {
            let mut status: c_int = 0;
            pass &= check(
                "wait4 exit status",
                unsafe { sys_wait4(child2, &mut status as *mut c_int, 0) } == child2,
            );
            let exit_status = ((status >> 8) & 0xff) as u8;
            pass &= check("exit status 42", exit_status == 42);
        }
    }

    pass &= test_errno_checks();

    pass
}

#[inline(always)]
unsafe fn sys_read(fd: usize, buf: *mut u8, count: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    const SYS_READ: usize = 0;
    #[cfg(target_arch = "aarch64")]
    const SYS_READ: usize = 63;
    #[cfg(target_arch = "riscv64")]
    const SYS_READ: usize = 63;

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
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        let ret: isize;
        asm!(
            "ecall",
            in("a7") SYS_READ,
            in("a0") fd,
            in("a1") buf,
            in("a2") count,
            lateout("a0") ret,
            options(nostack),
        );
        ret
    }
}

// ── Entry point ────────────────────────────────────────────────────────────

fn smoke_main() -> ! {
    let pass = run_tests();

    if pass {
        write_str(1, "SMOKE OK: userspace_smoke\n");
        unsafe { sys_exit_group(0) };
    } else {
        write_str(1, "SMOKE FAIL: tests failed\n");
        unsafe { sys_exit_group(1) };
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
            entry = sym smoke_main,
            options(noreturn),
        )
    }
    #[cfg(target_arch = "aarch64")]
    {
        asm!(
            "mov x29, #0",
            "and sp, sp, #-16",
            "bl {entry}",
            entry = sym smoke_main,
            options(noreturn),
        )
    }
    #[cfg(target_arch = "riscv64")]
    {
        asm!(
            "mv s0, zero",
            "and sp, sp, -16",
            "call {entry}",
            entry = sym smoke_main,
            options(noreturn),
        )
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { sys_exit_group(101) }
}
