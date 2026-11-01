//! UEFI GOP (Graphics Output Protocol) framebuffer driver.
//!
//! At boot time the UEFI firmware provides a linear framebuffer via GOP.
//! The bootloader records the base address, resolution and pixel format in
//! the boot info structure.  This driver maps that buffer and provides the
//! same `clear` / `blit` / `flush` API as the other GPU backends.
//!
//! ## GOP pixel formats (EFI_GRAPHICS_PIXEL_FORMAT)
//!   0 = PixelRedGreenBlueReserved8BitPerColor  (RGBX)
//!   1 = PixelBlueGreenRedReserved8BitPerColor  (BGRX)
//!   2 = PixelBitMask
//!   3 = PixelBltOnly  (no framebuffer, unsupported)

extern crate alloc;
use spin::Mutex;

use crate::drivers::gpu::framebuffer::{Framebuffer, PixelFormat};
use crate::drivers::gpu::gpu::DisplayInfo;

struct GopState {
    fb: Framebuffer,
    format: GopPixelFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GopPixelFormat {
    RgbX,
    BgrX,
    Unknown,
}

static GOP: Mutex<Option<GopState>> = Mutex::new(None);

/// Initialise from values supplied by the bootloader.
pub fn init(fb_base: u64, width: u32, height: u32, pitch_bytes: u32) {
    let _ = init_with_format(fb_base, width, height, pitch_bytes, 1);
}

/// Install a validated linear GOP mode. Bit-mask and BLT-only modes need
/// different pixel writers and must not be treated as 32-bit RGB buffers.
pub fn init_with_format(
    fb_base: u64,
    width: u32,
    height: u32,
    pitch_bytes: u32,
    pixel_format: u32,
) -> bool {
    let format = match pixel_format {
        0 => GopPixelFormat::RgbX,
        1 => GopPixelFormat::BgrX,
        _ => return false,
    };
    let Some(row_bytes) = width.checked_mul(4) else { return false; };
    let Some(size) = (pitch_bytes as u64).checked_mul(height as u64) else { return false; };
    if fb_base == 0 || width == 0 || height == 0 || pitch_bytes < row_bytes
        || pitch_bytes % 4 != 0 || fb_base.checked_add(size).is_none()
    {
        return false;
    }
    let fb = Framebuffer {
        phys: fb_base,
        width,
        height,
        pitch: pitch_bytes,
        format: PixelFormat::Xrgb8888,
    };
    *GOP.lock() = Some(GopState {
        fb,
        format,
    });
    true
}

pub fn is_initialised() -> bool {
    GOP.lock().is_some()
}

pub fn display_info() -> Option<DisplayInfo> {
    GOP.lock().as_ref().map(|g| DisplayInfo {
        width: g.fb.width,
        height: g.fb.height,
        pitch: g.fb.pitch,
        bpp: 32,
    })
}

pub fn clear(argb: u32) {
    if let Some(g) = GOP.lock().as_ref() {
        g.fb.clear(argb_to_gopfmt(argb, g.format));
    }
}

pub fn blit(x: u32, y: u32, width: u32, height: u32, pixels: &[u32]) {
    let g = GOP.lock();
    if let Some(g) = g.as_ref() {
        let fmt = g.format;
        // Convert each pixel to GOP native format.
        let conv: alloc::vec::Vec<u32> = pixels.iter().map(|&p| argb_to_gopfmt(p, fmt)).collect();
        g.fb.blit(x, y, width, height, &conv);
    }
}

/// Return the raw framebuffer physical address (for DRM scanout).
pub fn fb_phys() -> Option<u64> {
    GOP.lock().as_ref().map(|g| g.fb.phys)
}

/// Capture the boot-services GOP framebuffer before ExitBootServices.
/// Called by UEFI loader paths that snapshot the firmware mode early.
/// Returns `Some(Framebuffer)` if GOP is already initialised, else `None`.
pub fn capture_from_boot_services() -> Option<Framebuffer> {
    GOP.lock().as_ref().map(|g| g.fb.clone())
}

#[inline]
fn argb_to_gopfmt(argb: u32, fmt: GopPixelFormat) -> u32 {
    // Input: 0xAARRGGBB
    let r = (argb >> 16) & 0xFF;
    let g = (argb >> 8) & 0xFF;
    let b = argb & 0xFF;
    match fmt {
        // A u32 store is little-endian on all supported targets.
        GopPixelFormat::RgbX => (b << 16) | (g << 8) | r,
        GopPixelFormat::BgrX => (r << 16) | (g << 8) | b,
        GopPixelFormat::Unknown => argb,
    }
}
