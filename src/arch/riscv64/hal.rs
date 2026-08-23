//! RISC-V 64 HAL implementation — `arch::api` trait impls.

use core::arch::asm;
use core::ops::Range;

use crate::arch::api::{
    ArchInit, ContextSwitch, Cpu, FpState, Interrupts, PageFlags, Paging, Serial, Syscall, Timer,
    Tlb, TrapFrame,
};

/// Architecture implementation marker.
pub struct ArchImpl;

impl ArchInit for ArchImpl {
    fn early_init() {
        // Disable interrupts during initialization
        unsafe {
            asm!("csrc sstatus, {bit}", bit = const 1 << 1); // Clear SIE (Supervisor Interrupt Enable)
        }
        serial_init();
    }

    fn late_init() {
        // Enable supervisor-level interrupts
        unsafe {
            asm!(
                "csrs sstatus, {bit}",
                bit = const 1 << 1, // Set SIE (Supervisor Interrupt Enable)
            );
        }
    }
}

impl Interrupts for ArchImpl {
    #[inline]
    fn enable() {
        unsafe {
            asm!("csrs sstatus, {bit}", bit = const 1 << 1, options(nostack));
        }
    }
    #[inline]
    fn disable() {
        unsafe {
            asm!("csrc sstatus, {bit}", bit = const 1 << 1, options(nostack));
        }
    }
    #[inline]
    fn are_enabled() -> bool {
        let sstatus: usize;
        unsafe {
            asm!("csrr {sstatus}, sstatus", sstatus = out(reg) sstatus, options(nostack, nomem));
        }
        (sstatus & (1 << 1)) != 0
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
        // RISC-V doesn't have a pause hint, but we can use a fence
        unsafe {
            asm!("fence iorw, iorw", options(nostack, nomem));
        }
    }
    fn id() -> u32 {
        // Read hart ID from mhartid (M-mode only) or assume 0 in S-mode
        // In S-mode under UEFI, we typically don't have direct access to mhartid
        // Return 0 as default (can be enhanced with SBI call if available)
        0
    }
    fn flags() -> usize {
        let sstatus: usize;
        unsafe {
            asm!("csrr {sstatus}, sstatus", sstatus = out(reg) sstatus, options(nostack, nomem));
        }
        sstatus
    }
}

impl Timer for ArchImpl {
    fn init_timer() {
        // Supervisor timer is typically provided by SBI or firmware
        // On UEFI systems, the generic timer may already be configured
    }
    fn ticks_per_sec() -> u64 {
        // Return timebase frequency (typically from device tree or SBI)
        // Default to 10 MHz as a common value
        10_000_000
    }
    fn read_ticks() -> u64 {
        // Read time CSR (supervisor mode accessible)
        let lo: u32;
        let hi: u32;
        unsafe {
            asm!("csrr {lo}, time", lo = out(reg) lo, options(nostack, nomem));
            asm!("csrr {hi}, timeh", hi = out(reg) hi, options(nostack, nomem));
        }
        ((hi as u64) << 32) | (lo as u64)
    }
}

