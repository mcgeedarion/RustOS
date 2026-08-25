//! RISC-V 64 CPU helpers.

use core::arch::asm;

/// Read the current hart ID.
/// In S-mode under UEFI, we may not have direct access to mhartid.
/// Returns 0 as default (can be enhanced with SBI call if available).
#[inline]
pub fn cpu_id() -> u32 {
    // Try reading from SBI or return cached value
    // For now, return 0 as a placeholder
    0
}

/// Enable FPU access in supervisor mode.
/// Sets sstatus.FS = 0b11 (initial/dirty state).
#[inline]
pub unsafe fn enable_fpu() {
    let mut sstatus: usize;
    asm!(
        "csrr {sstatus}, sstatus",
        sstatus = out(reg) sstatus,
        options(nostack)
    );
    sstatus |= 0b11 << 13; // FS = 0b11
    asm!(
        "csrw sstatus, {sstatus}",
        sstatus = in(reg) sstatus,
        options(nostack)
    );
}

/// Read time CSR (monotonic counter).
#[inline]
pub fn read_time() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        asm!(
            "csrr {lo}, time",
            lo = out(reg) lo,
            options(nostack, nomem)
        );
        asm!(
            "csrr {hi}, timeh",
            hi = out(reg) hi,
            options(nostack, nomem)
        );
    }
    ((hi as u64) << 32) | (lo as u64)
}

/// Read the current privilege mode from sstatus.
#[inline]
pub fn current_privilege() -> u8 {
    let sstatus: usize;
    unsafe {
        asm!(
            "csrr {sstatus}, sstatus",
            sstatus = out(reg) sstatus,
            options(nostack, nomem)
        );
    }
    if sstatus & (1 << 8) != 0 {
        1 // Supervisor mode
    } else {
        0 // User mode
    }
}

/// Flush the instruction cache (if needed).
#[inline]
pub fn fence_i() {
    unsafe {
        asm!("fence.i", options(nostack, nomem));
    }
}

/// Full memory barrier.
#[inline]
pub fn fence() {
    unsafe {
        asm!("fence rw, rw", options(nostack, nomem));
    }
}
