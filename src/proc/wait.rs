//! wait4 / waitpid — reap zombie children, return exit status.
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
