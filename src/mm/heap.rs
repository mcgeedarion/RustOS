//! Heap bootstrap and explicit PMM-backed growth using the canonical allocator.
//! Growth does not run from GlobalAlloc: early allocations must not recurse
//! into an uninitialized PMM. Callers may grow explicitly after PMM setup.
use core::alloc::Layout;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::allocator::{InterruptGuard, ALLOCATOR};
use crate::mm::pmm::PAGE_SIZE;

const MAX_GROW_PAGES: usize = 512;
static HEAP_BYTES: AtomicUsize = AtomicUsize::new(0);
static HEAP_PAGES: AtomicUsize = AtomicUsize::new(0);
static GROW_LOCK: spin::Mutex<()> = spin::Mutex::new(());

pub fn heap_init() {
    crate::allocator::heap_init();
}

pub fn init_heap_tracking(_heap_virt_start: usize, initial_pages: usize) {
    HEAP_PAGES.store(initial_pages, Ordering::Relaxed);
    HEAP_BYTES.store(initial_pages * PAGE_SIZE, Ordering::Relaxed);
}

/// Add up to MAX_GROW_PAGES independently allocated PMM pages to the heap.
/// Returns the VA of the first page, not a promise of contiguous allocation.
pub fn grow(pages: usize) -> Option<usize> {
    if pages == 0 || pages > MAX_GROW_PAGES {
        return None;
    }
    let _interrupts = InterruptGuard::new();
    heap_init();
    let _grow = GROW_LOCK.lock();
    let mut frames = [0usize; MAX_GROW_PAGES];
    for i in 0..pages {
        match crate::mm::pmm::alloc_page() {
            Some(pa) => frames[i] = pa,
            None => {
                for &pa in &frames[..i] {
                    crate::mm::pmm::free_page(pa);
                }
                return None;
            }
        }
    }
    let first = crate::mm::phys::phys_to_virt(frames[0]);
    {
        let mut heap = ALLOCATOR.lock();
        for &pa in &frames[..pages] {
            let va = crate::mm::phys::phys_to_virt(pa);
            // Fresh frames are exclusively owned and mapped through physmap.
            unsafe { heap.add_free_region(va, PAGE_SIZE) };
        }
    }
    HEAP_PAGES.fetch_add(pages, Ordering::Relaxed);
    HEAP_BYTES.fetch_add(pages * PAGE_SIZE, Ordering::Relaxed);
    Some(first)
}

pub fn committed_pages() -> usize {
    HEAP_PAGES.load(Ordering::Relaxed)
}

pub fn committed_bytes() -> usize {
    HEAP_BYTES.load(Ordering::Relaxed)
}

/// Explicit post-PMM growth helper. Success means capacity was added; a
/// caller must retry allocation, which can still fail due to fragmentation.
pub fn handle_oom(layout: Layout) -> bool {
    let needed = layout.size().div_ceil(PAGE_SIZE).max(1);
    if needed > MAX_GROW_PAGES {
        return false;
    }
    let batch = needed.max(256);
    if grow(batch).is_some() {
        return true;
    }
    needed < batch && grow(needed).is_some()
}
