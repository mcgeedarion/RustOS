//! ACP 7.x (Audio Co-Processor) driver.
//!
//! The ACP is present on AMD APUs/GPUs and exposes:
//!   - I2S / TDM playback and capture endpoints
//!   - S/PDIF transmitter
//!   - DP/HDMI audio stream injection (AV-sync)
//!   - DMA ring buffers (16-entry descriptor rings)
//!   - Internal memory (SRAM) mapped at a fixed MMIO offset
//!
//! ## ACP Versions handled
//! | Version | SoC generation        | Max channels |
//! |---------|-----------------------|--------------|
//! | 6.x     | Renoir / Van Gogh     | 8            |
//! | 7.0     | Phoenix               | 8            |
//! | 7.1     | Hawk Point / Strix    | 16           |
//! | 7.x+    | future                | 16+          |
//!
//! ## Architecture
//!
//! ```text
//! AcpDevice
//!   ├─ [AcpEndpoint; MAX_ENDPOINTS]  (I2S/TDM/SPDIF/HDMI/DP endpoints)
//!   ├─ DmaRing × MAX_ENDPOINTS       (playback + capture rings)
//!   └─ HdmiAudioBridge               (AV-sync to DRM HDMI connector)
//! ```
//!
//! ## Usage
//! ```rust
//! use crate::display::drm::acp::{AcpDevice, AcpEndpointKind};
//! let mut acp = AcpDevice::new(ACP_MMIO_BASE);
//! acp.init().unwrap();
//! let ep = acp.open_endpoint(AcpEndpointKind::HdmiAudio { connector_id: 1 }).unwrap();
//! acp.set_sample_rate(ep, 48_000).unwrap();
//! acp.set_channels(ep, 8).unwrap();
//! acp.start_playback(ep).unwrap();
//! ```

extern crate alloc;
use alloc::vec::Vec;

/// ACP MMIO register offsets relative to the ACP base address.
/// Register widths are 32-bit.
pub mod regs {
    pub const ACP_SOFT_RESET: u32        = 0x0000;
    pub const ACP_CONTROL: u32           = 0x0004;
    pub const ACP_STATUS: u32            = 0x0008;
    pub const ACP_PGFSM_CONTROL: u32     = 0x0010;
    pub const ACP_PGFSM_STATUS: u32      = 0x0014;
    pub const ACP_INTR_STAT: u32         = 0x0020;
    pub const ACP_INTR_MASK: u32         = 0x0024;
    pub const ACP_INTR_CNTL: u32         = 0x0028;

    // I2S / TDM endpoint 0 (base; stride 0x40 per endpoint)
    pub const I2S_TX_FIFO_ADDR: u32      = 0x1000;
    pub const I2S_TX_FIFO_SIZE: u32      = 0x1004;
    pub const I2S_TX_DMA_SIZE: u32       = 0x1008;
    pub const I2S_TX_THRESHOLD: u32      = 0x100C;
    pub const I2S_TX_CONTROL: u32        = 0x1010;

    pub const I2S_RX_FIFO_ADDR: u32      = 0x1040;
    pub const I2S_RX_FIFO_SIZE: u32      = 0x1044;
    pub const I2S_RX_DMA_SIZE: u32       = 0x1048;
    pub const I2S_RX_CONTROL: u32        = 0x1050;

    // HDMI audio
    pub const HDMI_TX_FIFO_ADDR: u32     = 0x2000;
    pub const HDMI_TX_FIFO_SIZE: u32     = 0x2004;
    pub const HDMI_TX_DMA_SIZE: u32      = 0x2008;
    pub const HDMI_TX_CONTROL: u32       = 0x2010;
    pub const HDMI_TX_CHANNEL_STATUS: u32= 0x2020;

    // DP audio
    pub const DP_TX_FIFO_ADDR: u32       = 0x2100;
    pub const DP_TX_FIFO_SIZE: u32       = 0x2104;
    pub const DP_TX_DMA_SIZE: u32        = 0x2108;
    pub const DP_TX_CONTROL: u32         = 0x2110;

    // SPDIF
    pub const SPDIF_TX_FIFO_ADDR: u32    = 0x2200;
    pub const SPDIF_TX_FIFO_SIZE: u32    = 0x2204;
    pub const SPDIF_TX_CONTROL: u32      = 0x2210;

