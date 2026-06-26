//! Kernel console (writes to serial + VGA if available).
pub fn print(s: &str) {
    #[cfg(target_arch = "x86_64")]
    crate::arch::x86_64::serial::serial_print(s);
    #[cfg(target_arch = "aarch64")]
    crate::arch::aarch64::serial::write_bytes(s.as_bytes());
}
