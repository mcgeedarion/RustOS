//! Restartable-syscall bookkeeping.
//!
//! ## Protocol
//!
//!   1. A syscall that can block (nanosleep, clock_nanosleep, futex WAIT, …)
//!      detects EINTR from signal delivery.
//!   2. Before returning -EINTR to userspace it calls `set_restart` with the
//!      syscall number, the adjusted arguments, and enough context to repeat it.
//!   3. `check_restart` is invoked on the slow path of exception return;
//!      if SA_RESTART was set on the handler that interrupted the syscall, the
//!      syscall is re-entered in-kernel by restoring the original (possibly
//!      adjusted) argument registers.

use crate::sync::spinlock::SpinLock;

/// Per-process restart state.
#[derive(Clone, Copy, Debug, Default)]
pub struct RestartInfo {
    pub syscall_nr: usize,
    pub args: [usize; 6],
}

static RESTART_TABLE: SpinLock<[Option<RestartInfo>; 256]> =
    SpinLock::new([None; 256]);

/// Record restart info for `pid`.
pub fn set_restart(pid: usize, info: RestartInfo) {
    let mut t = RESTART_TABLE.lock();
    if pid < t.len() {
        t[pid] = Some(info);
    }
}

/// Retrieve and clear restart info for `pid`, if any.
pub fn get_restart(pid: usize) -> Option<RestartInfo> {
    let mut t = RESTART_TABLE.lock();
    if pid < t.len() { t[pid].take() } else { None }
}

/// Apply a pending restart to the live trap frame for `pid` on x86_64.
///
/// # Safety
/// `frame` must be the live supervisor trap frame for `pid` on the current CPU.
#[cfg(target_arch = "x86_64")]
pub unsafe fn apply_restart(
    pid: usize,
    frame: &mut crate::arch::x86_64::trap::TrapFrame,
) -> bool {
    if let Some(info) = get_restart(pid) {
        frame.rax = info.syscall_nr;
        frame.rdi = info.args[0];
        frame.rsi = info.args[1];
        frame.rdx = info.args[2];
        frame.r10 = info.args[3];
        frame.r8  = info.args[4];
        frame.r9  = info.args[5];
        true
    } else {
        false
    }
}

/// Apply a pending restart to the live trap frame for `pid` on AArch64.
///
/// # Safety
/// `frame` must be the live supervisor trap frame for `pid` on the current CPU.
#[cfg(target_arch = "aarch64")]
pub unsafe fn apply_restart(
    pid: usize,
    frame: &mut crate::arch::aarch64::trap::TrapFrame,
) -> bool {
    if let Some(info) = get_restart(pid) {
        frame.regs[8]  = info.syscall_nr;
        frame.regs[0]  = info.args[0];
        frame.regs[1]  = info.args[1];
        frame.regs[2]  = info.args[2];
        frame.regs[3]  = info.args[3];
        frame.regs[4]  = info.args[4];
        frame.regs[5]  = info.args[5];
        true
    } else {
        false
    }
}
