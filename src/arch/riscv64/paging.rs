//! RISC-V 64 Sv39 page table management.
//!
//! Sv39 uses a 3-level page table hierarchy:
//!   PGD (L0) → PMD (L1) → PTE (L2) → page frame
//!
//! VA bit decomposition (39-bit virtual address):
//!   [38:30] PGD index   (9 bits)
//!   [29:21] PMD index   (9 bits)
//!   [20:12] PTE index   (9 bits)
//!   [11:0]  page offset (12 bits)
//!
//! PTE flag bits (RISC-V Sv39):
//!   bit 0  V (Valid)
//!   bit 1  R (Readable)
//!   bit 2  W (Writable)
//!   bit 3  X (Executable)
//!   bit 4  U (User)
//!   bit 5  G (Global)
//!   bit 6  A (Accessed)
//!   bit 7  D (Dirty)
//!   bit 8  RSW (Reserved for software)
//!   [63:10] PPN (Physical Page Number)
//!
//! satp register format (Sv39):
//!   [63:60] mode (8 for Sv39)
//!   [59:44] ASID (optional)
//!   [43:0]  PPN of root page table (PGD)

use core::arch::asm;

// Sv39 PTE flags
pub const PTE_V: u64 = 1 << 0; // Valid
pub const PTE_R: u64 = 1 << 1; // Readable
pub const PTE_W: u64 = 1 << 2; // Writable
pub const PTE_X: u64 = 1 << 3; // Executable
pub const PTE_U: u64 = 1 << 4; // User
pub const PTE_G: u64 = 1 << 5; // Global
pub const PTE_A: u64 = 1 << 6; // Accessed
pub const PTE_D: u64 = 1 << 7; // Dirty
pub const PTE_COW: u64 = 1 << 8; // Software-defined CoW marker

// PTE address mask (bits [53:10] for PPN, shifted to byte address)
pub const PTE_ADDR_MASK: u64 = 0x3FFFF_FFC00; // Mask for PPN bits [53:10], aligned to 8 bytes

// Page constants
pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 4096;
pub const PAGE_MASK: usize = 0xFFF;

// Sv39 constants
pub const SV39_MODE: usize = 8;
pub const PGD_ENTRIES: usize = 512;
pub const PMD_ENTRIES: usize = 512;
pub const PTE_ENTRIES: usize = 512;

// Index calculation macros/functions
#[inline]
fn pgd_index(va: usize) -> usize {
    (va >> 30) & 0x1FF
}

#[inline]
fn pmd_index(va: usize) -> usize {
    (va >> 21) & 0x1FF
}

#[inline]
fn pte_index(va: usize) -> usize {
    (va >> 12) & 0x1FF
}

/// Kernel CR3 (satp value) — captured on first call.
pub fn kernel_cr3() -> usize {
    static KERNEL_SATP: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
    let cached = KERNEL_SATP.load(core::sync::atomic::Ordering::Relaxed);
    if cached != 0 {
        return cached;
    }
    let satp: usize;
    unsafe {
        asm!("csrr {satp}, satp", satp = out(reg) satp, options(nostack, nomem));
    }
    KERNEL_SATP.store(satp, core::sync::atomic::Ordering::Relaxed);
    satp
}

/// Allocate a zeroed page-table page from the PMM. Returns PA.
fn alloc_table() -> usize {
    let pa = crate::mm::pmm::alloc_page().expect("pmm: out of pages for page table");
    unsafe {
        core::ptr::write_bytes(pa as *mut u8, 0, PAGE_SIZE);
    }
    pa
}

/// Get pointer to PTE entry
#[inline]
unsafe fn pte_ptr(table_pa: usize, idx: usize) -> *mut u64 {
    (table_pa + idx * 8) as *mut u64
}

