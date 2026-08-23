//! RISC-V 64 serial console support.
//! 
//! This implements a 16550-compatible UART driver for QEMU virt machine.
//! The UART is mapped at physical address 0x10000000 on QEMU virt.

use core::ptr;

/// Base address of the 16550 UART on QEMU virt machine
const UART_BASE: usize = 0x10000000;

/// UART register offsets
const UART_RBR: usize = 0x00; // Receiver Buffer Register (read)
const UART_THR: usize = 0x00; // Transmitter Holding Register (write)
const UART_IER: usize = 0x01; // Interrupt Enable Register
const UART_FCR: usize = 0x02; // FIFO Control Register (write)
const UART_ISR: usize = 0x02; // Interrupt Status Register (read)
const UART_LCR: usize = 0x03; // Line Control Register
const UART_MCR: usize = 0x04; // Modem Control Register
const UART_LSR: usize = 0x05; // Line Status Register

/// Line Status Register bits
const LSR_THRE: u8 = 1 << 5; // Transmit Holding Register Empty

/// Initialize the serial console for RISC-V.
/// Configures the UART for 115200 8N1 operation.
pub fn init() {
    unsafe {
        // Disable interrupts
        ptr::write_volatile((UART_BASE + UART_IER) as *mut u8, 0x00);
        
        // Enable DLAB (Divisor Latch Access Bit) to set baud rate
        ptr::write_volatile((UART_BASE + UART_LCR) as *mut u8, 0x80);
        
        // Set divisor for 115200 baud (assuming 1.8432 MHz clock)
        // divisor = 1843200 / (16 * 115200) = 1
        ptr::write_volatile((UART_BASE + UART_RBR) as *mut u8, 0x01); // DLL
        ptr::write_volatile((UART_BASE + UART_IER) as *mut u8, 0x00); // DLM
        
        // 8 bits, no parity, one stop bit
        ptr::write_volatile((UART_BASE + UART_LCR) as *mut u8, 0x03);
        
        // Enable and clear FIFO
        ptr::write_volatile((UART_BASE + UART_FCR) as *mut u8, 0x07);
        
        // Ready to transmit
    }
}

/// Write a byte to the serial console.
/// Busy-waits until the UART transmitter is ready.
pub fn write_byte(byte: u8) {
    unsafe {
        // Wait until Transmit Holding Register is empty
        while (ptr::read_volatile((UART_BASE + UART_LSR) as *const u8) & LSR_THRE) == 0 {
            core::hint::spin_loop();
        }
        
        // Write the byte
        ptr::write_volatile((UART_BASE + UART_THR) as *mut u8, byte);
    }
}
