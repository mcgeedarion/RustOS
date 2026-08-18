//! ACPI table parser: RSDP → RSDT/XSDT → MADT → interrupt routing.
//!
//! ## Why we care
//!
//! On UEFI systems the firmware hands us a pointer to the RSDP.  From there we
//! locate the MADT so we can discover LAPIC/IOAPIC information (x86_64) or any
//! other tables we may want later.
//!
//! Power management tables (FADT, DSDT) are parsed by the `power` sub-module.
//! S3 sleep / resume is handled by `sleep`.
//! CPU frequency scaling (`_PSS`/`_PPC`) is in `cpufreq`.
//! Battery information (`_BIF`/`_BST`) is in `battery`.
//! PCIe ACPI-mediated hot-plug (GPE + Notify) is in `hotplug`.
//! NUMA topology (SRAT + SLIT) is in `numa`.
//!
//! ## Thread Safety
//!
//! All ACPI initialization functions must be called during early boot before
//! SMP is enabled. After initialization, read-only access to ACPI data is safe
//! from multiple threads.

pub mod battery;
pub mod cpufreq;
pub mod error;
pub mod hotplug;
pub mod numa;
pub mod power;
pub mod sleep;

use core::mem::size_of;
use core::slice;
use core::sync::atomic::{AtomicPtr, Ordering};

use crate::println;

pub use error::{AcpiError, AcpiResult, is_valid_physical_address};

#[repr(C, packed)]
pub struct RsdpV1 {
    sig: [u8; 8],
    csum: u8,
    oem_id: [u8; 6],
    rev: u8,
    rsdt_phys: u32,
}

#[repr(C, packed)]
pub struct RsdpV2 {
    v1: RsdpV1,
    len: u32,
    xsdt_phys: u64,
    ext_csum: u8,
    _rsvd: [u8; 3],
}

#[repr(C, packed)]
pub struct SdtHeader {
    pub sig: [u8; 4],
    pub len: u32,
    pub rev: u8,
    pub csum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_rev: u32,
    pub creator_id: u32,
    pub creator_rev: u32,
}

#[repr(C, packed)]
pub struct Madt {
    pub hdr: SdtHeader,
    pub lapic_addr: u32,
    pub flags: u32,
}

#[repr(C, packed)]
pub struct MadtEntryHdr {
    pub kind: u8,
    pub len: u8,
}

/// Root table pointer type with proper synchronization.
/// 
/// Uses AtomicPtr for thread-safe access after initialization.
pub struct AcpiRoot {
    /// Pointer to the root table (RSDT or XSDT)
    table_ptr: *const SdtHeader,
    /// True if this is an XSDT (64-bit entries), false for RSDT (32-bit)
    is_xsdt: bool,
}

// SAFETY: AcpiRoot is only written during init() before SMP, then read-only
unsafe impl Send for AcpiRoot {}
unsafe impl Sync for AcpiRoot {}

/// Global ACPI root table pointer.
/// 
/// Initialized once during boot via `init()`, then accessed read-only.
/// Uses AtomicPtr with appropriate memory ordering for safe concurrent reads.
static ACPI_ROOT: AtomicPtr<AcpiRoot> = AtomicPtr::new(core::ptr::null_mut());

fn checksum_ok(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b)) == 0
}

unsafe fn sig_eq<const N: usize>(ptr: *const u8, sig: &[u8; N]) -> bool {
    slice::from_raw_parts(ptr, N) == sig
}

