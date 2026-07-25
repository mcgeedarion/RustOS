//! DRM/KMS subsystem (kernel-mode setting).
//!
//! Implements a minimal DRM-compatible KMS layer:
//!   - CRTC management (one virtual CRTC per display)
//!   - Connector enumeration (HDMI 2.1, DisplayPort, VGA)
//!   - Plane management (primary plane + one overlay)
//!   - Atomic modesetting via `drm_atomic_commit`
//!   - GEM buffer object lifecycle (alloc/free/map)
//!   - HDMI 2.1 FRL modes (4K@120Hz, 8K@60Hz, 4K@144Hz)
//!   - ACP 7.x audio co-processor endpoint initialisation
//!
//! ## Architecture
//!   DrmDevice
//!     ├─ Vec<Crtc>     (one per physical display controller)
//!     ├─ Vec<Connector>(one per physical output port)
//!     ├─ Vec<Plane>    (primary + overlay planes)
//!     ├─ GemHeap       (DMA-coherent GEM buffer allocator)
//!     └─ Option<AcpDevice> (ACP 7.x audio co-processor)
//!
//! ## Usage
//!   ```rust
//!   drm::init();
//!   let fb = drm::gem_alloc(width, height, PixelFormat::Xrgb8888)?;
//!   drm::atomic_commit(crtc_id, connector_id, &mode, fb);
//!   ```

extern crate alloc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::display::drm::acp::{AcpDevice, AcpEndpointKind};
use crate::display::drm::hdmi::{FrlRate, HdmiCapabilities, HdmiVersion};
use crate::drivers::gpu::framebuffer::{Framebuffer, PixelFormat};

/// Pixel-clock constant for HDMI 2.1 high-rate modes.
const CLOCK_594MHZ: u32 = 594_000; // 4K@60 / 1080p@240
const CLOCK_1188MHZ: u32 = 1_188_000; // 4K@120 / 8K@60
const CLOCK_2178MHZ: u32 = 2_178_000; // 4K@144 (FRL only)

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisplayMode {
    pub hdisplay: u16,
    pub vdisplay: u16,
    pub clock_khz: u32,
    pub hsync_start: u16,
    pub hsync_end: u16,
    pub htotal: u16,
    pub vsync_start: u16,
    pub vsync_end: u16,
    pub vtotal: u16,
    /// Mode flags – bit 0: HDMI 2.1 FRL required, bit 1: DSC required.
    pub flags: u32,
}

impl DisplayMode {
    /// Standard 1920×1080 @ 60 Hz
    pub fn fullhd_60() -> Self {
        Self {
            hdisplay: 1920,
            vdisplay: 1080,
            clock_khz: 148_500,
            hsync_start: 2008,
            hsync_end: 2052,
            htotal: 2200,
            vsync_start: 1084,
            vsync_end: 1089,
            vtotal: 1125,
            flags: 0,
        }
    }
    /// Standard 1280×720 @ 60 Hz
    pub fn hd_60() -> Self {
        Self {
            hdisplay: 1280,
            vdisplay: 720,
            clock_khz: 74_250,
            hsync_start: 1390,
            hsync_end: 1430,
            htotal: 1650,
            vsync_start: 725,
            vsync_end: 730,
            vtotal: 750,
            flags: 0,
        }
    }
    /// 800×600 @ 60 Hz (SVGA)
    pub fn svga_60() -> Self {
        Self {
            hdisplay: 800,
            vdisplay: 600,
            clock_khz: 40_000,
            hsync_start: 840,
            hsync_end: 968,
            htotal: 1056,
            vsync_start: 601,
            vsync_end: 605,
            vtotal: 628,
            flags: 0,
        }
    }

    // ── HDMI 2.1 modes (require FRL) ──────────────────────────────────────

