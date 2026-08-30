//! wait4 / waitpid / waitid — reap zombie children, return exit status.
//!
//! ## Exit-status encoding (Linux ABI, POSIX §3.1)
//!
//! | Condition              | wstatus bits                        |
//! |------------------------|-------------------------------------|
//! | Normal exit(code)      | `(code & 0xff) << 8`                |
//! | Signal termination     | `signum & 0x7f`                     |
//! | Core dump              | `signum & 0x7f | 0x80`              |
//! | Stopped (WIFSTOPPED)   | `(stopsig & 0xff) << 8 | 0x7f`     |
//! | Continued (WIFCONTINUED)| `0xffff`                           |
//!
//! `WIFEXITED(s) = (s & 0x7f) == 0`
//! `WEXITSTATUS(s) = (s >> 8) & 0xff`
//! `WIFSIGNALED(s) = ((s & 0x7f) > 0) && ((s & 0x7f) != 0x7f)`
//! `WTERMSIG(s) = s & 0x7f`
//! `WIFSTOPPED(s) = (s & 0xff) == 0x7f`
//! `WSTOPSIG(s) = (s >> 8) & 0xff`
//! `WIFCONTINUED(s) = (s & 0xffff) == 0xffff`

extern crate alloc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::proc::process::State;
use crate::proc::{scheduler, signal};
use crate::uaccess::copy_to_user;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Accepted inside the `options` bitfield of waitpid / wait4.
pub const WNOHANG: i32 = 1;
pub const WUNTRACED: i32 = 2;
pub const WCONTINUED: i32 = 8;

/// idtype values for waitid(2).
pub const P_ALL: i32 = 0;
pub const P_PID: i32 = 1;
pub const P_PGID: i32 = 2;
pub const P_PIDFD: i32 = 3; // Linux 5.4+

/// options for waitid(2).
pub const WEXITED: i32 = 1;
pub const WSTOPPED: i32 = 2;
pub const WNOWAIT: i32 = 0x10000000;

// ---------------------------------------------------------------------------
// Exit-status helpers
// ---------------------------------------------------------------------------

/// Encode a normal exit code into `wstatus`.
/// Handles both signal-termination codes (sign-extended negative, low 7 bits
/// already set by `apply_default`) and ordinary `exit(code)` codes.
#[inline]
pub fn encode_exit(code: i32) -> i32 {
    // Signal-terminated: low 7 bits are the signal number (set by
    // `encode_signal_status` in signal.rs).
    if code < 0 {
        // Preserve signal number in low 7 bits (flip sign).
        (-code) & 0x7f
    } else {
        // Normal exit: shift exit status into high byte.
        (code & 0xff) << 8
    }
}

// ---------------------------------------------------------------------------
// Zombie notification (called from exit path)
// ---------------------------------------------------------------------------

/// Wake the parent so it can call waitpid() and collect the zombie.
pub fn notify_exit(child_pid: usize) {
    let (ppid, exit_code) =
        scheduler::with_proc(child_pid, |p| (p.ppid, p.exit_code)).unwrap_or((0, 0));

    if ppid == 0 {
        return;
    }

    // Send SIGCHLD to parent so it wakes from sigsuspend / pause.
    signal::send_sigchld(ppid, child_pid, exit_code);

    // Explicitly wake the parent task in case it is blocked in wait4.
    scheduler::wake_pid(ppid);
}

// ---------------------------------------------------------------------------
// sys_waitpid / sys_wait4
// ---------------------------------------------------------------------------

/// Collect the first zombie child that matches `target_pid`:
///   * `target_pid > 0` : wait for that specific child.
///   * `target_pid == -1`: wait for any child.
///   * `target_pid == 0` : wait for any child in the caller's process group.
///   * `target_pid < -1` : wait for any child in pgrp |target_pid|.
pub fn sys_waitpid(target_pid: isize, wstatus_va: usize, options: i32) -> isize {
    sys_wait4(target_pid, wstatus_va, options, 0 /* no rusage */)
}

