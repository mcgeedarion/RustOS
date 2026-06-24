//! Architecture-independent kernel entry-point dispatcher.

use crate::init::boot_info::BootInfo;

#[no_mangle]
pub extern "C" fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // ── Milestone 1: kernel entry ────────────────────────────────────────────
    // This is the very first Rust instruction after the arch stub hands off
    // to common code.  The counter is read before any subsystem init so that
    // the raw "time-to-kernel-main" value is captured.
    crate::boot_mark!("BOOT_ENTRY");

    crate::serial_println!(
        "RustOS: boot target [{}] — entering common kernel_main",
        BootInfo::priority().as_str(),
    );

    #[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
    {
        crate::kernel::architecture::log_kernel_architecture();
        debug_assert!(crate::kernel::architecture::is_hybrid_kernel());
    }
    crate::arch::init(boot_info)
}
