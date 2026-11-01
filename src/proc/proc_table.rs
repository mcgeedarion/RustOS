//! Authoritative process table. Lookup clones an Arc before locking a PCB.
//! Never block on ProcLock::inner while holding PROC_TABLE. The reaping
//! primitive uses try_lock only and retries if a PCB is busy. No user copy,
//! allocation, or resource destruction occurs inside that critical section.
extern crate alloc;
use crate::allocator::InterruptGuard;
use crate::proc::process::{Pcb, ProcLock, State};
use crate::proc::task_types::Task;
use crate::sync::spinlock::SpinLock;
use alloc::sync::Arc;
use alloc::vec::Vec;

static PROC_TABLE: SpinLock<Vec<Arc<ProcLock>>> = SpinLock::new(Vec::new());
static NEXT_PID: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(1);
pub fn next_pid() -> u32 { NEXT_PID.fetch_add(1, core::sync::atomic::Ordering::Relaxed) }
pub fn proc_count() -> usize { PROC_TABLE.lock().len() }
pub fn find_proc_lock(pid: usize) -> Option<Arc<ProcLock>> {
    PROC_TABLE.lock().iter().find(|pl| pl.pid as usize == pid).cloned()
}
pub fn with_proc<T, F: FnOnce(&Pcb) -> T>(pid: usize, f: F) -> Option<T> {
    let owner = find_proc_lock(pid)?;
    let pcb = owner.inner.lock();
    Some(f(&pcb))
}
pub fn with_proc_mut<T, F: FnOnce(&mut Pcb, &ProcLock) -> T>(pid: usize, f: F) -> Option<T> {
    let owner = find_proc_lock(pid)?;
    let mut pcb = owner.inner.lock();
    Some(f(&mut pcb, &owner))
}
pub fn with_procs_ro<T, F: FnOnce(&Vec<Arc<ProcLock>>) -> T>(f: F) -> T {
    let snapshot = PROC_TABLE.lock().clone();
    f(&snapshot)
}
/// The callback must not acquire PCB locks or call process-table helpers.
pub fn with_procs_mut<T, F: FnOnce(&mut Vec<Arc<ProcLock>>) -> T>(f: F) -> T {
    f(&mut PROC_TABLE.lock())
}
pub fn thread_count_of(pid: usize) -> Option<usize> {
    let tgid = find_proc_lock(pid)?.tgid;
    Some(with_procs_ro(|snapshot| snapshot.iter()
        .filter(|pl| pl.tgid == tgid && pl.load_state() != State::Zombie).count()))
}
pub fn enqueue(pcb: Pcb) {
    let owner = ProcLock::new(pcb);
    {
        let mut inner = owner.inner.lock();
        let pcb_ptr = &mut *inner as *mut Pcb;
        let task = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(Task::new(pcb_ptr)));
        unsafe { (*task).sched = inner.sched.clone(); (*task).pid = inner.pid as u32; }
        inner.task = task;
    }
    PROC_TABLE.lock().push(owner);
}

/// Atomically unlink this exact retired zombie, never a replacement with the
/// same PID. False means it was consumed/reparented, is not retired, or is busy;
/// callers must rescan. PCB inspection is nonblocking under the table lock,
/// so a thread holding inner and waiting for the table cannot deadlock us.
/// This is logical reaping only: exit's retention queue still owns resources.
pub fn try_reap_zombie(parent: usize, expected: &Arc<ProcLock>) -> bool {
    let removed = {
        let _interrupts = InterruptGuard::new();
        let mut table = PROC_TABLE.lock();
        let Some(index) = table.iter().position(|pl| Arc::ptr_eq(pl, expected)) else { return false };
        let Some(pcb) = expected.inner.try_lock() else { return false };
        if pcb.ppid != parent || pcb.state != State::Zombie || !pcb.task.is_null() {
            return false;
        }
        let removed = table.remove(index);
        drop(pcb);
        removed
    };
    // No Task Box or address space is freed here. Drop the table's Arc only
    // after releasing its locks; expected and exit's retirement owner remain.
    drop(removed);
    true
}

/// Legacy removal for rollback/internal callers, not wait-family consumption.
pub fn remove(pid: usize) {
    let removed = {
        let mut table = PROC_TABLE.lock();
        table.iter().position(|pl| pl.pid as usize == pid).map(|index| table.remove(index))
    };
    if let Some(owner) = removed {
        let task = owner.inner.lock().task;
        if !task.is_null() { drop(unsafe { alloc::boxed::Box::from_raw(task) }); }
    }
}
pub fn task_ptr_for_pid(pid: usize) -> *mut Task {
    with_proc(pid, |pcb| pcb.task).unwrap_or(core::ptr::null_mut())
}
