//! SMP subsystem — CPU topology, AP bringup, per-CPU orchestration.
//!
//! Boot sequence (x86_64):
//!   BSP: kernel_main → smp::init() → enumerate MADTs →
//!        ap_boot::start_all_aps()
//!   AP:  trampoline (real→long) → ap_entry() →
//!        percpu_init() → scheduler::ap_idle()
//!
//! Boot sequence (aarch64):
//!   CPU0: kernel_main → smp::init() → PSCI_CPU_ON per secondary
//!   AP:   ap_entry() → percpu_init() → scheduler::ap_idle()

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub mod ipi;
pub mod percpu;

/// Maximum CPUs this build supports.
pub const MAX_CPUS: usize = 256;

/// Global count of CPUs that have completed bringup and are online.
pub static ONLINE_CPUS: AtomicU32 = AtomicU32::new(0);

/// Set to true by BSP once all subsystems are ready for APs to proceed.
pub static AP_GO: AtomicBool = AtomicBool::new(false);

/// Compact description of one logical CPU.
#[derive(Clone, Debug)]
pub struct CpuInfo {
    /// Logical (OS-assigned) CPU index, 0-based.
    pub cpu_id: u32,
    /// x86_64: Local APIC id.  aarch64: MPIDR.
    pub hw_id: u64,
    /// True if this is the bootstrap processor.
    pub is_bsp: bool,
}

static CPU_TABLE: crate::sync::spinlock::SpinLock<Vec<CpuInfo>> =
    crate::sync::spinlock::SpinLock::new(Vec::new());

/// Register a CPU in the topology table (called by firmware parsers).
pub fn register_cpu(info: CpuInfo) {
    CPU_TABLE.lock().push(info);
}

/// Look up a CPU by its logical id.
pub fn cpu_info(cpu_id: u32) -> Option<CpuInfo> {
    CPU_TABLE.lock().iter().find(|c| c.cpu_id == cpu_id).cloned()
}

/// Initialise SMP: parse topology, start APs.
pub fn init() {
    #[cfg(target_arch = "x86_64")]
    {
        crate::arch::x86_64::smp::enumerate_topology();
        let n = CPU_TABLE.lock().len();
        if n > 1 {
            crate::arch::x86_64::smp::start_all_aps();
        } else {
            log::info!("smp: uniprocessor mode");
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        use crate::arch::aarch64::smp as aa_smp;
        aa_smp::enumerate_topology();
        ONLINE_CPUS.fetch_add(1, Ordering::Relaxed);
        aa_smp::start_all_aps();
    }
}

/// Called by each AP after basic CPU init is complete.
pub fn ap_online() {
    ONLINE_CPUS.fetch_add(1, Ordering::Relaxed);
    while !AP_GO.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
}

/// Block BSP until all expected APs are online.
pub fn wait_for_aps(expected: u32) {
    while ONLINE_CPUS.load(Ordering::Acquire) < expected {
        core::hint::spin_loop();
    }
    AP_GO.store(true, Ordering::Release);
}
