//! RISC-V 64-bit architecture support.

pub mod hal;
pub mod paging;
pub mod serial;
pub mod uefi_entry;

pub use hal::ArchImpl;

#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub use hal::init;

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
