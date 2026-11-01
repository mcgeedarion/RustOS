// src/io_uring/ops/write.rs

use crate::io_uring::{cqe::errno, sqe::Sqe};

/// Synchronous kernel-side handler for IORING_OP_WRITE.
pub fn handle(sqe: &Sqe) -> i32 {
    let fd = sqe.fd;
    let buf_va = sqe.addr;
    let len = sqe.len as usize;
    let offset = sqe.off;

    if fd < 0 {
        log::warn!("[io_uring::write] invalid fd={}", fd);
        return errno::E_BADF;
    }
    if buf_va == 0 {
        log::warn!("[io_uring::write] null buffer pointer fd={}", fd);
        return errno::E_INVAL;
    }
    if len == 0 {
        return 0;
    }

    #[cfg(feature = "boot_debug")]
    log::trace!(
        "[io_uring::write] fd={} buf={:#x} len={} off={} token={:#x}",
        fd,
        buf_va,
        len,
        offset,
        sqe.user_data
    );

    // Core write dispatch now routes through shared VFS/io syscalls.
    if sqe.op_flags != 0 { return -95; }
    if offset == u64::MAX {
        crate::fs::io_syscalls::sys_write(fd as usize, buf_va as usize, len) as i32
    } else {
        crate::fs::io_syscalls::sys_pwrite64(fd as usize, buf_va as usize, len, offset as i64) as i32
    }
}

use crate::io_uring::IoUringError;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Async wrapper around IORING_OP_WRITE.
pub struct IoWrite<'a> {
    fd: i32,
    buf: &'a [u8],
    offset: u64,
    token: u64,
    result: Option<i32>,
}

impl<'a> IoWrite<'a> {
    pub fn new(fd: i32, buf: &'a [u8], offset: u64, token: u64) -> Self {
        IoWrite {
            fd,
            buf,
            offset,
            token,
            result: None,
        }
    }
}

impl<'a> Future for IoWrite<'a> {
    type Output = Result<usize, IoUringError>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.result.is_none() {
            let sqe = crate::io_uring::sqe::Sqe::write(
                self.fd,
                self.buf.as_ptr() as u64,
                self.buf.len() as u32,
                self.offset,
                self.token,
            );
            self.result = Some(handle(&sqe));
        }

        let result = self.result.unwrap();
        Poll::Ready(if result < 0 { Err(IoUringError::OsError(result)) } else { Ok(result as usize) })
    }
}
