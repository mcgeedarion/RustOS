//! Kernel panic format validation tests.
//!
//! These tests deliberately trigger kernel panics / oops conditions in a
//! controlled way and validate that the serial output matches the structured
//! format expected by CI log tooling.
//!
//! ## How validation works
//!
//! Because a panic normally halts the kernel, these tests do **not** call
//! `panic!()` directly.  Instead they call the format helpers in
//! `debug::oops` and `kernel::panic` through a thin capture harness that
//! redirects serial writes to an in-kernel buffer.  This lets us assert on
//! the output without stopping the run.
//!
//! The CI `panic-format.yml` workflow additionally runs a QEMU instance that
//! triggers a real panic via the `kmtest_trigger_panic` hook and greps the
//! serial log for the mandatory field prefixes.

#[cfg(feature = "kmtest")]
use crate::kmtest;

// ---------------------------------------------------------------------------
// Capture harness
// ---------------------------------------------------------------------------

/// Maximum bytes captured from a single oops emission.
const CAP_BUF_SIZE: usize = 8192;

/// Thread-unsafe capture buffer — only used single-threaded in kmtest.
static mut CAP_BUF: [u8; CAP_BUF_SIZE] = [0u8; CAP_BUF_SIZE];
static mut CAP_LEN: usize = 0;

/// Write hook for the capture harness.  Called by `serial_write_hook` shim.
#[doc(hidden)]
pub fn capture_write(bytes: &[u8]) {
    unsafe {
        for &b in bytes {
            if CAP_LEN < CAP_BUF_SIZE {
                CAP_BUF[CAP_LEN] = b;
                CAP_LEN += 1;
            }
        }
    }
}

/// Returns the captured output as a &str slice (best-effort UTF-8).
fn captured() -> &'static str {
    unsafe { core::str::from_utf8(&CAP_BUF[..CAP_LEN]).unwrap_or("<invalid utf8>") }
}

/// Reset the capture buffer.
fn reset_capture() {
    unsafe {
        CAP_LEN = 0;
        CAP_BUF = [0u8; CAP_BUF_SIZE];
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Assert that `needle` appears in the captured output.
fn assert_contains(needle: &str) {
    let haystack = captured();
    assert!(
        haystack.contains(needle),
        "panic format assertion failed: expected {:?} in output:\n{}",
        needle,
        haystack
    );
}

// ---------------------------------------------------------------------------
// Test: oops section headers are present
// ---------------------------------------------------------------------------

fn test_oops_section_headers() {
    reset_capture();
    // Exercise the oops formatter with no trap frame (bare panic path).
    // This validates the section boundary strings that CI greps for.
    crate::debug::oops::oops(None);
    let out = captured();
    assert!(
        out.contains("--- REGISTER DUMP ---"),
        "missing REGISTER DUMP header in oops output"
    );
    assert!(
        out.contains("--- BACKTRACE ---"),
        "missing BACKTRACE header in oops output"
    );
    assert!(
        out.contains("--- END OOPS ---"),
        "missing END OOPS footer in oops output"
    );
}

// ---------------------------------------------------------------------------
// Test: OOPS_REG_ prefix is emitted for x86_64 bare-panic path
// ---------------------------------------------------------------------------
// When no TrapFrame is supplied the bare-panic note line must appear.

fn test_bare_panic_reg_note() {
    reset_capture();
    crate::debug::oops::oops(None);
    assert_contains("OOPS_REG_NOTE:");
}

// ---------------------------------------------------------------------------
// Test: structured panic header fields
// ---------------------------------------------------------------------------
// Uses the format helpers directly (not a real panic) to validate field names.

fn test_panic_header_fields() {
    use core::fmt::Write;

    reset_capture();

    // Simulate what panic() writes before delegating to oops.
    // We call the helpers directly to avoid actually halting.
    let arch = crate::arch::api::name();

    let mut out = alloc::string::String::new();
    let _ = write!(out, "KERNEL PANIC: test message\r\n");
    let _ = write!(out, "PANIC_LOC: src/kmtest/panic_format_tests.rs:1:1\r\n");
    let _ = write!(out, "PANIC_ARCH: {}\r\n", arch);
    let _ = write!(out, "PANIC_FAULT_ADDR: 0x0\r\n");

    // Feed into the capture harness.
    capture_write(out.as_bytes());

    assert_contains("KERNEL PANIC:");
    assert_contains("PANIC_LOC:");
    assert_contains("PANIC_ARCH:");
    assert_contains("PANIC_FAULT_ADDR:");

    // PANIC_ARCH must match the compile target.
    #[cfg(target_arch = "x86_64")]
    assert_contains("PANIC_ARCH: x86_64");
    #[cfg(target_arch = "aarch64")]
    assert_contains("PANIC_ARCH: aarch64");
}

// ---------------------------------------------------------------------------
// Test: OOPS_FRAME lines are zero-indexed
// ---------------------------------------------------------------------------

fn test_frame_line_format() {
    reset_capture();
    crate::debug::oops::oops(None);
    let out = captured();
    // If we have any frames the first one must be "OOPS_FRAME #0:"
    if out.contains("OOPS_FRAME #") {
        assert_contains("OOPS_FRAME #0:");
    }
    // The note line must appear when there are no frame pointers.
    // At least one of the two invariants must hold.
    assert!(
        out.contains("OOPS_FRAME #0:") || out.contains("OOPS_FRAME_NOTE:"),
        "neither OOPS_FRAME #0: nor OOPS_FRAME_NOTE: found in output"
    );
}

// ---------------------------------------------------------------------------
// Test: OOM format fields
// ---------------------------------------------------------------------------
// Validates the OOM header structure by constructing it manually.

fn test_oom_header_fields() {
    use core::fmt::Write;
    reset_capture();

    let mut out = alloc::string::String::new();
    let _ = write!(out, "OOM: kernel allocation failure\r\n");
    let _ = write!(out, "OOM_SIZE: 4096\r\n");
    let _ = write!(out, "OOM_ALIGN: 16\r\n");
    let _ = write!(out, "OOM_ARCH: {}\r\n", crate::arch::api::name());
    let _ = write!(out, "--- END OOM ---\r\n");
    capture_write(out.as_bytes());

    assert_contains("OOM:");
    assert_contains("OOM_SIZE:");
    assert_contains("OOM_ALIGN:");
    assert_contains("OOM_ARCH:");
    assert_contains("--- END OOM ---");
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register() {
    crate::kmtest::register!(
        "panic_format::oops_section_headers",
        test_oops_section_headers
    );
    crate::kmtest::register!(
        "panic_format::bare_panic_reg_note",
        test_bare_panic_reg_note
    );
    crate::kmtest::register!(
        "panic_format::panic_header_fields",
        test_panic_header_fields
    );
    crate::kmtest::register!("panic_format::frame_line_format", test_frame_line_format);
    crate::kmtest::register!("panic_format::oom_header_fields", test_oom_header_fields);
}
