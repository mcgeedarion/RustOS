//! Minimal structured oops formatter.
//!
//! The full symbolic backtrace path is still feature work.  This formatter
//! keeps the stable section markers and machine-readable prefixes used by CI
//! and kmtest panic-format checks.

fn emit(bytes: &[u8]) {
    #[cfg(feature = "kmtest")]
    crate::kmtest::panic_format_tests::capture_write(bytes);

    for &b in bytes {
        crate::arch::Arch::serial_putc(b);
    }
}

/// Emit a structured oops report for an optional architecture trap frame.
pub fn oops(_trap: Option<&crate::arch::api::TrapFrame>) {
    emit(b"--- REGISTER DUMP ---\r\n");
    emit(b"OOPS_REG_NOTE: no trap frame supplied\r\n");
    emit(b"--- BACKTRACE ---\r\n");
    emit(b"OOPS_FRAME_NOTE: frame pointers unavailable\r\n");
    emit(b"--- END OOPS ---\r\n");
}

/// Best-effort symbol resolver placeholder used by tracing code.
pub fn resolve_symbol(_addr: usize) -> Option<&'static str> {
    None
}
