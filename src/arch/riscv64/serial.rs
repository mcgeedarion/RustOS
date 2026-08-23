//! RISC-V 64 serial console support.

/// Initialize the serial console for RISC-V.
/// For now, this is a stub - in a real implementation you would
/// configure the UART (typically 16550 or SiFive UART).
pub fn init() {
    // Stub: In a real implementation, configure the UART here.
    // For QEMU virt machine, the UART is typically at 0x10000000.
}

/// Write a byte to the serial console.
/// This is a stub - implement actual UART write logic here.
pub fn write_byte(_byte: u8) {
    // Stub: In a real implementation, write to the UART data register.
    // For 16550-compatible UART:
    // unsafe { core::ptr::write_volatile(0x10000000 as *mut u8, byte); }
}
