//! Shared smoke assertions. The host regression harness supplies mock syscalls.
use super::{check, sys_close, sys_open, sys_read, sys_write, test_errno_checks, O_RDONLY};
use super::{sys_exit_group, sys_fork, sys_wait4, write_str};
use core::ffi::c_int;

pub(super) fn run_tests() -> bool {
    let mut pass = true;
    pass &= check("write stdout", unsafe { sys_write(1, b"SMOKE: write\n".as_ptr(), 13) } == 13);
    let fd = unsafe { sys_open(c"/dev/null".as_ptr() as *const u8, O_RDONLY, 0) };
    pass &= check("open /dev/null", fd >= 0);
    if fd >= 0 {
        pass &= check("close /dev/null", unsafe { sys_close(fd as usize) } == 0);
    }
    let null_fd = unsafe { sys_open(c"/dev/null".as_ptr() as *const u8, O_RDONLY, 0) };
    pass &= check("open /dev/null for read", null_fd >= 0);
    if null_fd >= 0 {
        let mut byte = 0u8;
        pass &= check("read /dev/null returns EOF", unsafe {
            sys_read(null_fd as usize, &mut byte, 1)
        } == 0);
        pass &= check("close /dev/null after read", unsafe { sys_close(null_fd as usize) } == 0);
    }

    // Every architecture must execute the process tests; unsupported clone/fork fails.
    let child = unsafe { sys_fork() };
    if child == 0 {
        write_str(1, "[smoke-child] exiting with status 42\n");
        unsafe { sys_exit_group(42) }
    }
    pass &= check("fork", child > 0);
    if child > 0 {
        let mut status: c_int = -1;
        pass &= check("wait4 child", unsafe { sys_wait4(child, &mut status, 0) } == child);
        pass &= check("child exit status", status == (42 << 8));
    }
    let child2 = unsafe { sys_fork() };
    if child2 == 0 { unsafe { sys_exit_group(42) } }
    pass &= check("second fork", child2 > 0);
    if child2 > 0 {
        let mut status: c_int = -1;
        pass &= check("wait4 exit status", unsafe { sys_wait4(child2, &mut status, 0) } == child2);
        pass &= check("exit status 42", status == (42 << 8));
    }
    pass &= test_errno_checks();
    pass
}
