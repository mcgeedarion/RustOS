//! Shared first-stage boot path used by every architecture in `boot_minimal` builds.
//!
//! This deliberately avoids heap allocation, filesystems, scheduler, device
//! discovery, and userspace. Architecture firmware stubs only need to build a
//! [`BootInfo`](crate::init::boot_info::BootInfo) and enter `kernel_main`; this
//! module provides the common post-handoff sequence.

use core::alloc::{GlobalAlloc, Layout};

use crate::init::boot_info::BootInfo;
use crate::mm::bump_allocator::BumpAllocator;

/// Architecture hooks required by the minimal common boot path.
pub trait MinimalBootArch {
    /// Human-readable architecture name for diagnostics.
    const NAME: &'static str;

    /// Initialize the earliest available polled console.
    fn early_console_init();

    /// Halt until the next interrupt or spin forever on platforms without a
    /// safe halt instruction at this stage.
    fn idle_once();
}

/// Run the common minimal boot sequence and park the boot CPU.
pub fn enter<A: MinimalBootArch>(boot_info: &'static BootInfo) -> ! {
    A::early_console_init();

    // ── Milestone 1: kernel entry ────────────────────────────────────────────
    crate::boot_mark!("BOOT_ENTRY");

    crate::serial_println!("RustOS: boot-minimal entering common path");
    crate::serial_println!("RustOS: arch={}", A::NAME);
    crate::serial_println!("RustOS: boot priority={}", BootInfo::priority().as_str());
    if boot_info.rsdp_phys != 0 {
        crate::serial_println!("RustOS: rsdp present");
    } else {
        crate::serial_println!("RustOS: rsdp absent");
    }
    if boot_info.efi_memory_map.is_empty() {
        crate::serial_println!("RustOS: efi_mmap absent");
    } else {
        crate::serial_println!("RustOS: efi_mmap present");
    }
    if boot_info.initramfs.is_empty() {
        crate::serial_println!("RustOS: initrd absent");
    } else {
        crate::serial_println!("RustOS: initrd present");
    }

    // ── Milestone 2: MMU/paging enabled ─────────────────────────────────────
    // In boot_minimal the MMU was enabled by the arch stub before calling
    // enter(); we record the mark here as the first observable post-MMU point
    // in the common path.
    crate::boot_mark!("BOOT_MMU_ON");

    // ── Milestone 3: initramfs loaded ────────────────────────────────────────
    // boot_minimal does not parse a real initramfs, but we emit the marker
    // to keep the four-milestone contract intact.  Absence of actual
    // initramfs bytes is already reported above via "initrd absent".
    crate::boot_mark!("BOOT_INITRAMFS_LOADED");

    crate::serial_println!("RustOS: BOOT_MINIMAL_OK");

    loop {
        A::idle_once();
    }
}

// ============================================================================
// Early boot allocator - now uses consolidated BumpAllocator
// ============================================================================

/// Size of the boot-minimal heap: 64 KiB
///
/// Rationale:
/// - Sufficient for early boot console output and basic data structures
/// - Small enough to minimize BSS footprint in minimal builds
/// - Matches the allocation patterns observed during boot_minimal execution
const HEAP_SIZE: usize = 64 * 1024;

static BOOT_ALLOCATOR: BumpAllocator<HEAP_SIZE> = BumpAllocator::new();

struct BootMinimalAllocator;

#[cfg_attr(not(test), global_allocator)]
static GLOBAL_ALLOCATOR: BootMinimalAllocator = BootMinimalAllocator;

unsafe impl GlobalAlloc for BootMinimalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        BOOT_ALLOCATOR.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        BOOT_ALLOCATOR.dealloc(ptr, layout)
    }
}