    /// 3840×2160 @ 60 Hz (4K UHD, HDMI 2.0 / TMDS boundary, also FRL)
    pub fn uhd4k_60() -> Self {
        Self {
            hdisplay: 3840,
            vdisplay: 2160,
            clock_khz: CLOCK_594MHZ,
            hsync_start: 4016,
            hsync_end: 4104,
            htotal: 4400,
            vsync_start: 2168,
            vsync_end: 2178,
            vtotal: 2250,
            flags: 0,
        }
    }
    /// 3840×2160 @ 120 Hz (4K UHD, HDMI 2.1 FRL ≥ 40 Gb/s required)
    pub fn uhd4k_120() -> Self {
        Self {
            hdisplay: 3840,
            vdisplay: 2160,
            clock_khz: CLOCK_1188MHZ,
            hsync_start: 4016,
            hsync_end: 4104,
            htotal: 4400,
            vsync_start: 2168,
            vsync_end: 2178,
            vtotal: 2250,
            flags: 0x1, // FRL required
        }
    }
    /// 3840×2160 @ 144 Hz (4K UHD, HDMI 2.1 FRL 48 Gb/s + DSC)
    pub fn uhd4k_144() -> Self {
        Self {
            hdisplay: 3840,
            vdisplay: 2160,
            clock_khz: CLOCK_2178MHZ,
            hsync_start: 4016,
            hsync_end: 4104,
            htotal: 4400,
            vsync_start: 2168,
            vsync_end: 2178,
            vtotal: 2250,
            flags: 0x3, // FRL + DSC required
        }
    }
    /// 7680×4320 @ 60 Hz (8K UHD, HDMI 2.1 FRL 48 Gb/s + DSC)
    pub fn uhd8k_60() -> Self {
        Self {
            hdisplay: 7680,
            vdisplay: 4320,
            clock_khz: CLOCK_1188MHZ,
            hsync_start: 8008,
            hsync_end: 8208,
            htotal: 8800,
            vsync_start: 4336,
            vsync_end: 4356,
            vtotal: 4500,
            flags: 0x3, // FRL + DSC required
        }
    }
    /// 2560×1440 @ 165 Hz (QHD, HDMI 2.1 FRL)
    pub fn qhd_165() -> Self {
        Self {
            hdisplay: 2560,
            vdisplay: 1440,
            clock_khz: 586_000,
            hsync_start: 2720,
            hsync_end: 2800,
            htotal: 2960,
            vsync_start: 1441,
            vsync_end: 1444,
            vtotal: 1481,
            flags: 0x1, // FRL required
        }
    }

    /// Returns `true` if this mode requires HDMI 2.1 FRL link.
    pub fn requires_frl(&self) -> bool {
        self.flags & 0x1 != 0
    }

    /// Returns `true` if this mode requires Display Stream Compression.
    pub fn requires_dsc(&self) -> bool {
        self.flags & 0x2 != 0
    }

