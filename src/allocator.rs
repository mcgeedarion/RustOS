//! Full-kernel fallback allocator wiring.
//!
//! The long-term heap implementation lives under `mm::heap`, but full-kernel
//! diagnostic builds still need a crate-level `#[global_allocator]` and the
//! historical `crate::allocator::heap_init()` entry point.  This module now
//! uses the consolidated `BumpAllocator` from `mm::bump_allocator` for cleaner
//! code reuse.
//!
//! ## Heap Size Rationale
//!
//! The 4 MiB heap size is chosen to:
//! - Accommodate early kernel allocations before the VM heap is fully wired
//! - Provide sufficient space for diagnostic output and error handling
//! - Remain small enough to fit in BSS without bloating the kernel image
//! - Match typical early-boot allocation patterns observed in development

use crate::mm::bump_allocator::BumpAllocator;
use core::alloc::{GlobalAlloc, Layout};

/// Size of the fallback heap: 4 MiB
/// See module-level docs for sizing rationale
const HEAP_SIZE: usize = 4 * 1024 * 1024;

static HEAP: BumpAllocator<HEAP_SIZE> = BumpAllocator::new();

/// Initialise the fallback heap cursor.
///
/// This is idempotent - safe to call multiple times.
pub fn heap_init() {
    HEAP.init();
}

// Re-export the bump allocator's GlobalAlloc implementation
unsafe impl GlobalAlloc for crate::allocator::KernelBumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        HEAP.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        HEAP.dealloc(ptr, layout)
    }
}

pub struct KernelBumpAllocator;

#[cfg_attr(not(test), global_allocator)]
static GLOBAL_ALLOCATOR: KernelBumpAllocator = KernelBumpAllocator;