/// Initialize ACPI subsystem from the RSDP physical address.
///
/// # Safety
/// - Must be called exactly once during early boot, before SMP initialization
/// - `rsdp_phys` must point to a valid, mapped RSDP structure
/// - Caller must ensure the RSDP memory remains valid for the lifetime of the system
///
/// # Errors
/// Returns silently with a log message if:
/// - RSDP address is null
/// - RSDP signature is invalid
/// - RSDP checksum fails
/// - No valid root table (RSDT/XSDT) is found
pub unsafe fn init(rsdp_phys: usize) {
    // Validate RSDP physical address before dereferencing
    if rsdp_phys == 0 {
        println!("acpi: no rsdp (null address)");
        return;
    }
    
    if !is_valid_physical_address(rsdp_phys, size_of::<RsdpV1>()) {
        println!("acpi: rsdp physical address {:#x} is invalid", rsdp_phys);
        return;
    }

    let v1 = &*(rsdp_phys as *const RsdpV1);
    if !sig_eq(v1.sig.as_ptr(), b"RSD PTR ") {
        println!("acpi: bad rsdp signature");
        return;
    }
    if !checksum_ok(slice::from_raw_parts(
        rsdp_phys as *const u8,
        size_of::<RsdpV1>(),
    )) {
        println!("acpi: rsdp v1 checksum failed");
        return;
    }

    if v1.rev >= 2 {
        // Validate RSDP v2 extended area
        let v2_size = size_of::<RsdpV2>();
        if !is_valid_physical_address(rsdp_phys, v2_size) {
            println!("acpi: rsdp v2 extended area at {:#x} is invalid", rsdp_phys);
            return;
        }
        
        let v2 = &*(rsdp_phys as *const RsdpV2);
        let len = core::ptr::addr_of!(v2.len).read_unaligned() as usize;
        let xsdt_phys = core::ptr::addr_of!(v2.xsdt_phys).read_unaligned();
        
        if len < size_of::<RsdpV2>() {
            println!("acpi: rsdp v2 length {} too small", len);
            return;
        }
        
        if checksum_ok(slice::from_raw_parts(rsdp_phys as *const u8, len)) && xsdt_phys != 0 {
            // Validate XSDT physical address
            if !is_valid_physical_address(xsdt_phys as usize, size_of::<SdtHeader>()) {
                println!("acpi: xsdt physical address {:#x} is invalid", xsdt_phys);
                return;
            }
            
            let root = AcpiRoot {
                table_ptr: xsdt_phys as usize as *const SdtHeader,
                is_xsdt: true,
            };
            let root_box = Box::new(root);
            let root_ptr = Box::into_raw(root_box);
            
            // Use Release ordering to ensure all prior writes are visible
            ACPI_ROOT.store(root_ptr, Ordering::Release);
            println!("acpi: xsdt @ {:#x}", xsdt_phys);
            return;
        }
    }

    let rsdt_phys = core::ptr::addr_of!(v1.rsdt_phys).read_unaligned();
    if rsdt_phys != 0 {
        // Validate RSDT physical address
        if !is_valid_physical_address(rsdt_phys as usize, size_of::<SdtHeader>()) {
            println!("acpi: rsdt physical address {:#x} is invalid", rsdt_phys);
            return;
        }
        
        let root = AcpiRoot {
            table_ptr: rsdt_phys as usize as *const SdtHeader,
            is_xsdt: false,
        };
        let root_box = Box::new(root);
        let root_ptr = Box::into_raw(root_box);
        
        ACPI_ROOT.store(root_ptr, Ordering::Release);
        println!("acpi: rsdt @ {:#x}", rsdt_phys);
    }
}

