//! Architecture-independent HAL traits and small forwarding helpers.

use core::ops::{BitOr, BitOrAssign, Range};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageFlags(u64);

impl PageFlags {
    pub const PRESENT: Self = Self(1 << 0);
    pub const WRITE: Self = Self(1 << 1);
    pub const USER: Self = Self(1 << 2);
    pub const COW: Self = Self(1 << 9);
    pub const NX: Self = Self(1 << 63);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn bits(self) -> u64 {
        self.0
    }
}

impl BitOr for PageFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for PageFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrapFrame {
    pub regs: [u64; 16],
    pub pc: u64,
    pub user_sp: u64,
    pub flags: u64,
}

impl TrapFrame {
    pub const fn zeroed() -> Self {
        Self {
            regs: [0; 16],
            pc: 0,
            user_sp: 0,
            flags: 0,
        }
    }
}

pub trait ArchInit {
    fn early_init();
    fn late_init();
}

pub trait Interrupts {
    fn enable();
    fn disable();
    fn are_enabled() -> bool;
}

pub trait Cpu {
    fn halt();
    fn spin_hint();
    fn id() -> u32;
    fn flags() -> usize;
}

pub trait Timer {
    fn init_timer();
    fn ticks_per_sec() -> u64;
    fn read_ticks() -> u64;
}

pub trait Paging {
    fn map_page(cr3: usize, va: usize, pa: usize, flags: PageFlags) -> bool;
    fn unmap_page(cr3: usize, va: usize) -> Option<usize>;
    fn virt_to_phys(cr3: usize, va: usize) -> Option<usize>;
    fn kernel_cr3() -> usize;
    fn load_cr3(cr3: usize);
    fn flush_va(va: usize);
    fn flush_all();
    fn clone_address_space(src_cr3: usize) -> Option<usize>;
    fn new_user_address_space() -> Option<usize>;
}

pub trait Tlb {
    fn flush_va(va: usize);
    fn flush_all();
    fn flush_asid(asid: u16);
}

pub trait ContextSwitch {
    /// Switch between initialized task contexts.
    ///
    /// # Safety
    /// The frames must be valid, correctly aligned, and exclusively accessible
    /// during the switch. The next page table and stack must remain mapped,
    /// and the caller must hold the architecture's scheduler/interrupt guards.
    unsafe fn switch_to(
        current_frame: *mut TrapFrame,
        next_frame: *const TrapFrame,
        next_cr3: usize,
    );
    fn make_user_frame(entry: u64, user_sp: u64) -> TrapFrame;
    fn current_sp() -> usize;
}

pub trait Syscall {
    fn syscall_setup();
    /// Return to a validated userspace context.
    ///
    /// # Safety
    /// `frame` must reference a live architecture-compatible frame whose
    /// instruction pointer, stack, flags, and address space are safe for user
    /// mode. The caller must establish the required interrupt and stack state.
    unsafe fn syscall_return(frame: *const TrapFrame) -> !;
}

pub trait Serial {
    fn serial_init();
    fn serial_putc(byte: u8);
    fn serial_getc() -> Option<u8>;
}

pub trait FpState {
    fn fp_init();
    /// Save the current task's floating-point state.
    ///
    /// # Safety
    /// `dst` must be writable for `fp_area_size()` bytes with the alignment
    /// required by this architecture. CPU FP support must be initialized and
    /// the caller must prevent concurrent access or migration during the save.
    unsafe fn fp_save(dst: *mut u8);
    /// Restore a previously saved task floating-point state.
    ///
    /// # Safety
    /// `src` must reference an aligned, initialized save area of at least
    /// `fp_area_size()` bytes for this CPU. The caller must prevent concurrent
    /// modification or migration and restore only validated control state.
    unsafe fn fp_restore(src: *const u8);
    fn fp_area_size() -> usize;
}

#[inline]
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub fn kernel_va_range() -> Range<usize> {
    crate::arch::hal::kernel_va_range()
}

#[inline]
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub fn is_user_addr(addr: usize) -> bool {
    crate::arch::hal::is_user_addr(addr)
}

#[inline]
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub fn is_valid_addr(addr: usize) -> bool {
    crate::arch::hal::is_valid_addr(addr)
}
