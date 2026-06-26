//! Boot memory map consumer — Phase 2.
//!
//! Reads the memory map provided by the bootloader and feeds usable
//! ranges to pmm_add_region().  Called once during kernel_main, after
//! heap_init() has bootstrapped the slab allocator.

use crate::mm::pmm;

/// Translate a physical address to a virtual address using the
/// architecture's higher-half mapping.
#[cfg(target_arch = "x86_64")]
#[inline]
fn phys_to_virt(pa: u64) -> usize {
    crate::arch::x86_64::mem_layout::higher_half::phys_to_virt(pa)
}

#[cfg(target_arch = "aarch64")]
#[inline]
fn phys_to_virt(pa: u64) -> usize {
    crate::arch::aarch64::mem_layout::phys_to_virt(pa)
}

/// Walk the bootloader memory map and register all usable RAM regions
/// with the physical memory manager.
pub fn init(map: &crate::boot::MemoryMap) {
    for entry in map.entries() {
        if entry.is_usable() {
            let start = phys_to_virt(entry.base) as *mut u8;
            let len   = entry.length as usize;
            unsafe { pmm::pmm_add_region(start, len); }
        }
    }
}
