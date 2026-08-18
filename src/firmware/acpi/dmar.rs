//! ACPI DMAR (DMA Remapping) Table Parser.
//!
//! ## Overview
//!
//! The DMAR table describes the DMA remapping hardware (Intel VT-d, AMD IOMMU)
//! available in the system. This is critical for:
//! - I/O virtualization (assigning devices to VMs)
//! - I/O memory protection (preventing rogue DMA)
//! - Interrupt remapping
//!
//! ## Structure
//!
//! The DMAR table contains a header followed by multiple remapping structures:
//! - Hardware Unit Definition (DRHD)
//! - Reserved Memory Region Definition (RMRR)
//! - Root Entry Translation Table Pointer (ATSR)
//! - Hardware Unit Definition for SATA (SATC)
//!
//! ## Usage
//!
//! Call `init()` during ACPI initialization to discover DMA remapping units.
//! The parsed information can be used to initialize an IOMMU driver.
//!
//! ## Thread Safety
//!
//! Initialization must complete before SMP is enabled. After initialization,
//! read-only access is safe from multiple threads.

use crate::println;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// DMAR table signature
const DMAR_SIGNATURE: &[u8; 4] = b"DMAR";

/// Whether DMAR has been initialized
static DMAR_INITIALIZED: AtomicBool = AtomicBool::new(false);
/// Host address width (in bits, e.g., 39, 48)
static DMAR_HOST_ADDRESS_WIDTH: AtomicU8 = AtomicU8::new(0);
/// Flags from DMAR header
static DMAR_FLAGS: AtomicU8 = AtomicU8::new(0);

/// DMAR table structure (ACPI 6.5 §5.2.11)
#[repr(C, packed)]
struct DmarTable {
    /// Standard ACPI table header
    header: super::SdtHeader,
    /// Host Address Width (encoded as N-1, where N is actual width)
    host_address_width: u8,
    /// Flags (bit 0 = IVRS, bit 1 = X2APIC mode, etc.)
    flags: u8,
    /// Reserved (10 bytes)
    _reserved: [u8; 10],
    // Followed by variable-length remapping structures
}

/// DMAR structure types
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum DmarStructureType {
    /// DMA Remapping Hardware Unit Definition (DRHD)
    HardwareUnitDefinition = 0,
    /// Reserved Memory Region Definition (RMRR)
    ReservedMemoryRegion = 1,
    /// Root Entry Translation Table Pointer (ATSR)
    RootAtsCapability = 2,
    /// DMA Remapping Hardware Unit Definition for SATA (SATC)
    Sasatc = 3,
}

impl TryFrom<u16> for DmarStructureType {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(DmarStructureType::HardwareUnitDefinition),
            1 => Ok(DmarStructureType::ReservedMemoryRegion),
            2 => Ok(DmarStructureType::RootAtsCapability),
            3 => Ok(DmarStructureType::Sasatc),
            _ => Err(()),
        }
    }
}

/// Common header for all DMAR sub-structures
#[repr(C, packed)]
struct DmarStructureHeader {
    /// Structure type (see DmarStructureType)
    structure_type: u16,
    /// Length of the structure in bytes
    length: u16,
}

/// DRHD structure header (DMA Remapping Hardware Unit Definition)
#[repr(C, packed)]
struct DrhdHeader {
    /// Common header
    header: DmarStructureHeader,
    /// Flags (bit 0 = INCLUDE_PCI_ALL)
    flags: u8,
    /// Reserved
    _reserved: u8,
    /// PCI Segment Number
    pci_segment: u16,
    /// Register Base Address (physical address)
    register_base_addr: u64,
    // Followed by device scope entries
}

/// RMRR structure header (Reserved Memory Region Definition)
#[repr(C, packed)]
struct RmrrHeader {
    /// Common header
    header: DmarStructureHeader,
    /// Reserved (2 bytes)
    _reserved: [u8; 2],
    /// PCI Segment Number
    pci_segment: u16,
    /// Reserved Memory Region Base Address
    rmrr_base_addr: u64,
    /// Reserved Memory Region Limit Address
    rmrr_limit_addr: u64,
    // Followed by device scope entries
}

/// Device Scope Entry header
#[repr(C, packed)]
struct DeviceScopeEntry {
    /// Device Scope Type
    entry_type: u8,
    /// Length of this entry in bytes
    length: u8,
    /// Reserved (2 bytes)
    _reserved: [u8; 2],
    /// Enumeration ID (for PCI: bus, device, function)
    enumeration_id: [u8; 2],
    /// PCI Path (variable length)
    // pci_path: [u8; variable],
}

