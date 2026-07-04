//! signalfd support — fd registry, read, poll, and close helpers.
//!
//! RustOS stores pending signals in the process signal layer.  This module owns
//! the descriptor state (the accepted signal mask plus open flags) and turns
//! matching pending `SigInfo` entries into Linux-compatible 128-byte
//! `signalfd_siginfo` records when the descriptor is read.

use crate::core::fast_hash::KernelFastMap;
use spin::Mutex;

const EAGAIN: isize = -11;
const EBADF: isize = -9;
const EFAULT: isize = -14;
const EINVAL: isize = -22;
const EMFILE: isize = -24;

const SFD_CLOEXEC: u32 = 0o2000000;
const SFD_NONBLOCK: u32 = 0o4000;
const VALID_FLAGS: u32 = SFD_CLOEXEC | SFD_NONBLOCK;
const SIGSET_BYTES: usize = 8;
const SIGNALFD_SIGINFO_SIZE: usize = 128;

#[derive(Clone, Copy)]
struct SignalFd {
    mask: u64,
    nonblock: bool,
    refs: usize,
}

/// Fast map is safe here: keys are kernel-assigned fd numbers and iteration
/// order is never exposed as an ABI.
static SIGNALFDS: Mutex<KernelFastMap<usize, SignalFd>> = Mutex::new(KernelFastMap::new());

fn read_mask(mask_va: usize, sigset_size: usize) -> Result<u64, isize> {
    if mask_va == 0 || sigset_size != SIGSET_BYTES {
        return Err(EINVAL);
    }
    let mut mask = 0u64;
    crate::uaccess::copy_from_user(&mut mask as *mut u64 as *mut u8, mask_va, SIGSET_BYTES)
        .map_err(|_| EFAULT)?;
    // Linux silently ignores SIGKILL/SIGSTOP in signalfd masks because those
    // signals cannot be blocked or consumed through signalfd.
    mask &= !(1u64 << (9 - 1));
    mask &= !(1u64 << (19 - 1));
    Ok(mask)
}

fn encode_siginfo(info: crate::proc::signal::SigInfo, out: &mut [u8]) {
    out[..SIGNALFD_SIGINFO_SIZE].fill(0);
    out[0..4].copy_from_slice(&info.sig.to_ne_bytes());
    out[4..8].copy_from_slice(&(0i32).to_ne_bytes()); // ssi_errno
    out[8..12].copy_from_slice(&info.code.to_ne_bytes());
    out[12..16].copy_from_slice(&info.pid.to_ne_bytes());
    out[16..20].copy_from_slice(&info.uid.to_ne_bytes());
    out[20..24].copy_from_slice(&(0i32).to_ne_bytes()); // ssi_fd
    out[24..28].copy_from_slice(&(0u32).to_ne_bytes()); // ssi_tid
    out[28..32].copy_from_slice(&(0u32).to_ne_bytes()); // ssi_band
    out[32..36].copy_from_slice(&(0u32).to_ne_bytes()); // ssi_overrun
    out[36..40].copy_from_slice(&(0u32).to_ne_bytes()); // ssi_trapno
    out[40..44].copy_from_slice(&info.status.to_ne_bytes());
    out[48..56].copy_from_slice(&(info.addr as u64).to_ne_bytes());
    out[56..64].copy_from_slice(&info.value.to_ne_bytes());
}

/// Attach signalfd state to an already-open anonymous VFS fd.
pub fn signalfd_register(fd: usize, mask: u64, flags: u32) {
    SIGNALFDS.lock().insert(
        fd,
        SignalFd {
            mask,
            nonblock: flags & SFD_NONBLOCK != 0,
            refs: 1,
        },
    );
}

/// Returns true when `fd` is a registered signalfd backing fd.
pub fn is_signalfd(fd: usize) -> bool {
    SIGNALFDS.lock().contains_key(&fd)
}

/// Close and unregister a signalfd backing fd.
pub fn sys_close_sfd(fd: usize) {
    let should_close = {
        let mut map = SIGNALFDS.lock();
        match map.get_mut(&fd) {
            Some(sfd) if sfd.refs > 1 => {
                sfd.refs -= 1;
                false
            },
            Some(_) => {
                map.remove(&fd);
                true
            },
            None => false,
        }
    };
    if should_close {
        let _ = crate::fs::vfs::close(fd);
    }
}

/// Duplicate hook for process-local fd aliases. State is shared by backing fd.
pub fn sfd_dup(fd: usize) {
    if let Some(sfd) = SIGNALFDS.lock().get_mut(&fd) {
        sfd.refs = sfd.refs.saturating_add(1);
    }
}

pub fn sys_signalfd(oldfd: isize, mask_va: usize, sigset_size: usize) -> isize {
    sys_signalfd4(oldfd, mask_va, sigset_size, 0)
}

pub fn sys_signalfd4(oldfd: isize, mask_va: usize, sigset_size: usize, flags: u32) -> isize {
    if flags & !VALID_FLAGS != 0 {
        return EINVAL;
    }
    let mask = match read_mask(mask_va, sigset_size) {
        Ok(mask) => mask,
        Err(e) => return e,
    };
    if oldfd >= 0 {
        let fd = oldfd as usize;
        let mut map = SIGNALFDS.lock();
        let sfd = match map.get_mut(&fd) {
            Some(sfd) => sfd,
            None => return EBADF,
        };
        sfd.mask = mask;
        sfd.nonblock = flags & SFD_NONBLOCK != 0;
        return fd as isize;
    }

    let open_flags = if flags & SFD_CLOEXEC != 0 {
        crate::fs::vfs::O_CLOEXEC
    } else {
        0
    };
    let fd = match crate::fs::vfs::open_anon(open_flags) {
        Ok(fd) => fd,
        Err(_) => return EMFILE,
    };
    signalfd_register(fd, mask, flags);
    fd as isize
}

/// Read one or more 128-byte signalfd_siginfo records.
pub fn signalfd_read(fd: usize, buf: &mut [u8]) -> isize {
    if buf.len() < SIGNALFD_SIGINFO_SIZE {
        return EINVAL;
    }
    let state = match SIGNALFDS.lock().get(&fd).copied() {
        Some(state) => state,
        None => return EBADF,
    };

    let tid = crate::proc::scheduler::current_pid() as usize;
    let mut written = 0usize;
    while written + SIGNALFD_SIGINFO_SIZE <= buf.len() {
        let info = match crate::proc::signal::take_pending_matching(tid, state.mask) {
            Some(info) => info,
            None => break,
        };
        encode_siginfo(info, &mut buf[written..written + SIGNALFD_SIGINFO_SIZE]);
        written += SIGNALFD_SIGINFO_SIZE;
    }

    if written == 0 {
        if state.nonblock {
            EAGAIN
        } else {
            // Wait-queue integration is not available here yet; report EAGAIN
            // rather than blocking forever in the syscall path.
            EAGAIN
        }
    } else {
        written as isize
    }
}

/// Poll readiness bitmask.  Bit 0 = POLLIN when a masked signal is pending.
pub fn signalfd_poll(fd: usize) -> u32 {
    let state = match SIGNALFDS.lock().get(&fd) {
        Some(state) => *state,
        None => return 0,
    };
    let tid = crate::proc::scheduler::current_pid() as usize;
    if crate::proc::signal::has_pending_matching(tid, state.mask) {
        0x001
    } else {
        0
    }
}
