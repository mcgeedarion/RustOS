//! AArch64 stage-1 page-table helpers for a 4 KiB, 48-bit VA setup.

#![allow(dead_code)]

use core::arch::asm;

use super::mem_layout::{mair, page, pte, tcr};

pub const PTE_VALID: usize = pte::VALID;
pub const PTE_TABLE: usize = pte::TABLE;
pub const PTE_USER: usize = pte::AP_RW_EL0;
pub const PTE_USER_RO: usize = pte::AP_RO_EL0;
pub const PTE_AF: usize = pte::AF;
pub const PTE_SH_INNER: usize = pte::SH_INNER;
pub const PTE_NORMAL: usize = pte::ATTR_NORMAL;
pub const PTE_UXN: usize = pte::UXN;
pub const PTE_PXN: usize = pte::PXN;

static mut KERNEL_TTBR1: usize = 0;

#[inline]
fn zero_page(pa: usize) {
    unsafe {
        core::ptr::write_bytes(pa as *mut u8, 0, page::SIZE);
    }
}

pub fn alloc_root_page_table() -> usize {
    let pa = crate::mm::pmm::alloc_page().expect("OOM: aarch64 root page table");
    zero_page(pa);
    pa
}

pub fn kernel_ttbr1() -> usize {
    unsafe { KERNEL_TTBR1 }
}

/// Install the kernel page table and enable the Armv8-A MMU/caches.
pub unsafe fn init_mmu(root_pa: usize) {
    KERNEL_TTBR1 = root_pa;
    asm!(
        "msr mair_el1, {mair}",
        "msr tcr_el1, {tcr}",
        "msr ttbr1_el1, {root}",
        "isb",
        "mrs x9, sctlr_el1",
        "orr x9, x9, {sctlr_bits}",
        "msr sctlr_el1, x9",
        "isb",
        mair = in(reg) mair::VALUE,
        tcr = in(reg) tcr::VALUE,
        root = in(reg) root_pa,
        sctlr_bits = const (super::mem_layout::sctlr::M | super::mem_layout::sctlr::C | super::mem_layout::sctlr::I),
        out("x9") _,
        options(nostack),
    );
}

/// Map a single 4 KiB page into the supplied AArch64 translation table root.
pub unsafe fn map_page(root_pa: usize, va: usize, pa: usize, flags: usize) {
    map_page_into(root_pa, va, pa, flags);
}

pub fn map_page_into(root_pa: usize, va: usize, pa: usize, flags: usize) {
    let idx = [
        super::mem_layout::va48::l0_index(va),
        super::mem_layout::va48::l1_index(va),
        super::mem_layout::va48::l2_index(va),
        super::mem_layout::va48::l3_index(va),
    ];

    unsafe {
        let mut table = root_pa as *mut usize;
        for level in 0..3 {
            let slot = table.add(idx[level]);
            let desc = slot.read_volatile();
            if desc & pte::VALID == 0 {
                let new_pa = crate::mm::pmm::alloc_page().expect("OOM: aarch64 page table");
                zero_page(new_pa);
                slot.write_volatile(pte::pa_to_desc(new_pa, pte::TABLE));
                table = new_pa as *mut usize;
            } else {
                table = (desc & pte::ADDR_MASK) as *mut usize;
            }
        }
        table.add(idx[3]).write_volatile(pte::pa_to_desc(
            pa,
            flags | pte::PAGE | pte::AF | pte::SH_INNER,
        ));
        super::hal::tlb_flush_page(va);
    }
}


/// Remove a 4 KiB leaf from the supplied address space and return its physical
/// frame base. The caller owns the frame and must release it when appropriate.
pub fn unmap_page(root_pa: usize, va: usize) -> Option<usize> {
    let idx = [
        super::mem_layout::va48::l0_index(va),
        super::mem_layout::va48::l1_index(va),
        super::mem_layout::va48::l2_index(va),
        super::mem_layout::va48::l3_index(va),
    ];
    unsafe {
        let mut table = root_pa as *mut usize;
        for level in 0..3 {
            let desc = table.add(idx[level]).read_volatile();
            if desc & pte::VALID == 0 || desc & pte::TABLE == 0 {
                return None;
            }
            table = (desc & pte::ADDR_MASK) as *mut usize;
        }
        let slot = table.add(idx[3]);
        let desc = slot.read_volatile();
        if desc & pte::VALID == 0 {
            return None;
        }
        slot.write_volatile(0);
        super::hal::tlb_flush_page(va);
        Some(desc & pte::ADDR_MASK)
    }
}

pub fn virt_to_phys(root_pa: usize, va: usize) -> Option<usize> {
    let idx = [
        super::mem_layout::va48::l0_index(va),
        super::mem_layout::va48::l1_index(va),
        super::mem_layout::va48::l2_index(va),
        super::mem_layout::va48::l3_index(va),
    ];
    unsafe {
        let mut table = root_pa as *const usize;
        for level in 0..4 {
            let desc = table.add(idx[level]).read_volatile();
            if desc & pte::VALID == 0 {
                return None;
            }
            if level == 3 || desc & pte::TABLE == 0 {
                return Some((desc & pte::ADDR_MASK) | (va & page::MASK));
            }
            table = (desc & pte::ADDR_MASK) as *const usize;
        }
    }
    None
}


/// Free user-half page-table pages. Leaf data frames must already be unmapped
/// and released by the process-memory owner. Upper-half kernel tables are
/// shared and must remain intact.
pub fn free_user_page_table(root_pa: usize) {
    unsafe fn free_table(table_pa: usize, level: usize) {
        for index in 0..512 {
            let slot = (table_pa as *const usize).add(index);
            let desc = slot.read_volatile();
            if desc & pte::VALID == 0 {
                continue;
            }
            // A table descriptor has bit 1 set at intermediate levels. Blocks
            // and 4 KiB pages are leaves and are not traversed.
            if level < 3 && desc & pte::TABLE != 0 {
                free_table(desc & pte::ADDR_MASK, level + 1);
            }
        }
        crate::mm::pmm::free_page(table_pa);
    }

    unsafe {
        let root = root_pa as *const usize;
        for index in 0..512 {
            let desc = root.add(index).read_volatile();
            if desc & pte::VALID != 0 && desc & pte::TABLE != 0 {
                free_table(desc & pte::ADDR_MASK, 1);
            }
        }
    }
    crate::mm::pmm::free_page(root_pa);
}
