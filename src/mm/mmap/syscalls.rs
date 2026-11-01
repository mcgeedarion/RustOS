use super::mm_lock::with_mm_write;
use super::vma::{insert_vma, page_align_up, remove_vma};
use super::{Vma, VmaKind, MAP_ANON, PAGE, PROT_READ, PROT_WRITE};
use crate::arch::{
    api::{PageFlags, Paging},
    Arch,
};
use crate::proc::scheduler;
use alloc::vec::Vec;

pub fn sys_munmap(addr: usize, length: usize) -> isize {
    if addr & (PAGE - 1) != 0 || length == 0 {
        return -22;
    }
    let Some(len) = length.checked_add(PAGE - 1).map(|n| n & !(PAGE - 1)) else {
        return -22;
    };
    let Some(end) = addr.checked_add(len) else { return -22; };
    if end > crate::uaccess::USER_SPACE_END { return -22; }
    let pid = scheduler::current_pid();
    with_mm_write(pid, |pcb| {
        if pcb.user_pagetable == 0 { return -14; }
        // Hold the write side across both leaf removal and VMA splitting.
        // Shared owners are dropped only after the TLB invalidation.
        super::mapping::remove_vma_inner(pcb, addr, end);
        0
    }).unwrap_or(-3)
}

pub fn sys_brk(addr: usize) -> isize {
    let pid = scheduler::current_pid();
    let (brk_base, cur_brk) = scheduler::with_proc(pid, |p| (p.brk_base, p.brk)).unwrap_or((0, 0));

    if brk_base == 0 {
        return cur_brk as isize;
    }
    if addr == 0 {
        return cur_brk as isize;
    }
    if addr < brk_base {
        return cur_brk as isize;
    }

    let new_brk = page_align_up(addr);
    let user_cr3 = scheduler::with_proc(pid, |p| p.user_pagetable).unwrap_or(0);
    if user_cr3 == 0 {
        return cur_brk as isize;
    }

    if new_brk > cur_brk {
        let extra = new_brk - cur_brk;
        let over = scheduler::with_proc(pid, |p| p.rlimits.exceeds_stack(extra)).unwrap_or(false);
        if over {
            return cur_brk as isize;
        }

        let pte_flags = PageFlags::PRESENT | PageFlags::WRITE | PageFlags::USER | PageFlags::NX;
        for page_va in (cur_brk..new_brk).step_by(PAGE) {
            if let Some(pa) = crate::mm::pmm::alloc_page() {
                <Arch as Paging>::map_page(user_cr3, page_va, pa, pte_flags);
            } else {
                scheduler::with_proc_mut(pid, |p, _| p.brk = page_va);
                super::anonymous::coalesce_or_insert_heap_vma(pid, brk_base, page_va);
                return page_va as isize;
            }
        }
        super::anonymous::coalesce_or_insert_heap_vma(pid, brk_base, new_brk);
    } else if new_brk < cur_brk {
        // Shrinking the brk region: unmap pages and defer freeing until after TLB shootdown.
        let mut to_free: Vec<usize> = Vec::new();
        for page_va in (new_brk..cur_brk).step_by(PAGE) {
            if let Some(pa) = <Arch as Paging>::virt_to_phys(user_cr3, page_va) {
                <Arch as Paging>::unmap_page(user_cr3, page_va);
                to_free.push(pa);
            }
        }
        // Shoot down TLB entries for the shrunk region before freeing frames.
        crate::smp::ipi::tlb_shootdown(new_brk as u64, cur_brk as u64, 0);
        for pa in to_free {
            crate::mm::pmm::free_page(pa);
        }
        super::anonymous::trim_heap_vma(pid, brk_base, new_brk);
    }

    scheduler::with_proc_mut(pid, |p, _| p.brk = new_brk);
    new_brk as isize
}

pub fn set_brk_base(pid: usize, end_of_bss: usize) {
    let base = set_brk_base_compute(end_of_bss);
    scheduler::with_proc_mut(pid, |p, _| {
        p.brk_base = base;
        p.brk = base;
    });
}

pub fn set_brk_base_compute(end_of_bss: usize) -> usize {
    page_align_up(end_of_bss)
}
