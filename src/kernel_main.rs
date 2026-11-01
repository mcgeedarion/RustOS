//! Architecture-independent kernel entry-point dispatcher.

use crate::init::boot_info::BootInfo;

#[no_mangle]
#[cfg(target_arch = "x86_64")]
pub extern "sysv64" fn kernel_main(boot_info: &'static BootInfo) -> ! {
    enter(boot_info)
}

#[no_mangle]
#[cfg(not(target_arch = "x86_64"))]
pub extern "C" fn kernel_main(boot_info: &'static BootInfo) -> ! {
    enter(boot_info)
}

fn enter(boot_info: &'static BootInfo) -> ! {
    // ── Milestone 1: kernel entry ────────────────────────────────────────────
    // This is the very first Rust instruction after the arch stub hands off
    // to common code.  The counter is read before any subsystem init so that
    // the raw "time-to-kernel-main" value is captured.
    #[cfg(all(feature = "userspace_boot", target_arch = "aarch64"))]
    crate::serial_println!("BOOT_MARK label=BOOT_ENTRY ticks=0");
    #[cfg(not(all(feature = "userspace_boot", target_arch = "aarch64")))]
    crate::boot_mark!("BOOT_ENTRY");

    #[cfg(all(feature = "userspace_boot", target_arch = "aarch64"))]
    crate::serial_println!("RustOS: boot target [SECONDARY] - entering common kernel_main");
    #[cfg(not(all(feature = "userspace_boot", target_arch = "aarch64")))]
    crate::serial_println!(
        "RustOS: boot target [{}] - entering common kernel_main",
        BootInfo::priority().as_str(),
    );

    #[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
    {
        crate::kernel::architecture::log_kernel_architecture();
        debug_assert!(crate::kernel::architecture::is_hybrid_kernel());
    }
    crate::arch::init(boot_info)
}
