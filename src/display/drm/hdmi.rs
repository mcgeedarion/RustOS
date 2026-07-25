//! HDMI 2.1 subsystem.
//!
//! Implements the HDMI 2.1 fixed-rate link (FRL) negotiation state machine,
//! SCDC (Status and Control Data Channel) register map, DSC (Display Stream
//! Compression) capability parsing, and the extended EDID CEA-861-H block
//! reader that exposes 4K/8K modes to the DRM connector.
//!
//! ## HDMI 2.1 vs 2.0 differences handled here
//! | Feature                  | HDMI 2.0 | HDMI 2.1 |
//! |--------------------------|----------|----------|
//! | Max bandwidth            | 18 Gb/s  | 48 Gb/s  |
//! | FRL lanes                | –        | 3 or 4   |
//! | DSC support              | –        | optional |
//! | ALLM / VRR / QMS / QFT   | –        | yes      |
//! | eARC                     | –        | yes      |
//!
//! ## Key types
//! - [`HdmiCapabilities`]   – connector-level capability set
//! - [`HdmiVersion`]        – discriminant for 1.x / 2.0 / 2.1
//! - [`FrlRate`]            – FRL lane/rate combination
//! - [`ScdcRegister`]       – typed SCDC register offsets
//! - [`HdmiSinkFeatures`]   – parsed EDID HF-VSDB / HF-SCDB flags
//!
//! ## Usage
//! ```rust
//! use crate::display::drm::hdmi::{HdmiCapabilities, FrlNegotiator};
//!
//! let caps = HdmiCapabilities::detect_from_edid(&edid_bytes);
//! if caps.version == HdmiVersion::Hdmi21 {
//!     let mut neg = FrlNegotiator::new();
//!     neg.start(FrlRate::Frl4Lanes10Gbps);
//! }
//! ```

/// HDMI specification version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HdmiVersion {
    Hdmi14,
    Hdmi20,
    Hdmi21,
}

impl Default for HdmiVersion {
    fn default() -> Self {
        HdmiVersion::Hdmi14
    }
}

/// FRL (Fixed-Rate Link) lane/rate combinations.
///
/// HDMI 2.1 Specification Table 10-1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrlRate {
    /// Legacy TMDS (< 48 Gb/s total)
    Tmds = 0,
    /// 3 lanes × 3 Gb/s = 9 Gb/s
    Frl3Lanes3Gbps = 1,
    /// 3 lanes × 6 Gb/s = 18 Gb/s
    Frl3Lanes6Gbps = 2,
    /// 4 lanes × 6 Gb/s = 24 Gb/s
    Frl4Lanes6Gbps = 3,
    /// 4 lanes × 8 Gb/s = 32 Gb/s
    Frl4Lanes8Gbps = 4,
    /// 4 lanes × 10 Gb/s = 40 Gb/s
    Frl4Lanes10Gbps = 5,
    /// 4 lanes × 12 Gb/s = 48 Gb/s (maximum)
    Frl4Lanes12Gbps = 6,
}

impl FrlRate {
    /// Return total link bandwidth in Mb/s.
    pub fn bandwidth_mbps(self) -> u32 {
        match self {
            FrlRate::Tmds => 18_000,
            FrlRate::Frl3Lanes3Gbps => 9_000,
            FrlRate::Frl3Lanes6Gbps => 18_000,
            FrlRate::Frl4Lanes6Gbps => 24_000,
            FrlRate::Frl4Lanes8Gbps => 32_000,
            FrlRate::Frl4Lanes10Gbps => 40_000,
            FrlRate::Frl4Lanes12Gbps => 48_000,
        }
    }

    /// Number of FRL lanes (0 for TMDS).
    pub fn lanes(self) -> u8 {
        match self {
            FrlRate::Tmds => 0,
            FrlRate::Frl3Lanes3Gbps | FrlRate::Frl3Lanes6Gbps => 3,
            _ => 4,
        }
    }
}

/// SCDC (Status and Control Data Channel) register byte offsets.
///
/// All registers are 8-bit. Defined in HDMI 2.1 Specification §10.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ScdcRegister {
    SinkVersion = 0x01,
    SourceVersion = 0x02,
    UpdateFlags0 = 0x10,
    UpdateFlags1 = 0x11,
    TmdsBitClockRatio = 0x20,
    ConfigControl0 = 0x30,
    StatusFlags0 = 0x40,
    StatusFlags1 = 0x41,
    FrlRate = 0x31,
    TestConfig0 = 0xC0,
}

/// Flags parsed from an HDMI Forum Vendor-Specific Data Block (HF-VSDB) or
/// HF-SCDB in the EDID extension block.
#[derive(Debug, Clone, Copy, Default)]
pub struct HdmiSinkFeatures {
    /// Maximum FRL rate supported by the sink.
    pub max_frl_rate: u8,
    /// Sink supports Display Stream Compression over HDMI.
    pub dsc_support: bool,
    /// Sink supports Auto Low-Latency Mode.
    pub allm: bool,
    /// Sink supports Variable Refresh Rate.
    pub vrr_support: bool,
    /// Sink supports Quick Media Switching.
    pub qms: bool,
    /// Sink supports Quick Frame Transport.
    pub qft: bool,
    /// Sink supports enhanced Audio Return Channel.
    pub earc: bool,
    /// Sink's maximum DSC compressed bpp (0 = unspecified).
    pub dsc_max_bpp: u8,
}

