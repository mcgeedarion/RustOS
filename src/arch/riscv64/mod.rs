#[cfg(any(
    not(feature = "boot_minimal"),
    all(feature = "boot_minimal", not(feature = "uefi_boot"))
))]
pub mod boot;
#[cfg(not(feature = "boot_minimal"))]
pub mod csr;
#[cfg(not(feature = "boot_minimal"))]
pub mod fdt;
pub mod hal;
pub mod mem_layout;
#[cfg(not(feature = "boot_minimal"))]
pub mod memory;
#[cfg(not(feature = "boot_minimal"))]
pub mod paging;
#[cfg(not(feature = "boot_minimal"))]
pub mod smp;
#[cfg(not(feature = "boot_minimal"))]
pub mod syscall;
#[cfg(not(feature = "boot_minimal"))]
pub mod trampoline;
#[cfg(not(feature = "boot_minimal"))]
pub mod trap;
#[cfg(any(
    all(feature = "boot_minimal", not(feature = "uefi_boot")),
    all(feature = "uefi_boot", feature = "riscv64_uefi_boot"),
    all(not(feature = "boot_minimal"), not(feature = "uefi_boot"))
))]
pub mod uefi_entry;
#[cfg(not(feature = "boot_minimal"))]
pub mod uentry;

// Canonical home for the RISC-V PLIC driver is `crate::irq::riscv64::plic`.
// Re-export it here so call sites that use the `arch::riscv64::plic` path
// (FDT walker, kernel init, etc.) keep working without a flag day.
#[cfg(not(feature = "boot_minimal"))]
pub use crate::irq::riscv64::plic;

/// RISC-V early/kernel boot hook used by the common entry point.
#[cfg(feature = "boot_minimal")]
pub fn init(boot_info: &'static crate::init::boot_info::BootInfo) -> ! {
    crate::boot_minimal::enter::<MinimalArch>(boot_info)
}

#[cfg(feature = "boot_minimal")]
struct MinimalArch;

#[cfg(feature = "boot_minimal")]
impl crate::boot_minimal::MinimalBootArch for MinimalArch {
    const NAME: &'static str = "riscv64";

    fn early_console_init() {}

    fn idle_once() {
        unsafe {
            core::arch::asm!("wfi", options(nostack, nomem));
        }
    }
}

/// RISC-V early/kernel boot hook used by the common entry point.
#[cfg(not(feature = "boot_minimal"))]
pub fn init(boot_info: &'static crate::init::boot_info::BootInfo) -> ! {
    use crate::arch::riscv64::{fdt, trap};
    use crate::irq::riscv64::plic;

    let fdt_ptr = boot_info.fdt.start;
    trap::trap_init();
    if fdt_ptr != 0 {
        unsafe {
            fdt::fdt_phase1(fdt_ptr);
        }
    }

    let regions = crate::arch::riscv64::memory::discover(fdt_ptr);
    unsafe {
        crate::mm::pmm::init_from_regions(&regions);
    }

    plic::init();
    crate::heap::init();
    crate::mm::init();
    crate::security::init();

    #[cfg(feature = "gdbstub")]
    {
        static mut GDBSTUB_SERIAL: crate::debug::gdbstub::serial::SerialPort =
            unsafe { crate::debug::gdbstub::serial::SerialPort::new() };
        unsafe {
            crate::debug::gdbstub::session::init(&mut GDBSTUB_SERIAL);
        }
    }

    crate::display::framebuffer::init();
    crate::display::drm::init();
    crate::display::wayland::init();

    if fdt_ptr != 0 {
        unsafe {
            fdt::fdt_phase2(fdt_ptr);
        }
    }
    crate::block::virtio_blk::init();

    crate::drivers::keyboard::init();
    crate::drivers::mouse::init();
    crate::drivers::evdev::init();
    crate::input::init();

    crate::init::initramfs::mount();
    crate::namespace::init();

    crate::time::init();
    crate::drivers::nic::init();
    crate::init::schemes::init();
    crate::dhcp::init();

    crate::proc::cgroup::init();
    crate::shell::init();

    #[cfg(feature = "kmtest")]
    crate::kmtest::init();

    crate::proc::spawn_init();

    unreachable!("scheduler returned to kernel_main");
}
