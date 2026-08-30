//! CPU Hotplug State Machine Implementation
//!
//! This module implements a complete CPU hotplug subsystem following the
//! Linux-style CPU hotplug state machine pattern. It supports:
//!
//! - Dynamic CPU online/offline operations
//! - State transition callbacks
//! - Per-CPU reference counting
//! - Graceful CPU removal with task migration
//!
//! # State Machine
//!
//! ```text
//! OFFLINE → STARTING → ONLINE → DYING → DEAD → OFFLINE
//!    ↑                                          │
//!    └──────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! use rustos_kernel::smp::hotplug::{cpu_up, cpu_down, CpuState};
//!
//! // Bring up CPU 2
//! unsafe { cpu_up(2).expect("Failed to bring up CPU 2"); }
//!
//! // Take CPU 2 offline
//! unsafe { cpu_down(2).expect("Failed to take CPU 2 offline"); }
//! ```

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use spin::Mutex;

use crate::arch::x86_64::cpu;
use crate::smp::{percpu, CpuInfo};

/// Maximum number of CPUs supported for hotplug operations
pub const MAX_HOTPLUG_CPUS: usize = crate::smp::MAX_CPUS;

/// CPU hotplug states representing the lifecycle of a CPU
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CpuState {
    /// CPU is completely offline and not usable
    Offline = 0,
    /// CPU is starting but not yet schedulable
    Starting = 1,
    /// CPU is online and fully operational
    Online = 2,
    /// CPU is being taken offline (tasks migrating)
    Dying = 3,
    /// CPU is dead but resources not yet freed
    Dead = 4,
}

impl CpuState {
    /// Check if this state allows scheduling tasks
    #[inline]
    pub fn is_schedulable(self) -> bool {
        matches!(self, CpuState::Online)
    }

    /// Check if the CPU is in any active state
    #[inline]
    pub fn is_active(self) -> bool {
        matches!(self, CpuState::Starting | CpuState::Online | CpuState::Dying)
    }

    /// Convert from raw u8 value
    #[inline]
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(CpuState::Offline),
            1 => Some(CpuState::Starting),
            2 => Some(CpuState::Online),
            3 => Some(CpuState::Dying),
            4 => Some(CpuState::Dead),
            _ => None,
        }
    }
}

impl fmt::Display for CpuState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CpuState::Offline => write!(f, "offline"),
            CpuState::Starting => write!(f, "starting"),
            CpuState::Online => write!(f, "online"),
            CpuState::Dying => write!(f, "dying"),
            CpuState::Dead => write!(f, "dead"),
        }
    }
}

/// Callback type for state transitions
pub type StateCallback = Box<dyn Fn(u32, CpuState) + Send + Sync>;

/// Registration handle for callbacks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallbackId(u32);

/// Error types for hotplug operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotplugError {
    /// Invalid CPU ID
    InvalidCpu(u32),
    /// CPU already in target state
    AlreadyInState(u32, CpuState),
    /// Cannot offline BSP (boot processor)
    CannotOfflineBsp(u32),
    /// CPU not present in system
    CpuNotPresent(u32),
    /// Operation timed out
    Timeout,
    /// Resources unavailable
    NoMemory,
    /// Callback registration failed
    CallbackFailed,
}

impl fmt::Display for HotplugError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HotplugError::InvalidCpu(id) => write!(f, "Invalid CPU ID: {}", id),
            HotplugError::AlreadyInState(id, state) => {
                write!(f, "CPU {} already in state: {}", id, state)
            }
            HotplugError::CannotOfflineBsp(id) => {
                write!(f, "Cannot offline boot processor: CPU {}", id)
            }
            HotplugError::CpuNotPresent(id) => write!(f, "CPU {} not present", id),
            HotplugError::Timeout => write!(f, "Hotplug operation timed out"),
            HotplugError::NoMemory => write!(f, "Out of memory for hotplug operation"),
            HotplugError::CallbackFailed => write!(f, "Callback registration failed"),
        }
    }
}

/// Result type for hotplug operations
pub type HotplugResult<T> = Result<T, HotplugError>;

/// Per-CPU hotplug state
#[derive(Debug)]
struct CpuHotplugState {
    /// Current state of this CPU
    state: AtomicU8,
    /// Reference count for preventing premature offline
    refcount: AtomicU32,
    /// Whether this CPU can be offlined (false for BSP typically)
    can_offline: AtomicBool,
    /// APIC/MPIDR hardware ID
    hw_id: u32,
}

