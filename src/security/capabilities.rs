// src/security/capabilities.rs
//! Linux Capabilities Implementation
//!
//! Provides fine-grained privilege control, allowing processes to have
//! specific privileges without full root access.

use crate::sync::SpinLock;
use core::sync::atomic::{AtomicU64, Ordering};

/// Capability sets per POSIX/Linux model
#[derive(Debug, Clone)]
pub struct CapabilitySet {
    /// Permitted capabilities (maximum available)
    pub permitted: AtomicU64,
    /// Effective capabilities (currently active)
    pub effective: AtomicU64,
    /// Inheritable capabilities (passed to children)
    pub inheritable: AtomicU64,
    /// Bounding set (limits what can be added to permitted)
    pub bounding: AtomicU64,
    /// Ambient capabilities (preserved across execve)
    pub ambient: AtomicU64,
}

impl CapabilitySet {
    /// Create a new capability set with no capabilities
    pub fn empty() -> Self {
        Self {
            permitted: AtomicU64::new(0),
            effective: AtomicU64::new(0),
            inheritable: AtomicU64::new(0),
            bounding: AtomicU64::new(u64::MAX), // Initially all allowed
            ambient: AtomicU64::new(0),
        }
    }

    /// Create a capability set with all capabilities (root)
    pub fn full() -> Self {
        let full = u64::MAX; // All 64 bits set
        Self {
            permitted: AtomicU64::new(full),
            effective: AtomicU64::new(full),
            inheritable: AtomicU64::new(full),
            bounding: AtomicU64::new(full),
            ambient: AtomicU64::new(full),
        }
    }

    /// Check if a capability is in the effective set
    pub fn has_effective(&self, cap: Capability) -> bool {
        let mask = 1u64 << (cap as u64);
        (self.effective.load(Ordering::Relaxed) & mask) != 0
    }

    /// Check if a capability is in the permitted set
    pub fn has_permitted(&self, cap: Capability) -> bool {
        let mask = 1u64 << (cap as u64);
        (self.permitted.load(Ordering::Relaxed) & mask) != 0
    }

    /// Check if a capability is in the inheritable set
    pub fn has_inheritable(&self, cap: Capability) -> bool {
        let mask = 1u64 << (cap as u64);
        (self.inheritable.load(Ordering::Relaxed) & mask) != 0
    }

    /// Add a capability to the effective set (must be in permitted)
    pub fn add_effective(&self, cap: Capability) -> Result<(), CapabilityError> {
        if !self.has_permitted(cap) {
            return Err(CapabilityError::NotPermitted);
        }
        
        let mask = 1u64 << (cap as u64);
        self.effective.fetch_or(mask, Ordering::Relaxed);
        Ok(())
    }

    /// Remove a capability from the effective set
    pub fn remove_effective(&self, cap: Capability) -> Result<(), CapabilityError> {
        let mask = !(1u64 << (cap as u64));
        self.effective.fetch_and(mask, Ordering::Relaxed);
        Ok(())
    }

    /// Add a capability to the permitted set (must be in bounding)
    pub fn add_permitted(&self, cap: Capability) -> Result<(), CapabilityError> {
        if !self.has_bounding(cap) {
            return Err(CapabilityError::NotInBounding);
        }
        
        let mask = 1u64 << (cap as u64);
        self.permitted.fetch_or(mask, Ordering::Relaxed);
        Ok(())
    }

    /// Remove a capability from permitted (also removes from effective)
    pub fn remove_permitted(&self, cap: Capability) -> Result<(), CapabilityError> {
        let mask = !(1u64 << (cap as u64));
        self.permitted.fetch_and(mask, Ordering::Relaxed);
        self.remove_effective(cap).ok(); // Also remove from effective
        Ok(())
    }

    /// Add a capability to the inheritable set (must be in bounding)
    pub fn add_inheritable(&self, cap: Capability) -> Result<(), CapabilityError> {
        if !self.has_bounding(cap) {
            return Err(CapabilityError::NotInBounding);
        }
        
        let mask = 1u64 << (cap as u64);
        self.inheritable.fetch_or(mask, Ordering::Relaxed);
        Ok(())
    }

    /// Remove a capability from inheritable
    pub fn remove_inheritable(&self, cap: Capability) -> Result<(), CapabilityError> {
        let mask = !(1u64 << (cap as u64));
        self.inheritable.fetch_and(mask, Ordering::Relaxed);
        Ok(())
    }