impl HdmiSinkFeatures {
    /// Parse an HF-VSDB block starting at `data[0]`.
    ///
    /// Returns `None` if the header OUI does not match `0xC45DD8`.
    pub fn parse_hf_vsdb(data: &[u8]) -> Option<Self> {
        // Minimum length: 3-byte OUI + version + max FRL + SCDB flags
        if data.len() < 7 {
            return None;
        }
        // HF-VSDB OUI: 0xD8 0x5D 0xC4 (little-endian in EDID)
        if data[0] != 0xD8 || data[1] != 0x5D || data[2] != 0xC4 {
            return None;
        }
        let flags1 = data[5];
        let flags2 = if data.len() > 6 { data[6] } else { 0 };
        Some(Self {
            max_frl_rate: data[4] & 0x3F,
            dsc_support: (flags1 & 0x80) != 0,
            allm: (flags1 & 0x02) != 0,
            vrr_support: (flags2 & 0x10) != 0,
            qms: (flags2 & 0x08) != 0,
            qft: (flags2 & 0x04) != 0,
            earc: (flags1 & 0x20) != 0,
            dsc_max_bpp: if data.len() > 8 { data[8] & 0x7F } else { 0 },
        })
    }
}

/// Full HDMI capability descriptor attached to a DRM connector.
#[derive(Debug, Clone, Default)]
pub struct HdmiCapabilities {
    pub version: HdmiVersion,
    pub sink: HdmiSinkFeatures,
    /// Maximum negotiated FRL rate (updated after link-training).
    pub negotiated_frl_rate: Option<FrlRate>,
    /// Whether SCDC is available on this link.
    pub scdc_present: bool,
}

impl HdmiCapabilities {
    /// Probe capabilities from raw EDID bytes.
    ///
    /// Searches all CEA-861 extension blocks for an HF-VSDB entry.
    pub fn detect_from_edid(edid: &[u8]) -> Self {
        let mut caps = Self::default();
        // Walk 128-byte EDID extension blocks (block 1 onward)
        let mut block = 128usize;
        while block + 128 <= edid.len() {
            let ext = &edid[block..block + 128];
            // CEA/CTA extension tag = 0x02
            if ext[0] == 0x02 {
                caps.parse_cea_block(ext);
            }
            block += 128;
        }
        caps
    }

    fn parse_cea_block(&mut self, block: &[u8]) {
        // Byte 2 = offset to start of 18-byte descriptor sections
        let dtd_offset = block[2] as usize;
        if dtd_offset < 4 {
            return;
        }
        let mut pos = 4usize;
        while pos < dtd_offset {
            let tag = (block[pos] >> 5) & 0x07;
            let len = (block[pos] & 0x1F) as usize;
            if pos + 1 + len > dtd_offset {
                break;
            }
            // Extended tag = 7, sub-tag 0x20 = HF-VSDB
            if tag == 7 && len >= 1 {
                let ext_tag = block[pos + 1];
                if ext_tag == 0x20 {
                    if let Some(feat) =
                        HdmiSinkFeatures::parse_hf_vsdb(&block[pos + 2..pos + 1 + len])
                    {
                        self.sink = feat;
                        self.version = HdmiVersion::Hdmi21;
                        self.scdc_present = true;
                    }
                }
            }
            pos += 1 + len;
        }
    }
}

/// FRL link-training state machine.
///
/// The negotiator iterates from the requested maximum FRL rate downward until
/// the sink acknowledges a stable link (FLT_READY + FRL_START handshake).
#[derive(Debug, Default)]
pub struct FrlNegotiator {
    state: FrlState,
    current_rate: u8,
    retries: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum FrlState {
    #[default]
    Idle,
    SetRate,
    WaitFltReady,
    FrlStart,
    Active,
    Failed,
}

impl FrlNegotiator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin negotiation at `rate`. Call [`step`] periodically until
    /// [`is_active`] or [`has_failed`] returns true.
    pub fn start(&mut self, rate: FrlRate) {
        self.current_rate = rate as u8;
        self.retries = 0;
        self.state = FrlState::SetRate;
    }

    /// Advance the state machine one step.
    ///
    /// In a real driver this is invoked from the SCDC I²C interrupt handler
    /// or a polled HDMI hot-plug IRQ workqueue.
    pub fn step(&mut self, flt_ready: bool, frl_start_ack: bool) {
        match self.state {
            FrlState::SetRate => {
                // Write FRL_RATE to SCDC then wait for FLT_READY
                self.state = FrlState::WaitFltReady;
            },
            FrlState::WaitFltReady => {
                if flt_ready {
                    self.state = FrlState::FrlStart;
                } else {
                    self.retries += 1;
                    if self.retries > 10 {
                        if self.current_rate > 1 {
                            self.current_rate -= 1;
                            self.retries = 0;
                            self.state = FrlState::SetRate;
                        } else {
                            self.state = FrlState::Failed;
                        }
                    }
                }
            },
            FrlState::FrlStart => {
                // Assert FRL_START in SCDC CONFIG_CONTROL_0
                if frl_start_ack {
                    self.state = FrlState::Active;
                }
            },
            _ => {},
        }
    }

    pub fn is_active(&self) -> bool {
        self.state == FrlState::Active
    }

    pub fn has_failed(&self) -> bool {
        self.state == FrlState::Failed
    }

    /// Return the currently negotiated FRL rate (only valid when active).
    pub fn active_rate(&self) -> Option<FrlRate> {
        if self.state == FrlState::Active {
            Some(match self.current_rate {
                1 => FrlRate::Frl3Lanes3Gbps,
                2 => FrlRate::Frl3Lanes6Gbps,
                3 => FrlRate::Frl4Lanes6Gbps,
                4 => FrlRate::Frl4Lanes8Gbps,
                5 => FrlRate::Frl4Lanes10Gbps,
                6 => FrlRate::Frl4Lanes12Gbps,
                _ => FrlRate::Tmds,
            })
        } else {
            None
        }
    }
}