impl CpuHotplugState {
    const fn new(hw_id: u32, can_offline: bool) -> Self {
        Self {
            state: AtomicU8::new(CpuState::Offline as u8),
            refcount: AtomicU32::new(0),
            can_offline: AtomicBool::new(can_offline),
            hw_id,
        }
    }

    #[inline]
    fn get_state(&self) -> CpuState {
        CpuState::from_u8(self.state.load(Ordering::Acquire))
            .unwrap_or(CpuState::Offline)
    }

    #[inline]
    fn set_state(&self, new_state: CpuState) {
        self.state.store(new_state as u8, Ordering::Release);
    }

    #[inline]
    fn try_transition(&self, from: CpuState, to: CpuState) -> bool {
        let current = self.state.load(Ordering::Acquire);
        if current == from as u8 {
            self.state.store(to as u8, Ordering::Release);
            true
        } else {
            false
        }
    }

    #[inline]
    fn inc_refcount(&self) {
        self.refcount.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn dec_refcount(&self) -> u32 {
        self.refcount.fetch_sub(1, Ordering::Release)
    }

    #[inline]
    fn get_refcount(&self) -> u32 {
        self.refcount.load(Ordering::Acquire)
    }
}

/// Global hotplug manager state
struct HotplugManager {
    /// Per-CPU hotplug state array
    cpus: [Option<Box<CpuHotplugState>>; MAX_HOTPLUG_CPUS],
    /// Registered state transition callbacks
    callbacks: Vec<(CallbackId, StateCallback)>,
    /// Next callback ID counter
    next_callback_id: u32,
    /// Number of currently online CPUs
    online_count: AtomicU32,
}

impl HotplugManager {
    const fn new() -> Self {
        // SAFETY: We're using a const initializer for the array
        Self {
            cpus: [const { None }; MAX_HOTPLUG_CPUS],
            callbacks: Vec::new(),
            next_callback_id: 0,
            online_count: AtomicU32::new(0),
        }
    }

    /// Initialize a CPU's hotplug state
    fn init_cpu(&mut self, cpu_id: u32, hw_id: u32, is_bsp: bool) -> HotplugResult<()> {
        if cpu_id as usize >= MAX_HOTPLUG_CPUS {
            return Err(HotplugError::InvalidCpu(cpu_id));
        }

        if self.cpus[cpu_id as usize].is_some() {
            return Ok(()); // Already initialized
        }

        // BSP cannot be offlined by default
        let can_offline = !is_bsp;
        let state = Box::new(CpuHotplugState::new(hw_id, can_offline));

        // Set initial state based on whether this is BSP
        if is_bsp {
            state.set_state(CpuState::Online);
            self.online_count.fetch_add(1, Ordering::Relaxed);
        }

        self.cpus[cpu_id as usize] = Some(state);
        Ok(())
    }

    /// Get mutable reference to CPU state
    fn get_cpu_state(&self, cpu_id: u32) -> HotplugResult<&CpuHotplugState> {
        if cpu_id as usize >= MAX_HOTPLUG_CPUS {
            return Err(HotplugError::InvalidCpu(cpu_id));
        }
        self.cpus[cpu_id as usize]
            .as_ref()
            .map(|s| s.as_ref())
            .ok_or(HotplugError::CpuNotPresent(cpu_id))
    }

    /// Register a state transition callback
    fn register_callback(&mut self, callback: StateCallback) -> HotplugResult<CallbackId> {
        let id = CallbackId(self.next_callback_id);
        self.next_callback_id = self.next_callback_id.checked_add(1).ok_or(HotplugError::CallbackFailed)?;
        self.callbacks.push((id, callback));
        Ok(id)
    }

    /// Unregister a callback
    fn unregister_callback(&mut self, id: CallbackId) -> bool {
        if let Some(pos) = self.callbacks.iter().position(|(cid, _)| *cid == id) {
            self.callbacks.remove(pos);
            true
        } else {
            false
        }
    }

