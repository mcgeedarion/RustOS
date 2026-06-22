//! Media (camera / ISP) subsystem.
//!
//! ## Subsystems
//!   isp4      — AMD ISP4 capture driver (Ryzen AI Max 300 / HP ZBook Ultra)
//!   media     — MediaDevice HAL trait + backend registry
//!
//! The AMD ISP4 driver (`amdisp4`) was merged into Linux 7.2 (June 2026).
//! It uses a firmware-delegated architecture: the kernel driver acts as a
//! high-level messenger, forwarding capture commands to ISP firmware running
//! on the AMD CCPU, rather than directly controlling the image sensor.
//!
//! ## Feature flags
//!   `amd-isp4`  — compile the AMD ISP4 backend (off by default)

pub mod media;

#[cfg(feature = "amd-isp4")]
pub mod isp4;
