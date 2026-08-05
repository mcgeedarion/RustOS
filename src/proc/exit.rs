//! Process / thread exit path.
//!
//! do_exit(pid, code) is the single canonical exit function.
//! sys_exit  [NR 60]  calls do_exit for the current thread.
//! sys_exit_group [NR 231] calls do_exit for every thread in the group.
//!
//! Exit sequence:
//!   1. robust_list_on_exit  — wake futex waiters on robust mutexes
//!   2. clear_child_tid      — zero futex word + FUTEX_WAKE (unblocks pthread_join)
//!   3. unregister_thread
//!   4. altstack_clear_pid + proc_name_clear + futex_clear_pid
//!   5. proc_fd_free         — close all open fds
//!   6. cgroup_exit          — remove PID from cgroup, maybe free cgroup
//!   7. reparent_orphans     — adopt children to init (PID 1)
//!   8. free_address_space (last thread in group only)
//!   9. group_pending_clear (last thread in group only)
//!  10. ns_exit (last thread in group only)
//!  11. free_kstack + State -> Zombie + exit_code = encode_exit(code)
//!  12. wake vfork_parent
//!  13. notify_exit (wakes parent waitpid, sends SIGCHLD)
//!  14. schedule()   — never returns
//!
//! PID 1 exit policy:
//!   RustOS treats `/init` exiting as a valid smoke-test completion path during
//!   early bring-up.  PID 1 follows the same teardown path as every other
//!   process, emits a serial marker before resources are destroyed, becomes a
//!   zombie, and the boot CPU falls back to the scheduler/idle loop.  Later
//!   service-manager policy may choose to reboot or respawn init, but the low
//!   level exit syscall must remain panic-free for both zero and non-zero
//!   statuses.

extern crate alloc;

use crate::arch::api::Cpu;
use crate::arch::Arch;
use crate::mm::kstack::free_kstack;
use crate::mm::mmap::free_address_space;
use crate::proc::futex::{futex_wake_addr, robust_list_on_exit};
use crate::proc::process::State;
use crate::proc::wait::encode_exit;
use crate::proc::{scheduler, thread, wait};
use crate::uaccess::copy_to_user;

fn clear_child_tid(pid: usize) {
    let va = scheduler::with_proc_mut(pid, |p, pl| {
        let va = p.clear_child_tid_va;
        p.clear_child_tid_va = 0;
        let _ = pl;
        va
    })
    .unwrap_or(0);
    if va == 0 {
        return;
    }
    let _ = copy_to_user(va, &0u32.to_ne_bytes());
    futex_wake_addr(va, 1);
}

fn is_last_live_thread(pid: usize, tgid: usize) -> bool {
    scheduler::with_procs_ro(|pl_vec| {
        !pl_vec.iter().any(|pl| {
            pl.pid as usize != pid && pl.tgid as usize == tgid && pl.load_state() != State::Zombie
        })
    })
}

/// Reparent all children of `pid` to init (PID 1).
///
/// If PID 1 itself exits (shouldn't happen in a healthy system) reparent to
/// PID 0 (the idle task / kernel), which won't collect them but prevents UAF.
fn reparent_orphans(pid: usize) {
    // Determine the new parent — prefer PID 1 (init), fall back to 0.
    let init_pid: usize = if pid != 1 && scheduler::with_proc(1, |_| ()).is_some() {
        1
    } else {
        0
    };

    // Collect PIDs of direct children.
    let children: alloc::vec::Vec<usize> = scheduler::with_procs_ro(|pl_vec| {
        pl_vec
            .iter()
            .filter(|pl| scheduler::with_proc(pl.pid as usize, |p| p.ppid == pid).unwrap_or(false))
            .map(|pl| pl.pid as usize)
            .collect()
    });

    for child in children {
        scheduler::with_proc_mut(child, |p, _pl| {
            p.ppid = init_pid;
        });
    }

    // Wake init so it can reap any newly-adopted zombies.
    if init_pid != 0 {
        scheduler::wake_pid(init_pid);
    }
}

fn ns_exit(pid: usize) {
    use crate::proc::namespace::INIT_NS;

    let ns = match scheduler::with_proc(pid, |p| p.ns.clone()) {
        Some(n) => n,
        None => return,
    };

    let shared_by_other =
        |ns_id: crate::proc::namespace::NsId,
         getter: fn(&crate::proc::process::Process) -> crate::proc::namespace::NsId|
         -> bool {
            scheduler::with_procs_ro(|pl_vec| {
                pl_vec.iter().any(|pl| {
                    pl.pid as usize != pid
                        && pl.load_state() != State::Zombie
                        && scheduler::with_proc(pl.pid as usize, getter).unwrap_or(INIT_NS) == ns_id
                })
            })
        };

    if ns.net != INIT_NS && !shared_by_other(ns.net, |p| p.ns.net) {
        crate::proc::net_ns::destroy_net_ns(ns.net);
    }
    if ns.mnt != INIT_NS && !shared_by_other(ns.mnt, |p| p.ns.mnt) {
        crate::proc::namespace::drop_mount_ns(ns.mnt);
    }
    if ns.uts != INIT_NS && !shared_by_other(ns.uts, |p| p.ns.uts) {
        crate::proc::namespace::drop_uts_ns(ns.uts);
    }
}

