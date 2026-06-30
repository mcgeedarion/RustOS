//! Compatibility stub for the VirtIO MMIO network backend.
//!
//! The current tree has a PCI/legacy-port VirtIO network driver in
//! `virtio_net.rs`.  The NIC facade still probes an MMIO backend on platforms
//! that may grow one later, so provide a no-device implementation to keep that
//! facade linkable without pretending a device exists.

use crate::drivers::net::nic::{MacAddr, NicStats};

pub fn is_initialised() -> bool {
    false
}

pub fn mac() -> Option<MacAddr> {
    None
}

pub fn stats() -> Option<NicStats> {
    None
}

pub fn send(_frame: &[u8]) -> Result<(), &'static str> {
    Err("virtio-net-mmio not initialised")
}

pub fn recv(_out: &mut [u8]) -> Option<usize> {
    None
}
