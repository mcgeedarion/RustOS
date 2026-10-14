extern crate alloc;

use alloc::vec::Vec;
use crate::arch::{
    api::{PageFlags, Paging},
    Arch,
};
use crate::fs::process_fd::proc_fd_fork;
use crate::mm::mmap::{VmaKind, PAGE, PROT_EXEC, PROT_WRITE};
use crate::proc::process::State;
use crate::proc::{cgroup, proc_table, scheduler};

/// Build an independent child address space by copying resident pages described
/// by the parent's VMAs. This avoids claiming COW support where the architecture
/// trap path cannot yet resolve COW faults consistently.
fn clone_user_mappings(parent: &crate::proc::process::Pcb) -> Result<usize, isize> {
    if parent.user_pagetable == 0 {
        return Err(-12);
    }
    let root = <Arch as Paging>::new_user_address_space().ok_or(-12)?;
    let mut copied: Vec<(usize, usize)> = Vec::new();

    for vma in &parent.vmas {
        for va in (vma.start..vma.end).step_by(PAGE) {
            if <Arch as Paging>::virt_to_phys(root, va).is_some() {
                continue;
            }
            let Some(source_pa) = <Arch as Paging>::virt_to_phys(parent.user_pagetable, va) else {
                // Uncommitted/lazy pages remain uncommitted in the child.
                continue;
            };
            let source_frame = source_pa & !(PAGE - 1);
            let flags = {
                let mut flags = PageFlags::PRESENT | PageFlags::USER;
                if vma.prot & PROT_WRITE != 0 {
                    flags |= PageFlags::WRITE;
                }
                if vma.prot & PROT_EXEC == 0 {
                    flags |= PageFlags::NX;
                }
                flags
            };

            if matches!(vma.kind, VmaKind::PhysMap(_)) {
                if !<Arch as Paging>::map_page(root, va, source_frame, flags) {
                    rollback_user_mappings(root, &copied);
                    return Err(-12);
                }
                continue;
            }

            let Some(destination_pa) = crate::mm::pmm::alloc_page() else {
                rollback_user_mappings(root, &copied);
                return Err(-12);
            };
            unsafe {
                core::ptr::copy_nonoverlapping(
                    source_frame as *const u8,
                    destination_pa as *mut u8,
                    PAGE,
                );
            }

            if !<Arch as Paging>::map_page(root, va, destination_pa, flags) {
                crate::mm::pmm::free_page(destination_pa);
                rollback_user_mappings(root, &copied);
                return Err(-12);
            }
            copied.push((va, destination_pa));
        }
    }

    Ok(root)
}

fn rollback_user_mappings(root: usize, copied: &[(usize, usize)]) {
    for (va, pa) in copied {
        let _ = <Arch as Paging>::unmap_page(root, *va);
        crate::mm::pmm::free_page(*pa);
    }
    <Arch as Paging>::free_page_table(root);
}

/// Fork creates a separately scheduled child with a copied userspace return
/// context. The child resumes at the instruction following fork with return 0.
pub fn sys_fork() -> isize {
    let parent_pid = scheduler::current_pid() as usize;
    let parent = match scheduler::with_proc(parent_pid, Clone::clone) {
        Some(pcb) => pcb,
        None => return -3, // ESRCH
    };

    let child_pid = proc_table::next_pid() as usize;
    let child_root = match clone_user_mappings(&parent) {
        Ok(root) => root,
        Err(errno) => return errno,
    };

    let mut child = parent.clone();
    child.pid = child_pid;
    child.ppid = parent_pid;
    child.tgid = child_pid;
    child.tg_id = child_pid;
    child.state = State::Ready;
    child.exit_code = 0;
    child.task = core::ptr::null_mut();
    child.user_pagetable = child_root;
    child.vmas = parent.vmas.clone();
    child.pending_signals.clear();
    child.signal_handlers = parent.fork_signal_handlers();
    child.sched.on_rq = false;
    child.ctx.pc = child.user_context.pc;
    child.ctx.sp = child.user_context.sp;
    child.pc = child.user_context.pc;
    child.sp = child.user_context.sp;

    #[cfg(target_arch = "x86_64")]
    { child.user_context.regs[0] = 0; } // RAX
    #[cfg(target_arch = "aarch64")]
    { child.user_context.regs[0] = 0; } // x0
    #[cfg(target_arch = "riscv64")]
    { child.user_context.regs[10] = 0; } // a0

    scheduler::enqueue(child);
    proc_fd_fork(parent_pid, child_pid);
    cgroup::cgroup_fork(parent_pid, child_pid);

    let task = scheduler::task_ptr_for_pid(child_pid);
    if task.is_null() {
        scheduler::remove_proc(child_pid);
        return -12;
    }
    scheduler::enqueue_task(task);

    child_pid as isize
}
