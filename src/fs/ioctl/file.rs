//! Generic file-descriptor ioctl helpers.
use super::consts::{FIONBIO, FIONREAD};
use crate::uaccess::copy_to_user;

pub fn vfs_fionread(fd: usize, arg: usize) -> isize {
    let n = if fd == 0 { crate::tty::serial::bytes_available() }
        else { match crate::fs::vfs::vfs_fionread(fd) { Some(n) => n, None => return -25 } };
    let n = n.min(i32::MAX as usize) as i32;
    copy_to_user(arg, &n.to_ne_bytes()).map(|_| 0).unwrap_or(-14)
}

pub fn pipe_fionread(fd: usize, arg: usize) -> isize {
    let n = crate::fs::pipe::pipe_bytes_available(fd).min(i32::MAX as usize) as i32;
    copy_to_user(arg, &n.to_ne_bytes()).map(|_| 0).unwrap_or(-14)
}

pub fn set_nonblock(fd: usize, arg: usize) -> isize {
    let mut buf = [0u8; 4];
    if crate::uaccess::copy_from_user(&mut buf, arg).is_err() { return -14; }
    let v = u32::from_ne_bytes(buf);
    let pid = crate::proc::scheduler::current_pid();
    let entry = match crate::fs::process_fd::proc_fd_get(pid, fd) { Some(e) => e, None => return -9 };
    crate::fs::process_fd::proc_fd_setfl(pid, fd,
        if v != 0 { entry.fl_flags | 0o4000 } else { entry.fl_flags & !0o4000 });
    crate::fs::fcntl::set_nonblock(entry.backing_fd, v != 0);
    0
}
