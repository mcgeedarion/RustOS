//! Fault-inject integration helpers.
//!
//! This file provides thin wrapper functions that subsystems call on their
//! hot paths.  Each wrapper checks the relevant [`FaultPoint`] and returns
//! the appropriate kernel error type if a failure should be injected.
//!
//! Using dedicated wrappers (rather than scattering raw `maybe_inject!`
//! calls everywhere) keeps the injection logic in one place and makes it
//! easy to audit or extend.

#![cfg(feature = "fault-inject")]

use super::points::{
    FAULT_PMM_ALLOC, FAULT_PMM_CONTIGUOUS, FAULT_SYSCALL_FDTABLE, FAULT_SYSCALL_RESOURCE,
    FAULT_VMM_MAP, FAULT_VMM_REGION,
};

// ── PMM hooks ────────────────────────────────────────────────────────────────

/// Call from `pmm::alloc_frame()` before the real allocation logic.
///
/// Returns `true` when a failure should be injected (caller must return
/// `None` / `Err(AllocError::OutOfMemory)`).
#[inline(always)]
pub fn pmm_alloc_frame_hook() -> bool {
    FAULT_PMM_ALLOC.check()
}

/// Call from `pmm::alloc_frames_contiguous()` before the real logic.
#[inline(always)]
pub fn pmm_alloc_contiguous_hook() -> bool {
    FAULT_PMM_CONTIGUOUS.check()
}

// ── VMM hooks ────────────────────────────────────────────────────────────────

/// Call from `vmm::map_page()` / `vmm::map_range()` before issuing
/// the real page-table walk and insertion.
#[inline(always)]
pub fn vmm_map_hook() -> bool {
    FAULT_VMM_MAP.check()
}

/// Call from the virtual-address region allocator before reserving space.
#[inline(always)]
pub fn vmm_region_hook() -> bool {
    FAULT_VMM_REGION.check()
}

// ── Syscall hooks ────────────────────────────────────────────────────────────

/// Call at the top of the syscall dispatch path to simulate generic
/// resource exhaustion.  The kernel errno equivalent is `ENOMEM` / `EAGAIN`.
#[inline(always)]
pub fn syscall_resource_hook() -> bool {
    FAULT_SYSCALL_RESOURCE.check()
}

/// Call inside `fdtable::insert()` to simulate `EMFILE`.
#[inline(always)]
pub fn syscall_fdtable_hook() -> bool {
    FAULT_SYSCALL_FDTABLE.check()
}
