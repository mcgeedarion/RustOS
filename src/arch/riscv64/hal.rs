//! RISC-V 64 HAL implementation.

use core::arch::asm;

/// Architecture implementation marker.
pub struct ArchImpl;

/// Initialize the RISC-V HAL.
#[cfg(any(not(feature = "boot_minimal"), feature = "userspace_boot"))]
pub fn init(_boot_info: &'static crate::init::boot_info::BootInfo) -> ! {
    // For now, just halt in a loop - this is a minimal stub.
    // A full implementation would set up paging, interrupts, etc.
    loop {
        unsafe {
            asm!("wfi");
        }
    }
}

/// Early HAL initialization (before boot services exit).
pub fn early_init() {
    // Disable interrupts during initialization.
    unsafe {
        asm!("csrc sstatus, {bit}", bit = const 1 << 1); // Clear SIE (Supervisor Interrupt Enable)
    }
}