    /// Drop a capability from the bounding set (irreversible)
    pub fn drop_bounding(&self, cap: Capability) -> Result<(), CapabilityError> {
        let mask = !(1u64 << (cap as u64));
        self.bounding.fetch_and(mask, Ordering::Relaxed);
        
        // Also remove from other sets
        self.remove_permitted(cap).ok();
        self.remove_inheritable(cap).ok();
        
        Ok(())
    }

    /// Check if a capability is in the bounding set
    pub fn has_bounding(&self, cap: Capability) -> bool {
        let mask = 1u64 << (cap as u64);
        (self.bounding.load(Ordering::Relaxed) & mask) != 0
    }

    /// Set the effective set to match permitted (common operation)
    pub fn set_full_effective(&self) {
        let permitted = self.permitted.load(Ordering::Relaxed);
        self.effective.store(permitted, Ordering::Relaxed);
    }

    /// Clear all capabilities
    pub fn clear(&self) {
        self.permitted.store(0, Ordering::Relaxed);
        self.effective.store(0, Ordering::Relaxed);
        self.inheritable.store(0, Ordering::Relaxed);
        self.ambient.store(0, Ordering::Relaxed);
    }

    /// Apply capability restrictions for dropping privileges
    pub fn drop_to(&self, caps: &[Capability]) -> Result<(), CapabilityError> {
        // First, calculate the new capability mask
        let mut new_mask: u64 = 0;
        for cap in caps {
            new_mask |= 1u64 << (*cap as u64);
        }
        
        // Verify all requested caps are in bounding set
        let bounding = self.bounding.load(Ordering::Relaxed);
        if (new_mask & !bounding) != 0 {
            return Err(CapabilityError::NotInBounding);
        }
        
        // Update sets
        self.permitted.store(new_mask, Ordering::Relaxed);
        self.effective.store(new_mask, Ordering::Relaxed);
        self.inheritable.store(new_mask, Ordering::Relaxed);
        
        Ok(())
    }

    /// Check if process has any capabilities
    pub fn is_privileged(&self) -> bool {
        self.effective.load(Ordering::Relaxed) != 0
    }

    /// Get the raw capability bitmask for effective set
    pub fn effective_raw(&self) -> u64 {
        self.effective.load(Ordering::Relaxed)
    }
}

/// Linux capabilities enumeration
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    CapChown = 0,           // Change file ownership
    CapDacOverride = 1,     // Bypass DAC checks
    CapDacReadSearch = 2,   // Bypass DAC read/search
    CapFowner = 3,          // Bypass permission checks on operations requiring file owner
    CapFsetid = 4,          // Allow setting setuid/setgid bits
    CapKill = 5,            // Send signals to any process
    CapSetgid = 6,          // Set GID
    CapSetuid = 7,          // Set UID
    CapSetpcap = 8,         // Set process capabilities
    CapLinuxImmutable = 9,  // Modify immutable files
    CapNetBindService = 10, // Bind to privileged ports (<1024)
    CapNetBroadcast = 11,   // Network broadcast
    CapNetAdmin = 12,       // Network administration
    CapNetRaw = 13,         // Use RAW sockets
    CapIpcLock = 14,        // Lock memory
    CapIpcOwner = 15,       // Bypass IPC permission checks
    CapSysModule = 16,      // Load/unload kernel modules
    CapSysRawio = 17,       // Raw I/O access
    CapSysChroot = 18,      // Use chroot
    CapSysPtrace = 19,      // Trace any process
    CapSysPacct = 20,       // Process accounting
    CapSysAdmin = 21,       // System administration (catch-all)
    CapSysBoot = 22,        // Reboot system
    CapSysNice = 23,        // Set nice value for any process
    CapSysResource = 24,    // Override resource limits
    CapSysTime = 25,        // Set system clock
    CapSysTtyConfig = 26,   // TTY configuration
    CapMknod = 27,          // Create special files
    CapLease = 28,          // File leases
    CapAuditWrite = 29,     // Write audit log
    CapAuditControl = 30,   // Configure audit subsystem
    CapSetfcap = 31,        // Set file capabilities
    CapMacOverride = 32,    // Override MAC settings
    CapMacAdmin = 33,       // MAC administration
    CapSyslog = 34,         // Syslog operations
    CapWakeAlarm = 35,      // Trigger wake alarms
    CapBlockSuspend = 36,   // Block system suspend
    CapAuditRead = 37,      // Read audit log
    CapPerfmon = 38,        // Performance monitoring
    CapBpf = 39,            // BPF operations
    CapCheckpointRestore = 40, // Checkpoint/restore
}

/// Capability errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityError {
    NotPermitted,
    NotInBounding,
    InvalidCapability,
    OperationNotAllowed,
}

