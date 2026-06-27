//! Stub binary entry point.
//!
//! UEFI builds boot through `rustos-kernel`'s own entry points
//! (`arch::x86_64::uefi_entry`, `arch::aarch64::uefi_entry`) and never link
//! this binary. The `[[bin]]` target still has to compile, though, so this
//! is kept intentionally minimal: no `#[panic_handler]` here, since
//! `rustos_kernel::kernel::panic` already provides one unconditionally and a
//! second definition would be a duplicate-handler build error.

#![no_std]
#![no_main]

extern crate rustos_kernel;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    loop {
        core::hint::spin_loop();
    }
}