pub fn sys_wait4(target_pid: isize, wstatus_va: usize, options: i32, rusage_va: usize) -> isize {
    let caller = scheduler::current_pid();
    let caller_pgid = scheduler::with_proc(caller, |p| p.pgid).unwrap_or(caller);

    loop {
        // Build list of eligible children.
        let eligible: Vec<usize> = scheduler::with_procs_ro(|pl| {
            pl.iter()
                .filter(|p| {
                    let is_child = p.ppid as usize == caller;
                    if !is_child {
                        return false;
                    }
                    match target_pid {
                        -1 => true,
                        0 => p.pgid as usize == caller_pgid,
                        n if n > 0 => p.pid as usize == n as usize,
                        n => p.pgid as usize == (-n) as usize,
                    }
                })
                .map(|p| p.pid as usize)
                .collect()
        });

        if eligible.is_empty() {
            return -10; // ECHILD — no matching children exist at all
        }

        // Look for a zombie among eligible children.
        let zombie = eligible.iter().find(|&&cpid| {
            scheduler::with_proc(cpid, |p| p.load_state() == State::Zombie).unwrap_or(false)
        });

        if let Some(&cpid) = zombie {
            let exit_code = scheduler::with_proc(cpid, |p| p.exit_code).unwrap_or(0);

            // Write wstatus if user pointer is non-null.  Do this before
            // reaping the zombie so an EFAULT leaves the child waitable for a
            // retry with a valid status pointer.
            if wstatus_va != 0 {
                if copy_to_user(wstatus_va, &(exit_code as u32).to_ne_bytes()).is_err() {
                    return -14; // EFAULT
                }
            }

            // Write rusage if requested.  As with wstatus, preserve the zombie
            // if the user buffer is invalid so callers do not lose child state
            // because of a bad pointer.
            if rusage_va != 0 {
                // Minimal rusage: zeroed struct.
                let zeroes = [0u8; 144]; // sizeof(struct rusage) on x86_64
                if copy_to_user(rusage_va, &zeroes).is_err() {
                    return -14; // EFAULT
                }
            }

            // Remove from the process table.
            scheduler::remove_proc(cpid);

            return cpid as isize;
        }

        // No zombie found.
        if options & WNOHANG != 0 {
            return 0; // Caller asked not to block.
        }

        // Block until a child exits and sends SIGCHLD / wakes us.
        scheduler::sleep_current();
        // Check for EINTR (pending signal was delivered while sleeping).
        if signal::has_pending_signal(caller) {
            return -4; // EINTR
        }
    }
}

// ---------------------------------------------------------------------------
// waitid(2) — POSIX wait with siginfo_t output
// ---------------------------------------------------------------------------

/// siginfo_t layout for waitid (simplified).
#[repr(C)]
pub struct SigInfoWait {
    si_signo: i32,
    si_errno: i32,
    si_code: i32,
    si_pid: i32,
    si_uid: u32,
    si_status: i32,
    si_utime: i64,
    si_stime: i64,
}

impl Default for SigInfoWait {
    fn default() -> Self {
        Self {
            si_signo: 17, // SIGCHLD
            si_errno: 0,
            si_code: 0,
            si_pid: 0,
            si_uid: 0,
            si_status: 0,
            si_utime: 0,
            si_stime: 0,
        }
    }
}

