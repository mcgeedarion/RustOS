//! Consolidated bump allocator for kernel early-boot scenarios.
//!
//! This module provides a reusable, safe bump allocator that can be used in
//! various early-boot contexts where a simple, no-free allocator is sufficient.
//!
//! # Usage
//!
//! ```rust,no_run
//! use crate::mm::bump_allocator::BumpAllocator;
//!
//! // Define your heap size with documentation
//! const MY_HEAP_SIZE: usize = 4 * 1024 * 1024; // 4 MiB - sized for early kernel allocations
//!
//! static MY_ALLOCATOR: BumpAllocator<MY_HEAP_SIZE> = BumpAllocator::new();
//! ```
//!
//! # Safety
//!
//! The allocator uses atomic operations to ensure thread-safe allocation without
//! locks. However, it never frees memory - all allocations live until reboot.

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// A simple lock-free bump allocator that never frees memory.
///
/// This allocator is designed for early-boot scenarios where:
/// - Simplicity is more important than memory efficiency
/// - No deallocation is needed (memory lives until reboot)
/// - Thread-safe allocation is required before full locking primitives are available
///
/// The allocator uses atomic compare-and-swap to ensure multiple CPUs can
/// allocate concurrently without locks.
///
/// # Type Parameters
///
/// * `SIZE` - The size of the heap in bytes. Should be a compile-time constant.
///
/// # Examples
///
/// ```rust,no_run
/// use core::alloc::GlobalAlloc;
/// use crate::mm::bump_allocator::BumpAllocator;
///
/// const HEAP_SIZE: usize = 1024 * 1024; // 1 MiB
/// static ALLOCATOR: BumpAllocator<HEAP_SIZE> = BumpAllocator::new();
///
/// unsafe {
///     let ptr = ALLOCATOR.alloc(Layout::new::<u32>());
/// }
/// ```
pub struct BumpAllocator<const SIZE: usize> {
    /// The actual heap storage, aligned to page boundaries for efficient access
    heap: UnsafeCell<[u8; SIZE]>,
    /// Current offset into the heap (number of bytes allocated)
    next: AtomicUsize,
    /// Whether the allocator has been initialized (for lazy init scenarios)
    initialized: AtomicBool,
}

// SAFETY: BumpAllocator can be shared between threads. Individual allocations
// are made thread-safe via atomic CAS operations on `next`.
unsafe impl<const SIZE: usize> Sync for BumpAllocator<SIZE> {}

impl<const SIZE: usize> BumpAllocator<SIZE> {
    /// Create a new uninitialized bump allocator.
    ///
    /// The allocator starts with zero bytes allocated. Memory is zero-initialized
    /// if the underlying array is zero-initialized (which it should be for statics).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use crate::mm::bump_allocator::BumpAllocator;
    ///
    /// const HEAP_SIZE: usize = 64 * 1024;
    /// static ALLOCATOR: BumpAllocator<HEAP_SIZE> = BumpAllocator::new();
    /// ```
    pub const fn new() -> Self {
        Self {
            // SAFETY: Caller must ensure the static is zero-initialized
            heap: UnsafeCell::new([0u8; SIZE]),
            next: AtomicUsize::new(0),
            initialized: AtomicBool::new(false),
        }
    }

    /// Initialize the allocator cursor.
    ///
    /// This is idempotent - calling it multiple times is safe and has no effect
    /// after the first successful initialization.
    ///
    /// # When to call
    ///
    /// Call this before any allocations if you want explicit initialization.
    /// If not called, the allocator will auto-initialize on first allocation.
    pub fn init(&self) {
        self.initialized.store(true, Ordering::Release);
    }

    /// Check if the allocator has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    /// Get the number of bytes currently allocated.
    ///
    /// This is useful for debugging and monitoring heap usage.
    ///
    /// # Ordering
    ///
    /// Uses `Relaxed` ordering as this is typically used for informational
    /// purposes only (e.g., /proc/meminfo reporting).
    pub fn bytes_allocated(&self) -> usize {
        self.next.load(Ordering::Relaxed)
    }

