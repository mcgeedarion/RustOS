//! Kernel console (writes to serial + VGA if available).
pub fn print(s: &str) {
    #[cfg(target_arch = "x86_64")]
    crate::arch::x86_64::serial::serial_print(s);
    #[cfg(target_arch = "aarch64")]
    crate::arch::aarch64::serial::write_bytes(s.as_bytes());
}

pub fn println(s: &str) {
    print(s);
    print("\n");
}

pub fn print_fmt(args: core::fmt::Arguments) {
    use core::fmt::Write;
    struct W;
    impl core::fmt::Write for W {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            print(s);
            Ok(())
        }
    }
    let _ = W.write_fmt(args);
}
