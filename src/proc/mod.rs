//! Process-management subsystem.
//!
//! Invariants:
//! - `proc_table` is the authoritative PID/TID index for live tasks.
//! - Scheduler-visible task state transitions go through `scheduler` + `thread`.
//! - Signal delivery and reaping semantics are centralized in `signal` and `wait`.
//! - Namespace, credentials, and cgroup membership must remain coherent across
//!   `fork`/`clone`/`exec`.

pub mod cgroup;
pub mod clone;
pub mod context;
pub mod creds;
pub mod cwd;

// Real dynlink module (ELF interpreter lookup / ld.so loading).
// Previously shadowed by an inline stub; now the full implementation is used.
pub mod dynlink;

// Real exec, exit, fork, wait implementations.
pub mod exec;
pub mod exit;
pub mod fork;
pub mod fork_syscall;

// Real futex (FUTEX_WAIT/WAKE/REQUEUE, robust lists, pi-futex skeleton).
pub mod futex;

pub mod ipc;

// Real itimer implementation (ITIMER_REAL / ITIMER_VIRTUAL / ITIMER_PROF).
pub mod itimer;

pub mod namespace;

// Real nanosleep / clock_gettime / sleep_ns_internal.
pub mod nanosleep;

// Real net-namespace helpers.
pub mod net_ns;

pub mod pid;
pub mod proc_table;
pub use proc_table as table;
pub mod process;
pub mod ptrace;
pub mod rlimit;
pub mod rusage;

// Real restart-block helpers (SA_RESTART syscall replay).
pub mod restart;

pub mod scheduler;
pub mod signal;

// Session / process-group helpers.
pub mod session {
    pub fn set_pgid(pid: usize, pgid: usize) -> isize {
        crate::proc::creds::sys_setpgid(pid as u32, pgid as u32)
    }

    pub fn get_pgid(pid: usize) -> isize {
        let target = if pid == 0 {
            crate::proc::scheduler::current_pid()
        } else {
            pid
        };
        crate::proc::scheduler::with_proc(target, |p| p.pgid as isize).unwrap_or(-3)
    }

    pub fn setsid() -> isize {
        crate::proc::creds::sys_setsid()
    }

    pub fn get_sid(pid: usize) -> isize {
        crate::proc::creds::sys_getsid(pid as u32)
    }
}

pub mod task_types;
pub use task_types as task;
pub mod thread;
pub mod time_ns;

// The real `pid_ns`, `task_group`, `sched_helpers`, `user_ns` modules.
pub mod pid_ns;
pub mod task_group;
pub mod sched_helpers;
pub mod user_ns;

pub mod wait;

// CoW page-fault handler re-exported from mm.
pub use crate::mm::cow_fault;

/// Returns the effective credentials of the currently-running task as a
/// `perm::Creds` value ready for VFS DAC checks.
///
/// Falls back to root (`uid/gid/euid/egid = 0`) on any scheduler miss so
/// that kernel-internal paths (init, IRQ context) always succeed.
pub fn current_creds() -> crate::fs::vfs::perm::Creds {
    let pid = crate::proc::scheduler::current_pid();
    crate::proc::scheduler::with_proc(pid, |p| crate::fs::vfs::perm::Creds {
        uid:  p.uid,
        gid:  p.gid,
        euid: p.euid,
        egid: p.egid,
    })
    .unwrap_or(crate::fs::vfs::perm::Creds { uid: 0, gid: 0, euid: 0, egid: 0 })
}
