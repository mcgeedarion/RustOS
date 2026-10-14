//! Global process table — the single source of truth for all `ProcLock`
//! entries.
//!
//! ## Design (S2 fix)
//!
//! The old `PROCS: SpinLock<Vec<Pcb>>` held a single lock for the *entire*
//! process table on every scheduler tick, signal delivery, fork, wait, and
//! ioctl call.  Under SMP this caused:
//!
//!   - `check_and_deliver` (signal path) and `tick` (scheduler) contending on the same lock
//!     simultaneously across CPUs.
//!   - `load_balance` in `tick` re-locking after a partial release.
//!   - `with_proc_mut` inside `do_exit` holding PROCS while calling `free_kstack`, which may
//!     trigger an allocator lock.
//!
//! The new design:
//!
//!   ```text
//!   PROC_TABLE: SpinLock<Vec<Arc<ProcLock>>>
//!         │                         │
//!         │  held only for lookup   │
//!         └─────────────────────────┘
//!                                   │
//!                              Arc<ProcLock>
//!                                   │
//!                           released before ──► ProcLock::inner:
//! spin::Mutex<Pcb>                           locking inner
//!   ```
//!
//! Rules:
//!   1. Take `PROC_TABLE` only to find + clone an `Arc<ProcLock>`, then drop it.
//!   2. Never hold `PROC_TABLE` while locking `ProcLock::inner`.
//!   3. Never hold two `ProcLock::inner` locks simultaneously (prevents lock-order inversions
//!      across fork/signal/wait paths).
//!   4. The scheduler hot path (`schedule`, `tick`, `load_balance`) reads `ProcLock::state_atom`
//!      (AtomicU8) and works through `*mut Task` pointers — it never locks `PROC_TABLE`.

extern crate alloc;
use crate::proc::process::{Pcb, ProcLock, State};
use crate::proc::task_types::Task;
use crate::sync::spinlock::SpinLock;
use alloc::sync::Arc;
use alloc::vec::Vec;

static PROC_TABLE: SpinLock<Vec<Arc<ProcLock>>> = SpinLock::new(Vec::new());

static NEXT_PID: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(1);

pub fn next_pid() -> u32 {
    NEXT_PID.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
}

pub fn proc_count() -> usize {
    PROC_TABLE.lock().len()
}

/// Find and clone the `Arc<ProcLock>` for `pid`.
/// Holds `PROC_TABLE` for the minimum time (lookup + clone only).
/// Returns `None` if the pid is not in the table.
pub fn find_proc_lock(pid: usize) -> Option<Arc<ProcLock>> {
    let table = PROC_TABLE.lock();
    table.iter().find(|pl| pl.pid as usize == pid).cloned()
}

/// Run `f` with a shared reference to the Pcb for `pid`.
/// Acquires PROC_TABLE briefly, clones the Arc, releases table lock,
/// then locks ProcLock::inner.
pub fn with_proc<T, F: FnOnce(&Pcb) -> T>(pid: usize, f: F) -> Option<T> {
    let pl = find_proc_lock(pid)?;
    let inner = pl.inner.lock();
    Some(f(&*inner))
}

/// Run `f` with a mutable reference to the Pcb and the owning `ProcLock`
/// (so `f` can call `pl.set_state`).
pub fn with_proc_mut<T, F: FnOnce(&mut Pcb, &ProcLock) -> T>(pid: usize, f: F) -> Option<T> {
    let pl = find_proc_lock(pid)?;
    let mut inner = pl.inner.lock();
    Some(f(&mut *inner, &pl))
}

/// Iterate all `ProcLock`s read-only.
/// Clones the entire Vec of Arcs (cheap: Arc clone is just a refcount bump)
/// so the table lock is not held during iteration.
pub fn with_procs_ro<T, F: FnOnce(&Vec<Arc<ProcLock>>) -> T>(f: F) -> T {
    let snapshot: Vec<Arc<ProcLock>> = PROC_TABLE.lock().clone();
    f(&snapshot)
}

/// Iterate all `ProcLock`s with mutation (e.g., fork inserts, exit removes).
/// Holds PROC_TABLE for the duration of `f` — keep `f` short.
pub fn with_procs_mut<T, F: FnOnce(&mut Vec<Arc<ProcLock>>) -> T>(f: F) -> T {
    f(&mut PROC_TABLE.lock())
}

/// Count live (non-Zombie) threads that share the same tgid as `pid`.
/// Used by `sys_setns` to reject CLONE_NEWPID / CLONE_NEWUSER joins from
/// multi-threaded processes (Linux setns(2) semantics).
///
/// We snapshot the table (cheap Arc-clone) so we don't hold PROC_TABLE
/// while reading inner state.
pub fn thread_count_of(pid: usize) -> Option<usize> {
    // First, find the tgid for pid — holds PROC_TABLE briefly.
    let tgid = {
        let table = PROC_TABLE.lock();
        table
            .iter()
            .find(|pl| pl.pid as usize == pid)
            .map(|pl| pl.tgid as usize)?
    };
    // Snapshot (Arc clones, no inner locks).
    let snapshot: Vec<Arc<ProcLock>> = PROC_TABLE.lock().clone();
    let count = snapshot
        .iter()
        .filter(|pl| pl.tgid as usize == tgid && pl.load_state() != State::Zombie)
        .count();
    Some(count)
}

/// Insert a new process.  Wraps the Pcb in a ProcLock and pushes to table.
/// Also allocates and links the Task struct.
pub fn enqueue(mut pcb: Pcb) {
    // Install the ProcLock first so the PCB has a stable address before the
    // Task back-pointer is created. Constructing Task::new(null_mut()) reads
    // through a null pointer and is undefined behavior.
    let proc_lock = ProcLock::new(pcb);
    {
        let mut inner = proc_lock.inner.lock();
        let pcb_ptr: *mut Pcb = &mut *inner;
        let task_ptr = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(Task::new(pcb_ptr)));
        unsafe {
            (*task_ptr).sched = inner.sched.clone();
            (*task_ptr).pid = inner.pid as u32;
        }
        inner.task = task_ptr;
    }
    PROC_TABLE.lock().push(proc_lock);
}

/// Remove a process entry from the table (called from zombify / reap).
/// Frees the associated Task heap allocation.
pub fn remove(pid: usize) {
    let mut table = PROC_TABLE.lock();
    if let Some(pos) = table.iter().position(|pl| pl.pid as usize == pid) {
        let pl = table.remove(pos);
        drop(table); // release table lock before touching inner
        let task_ptr = pl.inner.lock().task;
        if !task_ptr.is_null() {
            drop(unsafe { alloc::boxed::Box::from_raw(task_ptr) });
        }
    }
}

/// Return the raw Task pointer for `pid`.  Used by the scheduler to
/// enqueue a newly spawned task without re-locking the Pcb.
pub fn task_ptr_for_pid(pid: usize) -> *mut Task {
    find_proc_lock(pid)
        .and_then(|pl| {
            let inner = pl.inner.lock();
            let t = inner.task;
            if t.is_null() {
                None
            } else {
                Some(t)
            }
        })
        .unwrap_or(core::ptr::null_mut())
}