/// waitid(idtype, id, infop, options) — POSIX wait with fine-grained control.
///
/// `idtype` specifies how to interpret `id`:
///   - P_ALL: wait for any child (id is ignored)
///   - P_PID: wait for child with PID == id
///   - P_PGID: wait for child with PGID == id
///   - P_PIDFD: wait for child referenced by pidfd (not yet implemented)
///
/// `options` can include:
///   - WEXITED: wait for exited children
///   - WSTOPPED: wait for stopped children (requires WUNTRACED semantics)
///   - WCONTINUED: wait for continued children
///   - WNOHANG: return immediately if no child is available
///   - WNOWAIT: leave the child in a waitable state (do not reap)
///
/// Returns 0 on success, or negative errno.
pub fn sys_waitid(idtype: i32, id: i32, infop: usize, options: i32) -> isize {
    let caller = scheduler::current_pid();
    let caller_pgid = scheduler::with_proc(caller, |p| p.pgid).unwrap_or(caller);

    // Validate options: at least one of WEXITED, WSTOPPED, WCONTINUED must be set.
    let valid_opts = options & (WEXITED | WSTOPPED | WCONTINUED);
    if valid_opts == 0 {
        return -22; // EINVAL
    }

    loop {
        // Build list of eligible children based on idtype.
        let eligible: Vec<usize> = scheduler::with_procs_ro(|pl| {
            pl.iter()
                .filter(|p| {
                    let is_child = p.ppid as usize == caller;
                    if !is_child {
                        return false;
                    }
                    match idtype {
                        P_ALL => true,
                        P_PID => p.pid as i32 == id,
                        P_PGID => p.pgid as i32 == id,
                        P_PIDFD => false, // Not implemented
                        _ => false,
                    }
                })
                .map(|p| p.pid as usize)
                .collect()
        });

        if eligible.is_empty() {
            return -10; // ECHILD
        }

        // Look for a matching child based on options.
        let mut found_cpid: Option<usize> = None;
        let mut found_state: Option<State> = None;
        let mut found_exit_code: i32 = 0;

        for &cpid in &eligible {
            let state = scheduler::with_proc(cpid, |p| p.load_state()).unwrap_or(State::Blocked);
            let exit_code = scheduler::with_proc(cpid, |p| p.exit_code).unwrap_or(0);

            match state {
                State::Zombie if (options & WEXITED) != 0 => {
                    found_cpid = Some(cpid);
                    found_state = Some(state);
                    found_exit_code = exit_code;
                    break;
                }
                State::Stopped if (options & WSTOPPED) != 0 => {
                    found_cpid = Some(cpid);
                    found_state = Some(state);
                    found_exit_code = exit_code;
                    break;
                }
                State::Continued if (options & WCONTINUED) != 0 => {
                    found_cpid = Some(cpid);
                    found_state = Some(state);
                    found_exit_code = exit_code;
                    break;
                }
                _ => {}
            }
        }

        if let Some(cpid) = found_cpid {
            let state = found_state.unwrap();
            let exit_code = found_exit_code;

            // Write siginfo_t if user pointer is non-null.
            if infop != 0 {
                let mut info = SigInfoWait::default();
                info.si_pid = cpid as i32;

                match state {
                    State::Zombie => {
                        info.si_code = if exit_code >= 0 {
                            CLD_EXITED
                        } else {
                            CLD_KILLED
                        };
                        info.si_status = exit_code;
                    }
                    State::Stopped => {
                        info.si_code = CLD_STOPPED;
                        info.si_status = (exit_code >> 8) & 0xff;
                    }
                    State::Continued => {
                        info.si_code = CLD_CONTINUED;
                        info.si_status = 0;
                    }
                    _ => {}
                }

                if copy_to_user(infop, unsafe {
                    core::slice::from_raw_parts(
                        &info as *const _ as *const u8,
                        core::mem::size_of::<SigInfoWait>(),
                    )
                })
                .is_err()
                {
                    return -14; // EFAULT
                }
            }

            // If WNOWAIT is set, do not reap the child.
            if (options & WNOWAIT) == 0 && state == State::Zombie {
                scheduler::remove_proc(cpid);
            }

            return 0;
        }

        // No matching child found.
        if options & WNOHANG != 0 {
            return 0;
        }

        // Block until a child changes state.
        scheduler::sleep_current();
        if signal::has_pending_signal(caller) {
            return -4; // EINTR
        }
    }
}

// CLD_* codes for siginfo_t si_code field.
const CLD_EXITED: i32 = 1;
const CLD_KILLED: i32 = 2;
const CLD_STOPPED: i32 = 3;
const CLD_CONTINUED: i32 = 4;
