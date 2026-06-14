//! AArch64/ARM64 architecture support.
//!
//! Baseline hardware requirement follows the ReactOS ARM64 bring-up target:
//! UEFI firmware on an Armv8-A (or newer) processor with either a GICv2 or
//! GICv3 interrupt controller.

#[cfg(not(feature = "boot_minimal"))]
pub mod boot;
#[cfg(not(feature = "boot_minimal"))]
pub mod cpu;
#[cfg(not(feature = "boot_minimal"))]
pub mod hal;
#[cfg(not(feature = "boot_minimal"))]
pub mod interrupts;
#[cfg(not(feature = "boot_minimal"))]
pub mod kernel_main;
pub mod mem_layout;
#[cfg(not(feature = "boot_minimal"))]
pub mod paging;
#[cfg(not(feature = "boot_minimal"))]
pub mod pci;
pub mod serial;
#[cfg(not(feature = "boot_minimal"))]
pub mod simd;
#[cfg(not(feature = "boot_minimal"))]
pub mod smp;
#[cfg(not(feature = "boot_minimal"))]
pub mod syscall;
#[cfg(any(feature = "uefi_boot", feature = "boot_minimal"))]
pub mod uefi_entry;
#[cfg(not(feature = "boot_minimal"))]
pub mod uentry;

/// ARM64 early/kernel boot hook used by the common entry point.
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
    const NAME: &'static str = "aarch64";

    fn early_console_init() {
        unsafe {
            serial::init(13, 1);
        }
    }

    fn idle_once() {
        unsafe {
            core::arch::asm!("wfi", options(nostack, nomem));
        }
    }
}
