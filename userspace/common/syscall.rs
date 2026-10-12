//! Shared raw syscall wrappers for RustOS userspace.
#![allow(dead_code, clippy::missing_safety_doc)]
use core::arch::asm;

#[inline(always)]
pub unsafe fn syscall6(nr: usize, args: [usize; 6]) -> isize {
    let result: isize;
    #[cfg(target_arch = "x86_64")]
    asm!(
        "syscall",
        inlateout("rax") nr => result,
        in("rdi") args[0], in("rsi") args[1], in("rdx") args[2],
        in("r10") args[3], in("r8") args[4], in("r9") args[5],
        lateout("rcx") _, lateout("r11") _,
        options(nostack),
    );
    #[cfg(target_arch = "aarch64")]
    asm!(
        "svc #0",
        in("x8") nr, inlateout("x0") args[0] => result,
        in("x1") args[1], in("x2") args[2], in("x3") args[3],
        in("x4") args[4], in("x5") args[5],
        options(nostack),
    );
    #[cfg(target_arch = "riscv64")]
    asm!(
        "ecall",
        in("a7") nr, inlateout("a0") args[0] => result,
        in("a1") args[1], in("a2") args[2], in("a3") args[3],
        in("a4") args[4], in("a5") args[5],
        options(nostack),
    );
    result
}

const fn nr(x86: usize, generic: usize) -> usize {
    #[cfg(target_arch = "x86_64")]
    { let _ = generic; x86 }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    { let _ = x86; generic }
}

pub unsafe fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    syscall6(nr(1, 64), [fd, buf as usize, len, 0, 0, 0])
}
pub unsafe fn sys_read(fd: usize, buf: *mut u8, len: usize) -> isize {
    syscall6(nr(0, 63), [fd, buf as usize, len, 0, 0, 0])
}
pub unsafe fn sys_open(path: *const u8, flags: usize, mode: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    { syscall6(2, [path as usize, flags, mode, 0, 0, 0]) }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    { syscall6(56, [(-100isize) as usize, path as usize, flags, mode, 0, 0]) }
}
pub unsafe fn sys_close(fd: usize) -> isize {
    syscall6(nr(3, 57), [fd, 0, 0, 0, 0, 0])
}
pub unsafe fn sys_brk(addr: usize) -> usize {
    syscall6(nr(12, 214), [addr, 0, 0, 0, 0, 0]) as usize
}
pub unsafe fn sys_fork() -> isize {
    #[cfg(target_arch = "x86_64")]
    { syscall6(57, [0; 6]) }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    { syscall6(220, [17, 0, 0, 0, 0, 0]) }
}
pub unsafe fn sys_wait4(pid: isize, status: *mut i32, opts: usize, rusage: usize) -> isize {
    syscall6(nr(61, 260), [pid as usize, status as usize, opts, rusage, 0, 0])
}
pub unsafe fn sys_execve(path: *const u8, argv: *const usize, envp: *const usize) -> isize {
    syscall6(nr(59, 221), [path as usize, argv as usize, envp as usize, 0, 0, 0])
}
pub unsafe fn sys_lseek(fd: usize, offset: usize, whence: usize) -> isize {
    syscall6(nr(8, 62), [fd, offset, whence, 0, 0, 0])
}
pub unsafe fn sys_unlink(path: *const u8) -> isize {
    #[cfg(target_arch = "x86_64")]
    { syscall6(87, [path as usize, 0, 0, 0, 0, 0]) }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    { syscall6(35, [(-100isize) as usize, path as usize, 0, 0, 0, 0]) }
}
pub unsafe fn sys_exit_group(status: usize) -> ! {
    let _ = syscall6(nr(231, 94), [status, 0, 0, 0, 0, 0]);
    // A returning exit syscall is a kernel failure; let the runner time out.
    loop { core::hint::spin_loop(); }
}
