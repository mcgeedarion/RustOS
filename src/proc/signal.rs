//! Signal delivery, masking, and rt_sigreturn.
//!
//! ## Delivery model
//!
//! ### Thread-directed signals  (tgkill, tkill, raise)
//!
//!   send_signal(tid, sig) -> pushed to PENDING[tid]
//!   Delivered only to that specific thread in check_and_deliver.
//!
//! ### Group-directed signals  (kill, sigqueue, SIGCHLD to parent)
//!
//!   send_signal_group(tgid, sig) -> pushed to GROUP_PENDING[tgid]
//!   At check_and_deliver time, any thread in the group whose mask
//!   allows the signal may claim and handle it (first-come-first-served
//!   atomic removal from the group queue).
//!
//! ### Delivery order in check_and_deliver
//!
//!   1. Per-TID PENDING (thread-directed, highest priority)
//!   2. GROUP_PENDING   (group-directed, any unblocked thread)
//!
//! ## SA_RESTART
//!
//!   When a signal with SA_RESTART set interrupts a restartable syscall,
//!   `check_and_deliver_with_sepc` (called from the RISC-V ecall path)
//!   replays the syscall instead of returning -EINTR to userspace.
//!
//! ## AArch64 extensions
//!
//!   `check_and_deliver_aarch64` and `sys_rt_sigreturn_aarch64` mirror
//!   the x86_64 counterparts, using `ExceptionFrame` instead of `SyscallFrame`.

extern crate alloc;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use spin::Mutex;

use crate::proc::{process::State, scheduler};
use crate::uaccess::{copy_from_user, copy_to_user, USER_SPACE_END};

#[derive(Clone, Copy, Default, Debug)]
pub struct SigInfo {
    pub sig: u32,
    pub code: i32,
    pub pid: u32,
    pub uid: u32,
    pub status: i32,
    pub addr: usize,
    pub value: i64,
}

const SI_KERNEL: i32 = 128;
const CLD_EXITED: i32 = 1;
const CLD_KILLED: i32 = 2;
const SEGV_MAPERR: i32 = 1;
const SEGV_ACCERR: i32 = 2;
const SI_USER: i32 = 0;

/// Signal numbers (Linux ABI).
pub const SIGHUP: u32    = 1;
pub const SIGINT: u32    = 2;
pub const SIGQUIT: u32   = 3;
pub const SIGILL: u32    = 4;
pub const SIGTRAP: u32   = 5;
pub const SIGABRT: u32   = 6;
pub const SIGBUS: u32    = 7;
pub const SIGFPE: u32    = 8;
pub const SIGKILL: u32   = 9;
pub const SIGUSR1: u32   = 10;
pub const SIGSEGV: u32   = 11;
pub const SIGUSR2: u32   = 12;
pub const SIGPIPE: u32   = 13;
pub const SIGALRM: u32   = 14;
pub const SIGTERM: u32   = 15;
pub const SIGCHLD: u32   = 17;
pub const SIGCONT: u32   = 18;
pub const SIGSTOP: u32   = 19;

/// ILL_ILLOPC — illegal opcode.
const ILL_ILLOPC: i32 = 1;
/// FPE_INTDIV — integer divide by zero.
const FPE_INTDIV: i32 = 1;
/// BUS_ADRALN — invalid address alignment.
const BUS_ADRALN: i32 = 1;

/// Signals that are ignored by default (SIGCHLD=17, SIGURG=23, SIGWINCH=28).
const SIG_IGN_DEFAULT: u64 = (1u64 << 17) | (1u64 << 23) | (1u64 << 28);

/// Signals that stop by default (SIGTSTP=20, SIGTTIN=21, SIGTTOU=22, SIGSTOP=19).
const SIG_STOP_DEFAULT: u64 = (1u64 << 19) | (1u64 << 20) | (1u64 << 21) | (1u64 << 22);

/// Signals that cannot be caught or masked: SIGKILL(9) and SIGSTOP(19).
const SIG_UNCATCHABLE: u64 = (1u64 << 8) | (1u64 << 18); // bit = sig - 1

const SA_ONSTACK: u32    = 0x08000000;
const SA_RESTART: u32    = 0x10000000;
const SA_RESTORER: u32   = 0x04000000;
const SA_NODEFER: u32    = 0x40000000;

