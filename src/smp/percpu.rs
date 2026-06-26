//! Per-CPU storage.
//!
//! x86_64:  `GSBASE` MSR points to a `PercpuBlock` in a dedicated per-CPU
//!          mapping.  The `gs:0` slot holds the self-pointer so that
//!          `current_cpu_id()` is a single `mov rax, gs:[0]` with no memory
//!          barrier.
//!
//! aarch64: The `TPIDR_EL1` system register holds the pointer to the block.

use core::sync::atomic::{AtomicU32, Ordering};

/// Size of the interrupt stack allocated per CPU (16 KiB).
pub const IST_SIZE: usize = 16 * 1024;
/// Size of the syscall stack per CPU (16 KiB).
pub const SYSCALL_STACK_SIZE: usize = 16 * 1024;

/// The per-CPU block.  Must be `repr(C)` so the assembly trampoline can
/// access `self_ptr` at a fixed offset (offset 0).
///
/// `self_ptr` is at offset 0 (8 bytes on LP64), `cpu_id` at offset 8.
/// The x86_64 `current_cpu_id()` fast path reads `gs:[8]`.
/// Do NOT reorder the first two fields without updating the inline asm.
#[repr(C, align(64))]
pub struct PercpuBlock {
    /// Pointer to self — must stay at offset 0.
    pub self_ptr: *mut PercpuBlock,
    /// Logical CPU id (0-based). Offset 8 on LP64.
    pub cpu_id: u32,
    /// NUMA node.
    pub node: u32,
    /// Nesting depth of `push_off` / `pop_off` for disabling interrupts.
    pub intr_disable_depth: u32,
    /// Were interrupts enabled before the outermost `push_off`?
    pub intr_was_enabled: bool,
    /// Kernel interrupt stack (IST1 on x86_64).
    pub ist_stack: [u8; IST_SIZE],
    /// Syscall/SYSENTER stack.
    pub syscall_stack: [u8; SYSCALL_STACK_SIZE],
    /// Pointer to the currently running `Task` on this CPU.
    pub current_task: *mut crate::proc::task::Task,
    /// PID of the currently running process on this CPU.
    pub current_pid: u32,
    /// Runqueue for this CPU's CFS scheduler.
    pub runqueue: crate::proc::scheduler::RunQueue,
    /// Count of context switches on this CPU.
    pub ctx_switches: u64,
    /// IPI pending bitfield (one bit per `IpiKind`).
    pub ipi_pending: AtomicU32,
    /// Incremented on syscall entry, decremented on exit.
    pub in_syscall: u32,
}

/// Static storage for up to MAX_CPUS per-CPU blocks.
pub static mut PERCPU_BLOCKS: [PercpuBlock; crate::smp::MAX_CPUS] = {
    unsafe { core::mem::zeroed() }
};

/// Number of CPUs that have been initialised via `init()`.
static CPU_COUNT: AtomicU32 = AtomicU32::new(0);

pub fn cpu_count() -> u32 {
    CPU_COUNT.load(Ordering::Acquire)
}

/// Initialise per-CPU storage for `cpu_id` and install the block pointer
/// into the architecture-specific CPU register.
///
/// # Safety
/// Must be called exactly once per CPU before any other percpu access.
pub unsafe fn init(cpu_id: u32) {
    let blk = &mut PERCPU_BLOCKS[cpu_id as usize];
    blk.self_ptr = blk as *mut PercpuBlock;
    blk.cpu_id = cpu_id;
    if let Some(info) = crate::smp::cpu_info(cpu_id) {
        blk.node = info.node;
    }
    blk.intr_disable_depth = 0;
    blk.intr_was_enabled = false;
    blk.current_task = core::ptr::null_mut();
    blk.current_pid = 0;
    blk.ctx_switches = 0;
    blk.ipi_pending = AtomicU32::new(0);
    blk.runqueue = crate::proc::scheduler::RunQueue::new();
    CPU_COUNT.fetch_max(cpu_id + 1, Ordering::Release);

    #[cfg(target_arch = "x86_64")]
    {
        let addr = blk as *mut PercpuBlock as u64;
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xC000_0101u32,
            in("eax") (addr as u32),
            in("edx") ((addr >> 32) as u32),
            options(nostack, preserves_flags)
        );
    }
    #[cfg(target_arch = "aarch64")]
    {
        let addr = blk as *mut PercpuBlock as u64;
        core::arch::asm!(
            "msr tpidr_el1, {}",
            in(reg) addr,
            options(nostack)
        );
    }
}

#[inline(always)]
pub fn current_cpu_id() -> u32 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let id: u32;
        core::arch::asm!("mov {:e}, gs:[8]", out(reg) id,
            options(nostack, preserves_flags, readonly));
        return id;
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let ptr: u64;
        core::arch::asm!("mrs {}, tpidr_el1", out(reg) ptr,
            options(nostack, readonly));
        return (*(ptr as *const PercpuBlock)).cpu_id;
    }
    #[allow(unreachable_code)]
    0
}

#[inline(always)]
pub fn current_block() -> *mut PercpuBlock {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let ptr: *mut PercpuBlock;
        core::arch::asm!("mov {}, gs:[0]", out(reg) ptr,
            options(nostack, preserves_flags, readonly));
        return ptr;
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let ptr: u64;
        core::arch::asm!("mrs {}, tpidr_el1", out(reg) ptr,
            options(nostack, readonly));
        return ptr as *mut PercpuBlock;
    }
    #[allow(unreachable_code)]
    core::ptr::null_mut()
}

#[inline]
pub fn push_off() -> bool {
    let blk = unsafe { &mut *current_block() };
    let was_on;
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let flags: u64;
        core::arch::asm!("pushfq; pop {}", out(reg) flags, options(nostack));
        was_on = (flags & (1 << 9)) != 0;
        core::arch::asm!("cli", options(nostack, preserves_flags));
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let daif: u64;
        core::arch::asm!("mrs {}, daif", out(reg) daif, options(nostack));
        was_on = (daif & (1 << 7)) == 0; // IRQ bit clear = enabled
        core::arch::asm!("msr daifset, #2", options(nostack));
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    { was_on = false; }
    if blk.intr_disable_depth == 0 { blk.intr_was_enabled = was_on; }
    blk.intr_disable_depth += 1;
    was_on
}

#[inline]
pub fn pop_off() {
    let blk = unsafe { &mut *current_block() };
    assert!(blk.intr_disable_depth > 0, "pop_off underflow");
    blk.intr_disable_depth -= 1;
    if blk.intr_disable_depth == 0 && blk.intr_was_enabled {
        #[cfg(target_arch = "x86_64")]
        unsafe { core::arch::asm!("sti", options(nostack, preserves_flags)); }
        #[cfg(target_arch = "aarch64")]
        unsafe { core::arch::asm!("msr daifclr, #2", options(nostack)); }
    }
}
