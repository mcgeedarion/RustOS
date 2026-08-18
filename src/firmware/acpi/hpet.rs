//! ACPI HPET (High Precision Event Timer) Table Parser.
//!
//! ## Overview
//!
//! The HPET is a hardware timer that provides high-resolution timing capabilities.
//! The ACPI HPET table (`HPET`) describes the memory-mapped base address and
//! capabilities of the HPET hardware.
//!
//! ## Structure
//!
//! The HPET table contains:
//! - Event Timer Block ID (32-bit identifier)
//! - Base Address (64-bit memory-mapped I/O address)
//! - HPET Number (system timer number)
//! - Minimum Tick (optional, in femtoseconds)
//! - Page Protection and OEM attributes
//!
//! ## Usage
//!
//! Call `init()` during ACPI initialization to parse the HPET table and cache
//! the base address for use by the timer driver.
//!
//! ## Thread Safety
//!
//! Initialization must complete before SMP is enabled. After initialization,
//! read-only access is safe from multiple threads.

use crate::println;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

/// HPET table signature
const HPET_SIGNATURE: &[u8; 4] = b"HPET";

/// Cached HPET base address
static HPET_BASE: AtomicU64 = AtomicU64::new(0);
/// Whether HPET has been initialized
static HPET_INITIALIZED: AtomicBool = AtomicBool::new(false);
/// HPET timer number (usually 0)
static HPET_NUMBER: AtomicU8 = AtomicU8::new(0);
/// Minimum tick value in femtoseconds (if available)
static HPET_MIN_TICK: AtomicU64 = AtomicU64::new(0);

/// HPET table structure (ACPI 6.5 §5.2.13)
#[repr(C, packed)]
struct HpetTable {
    /// Standard ACPI table header
    header: super::SdtHeader,
    /// Event Timer Block ID (32-bit)
    event_timer_block_id: u32,
    /// Base Address Space ID (must be 0 for System Memory)
    base_address_space_id: u8,
    /// Register Bit Width (must be 64)
    register_bit_width: u8,
    /// Register Bit Offset (must be 0)
    register_bit_offset: u8,
    /// Reserved
    _reserved: u8,
    /// Base Address (64-bit physical address)
    base_address: u64,
    /// HPET Number (system timer number, usually 0)
    hpet_number: u8,
    /// Minimum Clock Tick (in femtoseconds, optional)
    minimum_clock_tick: u16,
    /// Page Protection And OEM Attribute (optional)
    page_protection_and_oem_attribute: u8,
}

/// Initialize HPET subsystem by parsing the HPET ACPI table.
///
/// # Safety
/// - Must be called after `super::init()` has initialized ACPI root tables
/// - Must complete before SMP initialization
///
/// # Errors
/// Returns silently with a log message if:
/// - HPET table is not found
/// - HPET table is malformed or too short
/// - Base address is invalid
pub unsafe fn init() {
    let hpet_ptr = match super::find_table(HPET_SIGNATURE) {
        Some(p) => p,
        None => {
            println!("acpi/hpet: HPET table not found");
            return;
        }
    };

    // Validate table length
    let hpet_len = (*hpet_ptr).len as usize;
    let min_len = core::mem::size_of::<HpetTable>();
    
    if hpet_len < min_len {
        println!(
            "acpi/hpet: HPET table too short ({} < {})",
            hpet_len, min_len
        );
        return;
    }

    let hpet = &*(hpet_ptr as *const HpetTable);

    // Validate base address space ID (0 = System Memory)
    if hpet.base_address_space_id != 0 {
        println!(
            "acpi/hpet: invalid address space ID {}",
            hpet.base_address_space_id
        );
        return;
    }

    // Validate register bit width (should be 64)
    if hpet.register_bit_width != 64 {
        println!(
            "acpi/hpet: unexpected register bit width {}",
            hpet.register_bit_width
        );
        // Continue anyway, some firmware may have non-standard values
    }

    // Validate base address
    let base_addr = hpet.base_address;
    if base_addr == 0 {
        println!("acpi/hpet: invalid base address (null)");
        return;
    }

    // Validate physical address
    if !super::is_valid_physical_address(base_addr as usize, 1024) {
        println!(
            "acpi/hpet: base address {:#x} is outside valid range",
            base_addr
        );
        return;
    }

    // Cache the values with Release ordering
    HPET_BASE.store(base_addr, Ordering::Release);
    HPET_NUMBER.store(hpet.hpet_number, Ordering::Release);
    
    // Store minimum tick if non-zero
    if hpet.minimum_clock_tick != 0 {
        HPET_MIN_TICK.store(hpet.minimum_clock_tick as u64, Ordering::Release);
    }
    
    HPET_INITIALIZED.store(true, Ordering::Release);

    println!(
        "acpi/hpet: initialized @ {:#x} (timer {}, min_tick={} fs)",
        base_addr,
        hpet.hpet_number,
        hpet.minimum_clock_tick
    );
}