const SIG_BLOCK: u32   = 0;
const SIG_UNBLOCK: u32 = 1;
const SIG_SETMASK: u32 = 2;

/// Thread-directed pending signals, keyed by TID.
static PENDING: Mutex<BTreeMap<usize, VecDeque<SigInfo>>> = Mutex::new(BTreeMap::new());

/// Group-directed pending signals, keyed by TGID.
static GROUP_PENDING: Mutex<BTreeMap<usize, VecDeque<SigInfo>>> = Mutex::new(BTreeMap::new());

/// Per-TID signal mask.
static SIGMASK: Mutex<BTreeMap<usize, u64>> = Mutex::new(BTreeMap::new());

#[derive(Clone, Copy, Default)]
struct AltStack {
    ss_sp: usize,
    ss_flags: i32,
    ss_size: usize,
}

const SS_DISABLE: i32 = 2;

static ALTSTACK: Mutex<BTreeMap<usize, AltStack>> = Mutex::new(BTreeMap::new());

/// Remove the altstack entry for `pid` (called from exit path).
pub fn altstack_clear_pid(pid: usize) {
    ALTSTACK.lock().remove(&pid);
}

/// Drain the group-directed pending queue for `tgid` (called when the last
/// thread in the group exits).
pub fn group_pending_clear(tgid: usize) {
    GROUP_PENDING.lock().remove(&tgid);
}

/// Returns `true` if there is at least one unmasked pending signal for `tid`.
pub fn has_pending_signal(tid: usize) -> bool {
    let mask = sigmask_for(tid);
    let tgid = scheduler::with_proc(tid, |p| p.tgid).unwrap_or(tid);

    // Check thread-directed pending.
    {
        let pend = PENDING.lock();
        if let Some(q) = pend.get(&tid) {
            if q.iter().any(|i| !is_masked_by(i.sig, mask)) {
                return true;
            }
        }
    }
    // Check group-directed pending.
    {
        let gpend = GROUP_PENDING.lock();
        if let Some(q) = gpend.get(&tgid) {
            if q.iter().any(|i| !is_masked_by(i.sig, mask)) {
                return true;
            }
        }
    }
    false
}

fn is_masked_by(sig: u32, mask: u64) -> bool {
    if sig == 0 || sig > 64 {
        return true;
    }
    // SIGKILL and SIGSTOP are never masked.
    if sig == SIGKILL || sig == SIGSTOP {
        return false;
    }
    (mask >> (sig - 1)) & 1 != 0
}

fn pids_in_pgrp(pgid: usize) -> Vec<usize> {
    scheduler::with_procs_ro(|procs| {
        procs
            .iter()
            .filter(|p| p.pgid == pgid && p.pid == p.tgid)
            .map(|p| p.tgid)
            .collect()
    })
}

pub fn send_signal(tid: usize, sig: i32) -> isize {
    if sig <= 0 || sig > 64 {
        return -22;
    }
    send_signal_info(
        tid,
        SigInfo {
            sig: sig as u32,
            code: SI_KERNEL,
            ..Default::default()
        },
    );
    0
}

pub fn send_signal_user(tid: usize, sig: i32) -> isize {
    if sig <= 0 || sig > 64 {
        return -22;
    }
    let bypass = sig == 9 || sig == 19;
    if !bypass {
        let queue_len = {
            let map = PENDING.lock();
            map.get(&tid).map_or(0, |q| q.len())
        };
        if queue_len >= 32 {
            return -11;
        }
    }
    send_signal_info(
        tid,
        SigInfo {
            sig: sig as u32,
            code: SI_USER,
            pid: scheduler::current_pid() as u32,
            ..Default::default()
        },
    );
    0
}

pub fn send_signal_info(tid: usize, info: SigInfo) {
    if info.sig == 0 {
        return;
    }
    // SIGKILL / SIGSTOP: bypass queue limits and deliver immediately.
    let sig = info.sig;
    PENDING
        .lock()
        .entry(tid)
        .or_insert_with(VecDeque::new)
        .push_back(info);
    scheduler::wake_for_signal(tid, sig);
}

