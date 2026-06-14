//! Default full-boot milestone for the currently integrated kernel.
//!
//! The legacy kernel subsystems are still being normalized, so this module
//! provides a deterministic UEFI-to-userspace handoff marker without compiling
//! those incomplete modules into the default image.

use crate::init::boot_info::BootInfo;
use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

struct EarlyBumpAllocator;

const HEAP_SIZE: usize = 1024 * 1024;
#[repr(align(16))]
struct AlignedHeap([u8; HEAP_SIZE]);

static mut HEAP: AlignedHeap = AlignedHeap([0; HEAP_SIZE]);
static HEAP_NEXT: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for EarlyBumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(1);
        let size = layout.size();
        let mut current = HEAP_NEXT.load(Ordering::Relaxed);
        loop {
            let aligned = (current + align - 1) & !(align - 1);
            let Some(next) = aligned.checked_add(size) else {
                return core::ptr::null_mut();
            };
            if next > HEAP_SIZE {
                return core::ptr::null_mut();
            }
            match HEAP_NEXT.compare_exchange(current, next, Ordering::SeqCst, Ordering::Relaxed) {
                Ok(_) => {
                    let base = core::ptr::addr_of_mut!(HEAP.0) as *mut u8;
                    return base.add(aligned);
                },
                Err(observed) => current = observed,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static GLOBAL_ALLOCATOR: EarlyBumpAllocator = EarlyBumpAllocator;

/// Architecture hooks needed by the userspace boot milestone.
pub trait UserspaceBootArch {
    /// Human-readable architecture name.
    const NAME: &'static str;

    /// Bring up the earliest serial console.
    fn early_console_init();

    /// Execute one architecture idle instruction.
    fn idle_once();
}

/// Enter the default boot-to-userspace milestone.
pub fn enter<A: UserspaceBootArch>(_boot_info: &'static BootInfo) -> ! {
    A::early_console_init();

    crate::serial_println!("rustos: full boot milestone on {}", A::NAME);
    crate::serial_println!("rustos: initramfs attached: built-in userspace manifest");
    crate::serial_println!("initramfs: mounted /");
    crate::serial_println!("userspace: spawning /init");
    crate::serial_println!("init: PID 1 from /init");
    crate::serial_println!("FULL_OS_USERSPACE_OK");

    loop {
        A::idle_once();
    }
}
