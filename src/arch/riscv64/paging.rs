//! RISC-V 64-bit Sv39 page table management.
//!
//! Layout: L2 (VPN[2]) → L1 (VPN[1]) → L0 (VPN[0]) → page frame.
//! All tables are 4096 bytes, containing 512 8-byte entries.
//!
//! VA bit decomposition (Sv39 – 39-bit virtual address):
//!   [38:30] VPN[2]  (9 bits) — top-level page directory
//!   [29:21] VPN[1]  (9 bits) — mid-level page directory
//!   [20:12] VPN[0]  (9 bits) — leaf page table
//!   [11:0]  page offset       (12 bits)
//!
//! PTE flag bits (RISC-V privileged spec §4.4):
//!   bit  0  V  — Valid
//!   bit  1  R  — Read
//!   bit  2  W  — Write
//!   bit  3  X  — Execute
//!   bit  4  U  — User-accessible
//!   bit  5  G  — Global
//!   bit  6  A  — Accessed  (managed by hardware or software)
//!   bit  7  D  — Dirty     (managed by hardware or software)
//!   bit  8  SOFTWARE: Copy-on-Write marker (never set by hardware)
//!
//! Physical Page Number (PPN) lives in PTE bits [53:10].
//! PPN is stored *shifted right by PAGE_SHIFT*, then placed at bit 10.
//!
//! satp register layout (MODE = Sv39 → value 8):
//!   [63:60] MODE (4 bits)
//!   [59:44] ASID (16 bits)
//!   [43:0]  PPN of root page table (44 bits)
//!
//! All physical addresses are identity-mapped (PA == VA) in the kernel.
//! satp holds (MODE_SV39 | (root_ppn)) for the current address space.

// ── Constants ────────────────────────────────────────────────────────────────

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT; // 4096
pub const TABLE_ENTRIES: usize = 512;

/// Sv39 satp MODE field — enables three-level paging.
pub const SATP_MODE_SV39: usize = 8 << 60;

/// PPN mask inside a PTE: bits [53:10] (44 bits).
const PTE_PPN_MASK: u64 = 0x003F_FFFF_FFFF_FC00;

// Public PTE flag constants (mirrors the x86_64 API surface).
pub const PTE_VALID: u64 = 1 << 0;    // V
pub const PTE_READ: u64 = 1 << 1;     // R
pub const PTE_WRITE: u64 = 1 << 2;    // W
pub const PTE_EXEC: u64 = 1 << 3;     // X
pub const PTE_USER: u64 = 1 << 4;     // U
pub const PTE_GLOBAL: u64 = 1 << 5;   // G
pub const PTE_ACCESSED: u64 = 1 << 6; // A
pub const PTE_DIRTY: u64 = 1 << 7;    // D
pub const PTE_COW: u64 = 1 << 8;      // software-defined CoW marker

// Internal shorthands.
const VALID: u64 = PTE_VALID;
const COW_BIT: u64 = PTE_COW;

// ── satp / TLB helpers ────────────────────────────────────────────────────────

/// Read the current `satp` CSR.
#[inline]
pub fn current_satp() -> usize {
    let satp: usize;
    unsafe {
        core::arch::asm!("csrr {}, satp", out(reg) satp, options(nostack, nomem));
    }
    satp
}

/// Extract the root page-table physical address from a `satp` value.
#[inline]
pub fn satp_to_root_pa(satp: usize) -> usize {
    // PPN field is bits [43:0]; PA = PPN << PAGE_SHIFT.
    (satp & 0x0000_0FFF_FFFF_FFFF) << PAGE_SHIFT
}

/// Kernel satp — lazily captured from the live CSR on the first call.
pub fn kernel_satp() -> usize {
    static KERNEL_SATP: core::sync::atomic::AtomicUsize =
        core::sync::atomic::AtomicUsize::new(0);
    let cached = KERNEL_SATP.load(core::sync::atomic::Ordering::Relaxed);
    if cached != 0 {
        return cached;
    }
    let satp = current_satp();
    KERNEL_SATP.store(satp, core::sync::atomic::Ordering::Relaxed);
    satp
}