pub fn send_signal_group(tgid: usize, sig: i32) -> isize {
    if sig <= 0 || sig > 64 {
        return -22;
    }
    send_signal_group_info(
        tgid,
        SigInfo {
            sig: sig as u32,
            code: SI_KERNEL,
            ..Default::default()
        },
    );
    0
}

pub fn send_signal_group_info(tgid: usize, info: SigInfo) {
    if info.sig == 0 {
        return;
    }
    let sig = info.sig;
    GROUP_PENDING
        .lock()
        .entry(tgid)
        .or_insert_with(VecDeque::new)
        .push_back(info);
    scheduler::with_procs_ro(|procs| {
        for p in procs.iter().filter(|p| p.tgid == tgid) {
            scheduler::wake_for_signal(p.pid, sig);
        }
    });
}

fn pop_pending(tid: usize) -> Option<SigInfo> {
    PENDING.lock().get_mut(&tid)?.pop_front()
}

fn pop_group_pending(tgid: usize) -> Option<SigInfo> {
    GROUP_PENDING.lock().get_mut(&tgid)?.pop_front()
}

#[derive(Clone, Copy, Default)]
pub struct SigAction {
    pub handler: usize,
    pub flags: u32,
    pub restorer: usize,
    pub mask: u64,
}

static SIGACTIONS: Mutex<BTreeMap<(usize, u32), SigAction>> = Mutex::new(BTreeMap::new());

/// Return signal action for `(tgid, sig)`.  Uses TGID so that all threads in
/// the group share the same disposition table.
pub fn get_sigaction(pid: usize, sig: u32) -> SigAction {
    let tgid = scheduler::with_proc(pid, |p| p.tgid).unwrap_or(pid);
    SIGACTIONS
        .lock()
        .get(&(tgid, sig))
        .copied()
        .unwrap_or_default()
}

pub fn set_sigaction(pid: usize, sig: u32, sa: SigAction) {
    // SIGKILL and SIGSTOP cannot be caught or ignored.
    if sig == SIGKILL || sig == SIGSTOP {
        return;
    }
    let tgid = scheduler::with_proc(pid, |p| p.tgid).unwrap_or(pid);
    SIGACTIONS.lock().insert((tgid, sig), sa);
}

// ---------------------------------------------------------------------------
// Signal frame layouts
// ---------------------------------------------------------------------------

#[repr(C)]
struct SigFrameX86 {
    pretcode: u64,
    uc_flags: u64,
    uc_link: u64,
    uc_stack_ss_sp: u64,
    uc_stack_ss_flags: u32,
    uc_stack_ss_size: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rdi: u64,
    rsi: u64,
    rbp: u64,
    rbx: u64,
    rdx: u64,
    rax: u64,
    rcx: u64,
    rsp: u64,
    rip: u64,
    eflags: u64,
    cs: u16,
    _pad: [u16; 3],
    ss: u16,
    _pad2: [u16; 3],
    si_signo: u32,
    si_errno: u32,
    si_code: i32,
    si_addr: u64,
    sig_mask: u64,
}

#[repr(C)]
struct SigFrameAarch64 {
    magic: u64,
    regs: [u64; 31],
    sp: u64,
    pc: u64,
    pstate: u64,
    si_signo: u32,
    si_code: i32,
    si_addr: u64,
    sig_mask: u64,
    restorer: u64,
}

const SIGFRAME_AARCH64_MAGIC: u64 = 0xDEAD_BEEF_CAFE_0000;