    // Clock / sample-rate
    pub const ACP_I2S_MASTER_BCLK_DIV: u32 = 0x3000;
    pub const ACP_I2S_MASTER_LRCK_DIV: u32 = 0x3004;
    pub const ACP_SCRATCH_REG: u32          = 0x4000;
}

/// ACP version discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AcpVersion {
    Acp6x,
    Acp70,
    Acp71,
    Unknown,
}

impl AcpVersion {
    /// Parse the version from the ACP_STATUS[7:4] HW revision field.
    pub fn from_hw_rev(rev: u8) -> Self {
        match rev {
            0x60..=0x6F => AcpVersion::Acp6x,
            0x70 => AcpVersion::Acp70,
            0x71..=0x7F => AcpVersion::Acp71,
            _ => AcpVersion::Unknown,
        }
    }

    /// Maximum number of I2S/TDM channel slots supported.
    pub fn max_channels(self) -> u8 {
        match self {
            AcpVersion::Acp6x | AcpVersion::Acp70 => 8,
            AcpVersion::Acp71 => 16,
            AcpVersion::Unknown => 2,
        }
    }
}

/// Endpoint kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpEndpointKind {
    /// I2S playback (TDM when channels > 2).
    I2sPlayback,
    /// I2S capture.
    I2sCapture,
    /// S/PDIF output.
    Spdif,
    /// HDMI audio feed to connector `connector_id`.
    HdmiAudio { connector_id: u32 },
    /// DisplayPort audio feed to connector `connector_id`.
    DpAudio { connector_id: u32 },
}

/// Endpoint identifier returned by [`AcpDevice::open_endpoint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcpEndpointId(pub u8);

const MAX_ENDPOINTS: usize = 8;

/// Single DMA descriptor (matches AMD ACP hardware layout).
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct AcpDmaDescriptor {
    /// Physical source address.
    pub src_addr: u32,
    /// Physical destination address.
    pub dst_addr: u32,
    /// Transfer byte count.
    pub count: u32,
    /// Status / control flags.
    pub flags: u32,
}

/// 16-entry DMA descriptor ring for one endpoint direction.
#[derive(Debug)]
pub struct DmaRing {
    pub descs: [AcpDmaDescriptor; 16],
    pub head: usize,
    pub tail: usize,
}

impl Default for DmaRing {
    fn default() -> Self {
        Self {
            descs: [AcpDmaDescriptor::default(); 16],
            head: 0,
            tail: 0,
        }
    }
}

impl DmaRing {
    /// Push a new transfer descriptor. Returns `Err(())` if full.
    pub fn push(&mut self, desc: AcpDmaDescriptor) -> Result<(), ()> {
        let next = (self.tail + 1) % 16;
        if next == self.head {
            return Err(()); // ring full
        }
        self.descs[self.tail] = desc;
        self.tail = next;
        Ok(())
    }

    /// Consume one completed descriptor from the head.
    pub fn pop(&mut self) -> Option<AcpDmaDescriptor> {
        if self.head == self.tail {
            return None;
        }
        let desc = self.descs[self.head];
        self.head = (self.head + 1) % 16;
        Some(desc)
    }

    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }
}

/// One audio endpoint context.
#[derive(Debug)]
pub struct AcpEndpoint {
    pub id: AcpEndpointId,
    pub kind: AcpEndpointKind,
    pub sample_rate: u32,
    pub channels: u8,
    pub bit_depth: u8,
    pub running: bool,
    pub ring: DmaRing,
}

impl AcpEndpoint {
    fn new(id: u8, kind: AcpEndpointKind) -> Self {
        Self {
            id: AcpEndpointId(id),
            kind,
            sample_rate: 48_000,
            channels: 2,
            bit_depth: 16,
            running: false,
            ring: DmaRing::default(),
        }
    }
}

/// ACP 7.x device context.
///
/// In a full kernel driver this would own the MMIO VA obtained from the
/// platform/PCI resource. Here we store the base address and issue typed
/// MMIO accesses via [`mmio_write`] / [`mmio_read`] helpers.
pub struct AcpDevice {
    pub mmio_base: u64,
    pub version: AcpVersion,
    endpoints: Vec<AcpEndpoint>,
    next_ep_id: u8,
}