/// Find an ACPI table by its 4-character signature.
///
/// # Safety
/// - Must be called after `init()` has successfully initialized ACPI
/// - The returned pointer is valid for the lifetime of the system
/// - Caller must not dereference the pointer without proper validation
///
/// # Arguments
/// * `sig` - 4-byte table signature (e.g., b"APIC", b"FACP", b"MCFG")
///
/// # Returns
/// `Some(pointer)` if table is found, `None` otherwise
pub unsafe fn find_table(sig: &[u8; 4]) -> Option<*const SdtHeader> {
    // Use Acquire ordering to ensure we see all writes from init()
    let root_ptr = ACPI_ROOT.load(Ordering::Acquire);
    if root_ptr.is_null() {
        return None;
    }
    
    let root = &*root_ptr;
    let hdr = root.table_ptr;
    
    // Validate the root table header before accessing
    if !is_valid_physical_address(hdr as usize, size_of::<SdtHeader>()) {
        println!("acpi: root table at invalid address {:#x}", hdr as usize);
        return None;
    }
    
    let hdr_ref = &*hdr;
    if hdr_ref.len < size_of::<SdtHeader>() as u32 {
        println!("acpi: root table length {} is too small", hdr_ref.len);
        return None;
    }
    
    if root.is_xsdt {
        // XSDT: 64-bit entries
        let total = hdr_ref.len as usize;
        let entries_bytes = total.saturating_sub(size_of::<SdtHeader>());
        let n = entries_bytes / 8;
        
        if n == 0 {
            return None;
        }
        
        let base = (hdr as usize + size_of::<SdtHeader>()) as *const u64;
        
        for i in 0..n {
            // Bounds check: ensure we don't read past the table
            let entry_addr = base.add(i) as usize;
            let table_end = hdr as usize + total;
            if entry_addr + 8 > table_end {
                println!("acpi: xsdt entry {} out of bounds", i);
                break;
            }
            
            let phys = *base.add(i) as usize;
            
            // Validate physical address before returning
            if phys == 0 || !is_valid_physical_address(phys, size_of::<SdtHeader>()) {
                println!("acpi: xsdt entry {} has invalid address {:#x}", i, phys);
                continue;
            }
            
            let th = &*(phys as *const SdtHeader);
            if &th.sig == sig {
                return Some(phys as *const SdtHeader);
            }
        }
    } else {
        // RSDT: 32-bit entries
        let total = hdr_ref.len as usize;
        let entries_bytes = total.saturating_sub(size_of::<SdtHeader>());
        let n = entries_bytes / 4;
        
        if n == 0 {
            return None;
        }
        
        let base = (hdr as usize + size_of::<SdtHeader>()) as *const u32;
        
        for i in 0..n {
            // Bounds check: ensure we don't read past the table
            let entry_addr = base.add(i) as usize;
            let table_end = hdr as usize + total;
            if entry_addr + 4 > table_end {
                println!("acpi: rsdt entry {} out of bounds", i);
                break;
            }
            
            let phys = *base.add(i) as usize;
            
            // Validate physical address before returning
            if phys == 0 || !is_valid_physical_address(phys, size_of::<SdtHeader>()) {
                println!("acpi: rsdt entry {} has invalid address {:#x}", i, phys);
                continue;
            }
            
            let th = &*(phys as *const SdtHeader);
            if &th.sig == sig {
                return Some(phys as *const SdtHeader);
            }
        }
    }
    
    None
}

pub unsafe fn madt() -> Option<&'static Madt> {
    let p = find_table(b"APIC")? as *const Madt;
    Some(&*p)
}

pub unsafe fn walk_madt(mut f: impl FnMut(&MadtEntryHdr, *const u8)) {
    let m = match madt() {
        Some(m) => m,
        None => return,
    };
    let start = (m as *const Madt as usize) + size_of::<Madt>();
    let end = (m as *const Madt as usize) + (m.hdr.len as usize);
    let mut p = start;
    while p + size_of::<MadtEntryHdr>() <= end {
        let h = &*(p as *const MadtEntryHdr);
        if h.len < size_of::<MadtEntryHdr>() as u8 || p + h.len as usize > end {
            break;
        }
        f(h, p as *const u8);
        p += h.len as usize;
    }
}

/// Return the physical base address of the PCIe ECAM window from the MCFG
/// table, if present.
///
/// The MCFG body starts at offset 44 from the table start:
///   36 bytes SdtHeader + 8 bytes reserved = 44.
/// The first MCFG allocation structure begins there with:
///   [0..8]  Base Address (u64)
///   [8..10] PCI Segment Group Number
///   [10]    Start PCI Bus
///   [11]    End PCI Bus
///   [12..16] Reserved
pub fn mcfg_base() -> Option<usize> {
    unsafe {
        let mcfg = find_table(b"MCFG")?;
        let body = (mcfg as usize + 44) as *const u64;
        let base = body.read_unaligned();
        if base == 0 {
            None
        } else {
            Some(base as usize)
        }
    }
}

/// Return the number of logical CPUs found in the MADT, or 1 if unknown.
pub fn cpu_count() -> usize {
    let mut count = 0usize;
    unsafe {
        walk_madt(|hdr, _| {
            // Type 0 = Local APIC (x86), type 0x0B = GIC CPU Interface (arm64)
            if hdr.kind == 0 || hdr.kind == 0x0B {
                count += 1;
            }
        });
    }
    if count == 0 {
        1
    } else {
        count
    }
}