fn push_sigframe_x86(
    frame: &mut crate::arch::x86_64::syscall::SyscallFrame,
    sa: &SigAction,
    info: &SigInfo,
) -> bool {
    let user_rsp = (frame.rsp - core::mem::size_of::<SigFrameX86>()) & !15;
    if user_rsp < 0x1000 || user_rsp >= USER_SPACE_END {
        return false;
    }
    let sf = SigFrameX86 {
        pretcode: sa.restorer as u64,
        uc_flags: 0,
        uc_link: 0,
        uc_stack_ss_sp: 0,
        uc_stack_ss_flags: SS_DISABLE as u32,
        uc_stack_ss_size: 0,
        r8: frame.r8 as u64,
        r9: frame.r9 as u64,
        r10: frame.r10 as u64,
        r11: frame.rflags as u64,
        r12: frame.r12 as u64,
        r13: frame.r13 as u64,
        r14: frame.r14 as u64,
        r15: frame.r15 as u64,
        rdi: frame.rdi as u64,
        rsi: frame.rsi as u64,
        rbp: frame.rbp as u64,
        rbx: frame.rbx as u64,
        rdx: frame.rdx as u64,
        rax: frame.rax as u64,
        rcx: frame.rip as u64,
        rsp: frame.rsp as u64,
        rip: frame.rip as u64,
        eflags: frame.rflags as u64,
        cs: 0x1b,
        _pad: [0; 3],
        ss: 0x23,
        _pad2: [0; 3],
        si_signo: info.sig,
        si_errno: 0,
        si_code: info.code,
        si_addr: info.addr as u64,
        sig_mask: sigmask_for(scheduler::current_pid()),
    };
    if !copy_to_user(user_rsp, unsafe {
        core::slice::from_raw_parts(
            &sf as *const _ as *const u8,
            core::mem::size_of::<SigFrameX86>(),
        )
    }) {
        return false;
    }
    frame.rsp = user_rsp;
    frame.rip = sa.handler;
    frame.rdi = info.sig as usize;
    frame.rsi = user_rsp + 16;
    frame.rdx = user_rsp;
    true
}

#[cfg(target_arch = "aarch64")]
fn push_sigframe_aarch64(
    frame: &mut crate::arch::aarch64::interrupts::ExceptionFrame,
    sa: &SigAction,
    info: &SigInfo,
) -> bool {
    let sp = (frame.sp_el0 as usize).wrapping_sub(core::mem::size_of::<SigFrameAarch64>()) & !15;
    if sp < 0x1000 || sp >= USER_SPACE_END {
        return false;
    }
    let mut regs = [0u64; 31];
    regs.copy_from_slice(&frame.x);
    let sf = SigFrameAarch64 {
        magic: SIGFRAME_AARCH64_MAGIC,
        regs,
        sp: frame.sp_el0,
        pc: frame.elr_el1,
        pstate: frame.spsr_el1,
        si_signo: info.sig,
        si_code: info.code,
        si_addr: info.addr as u64,
        sig_mask: sigmask_for(scheduler::current_pid()),
        restorer: sa.restorer as u64,
    };
    if !copy_to_user(sp, unsafe {
        core::slice::from_raw_parts(
            &sf as *const _ as *const u8,
            core::mem::size_of::<SigFrameAarch64>(),
        )
    }) {
        return false;
    }
    frame.sp_el0 = sp as u64;
    frame.elr_el1 = sa.handler as u64;
    frame.spsr_el1 = frame.spsr_el1 & !0xf;
    frame.x[0] = info.sig as u64;
    frame.x[1] = (sp + 8) as u64;
    frame.x[2] = sp as u64;
    frame.x[30] = sa.restorer as u64;
    true
}

fn sigmask_for(tid: usize) -> u64 {
    SIGMASK.lock().get(&tid).copied().unwrap_or(0)
}

fn is_masked(tid: usize, sig: u32) -> bool {
    is_masked_by(sig, sigmask_for(tid))
}

/// Apply the default action for a signal.
fn apply_default(pid: usize, info: &SigInfo) {
    let sig = info.sig;

    // SIGKILL and SIGSTOP are always handled as default regardless of
    // any registered handler — they are non-maskable.
    if sig == SIGKILL {
        crate::proc::exit::do_exit(pid, encode_signal_status(sig));
        return;
    }
    if sig == SIGSTOP {
        scheduler::with_proc_mut(pid, |p, pl| {
            pl.set_state(p, State::Stopped);
        });
        return;
    }

    let bit = 1u64 << (sig.saturating_sub(1));
    if SIG_IGN_DEFAULT & bit != 0 {
        return; // default is ignore
    }
    if SIG_STOP_DEFAULT & bit != 0 {
        scheduler::with_proc_mut(pid, |p, pl| {
            pl.set_state(p, State::Stopped);
        });
        return;
    }
    // Default is terminate (encodes signal number in low 7 bits of wstatus).
    crate::proc::exit::do_exit(pid, encode_signal_status(sig));
}

/// Encode a signal-termination wstatus: `signum & 0x7f`.
/// This is what WIFSIGNALED/WTERMSIG expects.
fn encode_signal_status(sig: u32) -> i32 {
    (sig & 0x7f) as i32
}

