//! RISC-V 64 HAL implementation.

use core::arch::asm;

/// Architecture implementation marker.
pub struct ArchImpl;

/// Initialize the RISC-V HAL.
#[cfg(any(not(feature = "boot_minimal"), feature = "userspace_boot"))]
pub fn init(boot_info: &'static crate::init::boot_info::BootInfo) -> ! {
    // Print boot information
    log!("RustOS riscv64 HAL initialized");
    log!("ACPI RSDP at: 0x{:X}", boot_info.rsdp_phys);
    
    if !boot_info.initramfs.is_empty() {
        log!("Initramfs: 0x{:X} - 0x{:X}", 
             boot_info.initramfs.start(),
             boot_info.initramfs.end());
    }
    
    // Enable interrupts after initialization
    unsafe {
        asm!(
            "csrs sstatus, {bit}",
            bit = const 1 << 1, // Set SIE (Supervisor Interrupt Enable)
        );
    }
    
    // Hand off to the scheduler
    #[cfg(not(feature = "userspace_boot"))]
    {
        crate::proc::scheduler::run_scheduler();
    }
    
    #[cfg(feature = "userspace_boot")]
    {
        crate::userspace_boot::init(boot_info);
    }
}

/// Early HAL initialization (before boot services exit).
pub fn early_init() {
    // Disable interrupts during initialization.
    unsafe {
        asm!("csrc sstatus, {bit}", bit = const 1 << 1); // Clear SIE (Supervisor Interrupt Enable)
    }
}
