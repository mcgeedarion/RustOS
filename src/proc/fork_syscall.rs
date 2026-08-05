use crate::fs::process_fd::proc_fd_fork;
use crate::proc::process::State;
use crate::proc::{cgroup, proc_table, scheduler};

/// Minimal fork implementation that creates a child process-table entry.
///
/// The child inherits the parent's process state, file-descriptor table, signal
/// dispositions, cgroup membership, and scheduling attributes.  Architecture
/// return-frame fixup is intentionally small here: register 0 is cleared in the
/// child context so future context-switch return code can observe the usual
/// fork child return value.
pub fn sys_fork() -> isize {
    let parent_pid = scheduler::current_pid() as usize;
    let parent = match scheduler::with_proc(parent_pid, Clone::clone) {
        Some(pcb) => pcb,
        None => return -3, // ESRCH
    };

    let child_pid = proc_table::next_pid() as usize;
    let mut child = parent.clone();
    child.pid = child_pid;
    child.ppid = parent_pid;
    child.tgid = child_pid;
    child.tg_id = child_pid;
    child.state = State::Ready;
    child.exit_code = 0;
    child.task = core::ptr::null_mut();
    child.pending_signals.clear();
    child.signal_handlers = parent.fork_signal_handlers();
    child.sched.on_rq = false;
    child.ctx.regs[0] = 0;

    scheduler::enqueue(child);
    proc_fd_fork(parent_pid, child_pid);
    cgroup::cgroup_fork(parent_pid, child_pid);

    let task = scheduler::task_ptr_for_pid(child_pid);
    scheduler::enqueue_task(task);

    child_pid as isize
}