// ---------------------------------------------------------------------------
// Delivery entry points (called from arch exception / syscall paths)
// ---------------------------------------------------------------------------

/// Called by `rust_syscall_handler` after every syscall (except rt_sigreturn).
/// x86_64 path.
pub fn check_and_deliver(frame: &mut crate::arch::x86_64::syscall::SyscallFrame) {
    let tid = scheduler::current_pid();
    let tgid = scheduler::with_proc(tid, |p| p.tgid).unwrap_or(tid);

    loop {
        let info = pop_pending(tid).or_else(|| pop_group_pending(tgid));
        let info = match info {
            Some(i) if !is_masked(tid, i.sig) => i,
            Some(i) => {
                // Re-queue masked signal and stop scanning.
                put_back_pending(tid, i);
                return;
            },
            None => return,
        };

        // SIGKILL / SIGSTOP: force default action regardless of handler.
        if info.sig == SIGKILL || info.sig == SIGSTOP {
            apply_default(tid, &info);
            return;
        }

        let sa = get_sigaction(tgid, info.sig);
        match sa.handler {
            0 => {
                apply_default(tid, &info);
                return;
            },
            1 => continue, // SIG_IGN — consume and check next
            _ => {
                if !push_sigframe_x86(frame, &sa, &info) {
                    // Could not set up user frame; apply default.
                    apply_default(tid, &info);
                }
                return;
            },
        }
    }
}

/// Called from `syscall_asm_entry` on NR 15 (rt_sigreturn) for x86_64.
pub fn sys_rt_sigreturn(frame: &mut crate::arch::x86_64::syscall::SyscallFrame) {
    let sp = frame.rsp;
    let mut bytes = [0u8; core::mem::size_of::<SigFrameX86>()];
    if !copy_from_user(sp, &mut bytes) {
        return;
    }
    let sf: SigFrameX86 = unsafe { core::mem::transmute(bytes) };
    frame.rip    = sf.rip as usize;
    frame.rflags = sf.eflags as usize;
    frame.rsp    = sf.rsp as usize;
    frame.rax    = sf.rax as usize;
    frame.rbx    = sf.rbx as usize;
    frame.rbp    = sf.rbp as usize;
    frame.rdi    = sf.rdi as usize;
    frame.rsi    = sf.rsi as usize;
    frame.rdx    = sf.rdx as usize;
    frame.r12    = sf.r12 as usize;
    frame.r13    = sf.r13 as usize;
    frame.r14    = sf.r14 as usize;
    frame.r15    = sf.r15 as usize;
    let tid = scheduler::current_pid();
    *SIGMASK.lock().entry(tid).or_insert(0) = sf.sig_mask;
}

/// Called by `aarch64_sync_handler` after every SVC (except rt_sigreturn).
#[cfg(target_arch = "aarch64")]
pub fn check_and_deliver_aarch64(frame: &mut crate::arch::aarch64::interrupts::ExceptionFrame) {
    let tid = scheduler::current_pid();
    let tgid = scheduler::with_proc(tid, |p| p.tgid).unwrap_or(tid);

    loop {
        let info = pop_pending(tid).or_else(|| pop_group_pending(tgid));
        let info = match info {
            Some(i) if !is_masked(tid, i.sig) => i,
            Some(i) => {
                put_back_pending(tid, i);
                return;
            },
            None => return,
        };

        if info.sig == SIGKILL || info.sig == SIGSTOP {
            apply_default(tid, &info);
            return;
        }

        let sa = get_sigaction(tgid, info.sig);
        match sa.handler {
            0 => { apply_default(tid, &info); return; },
            1 => continue,
            _ => {
                if !push_sigframe_aarch64(frame, &sa, &info) {
                    apply_default(tid, &info);
                }
                return;
            },
        }
    }
}

