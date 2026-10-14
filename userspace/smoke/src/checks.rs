//! Shared smoke assertions. The host regression harness supplies mock syscalls.

use super::{
    check, sys_close, sys_open, sys_pipe2, sys_read, sys_rename, sys_unlink, sys_write, test_errno_checks,
    O_CREAT, O_RDONLY, O_RDWR, O_TRUNC,
};
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64"))]
use super::{sys_exit_group, sys_fork, sys_wait4, write_str};
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64"))]
use core::ffi::c_int;

pub(super) fn run_tests() -> bool {
    let mut pass = true;

    // Test write to stdout.
    pass &= check(
        "write stdout",
        unsafe { sys_write(1, b"SMOKE: write\n".as_ptr(), 13) } == 13,
    );

    // Test open/close /dev/null.
    let fd = unsafe { sys_open(c"/dev/null".as_ptr() as *const u8, O_RDONLY, 0) };
    pass &= check("open /dev/null", fd >= 0);
    if fd >= 0 {
        pass &= check("close /dev/null", unsafe { sys_close(fd as usize) } == 0);
    }

    // A failed open must not silently skip the EOF check.
    let null_fd = unsafe { sys_open(c"/dev/null".as_ptr() as *const u8, O_RDONLY, 0) };
    pass &= check("open /dev/null for read", null_fd >= 0);
    if null_fd >= 0 {
        let mut byte: u8 = 0;
        let result = unsafe { sys_read(null_fd as usize, &mut byte as *mut u8, 1) };
        pass &= check("read /dev/null returns EOF", result == 0);
        pass &= check(
            "close /dev/null after read",
            unsafe { sys_close(null_fd as usize) } == 0,
        );
    }

    // Test fork/wait on architectures that support it.
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64"))]
    {
        let child = unsafe { sys_fork() };
        if child == 0 {
            write_str(1, "[smoke-child] exiting with status 42\n");
            unsafe { sys_exit_group(42) };
        }

        pass &= check("fork", child >= 0);
        if child > 0 {
            let mut status: c_int = 0;
            pass &= check(
                "wait4 child",
                unsafe { sys_wait4(child, &mut status as *mut c_int, 0) } == child,
            );
            let exit_status = ((status >> 8) & 0xff) as u8;
            pass &= check("child exit status", exit_status == 42);
        }

        let child2 = unsafe { sys_fork() };
        if child2 == 0 {
            unsafe { sys_exit_group(42) };
        }
        pass &= check("second fork", child2 >= 0);
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

    // Create a real writable file, write known bytes, reopen and read them
    // back, then remove it. A missing /tmp mount or unsupported write/unlink
    // syscall is a hard smoke failure.
    const FILE_PATH: &[u8] = b"/tmp/rustos-smoke-write-test\0";
    const RENAMED_PATH: &[u8] = b"/tmp/rustos-smoke-write-renamed\0";
    const FILE_DATA: &[u8] = b"rustos-write-test";
    let mut file_was_renamed = false;
    let file_fd = unsafe {
        sys_open(
            FILE_PATH.as_ptr(),
            O_CREAT | O_RDWR | O_TRUNC,
            0o600,
        )
    };
    pass &= check("create writable smoke file", file_fd >= 0);
    if file_fd >= 0 {
        let written = unsafe { sys_write(file_fd as usize, FILE_DATA.as_ptr(), FILE_DATA.len()) };
        pass &= check("write smoke file", written == FILE_DATA.len() as isize);
        pass &= check("close smoke file after write", unsafe { sys_close(file_fd as usize) } == 0);

        let rename_result = unsafe { sys_rename(FILE_PATH.as_ptr(), RENAMED_PATH.as_ptr()) };
        pass &= check("rename smoke file", rename_result == 0);
        file_was_renamed = rename_result == 0;
        let read_path = if file_was_renamed { RENAMED_PATH } else { FILE_PATH };
        let read_fd = unsafe { sys_open(read_path.as_ptr(), O_RDONLY, 0) };
        pass &= check("reopen smoke file", read_fd >= 0);
        if read_fd >= 0 {
            let mut contents = [0u8; 32];
            let bytes_read = unsafe { sys_read(read_fd as usize, contents.as_mut_ptr(), contents.len()) };
            pass &= check(
                "read back smoke file contents",
                bytes_read == FILE_DATA.len() as isize
                    && &contents[..FILE_DATA.len()] == FILE_DATA,
            );
            pass &= check("close smoke file after read", unsafe { sys_close(read_fd as usize) } == 0);
        }
    }
    // Remove whichever name remains, including when rename failed.
    let cleanup_path: &[u8] = if file_was_renamed { RENAMED_PATH } else { FILE_PATH };
    pass &= check("unlink smoke file", unsafe { sys_unlink(cleanup_path.as_ptr()) } == 0);

    // Exercise kernel-backed pipe IPC with a bounded in-pipe payload.
    let mut pipe_fds = [-1i32; 2];
    let pipe_result = unsafe { sys_pipe2(pipe_fds.as_mut_ptr(), 0) };
    pass &= check("pipe2 create", pipe_result == 0);
    if pipe_result == 0 {
        const PIPE_DATA: &[u8] = b"rustos-pipe-roundtrip";
        let pipe_written = unsafe {
            sys_write(pipe_fds[1] as usize, PIPE_DATA.as_ptr(), PIPE_DATA.len())
        };
        pass &= check("pipe write", pipe_written == PIPE_DATA.len() as isize);

        let mut pipe_readback = [0u8; 32];
        let pipe_read = unsafe {
            sys_read(pipe_fds[0] as usize, pipe_readback.as_mut_ptr(), pipe_readback.len())
        };
        pass &= check(
            "pipe read matches write",
            pipe_read == PIPE_DATA.len() as isize
                && &pipe_readback[..PIPE_DATA.len()] == PIPE_DATA,
        );
        pass &= check("close pipe read end", unsafe { sys_close(pipe_fds[0] as usize) } == 0);
        pass &= check("close pipe write end", unsafe { sys_close(pipe_fds[1] as usize) } == 0);
    }

    pass &= test_errno_checks();
    pass
}
