//! AArch64/ARM64 architecture support.
//!
//! Baseline hardware requirement follows the ReactOS ARM64 bring-up target:
//! UEFI firmware on an Armv8-A (or newer) processor with either a GICv2 or
//! GICv3 interrupt controller.

#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod boot;
#[cfg(not(feature = "boot_minimal"))]
pub mod cpu;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod hal;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod interrupts;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod kernel_main;
pub mod mem_layout;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod paging;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod pci;
pub mod serial;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod simd;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod smp;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod syscall;
#[cfg(any(
    feature = "uefi_boot",
    feature = "boot_minimal",
    feature = "userspace_boot"
))]
pub mod uefi_entry;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod uentry;

/// ARM64 early/kernel boot hook used by the common entry point.
#[cfg(not(feature = "boot_minimal"))]
pub fn init(boot_info: &'static crate::init::boot_info::BootInfo) -> ! {
    #[cfg(feature = "userspace_boot")]
    return crate::userspace_boot::enter::<UserspaceBootArch>(boot_info);
    #[cfg(not(feature = "userspace_boot"))]
    kernel_main::init(boot_info)
}

#[cfg(feature = "userspace_boot")]
struct UserspaceBootArch;

#[cfg(feature = "userspace_boot")]
impl crate::userspace_boot::UserspaceBootArch for UserspaceBootArch {
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