/// Map a single 4 KiB page into the Sv39 translation table root.
/// `root_satp` is the satp value (mode | ppn).
pub fn map_page(root_satp: usize, va: usize, pa: usize, pte_flags: u64) -> bool {
    // Extract PPN from satp (bits [43:10] of satp, shifted left 10)
    let root_ppn = root_satp & 0x000F_FFFF_FFFF;
    let root_pa = root_ppn << 10;

    let idx0 = pgd_index(va);
    let idx1 = pmd_index(va);
    let idx2 = pte_index(va);

    unsafe {
        // Level 0: PGD
        let pgd = root_pa as *mut u64;
        let pgd_entry = pgd.add(idx0).read_volatile();
        let pmd_pa = if pgd_entry & PTE_V == 0 {
            // Allocate new PMD
            let new_pmd = alloc_table();
            pgd.add(idx0).write_volatile(((new_pmd >> 10) & 0x000F_FFFF_FFFF) | PTE_V);
            new_pmd
        } else {
            ((pgd_entry & 0x000F_FFFF_FFFF) << 10) as usize
        };

        // Level 1: PMD
        let pmd = pmd_pa as *mut u64;
        let pmd_entry = pmd.add(idx1).read_volatile();
        let pte_pa = if pmd_entry & PTE_V == 0 {
            // Allocate new PTE table
            let new_pte = alloc_table();
            pmd.add(idx1).write_volatile(((new_pte >> 10) & 0x000F_FFFF_FFFF) | PTE_V);
            new_pte
        } else {
            ((pmd_entry & 0x000F_FFFF_FFFF) << 10) as usize
        };

        // Level 2: PTE (leaf)
        let pte = pte_pa as *mut u64;
        let ppn = (pa >> 10) & 0x000F_FFFF_FFFF;
        pte.add(idx2).write_volatile(ppn | pte_flags);

        // Flush TLB for this VA
        asm!("sfence.vma {va}", va = in(reg) va, options(nostack));
    }

    true
}

/// Unmap a page and return its physical address if it was mapped.
pub fn unmap_page(root_satp: usize, va: usize) -> Option<usize> {
    let root_ppn = root_satp & 0x000F_FFFF_FFFF;
    let root_pa = root_ppn << 10;

    let idx0 = pgd_index(va);
    let idx1 = pmd_index(va);
    let idx2 = pte_index(va);

    unsafe {
        // Level 0: PGD
        let pgd = root_pa as *const u64;
        let pgd_entry = pgd.add(idx0).read_volatile();
        if pgd_entry & PTE_V == 0 {
            return None;
        }
        let pmd_pa = ((pgd_entry & 0x000F_FFFF_FFFF) << 10) as usize;

        // Level 1: PMD
        let pmd = pmd_pa as *const u64;
        let pmd_entry = pmd.add(idx1).read_volatile();
        if pmd_entry & PTE_V == 0 {
            return None;
        }
        let pte_pa = ((pmd_entry & 0x000F_FFFF_FFFF) << 10) as usize;

        // Level 2: PTE (leaf)
        let pte = pte_pa as *mut u64;
        let pte_entry = pte.add(idx2).read_volatile();
        if pte_entry & PTE_V == 0 {
            return None;
        }

        let pa = ((pte_entry & 0x000F_FFFF_FFFF) << 10) as usize;
        pte.add(idx2).write_volatile(0);

        // Flush TLB
        asm!("sfence.vma {va}", va = in(reg) va, options(nostack));

        Some(pa)
    }
}

/// Translate virtual address to physical address.
pub fn virt_to_phys(root_satp: usize, va: usize) -> Option<usize> {
    let root_ppn = root_satp & 0x000F_FFFF_FFFF;
    let root_pa = root_ppn << 10;

    let idx0 = pgd_index(va);
    let idx1 = pmd_index(va);
    let idx2 = pte_index(va);

    unsafe {
        // Level 0: PGD
        let pgd = root_pa as *const u64;
        let pgd_entry = pgd.add(idx0).read_volatile();
        if pgd_entry & PTE_V == 0 {
            return None;
        }
        let pmd_pa = ((pgd_entry & 0x000F_FFFF_FFFF) << 10) as usize;

        // Level 1: PMD
        let pmd = pmd_pa as *const u64;
        let pmd_entry = pmd.add(idx1).read_volatile();
        if pmd_entry & PTE_V == 0 {
            return None;
        }
        let pte_pa = ((pmd_entry & 0x000F_FFFF_FFFF) << 10) as usize;

        // Level 2: PTE (leaf)
        let pte = pte_pa as *const u64;
        let pte_entry = pte.add(idx2).read_volatile();
        if pte_entry & PTE_V == 0 {
            return None;
        }

        let pa = ((pte_entry & 0x000F_FFFF_FFFF) << 10) | (va & PAGE_MASK);
        Some(pa)
    }
}