/// Build a `satp` value for the given root page-table physical address.
#[inline]
pub fn make_satp(root_pa: usize) -> usize {
    let ppn = root_pa >> PAGE_SHIFT;
    SATP_MODE_SV39 | ppn
}

/// Load a new root page table by writing `satp` and flushing the TLB.
#[inline]
pub fn load_satp(root_pa: usize) {
    let satp = make_satp(root_pa);
    unsafe {
        core::arch::asm!(
            "csrw satp, {satp}",
            "sfence.vma zero, zero",
            satp = in(reg) satp,
            options(nostack),
        );
    }
}

/// Flush the TLB entry for a single virtual address (current ASID).
#[inline]
pub fn sfence_vma(va: usize) {
    unsafe {
        core::arch::asm!(
            "sfence.vma {va}, zero",
            va = in(reg) va,
            options(nostack),
        );
    }
}

// ── PTE encoding / decoding ───────────────────────────────────────────────────

/// Encode a physical address into PTE PPN bits [53:10].
#[inline]
const fn pa_to_ppn_bits(pa: usize) -> u64 {
    ((pa as u64) >> PAGE_SHIFT) << 10
}

/// Extract the physical address from a leaf PTE.
#[inline]
pub fn pte_to_pa(pte: u64) -> usize {
    ((pte & PTE_PPN_MASK) >> 10) as usize * PAGE_SIZE
}

/// Return `true` if `pte` is a *pointer* (non-leaf) entry:
/// Valid is set, but R/W/X are all clear.
#[inline]
fn is_pointer_pte(pte: u64) -> bool {
    pte & VALID != 0 && pte & (PTE_READ | PTE_WRITE | PTE_EXEC) == 0
}

// ── Physical memory helpers ───────────────────────────────────────────────────

/// Allocate and zero one 4 KiB page from the PMM.  Returns PA.
fn alloc_table() -> usize {
    let pa = crate::mm::pmm::alloc_page().expect("pmm: out of pages for Sv39 table");
    unsafe {
        core::ptr::write_bytes(pa as *mut u8, 0, PAGE_SIZE);
    }
    pa
}

/// Mutable pointer to the `idx`-th 8-byte PTE inside the table at `table_pa`.
#[inline]
unsafe fn pte_ptr(table_pa: usize, idx: usize) -> *mut u64 {
    (table_pa + idx * 8) as *mut u64
}

// ── VA index extraction ───────────────────────────────────────────────────────

#[inline]
fn vpn2(va: usize) -> usize { (va >> 30) & 0x1FF }
#[inline]
fn vpn1(va: usize) -> usize { (va >> 21) & 0x1FF }
#[inline]
fn vpn0(va: usize) -> usize { (va >> PAGE_SHIFT) & 0x1FF }

// ── Table walk (mutable) ──────────────────────────────────────────────────────

/// Walk the Sv39 three-level page table for `va` under `root_pa`,
/// creating missing intermediate tables as needed.
/// Returns a mutable pointer to the leaf PTE.
unsafe fn walk_mut(root_pa: usize, va: usize) -> *mut u64 {
    // L2 (root)
    let l2e = pte_ptr(root_pa, vpn2(va));
    if !is_pointer_pte(*l2e) {
        let t = alloc_table();
        *l2e = pa_to_ppn_bits(t) | VALID;
    }
    let l1_pa = pte_to_pa(*l2e);

    // L1
    let l1e = pte_ptr(l1_pa, vpn1(va));
    if !is_pointer_pte(*l1e) {
        let t = alloc_table();
        *l1e = pa_to_ppn_bits(t) | VALID;
    }
    let l0_pa = pte_to_pa(*l1e);

    // L0 – leaf
    pte_ptr(l0_pa, vpn0(va))
}

// ── Table walk (read-only) ────────────────────────────────────────────────────

