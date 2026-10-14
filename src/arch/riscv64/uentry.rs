//! RISC-V 64 user-mode entry and saved context restore.

use core::arch::asm;
use crate::arch::riscv64::paging;

/// Enter a fresh U-mode executable with a clean register state.
#[inline(never)]
pub unsafe fn sret_to_user(root_pa: usize, entry: usize, user_sp: usize) -> ! {
    let satp = paging::make_satp(root_pa);
    asm!(
        "csrw satp, {satp}",
        "sfence.vma zero, zero",
        "csrw sepc, {entry}",
        "li t0, 32",
        "csrw sstatus, t0",
        "mv sp, {user_sp}",
        "li ra, 0",
        "li gp, 0",
        "li tp, 0",
        "li t0, 0",
        "li t1, 0",
        "li t2, 0",
        "li s0, 0",
        "li s1, 0",
        "li a0, 0",
        "li a1, 0",
        "li a2, 0",
        "li a3, 0",
        "li a4, 0",
        "li a5, 0",
        "li a6, 0",
        "li a7, 0",
        "li s2, 0",
        "li s3, 0",
        "li s4, 0",
        "li s5, 0",
        "li s6, 0",
        "li s7, 0",
        "li s8, 0",
        "li s9, 0",
        "li s10, 0",
        "li s11, 0",
        "li t3, 0",
        "li t4, 0",
        "li t5, 0",
        "li t6, 0",
        "sret",
        satp = in(reg) satp,
        entry = in(reg) entry,
        user_sp = in(reg) user_sp,
        options(nostack, noreturn)
    );
}

/// Restore a saved U-mode integer context and enter through SRET.
#[inline(never)]
pub unsafe fn sret_to_user_context(
    root_pa: usize,
    context: *const crate::proc::context::UserContext,
) -> ! {
    let satp = paging::make_satp(root_pa);
    asm!(
        "csrw satp, a0",
        "sfence.vma zero, zero",
        "mv t6, a1",
        "ld t0, 256(t6)",
        "csrw sepc, t0",
        "li t0, 32",
        "csrw sstatus, t0",
        "ld sp, 264(t6)",
        "ld ra, 8(t6)",
        "ld gp, 24(t6)",
        "ld tp, 32(t6)",
        "ld t0, 40(t6)",
        "ld t1, 48(t6)",
        "ld t2, 56(t6)",
        "ld s0, 64(t6)",
        "ld s1, 72(t6)",
        "ld a0, 80(t6)",
        "ld a1, 88(t6)",
        "ld a2, 96(t6)",
        "ld a3, 104(t6)",
        "ld a4, 112(t6)",
        "ld a5, 120(t6)",
        "ld a6, 128(t6)",
        "ld a7, 136(t6)",
        "ld s2, 144(t6)",
        "ld s3, 152(t6)",
        "ld s4, 160(t6)",
        "ld s5, 168(t6)",
        "ld s6, 176(t6)",
        "ld s7, 184(t6)",
        "ld s8, 192(t6)",
        "ld s9, 200(t6)",
        "ld s10, 208(t6)",
        "ld s11, 216(t6)",
        "ld t3, 224(t6)",
        "ld t4, 232(t6)",
        "ld t5, 240(t6)",
        "ld t6, 248(t6)",
        "sret",
        in("a0") satp,
        in("a1") context,
        options(nostack, noreturn)
    );
}
