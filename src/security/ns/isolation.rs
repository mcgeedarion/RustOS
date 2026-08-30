//! Namespace isolation enforcement and validation.
//!
//! This module provides runtime checks and enforcement for namespace isolation,
//! ensuring that processes in different namespaces cannot access each other's
//! resources. It implements the security boundary checks required for production
//! namespace isolation.
//!
//! ## Security Properties Enforced
//!
//! - **PID Isolation**: Processes in child PID namespaces cannot see or signal
//!   processes in parent namespaces
//! - **Mount Isolation**: Mount operations are confined to the namespace
//! - **Network Isolation**: Network sockets and interfaces are namespaced
//! - **User Isolation**: UID/GID mappings prevent privilege escalation
//! - **UTS Isolation**: Hostname changes are namespace-local

extern crate alloc;

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use spin::Mutex;

use super::ns::{NsSet, CLONE_NEWNET, CLONE_NEWNS, CLONE_NEWPID, CLONE_NEWUSER, CLONE_NEWUTS};

/// Global flag indicating whether namespace isolation is enforced
static ISOLATION_ENFORCED: AtomicBool = AtomicBool::new(false);

/// Count of isolation violations detected
static VIOLATION_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Enable namespace isolation enforcement.
/// Should be called early in boot after namespace subsystem initialization.
pub fn enable_isolation_enforcement() {
    ISOLATION_ENFORCED.store(true, Ordering::SeqCst);
    log::info!("namespace: isolation enforcement enabled");
}

/// Check if isolation enforcement is active
#[inline]
pub fn is_enforcement_active() -> bool {
    ISOLATION_ENFORCED.load(Ordering::Acquire)
}

/// Record an isolation violation for auditing
#[inline]
fn record_violation() {
    VIOLATION_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Get the count of detected isolation violations
pub fn get_violation_count() -> usize {
    VIOLATION_COUNT.load(Ordering::Acquire)
}

/// Validate that a process has access to a target namespace.
/// Returns Ok(()) if access is permitted, Err(errno) otherwise.
pub fn validate_namespace_access(
    current_ns: &Arc<NsSet>,
    target_ns: &Arc<NsSet>,
    ns_type: NamespaceType,
) -> Result<(), i32> {
    if !ISOLATION_ENFORCED.load(Ordering::Acquire) {
        return Ok(());
    }

    let access_granted = match ns_type {
        NamespaceType::Pid => Arc::ptr_eq(&current_ns.pid, &target_ns.pid),
        NamespaceType::Mount => Arc::ptr_eq(&current_ns.mnt, &target_ns.mnt),
        NamespaceType::Net => Arc::ptr_eq(&current_ns.net, &target_ns.net),
        NamespaceType::User => check_user_ns_access(&current_ns.user, &target_ns.user),
        NamespaceType::Uts => Arc::ptr_eq(&current_ns.uts, &target_ns.uts),
    };

    if access_granted {
        Ok(())
    } else {
        record_violation();
        Err(-1) // EPERM
    }
}

/// Types of namespaces for access validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamespaceType {
    Pid,
    Mount,
    Net,
    User,
    Uts,
}

impl NamespaceType {
    /// Get the clone flag corresponding to this namespace type
    pub fn clone_flag(self) -> u64 {
        match self {
            NamespaceType::Pid => CLONE_NEWPID,
            NamespaceType::Mount => CLONE_NEWNS,
            NamespaceType::Net => CLONE_NEWNET,
            NamespaceType::User => CLONE_NEWUSER,
            NamespaceType::Uts => CLONE_NEWUTS,
        }
    }
}

/// Check user namespace access with hierarchy awareness.
/// Child user namespaces can access parent resources (with mapping),
/// but not vice versa.
fn check_user_ns_access(current: &Arc<super::ns::user_ns::UserNs>, target: &Arc<super::ns::user_ns::UserNs>) -> bool {
    // Same namespace - always allow
    if Arc::ptr_eq(current, target) {
        return true;
    }

    // Check if current is a descendant of target (child can access parent)
    let mut ancestor = Some(current.clone());
    while let Some(ns) = ancestor.take() {
        if Arc::ptr_eq(&ns, target) {
            return true;
        }
        ancestor = ns.parent();
    }

    false
}

