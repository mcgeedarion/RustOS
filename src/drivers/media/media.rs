//! MediaDevice HAL — backend registry for camera / ISP devices.
//!
//! Modelled after the GPU HAL in `crate::drivers::gpu::gpu` so that
//! higher-level subsystems can dequeue frames without caring which
//! physical ISP is installed.
//!
//! ## Usage
//! ```rust
//! // During kernel init:
//! media::init_isp4(mmio_base, fw_pa);
//!
//! // From a capture thread:
//! if let Some(frame) = media::dequeue_frame() {
//!     process(frame.data());
//!     media::queue_buf(frame.index());
//! }
//! ```

extern crate alloc;
use alloc::vec::Vec;
use spin::Mutex;

// ── Frame descriptor returned by dequeue_frame ────────────────────────────

/// A single captured frame with metadata.
#[derive(Debug)]
pub struct CaptureFrame {
    /// Buffer index in the DMA ring (pass back to `queue_buf`).
    pub index: u32,
    /// Physical address of the pixel data.
    pub paddr: u64,
    /// Byte length of valid pixel data in the buffer.
    pub byte_len: u32,
    /// Frame sequence counter.
    pub sequence: u32,
    /// Capture timestamp in nanoseconds (monotonic).
    pub timestamp_ns: u64,
}

impl CaptureFrame {
    /// Byte slice view of the frame data (unsafe — caller ensures liveness).
    ///
    /// # Safety
    /// `paddr` must be a valid, mapped physical-to-virtual address.
    pub unsafe fn data(&self) -> &[u8] {
        core::slice::from_raw_parts(self.paddr as *const u8, self.byte_len as usize)
    }
}

// ── Backend enum ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum Backend {
    #[cfg(feature = "amd-isp4")]
    AmdIsp4,
    None,
}

static BACKEND: Mutex<Backend> = Mutex::new(Backend::None);

// ── Public init entry-points ──────────────────────────────────────────────

/// Initialise the AMD ISP4 backend.
///
/// * `mmio_base` — physical base address of the ISP4 MMIO BAR.
/// * `fw_paddr`  — physical address of the pre-loaded ISP4 firmware blob.
#[cfg(feature = "amd-isp4")]
pub fn init_isp4(mmio_base: u64, fw_paddr: u64) {
    crate::drivers::media::isp4::init(mmio_base, fw_paddr);
    *BACKEND.lock() = Backend::AmdIsp4;
}

// ── HAL dispatch ─────────────────────────────────────────────────────────

/// Returns `true` if a media backend has been registered.
pub fn is_initialised() -> bool {
    *BACKEND.lock() != Backend::None
}

/// Start streaming (VIDIOC_STREAMON equivalent).
/// Allocates the DMA ring and powers the ISP sensor pipeline.
pub fn capture_start(width: u32, height: u32) -> Result<(), &'static str> {
    match *BACKEND.lock() {
        #[cfg(feature = "amd-isp4")]
        Backend::AmdIsp4 => crate::drivers::media::isp4::capture_start(width, height),
        Backend::None => Err("no media backend initialised"),
    }
}

/// Stop streaming (VIDIOC_STREAMOFF equivalent).
pub fn capture_stop() {
    match *BACKEND.lock() {
        #[cfg(feature = "amd-isp4")]
        Backend::AmdIsp4 => crate::drivers::media::isp4::capture_stop(),
        Backend::None => {},
    }
}

/// Dequeue a captured frame from the hardware ring.
/// Returns `None` if no frame is ready.
pub fn dequeue_frame() -> Option<CaptureFrame> {
    match *BACKEND.lock() {
        #[cfg(feature = "amd-isp4")]
        Backend::AmdIsp4 => crate::drivers::media::isp4::dequeue_frame(),
        Backend::None => None,
    }
}

/// Return a buffer to the DMA ring so the hardware can refill it.
pub fn queue_buf(index: u32) {
    match *BACKEND.lock() {
        #[cfg(feature = "amd-isp4")]
        Backend::AmdIsp4 => crate::drivers::media::isp4::queue_buf(index),
        Backend::None => {},
    }
}

/// Return a list of supported pixel formats.
pub fn enum_formats() -> Vec<PixelFormat> {
    match *BACKEND.lock() {
        #[cfg(feature = "amd-isp4")]
        Backend::AmdIsp4 => crate::drivers::media::isp4::enum_formats(),
        Backend::None => alloc::vec![],
    }
}

// ── Pixel-format descriptor ───────────────────────────────────────────────

/// FourCC-style pixel format descriptor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelFormat {
    /// FourCC code (e.g. `b"YUYV"`).
    pub fourcc: [u8; 4],
    /// Human-readable description.
    pub desc: &'static str,
    /// Bits per pixel (packed).
    pub bpp: u8,
}

/// YUYV 4:2:2 packed (default AMD ISP4 output).
pub const FMT_YUYV: PixelFormat = PixelFormat {
    fourcc: *b"YUYV",
    desc: "YUYV 4:2:2",
    bpp: 16,
};
/// NV12 4:2:0 semi-planar.
pub const FMT_NV12: PixelFormat = PixelFormat {
    fourcc: *b"NV12",
    desc: "NV12 4:2:0",
    bpp: 12,
};