impl Paging for ArchImpl {
    fn map_page(cr3: usize, va: usize, pa: usize, flags: PageFlags) -> bool {
        // Translate HAL flags → native RISC-V PTE flags
        let mut pte_flags: u64 = 0b001; // V (valid)
        if flags.contains(PageFlags::WRITE) {
            pte_flags |= 0b010; // W (writable)
        }
        if flags.contains(PageFlags::USER) {
            pte_flags |= 0b100; // U (user)
        }
        if flags.contains(PageFlags::NX) {
            pte_flags |= 0b000; // X not set (execute disabled)
        } else {
            pte_flags |= 0b1000; // X (executable)
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
            asm!("csrw satp, {cr3}", cr3 = in(reg) cr3, options(nostack));
            asm!("sfence.vma zero, zero", options(nostack));
        }
    }
    fn flush_va(va: usize) {
        unsafe {
            asm!("sfence.vma {va}", va = in(reg) va, options(nostack));
        }
    }
    fn flush_all() {
        unsafe {
            asm!("sfence.vma zero, zero", options(nostack));
        }
    }
    fn clone_address_space(src_cr3: usize) -> Option<usize> {
        super::paging::clone_pgtable_cow(src_cr3)
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
            asm!("sfence.vma {va}", va = in(reg) va, options(nostack));
        }
    }
    fn flush_all() {
        unsafe {
            asm!("sfence.vma zero, zero", options(nostack));
        }
    }
    fn flush_asid(_asid: u16) {
        // RISC-V doesn't have ASID in base ISA (optional in some extensions)
        // Flush all TLB entries
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
        // Save callee-saved registers (s0-s11, sp, ra)
        asm!(
            "sd s0, 0({f})",
            "sd s1, 8({f})",
            "sd s2, 16({f})",
            "sd s3, 24({f})",
            "sd s4, 32({f})",
            "sd s5, 40({f})",
            "sd s6, 48({f})",
            "sd s7, 56({f})",
            "sd s8, 64({f})",
            "sd s9, 72({f})",
            "sd s10, 80({f})",
            "sd s11, 88({f})",
            "sd sp, 96({f})",
            "sd ra, 104({f})",
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
            "ld s0, 0({f})",
            "ld s1, 8({f})",
            "ld s2, 16({f})",
            "ld s3, 24({f})",
            "ld s4, 32({f})",
            "ld s5, 40({f})",
            "ld s6, 48({f})",
            "ld s7, 56({f})",
            "ld s8, 64({f})",
            "ld s9, 72({f})",
            "ld s10, 80({f})",
            "ld s11, 88({f})",
            "ld sp, 96({f})",
            "ld ra, 104({f})",
            f = in(reg) nf as *const TrapFrame,
            options(nostack)
        );
    }

    fn make_user_frame(entry: u64, user_sp: u64) -> TrapFrame {
        let mut f = TrapFrame::zeroed();
        f.pc = entry;
        f.user_sp = user_sp;
        // sstatus: SPP=0 (user mode), SIE=1 (interrupts enabled)
        f.flags = 1 << 1;
        f
    }

    fn current_sp() -> usize {
        let sp: usize;
        unsafe {
            asm!("mv {}, sp", out(reg) sp, options(nostack, nomem));
        }
        sp
    }
}

