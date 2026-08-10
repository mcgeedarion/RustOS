//! Copy-on-Write (CoW) page fault handler.
//!
//! Called by the arch page-fault dispatch when:
//!   - error_code bit 1 (W) == 1  — write fault
//!   - error_code bit 0 (P) == 1  — page IS present (protection violation)
//!   - error_code bit 2 (U) == 1  — fault in user mode
//!
//! The handler inspects the faulting virtual address, checks whether the
//! backing physical page has a reference count > 1 (shared), and if so
//! allocates a fresh private copy. If refcount == 1 the page is already
//! exclusively owned and we just flip the write-protect bit.
//!
//! ## Return convention
//!
//! `handle_cow_fault` returns `true` if the fault was resolved (the
//! faulting instruction should be retried) and `false` if it could not be
//! resolved (SIGSEGV has been queued before returning).

use crate::arch::{
    api::{PageFlags, Paging},
    Arch,
};
use crate::mm::mmap::{find_vma, PROT_EXEC, PROT_WRITE};
use crate::mm::phys::phys_to_virt;
use crate::mm::pmm::{alloc_page, free_page, get_page, page_refcount, put_page, PAGE_SIZE};
use crate::proc::scheduler;
use crate::proc::signal::send_sigsegv;

/// Try to resolve a CoW write-protection fault at `faulting_va`.
///
/// `current_pa` is the physical address currently mapped at `faulting_va`,
/// as read from the page-table by the arch trap handler.
pub fn handle_cow_fault(faulting_va: usize, current_pa: usize) -> bool {
    let page_va = faulting_va & !(PAGE_SIZE - 1);
    let pid = scheduler::current_pid();

    // Validate the VMA is writable.
    let vma = match find_vma(pid, faulting_va) {
        Some(v) => v,
        None => {
            send_sigsegv(pid, faulting_va);
            return false;
        }
    };

    if vma.prot & PROT_WRITE == 0 {
        // Genuinely read-only mapping — not a CoW page, deliver SIGSEGV.
        send_sigsegv(pid, faulting_va);
        return false;
    }

    let user_cr3 = scheduler::with_proc(pid, |p| p.user_pagetable).unwrap_or(0);
    if user_cr3 == 0 {
        send_sigsegv(pid, faulting_va);
        return false;
    }

    let refcount = page_refcount(current_pa);

    if refcount <= 1 {
        // Page is already exclusively owned by this process. The page-table
        // entry has the write-protect bit set (e.g., after fork + exec or
        // after a previous CoW split left us with refcount == 1). Just
        // re-map with write permission.
        let flags = prot_to_flags(vma.prot);
        <Arch as Paging>::map_page(user_cr3, page_va, current_pa, flags);
        <Arch as Paging>::flush_va(page_va);
        return true;
    }

    // Shared page: allocate a private copy.
    let new_pa = match alloc_page() {
        Some(p) => p,
        None => {
            // OOM — kill the process.
            crate::proc::signal::send_signal(pid, 9 /* SIGKILL */);
            return false;
        }
    };

    // Copy the old page content into the new page via the kernel direct map.
    let src_va = phys_to_virt(current_pa);
    let dst_va = phys_to_virt(new_pa);
    // SAFETY: both pointers are valid kernel-owned pages of exactly PAGE_SIZE.
    unsafe {
        core::ptr::copy_nonoverlapping(
            src_va as *const u8,
            dst_va as *mut u8,
            PAGE_SIZE,
        );
    }

    // Map the new private page with full permissions.
    let flags = prot_to_flags(vma.prot);
    <Arch as Paging>::map_page(user_cr3, page_va, new_pa, flags);
    <Arch as Paging>::flush_va(page_va);

    // Drop our reference to the old shared page.
    put_page(current_pa);

    true
}

/// Convert POSIX `prot` bits to arch page-table flags (user writable).
#[inline]
fn prot_to_flags(prot: u32) -> PageFlags {
    let mut f = PageFlags::PRESENT | PageFlags::USER | PageFlags::WRITE;
    if prot & PROT_EXEC == 0 {
        f |= PageFlags::NX;
    }
    f
}
