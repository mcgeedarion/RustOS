//! Restartable-syscall bookkeeping.
//!
//! ## Protocol
//!
//!   1. A syscall that can block (nanosleep, clock_nanosleep, futex WAIT, …)
//!      detects EINTR from signal delivery.
//!   2. Before returning -EINTR to userspace it calls `set_restart` with the
//!      syscall number, the adjusted argument registers.
//!   3. `signal::check_and_deliver` inspects the pending signal's SA_RESTART
//!      flag.  If set, it calls `apply_restart(pid)` to replay the syscall.
//!   4. `gdbstub` calls `get_restart(pid)` on register-snapshot packets so
//!      that a stopped task reports the pre-restart register view.
//!
//! ## Cleanup
//!
//!   `clear_restart(pid)` is called from `exit::do_exit` and `exec::do_execve`
//!   so stale blocks never leak across process lifetimes.

extern crate alloc;
use alloc::collections::BTreeMap;
use spin::Mutex;

/// Frozen register state for a restartable syscall.
#[derive(Clone, Copy, Debug)]
pub struct RestartBlock {
    /// The syscall instruction address (PC before the +advance).
    /// Restoring this makes the CPU re-execute the syscall on return.
    pub sepc_ecall: usize,

    /// The original syscall number. Stored so the GDB register snapshot
    /// can report it correctly even if the live frame's syscall register
    /// was clobbered.
    pub nr: usize,

    /// Possibly-adjusted syscall arguments a0–a5.
    /// For nanosleep / clock_nanosleep these carry the *remaining* timeout so
    /// the restarted sleep is not longer than the original requested duration.
    pub a0: usize,
    pub a1: usize,
    pub a2: usize,
    pub a3: usize,
    pub a4: usize,
    pub a5: usize,
}

static RESTART_BLOCKS: Mutex<BTreeMap<usize, RestartBlock>> = Mutex::new(BTreeMap::new());

/// Store a restart block for `pid`. Called by the syscall that returns -EINTR.
/// Any previous block for this pid is overwritten.
pub fn set_restart(pid: usize, rb: RestartBlock) {
    RESTART_BLOCKS.lock().insert(pid, rb);
}

/// Retrieve and *consume* the restart block for `pid`.
/// Returns `None` if no restart is pending.
pub fn take_restart(pid: usize) -> Option<RestartBlock> {
    RESTART_BLOCKS.lock().remove(&pid)
}

/// Peek at the restart block without consuming it.
/// Used by the GDB RSP register-snapshot path.
pub fn get_restart(pid: usize) -> Option<RestartBlock> {
    RESTART_BLOCKS.lock().get(&pid).copied()
}

/// Discard any pending restart block for `pid`.
/// Call from `do_exit`, `do_execve`, and `SIGKILL` delivery.
pub fn clear_restart(pid: usize) {
    RESTART_BLOCKS.lock().remove(&pid);
}
