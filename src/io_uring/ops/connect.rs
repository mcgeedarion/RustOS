// src/io_uring/ops/connect.rs

use crate::io_uring::{cqe::errno, sqe::Sqe};

/// Synchronous kernel-side handler for IORING_OP_CONNECT.
pub fn handle(sqe: &Sqe) -> i32 {
    let sock_fd = sqe.fd;
    let addr_va = sqe.addr;
    let addrlen = sqe.off;

    if sock_fd < 0 {
        log::warn!("[io_uring::connect] invalid sock_fd={}", sock_fd);
        return errno::E_BADF;
    }
    if addr_va == 0 {
        log::warn!("[io_uring::connect] null sockaddr pointer");
        return errno::E_INVAL;
    }
    // Minimum sockaddr size: sa_family (u16) + at least 2 bytes of address.
    if addrlen < 4 || addrlen > u32::MAX as u64 {
        log::warn!("[io_uring::connect] addrlen={} too small", addrlen);
        return errno::E_INVAL;
    }

    #[cfg(feature = "boot_debug")]
    log::trace!(
        "[io_uring::connect] sock_fd={} addr={:#x} addrlen={} token={:#x}",
        sock_fd,
        addr_va,
        addrlen,
        sqe.user_data
    );

    crate::net::socket::core::sys_connect(sock_fd as usize, addr_va as usize, addrlen as u32) as i32
}

use crate::io_uring::IoUringError;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Async wrapper around IORING_OP_CONNECT.
pub struct IoConnect {
    fd: i32,
    addr_va: u64,
    addrlen: u32,
    token: u64,
    result: Option<i32>,
}

impl IoConnect {
    pub fn new(fd: i32, addr_va: u64, addrlen: u32, token: u64) -> Self {
        IoConnect {
            fd,
            addr_va,
            addrlen,
            token,
            result: None,
        }
    }
}

impl Future for IoConnect {
    type Output = Result<(), IoUringError>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.result.is_none() {
            let sqe =
                crate::io_uring::sqe::Sqe::connect(self.fd, self.addr_va, self.addrlen, self.token);
            self.result = Some(handle(&sqe));
        }

        let result = self.result.unwrap();
        Poll::Ready(if result < 0 { Err(IoUringError::OsError(result)) } else { Ok(()) })
    }
}