/// Called from `aarch64_sync_handler` when ESR_EC == SVC64 and x8 == 139.
#[cfg(target_arch = "aarch64")]
pub fn sys_rt_sigreturn_aarch64(frame: &mut crate::arch::aarch64::interrupts::ExceptionFrame) {
    let sp = frame.sp_el0 as usize;
    let sf_size = core::mem::size_of::<SigFrameAarch64>();
    let mut bytes = alloc::vec![0u8; sf_size];
    if !copy_from_user(sp, &mut bytes) {
        return;
    }
    let magic = u64::from_ne_bytes(bytes[0..8].try_into().unwrap_or([0; 8]));
    if magic != SIGFRAME_AARCH64_MAGIC {
        return;
    }
    let sf: SigFrameAarch64 =
        unsafe { core::ptr::read_unaligned(bytes.as_ptr() as *const SigFrameAarch64) };
    frame.x.copy_from_slice(&sf.regs);
    frame.sp_el0   = sf.sp;
    frame.elr_el1  = sf.pc;
    frame.spsr_el1 = sf.pstate;
    let tid = scheduler::current_pid();
    *SIGMASK.lock().entry(tid).or_insert(0) = sf.sig_mask;
}

fn put_back_pending(tid: usize, info: SigInfo) {
    PENDING
        .lock()
        .entry(tid)
        .or_insert_with(VecDeque::new)
        .push_front(info);
}

// ---------------------------------------------------------------------------
// Syscall implementations
// ---------------------------------------------------------------------------

pub fn sys_rt_sigaction(
    pid: usize,
    sig: u32,
    new_sa: Option<SigAction>,
    old_sa: Option<&mut SigAction>,
) -> isize {
    if sig == 0 || sig > 64 {
        return -22;
    }
    // SIGKILL and SIGSTOP cannot be caught or ignored.
    if sig == SIGKILL || sig == SIGSTOP {
        return -22;
    }
    if let Some(old) = old_sa {
        *old = get_sigaction(pid, sig);
    }
    if let Some(sa) = new_sa {
        set_sigaction(pid, sig, sa);
    }
    0
}

pub fn sys_rt_sigprocmask(
    how: u32,
    set: Option<u64>,
    oldset: Option<&mut u64>,
    pid: usize,
) -> isize {
    let mut mask = SIGMASK.lock();
    let current = mask.entry(pid).or_insert(0);
    if let Some(old) = oldset {
        *old = *current;
    }
    if let Some(s) = set {
        match how {
            SIG_BLOCK   => *current |= s,
            SIG_UNBLOCK => *current &= !s,
            SIG_SETMASK => *current = s,
            _ => return -22,
        }
        // SIGKILL (bit 8) and SIGSTOP (bit 18) cannot be masked.
        *current &= !SIG_UNCATCHABLE;
    }
    0
}

pub fn sys_rt_sigpending(pid: usize) -> u64 {
    let thread = PENDING.lock().get(&pid).map_or(0u64, |q| {
        q.iter()
            .fold(0u64, |acc, i| acc | (1u64 << i.sig.saturating_sub(1)))
    });
    let tgid = scheduler::with_proc(pid, |p| p.tgid).unwrap_or(pid);
    let group = GROUP_PENDING.lock().get(&tgid).map_or(0u64, |q| {
        q.iter()
            .fold(0u64, |acc, i| acc | (1u64 << i.sig.saturating_sub(1)))
    });
    thread | group
}

// ---------------------------------------------------------------------------
// Convenience senders for kernel-internal use
// ---------------------------------------------------------------------------

pub fn send_sigchld(parent_pid: usize, child_pid: usize, exit_code: i32) {
    send_signal_group_info(
        parent_pid,
        SigInfo {
            sig: SIGCHLD,
            code: if exit_code >= 0 { CLD_EXITED } else { CLD_KILLED },
            pid: child_pid as u32,
            status: exit_code,
            ..Default::default()
        },
    );
}

/// Send SIGSEGV (segmentation fault) to `pid` with the faulting address.
pub fn send_sigsegv(pid: usize, fault_addr: usize) {
    send_signal_info(
        pid,
        SigInfo {
            sig: SIGSEGV,
            code: SEGV_MAPERR,
            addr: fault_addr,
            ..Default::default()
        },
    );
}

/// Send SIGSEGV (permission fault) to `pid`.
pub fn send_sigsegv_accerr(pid: usize, fault_addr: usize) {
    send_signal_info(
        pid,
        SigInfo {
            sig: SIGSEGV,
            code: SEGV_ACCERR,
            addr: fault_addr,
            ..Default::default()
        },
    );
}