    /// Get the number of bytes remaining in the heap.
    ///
    /// Note: Due to alignment requirements, actual usable space may be less.
    pub fn bytes_remaining(&self) -> usize {
        let allocated = self.next.load(Ordering::Relaxed);
        SIZE.saturating_sub(allocated)
    }

    /// Reset the allocator (for testing or special boot scenarios only).
    ///
    /// # Safety
    ///
    /// This is extremely dangerous and should only be called when:
    /// - No other CPU is allocating
    /// - All previously allocated pointers are no longer used
    /// - You understand this invalidates all previous allocations
    ///
    /// This is primarily intended for test scenarios or very early boot
    /// before SMP is enabled.
    pub unsafe fn reset(&self) {
        self.next.store(0, Ordering::Relaxed);
        self.initialized.store(false, Ordering::Relaxed);
    }
}

impl<const SIZE: usize> Default for BumpAllocator<SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl<const SIZE: usize> GlobalAlloc for BumpAllocator<SIZE> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Auto-initialize if needed
        if !self.initialized.load(Ordering::Acquire) {
            self.init();
        }

        let align = layout.align().max(1);
        let size = layout.size().max(1);

        // Get the base address of the heap
        let base = self.heap.get() as usize;

        loop {
            let current = self.next.load(Ordering::Relaxed);

            // Calculate aligned start position
            let start = match align_up(base + current, align) {
                Some(addr) => addr,
                None => return core::ptr::null_mut(),
            };

            // Calculate end position
            let end = match start.checked_add(size) {
                Some(end) => end,
                None => return core::ptr::null_mut(),
            };

            // Calculate new offset
            let new_next = end - base;

            // Check if we've exceeded heap size
            if new_next > SIZE {
                return core::ptr::null_mut();
            }

            // Try to claim this region with CAS
            if self
                .next
                .compare_exchange(current, new_next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return start as *mut u8;
            }

            // CAS failed, another CPU allocated - retry
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator: memory is reclaimed only by rebooting the kernel.
        // Intentionally left empty.
    }
}

/// Align a value up to the given alignment.
///
/// Returns `None` if the calculation would overflow.
///
/// # Examples
///
/// ```
/// assert_eq!(align_up(5, 4), Some(8));
/// assert_eq!(align_up(8, 4), Some(8));
/// assert_eq!(align_up(0, 4), Some(0));
/// ```
#[inline]
fn align_up(value: usize, align: usize) -> Option<usize> {
    debug_assert!(align.is_power_of_two(), "Alignment must be a power of two");
    let mask = align.checked_sub(1)?;
    value.checked_add(mask).map(|v| v & !mask)
}

// Helper import for UnsafeCell
use core::cell::UnsafeCell;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_up() {
        assert_eq!(align_up(5, 4), Some(8));
        assert_eq!(align_up(8, 4), Some(8));
        assert_eq!(align_up(0, 4), Some(0));
        assert_eq!(align_up(1, 16), Some(16));
        assert_eq!(align_up(17, 16), Some(32));
    }

    #[test]
    fn test_basic_allocation() {
        const HEAP_SIZE: usize = 1024;
        static ALLOCATOR: BumpAllocator<HEAP_SIZE> = BumpAllocator::new();

        unsafe {
            let ptr = ALLOCATOR.alloc(Layout::from_size_align(16, 8).unwrap());
            assert!(!ptr.is_null());
            assert_eq!(ptr as usize % 8, 0); // Check alignment
        }
    }

    #[test]
    fn test_initialization() {
        const HEAP_SIZE: usize = 1024;
        static ALLOCATOR: BumpAllocator<HEAP_SIZE> = BumpAllocator::new();

        assert!(!ALLOCATOR.is_initialized());
        ALLOCATOR.init();
        assert!(ALLOCATOR.is_initialized());

        // Second init should be safe
        ALLOCATOR.init();
        assert!(ALLOCATOR.is_initialized());
    }
}