/// Initialize DMAR subsystem by parsing the DMAR ACPI table.
///
/// # Safety
/// - Must be called after `super::init()` has initialized ACPI root tables
/// - Must complete before SMP initialization
///
/// # Errors
/// Returns silently with a log message if:
/// - DMAR table is not found
/// - DMAR table is malformed or too short
/// - No valid remapping units are discovered
pub unsafe fn init() {
    let dmar_ptr = match super::find_table(DMAR_SIGNATURE) {
        Some(p) => p,
        None => {
            println!("acpi/dmar: DMAR table not found (IOMMU may not be available)");
            return;
        }
    };

    // Validate table length
    let dmar_len = (*dmar_ptr).len as usize;
    let min_len = core::mem::size_of::<DmarTable>();
    
    if dmar_len < min_len {
        println!(
            "acpi/dmar: DMAR table too short ({} < {})",
            dmar_len, min_len
        );
        return;
    }

    let dmar = &*(dmar_ptr as *const DmarTable);

    // Extract and validate host address width
    // Encoded as N-1, so add 1 to get actual width
    let host_addr_width = dmar.host_address_width.wrapping_add(1);
    
    if host_addr_width < 30 || host_addr_width > 64 {
        println!(
            "acpi/dmar: invalid host address width {}",
            host_addr_width
        );
        return;
    }

    // Cache the values with Release ordering
    DMAR_HOST_ADDRESS_WIDTH.store(host_addr_width, Ordering::Release);
    DMAR_FLAGS.store(dmar.flags, Ordering::Release);
    DMAR_INITIALIZED.store(true, Ordering::Release);

    println!(
        "acpi/dmar: initialized (host_addr_width={}, flags={:#x})",
        host_addr_width,
        dmar.flags
    );

    // Parse sub-structures
    parse_dmar_structures(dmar_ptr as *const u8, dmar_len);
}

/// Parse DMAR sub-structures (DRHD, RMRR, etc.)
///
/// # Safety
/// - `dmar_ptr` must point to a valid DMAR table
/// - `dmar_len` must be the correct table length
unsafe fn parse_dmar_structures(dmar_ptr: *const u8, dmar_len: usize) {
    let header_size = core::mem::size_of::<DmarTable>();
    let mut offset = header_size;
    let mut drhd_count = 0;
    let mut rmrr_count = 0;

    while offset + core::mem::size_of::<DmarStructureHeader>() <= dmar_len {
        let struct_hdr = &*((dmar_ptr.add(offset)) as *const DmarStructureHeader);
        
        if struct_hdr.length == 0 || offset + struct_hdr.length as usize > dmar_len {
            println!("acpi/dmar: invalid structure at offset {}", offset);
            break;
        }

        match DmarStructureType::try_from(struct_hdr.structure_type) {
            Ok(DmarStructureType::HardwareUnitDefinition) => {
                parse_drhd(dmar_ptr.add(offset), struct_hdr.length as usize);
                drhd_count += 1;
            }
            Ok(DmarStructureType::ReservedMemoryRegion) => {
                parse_rmrr(dmar_ptr.add(offset), struct_hdr.length as usize);
                rmrr_count += 1;
            }
            Ok(DmarStructureType::RootAtsCapability) => {
                println!("acpi/dmar: ATSR structure found (not fully parsed)");
            }
            Ok(DmarStructureType::Sasatc) => {
                println!("acpi/dmar: SATC structure found (not fully parsed)");
            }
            Err(_) => {
                println!(
                    "acpi/dmar: unknown structure type {:#x} at offset {}",
                    struct_hdr.structure_type,
                    offset
                );
            }
        }

        offset += struct_hdr.length as usize;
    }

    println!(
        "acpi/dmar: found {} DRHD unit(s), {} RMRR region(s)",
        drhd_count, rmrr_count
    );
}

/// Parse a DRHD (DMA Remapping Hardware Unit Definition) structure
///
/// # Safety
/// - `ptr` must point to a valid DRHD structure
/// - `length` must be the correct structure length
unsafe fn parse_drhd(ptr: *const u8, length: usize) {
    if length < core::mem::size_of::<DrhdHeader>() {
        println!("acpi/dmar: DRHD structure too short");
        return;
    }

    let drhd = &*(ptr as *const DrhdHeader);
    let include_pci_all = (drhd.flags & 0x01) != 0;

    println!(
        "acpi/dmar: DRHD @ {:#x}, segment {}, include_all={}",
        drhd.register_base_addr,
        drhd.pci_segment,
        include_pci_all
    );

    // Validate register base address
    if drhd.register_base_addr == 0 {
        println!("acpi/dmar: DRHD has null register base address");
        return;
    }

    if !super::is_valid_physical_address(drhd.register_base_addr as usize, 4096) {
        println!(
            "acpi/dmar: DRHD register address {:#x} is invalid",
            drhd.register_base_addr
        );
        return;
    }

    // Parse device scope entries (if any)
    let device_scope_start = core::mem::size_of::<DrhdHeader>();
    let mut ds_offset = device_scope_start;
    
    while ds_offset + core::mem::size_of::<DeviceScopeEntry>() <= length {
        let entry = &*((ptr.add(ds_offset)) as *const DeviceScopeEntry);
        
        if entry.length == 0 || ds_offset + entry.length as usize > length {
            break;
        }

        println!(
            "acpi/dmar:   device scope type {}, len {}",
            entry.entry_type,
            entry.length
        );

        ds_offset += entry.length as usize;
    }
}

