//! Intrusive, address-sorted free regions. No allocation or platform calls.
use core::alloc::Layout;
use core::mem::{align_of, size_of};
use core::ptr::null_mut;

#[repr(C)]
struct Region {
    size: usize,
    next: *mut Region,
}

pub struct RegionHeap {
    head: *mut Region,
}

// All mutation requires exclusive access, provided by the caller's heap lock.
unsafe impl Send for RegionHeap {}

impl RegionHeap {
    pub const fn new() -> Self {
        Self { head: null_mut() }
    }

    fn align_up(value: usize, align: usize) -> Option<usize> {
        value.checked_add(align - 1).map(|v| v & !(align - 1))
    }

    fn allocation_layout(layout: Layout) -> Option<(usize, usize)> {
        let align = layout.align().max(align_of::<Region>());
        let size = Self::align_up(layout.size().max(size_of::<Region>()), align_of::<Region>())?;
        Some((size, align))
    }

    /// # Safety
    /// The range must be writable, exclusively owned, and disjoint from all
    /// live allocations and free regions. It remains owned by this heap.
    pub unsafe fn add_free_region(&mut self, start: usize, size: usize) {
        let Some(end) = start.checked_add(size) else { return };
        let Some(start) = Self::align_up(start, align_of::<Region>()) else { return };
        let end = end & !(align_of::<Region>() - 1);
        if end <= start || end - start < size_of::<Region>() {
            return;
        }
        let mut link = &mut self.head as *mut *mut Region;
        let mut previous: *mut Region = null_mut();
        while !(*link).is_null() && (*link as usize) < start {
            previous = *link;
            link = &mut (**link).next;
        }
        let next = *link;
        if !previous.is_null() {
            assert!((previous as usize) + (*previous).size <= start, "overlapping heap free");
        }
        if !next.is_null() {
            assert!(end <= next as usize, "overlapping heap free");
        }
        let node = start as *mut Region;
        node.write(Region { size: end - start, next });
        *link = node;
        if !next.is_null() && end == next as usize {
            (*node).size += (*next).size;
            (*node).next = (*next).next;
        }
        if !previous.is_null() && previous as usize + (*previous).size == start {
            (*previous).size += (*node).size;
            (*previous).next = (*node).next;
        }
    }

    /// # Safety
    /// Heap regions must satisfy add_free_region's ownership contract.
    pub unsafe fn allocate(&mut self, layout: Layout) -> *mut u8 {
        let Some((size, align)) = Self::allocation_layout(layout) else { return null_mut() };
        let mut link = &mut self.head as *mut *mut Region;
        while !(*link).is_null() {
            let node = *link;
            let base = node as usize;
            let end = base + (*node).size;
            let Some(mut start) = Self::align_up(base, align) else { return null_mut() };
            if start != base && start - base < size_of::<Region>() {
                let Some(after_header) = base.checked_add(size_of::<Region>()) else { return null_mut() };
                let Some(aligned) = Self::align_up(after_header, align) else { return null_mut() };
                start = aligned;
            }
            if let Some(allocation_end) = start.checked_add(size) {
                if allocation_end <= end {
                    let suffix = end - allocation_end;
                    if suffix == 0 || suffix >= size_of::<Region>() {
                        *link = (*node).next;
                        if start > base {
                            self.add_free_region(base, start - base);
                        }
                        if suffix > 0 {
                            self.add_free_region(allocation_end, suffix);
                        }
                        return start as *mut u8;
                    }
                }
            }
            link = &mut (*node).next;
        }
        null_mut()
    }

    /// # Safety
    /// ptr must be a live allocation from this heap with exactly this layout.
    pub unsafe fn deallocate(&mut self, ptr: *mut u8, layout: Layout) {
        if let Some((size, _)) = Self::allocation_layout(layout) {
            self.add_free_region(ptr as usize, size);
        }
    }

    pub fn free_bytes(&self) -> usize {
        let mut total = 0;
        let mut node = self.head;
        while !node.is_null() {
            unsafe {
                total += (*node).size;
                node = (*node).next;
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[repr(align(4096))]
    struct Arena([u8; 65536]);

    #[test]
    fn reclaims_and_coalesces_in_both_directions() {
        let mut arena = Arena([0; 65536]);
        let mut heap = RegionHeap::new();
        unsafe {
            heap.add_free_region(arena.0.as_mut_ptr() as usize, arena.0.len());
            let layout = Layout::from_size_align(1024, 16).unwrap();
            let a = heap.allocate(layout);
            let b = heap.allocate(layout);
            let c = heap.allocate(layout);
            assert!(!a.is_null() && !b.is_null() && !c.is_null());
            assert_ne!(a, b);
            heap.deallocate(b, layout);
            heap.deallocate(a, layout);
            heap.deallocate(c, layout);
            assert_eq!(heap.free_bytes(), 65536);
            let all = Layout::from_size_align(65536, 4096).unwrap();
            let p = heap.allocate(all);
            assert_eq!(p, arena.0.as_mut_ptr());
            assert_eq!(heap.free_bytes(), 0);
            heap.deallocate(p, all);
        }
    }

    #[test]
    fn supports_small_allocations_and_large_alignment() {
        let mut arena = Arena([0; 65536]);
        let mut heap = RegionHeap::new();
        unsafe {
            heap.add_free_region(arena.0.as_mut_ptr() as usize, arena.0.len());
            let small = Layout::from_size_align(1, 1).unwrap();
            let a = heap.allocate(small);
            let aligned = Layout::from_size_align(123, 4096).unwrap();
            let b = heap.allocate(aligned);
            assert!(!a.is_null() && !b.is_null());
            assert_eq!(b as usize % 4096, 0);
            assert_ne!(a, b);
            heap.deallocate(a, small);
            heap.deallocate(b, aligned);
            assert_eq!(heap.free_bytes(), 65536);
            assert!(heap.allocate(Layout::from_size_align(131072, 8).unwrap()).is_null());
            assert_eq!(heap.free_bytes(), 65536);
        }
    }

    #[test]
    fn threaded_allocations_do_not_overlap_and_restore_accounting() {
        use std::collections::BTreeMap;
        use std::sync::{Arc, Mutex};
        let mut arena = Box::new(Arena([0; 65536]));
        let mut heap = RegionHeap::new();
        unsafe { heap.add_free_region(arena.0.as_mut_ptr() as usize, arena.0.len()) };
        let state = Arc::new(Mutex::new((heap, BTreeMap::<usize, usize>::new())));
        std::thread::scope(|scope| {
            for worker in 0..8 {
                let state = state.clone();
                scope.spawn(move || {
                    for iteration in 0..2000 {
                        let layout = Layout::from_size_align(16 + (iteration + worker) % 512, 16).unwrap();
                        let address = {
                            let mut state = state.lock().unwrap();
                            let p = unsafe { state.0.allocate(layout) };
                            assert!(!p.is_null());
                            let start = p as usize;
                            let end = start + layout.size();
                            assert!(state.1.iter().all(|(&a, &b)| end <= a || start >= b));
                            assert!(state.1.insert(start, end).is_none());
                            start
                        };
                        unsafe { core::ptr::write_bytes(address as *mut u8, 0xA5, layout.size()) };
                        std::thread::yield_now();
                        let mut state = state.lock().unwrap();
                        assert!(state.1.remove(&address).is_some());
                        unsafe { state.0.deallocate(address as *mut u8, layout) };
                    }
                });
            }
        });
        let state = state.lock().unwrap();
        assert!(state.1.is_empty());
        assert_eq!(state.0.free_bytes(), 65536);
    }
}
