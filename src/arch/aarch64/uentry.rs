//! AArch64 EL0 (user-space) entry and stack allocation.
//!
//! ## ERET register contract
//!
//!   ELR_EL1  ← user entry PC
//!   SPSR_EL1 ← 0 (EL0t, all exceptions unmasked, AArch64)
//!   SP_EL0   ← user stack pointer
//!   TTBR0_EL1← process page-table base
//!   x0-x30   ← zeroed (no kernel pointer leaks)
//!
//! ## Safety
//!
//! The caller must ensure:
//!   - `entry` is a valid mapped EL0 virtual address.
//!   - `user_sp` points to a writable mapped user stack page.
//!   - `ttbr0` is the physical address of the process L0 page table.
//!   - Interrupts are disabled on entry (they re-enable when SPSR.I = 0 is restored by eret).
//!   - This function never returns to the caller.

use crate::mm::pmm;
use core::arch::asm;

/// 16 KiB user stack (4 pages).
const USER_STACK_PAGES: usize = 4;
const PAGE: usize = 4096;

/// Allocate and map a user stack for a fresh process, return the stack top
/// (highest VA; stacks grow down).
///
/// Places the stack just below the AArch64 48-bit user ceiling
/// (0x0000_FFFF_FFFF_F000).
pub fn alloc_user_stack(ttbr0: usize) -> Option<usize> {
    use crate::arch::aarch64::paging;

    const USER_STACK_TOP: usize = 0x0000_FFFF_FFFF_F000;
    let size = USER_STACK_PAGES * PAGE;
    let base = USER_STACK_TOP - size;

    for i in 0..USER_STACK_PAGES {
        let pa = pmm::alloc_page()?;
        unsafe {
            core::ptr::write_bytes(pa as *mut u8, 0, PAGE);
        }
        let va = base + i * PAGE;
        let flags = paging::PTE_VALID
            | paging::PTE_TABLE
            | paging::PTE_USER
            | paging::PTE_AF
            | paging::PTE_SH_INNER;
        unsafe {
            paging::map_page(ttbr0, va, pa, flags);
        }
    }

    Some(USER_STACK_TOP)
}

/// Transition to EL0 via ERET.
///
/// Loads TTBR0, zeros all GPRs to prevent kernel-pointer leaks, then erets
/// to `entry` with `user_sp` as SP_EL0.  Never returns.
///
/// # Safety
/// All preconditions in the module doc must hold.
#[inline(never)]
pub unsafe fn eret_to_user(ttbr0: usize, entry: usize, user_sp: usize) -> ! {
    asm!(
        // Switch to the process address space.
        "msr ttbr0_el1, {ttbr0}",
        "isb",

        "msr elr_el1,  {entry}",
        "msr spsr_el1, xzr",

        "msr sp_el0,   {sp}",

        // Zero all GPRs that musl/libc inspects on startup.
        "mov x0,  xzr",
        "mov x1,  xzr",
        "mov x2,  xzr",
        "mov x3,  xzr",
        "mov x4,  xzr",
        "mov x5,  xzr",
        "mov x6,  xzr",
        "mov x7,  xzr",
        "mov x8,  xzr",
        "mov x9,  xzr",
        "mov x10, xzr",
        "mov x11, xzr",
        "mov x12, xzr",
        "mov x13, xzr",
        "mov x14, xzr",
        "mov x15, xzr",
        "mov x16, xzr",
        "mov x17, xzr",
        "mov x18, xzr",
        "mov x19, xzr",
        "mov x20, xzr",
        "mov x21, xzr",
        "mov x22, xzr",
        "mov x23, xzr",
        "mov x24, xzr",
        "mov x25, xzr",
        "mov x26, xzr",
        "mov x27, xzr",
        "mov x28, xzr",
        "mov x29, xzr",
        "mov x30, xzr",

        "eret",

        ttbr0 = in(reg) ttbr0,
        entry = in(reg) entry,
        sp    = in(reg) user_sp,
        options(noreturn),
    );
}


/// Restore a saved EL0 register frame and enter through ERET.
/// User pages are rooted at TTBR0; kernel code and the saved PCB remain mapped
/// through TTBR1 while this non-returning transition is in progress.
pub unsafe fn eret_to_user_context(
    ttbr0: usize,
    context: *const crate::proc::context::UserContext,
) -> ! {
    asm!(
        "msr ttbr0_el1, x0",
        "isb",
        "mov x9, x1",
        "ldr x10, [x9, #256]",
        "msr elr_el1, x10",
        "ldr x10, [x9, #272]",
        "msr spsr_el1, x10",
        "ldr x10, [x9, #264]",
        "msr sp_el0, x10",
        "ldr x0,  [x9, #0]",
        "ldr x1,  [x9, #8]",
        "ldr x2,  [x9, #16]",
        "ldr x3,  [x9, #24]",
        "ldr x4,  [x9, #32]",
        "ldr x5,  [x9, #40]",
        "ldr x6,  [x9, #48]",
        "ldr x7,  [x9, #56]",
        "ldr x8,  [x9, #64]",
        "ldr x10, [x9, #80]",
        "ldr x11, [x9, #88]",
        "ldr x12, [x9, #96]",
        "ldr x13, [x9, #104]",
        "ldr x14, [x9, #112]",
        "ldr x15, [x9, #120]",
        "ldr x16, [x9, #128]",
        "ldr x17, [x9, #136]",
        "ldr x18, [x9, #144]",
        "ldr x19, [x9, #152]",
        "ldr x20, [x9, #160]",
        "ldr x21, [x9, #168]",
        "ldr x22, [x9, #176]",
        "ldr x23, [x9, #184]",
        "ldr x24, [x9, #192]",
        "ldr x25, [x9, #200]",
        "ldr x26, [x9, #208]",
        "ldr x27, [x9, #216]",
        "ldr x28, [x9, #224]",
        "ldr x29, [x9, #232]",
        "ldr x30, [x9, #240]",
        "ldr x9,  [x9, #72]",
        "eret",
        in("x0") ttbr0,
        in("x1") context,
        options(nostack, noreturn)
    );
}
