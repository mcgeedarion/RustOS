//! Full-kernel reclaiming heap. Slim boot profiles retain their bump allocator.
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;

#[path = "allocator/region_heap.rs"]
pub mod region_heap;
use region_heap::RegionHeap;

pub static ALLOCATOR: spin::Mutex<RegionHeap> = spin::Mutex::new(RegionHeap::new());
const BOOT_HEAP_SIZE: usize = 4 * 1024 * 1024;
#[repr(C, align(4096))]
struct BootHeap(UnsafeCell<[u8; BOOT_HEAP_SIZE]>);
// Access is transferred once to ALLOCATOR under INITIALIZED's synchronization.
unsafe impl Sync for BootHeap {}
static BOOT_HEAP: BootHeap = BootHeap(UnsafeCell::new([0; BOOT_HEAP_SIZE]));
static INITIALIZED: spin::Once<()> = spin::Once::new();

pub(crate) struct InterruptGuard(bool);
impl InterruptGuard {
    pub(crate) fn new() -> Self {
        #[cfg(not(test))]
        {
            use crate::arch::{api::Interrupts, Arch};
            let enabled = <Arch as Interrupts>::are_enabled();
            <Arch as Interrupts>::disable();
            Self(enabled)
        }
        #[cfg(test)]
        { Self(false) }
    }
}
impl Drop for InterruptGuard {
    fn drop(&mut self) {
        #[cfg(not(test))]
        if self.0 {
            use crate::arch::{api::Interrupts, Arch};
            <Arch as Interrupts>::enable();
        }
    }
}

pub fn heap_init() {
    let _interrupts = InterruptGuard::new();
    INITIALIZED.call_once(|| {
        let start = BOOT_HEAP.0.get() as *mut u8 as usize;
        unsafe { ALLOCATOR.lock().add_free_region(start, BOOT_HEAP_SIZE) };
        crate::mm::heap::init_heap_tracking(start, BOOT_HEAP_SIZE / crate::mm::pmm::PAGE_SIZE);
    });
}

pub struct KernelAllocator;
#[cfg_attr(not(test), global_allocator)]
static GLOBAL_ALLOCATOR: KernelAllocator = KernelAllocator;

unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _interrupts = InterruptGuard::new();
        heap_init();
        ALLOCATOR.lock().allocate(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _interrupts = InterruptGuard::new();
        ALLOCATOR.lock().deallocate(ptr, layout);
    }
}
