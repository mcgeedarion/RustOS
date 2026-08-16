//! Kernel-wide cfg aliases for cleaner conditional compilation.
//!
//! This module centralizes architecture and feature detection to avoid
//! repetitive `#[cfg(...)]` attributes throughout the codebase.
//!
//! # Usage
//!
//! ```rust,no_run
//! use crate::core::cfg_aliases::*;
//!
//! #[cfg(ARCH_X86_64)]
//! fn x86_specific_code() { }
//!
//! #[cfg(ARCH_AARCH64)]
//! fn arm_specific_code() { }
//!
//! #[cfg(KERNEL_FULL)]
//! fn full_kernel_feature() { }
//! ```

// ============================================================================
// Architecture Aliases
// ============================================================================

/// x86-64 architecture (AMD64, Intel 64)
#[cfg(target_arch = "x86_64")]
pub const ARCH_X86_64: bool = true;
#[cfg(not(target_arch = "x86_64"))]
pub const ARCH_X86_64: bool = false;

/// AArch64 architecture (ARM 64-bit)
#[cfg(target_arch = "aarch64")]
pub const ARCH_AARCH64: bool = true;
#[cfg(not(target_arch = "aarch64"))]
pub const ARCH_AARCH64: bool = false;

/// RISC-V 64-bit architecture
#[cfg(target_arch = "riscv64")]
pub const ARCH_RISCV64: bool = true;
#[cfg(not(target_arch = "riscv64"))]
pub const ARCH_RISCV64: bool = false;

// ============================================================================
// Build Profile Aliases
// ============================================================================

/// Full kernel build (default) - includes all features
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub const KERNEL_FULL: bool = true;
#[cfg(any(feature = "boot_minimal", feature = "userspace_boot"))]
pub const KERNEL_FULL: bool = false;

/// Minimal boot build - early boot only, no userspace
#[cfg(feature = "boot_minimal")]
pub const BOOT_MINIMAL: bool = true;
#[cfg(not(feature = "boot_minimal"))]
pub const BOOT_MINIMAL: bool = false;

/// Userspace boot milestone - boots to PID 1
#[cfg(feature = "userspace_boot")]
pub const USERSPACE_BOOT: bool = true;
#[cfg(not(feature = "userspace_boot"))]
pub const USERSPACE_BOOT: bool = false;

// ============================================================================
// Debug/Feature Aliases
// ============================================================================

/// Debug build (not release)
#[cfg(debug_assertions)]
pub const DEBUG_BUILD: bool = true;
#[cfg(not(debug_assertions))]
pub const DEBUG_BUILD: bool = false;

/// Kernel debugging features enabled
#[cfg(feature = "debug_kernel")]
pub const DEBUG_KERNEL: bool = true;
#[cfg(not(feature = "debug_kernel"))]
pub const DEBUG_KERNEL: bool = false;

/// GDB stub for kernel debugging
#[cfg(feature = "gdbstub")]
pub const GDBSTUB_ENABLED: bool = true;
#[cfg(not(feature = "gdbstub"))]
pub const GDBSTUB_ENABLED: bool = false;

/// KASAN (Kernel Address Sanitizer) enabled
#[cfg(feature = "kasan")]
pub const KASAN_ENABLED: bool = true;
#[cfg(not(feature = "kasan"))]
pub const KASAN_ENABLED: bool = false;

/// Fault injection for testing
#[cfg(feature = "fault_injection")]
pub const FAULT_INJECTION: bool = true;
#[cfg(not(feature = "fault_injection"))]
pub const FAULT_INJECTION: bool = false;

/// Kernel module testing
#[cfg(feature = "kmtest")]
pub const KMTEST: bool = true;
#[cfg(not(feature = "kmtest"))]
pub const KMTEST: bool = false;

// ============================================================================
// Hardware Feature Aliases
// ============================================================================

/// SMP (Symmetric Multi-Processing) support
#[cfg(feature = "smp")]
pub const SMP_ENABLED: bool = true;
#[cfg(not(feature = "smp"))]
pub const SMP_ENABLED: bool = false;

/// ACPI support (x86 standard)
#[cfg(all(target_arch = "x86_64", not(feature = "device_tree")))]
pub const ACPI_ENABLED: bool = true;
#[cfg(not(all(target_arch = "x86_64", not(feature = "device_tree"))))]
pub const ACPI_ENABLED: bool = false;

/// Device Tree support (common in ARM/embedded)
#[cfg(feature = "device_tree")]
pub const DEVICE_TREE: bool = true;
#[cfg(not(feature = "device_tree"))]
pub const DEVICE_TREE: bool = false;

/// UEFI boot support
#[cfg(feature = "uefi")]
pub const UEFI_BOOT: bool = true;
#[cfg(not(feature = "uefi"))]
pub const UEFI_BOOT: bool = false;

// ============================================================================
// Convenience Macros
// ============================================================================

/// Execute code only on x86-64
#[macro_export]
macro_rules! if_x86_64 {
    ($($tt:tt)*) => {
        #[cfg(target_arch = "x86_64")]
        $($tt)*
    };
}

/// Execute code only on AArch64
#[macro_export]
macro_rules! if_aarch64 {
    ($($tt:tt)*) => {
        #[cfg(target_arch = "aarch64")]
        $($tt)*
    };
}

/// Execute code only in full kernel builds
#[macro_export]
macro_rules! if_full_kernel {
    ($($tt:tt)*) => {
        #[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
        $($tt)*
    };
}

/// Execute code only in debug builds
#[macro_export]
macro_rules! if_debug {
    ($($tt:tt)*) => {
        #[cfg(debug_assertions)]
        $($tt)*
    };
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get the current architecture name as a string
pub const fn arch_name() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    return "x86_64";

    #[cfg(target_arch = "aarch64")]
    return "aarch64";

    #[cfg(target_arch = "riscv64")]
    return "riscv64";

    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    return "unknown";
}

/// Get the current build profile as a string
pub const fn build_profile() -> &'static str {
    #[cfg(feature = "boot_minimal")]
    return "boot_minimal";

    #[cfg(feature = "userspace_boot")]
    return "userspace_boot";

    #[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
    return "full";
}

/// Check if running in a debug build
pub const fn is_debug() -> bool {
    DEBUG_BUILD
}

// ============================================================================
// Compile-time Assertions
// ============================================================================

// Ensure only one build profile is active at a time
#[cfg(all(feature = "boot_minimal", feature = "userspace_boot"))]
compile_error!("Cannot enable both boot_minimal and userspace_boot features");

// Ensure supported architectures only
#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
)))]
compile_error!("Unsupported architecture. Supported: x86_64, aarch64, riscv64");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arch_detection() {
        // Exactly one arch should be true
        let arch_count = [ARCH_X86_64, ARCH_AARCH64, ARCH_RISCV64]
            .iter()
            .filter(|&&x| x)
            .count();
        assert_eq!(arch_count, 1, "Exactly one architecture should be detected");
    }

    #[test]
    fn test_build_profile() {
        // At most one build profile should be active
        let profile_count = [BOOT_MINIMAL, USERSPACE_BOOT, KERNEL_FULL]
            .iter()
            .filter(|&&x| x)
            .count();
        assert_eq!(profile_count, 1, "Exactly one build profile should be active");
    }

    #[test]
    fn test_arch_name() {
        let name = arch_name();
        assert!(!name.is_empty(), "Architecture name should not be empty");
    }
}
