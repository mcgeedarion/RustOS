//! RISC-V 64 interrupt controller support (PLIC/APLIC).
//!
//! On RISC-V systems, external interrupts are typically handled by:
//! - PLIC (Platform-Level Interrupt Controller) - older systems
//! - APLIC (Advanced PLIC) - newer systems with IMSIC support
//!
//! For QEMU virt machines, we use the PLIC which provides:
//! - Priority threshold per hart
//! - Claim/complete mechanism for interrupt handling
//! - Support for multiple harts (SMP)

pub mod plic;
