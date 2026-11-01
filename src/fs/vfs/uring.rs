//! VFS helpers required exclusively by io_uring.

extern crate alloc;
use alloc::vec::Vec;
use spin::Mutex;

// We reserve fd numbers in [URING_FD_BASE, URING_FD_BASE + MAX_URING_FDS) for
// io_uring instances.  The number stored is the ring_idx into RING_TABLE.

const URING_FD_BASE: usize = 0x5000_0000;
const MAX_URING_FDS: usize = 256;

static URING_FD_TABLE: Mutex<[Option<usize>; MAX_URING_FDS]> = Mutex::new([None; MAX_URING_FDS]);

/// Allocate a new fd that refers to `ring_idx`.  Returns -ENFILE on overflow.
pub fn alloc_fd_for_uring(ring_idx: usize) -> Option<usize> {
    let mut tbl = URING_FD_TABLE.lock();
    for (i, slot) in tbl.iter_mut().enumerate() {
        if slot.is_none() {
            *slot = Some(ring_idx);
            return Some(URING_FD_BASE + i);
        }
    }
    None
}

/// Return the ring_idx associated with `fd`, if it is an io_uring fd.
pub fn uring_fd_to_ring(fd: usize) -> Option<usize> {
    if fd < URING_FD_BASE || fd >= URING_FD_BASE + MAX_URING_FDS {
        return None;
    }
    URING_FD_TABLE.lock()[fd - URING_FD_BASE]
}

/// Release an io_uring fd (called from the close path).
pub fn close_uring_fd(fd: usize) -> bool {
    if fd < URING_FD_BASE || fd >= URING_FD_BASE + MAX_URING_FDS {
        return false;
    }
    let mut tbl = URING_FD_TABLE.lock();
    if tbl[fd - URING_FD_BASE].is_some() {
        let index = tbl[fd - URING_FD_BASE].take().unwrap();
        drop(tbl);
        crate::io_uring::ring::free_ring(index);
        true
    } else {
        false
    }
}

/// Returns true if `fd` is an io_uring fd.
pub fn is_uring_fd(fd: usize) -> bool {
    uring_fd_to_ring(fd).is_some()
}

/// Flush dirty data for `fd` to backing storage.
/// Delegates to the underlying VFS fd_sync if available; otherwise a no-op
/// (returning 0) for in-memory filesystems where flushing is meaningless.
pub fn fsync(fd: usize) -> isize {
    crate::fs::fcntl::fd_sync(fd)
}

/// Positional read directly into a kernel slice — no user-space copy.
/// Used by io_uring READ/READ_FIXED where the destination buffer is
/// kernel-mapped registered memory.
pub fn pread_buf(fd: usize, buf: &mut [u8], offset: i64) -> isize {
    crate::fs::vfs::pread_buf(fd, buf, offset)
}

/// Positional write from a kernel slice — no user-space copy.
pub fn pwrite_buf(fd: usize, buf: &[u8], offset: i64) -> isize {
    crate::fs::vfs::pwrite_buf(fd, buf, offset)
}
