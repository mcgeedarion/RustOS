//! pidfd — process file descriptors (Linux 5.3+, extended in 5.6+).
//!
//! A pidfd is a stable, race-free handle to a process.  Unlike raw PIDs
//! it cannot be recycled: if the process exits the fd remains valid and
//! operations return ESRCH rather than silently targeting a new process.
//!
//! ## Syscalls implemented
//!   pidfd_open(pid, flags)               [NR 434] -> pidfd
//!   pidfd_send_signal(pidfd, sig, info, flags) [NR 424] -> 0/-errno
//!   pidfd_getfd(pidfd, targetfd, flags)  [NR 438] -> new_fd/-errno
//!
//! ## Scheme integration
//!
//! `sys_pidfd_open` now allocates a scheme backing fd via
//! `alloc_scheme_backing_fd` and registers a `PidFdScheme` in
//! `SCHEME_FD_STORE`.  The scheme backing fd is installed into the process
//! fd table; the raw PIDFD_TABLE entry is still inserted first so that
//! `is_pidfd()` and `resolve()` never see a gap.
//!
//! `sys_pidfd_send_signal` and `sys_pidfd_getfd` receive the scheme backing
//! fd (resolved from the user fd by the syscall dispatch) and use
//! `scheme_bfd_to_table_fdno` to recover the raw PIDFD_TABLE key.

extern crate alloc;
use crate::core::fast_hash::KernelFastMap;
use alloc::sync::Arc;
use alloc::collections::BTreeMap;
use spin::Mutex;

use crate::fs::scheme_table::Scheme;
use scheme_api::{OpenFlags, SchemeError, SchemeFileId};

const PIDFD_NONBLOCK: u32 = 0x800;
const SIGKILL: u32 = 9;
const SIGSTOP: u32 = 19;
const SIGCONT: u32 = 18;

// Raw PIDFD_TABLE fdno namespace — still used by is_pidfd / resolve.
const PID_FD_BASE: usize = 1024;
const PID_FD_MAX: usize = 2048;

/// Maps raw PIDFD_TABLE fdno -> target pid.
/// Fast map is safe here: keys are bounded raw pidfd table fd numbers assigned
/// by the kernel and are not exposed through deterministic iteration.
static PIDFD_TABLE: Mutex<KernelFastMap<usize, Arc<crate::proc::process::ProcLock>>> =
    Mutex::new(KernelFastMap::new());
static PIDFD_BACKINGS: Mutex<BTreeMap<usize, usize>> = Mutex::new(BTreeMap::new());
/// Pending signal queue: maps pid -> pending signal (one slot; SIGKILL wins).
/// Fast map is safe here: keys are kernel pid values and this one-slot pending
/// signal cache is not an authorization or ordering boundary.
static PENDING_SIGNAL: Mutex<KernelFastMap<usize, u32>> = Mutex::new(KernelFastMap::new());

/// Translate a scheme backing fd to the PIDFD_TABLE fdno.
/// Returns None if `scheme_bfd` is not a registered pidfd scheme fd.
pub fn scheme_bfd_to_table_fdno(scheme_bfd: usize) -> Option<usize> {
    PIDFD_BACKINGS.lock().get(&scheme_bfd).copied()
}

pub struct PidFdScheme;

impl Scheme for PidFdScheme {
    fn poll_source(&self, fid: SchemeFileId) -> Option<Arc<dyn crate::sync::poll_source::PollSource>> {
        is_pidfd(fid.0 as usize).then(|| crate::fs::poll::pidfd_source(fid.0 as usize))
    }
    fn open(&self, _url: &str, _flags: OpenFlags) -> Result<SchemeFileId, SchemeError> {
        Err(SchemeError::InvalidArg) // created via pidfd_open only
    }

    /// pidfds are not directly readable (poll for POLLIN signals child exit).
    fn read(&self, _fid: SchemeFileId, _buf: &mut [u8]) -> Result<usize, SchemeError> {
        Err(SchemeError::InvalidArg)
    }

    fn write(&self, _fid: SchemeFileId, _buf: &[u8]) -> Result<usize, SchemeError> {
        Err(SchemeError::InvalidArg)
    }

    fn seek(&self, _fid: SchemeFileId, _offset: i64, _whence: u8) -> Result<u64, SchemeError> {
        Err(SchemeError::InvalidArg)
    }

    fn ioctl(&self, _fid: SchemeFileId, _cmd: u64, _arg: usize) -> Result<usize, SchemeError> {
        Err(SchemeError::InvalidArg)
    }

    fn close(&self, fid: SchemeFileId) -> Result<(), SchemeError> {
        let table_fdno = fid.0 as usize;
        free(table_fdno);
        Ok(())
    }
}

/// Allocate a raw PIDFD_TABLE fdno for `pid`. Returns fdno or -EMFILE.
fn alloc_table_fdno(process: Arc<crate::proc::process::ProcLock>) -> Result<usize, isize> {
    let mut tbl = PIDFD_TABLE.lock();
    for fd in PID_FD_BASE..PID_FD_MAX {
        if !tbl.contains_key(&fd) {
            tbl.insert(fd, process);
            return Ok(fd);
        }
    }
    Err(-24) // EMFILE
}

/// Release a raw PIDFD_TABLE entry.
pub fn free(fd: usize) {
    PIDFD_TABLE.lock().remove(&fd);
    PIDFD_BACKINGS.lock().retain(|_, raw| *raw != fd);
}

