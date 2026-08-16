//! Driver subsystems.
//!
//! ## Subsystems
//!   gpu/      — DRM/KMS, framebuffer, GOP, VGA, virtio-gpu, AMD GEM
//!   input/    — evdev, HID, keyboard, mouse, USB, Bluetooth, virtio-input
//!   net/      — e1000e, NIC abstraction, virtio-net (PCIe)
//!   block/    — AHCI, NVMe, virtio-blk
//!   platform/ — GPIO, PCIe ECAM
//!   virtio/   — MMIO transport and split virtqueue (shared by all virtio drivers)
//!   media/    — Camera / ISP subsystem (AMD ISP4 / amdisp4, merged Linux 7.2)
//!
//! Terminal semantics (line discipline, PTY, termios) live in `crate::tty`.

pub mod block;
pub mod gpu;
// Compatibility re-exports for older call sites that imported GPU drivers
// directly from `crate::drivers::*`.
#[cfg(target_arch = "x86_64")]
pub use gpu::vga;
pub use gpu::{drm, gop, virtio_gpu};
pub mod input;
pub mod keyboard;
pub mod net;
// Compatibility re-export for `crate::drivers::nic::*` callers.
pub use net::nic;
pub mod platform;
// Callers use crate::drivers::pcie; canonical home is platform::pcie.
pub use platform::pcie;
pub mod virtio;
pub mod virtio_blk;

// Camera / ISP subsystem — AMD ISP4 (amdisp4) and MediaDevice HAL.
// Use `crate::drivers::media::media::init_isp4(mmio_base, fw_paddr)` to
// register the backend during kernel init (requires feature "amd-isp4").
pub mod media;
