#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod apic;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod cpu;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod gdt;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod hal;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod idt;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod interrupts;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod kernel_main;
pub mod mem_layout;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod memory;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod paging;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod pci;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod power;
pub mod serial;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod syscall;
#[cfg(target_os = "uefi")]
pub mod uefi_boot_stack;
#[cfg(any(feature = "uefi_boot", feature = "boot_minimal"))]
pub mod uefi_entry;
#[cfg(not(any(feature = "uefi_boot", feature = "boot_minimal")))]
pub mod uefi_entry {
    /// Compatibility RSDP placeholder for non-UEFI diagnostic builds.
    pub static mut RSDP_PHYS: u64 = 0;
}
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod uentry;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod xsave;

/// x86_64 early/kernel boot hook used by the common entry point.
#[cfg(not(feature = "boot_minimal"))]
pub fn init(boot_info: &'static crate::init::boot_info::BootInfo) -> ! {
    #[cfg(feature = "userspace_boot")]
    {
        crate::userspace_boot::enter::<UserspaceBootArch>(boot_info)
    }
    #[cfg(not(feature = "userspace_boot"))]
    kernel_main::init(boot_info)
}

#[cfg(feature = "userspace_boot")]
struct UserspaceBootArch;

#[cfg(feature = "userspace_boot")]
impl crate::userspace_boot::UserspaceBootArch for UserspaceBootArch {
    const NAME: &'static str = "x86_64";

    fn early_console_init() {
        serial::early_init();
    }

    fn idle_once() {
        unsafe {
            core::arch::asm!("hlt", options(nostack, nomem));
        }
    }
}

#[cfg(feature = "boot_minimal")]
pub fn init(boot_info: &'static crate::init::boot_info::BootInfo) -> ! {
    crate::boot_minimal::enter::<MinimalArch>(boot_info)
}

#[cfg(feature = "boot_minimal")]
struct MinimalArch;

#[cfg(feature = "boot_minimal")]
impl crate::boot_minimal::MinimalBootArch for MinimalArch {
    const NAME: &'static str = "x86_64";

    fn early_console_init() {
        serial::early_init();
    }

    fn idle_once() {
        unsafe {
            core::arch::asm!("hlt", options(nostack, nomem));
        }
    }
}