/// Parse an RMRR (Reserved Memory Region Definition) structure
///
/// # Safety
/// - `ptr` must point to a valid RMRR structure
/// - `length` must be the correct structure length
unsafe fn parse_rmrr(ptr: *const u8, length: usize) {
    if length < core::mem::size_of::<RmrrHeader>() {
        println!("acpi/dmar: RMRR structure too short");
        return;
    }

    let rmrr = &*(ptr as *const RmrrHeader);

    println!(
        "acpi/dmar: RMRR segment {}, base {:#x}, limit {:#x}",
        rmrr.pci_segment,
        rmrr.rmrr_base_addr,
        rmrr.rmrr_limit_addr
    );

    // Validate addresses
    if rmrr.rmrr_base_addr == 0 || rmrr.rmrr_limit_addr == 0 {
        println!("acpi/dmar: RMRR has null addresses");
        return;
    }

    if rmrr.rmrr_base_addr >= rmrr.rmrr_limit_addr {
        println!("acpi/dmar: RMRR base >= limit (invalid)");
        return;
    }

    if !super::is_valid_physical_address(rmrr.rmrr_base_addr as usize, 1) {
        println!(
            "acpi/dmar: RMRR base address {:#x} is invalid",
            rmrr.rmrr_base_addr
        );
        return;
    }
}

/// Get the host address width (in bits).
///
/// # Returns
/// `Some(width)` if DMAR was successfully initialized, `None` otherwise.
pub fn get_host_address_width() -> Option<u8> {
    if DMAR_INITIALIZED.load(Ordering::Acquire) {
        Some(DMAR_HOST_ADDRESS_WIDTH.load(Ordering::Acquire))
    } else {
        None
    }
}

/// Get the DMAR flags.
///
/// # Returns
/// `Some(flags)` if DMAR was successfully initialized, `None` otherwise.
pub fn get_flags() -> Option<u8> {
    if DMAR_INITIALIZED.load(Ordering::Acquire) {
        Some(DMAR_FLAGS.load(Ordering::Acquire))
    } else {
        None
    }
}

/// Check if DMAR has been initialized.
pub fn is_initialized() -> bool {
    DMAR_INITIALIZED.load(Ordering::Relaxed)
}

/// DMAR flag definitions
pub mod flags {
    /// Intr Remapping Hardware Supported (bit 0)
    pub const INTR_REMAP: u8 = 1 << 0;
    /// X2APIC Mode Supported (bit 1)
    pub const X2APIC_MODE: u8 = 1 << 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dmar_structure_type_conversion() {
        assert_eq!(
            DmarStructureType::try_from(0),
            Ok(DmarStructureType::HardwareUnitDefinition)
        );
        assert_eq!(
            DmarStructureType::try_from(1),
            Ok(DmarStructureType::ReservedMemoryRegion)
        );
        assert_eq!(
            DmarStructureType::try_from(2),
            Ok(DmarStructureType::RootAtsCapability)
        );
        assert_eq!(
            DmarStructureType::try_from(3),
            Ok(DmarStructureType::Sasatc)
        );
        assert_eq!(DmarStructureType::try_from(4), Err(()));
        assert_eq!(DmarStructureType::try_from(0xFFFF), Err(()));
    }

    #[test]
    fn test_dmar_flag_bits() {
        assert_eq!(flags::INTR_REMAP, 0x01);
        assert_eq!(flags::X2APIC_MODE, 0x02);
    }

    #[test]
    fn test_atomic_initialization() {
        // Verify initial state
        assert!(!DMAR_INITIALIZED.load(Ordering::Relaxed));
        assert_eq!(DMAR_HOST_ADDRESS_WIDTH.load(Ordering::Relaxed), 0);
        assert_eq!(DMAR_FLAGS.load(Ordering::Relaxed), 0);
    }
}