/// Clone a page table using Copy-on-Write semantics.
/// Returns the new satp value or None on failure.
pub fn clone_pgtable_cow(src_satp: usize) -> Option<usize> {
    let src_root_ppn = src_satp & 0x000F_FFFF_FFFF;
    let src_root_pa = src_root_ppn << 10;

    // Allocate new root PGD
    let new_pgd = alloc_table();
    unsafe {
        core::ptr::write_bytes(new_pgd as *mut u8, 0, PAGE_SIZE);
    }

    // Walk source page tables and clone with COW markers
    let src_pgd = src_root_pa as *const u64;
    let dst_pgd = new_pgd as *mut u64;

    for i in 0..PGD_ENTRIES {
        let src_entry = src_pgd.add(i).read_volatile();
        if src_entry & PTE_V == 0 {
            continue;
        }

        let src_pmd_pa = ((src_entry & 0x000F_FFFF_FFFF) << 10) as usize;
        let new_pmd = alloc_table();
        unsafe {
            core::ptr::write_bytes(new_pmd as *mut u8, 0, PAGE_SIZE);
        }

        let src_pmd = src_pmd_pa as *const u64;
        let dst_pmd = new_pmd as *mut u64;

        for j in 0..PMD_ENTRIES {
            let src_pmd_entry = src_pmd.add(j).read_volatile();
            if src_pmd_entry & PTE_V == 0 {
                continue;
            }

            let src_pte_pa = ((src_pmd_entry & 0x000F_FFFF_FFFF) << 10) as usize;
            let new_pte = alloc_table();
            unsafe {
                core::ptr::write_bytes(new_pte as *mut u8, 0, PAGE_SIZE);
            }

            let src_pte = src_pte_pa as *const u64;
            let dst_pte = new_pte as *mut u64;

            for k in 0..PTE_ENTRIES {
                let src_pte_entry = src_pte.add(k).read_volatile();
                if src_pte_entry & PTE_V == 0 {
                    continue;
                }

                // Check if this is a writable user page - mark for COW
                if (src_pte_entry & PTE_U) != 0 && (src_pte_entry & PTE_W) != 0 {
                    // Mark as COW: clear W, set COW bit
                    let cow_entry = (src_pte_entry & !PTE_W) | PTE_COW;
                    dst_pte.add(k).write_volatile(cow_entry);
                    // Also update source to be COW
                    src_pte.add(k).write_volatile(cow_entry);
                } else {
                    // Just copy the entry
                    dst_pte.add(k).write_volatile(src_pte_entry);
                }
            }

            // Link PMD to new PTE table
            let pte_ppn = (new_pte >> 10) & 0x000F_FFFF_FFFF;
            dst_pmd.add(j).write_volatile(pte_ppn | PTE_V);
        }

        // Link PGD to new PMD table
        let pmd_ppn = (new_pmd >> 10) & 0x000F_FFFF_FFFF;
        dst_pgd.add(i).write_volatile(pmd_ppn | PTE_V);
    }

    // Build new satp value
    let new_satp = (SV39_MODE << 60) | (new_pgd >> 10);
    Some(new_satp)
}

/// Create a new user address space with kernel mappings copied.
pub fn new_user_address_space() -> Option<usize> {
    let pgd = alloc_table();
    unsafe {
        core::ptr::write_bytes(pgd as *mut u8, 0, PAGE_SIZE);
    }

    // Copy kernel mappings (upper half of address space)
    // In Sv39, kernel space starts at 0xFFFFFF8000000000 (bit 38 set)
    let ksatp = kernel_cr3();
    let k_root_ppn = ksatp & 0x000F_FFFF_FFFF;
    let k_root_pa = k_root_ppn << 10;

    unsafe {
        let src_pgd = (k_root_pa + 256 * 8) as *const u64; // Upper half starts at index 256
        let dst_pgd = (pgd + 256 * 8) as *mut u64;
        core::ptr::copy_nonoverlapping(src_pgd, dst_pgd, 256);
    }

    let satp = (SV39_MODE << 60) | (pgd >> 10);
    Some(satp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pgd_index() {
        assert_eq!(pgd_index(0x0000_0000_0000_0000), 0);
        assert_eq!(pgd_index(0x0000_0000_4000_0000), 1);
        assert_eq!(pgd_index(0x0000_0000_8000_0000), 2);
    }

    #[test]
    fn test_pmd_index() {
        assert_eq!(pmd_index(0x0000_0000_0000_0000), 0);
        assert_eq!(pmd_index(0x0000_0000_0020_0000), 1);
        assert_eq!(pmd_index(0x0000_0000_0040_0000), 2);
    }

    #[test]
    fn test_pte_index() {
        assert_eq!(pte_index(0x0000_0000_0000_0000), 0);
        assert_eq!(pte_index(0x0000_0000_0000_1000), 1);
        assert_eq!(pte_index(0x0000_0000_0000_2000), 2);
    }
}
