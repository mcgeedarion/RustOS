// src/io_uring/ops/read.rs

use crate::io_uring::{cqe::errno, sqe::Sqe};

/// Synchronous kernel-side handler for IORING_OP_READ.
pub fn handle(sqe: &Sqe) -> i32 {
    let fd = sqe.fd;
    let buf_va = sqe.addr;
    let len = sqe.len as usize;
    let offset = sqe.off;

    if fd < 0 {
        log::warn!("[io_uring::read] invalid fd={}", fd);
        return errno::E_BADF;
    }
    if buf_va == 0 {
        log::warn!("[io_uring::read] null buffer pointer fd={}", fd);
        return errno::E_INVAL;
    }
    if len == 0 {
        // Zero-length read is valid and returns 0.
        return 0;
    }

    #[cfg(feature = "boot_debug")]
    log::trace!(
        "[io_uring::read] fd={} buf={:#x} len={} off={} token={:#x}",
        fd,
        buf_va,
        len,
        offset,
        sqe.user_data
    );

    // Core read dispatch now routes through the shared VFS/io path so io_uring
    // read semantics match synchronous read(2).
    if sqe.op_flags != 0 { return -95; }
    if offset == u64::MAX {
        crate::fs::io_syscalls::sys_read(fd as usize, buf_va as usize, len) as i32
    } else {
        crate::fs::io_syscalls::sys_pread64(fd as usize, buf_va as usize, len, offset as i64) as i32
    }
}

// Callers should use `IoRead` instead of submitting SQEs directly.
// It encapsulates the submit → poll → wake cycle.

use crate::io_uring::IoUringError;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Async wrapper around IORING_OP_READ.
pub struct IoRead<'a> {
    fd: i32,
    buf: &'a mut [u8],
    offset: u64,
    token: u64,
    result: Option<i32>,
}

impl<'a> IoRead<'a> {
    pub fn new(fd: i32, buf: &'a mut [u8], offset: u64, token: u64) -> Self {
        IoRead {
            fd,
            buf,
            offset,
            token,
            result: None,
        }
    }
}

impl<'a> Future for IoRead<'a> {
    type Output = Result<usize, IoUringError>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.result.is_none() {
            let sqe = crate::io_uring::sqe::Sqe::read(
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
