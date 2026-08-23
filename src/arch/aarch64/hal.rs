//! AArch64 HAL implementation — `arch::api` trait impls.

use core::arch::asm;
use core::ops::Range;

use crate::arch::api::{
    ArchInit, ContextSwitch, Cpu, FpState, Interrupts, PageFlags, Paging, Serial, Syscall, Timer,
    Tlb, TrapFrame,
};

use super::mem_layout::{page, uart, va48};
use super::simd;

pub struct ArchImpl;

impl ArchInit for ArchImpl {
    fn early_init() {
        unsafe {
            asm!("msr daifset, #0b0010", options(nostack)); // Disable IRQ
        }
        serial_init();
    }

    fn late_init() {
        // Enable timer and interrupts
        unsafe {
            asm!("msr daifclr, #0b0010", options(nostack)); // Enable IRQ
        }
    }
}

impl Interrupts for ArchImpl {
    #[inline]
    fn enable() {
        unsafe {
            asm!("msr daifclr, #0b0010", options(nostack));
        }
    }
    #[inline]
    fn disable() {
        unsafe {
            asm!("msr daifset, #0b0010", options(nostack));
        }
    }
    #[inline]
    fn are_enabled() -> bool {
        let daif: usize;
        unsafe {
            asm!("mrs {daif}, daif", daif = out(reg) daif, options(nostack, nomem));
        }
        daif & (1 << 7) == 0
    }
}

impl Cpu for ArchImpl {
    #[inline]
    fn halt() {
        unsafe {
            asm!("wfi", options(nostack, nomem));
        }
    }
    #[inline]
    fn spin_hint() {
        unsafe {
            asm!("yield", options(nostack, nomem));
        }
    }
    fn id() -> u32 {
        let mpidr: usize;
        unsafe {
            asm!("mrs {mpidr}, mpidr_el1", mpidr = out(reg) mpidr, options(nostack, nomem));
        }
        (mpidr & 0xff_ff) as u32
    }
    fn flags() -> usize {
        let daif: usize;
        unsafe {
            asm!("mrs {daif}, daif", daif = out(reg) daif, options(nostack, nomem));
        }
        daif
    }
}

impl Timer for ArchImpl {
    fn init_timer() {
        // Generic timer is enabled by default on UEFI systems
        // Configure timer frequency if needed
    }
    fn ticks_per_sec() -> u64 {
        // Return generic timer frequency
        let freq: u64;
        unsafe {
            asm!("mrs {freq}, cntfrq_el0", freq = out(reg) freq, options(nostack, nomem));
        }
        freq
    }
    fn read_ticks() -> u64 {
        let cnt: u64;
        unsafe {
            asm!("mrs {cnt}, cntvct_el0", cnt = out(reg) cnt, options(nostack, nomem));
        }
        cnt
    }
}

impl Paging for ArchImpl {
    fn map_page(cr3: usize, va: usize, pa: usize, flags: PageFlags) -> bool {
        // Translate HAL flags → native AArch64 PTE flags
        let mut pte_flags: u64 = 0b11; // AP[1:0] = 11 (read/write), AF = 1
        if !flags.contains(PageFlags::WRITE) {
            pte_flags &= !0b10; // Clear AP[1] for read-only
        }
        if flags.contains(PageFlags::USER) {
            pte_flags |= 0b100; // AP[2] = 1 for user access
        }
        if flags.contains(PageFlags::NX) {
            pte_flags |= 1 << 54; // XN bit
        }
        super::paging::map_page(cr3, va, pa, pte_flags)
    }
    fn unmap_page(cr3: usize, va: usize) -> Option<usize> {
        super::paging::unmap_page(cr3, va)
    }
    fn virt_to_phys(cr3: usize, va: usize) -> Option<usize> {
        super::paging::virt_to_phys(cr3, va)
    }
    fn kernel_cr3() -> usize {
        super::paging::kernel_cr3()
    }
    fn load_cr3(cr3: usize) {
        unsafe {
            asm!("msr ttbr0_el1, {cr3}", cr3 = in(reg) cr3, options(nostack));
            asm!("dsb ish", options(nostack));
            asm!("isb", options(nostack));
        }
    }
    fn flush_va(va: usize) {
        unsafe {
            let va_page = va >> page::SHIFT;
            asm!("dsb ishst", "tlbi vaae1is, {page}", "dsb ish", "isb", page = in(reg) va_page, options(nostack));
        }
    }
    fn flush_all() {
        unsafe {
            asm!("dsb ishst", "tlbi vmalle1", "dsb ish", "isb", options(nostack));
        }
    }
    fn clone_address_space(src_cr3: usize) -> Option<usize> {
        super::paging::clone_pgd_cow(src_cr3)
    }
    fn new_user_address_space() -> Option<usize> {
        use crate::mm::pmm::alloc_page;
        let pgd = alloc_page()?;
        unsafe {
            core::ptr::write_bytes(pgd as *mut u8, 0, 4096);
        }
        // Copy kernel mappings (upper half)
        let kpgd = Self::kernel_cr3();
        unsafe {
            let src = (kpgd + 256 * 8) as *const u64;
            let dst = (pgd + 256 * 8) as *mut u64;
            core::ptr::copy_nonoverlapping(src, dst, 256);
        }
        Some(pgd)
    }
}

