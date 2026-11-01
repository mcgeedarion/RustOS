//! Wait-family metadata access and atomic logical zombie reaping.
//! Copyout precedes unlink so EFAULT preserves the child. A waiter losing the
//! unlink race rescans and cannot return another waiter's successful reap.
//! Output buffers may have been written before a losing-race error/return.
//! Blocking restart/sleep behavior is retained; lost-wakeup repair is separate.
extern crate alloc;
use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::proc::process::{ProcLock, State};
use crate::proc::{proc_table, scheduler, signal};
use crate::uaccess::copy_to_user;

pub const WNOHANG: i32 = 1;
pub const WUNTRACED: i32 = 2;
pub const WCONTINUED: i32 = 8;
pub const P_ALL: i32 = 0;
pub const P_PID: i32 = 1;
pub const P_PGID: i32 = 2;
pub const P_PIDFD: i32 = 3;
// These existing waitid ABI constants/semantics are not redesigned here.
pub const WEXITED: i32 = 1;
pub const WSTOPPED: i32 = 2;
pub const WNOWAIT: i32 = 0x10000000;
const CLD_EXITED: i32 = 1;
const CLD_KILLED: i32 = 2;
const CLD_STOPPED: i32 = 3;
const CLD_CONTINUED: i32 = 4;

#[inline]
pub fn encode_exit(code: i32) -> i32 {
    if code < 0 { (-code) & 0x7f } else { (code & 0xff) << 8 }
}
pub fn notify_exit(child_pid: usize) {
    let (ppid, exit_code) = scheduler::with_proc(child_pid, |p| (p.ppid, p.exit_code)).unwrap_or((0, 0));
    if ppid != 0 {
        signal::send_sigchld(ppid, child_pid, exit_code);
        scheduler::wake_pid(ppid);
    }
}

struct Child {
    owner: Arc<ProcLock>, pid: usize, pgid: usize, state: State, exit_code: i32,
}
fn children_of(parent: usize) -> Vec<Child> {
    // with_procs_ro releases the table lock before invoking this callback.
    // Parent/group/state/status belong to the PCB, not the outer ProcLock.
    scheduler::with_procs_ro(|snapshot| snapshot.iter().filter_map(|owner| {
        let pcb = owner.inner.lock();
        if pcb.ppid != parent { return None; }
        Some(Child { owner: Arc::clone(owner), pid: pcb.pid, pgid: pcb.pgid,
            state: pcb.state, exit_code: pcb.exit_code })
    }).collect())
}
fn matches_wait_pid(child: &Child, target: isize, caller_pgid: usize) -> bool {
    match target {
        -1 => true,
        0 => child.pgid == caller_pgid,
        n if n > 0 => child.pid == n as usize,
        n => checked_wait_group(n).is_some_and(|group| child.pgid == group),
    }
}
pub fn sys_waitpid(target_pid: isize, wstatus_va: usize, options: i32) -> isize {
    sys_wait4(target_pid, wstatus_va, options, 0)
}
pub fn sys_wait4(target_pid: isize, wstatus_va: usize, options: i32, rusage_va: usize) -> isize {
    if target_pid == isize::MIN { return -22; }
    let caller = scheduler::current_pid();
    let caller_pgid = scheduler::with_proc(caller, |p| p.pgid).unwrap_or(caller);
    loop {
        let eligible: Vec<Child> = children_of(caller).into_iter()
            .filter(|child| matches_wait_pid(child, target_pid, caller_pgid)).collect();
        if eligible.is_empty() { return -10; }
        if let Some(child) = eligible.iter().find(|child| child.state == State::Zombie) {
            if wstatus_va != 0 && copy_to_user(wstatus_va, &child.exit_code.to_ne_bytes()).is_err() {
                return -14;
            }
            if rusage_va != 0 && copy_to_user(rusage_va, &[0u8; 144]).is_err() { return -14; }
            if proc_table::try_reap_zombie(caller, &child.owner) { return child.pid as isize; }
            continue;
        }
        if options & WNOHANG != 0 { return 0; }
        // Preserve the existing abandoned-continuation restart convention.
        // This patch does not close the scan-to-sleep lost-wakeup window.
        let _ = scheduler::with_proc_mut(caller, |pcb, _| {
            #[cfg(target_arch = "x86_64")]
            { pcb.user_context.pc = pcb.user_context.pc.saturating_sub(2); }
            #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
            { pcb.user_context.pc = pcb.user_context.pc.saturating_sub(4); }
            pcb.pc = pcb.user_context.pc;
        });
        scheduler::sleep_current();
        if signal::has_pending_signal(caller) { return -4; }
    }
}