impl AcpDevice {
    /// Create a new ACP device context. `mmio_base` is the kernel virtual
    /// address of the ACP register block (BAR2 on AMD integrated GPUs).
    pub fn new(mmio_base: u64) -> Self {
        Self {
            mmio_base,
            version: AcpVersion::Unknown,
            endpoints: Vec::new(),
            next_ep_id: 0,
        }
    }

    /// Soft-reset and enable the ACP block, then detect the HW version.
    pub fn init(&mut self) -> Result<(), AcpError> {
        // Assert soft-reset
        self.mmio_write(regs::ACP_SOFT_RESET, 0x1);
        // Deassert soft-reset
        self.mmio_write(regs::ACP_SOFT_RESET, 0x0);
        // Enable clock gating
        self.mmio_write(regs::ACP_CONTROL, 0x1);
        // Wait for PGFSM power-up (in a real driver: poll with timeout)
        let pgfsm = self.mmio_read(regs::ACP_PGFSM_STATUS);
        if pgfsm & 0x3 != 0x0 {
            return Err(AcpError::PowerUpFailed);
        }
        // Read HW revision from status register bits [7:4]
        let status = self.mmio_read(regs::ACP_STATUS);
        self.version = AcpVersion::from_hw_rev(((status >> 4) & 0xFF) as u8);
        Ok(())
    }

    /// Open an audio endpoint. Returns an [`AcpEndpointId`] handle.
    pub fn open_endpoint(&mut self, kind: AcpEndpointKind) -> Result<AcpEndpointId, AcpError> {
        if self.endpoints.len() >= MAX_ENDPOINTS {
            return Err(AcpError::NoEndpointSlots);
        }
        let id = self.next_ep_id;
        self.next_ep_id += 1;
        self.endpoints.push(AcpEndpoint::new(id, kind));
        Ok(AcpEndpointId(id))
    }

    /// Set the sample rate for an endpoint. Reprograms the BCLK/LRCK dividers.
    pub fn set_sample_rate(&mut self, ep: AcpEndpointId, hz: u32) -> Result<(), AcpError> {
        let ep = self.endpoint_mut(ep)?;
        ep.sample_rate = hz;
        // BCLK = sample_rate × channels × bit_depth
        // DIV = ACP_REF_CLK / BCLK  (ACP ref clock = 200 MHz)
        let bclk = hz * ep.channels as u32 * ep.bit_depth as u32;
        let div = 200_000_000u32.checked_div(bclk).unwrap_or(1);
        self.mmio_write(regs::ACP_I2S_MASTER_BCLK_DIV, div);
        self.mmio_write(regs::ACP_I2S_MASTER_LRCK_DIV, div * ep.bit_depth as u32);
        Ok(())
    }

    /// Set the channel count for an endpoint.
    pub fn set_channels(&mut self, ep: AcpEndpointId, channels: u8) -> Result<(), AcpError> {
        if channels > self.version.max_channels() {
            return Err(AcpError::UnsupportedChannelCount);
        }
        self.endpoint_mut(ep)?.channels = channels;
        Ok(())
    }

    /// Set the PCM bit depth (16 or 32 supported).
    pub fn set_bit_depth(&mut self, ep: AcpEndpointId, bits: u8) -> Result<(), AcpError> {
        if bits != 16 && bits != 32 {
            return Err(AcpError::UnsupportedBitDepth);
        }
        self.endpoint_mut(ep)?.bit_depth = bits;
        Ok(())
    }

    /// Queue a DMA buffer for playback / capture on `ep`.
    pub fn queue_buffer(
        &mut self,
        ep: AcpEndpointId,
        phys_addr: u32,
        size: u32,
    ) -> Result<(), AcpError> {
        let ep = self.endpoint_mut(ep)?;
        ep.ring
            .push(AcpDmaDescriptor {
                src_addr: phys_addr,
                dst_addr: self.tx_fifo_addr(ep.kind),
                count: size,
                flags: 0,
            })
            .map_err(|_| AcpError::RingFull)
    }