fn zombify(pid: usize, code: i32) -> usize {
    let (kstack_top, vfork_parent) = scheduler::with_proc_mut(pid, |p, pl| {
        let ks = p.kstack_top;
        p.kstack_top = 0;
        p.exit_code = encode_exit(code);
        pl.set_state(p, State::Zombie);
        (ks, p.vfork_parent)
    })
    .unwrap_or((0, 0));

    if kstack_top != 0 {
        free_kstack(kstack_top);
    }
    vfork_parent
}

fn emit_pid1_exit_marker(pid: usize, code: i32) {
    if pid == 1 {
        crate::serial_println!(
            "INIT_EXIT_CLEAN pid=1 status={} policy=scheduler-idle",
            code
        );
    }
}

pub fn do_exit(pid: usize, code: i32) {
    let tgid = thread::tgid_of(pid);

    emit_pid1_exit_marker(pid, code);

    robust_list_on_exit(pid);
    clear_child_tid(pid);
    thread::unregister_thread(pid);

    crate::proc::signal::altstack_clear_pid(pid);
    crate::syscall::proc_name_clear(pid);
    crate::proc::futex::futex_clear_pid(pid);

    crate::init::service_manager::on_process_exit(pid, code);
    crate::syscall::scheme::cleanup_pid(pid);
    crate::syscall::driver::cleanup_pid(pid);
    crate::ipc::endpoint_cleanup_pid(pid);

    crate::fs::process_fd::proc_fd_free(pid);

    // Reparent children to init *before* we tear down the address space,
    // so that any orphaned zombies are visible to init immediately.
    reparent_orphans(pid);

    let last = is_last_live_thread(pid, tgid);
    if last {
        let user_pagetable = scheduler::with_proc(pid, |p| p.user_pagetable).unwrap_or(0);
        free_address_space(pid, user_pagetable);
        crate::proc::signal::group_pending_clear(tgid);
        ns_exit(pid);
    }
    crate::proc::cgroup::cgroup_exit(pid);

    let vfork_parent = zombify(pid, code);
    if vfork_parent != 0 {
        scheduler::wake_pid(vfork_parent);
    }
    wait::notify_exit(pid);
    scheduler::schedule();

    loop {
        <Arch as Cpu>::halt();
    }
}

pub fn sys_exit(status: i32) -> isize {
    do_exit(scheduler::current_pid(), status);
    0
}

pub fn sys_exit_group(status: i32) -> isize {
    let pid = scheduler::current_pid();
    let tgid = thread::tgid_of(pid);

    let siblings: alloc::vec::Vec<usize> = scheduler::with_procs_ro(|pl_vec| {
        pl_vec
            .iter()
            .filter(|pl| pl.pid as usize != pid && pl.tgid as usize == tgid)
            .map(|pl| pl.pid as usize)
            .collect()
    });

    for sibling in siblings {
        robust_list_on_exit(sibling);
        clear_child_tid(sibling);
        thread::unregister_thread(sibling);
        crate::proc::signal::altstack_clear_pid(sibling);
        crate::syscall::proc_name_clear(sibling);
        crate::proc::futex::futex_clear_pid(sibling);
        crate::init::service_manager::on_process_exit(sibling, status);
        crate::syscall::scheme::cleanup_pid(sibling);
        crate::syscall::driver::cleanup_pid(sibling);
        crate::ipc::endpoint_cleanup_pid(sibling);
        crate::fs::process_fd::proc_fd_free(sibling);
        reparent_orphans(sibling);
        let user_pagetable_sibling =
            scheduler::with_proc(sibling, |p| p.user_pagetable).unwrap_or(0);
        if user_pagetable_sibling != 0 {
            free_address_space(sibling, user_pagetable_sibling);
        }
        crate::proc::cgroup::cgroup_exit(sibling);
        let vfork_parent = zombify(sibling, status);
        if vfork_parent != 0 {
            scheduler::wake_pid(vfork_parent);
        }
        wait::notify_exit(sibling);
    }

    do_exit(pid, status);
    0
}
