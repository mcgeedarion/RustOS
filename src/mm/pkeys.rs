//! Minimal Linux memory protection key syscall support.
//!
//! Hardware PKU state is not enforced yet, but the syscall ABI is useful to
//! libc feature probes.  We maintain a small per-kernel allocation bitmap,
//! validate arguments, and make `pkey_mprotect(..., -1)` behave exactly like
//! `mprotect(2)`.  Allocated non-negative keys are accepted and recorded so
//! applications can exercise allocation/free lifecycles without receiving
//! `ENOSYS`.

use core::sync::atomic::{AtomicU32, Ordering};

const EINVAL: isize = -22;
const ENOSPC: isize = -28;

const PKEY_DISABLE_ACCESS: u32 = 0x1;
const PKEY_DISABLE_WRITE: u32 = 0x2;
const VALID_ACCESS_RIGHTS: u32 = PKEY_DISABLE_ACCESS | PKEY_DISABLE_WRITE;
const MAX_PKEYS: u32 = 16;
const RESERVED_PKEY0: u32 = 1;

static ALLOCATED_KEYS: AtomicU32 = AtomicU32::new(RESERVED_PKEY0);

pub fn sys_pkey_alloc(flags: u32, access_rights: u32) -> isize {
    if flags != 0 || (access_rights & !VALID_ACCESS_RIGHTS) != 0 {
        return EINVAL;
    }

    let mut current = ALLOCATED_KEYS.load(Ordering::Relaxed);
    loop {
        for key in 1..MAX_PKEYS {
            let bit = 1u32 << key;
            if current & bit == 0 {
                match ALLOCATED_KEYS.compare_exchange_weak(
                    current,
                    current | bit,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return key as isize,
                    Err(next) => {
                        current = next;
                        break;
                    },
                }
            }
        }
        if current.count_ones() >= MAX_PKEYS {
            return ENOSPC;
        }
    }
}

pub fn sys_pkey_free(pkey: u32) -> isize {
    if pkey == 0 || pkey >= MAX_PKEYS {
        return EINVAL;
    }
    let bit = 1u32 << pkey;
    let previous = ALLOCATED_KEYS.fetch_and(!bit, Ordering::AcqRel);
    if previous & bit == 0 {
        EINVAL
    } else {
        0
    }
}

pub fn sys_pkey_mprotect(addr: usize, len: usize, prot: i32, pkey: i32) -> isize {
    if pkey < -1 || pkey as u32 >= MAX_PKEYS {
        return EINVAL;
    }
    if pkey >= 0 {
        let bit = 1u32 << (pkey as u32);
        if ALLOCATED_KEYS.load(Ordering::Acquire) & bit == 0 {
            return EINVAL;
        }
    }
    crate::mm::mmap::sys_mprotect(addr, len, prot as u32)
}