    /// Start audio streaming on the given endpoint.
    pub fn start_playback(&mut self, ep: AcpEndpointId) -> Result<(), AcpError> {
        let ep = self.endpoint_mut(ep)?;
        if ep.running {
            return Ok(());
        }
        let ctrl_reg = self.ctrl_reg(ep.kind);
        self.mmio_write(ctrl_reg, 0x1);
        ep.running = true;
        Ok(())
    }

    /// Stop audio streaming on the given endpoint.
    pub fn stop_playback(&mut self, ep: AcpEndpointId) -> Result<(), AcpError> {
        let ep = self.endpoint_mut(ep)?;
        if !ep.running {
            return Ok(());
        }
        let ctrl_reg = self.ctrl_reg(ep.kind);
        self.mmio_write(ctrl_reg, 0x0);
        ep.running = false;
        Ok(())
    }

    /// Interrupt handler: drain completed descriptors and re-arm DMA.
    ///
    /// Call this from the ACP IRQ handler (mapped to the GPU interrupt line).
    pub fn handle_irq(&mut self) {
        let stat = self.mmio_read(regs::ACP_INTR_STAT);
        // Acknowledge all pending interrupts
        self.mmio_write(regs::ACP_INTR_STAT, stat);
        // Drain completed descriptors on each running endpoint
        for ep in self.endpoints.iter_mut() {
            if ep.running {
                while ep.ring.pop().is_some() {
                    // In a real driver: call wakeup/completion callback
                }
            }
        }
    }

    // ── private helpers ────────────────────────────────────────────────────

    fn endpoint_mut(&mut self, id: AcpEndpointId) -> Result<&mut AcpEndpoint, AcpError> {
        self.endpoints
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or(AcpError::InvalidEndpoint)
    }

    fn tx_fifo_addr(&self, kind: AcpEndpointKind) -> u32 {
        match kind {
            AcpEndpointKind::I2sPlayback => regs::I2S_TX_FIFO_ADDR,
            AcpEndpointKind::I2sCapture => regs::I2S_RX_FIFO_ADDR,
            AcpEndpointKind::Spdif => regs::SPDIF_TX_FIFO_ADDR,
            AcpEndpointKind::HdmiAudio { .. } => regs::HDMI_TX_FIFO_ADDR,
            AcpEndpointKind::DpAudio { .. } => regs::DP_TX_FIFO_ADDR,
        }
    }

    fn ctrl_reg(&self, kind: AcpEndpointKind) -> u32 {
        match kind {
            AcpEndpointKind::I2sPlayback => regs::I2S_TX_CONTROL,
            AcpEndpointKind::I2sCapture => regs::I2S_RX_CONTROL,
            AcpEndpointKind::Spdif => regs::SPDIF_TX_CONTROL,
            AcpEndpointKind::HdmiAudio { .. } => regs::HDMI_TX_CONTROL,
            AcpEndpointKind::DpAudio { .. } => regs::DP_TX_CONTROL,
        }
    }

    #[inline(always)]
    fn mmio_write(&self, reg: u32, val: u32) {
        let addr = (self.mmio_base + reg as u64) as *mut u32;
        unsafe { core::ptr::write_volatile(addr, val) };
    }

    #[inline(always)]
    fn mmio_read(&self, reg: u32) -> u32 {
        let addr = (self.mmio_base + reg as u64) as *const u32;
        unsafe { core::ptr::read_volatile(addr) }
    }
}

/// Errors produced by the ACP subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpError {
    PowerUpFailed,
    InvalidEndpoint,
    NoEndpointSlots,
    UnsupportedChannelCount,
    UnsupportedBitDepth,
    RingFull,
    HardwareError,
}

impl core::fmt::Display for AcpError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AcpError::PowerUpFailed => write!(f, "ACP: power-up failed (PGFSM not ready)"),
            AcpError::InvalidEndpoint => write!(f, "ACP: invalid endpoint handle"),
            AcpError::NoEndpointSlots => write!(f, "ACP: no free endpoint slots"),
            AcpError::UnsupportedChannelCount => write!(f, "ACP: unsupported channel count"),
            AcpError::UnsupportedBitDepth => write!(f, "ACP: unsupported bit depth"),
            AcpError::RingFull => write!(f, "ACP: DMA ring full"),
            AcpError::HardwareError => write!(f, "ACP: hardware error"),
        }
    }
}
