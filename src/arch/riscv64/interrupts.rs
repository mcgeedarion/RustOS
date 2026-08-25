//! RISC-V 64 PLIC (Platform-Level Interrupt Controller) driver.
//!
//! The PLIC is the standard interrupt controller for RISC-V systems, providing:
//! - Priority-based interrupt handling
//! - Per-hart context registers
//! - Claim/complete mechanism for interrupt acknowledgment

use core::ptr;

/// PLIC register offsets
const PLIC_PRIORITY: usize = 0x0000;       // Interrupt priority (per source)
const PLIC_PENDING: usize = 0x1000;        // Pending bits (per source)
const PLIC_ENABLE: usize = 0x2000;         // Enable bits (per hart)
const PLIC_THRESHOLD: usize = 0x200000;    // Priority threshold (per context)
const PLIC_CLAIM: usize = 0x200004;        // Claim/complete register (per context)

/// Maximum number of interrupt sources supported
const PLIC_MAX_SOURCES: usize = 1024;

/// PLIC context size per hart (S-mode context starts at offset 0x1000 per hart)
const PLIC_CONTEXT_SIZE: usize = 0x1000;

/// Base address of PLIC on QEMU virt machine
const PLIC_BASE: usize = 0x0C00_0000;

/// Initialize the PLIC interrupt controller (BSP only).
pub fn init() {
    unsafe {
        // Set priority threshold to 0 (accept all priorities)
        let threshold_reg = PLIC_BASE + PLIC_THRESHOLD;
        ptr::write_volatile(threshold_reg as *mut u32, 0);
        
        // Enable UART interrupt (IRQ 10 on QEMU virt)
        let enable_reg = PLIC_BASE + PLIC_ENABLE;
        let mut enable_val = ptr::read_volatile(enable_reg as *const u32);
        enable_val |= 1 << 10;
        ptr::write_volatile(enable_reg as *mut u32, enable_val);
        
        // Set UART priority to 1 (above threshold of 0)
        let uart_priority_reg = PLIC_BASE + PLIC_PRIORITY + (10 * 4);
        ptr::write_volatile(uart_priority_reg as *mut u32, 1);
        
        crate::serial_println!("riscv64: PLIC initialized");
    }
}

/// Per-CPU initialization for secondary CPUs.
pub fn init_percpu() {
    let hart_id = super::super::arch::riscv64::hal::cpu_id();
    let context_offset = ((hart_id + 1) as usize) * PLIC_CONTEXT_SIZE; // S-mode context
    
    unsafe {
        // Set threshold for this hart's context
        let threshold_reg = PLIC_BASE + PLIC_THRESHOLD + context_offset;
        ptr::write_volatile(threshold_reg as *mut u32, 0);
    }
}

/// Handle an external interrupt from the PLIC.
/// Returns the interrupt ID if one was claimed, or None if none pending.
pub fn handle_irq() -> Option<u32> {
    let hart_id = super::super::arch::riscv64::hal::cpu_id();
    let context_offset = ((hart_id + 1) as usize) * PLIC_CONTEXT_SIZE;
    
    unsafe {
        // Claim an interrupt
        let claim_reg = PLIC_BASE + PLIC_CLAIM + context_offset;
        let irq_id = ptr::read_volatile(claim_reg as *const u32);
        
        if irq_id == 0 {
            return None;
        }
        
        // Dispatch based on IRQ ID
        match irq_id {
            10 => {
                // UART interrupt - check if data available
                if let Some(_byte) = super::super::arch::riscv64::serial::getc() {
                    // Could buffer input here for later processing
                }
            },
            _ => {
                // Unknown IRQ - could log or ignore
            }
        }
        
        // Complete the interrupt by writing back the claimed ID
        ptr::write_volatile(claim_reg as *mut u32, irq_id);
        
        Some(irq_id)
    }
}

/// Enable a specific interrupt source.
pub fn enable_irq(irq: u32) {
    if irq >= PLIC_MAX_SOURCES as u32 {
        return;
    }
    unsafe {
        let enable_reg = PLIC_BASE + PLIC_ENABLE + ((irq / 32) * 4);
        let mut val = ptr::read_volatile(enable_reg as *const u32);
        val |= 1 << (irq % 32);
        ptr::write_volatile(enable_reg as *mut u32, val);
    }
}

/// Disable a specific interrupt source.
pub fn disable_irq(irq: u32) {
    if irq >= PLIC_MAX_SOURCES as u32 {
        return;
    }
    unsafe {
        let enable_reg = PLIC_BASE + PLIC_ENABLE + ((irq / 32) * 4);
        let mut val = ptr::read_volatile(enable_reg as *const u32);
        val &= !(1 << (irq % 32));
        ptr::write_volatile(enable_reg as *mut u32, val);
    }
}

/// Set priority for an interrupt source (0-7).
pub fn set_priority(irq: u32, priority: u32) {
    if irq >= PLIC_MAX_SOURCES as u32 || priority > 7 {
        return;
    }
    unsafe {
        let priority_reg = PLIC_BASE + PLIC_PRIORITY + (irq as usize * 4);
        ptr::write_volatile(priority_reg as *mut u32, priority & 0b111);
    }
}
