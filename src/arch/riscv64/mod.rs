//! RISC-V 64-bit architecture support.

#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod boot;
#[cfg(any(not(feature = "boot_minimal"), feature = "userspace_boot"))]
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

/// RISC-V 64 early/kernel boot hook used by the common entry point.
#[cfg(any(not(feature = "boot_minimal"), feature = "userspace_boot"))]
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
    const NAME: &'static str = "riscv64";

    fn early_console_init() {
        serial::init();
    }

    fn idle_once() {
        unsafe {
            core::arch::asm!("wfi", options(nostack, nomem));
        }
    }
}

#[cfg(all(feature = "boot_minimal", not(feature = "userspace_boot")))]
pub fn init(boot_info: &'static crate::init::boot_info::BootInfo) -> ! {
    crate::boot_minimal::enter::<MinimalArch>(boot_info)
}

#[cfg(all(feature = "boot_minimal", not(feature = "userspace_boot")))]
struct MinimalArch;

#[cfg(all(feature = "boot_minimal", not(feature = "userspace_boot")))]
impl crate::boot_minimal::MinimalBootArch for MinimalArch {
    const NAME: &'static str = "riscv64";

    fn early_console_init() {
        serial::init();
    }

    fn idle_once() {
        unsafe {
            core::arch::asm!("wfi", options(nostack, nomem));
        }
    }
}
