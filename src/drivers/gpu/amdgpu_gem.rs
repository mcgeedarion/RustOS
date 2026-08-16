//! AMD GPU GEM buffer manager.
//!
//! Provides a GEM (Graphics Execution Manager) heap for AMD GFX hardware.
//! Full command-ring and display-engine initialisation is architecture-
//! specific; this module implements the common buffer-management layer and
//! a minimal MMIO init sequence for GFX9 (Vega/Navi) as exposed by QEMU
//! `amdgpu` PCI device.
//!
//! ## Architecture
//!   - GEM heap: slab over PMM pages, handles 1–N pages per BO
//!   - GART (GPU address translation): identity-mapped for now
//!   - Display Engine: not yet implemented (use virtio-gpu for display)
//!
//! ## Thread Safety
//! All public functions are safe to call from multiple threads concurrently.
//! The internal state is protected by a spinlock (`AMD`).

extern crate alloc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::drivers::gpu::framebuffer::{Framebuffer, PixelFormat};
use crate::drivers::gpu::gpu::DisplayInfo;
use crate::mm::pmm;

const MMIO_SCRATCH_0: usize = 0x2040; // Scratch register (sanity check)
const MMIO_HDP_FLUSH: usize = 0x2F00; // HDP cache flush
const MMIO_GRBM_SOFT_RESET: usize = 0x8020; // GRBM soft reset
const MMIO_CP_ME_RAM_WADDR: usize = 0xC000;
const MMIO_SOC15_OFFSET: usize = 0x0; // SoC15 base offset

#[derive(Clone, Debug)]
pub struct GemBo {
    pub handle: u32,
    pub size: usize,
    pub phys: u64,   // CPU-visible physical address
    pub gpu_va: u64, // GPU virtual address (GART-mapped)
}

struct AmdGpu {
    mmio: usize,
    bos: Vec<GemBo>,
    next_handle: u32,
    width: u32,
    height: u32,
    fb: Option<Framebuffer>,
}

static AMD: Mutex<Option<AmdGpu>> = Mutex::new(None);

/// # Safety
/// `mmio_base` must be a valid, mapped MMIO region for the AMD GPU device.
pub fn init(mmio_base: u64) {
    unsafe {
        _init(mmio_base as usize);
    }
}

pub fn is_initialised() -> bool {
    AMD.lock().is_some()
}

pub fn display_info() -> Option<DisplayInfo> {
    AMD.lock().as_ref().map(|g| DisplayInfo {
        width: g.width,
        height: g.height,
        pitch: g.width * 4,
        bpp: 32,
    })
}

/// Allocate a GEM BO of `size` bytes. Returns handle, or None on OOM.
///
/// Uses the PMM to allocate physically contiguous pages suitable for DMA.
pub fn gem_alloc(size: usize) -> Option<u32> {
    let phys = alloc_dma(size, 4096)?;
    let mut amd = AMD.lock();
    let g = amd.as_mut()?;
    let h = g.next_handle;
    g.next_handle = g.next_handle.checked_add(1)?; // Prevent overflow
    g.bos.push(GemBo {
        handle: h,
        size,
        phys,
        gpu_va: phys,
    });
    Some(h)
}

/// Free a GEM BO by handle.
pub fn gem_free(handle: u32) {
    if let Some(g) = AMD.lock().as_mut() {
        g.bos.retain(|b| b.handle != handle);
    }
}

/// Return physical address of a GEM BO.
pub fn gem_phys(handle: u32) -> Option<u64> {
    AMD.lock()
        .as_ref()?
        .bos
        .iter()
        .find(|b| b.handle == handle)
        .map(|b| b.phys)
}

/// # Safety
/// `mmio` must point to a valid, mapped MMIO region.
unsafe fn _init(mmio: usize) {
    use core::ptr::{read_volatile, write_volatile};

    // Sanity: write + read scratch register.
    write_volatile((mmio + MMIO_SCRATCH_0) as *mut u32, 0xDEAD_BEEF);
    let v = read_volatile((mmio + MMIO_SCRATCH_0) as *const u32);
    if v != 0xDEAD_BEEF {
        return;
    } // not accessible

    // Soft-reset graphics pipeline (GRBM).
    write_volatile((mmio + MMIO_GRBM_SOFT_RESET) as *mut u32, 0x0000_8001);
    for _ in 0..100_000 {
        core::hint::spin_loop();
    }
    write_volatile((mmio + MMIO_GRBM_SOFT_RESET) as *mut u32, 0);

    // Flush HDP.
    write_volatile((mmio + MMIO_HDP_FLUSH) as *mut u32, 1);

    // Default display: 1024x768.
    let w = 1024u32;
    let h = 768u32;
    let fb_size = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
    let fb_phys = alloc_dma(fb_size, 4096);
    let fb = fb_phys.map(|p| Framebuffer::new(p, w, h, PixelFormat::Xrgb8888));

    *AMD.lock() = Some(AmdGpu {
        mmio,
        bos: Vec::new(),
        next_handle: 1,
        width: w,
        height: h,
        fb,
    });
}

/// Allocate DMA-capable memory using the PMM.
/// Returns the physical address, or None on failure.
fn alloc_dma(size: usize, align: usize) -> Option<u64> {
    let pages = size.checked_add(0xFFF)?.checked_div(0x1000)?;
    let phys = pmm::alloc_pages_aligned(pages, align)?.as_ptr() as u64;
    unsafe {
        core::ptr::write_bytes(phys as *mut u8, 0, pages.checked_mul(0x1000)?);
    }
    Some(phys)
}
