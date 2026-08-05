//! `process_vm_readv(2)` / `process_vm_writev(2)` support.
//!
//! The fully general Linux ABI copies between two distinct process address
//! spaces after ptrace-style credential checks.  RustOS does not yet expose a
//! stable remote-mm pinning API, so this module implements the safe and useful
//! same-process case and rejects true cross-process access with `EPERM` rather
//! than advertising an unimplemented syscall.  The implementation still performs
//! the ABI validation Linux callers expect: flags must be zero, iovec arrays and
//! pointed-to buffers must be valid userspace ranges, and byte counts saturate at
//! `isize::MAX`.

extern crate alloc;

use alloc::vec::Vec;
use core::cmp::min;
use core::mem::size_of;

const EFAULT: isize = -14;
const EINVAL: isize = -22;
const EPERM: isize = -1;

const IOV_MAX: usize = 1024;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct UserIovec {
    base: usize,
    len: usize,
}

fn read_iovecs(iov_va: usize, count: usize) -> Result<Vec<UserIovec>, isize> {
    if count > IOV_MAX {
        return Err(EINVAL);
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    if iov_va == 0 {
        return Err(EFAULT);
    }

    let bytes = count.checked_mul(size_of::<UserIovec>()).ok_or(EINVAL)?;
    let mut iovecs = Vec::with_capacity(count);
    unsafe { iovecs.set_len(count) };
    crate::uaccess::copy_from_user(iovecs.as_mut_ptr() as *mut u8, iov_va, bytes)
        .map_err(|_| EFAULT)?;

    let mut total = 0usize;
    for iov in &iovecs {
        total = total.checked_add(iov.len).ok_or(EINVAL)?;
        if total > isize::MAX as usize {
            return Err(EINVAL);
        }
        if iov.len != 0 && !crate::uaccess::validate_user_ptr(iov.base, iov.len) {
            return Err(EFAULT);
        }
    }
    Ok(iovecs)
}

fn current_only(pid: usize) -> Result<(), isize> {
    let current = crate::proc::scheduler::current_pid() as usize;
    if pid == current {
        Ok(())
    } else if crate::proc::scheduler::with_proc(pid, |_| ()).is_some() {
        Err(EPERM)
    } else {
        Err(-3) // ESRCH
    }
}

fn copy_between_iovecs(dst: &[UserIovec], src: &[UserIovec]) -> Result<isize, isize> {
    let mut copied = 0usize;
    let mut di = 0usize;
    let mut si = 0usize;
    let mut doff = 0usize;
    let mut soff = 0usize;
    let mut bounce = [0u8; 256];

    while di < dst.len() && si < src.len() {
        if doff == dst[di].len {
            di += 1;
            doff = 0;
            continue;
        }
        if soff == src[si].len {
            si += 1;
            soff = 0;
            continue;
        }

        let n = min(min(dst[di].len - doff, src[si].len - soff), bounce.len());
        if n == 0 {
            break;
        }
        crate::uaccess::copy_from_user(bounce.as_mut_ptr(), src[si].base + soff, n)
            .map_err(|_| EFAULT)?;
        crate::uaccess::copy_to_user(dst[di].base + doff, bounce.as_ptr(), n)
            .map_err(|_| EFAULT)?;
        copied += n;
        doff += n;
        soff += n;
    }

    Ok(copied as isize)
}

pub fn sys_process_vm_readv(
    pid: usize,
    local_iov: usize,
    liovcnt: usize,
    remote_iov: usize,
    riovcnt: usize,
    flags: u32,
) -> isize {
    if flags != 0 {
        return EINVAL;
    }
    if let Err(e) = current_only(pid) {
        return e;
    }
    let local = match read_iovecs(local_iov, liovcnt) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let remote = match read_iovecs(remote_iov, riovcnt) {
        Ok(v) => v,
        Err(e) => return e,
    };
    copy_between_iovecs(&local, &remote).unwrap_or_else(|e| e)
}

pub fn sys_process_vm_writev(
    pid: usize,
    local_iov: usize,
    liovcnt: usize,
    remote_iov: usize,
    riovcnt: usize,
    flags: u32,
) -> isize {
    if flags != 0 {
        return EINVAL;
    }
    if let Err(e) = current_only(pid) {
        return e;
    }
    let local = match read_iovecs(local_iov, liovcnt) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let remote = match read_iovecs(remote_iov, riovcnt) {
        Ok(v) => v,
        Err(e) => return e,
    };
    copy_between_iovecs(&remote, &local).unwrap_or_else(|e| e)
}
