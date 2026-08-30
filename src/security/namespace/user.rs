// src/security/namespace/user.rs
//! User Namespace Implementation
//!
//! Provides isolation of user and group IDs, enabling unprivileged processes
//! to have root capabilities within their namespace while remaining unprivileged
//! outside it.

use crate::sync::SpinLock;
use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU32, Ordering};

/// Unique namespace identifier
static NEXT_NS_ID: AtomicU32 = AtomicU32::new(1);

/// UID/GID mapping entry
#[derive(Debug, Clone)]
pub struct IdMapping {
    pub ns_start: u32,      // Starting ID in the namespace
    pub host_start: u32,    // Starting ID on the host
    pub length: u32,        // Number of IDs to map
}

/// User namespace containing UID/GID mappings and capabilities
pub struct UserNamespace {
    id: u32,
    parent: Option<&'static UserNamespace>,
    
    // UID mappings: container_uid -> host_uid ranges
    uid_mappings: SpinLock<Vec<IdMapping>>,
    
    // GID mappings: container_gid -> host_gid ranges  
    gid_mappings: SpinLock<Vec<IdMapping>>,
    
    // Capabilities granted within this namespace
    capabilities: SpinLock<u64>,
    
    // Root UIDs/GIDs in this namespace
    root_uid: AtomicU32,
    root_gid: AtomicU32,
}

impl UserNamespace {
    /// Create a new user namespace
    pub fn new(parent: Option<&'static UserNamespace>) -> Self {
        let id = NEXT_NS_ID.fetch_add(1, Ordering::Relaxed);
        
        Self {
            id,
            parent,
            uid_mappings: SpinLock::new(Vec::new()),
            gid_mappings: SpinLock::new(Vec::new()),
            capabilities: SpinLock::new(0),
            root_uid: AtomicU32::new(0),
            root_gid: AtomicU32::new(0),
        }
    }

    /// Get the namespace ID
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Get the parent namespace
    pub fn parent(&self) -> Option<&UserNamespace> {
        self.parent
    }

    /// Add a UID mapping
    /// 
    /// Maps [ns_start, ns_start + length) in the namespace to
    /// [host_start, host_start + length) on the host.
    pub fn add_uid_mapping(&self, ns_start: u32, host_start: u32, length: u32) -> Result<(), NamespaceError> {
        if length == 0 || ns_start.checked_add(length).is_none() || host_start.checked_add(length).is_none() {
            return Err(NamespaceError::InvalidMapping);
        }

        let mut mappings = self.uid_mappings.lock();
        
        // Check for overlapping mappings
        for existing in mappings.iter() {
            let existing_end = existing.ns_start + existing.length;
            let new_end = ns_start + length;
            
            if ns_start < existing_end && new_end > existing.ns_start {
                return Err(NamespaceError::MappingOverlap);
            }
        }
        
        mappings.push(IdMapping {
            ns_start,
            host_start,
            length,
        });
        
        Ok(())
    }

    /// Add a GID mapping
    pub fn add_gid_mapping(&self, ns_start: u32, host_start: u32, length: u32) -> Result<(), NamespaceError> {
        if length == 0 || ns_start.checked_add(length).is_none() || host_start.checked_add(length).is_none() {
            return Err(NamespaceError::InvalidMapping);
        }

        let mut mappings = self.gid_mappings.lock();
        
        // Check for overlapping mappings
        for existing in mappings.iter() {
            let existing_end = existing.ns_start + existing.length;
            let new_end = ns_start + length;
            
            if ns_start < existing_end && new_end > existing.ns_start {
                return Err(NamespaceError::MappingOverlap);
            }
        }
        
        mappings.push(IdMapping {
            ns_start,
            host_start,
            length,
        });
        
        Ok(())
    }

    /// Translate a UID from namespace to host
    pub fn map_uid_to_host(&self, ns_uid: u32) -> Result<u32, NamespaceError> {
        let mappings = self.uid_mappings.lock();
        
        for mapping in mappings.iter() {
            if ns_uid >= mapping.ns_start && ns_uid < mapping.ns_start + mapping.length {
                let offset = ns_uid - mapping.ns_start;
                return Ok(mapping.host_start + offset);
            }
        }
        
        // No mapping found - deny access
        Err(NamespaceError::NoMapping)
    }

