//! Boot performance markers.
//!
//! Provides the [`boot_mark!`] macro which snapshots the architecture's
//! free-running hardware counter and emits a single, parseable line on the
//! serial console:
//!
//! ```text
//! BOOT_MARK label=BOOT_ENTRY ticks=12345678
//! ```
//!
//! ## Counter sources
//!
//! | Architecture | Instruction  | Register / CSR              |
//! |--------------|--------------|-----------------------------|
//! | x86\_64      | `rdtsc`      | TSC (64-bit, IA32\_TSC)     |
//! | AArch64      | `mrs`        | `cntvct_el0` (virtual timer)|
//!
//! Both counters are monotonic, non-wrapping in practice for the boot
//! window, and readable from kernel EL/privilege level without enabling
//! any extra CPU feature flags.
//!
//! ## Wire format
//!
//! ```text
//! BOOT_MARK label=<LABEL> ticks=<DECIMAL_U64>
//! ```
//!
//! A line is terminated by `\n` (the serial_println! macro appends it).
//! Fields are separated by a single space.  The prefix `BOOT_MARK` is
//! fixed and may be used as an anchor by grep / awk / Python.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::boot_mark;
//!
//! boot_mark!("BOOT_ENTRY");
//! // ... set up MMU ...
//! boot_mark!("BOOT_MMU_ON");
//! ```

/// Read the architecture's free-running hardware timer counter.
///
/// Returns a raw tick count.  The frequency varies by architecture and
/// platform; use the delta between two marks to derive elapsed time.
#[inline(always)]
pub fn read_hw_counter() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        let lo: u32;
        let hi: u32;
        // SAFETY: `rdtsc` is available on all x86_64 CPUs and is safe to
        // execute in kernel mode without any prerequisite CR4/MSR setup.
        unsafe {
            core::arch::asm!(
                "rdtsc",
                out("eax") lo,
                out("edx") hi,
                options(nomem, nostack, preserves_flags),
            );
        }
        ((hi as u64) << 32) | (lo as u64)
    }

    #[cfg(target_arch = "aarch64")]
    {
        let tick: u64;
        // SAFETY: `cntvct_el0` is the virtual counter register accessible
        // from EL1 (kernel) without any additional enable bits on ARMv8-A.
        unsafe {
            core::arch::asm!(
                "mrs {}, cntvct_el0",
                out(reg) tick,
                options(nomem, nostack, preserves_flags),
            );
        }
        tick
    }


    // Fallback for host-side unit-test builds (std target).
}

/// Emit a timestamped boot milestone marker on the serial console.
///
/// # Format
///
/// ```text
/// BOOT_MARK label=BOOT_ENTRY ticks=12345678
/// ```
///
/// # Example
///
/// ```rust,ignore
/// boot_mark!("BOOT_ENTRY");
/// ```
#[macro_export]
macro_rules! boot_mark {
    ($label:expr) => {{
        let _ticks = $crate::boot_perf::read_hw_counter();
        $crate::serial_println!("BOOT_MARK label={} ticks={}", $label, _ticks);
    }};
}
