//! Runtime smoke: processes, descriptors, a regular file and errno paths.
#![no_std]
#![no_main]
use core::ffi::c_int;
#[path = "../../common/syscall.rs"]
mod abi;
#[path = "../../common/entry.rs"]
mod entry;
use abi::{sys_write, sys_read, sys_open, sys_close, sys_fork, sys_execve, sys_exit_group};
mod checks;
const O_RDONLY: usize = 0;

unsafe fn sys_wait4(pid: isize, status: *mut c_int, opts: usize) -> isize {
    loop {
        let result = abi::sys_wait4(pid, status, opts, 0);
        if result != -4 { return result; }
    }
}

fn write_all(fd: usize, bytes: &[u8]) {
    let mut done = 0;
    while done < bytes.len() {
        let n = unsafe { sys_write(fd, bytes[done..].as_ptr(), bytes.len() - done) };
        if n == -4 { continue; }
        if n <= 0 || n as usize > bytes.len() - done { unsafe { sys_exit_group(1) } }
        done += n as usize;
    }
}
fn write_str(fd: usize, text: &str) { write_all(fd, text.as_bytes()); }
fn check(name: &str, ok: bool) -> bool {
    if !ok {
        write_str(2, "SMOKE FAIL: ");
        write_str(2, name);
        write_str(2, "\n");
    }
    ok
}

fn test_errno_checks() -> bool {
    let mut pass = true;
    pass &= check("bad write EFAULT", unsafe { sys_write(1, 1 as *const u8, 1) } == -14);
    pass &= check("bad open EFAULT", unsafe { sys_open(1 as *const u8, O_RDONLY, 0) } == -14);
    pass &= check("bad close EBADF", unsafe { sys_close(!0usize) } == -9);
    pass &= check("bad wait4 ECHILD", unsafe { sys_wait4(999999, core::ptr::null_mut(), 0) } == -10);
    pass &= check("bad execve EFAULT", unsafe {
        sys_execve(1 as *const u8, core::ptr::null(), core::ptr::null())
    } == -14);
    pass
}

fn test_regular_file() -> bool {
    let path = c"/rustos-smoke.tmp".as_ptr() as *const u8;
    let fd = unsafe { sys_open(path, 2 | 64 | 128, 0o600) };
    if !check("create regular file", fd >= 0) { return false; }
    let fd = fd as usize;
    const DATA: &[u8] = b"rustos userspace file roundtrip";
    let mut pass = check("write regular file", unsafe {
        sys_write(fd, DATA.as_ptr(), DATA.len())
    } == DATA.len() as isize);
    pass &= check("seek regular file", unsafe { abi::sys_lseek(fd, 0, 0) } == 0);
    let mut readback = [0u8; DATA.len()];
    pass &= check("read regular file", unsafe {
        sys_read(fd, readback.as_mut_ptr(), readback.len())
    } == DATA.len() as isize);
    pass &= check("regular file contents", readback.as_slice() == DATA);
    pass &= check("close regular file", unsafe { sys_close(fd) } == 0);
    pass &= check("unlink regular file", unsafe { abi::sys_unlink(path) } == 0);
    let missing = unsafe { sys_open(path, O_RDONLY, 0) };
    pass &= check("unlinked file ENOENT", missing == -2);
    if missing >= 0 { let _ = unsafe { sys_close(missing as usize) }; }
    pass
}

fn smoke_main() -> ! {
    write_str(1, "USERSPACE_SMOKE_START\n");
    let pass = checks::run_tests() & test_regular_file();
    if pass {
        write_str(1, "SMOKE OK: userspace_smoke\nUSERSPACE_SMOKE_PASS\n");
        unsafe { sys_exit_group(0) }
    }
    write_str(2, "USERSPACE_SMOKE_FAIL\n");
    unsafe { sys_exit_group(1) }
}

userspace_entry!(smoke_main);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    write_str(2, "USERSPACE_SMOKE_FAIL panic\n");
    unsafe { sys_exit_group(101) }
}
