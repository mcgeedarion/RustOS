//! Per-CPU storage.
//!
//! x86_64: GSBASE belongs to `arch::x86_64::gdt::PerCpu`, whose first slots
//! are consumed by syscall entry. Scheduler storage is indexed by that
//! structure's logical CPU id; it must never replace the syscall GS base.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use core::cell::UnsafeCell;

/// Size of the interrupt stack allocated per CPU (16 KiB).
pub const IST_SIZE: usize = 16 * 1024;
/// Size of the syscall stack per CPU (16 KiB).
pub const SYSCALL_STACK_SIZE: usize = 16 * 1024;

/// The per-CPU block.  Must be `repr(C)` so the assembly trampoline can
/// access `self_ptr` at a fixed offset (offset 0).
///
/// ## Field ordering note
///
/// `self_ptr` is at offset 0 (8 bytes on LP64), and `cpu_id` is therefore
/// at offset 8. This is not the GS-visible syscall structure.
///
/// `current_pid` is written by `scheduler::schedule()` on every CPU every
/// time a new task is selected.  `scheduler::current_pid()` reads it
/// directly without going through `current_task`, so it is accurate even
/// during the brief window when `current_task` is null between context
/// switches.  Initialised to 0 (idle / no process).
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
    ///
    /// Written by `scheduler::schedule()` unconditionally on every CPU
    /// whenever a new task is selected (or set to 0 when the runqueue
    /// drains).  Read by `scheduler::current_pid()` as the authoritative
    /// source; the global `CURRENT_PID` AtomicU32 in scheduler.rs is only
    /// a fallback for early-boot code that runs before percpu blocks are
    /// initialised.
    pub current_pid: u32,
    /// Runqueue for this CPU's CFS scheduler.
    pub runqueue: crate::proc::scheduler::RunQueue,
    /// Count of context switches on this CPU.
    pub ctx_switches: u64,
    /// IPI pending bitfield (one bit per `IpiKind`).
    pub ipi_pending: AtomicU32,
    /// Incremented on syscall entry, decremented on exit.
    /// When non-zero, scheduler ticks charge stime_ns instead of utime_ns.
    pub in_syscall: u32,
    /// Typed CPU-local values use fixed word-sized slots.
    pub(crate) cpu_local_slots: [UnsafeCell<usize>; 64],
}

impl PercpuBlock {
    const fn new() -> Self {
        Self {
            self_ptr: core::ptr::null_mut(),
            cpu_id: 0,
            node: 0,
            intr_disable_depth: 0,
            intr_was_enabled: false,
            ist_stack: [0; IST_SIZE],
            syscall_stack: [0; SYSCALL_STACK_SIZE],
            current_task: core::ptr::null_mut(),
            current_pid: 0,
            runqueue: crate::proc::scheduler::RunQueue::new(),
            ctx_switches: 0,
            ipi_pending: AtomicU32::new(0),
            in_syscall: 0,
            cpu_local_slots: [const { UnsafeCell::new(0) }; 64],
        }
    }
}

/// Static storage for up to MAX_CPUS per-CPU blocks.
pub static mut PERCPU_BLOCKS: [PercpuBlock; crate::smp::MAX_CPUS] =
    [const { PercpuBlock::new() }; crate::smp::MAX_CPUS];

/// Number of CPUs that have been initialised via `init()`.
static CPU_COUNT: AtomicU32 = AtomicU32::new(0);
static INITIALIZED: [AtomicBool; crate::smp::MAX_CPUS] =
    [const { AtomicBool::new(false) }; crate::smp::MAX_CPUS];

/// Returns the number of CPUs that have called `init()`.
#[inline]
pub fn cpu_count() -> u32 {
    CPU_COUNT.load(Ordering::Acquire)
}

/// Initialise per-CPU storage for `cpu_id` and install the block pointer
/// into the architecture-specific CPU register.
///
/// # Safety
/// Must be called exactly once per CPU before any other percpu access.
pub unsafe fn init(cpu_id: u32) {
    assert!((cpu_id as usize) < crate::smp::MAX_CPUS);
    if INITIALIZED[cpu_id as usize].load(Ordering::Acquire) {
        return;
    }
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
    blk.in_syscall = 0;

    INITIALIZED[cpu_id as usize].store(true, Ordering::Release);
    CPU_COUNT.fetch_max(cpu_id + 1, Ordering::Release);
}

/// Returns the current CPU's logical id.
#[inline(always)]
pub fn current_cpu_id() -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        crate::arch::x86_64::gdt::current_cpu_id()
    }
    #[cfg(target_arch = "aarch64")]
    { crate::arch::aarch64::cpu::cpu_id() as u32 }
    #[cfg(target_arch = "riscv64")]
    { crate::arch::riscv64::cpu::cpu_id() }
}

/// Returns a raw pointer to the current CPU's PercpuBlock.
#[inline(always)]
pub fn current_block() -> *mut PercpuBlock {
    #[cfg(target_arch = "x86_64")]
    if crate::arch::x86_64::gdt::current_percpu().is_null() {
        return core::ptr::null_mut();
    }
    let id = current_cpu_id() as usize;
    if id >= crate::smp::MAX_CPUS || !INITIALIZED[id].load(Ordering::Acquire) {
        return core::ptr::null_mut();
    }
    unsafe { core::ptr::addr_of_mut!(PERCPU_BLOCKS[id]) }
}

/// Disable interrupts and increment the nesting depth.  Returns `true` if
/// this is the outermost push (interrupts were enabled on entry).
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
    #[cfg(not(target_arch = "x86_64"))]
    {
        was_on = false;
    }
    if blk.intr_disable_depth == 0 {
        blk.intr_was_enabled = was_on;
    }
    blk.intr_disable_depth += 1;
    was_on
}

/// Decrement nesting depth; re-enable interrupts when depth reaches 0
/// if they were enabled before the outermost `push_off`.
#[inline]
pub fn pop_off() {
    let blk = unsafe { &mut *current_block() };
    assert!(blk.intr_disable_depth > 0, "pop_off underflow");
    blk.intr_disable_depth -= 1;
    if blk.intr_disable_depth == 0 && blk.intr_was_enabled {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("sti", options(nostack, preserves_flags));
        }
    }
}