/// Validate PID namespace isolation for signal operations.
/// Prevents processes in child PID namespaces from signaling parent namespace processes.
pub fn validate_pid_signal(sender_ns: &Arc<super::ns::pid_ns::PidNs>, target_ns: &Arc<super::ns::pid_ns::PidNs>) -> Result<(), i32> {
    if !ISOLATION_ENFORCED.load(Ordering::Acquire) {
        return Ok(());
    }

    // Can signal within same namespace
    if Arc::ptr_eq(sender_ns, target_ns) {
        return Ok(());
    }

    // Can signal descendants (parent can signal child)
    let mut ancestor = Some(target_ns.clone());
    while let Some(ns) = ancestor.take() {
        if Arc::ptr_eq(&ns, sender_ns) {
            return Ok(());
        }
        ancestor = ns.parent();
    }

    // Cannot signal ancestors or unrelated namespaces
    record_violation();
    Err(-1) // EPERM
}

/// Validate mount namespace isolation for mount operations.
/// Ensures mounts only affect the current namespace.
pub fn validate_mount_operation(ns: &Arc<super::ns::mnt_ns::MntNs>, target_mnt: Option<&Arc<super::ns::mnt_ns::MntNs>>) -> Result<(), i32> {
    if !ISOLATION_ENFORCED.load(Ordering::Acquire) {
        return Ok(());
    }

    if let Some(target) = target_mnt {
        if !Arc::ptr_eq(ns, target) {
            record_violation();
            return Err(-1); // EPERM
        }
    }

    Ok(())
}

/// Validate network namespace isolation for socket operations.
/// Prevents cross-namespace socket access.
pub fn validate_net_socket_access(socket_ns: &Arc<super::ns::net_ns::NetNs>, process_ns: &Arc<super::ns::net_ns::NetNs>) -> Result<(), i32> {
    if !ISOLATION_ENFORCED.load(Ordering::Acquire) {
        return Ok(());
    }

    if !Arc::ptr_eq(socket_ns, process_ns) {
        record_violation();
        return Err(-1); // EPERM
    }

    Ok(())
}

/// Check if a namespace change would violate isolation policies.
/// Used by setns(2) and unshare(2) syscalls.
pub fn validate_namespace_change(
    current_ns: &NsSet,
    new_ns: &NsSet,
    flags: u64,
) -> Result<(), i32> {
    if !ISOLATION_ENFORCED.load(Ordering::Acquire) {
        return Ok(());
    }

    // Check each namespace type being changed
    if flags & CLONE_NEWPID != 0 {
        // Cannot join a PID namespace after creation
        return Err(-1); // EINVAL
    }

    if flags & CLONE_NEWUSER != 0 {
        // User namespace changes require capability checks
        // For now, enforce strict isolation
        if !check_user_ns_access(&current_ns.user, &new_ns.user) {
            record_violation();
            return Err(-1); // EPERM
        }
    }

    Ok(())
}

/// Namespace isolation statistics for monitoring
#[derive(Debug, Default)]
pub struct IsolationStats {
    pub enforcement_enabled: bool,
    pub violation_count: usize,
    pub pid_namespaces: usize,
    pub mnt_namespaces: usize,
    pub net_namespaces: usize,
    pub uts_namespaces: usize,
    pub user_namespaces: usize,
}

/// Get current isolation statistics
pub fn get_isolation_stats() -> IsolationStats {
    IsolationStats {
        enforcement_enabled: ISOLATION_ENFORCED.load(Ordering::Acquire),
        violation_count: VIOLATION_COUNT.load(Ordering::Acquire),
        // These would be populated from global namespace counters
        pid_namespaces: 0,
        mnt_namespaces: 0,
        net_namespaces: 0,
        uts_namespaces: 0,
        user_namespaces: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isolation_disabled_by_default() {
        assert!(!is_enforcement_active());
    }

    #[test]
    fn test_enable_enforcement() {
        enable_isolation_enforcement();
        assert!(is_enforcement_active());
    }

    #[test]
    fn test_violation_counting() {
        let initial = get_violation_count();
        record_violation();
        assert_eq!(get_violation_count(), initial + 1);
    }

    #[test]
    fn test_namespace_type_flags() {
        assert_eq!(NamespaceType::Pid.clone_flag(), CLONE_NEWPID);
        assert_eq!(NamespaceType::Mount.clone_flag(), CLONE_NEWNS);
        assert_eq!(NamespaceType::Net.clone_flag(), CLONE_NEWNET);
        assert_eq!(NamespaceType::User.clone_flag(), CLONE_NEWUSER);
        assert_eq!(NamespaceType::Uts.clone_flag(), CLONE_NEWUTS);
    }
}
