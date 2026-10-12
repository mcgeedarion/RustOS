//! Minimal userspace ELF entry stubs, not firmware or kernel entry points.
#[macro_export]
macro_rules! userspace_entry {
    ($entry:path) => {
        #[no_mangle]
        #[unsafe(naked)]
        unsafe extern "C" fn _start() -> ! {
            #[cfg(target_arch = "x86_64")]
            core::arch::naked_asm!(
                "and rsp, -16", "xor rbp, rbp", "call {entry}", "ud2",
                entry = sym $entry,
            );
            #[cfg(target_arch = "aarch64")]
            core::arch::naked_asm!(
                "mov x29, xzr", "mov x9, sp",
                "and x9, x9, #0xfffffffffffffff0", "mov sp, x9",
                "bl {entry}", "brk #0", entry = sym $entry,
            );
            #[cfg(target_arch = "riscv64")]
            core::arch::naked_asm!(
                "andi sp, sp, -16", "mv s0, zero", "call {entry}", "unimp",
                entry = sym $entry,
            );
        }
    };
}