/// Return the base address of the PCIe ECAM region from MCFG, if present.
/// Alias kept for x86_64 callers that used `pcie_ecam_base()`.
pub fn pcie_ecam_base() -> Option<u64> {
    mcfg_base().map(|b| b as u64)
}

/// Get the DSDT table from the FADT.
///
/// This is a helper function to avoid code duplication across multiple modules
/// (power, cpufreq, battery, hotplug).
///
/// # Safety
/// - Must be called after `init()` has successfully initialized ACPI
/// - The returned pointer is valid for the lifetime of the system
///
/// # Returns
/// `Some(&SdtHeader)` if DSDT is found and valid, `None` otherwise
pub unsafe fn get_dsdt() -> Option<&'static SdtHeader> {
    let fadt = find_table(b"FACP")?;
    
    // Validate FADT length before accessing DSDT pointer
    if (*fadt).len < 44 {
        println!("acpi: FADT too short for DSDT pointer");
        return None;
    }
    
    let base = fadt as *const u8;
    let dsdt_phys = (base.add(40) as *const u32).read_unaligned() as usize;
    
    if dsdt_phys == 0 {
        return None;
    }
    
    // Validate DSDT physical address
    if !is_valid_physical_address(dsdt_phys, size_of::<SdtHeader>()) {
        println!("acpi: DSDT at invalid physical address {:#x}", dsdt_phys);
        return None;
    }
    
    let dsdt = &*(dsdt_phys as *const SdtHeader);
    if &dsdt.sig != b"DSDT" {
        return None;
    }
    
    Some(dsdt)
}

/// Get the AML bytecode stream from a DSDT table.
///
/// # Safety
/// - Must be called after `init()` has successfully initialized ACPI
/// - The returned slice is valid for the lifetime of the system
///
/// # Arguments
/// * `dsdt` - Pointer to the DSDT header
///
/// # Returns
/// A slice containing the AML bytecode, or `None` if the table is invalid
pub unsafe fn get_aml_from_dsdt(dsdt: &'static SdtHeader) -> Option<&'static [u8]> {
    if dsdt.len < size_of::<SdtHeader>() as u32 {
        return None;
    }
    
    let aml_off = size_of::<SdtHeader>();
    let aml_len = (dsdt.len as usize).saturating_sub(aml_off);
    
    if aml_len == 0 {
        return None;
    }
    
    let aml_start = (dsdt as *const SdtHeader as usize) + aml_off;
    
    // Validate the AML region
    if !is_valid_physical_address(aml_start, aml_len) {
        return None;
    }
    
    Some(core::slice::from_raw_parts(aml_start as *const u8, aml_len))
}

/// Convenience wrapper to get DSDT AML bytecode.
///
/// # Safety
/// - Must be called after `init()` has successfully initialized ACPI
pub unsafe fn get_dsdt_aml() -> Option<&'static [u8]> {
    let dsdt = get_dsdt()?;
    get_aml_from_dsdt(dsdt)
}

/// Convenience: initialise all ACPI sub-systems after the RSDP has been found.
///
/// Call order:
/// 1. `init(rsdp_phys)`       — root table discovery
/// 2. `power::init()`         — FADT/DSDT, SCI IRQ
/// 3. `sleep::init()`         — FACS, S3 wakeup vector
/// 4. `cpufreq::init()`       — _PSS / _PPC P-state table
/// 5. `battery::init()`       — _BIF / _BST battery info
/// 6. `hotplug::init()`       — GPE hot-plug handler
/// 7. `numa::init()`          — SRAT + SLIT topology
pub unsafe fn init_all(rsdp_phys: usize) {
    init(rsdp_phys);
    power::init();
    sleep::init();
    cpufreq::init();
    battery::init();
    hotplug::init();
    numa::init();
}

/// Convenience wrapper used by x86_64 kernel_main.
pub unsafe fn acpi_init(rsdp_phys: usize) {
    init(rsdp_phys);
}