impl Syscall for ArchImpl {
    fn syscall_setup() {
        // RISC-V syscalls use ecall instruction
        // Vector table setup is done in interrupt initialization
    }
    unsafe fn syscall_return(frame: *const TrapFrame) -> ! {
        let f = &*frame;
        // Restore user registers and return via sret
        asm!(
            "mv a0, {ret}",
            "mv sp, {usp}",
            "csrw sepc, {pc}",
            "csrw sstatus, {flags}",
            "sret",
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
        super::serial::init();
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
            // Enable FPU: mstatus.FS = 0b11 (initial state)
            // In S-mode, we modify sstatus
            let mut sstatus: usize;
            asm!("csrr {sstatus}, sstatus", sstatus = out(reg) sstatus, options(nostack));
            sstatus |= 0b11 << 13; // FS = 0b11 (dirty/initial)
            asm!("csrw sstatus, {sstatus}", sstatus = in(reg) sstatus, options(nostack));
        }
    }
    unsafe fn fp_save(dst: *mut u8) {
        // Save FPU registers (f0-f31, fcsr)
        let ptr = dst as *mut u64;
        asm!(
            "fsd f0, 0({ptr})",
            "fsd f1, 8({ptr})",
            "fsd f2, 16({ptr})",
            "fsd f3, 24({ptr})",
            "fsd f4, 32({ptr})",
            "fsd f5, 40({ptr})",
            "fsd f6, 48({ptr})",
            "fsd f7, 56({ptr})",
            "fsd f8, 64({ptr})",
            "fsd f9, 72({ptr})",
            "fsd f10, 80({ptr})",
            "fsd f11, 88({ptr})",
            "fsd f12, 96({ptr})",
            "fsd f13, 104({ptr})",
            "fsd f14, 112({ptr})",
            "fsd f15, 120({ptr})",
            "fsd f16, 128({ptr})",
            "fsd f17, 136({ptr})",
            "fsd f18, 144({ptr})",
            "fsd f19, 152({ptr})",
            "fsd f20, 160({ptr})",
            "fsd f21, 168({ptr})",
            "fsd f22, 176({ptr})",
            "fsd f23, 184({ptr})",
            "fsd f24, 192({ptr})",
            "fsd f25, 200({ptr})",
            "fsd f26, 208({ptr})",
            "fsd f27, 216({ptr})",
            "fsd f28, 224({ptr})",
            "fsd f29, 232({ptr})",
            "fsd f30, 240({ptr})",
            "fsd f31, 248({ptr})",
            "frcsr {fcsr}",
            "sd {fcsr}, 256({ptr})",
            ptr = in(reg) ptr,
            fcsr = out(reg) _,
            options(nostack)
        );
    }
    unsafe fn fp_restore(src: *const u8) {
        let ptr = src as *const u64;
        asm!(
            "fld f0, 0({ptr})",
            "fld f1, 8({ptr})",
            "fld f2, 16({ptr})",
            "fld f3, 24({ptr})",
            "fld f4, 32({ptr})",
            "fld f5, 40({ptr})",
            "fld f6, 48({ptr})",
            "fld f7, 56({ptr})",
            "fld f8, 64({ptr})",
            "fld f9, 72({ptr})",
            "fld f10, 80({ptr})",
            "fld f11, 88({ptr})",
            "fld f12, 96({ptr})",
            "fld f13, 104({ptr})",
            "fld f14, 112({ptr})",
            "fld f15, 120({ptr})",
            "fld f16, 128({ptr})",
            "fld f17, 136({ptr})",
            "fld f18, 144({ptr})",
            "fld f19, 152({ptr})",
            "fld f20, 160({ptr})",
            "fld f21, 168({ptr})",
            "fld f22, 176({ptr})",
            "fld f23, 184({ptr})",
            "fld f24, 192({ptr})",
            "fld f25, 200({ptr})",
            "fld f26, 208({ptr})",
            "fld f27, 216({ptr})",
            "fld f28, 224({ptr})",
            "fld f29, 232({ptr})",
            "fld f30, 240({ptr})",
            "fld f31, 248({ptr})",
            "ld t0, 256({ptr})",
            "fscsr t0",
            ptr = in(reg) ptr,
            options(nostack)
        );
    }
    fn fp_area_size() -> usize {
        // 32 FPRs * 8 bytes + 1 FCSR * 8 bytes = 264 bytes
        264
    }
}

/// Initialize the RISC-V HAL (called from arch::init).
#[cfg(any(not(feature = "boot_minimal"), feature = "userspace_boot"))]
pub fn init(boot_info: &'static crate::init::boot_info::BootInfo) -> ! {
    // Print boot information
    log!("RustOS riscv64 HAL initialized");
    log!("ACPI RSDP at: 0x{:X}", boot_info.rsdp_phys);
    
    if !boot_info.initramfs.is_empty() {
        log!("Initramfs: 0x{:X} - 0x{:X}", 
             boot_info.initramfs.start(),
             boot_info.initramfs.end());
    }
    
    // Late init (enables interrupts)
    <ArchImpl as ArchInit>::late_init();
    
    // Hand off to the scheduler
    #[cfg(not(feature = "userspace_boot"))]
    {
        crate::proc::scheduler::run_scheduler();
    }
    
    #[cfg(feature = "userspace_boot")]
    {
        crate::userspace_boot::init(boot_info);
    }
}

/// Canonical kernel virtual address range on RISC-V 64 (higher half).
#[inline]
pub fn kernel_va_range() -> Range<usize> {
    0xFFFF_FFFF_0000_0000usize..usize::MAX
}

/// `true` if `addr` is in the user-space half of the AS.
#[inline]
pub fn is_user_addr(addr: usize) -> bool {
    addr < 0x0000_0000_8000_0000
}

/// `true` if `addr` is canonical (top 16 bits are sign-extension of bit 47).
#[inline]
pub fn is_valid_addr(addr: usize) -> bool {
    let hi = addr >> 47;
    hi == 0 || hi == 0x1_FFFF
}
