//! IRQ / interrupt-controller subsystem.
//!
//! Organised by architecture so a future x86_64 APIC driver can live here
//! alongside other controllers without polluting `src/drivers/`.
//!
//! ## Modules
//!   x86_64  — APIC (external IRQs) + APIC timer
//!   aarch64 — GIC (external IRQs) + timer

#[cfg(target_arch = "aarch64")]
pub mod aarch64;
#[cfg(target_arch = "x86_64")]
pub mod x86_64;