/// Get the HPET base address.
///
/// # Returns
/// `Some(address)` if HPET was successfully initialized, `None` otherwise.
///
/// # Thread Safety
/// Safe to call from multiple threads after `init()` has completed.
pub fn get_base_address() -> Option<u64> {
    if HPET_INITIALIZED.load(Ordering::Acquire) {
        Some(HPET_BASE.load(Ordering::Acquire))
    } else {
        None
    }
}

/// Get the HPET timer number.
///
/// # Returns
/// The timer number (usually 0), or `None` if not initialized.
pub fn get_timer_number() -> Option<u8> {
    if HPET_INITIALIZED.load(Ordering::Acquire) {
        Some(HPET_NUMBER.load(Ordering::Acquire))
    } else {
        None
    }
}

/// Get the minimum clock tick value in femtoseconds.
///
/// # Returns
/// `Some(tick_value)` if available and HPET is initialized, `None` otherwise.
pub fn get_minimum_tick() -> Option<u64> {
    if HPET_INITIALIZED.load(Ordering::Acquire) {
        let tick = HPET_MIN_TICK.load(Ordering::Acquire);
        if tick != 0 {
            Some(tick)
        } else {
            None
        }
    } else {
        None
    }
}

/// Check if HPET has been initialized.
pub fn is_initialized() -> bool {
    HPET_INITIALIZED.load(Ordering::Relaxed)
}

/// HPET capabilities register offsets (for reference)
pub mod offsets {
    /// General Capabilities and ID Register
    pub const CAPABILITIES_ID: u64 = 0x000;
    /// General Configuration Register
    pub const GENERAL_CONFIG: u64 = 0x010;
    /// General Interrupt Status Register
    pub const GENERAL_STATUS: u64 = 0x020;
    /// Main Counter Value Register
    pub const MAIN_COUNTER: u64 = 0xF0;
    /// Timer N Configuration and Capability Register (base offset)
    pub const TIMER_N_CONFIG: u64 = 0x100;
    /// Timer N Comparator Value Register (base offset)
    pub const TIMER_N_COMPARATOR: u64 = 0x108;
    /// Timer N FSB Interrupt Route Register (base offset, optional)
    pub const TIMER_N_FSB: u64 = 0x110;
}

/// HPET capability bits (from CAPABILITIES_ID register)
pub mod capabilities {
    /// Counter clock period mask (bits 31:16)
    pub const COUNTER_CLK_PERIOD_MASK: u64 = 0xFFFF_0000;
    /// Counter clock period shift
    pub const COUNTER_CLK_PERIOD_SHIFT: u64 = 16;
    /// Number of timers comparators mask (bits 15:8)
    pub const NUM_TIMERS_MASK: u64 = 0x0000_FF00;
    /// Number of timers shift
    pub const NUM_TIMERS_SHIFT: u64 = 8;
    /// 64-bit counter support (bit 0)
    pub const COUNTER_SIZE: u64 = 1 << 0;
    /// Legacy replacement capable (bit 1)
    pub const LEGACY_REPLACEMENT: u64 = 1 << 1;
    /// PCI vendor ID present (bit 15)
    pub const VENDOR_ID_PRESENT: u64 = 1 << 15;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hpet_offsets() {
        assert_eq!(offsets::CAPABILITIES_ID, 0x000);
        assert_eq!(offsets::GENERAL_CONFIG, 0x010);
        assert_eq!(offsets::GENERAL_STATUS, 0x020);
        assert_eq!(offsets::MAIN_COUNTER, 0xF0);
        assert_eq!(offsets::TIMER_N_CONFIG, 0x100);
        assert_eq!(offsets::TIMER_N_COMPARATOR, 0x108);
    }

    #[test]
    fn test_hpet_capability_bits() {
        assert_eq!(capabilities::COUNTER_SIZE, 1);
        assert_eq!(capabilities::LEGACY_REPLACEMENT, 2);
        assert_eq!(capabilities::VENDOR_ID_PRESENT, 1 << 15);
        
        // Test mask and shift
        assert_eq!(capabilities::NUM_TIMERS_MASK, 0xFF00);
        assert_eq!(capabilities::NUM_TIMERS_SHIFT, 8);
    }

    #[test]
    fn test_atomic_initialization() {
        // Verify initial state
        assert!(!HPET_INITIALIZED.load(Ordering::Relaxed));
        assert_eq!(HPET_BASE.load(Ordering::Relaxed), 0);
        assert_eq!(HPET_NUMBER.load(Ordering::Relaxed), 0);
        assert_eq!(HPET_MIN_TICK.load(Ordering::Relaxed), 0);
    }
}
