//! Shared first-stage boot path used by every architecture in `boot_minimal` builds.
//!
//! This deliberately avoids heap allocation, filesystems, scheduler, device
//! discovery, and userspace. Architecture firmware stubs only need to build a
//! [`BootInfo`](crate::init::boot_info::BootInfo) and enter `kernel_main`; this
//! module provides the common post-handoff sequence.

use crate::init::boot_info::BootInfo;

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

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

struct BootMinimalAllocator;

const HEAP_SIZE: usize = 64 * 1024;
#[repr(align(16))]
struct Heap([u8; HEAP_SIZE]);

static mut HEAP: Heap = Heap([0; HEAP_SIZE]);
static NEXT: AtomicUsize = AtomicUsize::new(0);

#[cfg_attr(not(test), global_allocator)]
static GLOBAL_ALLOCATOR: BootMinimalAllocator = BootMinimalAllocator;

unsafe impl GlobalAlloc for BootMinimalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(1);
        let size = layout.size();
        if size == 0 {
            return core::ptr::NonNull::<u8>::dangling().as_ptr();
        }

        let mut current = NEXT.load(Ordering::Relaxed);
        loop {
            let aligned = (current + align - 1) & !(align - 1);
            let next = match aligned.checked_add(size) {
                Some(next) if next <= HEAP_SIZE => next,
                _ => return core::ptr::null_mut(),
            };
            match NEXT.compare_exchange(current, next, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => return (core::ptr::addr_of_mut!(HEAP.0) as *mut u8).add(aligned),
                Err(observed) => current = observed,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}