    /// Translate a GID from namespace to host
    pub fn map_gid_to_host(&self, ns_gid: u32) -> Result<u32, NamespaceError> {
        let mappings = self.gid_mappings.lock();
        
        for mapping in mappings.iter() {
            if ns_gid >= mapping.ns_start && ns_gid < mapping.ns_start + mapping.length {
                let offset = ns_gid - mapping.ns_start;
                return Ok(mapping.host_start + offset);
            }
        }
        
        Err(NamespaceError::NoMapping)
    }

    /// Grant a capability to this namespace
    pub fn grant_capability(&self, cap: Capability) -> Result<(), NamespaceError> {
        let mut caps = self.capabilities.lock();
        *caps |= cap as u64;
        Ok(())
    }

    /// Revoke a capability from this namespace
    pub fn revoke_capability(&self, cap: Capability) -> Result<(), NamespaceError> {
        let mut caps = self.capabilities.lock();
        *caps &= !(cap as u64);
        Ok(())
    }

    /// Check if a capability is granted in this namespace
    pub fn has_capability(&self, cap: Capability) -> bool {
        let caps = self.capabilities.lock();
        (*caps & (cap as u64)) != 0
    }

    /// Set the root UID for this namespace
    pub fn set_root_uid(&self, uid: u32) {
        self.root_uid.store(uid, Ordering::Relaxed);
    }

    /// Set the root GID for this namespace
    pub fn set_root_gid(&self, gid: u32) {
        self.root_gid.store(gid, Ordering::Relaxed);
    }

    /// Get the root UID
    pub fn root_uid(&self) -> u32 {
        self.root_uid.load(Ordering::Relaxed)
    }

    /// Get the root GID
    pub fn root_gid(&self) -> u32 {
        self.root_gid.load(Ordering::Relaxed)
    }

    /// Check if a process with given UID has effective root in this namespace
    pub fn is_effective_root(&self, ns_uid: u32) -> bool {
        ns_uid == self.root_uid.load(Ordering::Relaxed)
    }
}

/// Linux capabilities bitmask
#[repr(u64)]
#[derive(Debug, Clone, Copy)]
pub enum Capability {
    CapChown = 0,
    CapDacOverride = 1,
    CapDacReadSearch = 2,
    CapFowner = 3,
    CapFsetid = 4,
    CapKill = 5,
    CapSetgid = 6,
    CapSetuid = 7,
    CapSetpcap = 8,
    CapLinuxImmutable = 9,
    CapNetBindService = 10,
    CapNetBroadcast = 11,
    CapNetAdmin = 12,
    CapNetRaw = 13,
    CapIpcLock = 14,
    CapIpcOwner = 15,
    CapSysModule = 16,
    CapSysRawio = 17,
    CapSysChroot = 18,
    CapSysPtrace = 19,
    CapSysPacct = 20,
    CapSysAdmin = 21,
    CapSysBoot = 22,
    CapSysNice = 23,
    CapSysResource = 24,
    CapSysTime = 25,
    CapSysTtyConfig = 26,
    CapMknod = 27,
    CapLease = 28,
    CapAuditWrite = 29,
    CapAuditControl = 30,
    CapSetfcap = 31,
    CapMacOverride = 32,
    CapMacAdmin = 33,
    CapSyslog = 34,
    CapWakeAlarm = 35,
    CapBlockSuspend = 36,
    CapAuditRead = 37,
    CapPerfmon = 38,
    CapBpf = 39,
    CapCheckpointRestore = 40,
}

/// User namespace errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamespaceError {
    InvalidMapping,
    MappingOverlap,
    NoMapping,
    PermissionDenied,
    NotSupported,
}

/// Process credentials within a user namespace
pub struct ProcessCredentials {
    pub user_ns: &'static UserNamespace,
    pub uid: AtomicU32,
    pub gid: AtomicU32,
    pub euid: AtomicU32, // Effective UID
    pub egid: AtomicU32, // Effective GID
    pub suid: AtomicU32, // Saved UID
    pub sgid: AtomicU32, // Saved GID
}

impl ProcessCredentials {
    pub fn new(user_ns: &'static UserNamespace, uid: u32, gid: u32) -> Self {
        Self {
            user_ns,
            uid: AtomicU32::new(uid),
            gid: AtomicU32::new(gid),
            euid: AtomicU32::new(uid),
            egid: AtomicU32::new(gid),
            suid: AtomicU32::new(uid),
            sgid: AtomicU32::new(gid),
        }
    }

    /// Check if the process has a specific capability
    pub fn has_capability(&self, cap: Capability) -> bool {
        // Root in the namespace gets all granted capabilities
        if self.euid.load(Ordering::Relaxed) == self.user_ns.root_uid() {
            return self.user_ns.has_capability(cap);
        }
        
        // Non-root has no capabilities by default
        false
    }

