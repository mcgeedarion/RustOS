//! Architecture module.

pub mod api;

pub mod time {
    pub fn current_unix_time_secs() -> u64 {
        #[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
        {
            let mono_ns = crate::time::read_monotonic_ns() as i64;
            let offset_ns = crate::time::realtime_offset_ns();
            return mono_ns.saturating_add(offset_ns).max(0) as u64 / crate::time::NSEC_PER_SEC;
        }
        #[cfg(any(feature = "boot_minimal", feature = "userspace_boot"))]
        {
            0
        }
    }
}

/// Minimal early-console facade used by panic paths before the full console
/// subsystem is available.
pub mod console {
    /// Write one byte to the active architecture's earliest available console.
    ///
    /// # Safety
    ///
    /// This is intended for panic/early-boot paths where normal locking and
    /// device discovery may not be available. Callers must tolerate polled I/O
    /// and architecture-specific best-effort output.
    pub unsafe fn early_putchar(byte: u8) {
        #[cfg(target_arch = "x86_64")]
        crate::arch::x86_64::serial::write_byte(byte);

        #[cfg(target_arch = "aarch64")]
        crate::arch::aarch64::serial::write_byte(byte);

        #[cfg(target_arch = "riscv64")]
        crate::arch::riscv64::serial::write_byte(byte);
    }
}

#[cfg(target_arch = "aarch64")]
pub mod aarch64;
#[cfg(all(
    target_arch = "aarch64",
    not(feature = "boot_minimal")
))]
pub use aarch64::hal;
#[cfg(target_arch = "aarch64")]
use aarch64::hal::ArchImpl;

#[cfg(target_arch = "x86_64")]
pub mod x86_64;
#[cfg(all(
    target_arch = "x86_64",
    not(feature = "boot_minimal")
))]
pub use x86_64::hal;
#[cfg(target_arch = "x86_64")]
use x86_64::hal::ArchImpl;

#[cfg(target_arch = "riscv64")]
pub mod riscv64;
#[cfg(all(
    target_arch = "riscv64",
    not(feature = "boot_minimal")
))]
pub use riscv64::hal;
#[cfg(target_arch = "riscv64")]
use riscv64::hal::ArchImpl;

/// The concrete architecture implementation.
/// Generic code uses this type alias to access all HAL traits.
#[cfg(not(feature = "boot_minimal"))]
pub type Arch = ArchImpl;

/// Run architecture-specific boot initialisation and hand off to the scheduler
/// or final idle loop. This is the only hook called by the common kernel entry.
pub fn init(boot_info: &'static crate::init::boot_info::BootInfo) -> ! {
    #[cfg(target_arch = "x86_64")]
    {
        x86_64::init(boot_info)
    }
    #[cfg(target_arch = "aarch64")]
    {
        aarch64::init(boot_info)
    }
    #[cfg(target_arch = "riscv64")]
    {
        riscv64::init(boot_info)
    }
}