impl Tlb for ArchImpl {
    fn flush_va(va: usize) {
        unsafe {
            let va_page = va >> page::SHIFT;
            asm!("dsb ishst", "tlbi vaae1is, {page}", "dsb ish", "isb", page = in(reg) va_page, options(nostack));
        }
    }
    fn flush_all() {
        unsafe {
            asm!("dsb ishst", "tlbi vmalle1", "dsb ish", "isb", options(nostack));
        }
    }
    fn flush_asid(_asid: u16) {
        // AArch64 supports ASID in TTBR0_EL1
        // For now, flush all (can be enhanced to use ASID)
        Self::flush_all();
    }
}

impl ContextSwitch for ArchImpl {
    unsafe fn switch_to(
        current_frame: *mut TrapFrame,
        next_frame: *const TrapFrame,
        next_cr3: usize,
    ) {
        let frame = &mut *current_frame;
        // Save callee-saved registers (x19-x28, x29=FP, x30=LR)
        asm!(
            "stp x19, x20, [{f}, #0]",
            "stp x21, x22, [{f}, #16]",
            "stp x23, x24, [{f}, #32]",
            "stp x25, x26, [{f}, #48]",
            "stp x27, x28, [{f}, #64]",
            "mov x9, sp",
            "str x9, [{f}, #80]",
            f = in(reg) frame as *mut TrapFrame,
            options(nostack)
        );
        
        // Switch address space if needed
        if next_cr3 != Self::kernel_cr3() {
            Self::load_cr3(next_cr3);
        }
        
        // Restore callee-saved registers from next_frame
        let nf = &*next_frame;
        asm!(
            "ldp x19, x20, [{f}, #0]",
            "ldp x21, x22, [{f}, #16]",
            "ldp x23, x24, [{f}, #32]",
            "ldp x25, x26, [{f}, #48]",
            "ldp x27, x28, [{f}, #64]",
            "ldr x9, [{f}, #80]",
            "mov sp, x9",
            f = in(reg) nf as *const TrapFrame,
            options(nostack)
        );
    }

    fn make_user_frame(entry: u64, user_sp: u64) -> TrapFrame {
        let mut f = TrapFrame::zeroed();
        f.pc = entry;
        f.user_sp = user_sp;
        // SPSR: EL0t, interrupts enabled
        f.flags = 0x0000_0000;
        f
    }

    fn current_sp() -> usize {
        let sp: usize;
        unsafe {
            asm!("mov {}, sp", out(reg) sp, options(nostack, nomem));
        }
        sp
    }
}

impl Syscall for ArchImpl {
    fn syscall_setup() {
        // Configure SCTLR_EL1 for syscall support
        // Set up VBAR_EL1 for exception vectors (done in interrupt setup)
    }
    unsafe fn syscall_return(frame: *const TrapFrame) -> ! {
        let f = &*frame;
        // Restore user registers and return via eret
        asm!(
            "mov x0, {ret}",
            "mov sp, {usp}",
            "msr elr_el1, {pc}",
            "msr spsr_el1, {flags}",
            "eret",
            ret   = in(reg) f.regs[0],
            usp   = in(reg) f.user_sp,
            pc    = in(reg) f.pc,
            flags = in(reg) f.flags,
            options(noreturn)
        );
    }
}

impl Serial for ArchImpl {
    fn serial_init() {
        super::serial::init(13, 1);
    }
    fn serial_putc(byte: u8) {
        super::serial::putc(byte);
    }
    fn serial_getc() -> Option<u8> {
        super::serial::getc()
    }
}

impl FpState for ArchImpl {
    fn fp_init() {
        unsafe {
            // Enable FP/SIMD: CPACR_EL1.FPEN = 0b11, ZEN = 0b11 (if SVE)
            let mut cpacr: u64;
            asm!("mrs {cpacr}, cpacr_el1", cpacr = out(reg) cpacr, options(nostack));
            cpacr |= 0b11 << 20; // FPEN
            cpacr |= 0b11 << 16; // ZEN (SVE)
            asm!("msr cpacr_el1, {cpacr}", cpacr = in(reg) cpacr, options(nostack));
            asm!("isb", options(nostack));
        }
    }
    unsafe fn fp_save(dst: *mut u8) {
        simd::save(&mut *(dst as *mut simd::FpState));
    }
    unsafe fn fp_restore(src: *const u8) {
        simd::restore(&*(src as *const simd::FpState));
    }
    fn fp_area_size() -> usize {
        core::mem::size_of::<simd::FpState>()
    }
}

/// Canonical kernel virtual address range on AArch64 (higher half).
#[inline]
pub fn kernel_va_range() -> Range<usize> {
    va48::KERNEL_BASE..usize::MAX
}

/// `true` if `addr` is in the user-space half of the AS.
#[inline]
pub fn is_user_addr(addr: usize) -> bool {
    addr < va48::USER_TOP
}

/// `true` if `addr` is canonical (top 16 bits are sign-extension of bit 47).
#[inline]
pub fn is_valid_addr(addr: usize) -> bool {
    let sign = addr >> 48;
    sign == 0 || sign == 0xffff
}