    /// Check if the process can perform an operation requiring a capability
    pub fn capable(&self, cap: Capability) -> bool {
        self.has_capability(cap)
    }

    /// Set the effective UID (requires CAP_SETUID or matching real UID)
    pub fn set_euid(&self, new_euid: u32) -> Result<(), NamespaceError> {
        let current_uid = self.uid.load(Ordering::Relaxed);
        let current_euid = self.euid.load(Ordering::Relaxed);
        
        // Allow if:
        // 1. Process is root in namespace
        // 2. Setting to current real UID
        // 3. Setting to current effective UID
        // 4. Has CAP_SETUID
        if current_euid == self.user_ns.root_uid()
            || new_euid == current_uid
            || new_euid == current_euid
            || self.has_capability(Capability::CapSetuid)
        {
            self.euid.store(new_euid, Ordering::Relaxed);
            Ok(())
        } else {
            Err(NamespaceError::PermissionDenied)
        }
    }

    /// Set the real UID (requires CAP_SETUID)
    pub fn set_uid(&self, new_uid: u32) -> Result<(), NamespaceError> {
        if self.has_capability(Capability::CapSetuid) {
            self.uid.store(new_uid, Ordering::Relaxed);
            self.euid.store(new_uid, Ordering::Relaxed);
            self.suid.store(new_uid, Ordering::Relaxed);
            Ok(())
        } else {
            Err(NamespaceError::PermissionDenied)
        }
    }

    /// Map current namespace UID to host UID
    pub fn host_uid(&self) -> Result<u32, NamespaceError> {
        let ns_uid = self.uid.load(Ordering::Relaxed);
        self.user_ns.map_uid_to_host(ns_uid)
    }

    /// Map current namespace GID to host GID
    pub fn host_gid(&self) -> Result<u32, NamespaceError> {
        let ns_gid = self.gid.load(Ordering::Relaxed);
        self.user_ns.map_gid_to_host(ns_gid)
    }
}

/// Global initial user namespace (root namespace)
static mut INITIAL_USER_NS: Option<UserNamespace> = None;

/// Get the initial/root user namespace
pub fn get_initial_user_ns() -> &'static UserNamespace {
    unsafe {
        if INITIAL_USER_NS.is_none() {
            // Create root namespace with all capabilities
            let ns = UserNamespace::new(None);
            
            // Map all UIDs/GIDs 1:1 in root namespace
            ns.add_uid_mapping(0, 0, u32::MAX).ok();
            ns.add_gid_mapping(0, 0, u32::MAX).ok();
            
            // Grant all capabilities to root namespace
            for i in 0..=40u64 {
                ns.grant_capability(core::mem::transmute(i)).ok();
            }
            
            INITIAL_USER_NS = Some(ns);
        }
        INITIAL_USER_NS.as_ref().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uid_mapping() {
        let ns = UserNamespace::new(None);
        
        // Map namespace UIDs 0-99 to host UIDs 1000-1099
        ns.add_uid_mapping(0, 1000, 100).unwrap();
        
        assert_eq!(ns.map_uid_to_host(0).unwrap(), 1000);
        assert_eq!(ns.map_uid_to_host(50).unwrap(), 1050);
        assert_eq!(ns.map_uid_to_host(99).unwrap(), 1099);
        assert!(ns.map_uid_to_host(100).is_err());
    }

    #[test]
    fn test_capability_grant() {
        let ns = UserNamespace::new(None);
        
        assert!(!ns.has_capability(Capability::CapSysAdmin));
        
        ns.grant_capability(Capability::CapSysAdmin).unwrap();
        assert!(ns.has_capability(Capability::CapSysAdmin));
        
        ns.revoke_capability(Capability::CapSysAdmin).unwrap();
        assert!(!ns.has_capability(Capability::CapSysAdmin));
    }

    #[test]
    fn test_process_credentials() {
        let ns = UserNamespace::new(None);
        ns.add_uid_mapping(0, 1000, 100).unwrap();
        ns.set_root_uid(0);
        
        let creds = ProcessCredentials::new(&ns, 0, 0);
        
        // Root should have capabilities
        ns.grant_capability(Capability::CapSetuid).unwrap();
        assert!(creds.has_capability(Capability::CapSetuid));
        
        // Should be able to change EUID
        assert!(creds.set_euid(50).is_ok());
        assert_eq!(creds.euid.load(Ordering::Relaxed), 50);
    }
}