/// Walk read-only; returns `None` if any level is absent or not a pointer PTE.
unsafe fn walk_ro(root_pa: usize, va: usize) -> Option<*mut u64> {
    let l2e = pte_ptr(root_pa, vpn2(va));
    if !is_pointer_pte(*l2e) {
        return None;
    }
    let l1_pa = pte_to_pa(*l2e);

    let l1e = pte_ptr(l1_pa, vpn1(va));
    if !is_pointer_pte(*l1e) {
        return None;
    }
    let l0_pa = pte_to_pa(*l1e);

    Some(pte_ptr(l0_pa, vpn0(va)))
}

// ── Public mapping API ────────────────────────────────────────────────────────

/// Map `va` → `pa` in the page table rooted at `root_pa` with the given
/// PTE `flags`.  `PTE_VALID` is always OR-ed in automatically.
/// Creates any missing intermediate tables.
pub fn map_page(root_pa: usize, va: usize, pa: usize, flags: u64) {
    unsafe {
        let pte = walk_mut(root_pa, va);
        *pte = pa_to_ppn_bits(pa) | (flags | VALID);
    }
}

/// Remove the mapping for `va` in the **current** address space.
/// Returns the physical address that was mapped, or `None`.
pub fn unmap_page(va: usize) -> Option<usize> {
    let root_pa = satp_to_root_pa(current_satp());
    unsafe {
        let pte = walk_ro(root_pa, va)?;
        if *pte & VALID == 0 {
            return None;
        }
        let pa = pte_to_pa(*pte);
        *pte = 0;
        sfence_vma(va);
        Some(pa)
    }
}

/// Return the physical address mapped at `va` in the address space rooted at
/// `root_pa`, or `None` if no valid leaf PTE exists.
pub fn virt_to_phys(root_pa: usize, va: usize) -> Option<usize> {
    unsafe {
        let pte = walk_ro(root_pa, va)?;
        if *pte & VALID == 0 {
            return None;
        }
        Some(pte_to_pa(*pte))
    }
}

// ── CoW fork ─────────────────────────────────────────────────────────────────

/// Clone the parent's Sv39 root for a copy-on-write fork.
///
/// For every **leaf** PTE in the parent that is Valid:
///   - If Writable: clear `PTE_WRITE`, set `PTE_COW` in both parent and child.
///   - Copy the (now read-only) PTE into the child's page tables.
///
/// The child gets its own L2/L1/L0 tables (new pages per level),
/// but the physical *leaf* pages are shared until a write fault occurs.
///
/// Returns the physical address of the new child root (for `make_satp`).
pub fn clone_root_cow(parent_root: usize) -> usize {
    let child_root = alloc_table();

    unsafe {
        for l2i in 0..TABLE_ENTRIES {
            let parent_l2e = pte_ptr(parent_root, l2i);
            if !is_pointer_pte(*parent_l2e) {
                continue;
            }

            let child_l1 = alloc_table();
            let child_l2e = pte_ptr(child_root, l2i);
            *child_l2e = pa_to_ppn_bits(child_l1) | VALID;

            let parent_l1_pa = pte_to_pa(*parent_l2e);
            for l1i in 0..TABLE_ENTRIES {
                let parent_l1e = pte_ptr(parent_l1_pa, l1i);
                if !is_pointer_pte(*parent_l1e) {
                    continue;
                }

                let child_l0 = alloc_table();
                let child_l1e = pte_ptr(child_l1, l1i);
                *child_l1e = pa_to_ppn_bits(child_l0) | VALID;

                let parent_l0_pa = pte_to_pa(*parent_l1e);
                for l0i in 0..TABLE_ENTRIES {
                    let parent_pte = pte_ptr(parent_l0_pa, l0i);
                    if *parent_pte & VALID == 0 {
                        continue;
                    }

                    let mut entry = *parent_pte;
                    if entry & PTE_WRITE != 0 {
                        entry = (entry & !PTE_WRITE) | COW_BIT;
                        *parent_pte = entry;
                        // Flush parent's TLB entry for this VA.
                        let va = (l2i << 30) | (l1i << 21) | (l0i << PAGE_SHIFT);
                        sfence_vma(va);
                    }
                    let child_pte = pte_ptr(child_l0, l0i);
                    *child_pte = entry;
                }
            }
        }
    }

    child_root
}
