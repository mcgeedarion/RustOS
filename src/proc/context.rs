/// Saved process CPU context.
///
/// This minimal representation is enough for process-table construction and
/// can be extended by architecture-specific switch code as the full context
/// switch path is wired back up.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Context {
    pub regs: [usize; 16],
    pub sp: usize,
    pub pc: usize,
}

impl Context {
    pub const fn zero() -> Self {
        Self {
            regs: [0; 16],
            sp: 0,
            pc: 0,
        }
    }
}


/// Architecture-neutral saved userspace register state.
///
/// regs uses architectural integer register numbering: x86-64 uses its
/// documented slot map in arch/x86_64/syscall.rs, AArch64 uses x0-x30, and
/// RISC-V uses x0-x31. Entry stubs capture this frame before dispatch so a
/// fork child can resume after the syscall with its return register set to zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserContext {
    pub regs: [usize; 32],
    pub pc: usize,
    pub sp: usize,
    pub flags: usize,
}

impl UserContext {
    pub const fn zero() -> Self {
        Self {
            regs: [0; 32],
            pc: 0,
            sp: 0,
            flags: 0,
        }
    }
}


/// First-run path for a schedulable user process.
///
/// User tasks have a valid mapped ELF entry and a complete userspace return
/// context. The architecture handoff never returns to the caller's old stack.
pub unsafe fn restore(task: *mut crate::proc::task_types::Task) -> ! {
    if task.is_null() {
        crate::serial_println!("context: refusing to restore a null task");
        loop {
            <crate::arch::Arch as crate::arch::api::Cpu>::halt();
        }
    }

    let pid = (*task).pid as usize;
    let saved = crate::proc::scheduler::with_proc(pid, |pcb| {
        (
            pcb.user_pagetable,
            core::ptr::addr_of!(pcb.user_context),
            pcb.user_context.pc,
            pcb.user_context.sp,
        )
    });

    let (root, context, pc, sp) = match saved {
        Some(value) => value,
        None => {
            crate::serial_println!("context: PID {} has no PCB", pid);
            loop {
                crate::arch::ArchImpl::halt();
            }
        }
    };

    if root == 0 || pc == 0 || sp == 0 {
        crate::serial_println!(
            "context: refusing invalid user context pid={} root={:#x} pc={:#x} sp={:#x}",
            pid,
            root,
            pc,
            sp
        );
        loop {
            crate::arch::ArchImpl::halt();
        }
    }

    #[cfg(target_arch = "x86_64")]
    crate::arch::x86_64::uentry::sysret_to_user_context(root, context);
    #[cfg(target_arch = "aarch64")]
    crate::arch::aarch64::uentry::eret_to_user_context(root, context);
    #[cfg(target_arch = "riscv64")]
    crate::arch::riscv64::uentry::sret_to_user_context(root, context);
}

/// Preemptive user-task switch.
///
/// The architecture trap/syscall entry path saves the outgoing userspace
/// frame before the scheduler is entered. Restore the selected task directly
/// from its saved user frame rather than pretending that the minimal kernel
/// Context struct is a complete callee-saved register frame.
pub unsafe fn switch(
    _previous: *mut crate::proc::task_types::Task,
    next: *mut crate::proc::task_types::Task,
) -> ! {
    restore(next)
}
