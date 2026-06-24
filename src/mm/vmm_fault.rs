//! VMM fault-injection integration.
//!
//! Drop-in hooks for the virtual memory manager's mapping and region
//! reservation paths.  See `pmm_fault.rs` for the integration pattern.

#![cfg(feature = "fault-inject")]

use crate::fault_inject::points::{FAULT_VMM_MAP, FAULT_VMM_REGION};

/// Returns `true` → caller should return a map-failure error.
///
/// Suitable errno: `ENOMEM`.
#[inline(always)]
pub fn check_map_page() -> bool {
    if FAULT_VMM_MAP.check() {
        log::warn!("[fault-inject] FAULT_VMM_MAP fired — injecting map failure");
        true
    } else {
        false
    }
}

/// Returns `true` → caller should return a region-reservation error.
///
/// Suitable errno: `ENOMEM`.
#[inline(always)]
pub fn check_alloc_region() -> bool {
    if FAULT_VMM_REGION.check() {
        log::warn!("[fault-inject] FAULT_VMM_REGION fired — injecting region alloc failure");
        true
    } else {
        false
    }
}
