//! PMM fault-injection integration.
//!
//! Import this module's public API from `src/mm/mod.rs` (guarded by
//! `#[cfg(feature = "fault-inject")]`) and call the hooks at the top of
//! each allocator entry-point.
//!
//! # Integration example (in pmm.rs / pmm/mod.rs)
//!
//! ```rust,ignore
//! pub fn alloc_frame() -> Option<PhysFrame> {
//!     #[cfg(feature = "fault-inject")]
//!     if crate::mm::pmm_fault::check_alloc_frame() {
//!         log::warn!("[fault-inject] pmm::alloc_frame injected OOM");
//!         return None;
//!     }
//!     // ... real allocation ...
//! }
//! ```

#![cfg(feature = "fault-inject")]

use crate::fault_inject::points::{FAULT_PMM_ALLOC, FAULT_PMM_CONTIGUOUS};

/// Returns `true` → caller should return `None` (OOM).
#[inline(always)]
pub fn check_alloc_frame() -> bool {
    if FAULT_PMM_ALLOC.check() {
        log::warn!("[fault-inject] FAULT_PMM_ALLOC fired — injecting OOM");
        true
    } else {
        false
    }
}

/// Returns `true` → caller should return `Err(AllocError::OutOfMemory)`.
#[inline(always)]
pub fn check_alloc_contiguous() -> bool {
    if FAULT_PMM_CONTIGUOUS.check() {
        log::warn!("[fault-inject] FAULT_PMM_CONTIGUOUS fired — injecting contiguous alloc failure");
        true
    } else {
        false
    }
}