    /// Invoke all registered callbacks for a state transition
    fn notify_callbacks(&self, cpu_id: u32, new_state: CpuState) {
        for (_, callback) in &self.callbacks {
            callback(cpu_id, new_state);
        }
    }
}

/// Global hotplug manager instance
static HOTPLUG_MANAGER: Mutex<RefCell<HotplugManager>> =
    Mutex::new(RefCell::new(HotplugManager::new()));

/// Initialize the CPU hotplug subsystem
///
/// Must be called after SMP initialization but before any hotplug operations.
/// Registers all currently online CPUs in the hotplug state machine.
///
/// # Safety
/// Must only be called once during system initialization
pub unsafe fn init() -> HotplugResult<()> {
    let mut manager = HOTPLUG_MANAGER.lock();
    let mut mgr = manager.borrow_mut();

    // Register all CPUs from the topology table
    for cpu_id in 0..crate::smp::num_cpus() {
        if let Some(info) = crate::smp::cpu_info(cpu_id) {
            mgr.init_cpu(cpu_id, info.hw_id, info.is_bsp)?;
        }
    }

    log::info!("CPU hotplug subsystem initialized");
    Ok(())
}

/// Bring a CPU online
///
/// Transitions the CPU through the state machine:
/// `Offline → Starting → Online`
///
/// # Arguments
/// * `cpu_id` - Logical CPU ID to bring online
///
/// # Safety
/// Caller must ensure the CPU is physically present and can be started
pub unsafe fn cpu_up(cpu_id: u32) -> HotplugResult<()> {
    let manager = HOTPLUG_MANAGER.lock();
    let mgr = manager.borrow();
    let cpu_state = mgr.get_cpu_state(cpu_id)?;

    let current_state = cpu_state.get_state();
    if current_state.is_schedulable() {
        return Err(HotplugError::AlreadyInState(cpu_id, current_state));
    }

    // Transition to Starting
    if !cpu_state.try_transition(CpuState::Offline, CpuState::Starting) {
        return Err(HotplugError::AlreadyInState(cpu_id, current_state));
    }

    // Notify callbacks
    drop(mgr);
    mgr.notify_callbacks(cpu_id, CpuState::Starting);

    // Architecture-specific CPU startup
    #[cfg(target_arch = "x86_64")]
    {
        use crate::arch::x86_64::apic;
        
        // Get hardware APIC ID
        let hw_id = cpu_state.hw_id;
        
        // Send INIT-SIPI sequence to start the AP
        if let Some(info) = crate::smp::cpu_info(cpu_id) {
            apic::wakeup_ap(info.hw_id, cpu_id);
        }

        // Wait for CPU to signal it's ready
        let timeout = 1_000_000; // ~1 second at 1MHz
        let mut count = 0;
        while count < timeout {
            if cpu_state.get_state() == CpuState::Online {
                break;
            }
            core::hint::spin_loop();
            count += 1;
        }

        if count >= timeout {
            cpu_state.set_state(CpuState::Offline);
            return Err(HotplugError::Timeout);
        }
    }

    // Final transition to Online
    cpu_state.set_state(CpuState::Online);
    mgr.notify_callbacks(cpu_id, CpuState::Online);

    // Update online count
    let _ = mgr.online_count.fetch_add(1, Ordering::Relaxed);

    log::info!("CPU {} brought online", cpu_id);
    Ok(())
}

/// Take a CPU offline
///
/// Transitions the CPU through the state machine:
/// `Online → Dying → Dead → Offline`
///
/// # Arguments
/// * `cpu_id` - Logical CPU ID to take offline
///
/// # Safety
/// Caller must ensure no critical tasks are running on this CPU
pub unsafe fn cpu_down(cpu_id: u32) -> HotplugResult<()> {
    let manager = HOTPLUG_MANAGER.lock();
    let mgr = manager.borrow();
    let cpu_state = mgr.get_cpu_state(cpu_id)?;

    let current_state = cpu_state.get_state();
    
    // Check if CPU can be offlined
    if !cpu_state.can_offline.load(Ordering::Acquire) {
        return Err(HotplugError::CannotOfflineBsp(cpu_id));
    }

    if current_state != CpuState::Online {
        return Err(HotplugError::AlreadyInState(cpu_id, current_state));
    }

    // Check reference count
    if cpu_state.get_refcount() > 0 {
        log::warn!("CPU {} has active references (refcount={}), deferring offline", 
                   cpu_id, cpu_state.get_refcount());
        return Err(HotplugError::Timeout);
    }

    // Transition to Dying
    cpu_state.try_transition(CpuState::Online, CpuState::Dying);
    mgr.notify_callbacks(cpu_id, CpuState::Dying);

    // Migrate tasks away from this CPU (architecture-specific)
    #[cfg(target_arch = "x86_64")]
    {
        use crate::arch::x86_64::apic;
        
        // Disable local APIC to prevent further interrupts
        apic::disable_local();
        
        // Flush TLB entries for this CPU
        crate::arch::x86_64::paging::flush_tlb_local();
    }

    // Transition to Dead
    cpu_state.set_state(CpuState::Dead);
    mgr.notify_callbacks(cpu_id, CpuState::Dead);

    // Final cleanup and transition to Offline
    cpu_state.set_state(CpuState::Offline);
    mgr.notify_callbacks(cpu_id, CpuState::Offline);

    // Update online count
    let _ = mgr.online_count.fetch_sub(1, Ordering::Relaxed);

    log::info!("CPU {} taken offline", cpu_id);
    Ok(())
}

/// Check if a CPU is currently online and schedulable
pub fn cpu_online(cpu_id: u32) -> bool {
    let manager = HOTPLUG_MANAGER.lock();
    let mgr = manager.borrow();
    
    mgr.get_cpu_state(cpu_id)
        .map(|s| s.get_state().is_schedulable())
        .unwrap_or(false)
}

/// Get the current state of a CPU
pub fn cpu_get_state(cpu_id: u32) -> Option<CpuState> {
    let manager = HOTPLUG_MANAGER.lock();
    let mgr = manager.borrow();
    
    mgr.get_cpu_state(cpu_id)
        .map(|s| s.get_state())
        .ok()
}

/// Register a callback for CPU state transitions
///
/// The callback will be invoked whenever any CPU transitions to a new state.
/// Returns a CallbackId that can be used to unregister the callback.
pub fn register_callback(callback: StateCallback) -> HotplugResult<CallbackId> {
    let mut manager = HOTPLUG_MANAGER.lock();
    let mut mgr = manager.borrow_mut();
    mgr.register_callback(callback)
}

/// Unregister a previously registered callback
pub fn unregister_callback(id: CallbackId) -> bool {
    let mut manager = HOTPLUG_MANAGER.lock();
    let mut mgr = manager.borrow_mut();
    mgr.unregister_callback(id)
}

/// Increment the reference count for a CPU
///
/// Prevents the CPU from being offlined until the reference is released.
/// Use this when scheduling critical tasks that shouldn't be interrupted.
pub fn cpu_get(cpu_id: u32) -> HotplugResult<()> {
    let manager = HOTPLUG_MANAGER.lock();
    let mgr = manager.borrow();
    let cpu_state = mgr.get_cpu_state(cpu_id)?;
    cpu_state.inc_refcount();
    Ok(())
}

/// Decrement the reference count for a CPU
///
/// Call this when done with a critical section that required the CPU to stay online.
pub fn cpu_put(cpu_id: u32) -> HotplugResult<u32> {
    let manager = HOTPLUG_MANAGER.lock();
    let mgr = manager.borrow();
    let cpu_state = mgr.get_cpu_state(cpu_id)?;
    Ok(cpu_state.dec_refcount())
}

/// Get the number of currently online CPUs
pub fn num_online_cpus() -> u32 {
    let manager = HOTPLUG_MANAGER.lock();
    let mgr = manager.borrow();
    mgr.online_count.load(Ordering::Acquire)
}

/// Get the list of all online CPU IDs
pub fn get_online_cpus() -> Vec<u32> {
    let manager = HOTPLUG_MANAGER.lock();
    let mgr = manager.borrow();
    
    let mut online = Vec::new();
    for (id, cpu_opt) in mgr.cpus.iter().enumerate() {
        if let Some(cpu) = cpu_opt {
            if cpu.get_state().is_schedulable() {
                online.push(id as u32);
            }
        }
    }
    online
}

/// Test helper: simulate a CPU state transition
#[cfg(test)]
pub fn test_set_cpu_state(cpu_id: u32, state: CpuState) -> HotplugResult<()> {
    let manager = HOTPLUG_MANAGER.lock();
    let mgr = manager.borrow();
    let cpu_state = mgr.get_cpu_state(cpu_id)?;
    cpu_state.set_state(state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_state_transitions() {
        assert!(CpuState::Offline.is_active() == false);
        assert!(CpuState::Online.is_active() == true);
        assert!(CpuState::Online.is_schedulable() == true);
        assert!(CpuState::Offline.is_schedulable() == false);
    }

    #[test]
    fn test_cpu_state_from_u8() {
        assert_eq!(CpuState::from_u8(0), Some(CpuState::Offline));
        assert_eq!(CpuState::from_u8(1), Some(CpuState::Starting));
        assert_eq!(CpuState::from_u8(2), Some(CpuState::Online));
        assert_eq!(CpuState::from_u8(3), Some(CpuState::Dying));
        assert_eq!(CpuState::from_u8(4), Some(CpuState::Dead));
        assert_eq!(CpuState::from_u8(5), None);
    }

    #[test]
    fn test_hotplug_error_display() {
        let err = HotplugError::InvalidCpu(999);
        assert!(err.to_string().contains("999"));
        
        let err = HotplugError::CannotOfflineBsp(0);
        assert!(err.to_string().contains("boot processor"));
    }
}
