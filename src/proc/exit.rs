//! Logical task exit with conservative lifetime retention.
//!
//! Exit publishes status and wakes the parent, but does not destroy an active
//! address space, Task, or PCB. RetiredTask keeps the PCB Arc and raw Task
//! allocation alive even after proc_table::remove unlinks the zombie.
//! No automatic resource reclaimer is installed. Physical memory and metadata
//! remain retained pending a scheduler quiescence protocol.

extern crate alloc;
use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::allocator::InterruptGuard;
use crate::arch::{api::Cpu, Arch};
use crate::mm::kstack::free_kstack;
use crate::proc::futex::{futex_wake_addr, robust_list_on_exit};
use crate::proc::process::{ProcLock, State};
use crate::proc::wait::encode_exit;
use crate::proc::{proc_table, scheduler, thread, wait};
use crate::uaccess::copy_to_user;

struct RetiredTask {
    _owner: Arc<ProcLock>,
    // Owns the Box allocation previously reachable through Pcb.task. Do not
    // reconstruct/drop that Box until no CPU/runqueue/continuation can use it.
    _task: usize,
}
static RETIRED_TASKS: spin::Mutex<Vec<RetiredTask>> = spin::Mutex::new(Vec::new());

pub fn retired_task_count() -> usize {
    let _interrupts = InterruptGuard::new();
    RETIRED_TASKS.lock().len()
}

fn clear_child_tid(pid: usize) {
    let va = scheduler::with_proc_mut(pid, |p, _| {
        core::mem::replace(&mut p.clear_child_tid_va, 0)
    }).unwrap_or(0);
    if va != 0 {
        let _ = copy_to_user(va, &0u32.to_ne_bytes());
        futex_wake_addr(va, 1);
    }
}
fn is_last_live_thread(pid: usize, tgid: usize) -> bool {
    scheduler::with_procs_ro(|procs| !procs.iter().any(|pl| {
        pl.pid as usize != pid && pl.tgid as usize == tgid && pl.load_state() != State::Zombie
    }))
}
fn reparent_orphans(pid: usize) {
    let init = if pid != 1 && scheduler::with_proc(1, |_| ()).is_some() { 1 } else { 0 };
    let children: Vec<usize> = scheduler::with_procs_ro(|procs| procs.iter()
        .filter(|pl| scheduler::with_proc(pl.pid as usize, |p| p.ppid == pid).unwrap_or(false))
        .map(|pl| pl.pid as usize).collect());
    for child in children {
        scheduler::with_proc_mut(child, |p, _| p.ppid = init);
    }
    if init != 0 { scheduler::wake_pid(init); }
}
fn ns_exit(pid: usize) {
    use crate::proc::namespace::INIT_NS;
    let Some(ns) = scheduler::with_proc(pid, |p| p.ns.clone()) else { return };
    let shared = |id: crate::proc::namespace::NsId,
                  get: fn(&crate::proc::process::Process) -> crate::proc::namespace::NsId| {
        scheduler::with_procs_ro(|procs| procs.iter().any(|pl| {
            pl.pid as usize != pid && pl.load_state() != State::Zombie
                && scheduler::with_proc(pl.pid as usize, get).unwrap_or(INIT_NS) == id
        }))
    };
    if ns.net != INIT_NS && !shared(ns.net, |p| p.ns.net) {
        crate::proc::net_ns::destroy_net_ns(ns.net);
    }
    if ns.mnt != INIT_NS && !shared(ns.mnt, |p| p.ns.mnt) {
        crate::proc::namespace::drop_mount_ns(ns.mnt);
    }
    if ns.uts != INIT_NS && !shared(ns.uts, |p| p.ns.uts) {
        crate::proc::namespace::drop_uts_ns(ns.uts);
    }
}

