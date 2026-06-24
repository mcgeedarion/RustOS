//! Kernel Fault Injection Framework
//!
//! This module provides a lightweight, compile-time-gated fault injection
//! engine for resilience testing of critical kernel subsystems.
//!
//! # Design
//!
//! - **Zero-cost when disabled**: all public APIs compile to no-ops / `false`
//!   when `cfg(feature = "fault-inject")` is absent, so production builds
//!   are completely unaffected.
//! - **Per-subsystem counters**: each [`FaultPoint`] holds an atomic *budget*
//!   (how many more calls will succeed before a failure is injected) and a
//!   *trigger count* (how many times a failure has actually fired), allowing
//!   precise, repeatable test scenarios.
//! - **Thread-safe**: all state is manipulated through [`AtomicI64`] /
//!   [`AtomicU64`] so no mutex is needed on the hot path.
//!
//! # Usage
//!
//! ```rust,ignore
//! // In a test or debug path:
//! use crate::fault_inject::{FaultPoint, FAULT_PMM_ALLOC};
//!
//! // Make the next physical-page allocation fail:
//! FAULT_PMM_ALLOC.arm(0);
//!
//! // In the allocator itself:
//! #[cfg(feature = "fault-inject")]
//! if FAULT_PMM_ALLOC.check() {
//!     return Err(AllocError::OutOfMemory);
//! }
//! ```
//!
//! # Compile-time gate
//!
//! The entire framework (including all static [`FaultPoint`] instances) only
//! exists when the `fault-inject` Cargo feature is enabled.  Call-sites must
//! wrap usage in `#[cfg(feature = "fault-inject")]` or use the provided
//! [`maybe_inject!`] macro which expands to nothing when the feature is off.

#![cfg(feature = "fault-inject")]

use core::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};

// ── Public re-exports ────────────────────────────────────────────────────────
pub use points::*;

// ── FaultPoint ───────────────────────────────────────────────────────────────

/// A single instrumentation site that can be armed to inject a failure.
///
/// Construct instances as `static` values (see [`points`] module).
pub struct FaultPoint {
    /// Human-readable name, used in log messages.
    pub name: &'static str,
    /// Whether the point is currently armed.  Reset to `false` after firing
    /// *all* available failure budget.
    armed: AtomicBool,
    /// Remaining successful calls before the next failure fires.
    /// An armed point with `budget == 0` fires on the *very next* call.
    /// Negative values are clamped to 0 internally.
    budget: AtomicI64,
    /// Total number of times a failure has been injected at this point.
    triggers: AtomicU64,
}

