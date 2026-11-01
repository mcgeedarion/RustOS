//! sys_ioctl dispatcher.
//!
//! Routes ioctl requests to the appropriate handler submodule:
//!   consts.rs  — all request number constants
//!   tty.rs     — TIOC* / termios / FION* on tty fds
//!   net.rs     — SIOC* network interface ioctls
//!   block.rs   — BLK* block device ioctls
//!   file.rs    — generic fd helpers (FIONREAD, FIONBIO)

pub mod block;
pub mod consts;
pub mod file;
pub mod net;
pub mod tty;

use crate::uaccess::{copy_from_user, copy_to_user};
use consts::*;

pub fn sys_ioctl(fd: usize, req: usize, arg: usize) -> isize {
    let user_fd = fd;
    let pid = crate::proc::scheduler::current_pid();
    let entry = match crate::fs::process_fd::proc_fd_get(pid, user_fd) {
        Some(entry) => entry, None => return -9,
    };
    let fd = entry.backing_fd;
    match req {
        FIOCLEX | FIONCLEX => {
            crate::fs::process_fd::proc_fd_set_cloexec(pid, user_fd, req == FIOCLEX);
            return 0;
        },
        FIONBIO => return file::set_nonblock(user_fd, arg),
        _ => {},
    }
    if let Some(pipe) = crate::fs::pipe::user_pipe_backing(user_fd) {
        return match req { FIONREAD => file::pipe_fionread(pipe, arg), _ => -25 };
    }
    if crate::fs::scheme_fd::is_scheme_fd(fd) {
        return crate::fs::scheme_fd::scheme_fd_ioctl(fd, req as u64, arg);
    }
    // Pipe / epoll FDs
    if crate::fs::pipe::is_pipe(fd) {
        match req {
            FIONREAD => return file::pipe_fionread(fd, arg),
            FIONBIO => return file::set_nonblock(fd, arg),
            _ => return -25,
        }
    }

    // Socket FDs
    if crate::net::socket::is_socket_fd(fd) {
        return match req {
            req if req >= SIOCGIFNAME && req <= SIOCETHTOOL => net::sioc_ioctl(req, arg),
            FIONREAD => file::vfs_fionread(fd, arg),
            FIONBIO => file::set_nonblock(fd, arg),
            _ => -25,
        };
    }

    // Block device FDs (virtio-blk)
    if matches!(entry.path.as_deref(), Some("/dev/vda") | Some("/dev/virtio0")) {
        return block::blk_ioctl(fd, req, arg);
    }

    // GPU / framebuffer FDs
    if entry.path.as_deref() == Some("/dev/fb0") {
        match req {
            // FBIOGET_VSCREENINFO (0x4600)
            0x4600 => {
                if let Some(info) = crate::drivers::gop::display_info() {
                    let mut vi = [0u8; 160];
                    vi[0..4].copy_from_slice(&info.width.to_ne_bytes());
                    vi[4..8].copy_from_slice(&info.height.to_ne_bytes());
                    vi[8..12].copy_from_slice(&info.width.to_ne_bytes());
                    vi[12..16].copy_from_slice(&info.height.to_ne_bytes());
                    vi[24..28].copy_from_slice(&32u32.to_ne_bytes()); // bits_per_pixel
                    return copy_to_user(arg, &vi).map(|_| 0).unwrap_or(-14);
                }
                return -19;
            },
            // FBIOGET_FSCREENINFO (0x4602)
            0x4602 => {
                if let Some(info) = crate::drivers::gop::display_info() {
                    let mut fi = [0u8; 80];
                    let fb_phys = match crate::drivers::gop::fb_phys() { Some(p) => p, None => return -19 };
                    fi[16..24].copy_from_slice(&fb_phys.to_ne_bytes());
                    let line_len: u32 = info.pitch;
                    fi[48..52].copy_from_slice(&line_len.to_ne_bytes());
                    return copy_to_user(arg, &fi).map(|_| 0).unwrap_or(-14);
                }
                return -19;
            },
            // FBIOPAN_DISPLAY (0x4606) — no-op
            0x4606 => return -22,
            _ => return -25,
        }
    }

    // Epoll FDs
    if crate::fs::poll::is_epoll_fd(fd) {
        return match req {
            FIONBIO => {
                file::set_nonblock(fd, arg);
                0
            },
            _ => -25,
        };
    }

    // Default: treat as tty/vfs
    let is_tty = fd <= 2;
    match req {
        req if is_tty
            && (req == TCGETS
                || req == TCSETS
                || req == TCSETSW
                || req == TCSETSF
                || req == TIOCGPGRP
                || req == TIOCSPGRP
                || req == TIOCGWINSZ
                || req == TIOCSWINSZ
                || req == TIOCGPTPEER
                || req == TIOCSPTLCK
                || req == TIOCGPTN
                || req == TIOCNOTTY
                || req == TIOCSCTTY
                || req == TIOCEXCL
                || req == TIOCNXCL
                || req == TIOCOUTQ
                || req == TIOCSTI) =>
        {
            tty::tty_ioctl(fd, req, arg)
        },
        FIONREAD => file::vfs_fionread(fd, arg),
        FIONBIO => file::set_nonblock(fd, arg),
        FIOASYNC => -95,
        _ if req >= SIOCGIFNAME && req <= SIOCETHTOOL => net::sioc_ioctl(req, arg),
        _ => -25,
    }
}
