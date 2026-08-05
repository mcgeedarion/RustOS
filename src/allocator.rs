//! Full-kernel fallback allocator wiring.
//!
//! The long-term heap implementation lives under `mm::heap`, but full-kernel
//! diagnostic builds still need a crate-level `#[global_allocator]` and the
//! historical `crate::allocator::heap_init()` entry point.  This bump allocator
//! is intentionally simple: it makes early allocations possible before the VM
//! heap is fully wired, and it never frees.

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const HEAP_SIZE: usize = 4 * 1024 * 1024;

#[repr(align(4096))]
struct Heap([u8; HEAP_SIZE]);

static mut HEAP: Heap = Heap([0; HEAP_SIZE]);
static NEXT: AtomicUsize = AtomicUsize::new(0);
static INITIALISED: AtomicBool = AtomicBool::new(false);

pub struct KernelBumpAllocator;

#[cfg_attr(not(test), global_allocator)]
static GLOBAL_ALLOCATOR: KernelBumpAllocator = KernelBumpAllocator;

/// Initialise the fallback heap cursor.
pub fn heap_init() {
    INITIALISED.store(true, Ordering::Release);
}

#[inline]
fn align_up(value: usize, align: usize) -> Option<usize> {
    let mask = align.checked_sub(1)?;
    value.checked_add(mask).map(|v| v & !mask)
}

unsafe impl GlobalAlloc for KernelBumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !INITIALISED.load(Ordering::Acquire) {
            heap_init();
        }

        let align = layout.align().max(1);
        let size = layout.size().max(1);
        let base = HEAP.0.as_ptr() as usize;

        loop {
            let current = NEXT.load(Ordering::Relaxed);
            let start = match align_up(base + current, align) {
                Some(addr) => addr,
                None => return core::ptr::null_mut(),
            };
            let end = match start.checked_add(size) {
                Some(end) => end,
                None => return core::ptr::null_mut(),
            };
            let new_next = end - base;
            if new_next > HEAP_SIZE {
                return core::ptr::null_mut();
            }
            if NEXT
                .compare_exchange(current, new_next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return start as *mut u8;
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator: memory is reclaimed only by rebooting the kernel.
    }
}