/// Retain ownership before publishing Zombie. Clearing Pcb.task prevents the
/// legacy proc_table::remove path from freeing a Task still used by schedule().
fn zombify(pid: usize, code: i32) -> Option<usize> {
    let owner = proc_table::find_proc_lock(pid)?;
    let _interrupts = InterruptGuard::new();
    let mut retired = RETIRED_TASKS.lock();
    let mut pcb = owner.inner.lock();
    if owner.load_state() == State::Zombie { return None; }
    let task = core::mem::replace(&mut pcb.task, core::ptr::null_mut());
    let stack = core::mem::replace(&mut pcb.kstack_top, 0);
    pcb.exit_code = encode_exit(code);
    let vfork_parent = pcb.vfork_parent;
    // Keep the PCB, its VMAs, root, and Task allocation alive across logical
    // reaping. The old task's PCB back-pointer remains valid during handoff.
    retired.push(RetiredTask { _owner: Arc::clone(&owner), _task: task as usize });
    owner.set_state(&mut pcb, State::Zombie);
    drop(pcb);
    drop(retired);
    // This is retirement only, per mm::kstack's current API contract.
    if stack != 0 { free_kstack(stack); }
    Some(vfork_parent)
}

fn cleanup_task(pid: usize, code: i32) {
    #[cfg(feature = "full_kernel")]
    {
        crate::ipc::sem::undo_on_exit(pid);
        crate::ipc::shm::cleanup_pid(pid);
        crate::mm::vmm::forget_process(pid);
    }
    robust_list_on_exit(pid);
    clear_child_tid(pid);
    thread::unregister_thread(pid);
    crate::proc::signal::altstack_clear_pid(pid);
    // There is no separate name registry to clear. Do not call the obsolete
    // syscall::proc_name_clear helper, which recursively calls itself.
    crate::proc::futex::futex_clear_pid(pid);
    crate::init::service_manager::on_process_exit(pid, code);
    crate::syscall::scheme::cleanup_pid(pid);
    crate::syscall::driver::cleanup_pid(pid);
    crate::ipc::endpoint_cleanup_pid(pid);
    crate::fs::process_fd::proc_fd_free(pid);
    reparent_orphans(pid);
    crate::proc::cgroup::cgroup_exit(pid);
}
fn publish_exit(pid: usize, code: i32) {
    if let Some(parent) = zombify(pid, code) {
        if parent != 0 { scheduler::wake_pid(parent); }
        wait::notify_exit(pid);
        if pid == 1 {
            crate::serial_println!("INIT_EXIT_CLEAN pid=1 status={} policy=scheduler-idle", code);
        }
    }
}

pub fn do_exit(pid: usize, code: i32) {
    if scheduler::with_proc(pid, |p| p.state == State::Zombie).unwrap_or(true) { return; }
    let tgid = thread::tgid_of(pid);
    cleanup_task(pid, code);
    if is_last_live_thread(pid, tgid) {
        crate::proc::signal::group_pending_clear(tgid);
        ns_exit(pid);
    }
    // Do not free_address_space here: CR3 and an outgoing syscall continuation
    // may still reference it. RetiredTask retains its ownership and metadata.
    publish_exit(pid, code);
    scheduler::schedule();
    loop { <Arch as Cpu>::halt(); }
}
pub fn sys_exit(status: i32) -> isize {
    do_exit(scheduler::current_pid(), status);
    0
}
pub fn sys_exit_group(status: i32) -> isize {
    let pid = scheduler::current_pid();
    let tgid = thread::tgid_of(pid);
    let siblings: Vec<usize> = scheduler::with_procs_ro(|procs| procs.iter()
        .filter(|pl| pl.pid as usize != pid && pl.tgid as usize == tgid
            && pl.load_state() != State::Zombie)
        .map(|pl| pl.pid as usize).collect());
    // This preserves the existing group-exit policy. Cross-CPU stopping and
    // cleanup serialization for running siblings still require integration.
    for sibling in siblings {
        cleanup_task(sibling, status);
        publish_exit(sibling, status);
    }
    do_exit(pid, status);
    0
}
