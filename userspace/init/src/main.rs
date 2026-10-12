//! PID 1: validate basic syscalls, execute smoke, and reap the exact child.
#![no_std]
#![no_main]
#[path = "../../common/syscall.rs"]
mod abi;
#[path = "../../common/entry.rs"]
mod entry;
use abi::*;

fn write_all(fd: usize, bytes: &[u8]) {
    let mut done = 0;
    while done < bytes.len() {
        let n = unsafe { sys_write(fd, bytes[done..].as_ptr(), bytes.len() - done) };
        if n == -4 { continue; }
        if n <= 0 || n as usize > bytes.len() - done {
            unsafe { sys_exit_group(1) }
        }
        done += n as usize;
    }
}

fn fail(name: &[u8]) -> ! {
    write_all(2, b"USERSPACE_INIT_FAIL ");
    write_all(2, name);
    write_all(2, b"\n");
    unsafe { sys_exit_group(1) }
}

fn expect_eq(name: &[u8], got: isize, want: isize) {
    if got != want { fail(name); }
    write_all(1, b"[init] TEST PASS: ");
    write_all(1, name);
    write_all(1, b"\n");
}

fn expect_fd(name: &[u8], got: isize) -> usize {
    if got < 0 { fail(name); }
    got as usize
}

fn run_syscall_tests() {
    expect_eq(b"write invalid fd EBADF", unsafe { sys_write(99, b"x".as_ptr(), 1) }, -9);
    expect_eq(b"write invalid pointer EFAULT", unsafe { sys_write(1, 1 as *const u8, 1) }, -14);
    expect_eq(b"execve invalid pointer EFAULT", unsafe {
        sys_execve(1 as *const u8, core::ptr::null(), core::ptr::null())
    }, -14);
    let fd = expect_fd(b"open /dev/null", unsafe {
        sys_open(c"/dev/null".as_ptr() as *const u8, 1, 0)
    });
    expect_eq(b"write /dev/null", unsafe { sys_write(fd, b"discard".as_ptr(), 7) }, 7);
    expect_eq(b"close /dev/null", unsafe { sys_close(fd) }, 0);
    let fd = expect_fd(b"open /dev/zero", unsafe {
        sys_open(c"/dev/zero".as_ptr() as *const u8, 0, 0)
    });
    let mut zeros = [0xAAu8; 8];
    expect_eq(b"read /dev/zero", unsafe { sys_read(fd, zeros.as_mut_ptr(), zeros.len()) }, 8);
    if zeros.iter().any(|byte| *byte != 0) { fail(b"/dev/zero contents"); }
    expect_eq(b"close /dev/zero", unsafe { sys_close(fd) }, 0);
}

fn run_smoke() {
    let child = unsafe { sys_fork() };
    if child < 0 { fail(b"fork smoke"); }
    if child == 0 {
        let path = c"/bin/smoke".as_ptr() as *const u8;
        let argv = [path as usize, 0];
        let envp = [0usize];
        let _ = unsafe { sys_execve(path, argv.as_ptr(), envp.as_ptr()) };
        // Every return from execve is a failure, even an erroneous zero return.
        write_all(2, b"USERSPACE_INIT_FAIL exec smoke\n");
        unsafe { sys_exit_group(127) }
    }
    let mut status = -1i32;
    let waited = loop {
        let result = unsafe { sys_wait4(child, &mut status, 0, 0) };
        if result != -4 { break result; }
    };
    if waited != child || status != 0 { fail(b"smoke child status"); }
}

fn init_main() -> ! {
    write_all(1, b"BOOT_INIT_EXEC\n");
    write_all(1, b"[init] Hello from userspace!\n");
    write_all(1, b"[init] rustos PID 1 running (no-libc Rust init)\n");
    if unsafe { sys_brk(0) } == 0 {
        write_all(2, b"[init] WARNING: brk(0) returned 0\n");
    }
    run_syscall_tests();
    run_smoke();
    write_all(1, b"SMOKE OK: userspace_init\n");
    write_all(1, b"[init] TEST PASS: userspace_init\n");
    write_all(1, b"FULL_OS_USERSPACE_OK\n");
    unsafe { sys_exit_group(0) }
}

userspace_entry!(init_main);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    write_all(2, b"USERSPACE_INIT_FAIL panic\n");
    unsafe { sys_exit_group(101) }
}
