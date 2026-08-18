//! Integration tests for ACPI table parsing and validation.
//!
//! These tests use synthetic ACPI table data to verify correct parsing behavior,
//! error handling, and edge cases.

#[cfg(test)]
mod tests {
    use super::super::*;
    use core::mem::size_of;

    /// Create a valid RSDP v1 structure with correct checksum
    fn create_rsdp_v1(rsdt_addr: u32) -> [u8; 20] {
        let mut rsdp = [0u8; 20];
        
        // Signature "RSD PTR "
        rsdp[0..8].copy_from_slice(b"RSD PTR ");
        
        // OEM ID (6 bytes)
        rsdp[8..14].copy_from_slice(b"TESTOEM");
        
        // Revision 0 (v1)
        rsdp[14] = 0;
        
        // RSDT physical address (little-endian)
        rsdp[15..19].copy_from_slice(&rsdt_addr.to_le_bytes());
        
        // Calculate checksum (bytes 0-19 must sum to 0)
        let sum: u8 = rsdp[0..19].iter().fold(0, |acc, &b| acc.wrapping_add(b));
        rsdp[19] = sum.wrapping_neg();
        
        rsdp
    }

    /// Create a valid RSDP v2 structure with correct checksums
    fn create_rsdp_v2(xsdt_addr: u64) -> [u8; 36] {
        let mut rsdp = [0u8; 36];
        
        // Signature "RSD PTR "
        rsdp[0..8].copy_from_slice(b"RSD PTR ");
        
        // OEM ID (6 bytes)
        rsdp[8..14].copy_from_slice(b"TESTOEM");
        
        // Revision 2 (v2)
        rsdp[14] = 2;
        
        // Length (36 bytes)
        rsdp[16..20].copy_from_slice(&36u32.to_le_bytes());
        
        // XSDT physical address (little-endian)
        rsdp[20..28].copy_from_slice(&xsdt_addr.to_le_bytes());
        
        // Calculate v1 checksum (bytes 0-19)
        let sum1: u8 = rsdp[0..20].iter().fold(0, |acc, &b| acc.wrapping_add(b));
        rsdp[19] = sum1.wrapping_neg();
        
        // Calculate extended checksum (all 36 bytes)
        let sum2: u8 = rsdp[0..36].iter().fold(0, |acc, &b| acc.wrapping_add(b));
        rsdp[35] = sum2.wrapping_neg();
        
        rsdp
    }

    /// Create a minimal SDT header
    fn create_sdt_header(sig: &[u8; 4], length: u32) -> SdtHeader {
        let mut hdr = SdtHeader {
            sig: *sig,
            len: length,
            rev: 1,
            csum: 0,
            oem_id: *b"TESTOEM",
            oem_table_id: *b"TESTTABLE",
            oem_rev: 1,
            creator_id: 0x12345678,
            creator_rev: 1,
        };
        
        // Note: We don't calculate checksum here as it's not needed for these tests
        hdr
    }

    #[test]
    fn test_rsdp_v1_creation() {
        let rsdp = create_rsdp_v1(0x12345678);
        
        // Verify signature
        assert_eq!(&rsdp[0..8], b"RSD PTR ");
        
        // Verify revision
        assert_eq!(rsdp[14], 0);
        
        // Verify RSDT address
        let rsdt_addr = u32::from_le_bytes([rsdp[15], rsdp[16], rsdp[17], rsdp[18]]);
        assert_eq!(rsdt_addr, 0x12345678);
        
        // Verify checksum
        let sum: u8 = rsdp.iter().fold(0, |acc, &b| acc.wrapping_add(b));
        assert_eq!(sum, 0);
    }

    #[test]
    fn test_rsdp_v2_creation() {
        let rsdp = create_rsdp_v2(0x123456789ABCDEF0);
        
        // Verify signature
        assert_eq!(&rsdp[0..8], b"RSD PTR ");
        
        // Verify revision and length
        assert_eq!(rsdp[14], 2);
        let length = u32::from_le_bytes([rsdp[16], rsdp[17], rsdp[18], rsdp[19]]);
        assert_eq!(length, 36);
        
        // Verify XSDT address
        let xsdt_addr = u64::from_le_bytes([
            rsdp[20], rsdp[21], rsdp[22], rsdp[23],
            rsdp[24], rsdp[25], rsdp[26], rsdp[27]
        ]);
        assert_eq!(xsdt_addr, 0x123456789ABCDEF0);
        
        // Verify both checksums
        let sum1: u8 = rsdp[0..20].iter().fold(0, |acc, &b| acc.wrapping_add(b));
        assert_eq!(sum1, 0);
        
        let sum2: u8 = rsdp.iter().fold(0, |acc, &b| acc.wrapping_add(b));
        assert_eq!(sum2, 0);
    }

