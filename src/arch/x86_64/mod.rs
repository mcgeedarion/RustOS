#[cfg(not(feature = "boot_minimal"))]
pub mod apic;
#[cfg(not(feature = "boot_minimal"))]
pub mod cpu;
#[cfg(not(feature = "boot_minimal"))]
pub mod gdt;
#[cfg(not(feature = "boot_minimal"))]
pub mod hal;
#[cfg(not(feature = "boot_minimal"))]
pub mod idt;
#[cfg(not(feature = "boot_minimal"))]
pub mod interrupts;
#[cfg(not(feature = "boot_minimal"))]
pub mod kernel_main;
pub mod mem_layout;
#[cfg(not(feature = "boot_minimal"))]
pub mod memory;
#[cfg(not(feature = "boot_minimal"))]
pub mod paging;
#[cfg(not(feature = "boot_minimal"))]
pub mod pci;
#[cfg(not(feature = "boot_minimal"))]
pub mod power;
pub mod serial;
#[cfg(not(feature = "boot_minimal"))]
pub mod syscall;
#[cfg(target_os = "uefi")]
pub mod uefi_boot_stack;
#[cfg(any(feature = "uefi_boot", feature = "boot_minimal"))]
pub mod uefi_entry;
#[cfg(not(feature = "boot_minimal"))]
pub mod uentry;
#[cfg(not(feature = "boot_minimal"))]
pub mod xsave;

/// x86_64 early/kernel boot hook used by the common entry point.
#[cfg(not(feature = "boot_minimal"))]
pub fn init(boot_info: &'static crate::init::boot_info::BootInfo) -> ! {
    kernel_main::init(boot_info)
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