/// Send SIGBUS (bus error / alignment fault) to `pid`.
pub fn send_sigbus(pid: usize, fault_addr: usize) {
    send_signal_info(
        pid,
        SigInfo {
            sig: SIGBUS,
            code: BUS_ADRALN,
            addr: fault_addr,
            ..Default::default()
        },
    );
}

/// Send SIGILL (illegal instruction) to `pid`.
pub fn send_sigill(pid: usize, fault_addr: usize) {
    send_signal_info(
        pid,
        SigInfo {
            sig: SIGILL,
            code: ILL_ILLOPC,
            addr: fault_addr,
            ..Default::default()
        },
    );
}

/// Send SIGFPE (floating-point / integer arithmetic exception) to `pid`.
pub fn send_sigfpe(pid: usize, fault_addr: usize) {
    send_signal_info(
        pid,
        SigInfo {
            sig: SIGFPE,
            code: FPE_INTDIV,
            addr: fault_addr,
            ..Default::default()
        },
    );
}

/// Send SIGTERM to the given PID (group-directed).
pub fn send_sigterm(tgid: usize) {
    send_signal_group(tgid, SIGTERM as i32);
}

/// Send SIGKILL to the given PID (group-directed, non-maskable).
pub fn send_sigkill(tgid: usize) {
    // Deliver SIGKILL directly: it bypasses masking and user handlers.
    send_signal_group_info(
        tgid,
        SigInfo {
            sig: SIGKILL,
            code: SI_KERNEL,
            ..Default::default()
        },
    );
}

/// `sys_kill(pid, sig)` — send signal to a process or process group.
///
/// `pid > 0`  : signal sent to process `pid`.
/// `pid == 0` : signal sent to every process in the caller's pgrp.
/// `pid == -1`: signal sent to every process (except PID 1).
/// `pid < -1` : signal sent to pgrp `|pid|`.
pub fn sys_kill(pid: isize, sig: i32) -> isize {
    if sig < 0 || sig > 64 {
        return -22;
    }
    if sig == 0 {
        // Existence check only.
        return if pid > 0 {
            if scheduler::with_proc(pid as usize, |_| ()).is_some() { 0 } else { -3 }
        } else {
            0
        };
    }

    let caller = scheduler::current_pid();

    match pid {
        n if n > 0 => {
            let tgid = scheduler::with_proc(n as usize, |p| p.tgid).unwrap_or(n as usize);
            send_signal_group(tgid, sig)
        },
        0 => {
            let pgid = scheduler::with_proc(caller, |p| p.pgid).unwrap_or(caller);
            for tgid in pids_in_pgrp(pgid) {
                send_signal_group(tgid, sig);
            }
            0
        },
        -1 => {
            // Broadcast to all processes except init (PID 1).
            let targets: Vec<usize> = scheduler::with_procs_ro(|pl| {
                pl.iter()
                    .filter(|p| p.pid as usize > 1 && p.tgid == p.pid)
                    .map(|p| p.tgid as usize)
                    .collect()
            });
            for tgid in targets {
                send_signal_group(tgid, sig);
            }
            0
        },
        n => {
            let pgid = (-n) as usize;
            for tgid in pids_in_pgrp(pgid) {
                send_signal_group(tgid, sig);
            }
            0
        },
    }
}

/// `sys_tgkill(tgid, tid, sig)` — deliver a thread-directed signal, verifying
/// that `tid` belongs to `tgid`.
pub fn sys_tgkill(tgid: usize, tid: usize, sig: i32) -> isize {
    if sig < 0 || sig > 64 {
        return -22;
    }
    // Verify tgid / tid membership.
    let actual_tgid = match scheduler::with_proc(tid, |p| p.tgid) {
        Some(t) => t,
        None => return -3,
    };
    if actual_tgid != tgid {
        return -3;
    }
    if sig == 0 {
        return 0;
    }
    send_signal(tid, sig)
}

/// `sys_tkill(tid, sig)` — thread-directed kill (deprecated, no tgid check).
pub fn sys_tkill(tid: usize, sig: i32) -> isize {
    if sig < 0 || sig > 64 {
        return -22;
    }
    if sig == 0 {
        return if scheduler::with_proc(tid, |_| ()).is_some() { 0 } else { -3 };
    }
    send_signal(tid, sig)
}