impl FaultPoint {
    /// Create a new, unarmed fault point.
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            armed: AtomicBool::new(false),
            budget: AtomicI64::new(0),
            triggers: AtomicU64::new(0),
        }
    }

    /// Arm this fault point.
    ///
    /// - `skip`: number of *successful* calls to allow before the first
    ///   failure fires.  Pass `0` to fail immediately on the next call.
    pub fn arm(&self, skip: u32) {
        self.budget.store(skip as i64, Ordering::Release);
        self.armed.store(true, Ordering::Release);
    }

    /// Disarm the fault point without waiting for it to fire.
    pub fn disarm(&self) {
        self.armed.store(false, Ordering::Release);
        self.budget.store(0, Ordering::Release);
    }

    /// Reset the trigger counter (call between test cases).
    pub fn reset_triggers(&self) {
        self.triggers.store(0, Ordering::Relaxed);
    }

    /// How many times has this point fired?
    pub fn trigger_count(&self) -> u64 {
        self.triggers.load(Ordering::Relaxed)
    }

    /// **Hot-path check**: returns `true` if a failure should be injected
    /// right now.  Callers should propagate an appropriate error immediately.
    ///
    /// Internally:
    /// 1. If not armed → return `false`.
    /// 2. Decrement budget.  If it was already ≤ 0 → fire, increment trigger
    ///    count, disarm (one-shot by default).
    /// 3. Otherwise → return `false`.
    #[inline]
    pub fn check(&self) -> bool {
        if !self.armed.load(Ordering::Acquire) {
            return false;
        }
        let remaining = self.budget.fetch_sub(1, Ordering::AcqRel);
        if remaining <= 0 {
            self.triggers.fetch_add(1, Ordering::Relaxed);
            // One-shot: disarm after firing so the kernel does not stay broken.
            self.armed.store(false, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Like [`check`] but keeps the point armed after firing (useful for
    /// testing that *every* subsequent call fails until explicitly disarmed).
    #[inline]
    pub fn check_persistent(&self) -> bool {
        if !self.armed.load(Ordering::Acquire) {
            return false;
        }
        let remaining = self.budget.fetch_sub(1, Ordering::AcqRel);
        if remaining <= 0 {
            // Reset budget to -1 so every future check also fires immediately.
            self.budget.store(-1, Ordering::Release);
            self.triggers.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}

// SAFETY: FaultPoint only uses atomics; there is no interior mutability that
// requires exclusion beyond what the atomics provide.
unsafe impl Sync for FaultPoint {}
unsafe impl Send for FaultPoint {}

// ── Well-known fault points (subsystem catalogue) ────────────────────────────

/// All static [`FaultPoint`] instances used by the kernel subsystems.
pub mod points {
    use super::FaultPoint;

    // ── Physical Memory Manager ──────────────────────────────────────────────

    /// Injected into the PMM frame allocator.  Simulates physical-memory
    /// exhaustion (OOM) by making `alloc_frame()` return `None`.
    pub static FAULT_PMM_ALLOC: FaultPoint = FaultPoint::new("pmm::alloc_frame");

    /// Injected into the PMM bulk allocator (`alloc_frames_contiguous`).
    /// Simulates failure to satisfy a contiguous multi-page request.
    pub static FAULT_PMM_CONTIGUOUS: FaultPoint =
        FaultPoint::new("pmm::alloc_frames_contiguous");

    // ── Virtual Memory Manager ───────────────────────────────────────────────

    /// Injected into the VMM page-table mapper.  Simulates a failure to
    /// insert a new mapping (e.g., page-table page allocation failed).
    pub static FAULT_VMM_MAP: FaultPoint = FaultPoint::new("vmm::map_page");

    /// Injected into the VMM region allocator.  Simulates `ENOMEM` when
    /// trying to reserve a new virtual-address region.
    pub static FAULT_VMM_REGION: FaultPoint = FaultPoint::new("vmm::alloc_region");

    // ── Syscall Paths ────────────────────────────────────────────────────────

    /// Injected at the syscall dispatch layer.  Simulates resource exhaustion
    /// (`EAGAIN` / `ENOMEM`) for any syscall that touches kernel resources.
    pub static FAULT_SYSCALL_RESOURCE: FaultPoint =
        FaultPoint::new("syscall::resource_check");

    /// Injected into the per-process file-descriptor table.  Simulates
    /// `EMFILE` (per-process fd limit reached).
    pub static FAULT_SYSCALL_FDTABLE: FaultPoint =
        FaultPoint::new("syscall::fdtable_insert");
}

// ── maybe_inject! macro ──────────────────────────────────────────────────────

/// Conditionally inject a failure at a [`FaultPoint`].
///
/// ```rust,ignore
/// // Expands (when feature = "fault-inject") to:
/// //   if FAULT_PMM_ALLOC.check() { return Err(AllocError::OutOfMemory); }
/// maybe_inject!(FAULT_PMM_ALLOC, Err(AllocError::OutOfMemory));
/// ```
///
/// When the feature is **disabled** the macro expands to nothing.
#[macro_export]
macro_rules! maybe_inject {
    ($point:expr, $ret:expr) => {
        #[cfg(feature = "fault-inject")]
        {
            if $point.check() {
                return $ret;
            }
        }
    };
}

/// Like [`maybe_inject!`] but uses [`FaultPoint::check_persistent`].
#[macro_export]
macro_rules! maybe_inject_persistent {
    ($point:expr, $ret:expr) => {
        #[cfg(feature = "fault-inject")]
        {
            if $point.check_persistent() {
                return $ret;
            }
        }
    };
}
