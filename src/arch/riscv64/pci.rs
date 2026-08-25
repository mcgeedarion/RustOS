//! RISC-V 64 PCI/PCIe support.
//!
//! This module provides ECAM (Enhanced Configuration Access Mechanism) access
//! for PCIe configuration space on RISC-V systems.

use core::ptr;

/// ECAM base address (set during init)
static mut ECAM_BASE: usize = 0;

/// Initialize ECAM with the given base address.
pub fn ecam_init(base: usize) {
    unsafe {
        ECAM_BASE = base;
    }
}

/// Get the current ECAM base address.
pub fn ecam_base() -> usize {
    unsafe { ECAM_BASE }
}

/// PCI configuration space access via ECAM.
/// 
/// ECAM formula:
///   cfg_addr = ECAM_BASE + (bus << 20) | (device << 15) | (function << 12) | register
#[inline]
fn cfg_addr(bus: u32, device: u32, function: u32, register: u32) -> usize {
    let ecam = unsafe { ECAM_BASE };
    ecam + ((bus as usize) << 20)
        | ((device as usize) << 15)
        | ((function as usize) << 12)
        | (register as usize & 0xFC)
}

/// Read a 32-bit value from PCI configuration space.
pub fn config_read(bus: u32, device: u32, function: u32, register: u32) -> u32 {
    if unsafe { ECAM_BASE } == 0 {
        return 0xFFFF_FFFF; // Indicate no ECAM configured
    }
    let addr = cfg_addr(bus, device, function, register);
    unsafe { ptr::read_volatile(addr as *const u32) }
}

/// Write a 32-bit value to PCI configuration space.
pub fn config_write(bus: u32, device: u32, function: u32, register: u32, value: u32) {
    if unsafe { ECAM_BASE } == 0 {
        return;
    }
    let addr = cfg_addr(bus, device, function, register);
    unsafe { ptr::write_volatile(addr as *mut u32, value) }
}

/// Read a BAR (Base Address Register) and return its size/type.
pub fn read_bar(bus: u32, device: u32, function: u32, bar_index: u32) -> Option<u64> {
    let bar_reg = 0x10 + (bar_index * 4);
    let bar_val = config_read(bus, device, function, bar_reg);
    
    if bar_val == 0 || bar_val == 0xFFFF_FFFF {
        return None;
    }
    
    // Check if memory-mapped or I/O
    if bar_val & 1 == 0 {
        // Memory space
        Some(bar_val as u64)
    } else {
        // I/O space (not typically used on RISC-V)
        Some((bar_val & !3) as u64)
    }
}

/// Enable a PCI device (memory and I/O decoding, bus mastering).
pub fn enable_device(bus: u32, device: u32, function: u32) {
    // Read command register
    let cmd = config_read(bus, device, function, 0x04);
    // Set memory space enable (bit 1) and bus master enable (bit 2)
    config_write(bus, device, function, 0x04, cmd | 0x06);
}

/// Find a PCI device by class code.
pub fn find_device_by_class(class: u8) -> Option<(u32, u32, u32)> {
    for bus in 0..256 {
        for device in 0..32 {
            for function in 0..8 {
                let vendor_id = config_read(bus, device, function, 0x00);
                if vendor_id == 0xFFFF_FFFF {
                    continue; // No device at this location
                }
                
                let class_reg = config_read(bus, device, function, 0x08);
                let class_code = (class_reg >> 24) as u8;
                
                if class_code == class {
                    return Some((bus, device, function));
                }
            }
        }
    }
    None
}

/// PCI class codes.
pub const PCI_CLASS_STORAGE_AHCI: u8 = 0x01;
pub const PCI_CLASS_STORAGE_NVME: u8 = 0x01; // Subclass 0x08 for NVMe
pub const PCI_CLASS_NETWORK: u8 = 0x02;
pub const PCI_CLASS_DISPLAY: u8 = 0x03;