#[repr(C)]
pub struct SigInfoWait {
    si_signo: i32, si_errno: i32, si_code: i32, si_pid: i32, si_uid: u32,
    si_status: i32, si_utime: i64, si_stime: i64,
}
impl Default for SigInfoWait {
    fn default() -> Self {
        Self { si_signo: 17, si_errno: 0, si_code: 0, si_pid: 0, si_uid: 0,
            si_status: 0, si_utime: 0, si_stime: 0 }
    }
}
/// Shares PCB snapshots and atomic zombie unlink with wait4. The existing
/// waitid ABI layout, flags, and stopped/continued semantics remain partial.
pub fn sys_waitid(idtype: i32, id: i32, infop: usize, options: i32) -> isize {
    let caller = scheduler::current_pid();
    if options & (WEXITED | WSTOPPED | WCONTINUED) == 0 { return -22; }
    loop {
        let eligible: Vec<Child> = children_of(caller).into_iter().filter(|child| match idtype {
            P_ALL => true, P_PID => child.pid as i32 == id, P_PGID => child.pgid as i32 == id,
            _ => false,
        }).collect();
        if eligible.is_empty() { return -10; }
        let found = eligible.iter().find(|child| match child.state {
            State::Zombie => options & WEXITED != 0,
            State::Stopped => options & WSTOPPED != 0,
            State::Continued => options & WCONTINUED != 0,
            _ => false,
        });
        if let Some(child) = found {
            if infop != 0 {
                let mut info = SigInfoWait::default();
                info.si_pid = child.pid as i32;
                match child.state {
                    State::Zombie => {
                        info.si_code = if child.exit_code >= 0 { CLD_EXITED } else { CLD_KILLED };
                        info.si_status = child.exit_code;
                    }
                    State::Stopped => { info.si_code = CLD_STOPPED; info.si_status = (child.exit_code >> 8) & 0xff; }
                    State::Continued => { info.si_code = CLD_CONTINUED; }
                    _ => {}
                }
                let bytes = unsafe { core::slice::from_raw_parts(
                    &info as *const _ as *const u8, core::mem::size_of::<SigInfoWait>()) };
                if copy_to_user(infop, bytes).is_err() { return -14; }
            }
            if options & WNOWAIT == 0 && child.state == State::Zombie
                && !proc_table::try_reap_zombie(caller, &child.owner) { continue; }
            return 0;
        }
        if options & WNOHANG != 0 { return 0; }
        scheduler::sleep_current();
        if signal::has_pending_signal(caller) { return -4; }
    }
}
#[inline]
fn checked_wait_group(selector: isize) -> Option<usize> {
    if selector >= -1 { None } else { selector.checked_neg().map(|group| group as usize) }
}
#[cfg(test)]
mod tests {
    use super::checked_wait_group;
    #[test]
    fn wait_group_selector_handles_signed_boundaries() {
        assert_eq!(checked_wait_group(-2), Some(2));
        assert_eq!(checked_wait_group(-42), Some(42));
        assert_eq!(checked_wait_group(-1), None);
        assert_eq!(checked_wait_group(0), None);
        assert_eq!(checked_wait_group(isize::MIN), None);
    }
}
