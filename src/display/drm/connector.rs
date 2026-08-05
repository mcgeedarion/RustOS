//! Connector management.
//!
//! A connector represents a physical display output port. It reports
//! connection status and the list of modes supported by the attached display.
//!
//! Extends the original connector with HDMI 2.1 capability metadata and an
//! optional ACP audio endpoint binding.

use super::acp::AcpEndpointId;
use super::hdmi::HdmiCapabilities;
use super::DisplayMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorType {
    Unknown,
    Vga,
    DviI,
    DviD,
    DviA,
    Composite,
    Hdmia,
    Hdmib,
    /// HDMI 2.1 (FRL-capable) connector.
    Hdmi21,
    DisplayPort,
    Lvds,
    Component,
    Virtual,
}

impl ConnectorType {
    /// Returns `true` for any HDMI variant (including 2.1).
    pub fn is_hdmi(self) -> bool {
        matches!(
            self,
            ConnectorType::Hdmia | ConnectorType::Hdmib | ConnectorType::Hdmi21
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Unknown,
}

pub struct Connector {
    pub id: u32,
    pub connector_type: ConnectorType,
    pub status: ConnectionStatus,
    pub modes: &'static [DisplayMode],
    /// HDMI 2.1 capabilities (populated after EDID probe).
    pub hdmi_caps: Option<HdmiCapabilities>,
    /// ACP audio endpoint bound to this connector (populated by ACP driver).
    pub acp_audio_ep: Option<AcpEndpointId>,
}

impl Connector {
    pub fn new(
        id: u32,
        connector_type: ConnectorType,
        status: ConnectionStatus,
        modes: &'static [DisplayMode],
    ) -> Self {
        Self {
            id,
            connector_type,
            status,
            modes,
            hdmi_caps: None,
            acp_audio_ep: None,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.status == ConnectionStatus::Connected
    }

    /// Attach parsed HDMI 2.1 capabilities (called after EDID read).
    pub fn set_hdmi_caps(&mut self, caps: HdmiCapabilities) {
        self.hdmi_caps = Some(caps);
    }

    /// Bind an ACP audio endpoint to this connector.
    pub fn bind_acp_endpoint(&mut self, ep: AcpEndpointId) {
        self.acp_audio_ep = Some(ep);
    }

    /// Returns `true` if the sink supports HDMI 2.1 FRL.
    pub fn is_hdmi21_capable(&self) -> bool {
        self.hdmi_caps
            .as_ref()
            .map(|c| c.version >= super::hdmi::HdmiVersion::Hdmi21)
            .unwrap_or(false)
    }
}
