//! In-kernel fault-injection test suite.
//!
//! Gated behind **both** `fault-inject` and `kmtest` features so these tests
//! are never compiled into a production kernel image.
//!
//! # Running
//!
//! ```
//! cargo build --features "fault-inject,kmtest" --target <your-target>
//! ```
//!
//! Each test function follows the convention used by the existing `kmtest`
//! harness: return `Ok(())` on success or `Err(&'static str)` with a
//! descriptive message on failure.  Panics are explicitly disallowed on
//! expected error paths — the tests assert `Err(_)` return values instead.

#![cfg(all(feature = "fault-inject", feature = "kmtest"))]

use crate::fault_inject::points::{
    FAULT_PMM_ALLOC, FAULT_PMM_CONTIGUOUS,
    FAULT_VMM_MAP, FAULT_VMM_REGION,
    FAULT_SYSCALL_RESOURCE, FAULT_SYSCALL_FDTABLE,
};
use crate::syscall::fault::{check_resource, check_fdtable, FaultErrno};
use crate::mm::pmm_fault::{check_alloc_frame, check_alloc_contiguous};
use crate::mm::vmm_fault::{check_map_page, check_alloc_region};

// ── Helper ───────────────────────────────────────────────────────────────────

/// Resets all fault points to a clean (disarmed, zero-trigger) state.
/// Call at the start of each test to prevent cross-test contamination.
fn reset_all() {
    for point in &[
        &FAULT_PMM_ALLOC,
        &FAULT_PMM_CONTIGUOUS,
        &FAULT_VMM_MAP,
        &FAULT_VMM_REGION,
        &FAULT_SYSCALL_RESOURCE,
        &FAULT_SYSCALL_FDTABLE,
    ] {
        point.disarm();
        point.reset_triggers();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TEST 1 — PMM OOM injection
// ─────────────────────────────────────────────────────────────────────────────

/// Verify that arming `FAULT_PMM_ALLOC` causes `check_alloc_frame()` to
/// return `true` (OOM) exactly once, and that callers can handle it without
/// panicking.
pub fn test_pmm_oom_injection() -> Result<(), &'static str> {
    reset_all();

    // Arm: fail immediately on the next call.
    FAULT_PMM_ALLOC.arm(0);

    // Simulate allocator entry point.
    let oom = check_alloc_frame();
    if !oom {
        return Err("FAULT_PMM_ALLOC armed but check_alloc_frame() returned false");
    }

    // After one-shot fire, the point must be disarmed.
    if check_alloc_frame() {
        return Err("FAULT_PMM_ALLOC fired twice — one-shot contract violated");
    }

    // Trigger count must be exactly 1.
    if FAULT_PMM_ALLOC.trigger_count() != 1 {
        return Err("FAULT_PMM_ALLOC trigger_count != 1 after one injection");
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// TEST 2 — PMM contiguous alloc failure with skip-budget
// ─────────────────────────────────────────────────────────────────────────────

/// Verify the `skip` budget: arming with `skip=2` allows two successful
/// calls before the third call is failed.
pub fn test_pmm_contiguous_skip_budget() -> Result<(), &'static str> {
    reset_all();

    // Allow 2 passes, fail on the 3rd call.
    FAULT_PMM_CONTIGUOUS.arm(2);

    // First two calls should NOT trigger.
    if check_alloc_contiguous() {
        return Err("call 1 of 3 triggered unexpectedly (budget was 2)");
    }
    if check_alloc_contiguous() {
        return Err("call 2 of 3 triggered unexpectedly (budget was 2)");
    }
    // Third call should trigger.
    if !check_alloc_contiguous() {
        return Err("call 3 of 3 did NOT trigger — expected failure injection");
    }

    if FAULT_PMM_CONTIGUOUS.trigger_count() != 1 {
        return Err("FAULT_PMM_CONTIGUOUS trigger_count != 1");
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// TEST 3 — VMM map failure injection
// ─────────────────────────────────────────────────────────────────────────────

/// Verify that `FAULT_VMM_MAP` causes `check_map_page()` to return `true`
/// (map failure) and that the kernel does not panic — only returns an error.
pub fn test_vmm_map_failure() -> Result<(), &'static str> {
    reset_all();

    FAULT_VMM_MAP.arm(0);

    // Simulate vmm::map_page() entry point.
    if !check_map_page() {
        return Err("FAULT_VMM_MAP armed but check_map_page() did not fire");
    }

    // Must be one-shot disarmed.
    if check_map_page() {
        return Err("check_map_page() fired a second time — one-shot violated");
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// TEST 4 — VMM region alloc failure injection
// ─────────────────────────────────────────────────────────────────────────────

/// Verify that `FAULT_VMM_REGION` causes virtual-address region allocation
/// to fail gracefully.
pub fn test_vmm_region_failure() -> Result<(), &'static str> {
    reset_all();

    FAULT_VMM_REGION.arm(0);

    if !check_alloc_region() {
        return Err("FAULT_VMM_REGION armed but check_alloc_region() did not fire");
    }
    if check_alloc_region() {
        return Err("check_alloc_region() fired a second time — one-shot violated");
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// TEST 5 — Syscall resource exhaustion
// ─────────────────────────────────────────────────────────────────────────────

/// Verify that `FAULT_SYSCALL_RESOURCE` makes `check_resource()` return
/// `Err(FaultErrno::ResourceExhausted)` — not a panic.
pub fn test_syscall_resource_exhaustion() -> Result<(), &'static str> {
    reset_all();

    FAULT_SYSCALL_RESOURCE.arm(0);

    match check_resource() {
        Err(FaultErrno::ResourceExhausted) => {} // expected
        Err(other) => {
            let _ = other; // avoid unused-variable warning in no_std
            return Err("check_resource() returned wrong error variant");
        }
        Ok(()) => return Err("check_resource() returned Ok — fault did not fire"),
    }

    // After one-shot, subsequent calls must succeed.
    if check_resource().is_err() {
        return Err("check_resource() still returning Err after one-shot disarm");
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// TEST 6 — Syscall fd-table exhaustion (EMFILE)
// ─────────────────────────────────────────────────────────────────────────────

/// Verify that `FAULT_SYSCALL_FDTABLE` causes `check_fdtable()` to return
/// `Err(FaultErrno::TooManyFiles)` without panicking.
pub fn test_syscall_fdtable_exhaustion() -> Result<(), &'static str> {
    reset_all();

    FAULT_SYSCALL_FDTABLE.arm(0);

    match check_fdtable() {
        Err(FaultErrno::TooManyFiles) => {} // expected
        Err(_) => return Err("check_fdtable() returned wrong error variant"),
        Ok(()) => return Err("check_fdtable() returned Ok — fault did not fire"),
    }

    // One-shot: must be clean now.
    if check_fdtable().is_err() {
        return Err("check_fdtable() still returning Err after one-shot disarm");
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// TEST 7 — Persistent mode: fault fires on every call until disarmed
// ─────────────────────────────────────────────────────────────────────────────

/// Verify `check_persistent()` behaviour: the fault keeps firing until
/// `disarm()` is called explicitly.
pub fn test_persistent_fault_mode() -> Result<(), &'static str> {
    reset_all();

    FAULT_VMM_MAP.arm(0);

    // Use check_persistent directly on the point.
    if !FAULT_VMM_MAP.check_persistent() {
        return Err("persistent check did not fire on first call");
    }
    if !FAULT_VMM_MAP.check_persistent() {
        return Err("persistent check did not fire on second call");
    }
    if !FAULT_VMM_MAP.check_persistent() {
        return Err("persistent check did not fire on third call");
    }

    // Explicit disarm must stop the firing.
    FAULT_VMM_MAP.disarm();
    if FAULT_VMM_MAP.check_persistent() {
        return Err("persistent check fired after explicit disarm");
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Test registration
// ─────────────────────────────────────────────────────────────────────────────

/// Register all fault-inject tests with the kmtest harness.
///
/// Call this from `src/kmtest/mod.rs` inside the
/// `#[cfg(all(feature = "fault-inject", feature = "kmtest"))]` block.
pub fn register_tests(runner: &mut crate::kmtest::TestRunner) {
    runner.register("fault_inject::pmm_oom_injection",           test_pmm_oom_injection);
    runner.register("fault_inject::pmm_contiguous_skip_budget",  test_pmm_contiguous_skip_budget);
    runner.register("fault_inject::vmm_map_failure",             test_vmm_map_failure);
    runner.register("fault_inject::vmm_region_failure",          test_vmm_region_failure);
    runner.register("fault_inject::syscall_resource_exhaustion", test_syscall_resource_exhaustion);
    runner.register("fault_inject::syscall_fdtable_exhaustion",  test_syscall_fdtable_exhaustion);
    runner.register("fault_inject::persistent_fault_mode",       test_persistent_fault_mode);
}