    /// Minimum [`FrlRate`] required to carry this mode (approximate).
    pub fn min_frl_rate(&self) -> FrlRate {
        match self.clock_khz {
            0..=594_000 => FrlRate::Tmds,
            594_001..=1_188_000 => FrlRate::Frl4Lanes10Gbps,
            _ => FrlRate::Frl4Lanes12Gbps,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectorType {
    HDMI,
    /// HDMI 2.1 FRL-capable connector.
    HDMI21,
    DisplayPort,
    VGA,
    LVDS,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct Connector {
    pub id: u32,
    pub kind: ConnectorType,
    pub connected: bool,
    pub modes: Vec<DisplayMode>,
    pub crtc_id: Option<u32>,
    /// HDMI 2.1 capability cache (filled at connect time from EDID).
    pub hdmi_caps: Option<HdmiCapabilities>,
}

impl Connector {
    /// Attach HDMI 2.1 capabilities after an EDID read.
    pub fn set_hdmi_caps(&mut self, caps: HdmiCapabilities) {
        if caps.version >= HdmiVersion::Hdmi21 {
            self.kind = ConnectorType::HDMI21;
        }
        self.hdmi_caps = Some(caps);
    }

    /// Return `true` if the attached sink supports FRL at or above `rate`.
    pub fn supports_frl(&self, rate: FrlRate) -> bool {
        self.hdmi_caps
            .as_ref()
            .map(|c| c.sink.max_frl_rate >= rate as u8)
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug)]
pub struct Crtc {
    pub id: u32,
    pub mode: Option<DisplayMode>,
    pub fb_id: Option<u32>,
    pub x: u32,
    pub y: u32,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneType {
    Primary,
    Overlay,
    Cursor,
}

#[derive(Clone, Debug)]
pub struct Plane {
    pub id: u32,
    pub kind: PlaneType,
    pub crtc_id: Option<u32>,
    pub fb_id: Option<u32>,
    pub src_x: u32,
    pub src_y: u32,
    pub src_w: u32,
    pub src_h: u32,
    pub crtc_x: i32,
    pub crtc_y: i32,
    pub crtc_w: u32,
    pub crtc_h: u32,
}

#[derive(Clone, Debug)]
pub struct GemBo {
    pub handle: u32,
    pub size: usize,
    pub phys: u64,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub format: PixelFormat,
}

struct DrmDevice {
    crtcs: Vec<Crtc>,
    connectors: Vec<Connector>,
    planes: Vec<Plane>,
    gem_bos: Vec<GemBo>,
    next_handle: u32,
    /// ACP 7.x audio device (None if ACP MMIO base unknown at init time).
    acp: Option<AcpDevice>,
}

static DRM: Mutex<Option<DrmDevice>> = Mutex::new(None);

/// Initialise the DRM subsystem with HDMI 2.1 mode table and ACP audio.
///
/// Registers one CRTC, one HDMI 2.1 connector (with 4K/8K modes), two planes,
/// and — if `acp_mmio_base` is non-zero — initialises the ACP 7.x audio block
/// and binds an HDMI audio endpoint to connector 1.
pub fn init_with_acp(acp_mmio_base: u64) {
    let crtcs = alloc::vec![Crtc {
        id: 1,
        mode: None,
        fb_id: None,
        x: 0,
        y: 0,
        enabled: false,
    }];

    let connectors = alloc::vec![Connector {
        id: 1,
        kind: ConnectorType::HDMI21,
        connected: true,
        modes: alloc::vec![
            // Legacy / HDMI 2.0 modes
            DisplayMode::fullhd_60(),
            DisplayMode::hd_60(),
            DisplayMode::svga_60(),
            // HDMI 2.1 modes
            DisplayMode::uhd4k_60(),
            DisplayMode::uhd4k_120(),
            DisplayMode::uhd4k_144(),
            DisplayMode::uhd8k_60(),
            DisplayMode::qhd_165(),
        ],
        crtc_id: None,
        hdmi_caps: None,
    }];

    let planes = alloc::vec![
        Plane {
            id: 1,
            kind: PlaneType::Primary,
            crtc_id: None,
            fb_id: None,
            src_x: 0,
            src_y: 0,
            src_w: 0,
            src_h: 0,
            crtc_x: 0,
            crtc_y: 0,
            crtc_w: 0,
            crtc_h: 0,
        },
        Plane {
            id: 2,
            kind: PlaneType::Overlay,
            crtc_id: None,
            fb_id: None,
            src_x: 0,
            src_y: 0,
            src_w: 0,
            src_h: 0,
            crtc_x: 0,
            crtc_y: 0,
            crtc_w: 0,
            crtc_h: 0,
        },
    ];

    // Initialise ACP 7.x if a MMIO base was provided.
    let acp = if acp_mmio_base != 0 {
        let mut dev = AcpDevice::new(acp_mmio_base);
        // Best-effort; failures are non-fatal (audio is optional).
        let _ = dev.init();
        // Open an HDMI audio endpoint for connector 1.
        let _ = dev.open_endpoint(AcpEndpointKind::HdmiAudio { connector_id: 1 });
        Some(dev)
    } else {
        None
    };

    *DRM.lock() = Some(DrmDevice {
        crtcs,
        connectors,
        planes,
        gem_bos: Vec::new(),
        next_handle: 1,
        acp,
    });
}

/// Initialise the DRM subsystem without ACP (backwards-compatible entry point).
pub fn init() {
    init_with_acp(0);
}

pub fn is_initialised() -> bool {
    DRM.lock().is_some()
}

/// Initialise display heads. Idempotent wrapper around `init()`.
pub fn init_heads() {
    if !is_initialised() {
        init();
    }
}

/// Number of active display heads (CRTCs).
pub fn num_heads() -> usize {
    DRM.lock().as_ref().map(|d| d.crtcs.len()).unwrap_or(0)
}

/// Update HDMI 2.1 capabilities for `connector_id` from raw EDID bytes.
///
/// Upgrades the connector kind to `HDMI21` if the sink reports FRL support,
/// and adds the 4K/8K mode table entries that require FRL/DSC.
pub fn update_hdmi_caps(connector_id: u32, edid: &[u8]) {
    let caps = HdmiCapabilities::detect_from_edid(edid);
    let mut drm = DRM.lock();
    if let Some(d) = drm.as_mut() {
        if let Some(conn) = d.connectors.iter_mut().find(|c| c.id == connector_id) {
            conn.set_hdmi_caps(caps);
        }
    }
}

/// Allocate a GEM buffer object. Returns handle or None on OOM.
pub fn gem_alloc(width: u32, height: u32, format: PixelFormat) -> Option<u32> {
    let bpp = format.bytes_per_pixel() as u32;
    let pitch = width * bpp;
    let size = (pitch * height) as usize;
    let phys = alloc_dma(size, 4096)?;

    let mut drm = DRM.lock();
    let d = drm.as_mut()?;
    let handle = d.next_handle;
    d.next_handle += 1;
    d.gem_bos.push(GemBo {
        handle,
        size,
        phys,
        width,
        height,
        pitch,
        format,
    });
    Some(handle)
}

/// Free a GEM buffer object by handle.
pub fn gem_free(handle: u32) {
    let mut drm = DRM.lock();
    if let Some(d) = drm.as_mut() {
        d.gem_bos.retain(|bo| bo.handle != handle);
    }
}

/// Get a copy of a GEM BO descriptor.
pub fn gem_info(handle: u32) -> Option<GemBo> {
    DRM.lock()
        .as_ref()?
        .gem_bos
        .iter()
        .find(|b| b.handle == handle)
        .cloned()
}

/// Map a GEM BO into kernel virtual address space (identity-mapped).
pub fn gem_map(handle: u32) -> Option<*mut u32> {
    gem_info(handle).map(|b| b.phys as *mut u32)
}

/// Atomic commit: attach `fb_handle` to `crtc_id` with `mode`.
///
/// For HDMI 2.1 FRL modes (`mode.requires_frl() == true`) the commit
/// verifies that the connected sink reports sufficient FRL bandwidth.
/// Returns `Err("frl_not_supported")` if the negotiation cannot proceed.
pub fn atomic_commit(
    crtc_id: u32,
    connector_id: u32,
    mode: &DisplayMode,
    fb_handle: u32,
) -> Result<(), &'static str> {
    let mut drm = DRM.lock();
    let d = drm.as_mut().ok_or("drm not initialised")?;

    // Verify FRL capability for HDMI 2.1 modes.
    if mode.requires_frl() {
        let conn = d
            .connectors
            .iter()
            .find(|c| c.id == connector_id)
            .ok_or("invalid connector_id")?;
        let min_rate = mode.min_frl_rate();
        if !conn.supports_frl(min_rate) {
            return Err("frl_not_supported");
        }
    }

    // Update CRTC state.
    let crtc = d
        .crtcs
        .iter_mut()
        .find(|c| c.id == crtc_id)
        .ok_or("invalid crtc_id")?;
    crtc.mode = Some(*mode);
    crtc.fb_id = Some(fb_handle);
    crtc.enabled = true;

    if let Some(conn) = d.connectors.iter_mut().find(|c| c.id == connector_id) {
        conn.crtc_id = Some(crtc_id);
    }

    // Update primary plane.
    if let Some(plane) = d.planes.iter_mut().find(|p| p.kind == PlaneType::Primary) {
        plane.crtc_id = Some(crtc_id);
        plane.fb_id = Some(fb_handle);
        plane.src_w = mode.hdisplay as u32;
        plane.src_h = mode.vdisplay as u32;
        plane.crtc_w = mode.hdisplay as u32;
        plane.crtc_h = mode.vdisplay as u32;
    }

    // Delegate scanout to the GPU backend.
    if let Some(bo) = d.gem_bos.iter().find(|b| b.handle == fb_handle) {
        let fb = Framebuffer::from_gem(bo);
        crate::drivers::gpu::gpu::set_scanout(&fb);
    }

    Ok(())
}

pub fn connectors() -> Vec<Connector> {
    DRM.lock()
        .as_ref()
        .map(|d| d.connectors.clone())
        .unwrap_or_default()
}

pub fn crtcs() -> Vec<Crtc> {
    DRM.lock()
        .as_ref()
        .map(|d| d.crtcs.clone())
        .unwrap_or_default()
}

pub fn planes() -> Vec<Plane> {
    DRM.lock()
        .as_ref()
        .map(|d| d.planes.clone())
        .unwrap_or_default()
}

fn alloc_dma(size: usize, align: usize) -> Option<u64> {
    let pages = (size + 0xFFF) / 0x1000;
    let phys = crate::mm::pmm::alloc_pages_aligned(pages, align)?.as_ptr() as u64;
    unsafe {
        core::ptr::write_bytes(phys as *mut u8, 0, pages * 0x1000);
    }
    Some(phys)
}
