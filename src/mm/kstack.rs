//! Kernel-stack allocation records and conservative deferred reclamation.
//!
//! The registry owns every allocation, including CPU-lifetime GDT/IST stacks.
//! Dropping a KstackInfo does not release it. Legacy free_kstack(top) only
//! retires a record: it never unmaps a potentially active stack. Reclamation
//! requires an explicit, externally established all-CPU quiescence guarantee.
//! Until that scheduler protocol exists, retired allocations retain memory.

use crate::arch::{api::{PageFlags, Paging}, Arch};
use crate::allocator::InterruptGuard;
use crate::mm::pmm::{self, PAGE_SIZE};

#[path = "kstack/ownership.rs"]
mod ownership;
use ownership::{Registry, Token};

const STACK_PAGES: usize = 3;
const MAX_STACKS: usize = 4096;
struct Backing { base_pa: usize, base_va: usize, top: usize }
static STACKS: spin::Mutex<Registry<Backing, MAX_STACKS>> =
    spin::Mutex::new(Registry::new());

/// Non-cloneable retirement capability. Allocation metadata stays owned by
/// STACKS even when this handle leaves scope (as GDT/IST setup currently does).
#[must_use]
pub struct KstackInfo {
    /// Compatibility address for GDT users. This address alone is not ownership.
    pub top: usize,
    token: Token,
}
impl KstackInfo {
    pub fn stack_top(&self) -> usize { self.top }
    /// Retire this allocation without unmapping or freeing its frames.
    pub fn retire(self) -> bool {
        let _interrupts = InterruptGuard::new();
        STACKS.lock().retire(self.token)
    }
}

fn writable_flags() -> PageFlags {
    PageFlags::PRESENT | PageFlags::WRITE | PageFlags::NX
}

fn restore_direct_map(backing: &Backing) -> bool {
    let root = <Arch as Paging>::kernel_cr3();
    for page in 0..STACK_PAGES {
        let va = backing.base_va + page * PAGE_SIZE;
        let pa = backing.base_pa + page * PAGE_SIZE;
        if !<Arch as Paging>::map_page(root, va, pa, writable_flags()) {
            return false;
        }
        <Arch as Paging>::flush_va(va);
    }
    true
}

/// Allocate a guard frame followed by two contiguous writable stack frames.
///
/// Physmap VAs preserve physical offsets, so three independently allocated
/// frames cannot be used to construct one contiguous stack. A contiguous PMM
/// request must succeed; no fragmented fallback is advertised as a stack.
pub fn alloc_kstack() -> Option<KstackInfo> {
    let _interrupts = InterruptGuard::new();
    let token = STACKS.lock().reserve()?;
    let Some(base_pa) = pmm::alloc_pages_contig(STACK_PAGES) else {
        STACKS.lock().cancel(token);
        return None;
    };
    let base_va = crate::mm::phys::phys_to_virt(base_pa);
    let backing = Backing {
        base_pa, base_va, top: base_va + STACK_PAGES * PAGE_SIZE,
    };
    let root = <Arch as Paging>::kernel_cr3();
    let mut mapped = true;
    for page in 0..STACK_PAGES {
        let flags = if page == 0 { PageFlags::empty() } else { writable_flags() };
        if !<Arch as Paging>::map_page(
            root, base_va + page * PAGE_SIZE, base_pa + page * PAGE_SIZE, flags,
        ) {
            mapped = false;
            break;
        }
        <Arch as Paging>::flush_va(base_va + page * PAGE_SIZE);
    }
    if !mapped {
        if restore_direct_map(&backing) {
            pmm::free_pages_contig(base_pa, STACK_PAGES);
            STACKS.lock().cancel(token);
        } else {
            // Keep ownership on rollback failure. Never return frames to the
            // PMM while their direct-map mappings may still be unusable.
            let mut stacks = STACKS.lock();
            assert!(stacks.install(token, backing).is_ok());
            stacks.retire(token);
        }
        return None;
    }
    let top = backing.top;
    assert!(STACKS.lock().install(token, backing).is_ok());
    Some(KstackInfo { top, token })
}

/// Legacy address-based retirement entry used by exit::zombify.
///
/// Despite the historical name, this function does not release memory. Unknown
/// addresses and repeated retirement requests are ignored. New code should
/// retain KstackInfo and consume its retire() method instead of copying top.
pub fn free_kstack(top: usize) {
    if top == 0 { return; }
    let _interrupts = InterruptGuard::new();
    STACKS.lock().retire_matching(|backing| backing.top == top);
}

/// Reclaim retired allocations after external scheduler quiescence.
/// No automatic caller is installed: premature reclamation is worse than
/// retaining retired frames. Returns the number of allocations reclaimed.
///
/// # Safety
/// All other CPUs must be stopped and remain stopped until this function
/// returns and the caller completes any required cross-CPU TLB invalidation.
/// The caller must exclude stack allocation/retirement and all frame reuse
/// until that invalidation is complete. No CPU may be executing on, hold a
/// saved continuation into, or subsequently re-enter any retired stack.
/// TSS/IST/GS selectors, interrupt stacks, and runqueues must no longer target
/// retired stacks. The calling CPU must itself use a non-retired stack, and
/// the backing addresses must remain valid in physmap.
pub unsafe fn reclaim_retired_on_stopped_machine() -> usize {
    let _interrupts = InterruptGuard::new();
    let mut count = 0;
    loop {
        let backing = {
            let mut stacks = STACKS.lock();
            let Some((token, backing)) = stacks.first_retired() else { break };
            if !restore_direct_map(backing) { break; }
            // Other CPUs remain stopped through restoration and the caller's
            // required TLB invalidation before any freed frame can be reused.
            stacks.take_retired(token).expect("retired stack lost ownership")
        };
        pmm::free_pages_contig(backing.base_pa, STACK_PAGES);
        count += 1;
    }
    count
}
