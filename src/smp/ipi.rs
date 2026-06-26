//! Inter-Processor Interrupts (IPI).
//!
//! IPI vector layout (x86_64, vectors 0xF0–0xFF):
//!   0xF0 — TLB shootdown
//!   0xF1 — scheduler reschedule
//!   0xF2 — function call (deferred work)
//!   0xFE — panic halt (NMI preferred but vector fallback)
//!
//! AArch64: uses SGI (software-generated interrupt) via ICC_SGI1R_EL1.
//! The target takes a GIC Group-1 SGI and calls `ipi::dispatch(cpu_id)`
//! from the IRQ handler.
//!
//! ## IPI pending bits  (`PercpuBlock::ipi_pending`, AtomicU32)
//!   bit 0 = TlbShootdown  → handle_tlb_shootdown(cpu_id)
//!   bit 1 = Reschedule    → proc::scheduler::schedule()
//!   bit 2 = FuncCall      → (future deferred-work queue)
//!   bit 3 = PanicHalt     → halt this CPU
//!
//! ## Send protocol
//!   1. Set target’s `ipi_pending` bit(s) with `fetch_or(..., Release)`.
//!   2. Call `send(target_cpu, kind)` — invokes the arch-specific mechanism
//!      (x86_64: APIC ICR write; AArch64: SGI via ICC_SGI1R_EL1).
//!
//! ## Receive protocol (AArch64, SGI interrupt handler)
//!   1. EOI the GIC interrupt.
//!   2. Call `ipi::dispatch(cpu_id)` — reads & clears `ipi_pending`
//!      atomically then dispatches each set bit.
//!
//! ## `schedule_on(task, cpu)` integration
//!   When the scheduler pins a task to a specific remote CPU,
//!   it calls `send_reschedule(cpu)` after enqueuing the task.
//!   The remote CPU wakes from `wfi` and calls `schedule()`.

use crate::smp::{cpu_info, num_online_cpus, MAX_CPUS};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// IPI vector base (x86_64).
pub const IPI_TLB_SHOOTDOWN: u8 = 0xF0;
pub const IPI_RESCHEDULE:    u8 = 0xF1;
pub const IPI_FUNC_CALL:     u8 = 0xF2;
pub const IPI_PANIC_HALT:    u8 = 0xFE;

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum IpiKind {
    TlbShootdown = 0,
    Reschedule   = 1,
    FuncCall     = 2,
    PanicHalt    = 3,
}

#[derive(Clone, Copy, Default)]
pub struct ShootdownReq {
    pub start: u64,
    pub end:   u64,
    pub asid:  u16,
    pub done:  bool,
}

static mut SHOOTDOWN_REQS: [ShootdownReq; MAX_CPUS] = [ShootdownReq {
    start: 0, end: 0, asid: 0, done: false,
}; MAX_CPUS];

static SHOOTDOWN_ACK: AtomicU64 = AtomicU64::new(0);

#[inline]
fn set_pending(cpu: u32, kind: IpiKind) {
    unsafe {
        crate::smp::percpu::PERCPU_BLOCKS[cpu as usize]
            .ipi_pending
            .fetch_or(1 << (kind as u8), Ordering::Release);
    }
}

/// Send an IPI to a single target CPU.
pub fn send(target_cpu: u32, kind: IpiKind) {
    set_pending(target_cpu, kind);
    #[cfg(target_arch = "x86_64")]
    {
        if let Some(info) = cpu_info(target_cpu) {
            let vec = match kind {
                IpiKind::TlbShootdown => IPI_TLB_SHOOTDOWN,
                IpiKind::Reschedule   => IPI_RESCHEDULE,
                IpiKind::FuncCall     => IPI_FUNC_CALL,
                IpiKind::PanicHalt    => IPI_PANIC_HALT,
            };
            crate::arch::x86_64::apic::send_ipi(info.hw_id as u32, vec);
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if let Some(info) = cpu_info(target_cpu) {
            // ICC_SGI1R_EL1: target list from MPIDR Aff0, INTID=0 (SGI0).
            let mpidr = info.hw_id;
            let aff1  = (mpidr >> 8) & 0xFF;
            let aff2  = (mpidr >> 16) & 0xFF;
            let aff3  = (mpidr >> 32) & 0xFF;
            let tl    = 1u64 << (mpidr & 0xF);
            let val   = (aff3 << 48) | (aff2 << 32) | (aff1 << 16) | tl;
            unsafe {
                core::arch::asm!(
                    "msr icc_sgi1r_el1, {}",
                    in(reg) val,
                    options(nostack)
                );
            }
        }
    }
}

/// Send a TLB shootdown IPI to all CPUs except `self_cpu`.
pub fn broadcast_tlb_shootdown(self_cpu: u32, start: u64, end: u64, asid: u16) {
    let n = num_online_cpus();
    for cpu in 0..n {
        if cpu == self_cpu { continue; }
        unsafe {
            SHOOTDOWN_REQS[cpu as usize] = ShootdownReq { start, end, asid, done: false };
        }
        set_pending(cpu, IpiKind::TlbShootdown);
    }
    // Flush self.
    crate::arch::tlb::flush_range(start, end, asid);
    // Now actually send the IPIs.
    for cpu in 0..n {
        if cpu == self_cpu { continue; }
        send(cpu, IpiKind::TlbShootdown);
    }
    // Wait for all targets to acknowledge.
    let mask: u64 = (1u64 << n) - 1;
    let self_bit = 1u64 << self_cpu;
    let expected = mask & !self_bit;
    while SHOOTDOWN_ACK.load(Ordering::Acquire) & expected != expected {
        core::hint::spin_loop();
    }
    SHOOTDOWN_ACK.store(0, Ordering::Release);
}

/// Called from the SGI interrupt handler (AArch64) or APIC IRQ handler (x86_64)
/// to dispatch pending IPI work for this CPU.
pub fn dispatch(cpu_id: u32) {
    let pending = unsafe {
        crate::smp::percpu::PERCPU_BLOCKS[cpu_id as usize]
            .ipi_pending
            .swap(0, Ordering::AcqRel)
    };
    if pending & (1 << IpiKind::TlbShootdown as u8) != 0 {
        handle_tlb_shootdown(cpu_id);
    }
    if pending & (1 << IpiKind::Reschedule as u8) != 0 {
        crate::proc::scheduler::schedule();
    }
    if pending & (1 << IpiKind::PanicHalt as u8) != 0 {
        crate::kernel::panic::halt_this_cpu();
    }
}

fn handle_tlb_shootdown(cpu_id: u32) {
    let req = unsafe { SHOOTDOWN_REQS[cpu_id as usize] };
    crate::arch::tlb::flush_range(req.start, req.end, req.asid);
    SHOOTDOWN_ACK.fetch_or(1 << cpu_id, Ordering::Release);
}

/// Convenience: send reschedule IPI to a specific CPU.
#[inline]
pub fn send_reschedule(cpu: u32) {
    send(cpu, IpiKind::Reschedule);
}

/// Convenience: halt all other CPUs (panic path).
pub fn broadcast_panic_halt(self_cpu: u32) {
    let n = num_online_cpus();
    for cpu in 0..n {
        if cpu != self_cpu { send(cpu, IpiKind::PanicHalt); }
    }
}
