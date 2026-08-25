//! RISC-V 64-bit memory layout and architectural constants.
//!
//! This module defines the canonical Sv39 virtual memory layout used by
//! RustOS on RISC-V 64-bit systems.

/// Page size and related constants.
pub mod page {
    pub const SIZE: usize = 4096;
    pub const SHIFT: usize = 12;
    pub const MASK: usize = SIZE - 1;
    pub const TABLE_ENTRIES: usize = 512;

    #[inline]
    pub const fn align_up(n: usize) -> usize {
        (n + MASK) & !MASK
    }
    #[inline]
    pub const fn align_down(n: usize) -> usize {
        n & !MASK
    }
}

/// Sv39 39-bit VA layout using 4 KiB translation granules (three table levels).
/// 
/// RISC-V Sv39 uses a three-level page table:
///   - VPN[2] (bits 38:30) → L2 (root)
///   - VPN[1] (bits 29:21) → L1 (middle)
///   - VPN[0] (bits 20:12) → L0 (leaf)
pub mod va39 {
    pub const VA_BITS: usize = 39;
    /// User space top (half of 39-bit address space)
    pub const USER_TOP: usize = 1usize << 38;
    /// Kernel base (higher half)
    pub const KERNEL_BASE: usize = 0xFFFF_FFFF_0000_0000;
    /// Physical offset for direct mapping
    pub const PHYS_OFFSET: usize = 0xFFFF_FFFF_8000_0000;

    #[inline]
    pub const fn l2_index(va: usize) -> usize {
        (va >> 30) & 0x1ff
    }
    #[inline]
    pub const fn l1_index(va: usize) -> usize {
        (va >> 21) & 0x1ff
    }
    #[inline]
    pub const fn l0_index(va: usize) -> usize {
        (va >> 12) & 0x1ff
    }

    #[inline]
    pub const fn phys_to_virt(pa: usize) -> usize {
        pa + PHYS_OFFSET
    }
    #[inline]
    pub const fn virt_to_phys(va: usize) -> usize {
        va - PHYS_OFFSET
    }
}

/// RISC-V Sv39 PTE flag bits (privileged spec §4.4).
pub mod pte {
    pub const VALID: u64 = 1 << 0;    // V - Valid
    pub const READ: u64 = 1 << 1;     // R - Readable
    pub const WRITE: u64 = 1 << 2;    // W - Writable
    pub const EXEC: u64 = 1 << 3;     // X - Executable
    pub const USER: u64 = 1 << 4;     // U - User accessible
    pub const GLOBAL: u64 = 1 << 5;   // G - Global
    pub const ACCESSED: u64 = 1 << 6; // A - Accessed
    pub const DIRTY: u64 = 1 << 7;    // D - Dirty
    pub const COW: u64 = 1 << 8;      // Software CoW marker
    
    pub const ADDR_MASK: u64 = 0x003F_FFFF_FFFF_FC00;

    #[inline]
    pub const fn pa_to_pte(pa: usize, flags: u64) -> u64 {
        ((pa as u64) >> 12) << 10 | flags | VALID
    }
    #[inline]
    pub const fn pte_to_pa(pte: u64) -> usize {
        ((pte & ADDR_MASK) >> 10) as usize * SIZE
    }
}

/// satp register configuration for Sv39.
pub mod satp {
    pub const MODE_SV39: usize = 8 << 60;
    pub const ASID_SHIFT: usize = 44;
    pub const ASID_MASK: usize = 0xFFFF;
    pub const PPN_MASK: usize = 0x0000_0FFF_FFFF_FFFF;
}

/// Sstatus register bits.
pub mod sstatus {
    pub const SIE: usize = 1 << 1;    // Supervisor Interrupt Enable
    pub const SPIE: usize = 1 << 5;   // Previous SIE
    pub const SPP: usize = 1 << 8;    // Previous privilege mode
}

/// QEMU virt machine defaults.
pub mod qemu_virt {
    /// UART base address (16550-compatible)
    pub const UART_BASE: usize = 0x1000_0000;
    /// PLIC base address (Platform-Level Interrupt Controller)
    pub const PLIC_BASE: usize = 0x0C00_0000;
    /// PLIC context offset per hart
    pub const PLIC_CONTEXT_SIZE: usize = 0x1000;
    /// VirtIO MMIO base
    pub const VIRTIO_MMIO_BASE: usize = 0x1000_1000;
    /// PCIe ECAM base (if available)
    pub const PCIE_ECAM_BASE: usize = 0x3000_0000;
}
