//! ACPI error types for unified error handling across all ACPI submodules.
//!
//! This module provides a comprehensive error type that covers all possible
//! failure modes when parsing ACPI tables or interacting with ACPI functionality.

use core::fmt;

/// Unified error type for all ACPI operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpiError {
    /// RSDP signature was invalid or missing.
    RsdpInvalid,
    /// RSDP checksum validation failed.
    RsdpChecksumFailed,
    /// No RSDP pointer was provided (address was 0).
    RsdpNotFound,
    /// Table with specified signature was not found.
    TableNotFound(&'static str),
    /// Table signature did not match expected value.
    InvalidSignature(&'static str),
    /// Table checksum validation failed.
    ChecksumFailed(&'static str),
    /// Table length is shorter than required minimum.
    InvalidLength(&'static str),
    /// Physical address is null (0) when a valid address was expected.
    AddressInvalid(&'static str),
    /// PM1 control blocks were not properly initialized.
    MissingPmBlocks,
    /// DSDT is missing or has invalid signature.
    DsdtInvalid,
    /// FADT is missing or too short to parse required fields.
    FadtInvalid,
    /// Required ACPI table (FACP/FADT) is absent.
    FadtAbsent,
    /// S-state index is out of valid range (0-5).
    InvalidSleepState(usize),
    /// P-state index is out of range.
    PstateOutOfRange,
    /// No P-states were discovered in the DSDT.
    NoPstates,
    /// P-state request exceeds _PPC platform limit.
    PstateExceedsPpc,
    /// Architecture-specific operation not implemented.
    NotImplemented,
    /// Memory-mapped address is outside valid physical memory range.
    InvalidPhysicalAddress(usize),
    /// GPE block information is missing from FADT.
    NoGpeBlock,
    /// FACS table not found or invalid.
    FacsInvalid,
    /// SRAT table parsing encountered an error.
    SratError,
    /// SLIT table parsing encountered an error.
    SlitError,
}

impl fmt::Display for AcpiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AcpiError::RsdpInvalid => write!(f, "RSDP signature invalid"),
            AcpiError::RsdpChecksumFailed => write!(f, "RSDP checksum validation failed"),
            AcpiError::RsdpNotFound => write!(f, "no RSDP pointer provided"),
            AcpiError::TableNotFound(sig) => write!(f, "ACPI table '{}' not found", sig),
            AcpiError::InvalidSignature(sig) => write!(f, "invalid signature for '{}'", sig),
            AcpiError::ChecksumFailed(table) => write!(f, "{} checksum failed", table),
            AcpiError::InvalidLength(table) => write!(f, "{} length is invalid", table),
            AcpiError::AddressInvalid(field) => write!(f, "invalid address in {}", field),
            AcpiError::MissingPmBlocks => write!(f, "PM1 control blocks missing"),
            AcpiError::DsdtInvalid => write!(f, "DSDT missing or invalid"),
            AcpiError::FadtInvalid => write!(f, "FADT too short or malformed"),
            AcpiError::FadtAbsent => write!(f, "FADT/FACP table absent"),
            AcpiError::InvalidSleepState(sx) => write!(f, "invalid S-state {}", sx),
            AcpiError::PstateOutOfRange => write!(f, "P-state index out of range"),
            AcpiError::NoPstates => write!(f, "no P-states discovered"),
            AcpiError::PstateExceedsPpc => write!(f, "P-state exceeds _PPC limit"),
            AcpiError::NotImplemented => write!(f, "operation not implemented"),
            AcpiError::InvalidPhysicalAddress(addr) => {
                write!(f, "invalid physical address {:#x}", addr)
            }
            AcpiError::NoGpeBlock => write!(f, "no GPE0 block available"),
            AcpiError::FacsInvalid => write!(f, "FACS not found or invalid"),
            AcpiError::SratError => write!(f, "SRAT parsing error"),
            AcpiError::SlitError => write!(f, "SLIT parsing error"),
        }
    }
}

/// Result type alias for ACPI operations.
pub type AcpiResult<T> = Result<T, AcpiError>;

/// Validate that a physical address is within a reasonable range.
///
/// # Safety
/// This function does not actually validate that the address is mapped,
/// only that it's within a plausible physical address range.
///
/// # Arguments
/// * `addr` - The physical address to validate
/// * `size` - The size of the memory region to access
///
/// # Returns
/// `true` if the address appears valid, `false` otherwise
pub const fn is_valid_physical_address(addr: usize, size: usize) -> bool {
    // Reject null addresses
    if addr == 0 {
        return false;
    }
    
    // Reject addresses that would overflow
    if addr.checked_add(size).is_none() {
        return false;
    }
    
    // Reject addresses above 512 GiB (reasonable upper bound for most systems)
    // This can be adjusted based on target hardware
    const MAX_PHYSICAL_ADDR: usize = 0x80_0000_0000; // 512 GiB
    if addr > MAX_PHYSICAL_ADDR {
        return false;
    }
    
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        assert_eq!(
            format!("{}", AcpiError::TableNotFound("FACP")),
            "ACPI table 'FACP' not found"
        );
        assert_eq!(
            format!("{}", AcpiError::InvalidPhysicalAddress(0x1234)),
            "invalid physical address 0x1234"
        );
        assert_eq!(
            format!("{}", AcpiError::RsdpInvalid),
            "RSDP signature invalid"
        );
        assert_eq!(
            format!("{}", AcpiError::ChecksumFailed("DSDT")),
            "DSDT checksum failed"
        );
        assert_eq!(
            format!("{}", AcpiError::InvalidSleepState(7)),
            "invalid S-state 7"
        );
        assert_eq!(
            format!("{}", AcpiError::NotImplemented),
            "operation not implemented"
        );
    }

    #[test]
    fn test_error_equality() {
        assert_eq!(AcpiError::RsdpInvalid, AcpiError::RsdpInvalid);
        assert_eq!(
            AcpiError::TableNotFound("APIC"),
            AcpiError::TableNotFound("APIC")
        );
        assert_ne!(
            AcpiError::TableNotFound("FACP"),
            AcpiError::TableNotFound("APIC")
        );
    }

    #[test]
    fn test_valid_physical_address() {
        // Valid addresses within range
        assert!(is_valid_physical_address(0x1000, 100));
        assert!(is_valid_physical_address(0x1_0000_0000, 4096)); // 4 GiB
        assert!(is_valid_physical_address(0x80_0000_0000 - 100, 100)); // Just under 512 GiB
        
        // Null address
        assert!(!is_valid_physical_address(0, 100));
        assert!(!is_valid_physical_address(0, 4096));
        
        // Above 512 GiB limit
        assert!(!is_valid_physical_address(0x100_0000_0000, 100)); // 1 TiB
        assert!(!is_valid_physical_address(0x80_0000_0000, 100)); // Exactly 512 GiB
        
        // Overflow cases
        assert!(!is_valid_physical_address(usize::MAX, 100));
        assert!(!is_valid_physical_address(usize::MAX - 50, 100));
        
        // Edge case: exactly at boundary
        assert!(is_valid_physical_address(0x7FFF_FFFF, 100));
    }

    #[test]
    fn test_result_type_alias() {
        let ok_result: AcpiResult<()> = Ok(());
        let err_result: AcpiResult<()> = Err(AcpiError::RsdpNotFound);
        
        assert!(ok_result.is_ok());
        assert!(err_result.is_err());
        assert_eq!(err_result.unwrap_err(), AcpiError::RsdpNotFound);
    }
}