/// True if `fd` is a live raw PIDFD_TABLE fdno.
pub fn is_pidfd(fd: usize) -> bool {
    PIDFD_TABLE.lock().contains_key(&fd)
}

pub fn pidfd_poll(fd: usize, events: u32) -> u32 {
    match PIDFD_TABLE.lock().get(&fd) {
        None => crate::fs::poll::POLLNVAL,
        Some(process) if process.load_state() == crate::proc::process::State::Zombie =>
            events & crate::fs::poll::POLLIN,
        _ => 0,
    }
}

/// Resolve raw PIDFD_TABLE fdno -> target pid.
/// Returns None if the entry is missing or the process is a zombie.
fn resolve(table_fdno: usize) -> Option<usize> {
    let process = PIDFD_TABLE.lock().get(&table_fdno)?.clone();
    let pid = process.pid as usize;
    let procs = crate::proc::scheduler::procs_lock();
    let alive = procs
        .iter()
        .any(|p| Arc::ptr_eq(p, &process) && p.load_state() != crate::proc::process::State::Zombie);
    crate::proc::scheduler::procs_unlock();
    if alive {
        Some(pid)
    } else {
        None
    }
}

pub fn sys_pidfd_open(pid: usize, flags: u32) -> isize {
    use crate::fs::process_fd::proc_fd_install;
    use crate::fs::scheme_fd::{alloc_scheme_backing_fd, scheme_fd_register};
    use alloc::sync::Arc;

    if flags & !(PIDFD_NONBLOCK) != 0 {
        return -22;
    } // EINVAL
    if pid == 0 {
        return -3;
    } // ESRCH

    let procs = crate::proc::scheduler::procs_lock();
    let process = procs.iter().find(|p| p.pid as usize == pid).cloned();
    crate::proc::scheduler::procs_unlock();
    let process = match process { Some(p) => p, None => return -3 };

    let table_fdno = match alloc_table_fdno(process) {
        Ok(f) => f,
        Err(e) => return e,
    };

    let scheme: Arc<dyn Scheme> = Arc::new(PidFdScheme);
    let scheme_bfd = alloc_scheme_backing_fd();
    scheme_fd_register(scheme_bfd, scheme, SchemeFileId(table_fdno as u64));
    PIDFD_BACKINGS.lock().insert(scheme_bfd, table_fdno);

    let pid_caller = crate::proc::scheduler::current_pid();
    let nonblock_fl = if flags & PIDFD_NONBLOCK != 0 {
        PIDFD_NONBLOCK
    } else {
        0
    };
    // FD_CLOEXEC flag value is 1 in the install flags convention.
    let user_fd = proc_fd_install(
        pid_caller,
        scheme_bfd,
        None,
        nonblock_fl | crate::fs::process_fd::O_CLOEXEC_FLAG,
        None,
    );
    user_fd as isize
}

// The syscall dispatch resolves the user-visible fd to a scheme bfd before
// calling this.  We translate to the PIDFD_TABLE fdno via
// scheme_bfd_to_table_fdno.

pub fn sys_pidfd_send_signal(scheme_bfd: usize, sig: u32, info_va: usize, flags: u32) -> isize {
    if flags != 0 || info_va != 0 {
        return -22;
    }
    if sig > 64 {
        return -22;
    }

    let table_fdno = match scheme_bfd_to_table_fdno(scheme_bfd) {
        Some(f) => f,
        None => return -9, // EBADF
    };
    let pid = match resolve(table_fdno) {
        Some(p) => p,
        None => return -3, // ESRCH
    };

    let caller = crate::proc::scheduler::current_pid();
    let creds = crate::proc::scheduler::with_proc(caller, |p|
        (p.uid, p.euid, p.sid, p.caps.has(crate::security::capset::cap::KILL)));
    let target = crate::proc::scheduler::with_proc(pid, |p| (p.uid, p.suid, p.sid));
    let allowed = match (creds, target) {
        (Some((uid, euid, sid, cap)), Some((tuid, suid, tsid))) =>
            cap || uid == tuid || uid == suid || euid == tuid || euid == suid
                || (sig == SIGCONT && sid == tsid),
        _ => return -3,
    };
    if !allowed { return -1; }
    if sig == 0 { return 0; }
    crate::proc::signal::send_signal_user(pid, sig as i32)
}

/// Check and consume one pending signal for `pid`.
pub fn check_pending_signal(pid: usize) -> u32 {
    PENDING_SIGNAL.lock().remove(&pid).unwrap_or(0)
}

pub fn sys_pidfd_getfd(scheme_bfd: usize, targetfd: usize, flags: u32) -> isize {
    if flags != 0 {
        return -22;
    }

    let table_fdno = match scheme_bfd_to_table_fdno(scheme_bfd) {
        Some(f) => f,
        None => return -9,
    };
    let target_pid = match resolve(table_fdno) {
        Some(p) => p,
        None => return -3,
    };

    let caller_pid = crate::proc::scheduler::current_pid();
    let has_ptrace = crate::proc::scheduler::with_proc(caller_pid, |p|
        p.caps.has(crate::security::capset::cap::SYS_PTRACE)).unwrap_or(false);
    if !has_ptrace && caller_pid != target_pid {
        return -1;
    } // EPERM

    crate::fs::process_fd::proc_fd_dup_into(target_pid, targetfd, caller_pid, 0, true)
}
