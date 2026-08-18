//! ACPI CPU Frequency Scaling — `_PSS` / `_PCT` / `_PPC` parser + P-state
//! driver.
//!
//! ## Design
//!
//! ACPI defines **P-states** (performance states) in the DSDT/SSDT via:
//! - `_PCT`  — control/status register addresses (MSR or I/O)
//! - `_PSS`  — list of (freq_MHz, power_mW, latency_us, bus_latency_us, control, status) tuples,
//!   sorted highest→lowest performance
//! - `_PPC`  — highest P-state the platform currently allows
//!
//! Because we do not have a full AML interpreter, we perform a *byte-pattern
//! scan* of the DSDT AML for the well-known `_PSS` package structure.  This
//! covers the vast majority of x86 laptops and QEMU's default DSDT.
//!
//! For actual register writes we use the Intel SpeedStep MSR (0x199)
//! unconditionally; CPUID checks should be added before shipping.
//!
//! ## Thread Safety
//!
//! All initialization must complete before SMP is enabled. After initialization,
//! read-only access to P-state data is safe from multiple threads.

use super::error::{AcpiError, AcpiResult};
use super::SdtHeader;
use crate::println;
use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

const MAX_PSTATES: usize = 16;

#[derive(Copy, Clone, Default, Debug)]
pub struct Pstate {
    /// Core frequency in MHz.
    pub freq_mhz: u32,
    /// Typical power dissipation in mW.
    pub power_mw: u32,
    /// Transition latency in µs.
    pub latency_us: u32,
    /// Value to write to the control register to enter this state.
    pub control: u32,
    /// Value the status register returns when in this state.
    pub status: u32,
}

use spin::Once;

/// P-state table storage with proper synchronization.
/// Initialized once during boot, then read-only.
static PSTATE_TABLE: Once<[Pstate; MAX_PSTATES]> = Once::new();
static PSTATE_COUNT: AtomicU8 = AtomicU8::new(0);
static CURRENT_PSTATE: AtomicU8 = AtomicU8::new(0);
static MAX_ALLOWED: AtomicU8 = AtomicU8::new(0); // from _PPC

/// Get mutable reference to PSTATE_TABLE during initialization.
/// 
/// # Safety
/// - Must only be called during single-threaded boot before SMP
/// - PSTATE_TABLE must not have been initialized yet
unsafe fn get_pstate_table_mut() -> &'static mut [Pstate; MAX_PSTATES] {
    PSTATE_TABLE.get_or_try_init(|| {
        Ok::<[Pstate; MAX_PSTATES], ()>([Pstate {
            freq_mhz: 0,
            power_mw: 0,
            latency_us: 0,
            control: 0,
            status: 0,
        }; MAX_PSTATES])
    }).unwrap()
}

const IA32_PERF_CTL: u32 = 0x199;
const IA32_PERF_STS: u32 = 0x198;

#[inline(always)]
#[cfg(target_arch = "x86_64")]
unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") lo,
        out("edx") hi,
        options(nomem, nostack)
    );
    (hi as u64) << 32 | lo as u64
}

#[inline(always)]
#[cfg(target_arch = "x86_64")]
unsafe fn wrmsr(msr: u32, val: u64) {
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") val as u32,
        in("edx") (val >> 32) as u32,
        options(nomem, nostack)
    );
}

/// Extract a DWORD from AML stream at `i` after an 0x0C (DWordPrefix).
/// Returns the value and advances `i` by 5.
///
/// # Safety
/// Caller must ensure `i` is within bounds of `aml`.
unsafe fn read_aml_dword(aml: &[u8], i: usize) -> Option<u32> {
    // Use .get() for bounds-safe access
    if *aml.get(i)? != 0x0C {
        return None;
    }
    
    // Check bounds for the 4-byte payload
    let payload = aml.get(i + 1..i + 5)?;
    if payload.len() != 4 {
        return None;
    }
    
    Some(u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]))
}

