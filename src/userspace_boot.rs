//! Init-image diagnostics for the transitional `userspace_boot` profile.
//!
//! The architecture stub supplies firmware mappings and an early console, not
//! the full MM, VFS, or scheduler. This path locates `/init` using the real
//! zero-copy CPIO parser and validates its ELF metadata. It then reports that
//! execution is unavailable; it must not emit PID-created or user-executed
//! success markers until an actual process execution backend is integrated.

use core::alloc::{GlobalAlloc, Layout};

use crate::init::boot_info::BootInfo;
use crate::mm::bump_allocator::BumpAllocator;

const HEAP_SIZE: usize = 1024 * 1024;
static USERSPACE_ALLOCATOR: BumpAllocator<HEAP_SIZE> = BumpAllocator::new();

struct EarlyBumpAllocator;

#[cfg_attr(not(test), global_allocator)]
static GLOBAL_ALLOCATOR: EarlyBumpAllocator = EarlyBumpAllocator;

unsafe impl GlobalAlloc for EarlyBumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        USERSPACE_ALLOCATOR.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        USERSPACE_ALLOCATOR.dealloc(ptr, layout)
    }
}

/// Hooks supplied by the slim architecture boot stub.
pub trait UserspaceBootArch {
    const NAME: &'static str;
    fn early_console_init();
    fn idle_once();
}

/// Inspect the init image without claiming unavailable VFS/process services.
pub fn enter<A: UserspaceBootArch>(boot_info: &'static BootInfo) -> ! {
    A::early_console_init();
    let _ = boot_info;
    crate::serial_println!("rustos: userspace_boot diagnostics - arch={}", A::NAME);

    // load() assumes a registered range and otherwise parks the CPU. Check
    // first so a missing archive produces an actionable failure instead.
    if !crate::fs::has_initramfs_range() {
        panic!(
            "kernel: no initramfs registered; build it with cargo xtask build-init \
             and boot with --features userspace_boot --initrd"
        );
    }
    let ram = crate::fs::initramfs::load();
    let init_elf = ram.file("/init").unwrap_or_else(|| {
        panic!(
            "kernel: /init not found in the initramfs; rebuild with \
             cargo xtask build-init --arch <arch> and boot with --initrd"
        )
    });
    crate::serial_println!("initramfs: found /init ({} bytes)", init_elf.len());
    crate::boot_mark!("BOOT_INITRAMFS_LOADED");

    match crate::proc::exec::validate_executable_elf(init_elf) {
        Ok(has_interpreter) => {
            crate::serial_println!(
                "userspace_boot: ELF metadata valid has_interpreter={}; execution backend intentionally excluded",
                has_interpreter
            );
        }
        Err(error) => {
            panic!("kernel: invalid /init ELF metadata: {}", error);
        }
    }

    // This profile deliberately excludes the full MM/VFS/process execution
    // graph. Never call the full process loader or emit PID/execution success
    // markers from this diagnostic-only path.
    crate::serial_println!("USERSPACE_BOOT_UNSUPPORTED");

    loop {
        A::idle_once();
    }
}