/// Check if current process has a specific capability
pub fn capable(cap: Capability) -> bool {
    // In real implementation, would check current task's capability set
    // For now, assume we're checking against a global or thread-local set
    unsafe {
        if let Some(current_caps) = CURRENT_CAPS.as_ref() {
            return current_caps.has_effective(cap);
        }
    }
    false
}

/// Require a capability or return error
pub fn require_capability(cap: Capability) -> Result<(), CapabilityError> {
    if capable(cap) {
        Ok(())
    } else {
        Err(CapabilityError::OperationNotAllowed)
    }
}

/// Global current process capabilities (thread-local in real impl)
static mut CURRENT_CAPS: Option<CapabilitySet> = None;

/// Initialize capabilities for current process
pub fn init_capabilities(full: bool) {
    unsafe {
        CURRENT_CAPS = Some(if full {
            CapabilitySet::full()
        } else {
            CapabilitySet::empty()
        });
    }
}

/// Get current capability set
pub fn get_current_capabilities() -> Option<&'static CapabilitySet> {
    unsafe { CURRENT_CAPS.as_ref() }
}

/// Drop all capabilities (for privilege separation)
pub fn drop_all_capabilities() -> Result<(), CapabilityError> {
    unsafe {
        if let Some(ref caps) = CURRENT_CAPS {
            caps.clear();
            Ok(())
        } else {
            Err(CapabilityError::OperationNotAllowed)
        }
    }
}

/// Drop to specific capabilities
pub fn drop_to_capabilities(caps: &[Capability]) -> Result<(), CapabilityError> {
    unsafe {
        if let Some(ref current) = CURRENT_CAPS {
            current.drop_to(caps)
        } else {
            Err(CapabilityError::OperationNotAllowed)
        }
    }
}

/// Common capability sets for different use cases
pub mod presets {
    use super::*;

    /// Capabilities needed for network operations
    pub const NETWORK_CAPS: &[Capability] = &[
        Capability::CapNetBindService,
        Capability::CapNetBroadcast,
        Capability::CapNetAdmin,
        Capability::CapNetRaw,
    ];

    /// Capabilities needed for system administration (minimal)
    pub const SYS_ADMIN_MINIMAL: &[Capability] = &[
        Capability::CapSysAdmin,
        Capability::CapSysPtrace,
        Capability::CapSysModule,
        Capability::CapSysRawio,
    ];

    /// Capabilities for container runtime
    pub const CONTAINER_CAPS: &[Capability] = &[
        Capability::CapSetuid,
        Capability::CapSetgid,
        Capability::CapSetpcap,
        Capability::CapNetBindService,
        Capability::CapSysChroot,
        Capability::CapMknod,
    ];

    /// Capabilities for auditing
    pub const AUDIT_CAPS: &[Capability] = &[
        Capability::CapAuditWrite,
        Capability::CapAuditControl,
        Capability::CapAuditRead,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_set_creation() {
        let empty = CapabilitySet::empty();
        assert!(!empty.has_effective(Capability::CapSysAdmin));
        
        let full = CapabilitySet::full();
        assert!(full.has_effective(Capability::CapSysAdmin));
    }

    #[test]
    fn test_add_remove_capability() {
        let caps = CapabilitySet::empty();
        
        // Can't add to effective without being in permitted
        assert!(caps.add_effective(Capability::CapSysAdmin).is_err());
        
        // Add to permitted first
        caps.add_permitted(Capability::CapSysAdmin).unwrap();
        caps.add_effective(Capability::CapSysAdmin).unwrap();
        
        assert!(caps.has_effective(Capability::CapSysAdmin));
        
        // Remove from effective
        caps.remove_effective(Capability::CapSysAdmin).unwrap();
        assert!(!caps.has_effective(Capability::CapSysAdmin));
        assert!(caps.has_permitted(Capability::CapSysAdmin));
    }

    #[test]
    fn test_drop_bounding() {
        let caps = CapabilitySet::full();
        
        // Drop CAP_SYS_MODULE from bounding set
        caps.drop_bounding(Capability::CapSysModule).unwrap();
        
        // Should not be able to add it back
        assert!(caps.add_permitted(Capability::CapSysModule).is_err());
    }

    #[test]
    fn test_drop_to_specific_caps() {
        let caps = CapabilitySet::full();
        
        // Drop to only network capabilities
        caps.drop_to(presets::NETWORK_CAPS).unwrap();
        
        // Should have network caps
        assert!(caps.has_effective(Capability::CapNetBindService));
        
        // Should not have admin caps
        assert!(!caps.has_effective(Capability::CapSysAdmin));
    }
}