/// Scan the DSDT AML for `_PSS` package entries.
/// Each entry is a Package{} of 6 DWORDs.
unsafe fn scan_pss(aml: &[u8]) {
    let marker = *b"_PSS";
    let mut i = 0usize;
    let mut found = 0usize;

    // Safety: bounded by aml.len() at every access.
    while i + 6 < aml.len() {
        if aml[i..i + 4] != marker {
            i += 1;
            continue;
        }
        // Skip _PSS name (4 bytes) + Package opcode (0x12, 1 byte) + PkgLength.
        // PkgLength encoding: if byte < 0x40 it is the length; otherwise multi-byte.
        let mut j = i + 4;
        if j >= aml.len() {
            break;
        }
        if aml[j] != 0x12 {
            i += 1;
            continue;
        } // PackageOp
        j += 1;
        // Skip PkgLength (1..4 bytes).
        if j >= aml.len() {
            break;
        }
        let pkg_lead = aml[j];
        let pkg_hdr_extra = (pkg_lead >> 6) as usize;
        j += 1 + pkg_hdr_extra;
        // NumElements byte.
        if j >= aml.len() {
            break;
        }
        let num_entries = aml[j] as usize;
        j += 1;

        let count = num_entries.min(MAX_PSTATES);
        for e in 0..count {
            // Each PSS element is Package{DWord×6}.
            // We expect: PackageOp(0x12), PkgLen, NumEl(6),
            // then 6 × (DWordPrefix + 4 bytes).
            if j + 2 >= aml.len() {
                break;
            }
            if aml[j] != 0x12 {
                j += 1;
                continue;
            }
            j += 1; // skip PackageOp
                    // skip inner PkgLength
            if j >= aml.len() {
                break;
            }
            let il = aml[j] as usize;
            let il_extra = (aml[j] >> 6) as usize;
            j += 1 + il_extra;
            if j >= aml.len() {
                break;
            }
            j += 1; // NumElements = 6

            let mut vals = [0u32; 6];
            let mut ok = true;
            for k in 0..6 {
                match read_aml_dword(aml, j) {
                    Some(v) => {
                        vals[k] = v;
                        j += 5;
                    },
                    None => {
                        ok = false;
                        break;
                    },
                }
            }
            if !ok {
                continue;
            }
            if e < MAX_PSTATES {
                let table = get_pstate_table_mut();
                table[e] = Pstate {
                    freq_mhz: vals[0],
                    power_mw: vals[1],
                    latency_us: vals[2],
                    control: vals[4],
                    status: vals[5],
                };
                found = found.max(e + 1);
            }
        }
        break; // only parse the first _PSS
    }

    PSTATE_COUNT.store(found as u8, Ordering::Release);
    if found > 0 {
        println!("acpi/cpufreq: {} P-states discovered", found);
        let table = PSTATE_TABLE.get().unwrap();
        for k in 0..found {
            println!(
                "  P{}  {} MHz  {} mW  ctrl={:#x}",
                k, table[k].freq_mhz, table[k].power_mw, table[k].control
            );
        }
    }
}

/// Scan for `_PPC` (maximum allowed P-state index).
unsafe fn scan_ppc(aml: &[u8]) {
    let marker = *b"_PPC";
    let mut i = 0usize;
    while i + 7 < aml.len() {
        if aml.get(i..i + 4) == Some(&marker[..]) {
            // _PPC typically returns a byte integer: 0x0A <value>.
            if i + 5 < aml.len() && aml[i + 4] == 0x0A {
                MAX_ALLOWED.store(aml[i + 5], Ordering::Release);
                println!("acpi/cpufreq: _PPC = {}", aml[i + 5]);
            }
            return;
        }
        i += 1;
    }
}

/// Parse the DSDT for P-state tables.
///
/// # Safety
/// - Must be called after `super::init()` and `super::power::parse_dsdt()`
/// - Must complete before SMP initialization (writes to static P-state table)
///
/// # Errors
/// Returns silently if DSDT is not found or malformed.
pub unsafe fn init() {
    // Use the shared helper to get DSDT AML with proper validation
    let aml = match super::get_dsdt_aml() {
        Some(a) => a,
        None => {
            println!("acpi/cpufreq: DSDT not found or invalid");
            return;
        },
    };

    scan_pss(aml);
    scan_ppc(aml);

    // Default to the highest-performance state.
    if PSTATE_COUNT.load(Ordering::Acquire) > 0 {
        let _ = set_pstate(0);
    }
}

/// Return a copy of all discovered P-states.
///
/// # Panics
/// Panics if called before `init()` has initialized the P-state table.
pub fn pstates() -> &'static [Pstate] {
    let n = PSTATE_COUNT.load(Ordering::Acquire) as usize;
    // SAFETY: PSTATE_TABLE is only written during `init()` before any CPU
    // goes multi-threaded; reads afterwards are safe with Acquire ordering.
    unsafe { &PSTATE_TABLE[..n] }
}

/// Request P-state `index` (0 = highest performance).
///
/// # Safety
/// - Must be called after `init()` has initialized the P-state table
/// - On x86_64, requires CPU support for MSR writes
///
/// # Errors
/// Returns `Err` if:
/// - No P-states are available
/// - Index is out of range
/// - Index exceeds `_PPC` platform limit
#[cfg(target_arch = "x86_64")]
pub fn set_pstate(index: usize) -> Result<(), &'static str> {
    let count = PSTATE_COUNT.load(Ordering::Acquire) as usize;
    let ppc = MAX_ALLOWED.load(Ordering::Acquire) as usize;

    if count == 0 {
        return Err("no P-states available");
    }
    if index >= count {
        return Err("P-state index out of range");
    }
    if index < ppc {
        return Err("P-state exceeds _PPC limit");
    }

    let ctrl = unsafe { PSTATE_TABLE.get(index).ok_or("P-state index out of bounds")?.control } as u64;
    unsafe {
        wrmsr(IA32_PERF_CTL, ctrl);
    }
    CURRENT_PSTATE.store(index as u8, Ordering::Release);
    println!("acpi/cpufreq: → P{}  {} MHz", index, unsafe {
        PSTATE_TABLE.get(index).unwrap().freq_mhz
    });
    Ok(())
}

#[cfg(not(target_arch = "x86_64"))]
pub fn set_pstate(_index: usize) -> Result<(), &'static str> {
    Err("cpufreq not implemented for this arch")
}

/// Return the current P-state index.
pub fn current_pstate() -> usize {
    CURRENT_PSTATE.load(Ordering::Acquire) as usize
}

/// Read back the current performance status from the hardware.
#[cfg(target_arch = "x86_64")]
pub fn read_perf_status() -> u64 {
    unsafe { rdmsr(IA32_PERF_STS) }
}