    #[test]
    fn test_sdt_header_creation() {
        let hdr = create_sdt_header(b"FACP", 100);
        
        assert_eq!(&hdr.sig, b"FACP");
        assert_eq!(hdr.len, 100);
        assert_eq!(hdr.rev, 1);
        assert_eq!(&hdr.oem_id, b"TESTOEM");
        assert_eq!(&hdr.oem_table_id, b"TESTTABLE");
    }

    #[test]
    fn test_checksum_helper() {
        // Valid checksum
        let valid = [0x41, 0x42, 0x43, 0x34]; // 'A' + 'B' + 'C' + 0x34 = 0x100 -> 0
        assert!(checksum_ok(&valid));
        
        // Invalid checksum
        let invalid = [0x41, 0x42, 0x43, 0x00];
        assert!(!checksum_ok(&invalid));
        
        // Empty slice
        let empty: [u8; 0] = [];
        assert!(checksum_ok(&empty));
        
        // All zeros
        let zeros = [0u8; 10];
        assert!(checksum_ok(&zeros));
    }

    #[test]
    fn test_signature_comparison() {
        let test_data = [b'T', b'E', b'S', b'T', 0x00, 0x01, 0x02, 0x03];
        
        unsafe {
            assert!(sig_eq(test_data.as_ptr(), b"TEST"));
            assert!(!sig_eq(test_data.as_ptr(), b"FAIL"));
        }
    }

    #[test]
    fn test_acpi_error_variants() {
        // Test that all error variants can be constructed
        let _errors = [
            AcpiError::RsdpInvalid,
            AcpiError::RsdpChecksumFailed,
            AcpiError::RsdpNotFound,
            AcpiError::TableNotFound("APIC"),
            AcpiError::InvalidSignature("MCFG"),
            AcpiError::ChecksumFailed("DSDT"),
            AcpiError::InvalidLength("FADT"),
            AcpiError::AddressInvalid("DSDT pointer"),
            AcpiError::MissingPmBlocks,
            AcpiError::DsdtInvalid,
            AcpiError::FadtInvalid,
            AcpiError::FadtAbsent,
            AcpiError::InvalidSleepState(7),
            AcpiError::PstateOutOfRange,
            AcpiError::NoPstates,
            AcpiError::PstateExceedsPpc,
            AcpiError::NotImplemented,
            AcpiError::InvalidPhysicalAddress(0xBADADDR),
            AcpiError::NoGpeBlock,
            AcpiError::FacsInvalid,
            AcpiError::SratError,
            AcpiError::SlitError,
        ];
    }

    #[test]
    fn test_pstate_struct() {
        use super::cpufreq::Pstate;
        
        let pstate = Pstate {
            freq_mhz: 2400,
            power_mw: 35000,
            latency_us: 100,
            control: 0x1234,
            status: 0x5678,
        };
        
        assert_eq!(pstate.freq_mhz, 2400);
        assert_eq!(pstate.power_mw, 35000);
        assert_eq!(pstate.latency_us, 100);
        assert_eq!(pstate.control, 0x1234);
        assert_eq!(pstate.status, 0x5678);
        
        // Test default
        let default_pstate = Pstate::default();
        assert_eq!(default_pstate.freq_mhz, 0);
        assert_eq!(default_pstate.power_mw, 0);
    }

    #[test]
    fn test_battery_info_struct() {
        use super::battery::{BatteryInfo, BatteryStatus};
        
        let info = BatteryInfo {
            power_unit: 0,
            design_capacity: 5000,
            last_full_capacity: 4800,
            battery_technology: 1,
            design_voltage_mv: 11100,
            warning_capacity: 500,
            low_capacity: 100,
            capacity_granularity1: 1,
            capacity_granularity2: 1,
        };
        
        assert_eq!(info.design_capacity, 5000);
        assert_eq!(info.last_full_capacity, 4800);
        assert_eq!(info.battery_technology, 1); // rechargeable
        
        let status = BatteryStatus {
            state: 1, // discharging
            present_rate: 2500,
            remaining_capacity: 3200,
            present_voltage: 10800,
        };
        
        assert_eq!(status.state, 1);
        assert_eq!(status.present_rate, 2500);
    }

    #[test]
    fn test_physical_address_validation_edge_cases() {
        // Maximum valid address (just under 512 GiB)
        let max_valid = 0x80_0000_0000usize - 1;
        assert!(is_valid_physical_address(max_valid, 1));
        
        // Exactly at limit should fail
        assert!(!is_valid_physical_address(0x80_0000_0000, 1));
        
        // Small size at boundary
        assert!(is_valid_physical_address(0x7FFF_FFFF, 1));
        assert!(is_valid_physical_address(0x7FFF_FFFF, 256));
        
        // Large size near boundary
        assert!(!is_valid_physical_address(0x7F_FF0000, 0x200000)); // Would exceed 512 GiB
    }
}
