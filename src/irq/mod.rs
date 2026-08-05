//! IRQ / interrupt-controller subsystem.
//!
//! Organised by architecture so a future x86_64 APIC driver can live here
//! alongside the aarch64 GIC without polluting `src/drivers/`.
//!
//! These are **not** drivers — they are fundamental platform machinery that
//! the trap handler depends on during the earliest stages of kernel init.

#[cfg(target_arch = "aarch64")]
pub mod aarch64;
