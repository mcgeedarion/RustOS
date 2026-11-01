// src/io_uring/ops/accept.rs

use crate::io_uring::{cqe::errno, sqe::Sqe};

// Mirrored from Linux <sys/socket.h>
pub mod sock_flags {
    pub const SOCK_NONBLOCK: u32 = 0o0004000;
    pub const SOCK_CLOEXEC: u32 = 0o2000000;
}

/// Synchronous kernel-side handler for IORING_OP_ACCEPT.
pub fn handle(sqe: &Sqe) -> i32 {
    let listen_fd = sqe.fd;
    let addr_va = sqe.addr;
    let addrlen_va = sqe.off;
    let flags = sqe.op_flags;

    if listen_fd < 0 {
        log::warn!("[io_uring::accept] invalid listen_fd={}", listen_fd);
        return errno::E_BADF;
    }

    #[cfg(feature = "boot_debug")]
    log::trace!(
        "[io_uring::accept] listen_fd={} addr={:#x} addrlen={:#x} flags={:#x} token={:#x}",
        listen_fd,
        addr_va,
        addrlen_va,
        flags,
        sqe.user_data
    );

    if flags & !(sock_flags::SOCK_NONBLOCK | sock_flags::SOCK_CLOEXEC) != 0 { return -22; }
    let fd = crate::net::socket::core::sys_accept(
        listen_fd as usize, addr_va as usize, addrlen_va as usize,
    );
    if fd >= 0 {
        let pid = crate::proc::scheduler::current_pid();
        if flags & sock_flags::SOCK_CLOEXEC != 0 {
            crate::fs::process_fd::proc_fd_set_cloexec(pid, fd as usize, true);
        }
        if flags & sock_flags::SOCK_NONBLOCK != 0 {
            crate::fs::process_fd::proc_fd_set_nonblock(pid, fd as usize, true);
        }
    }
    fd as i32
}

use crate::io_uring::{with_ring, IoUringError};
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Async wrapper around IORING_OP_ACCEPT.
pub struct IoAccept {
    ring_idx: usize,
    fd: i32,
    addr_va: u64,
    addrlen_va: u64,
    flags: u32,
    token: u64,
    result: Option<i32>,
}

impl IoAccept {
    /// Create a new accept future.
    pub fn new(
        ring_idx: usize,
        fd: i32,
        addr_va: u64,
        addrlen_va: u64,
        flags: u32,
        token: u64,
    ) -> Self {
        IoAccept {
            ring_idx,
            fd,
            addr_va,
            addrlen_va,
            flags,
            token,
            result: None,
        }
    }
}

impl Future for IoAccept {
    type Output = Result<i32, IoUringError>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.result.is_none() {
            if with_ring(self.ring_idx, |_| ()).is_none() {
                return Poll::Ready(Err(IoUringError::InvalidRing));
            }
            let sqe = Sqe::accept(
                self.fd,
                self.addr_va,
                self.addrlen_va,
                self.flags,
                self.token,
            );
            self.result = Some(handle(&sqe));
        }

        let result = self.result.unwrap();
        Poll::Ready(if result < 0 { Err(IoUringError::OsError(result)) } else { Ok(result) })
    }
}
