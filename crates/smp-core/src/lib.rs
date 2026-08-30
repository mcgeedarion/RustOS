//! SMP (Symmetric Multi-Processing) Core Framework
//!
//! This module provides the foundation for multi-core processing in RustOS,
//! including CPU hotplug, core enumeration, and per-CPU data management.

#![no_std]
#![warn(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use spin::Mutex;

/// Maximum number of CPUs supported
pub const MAX_CPUS: usize = 256;

/// CPU state flags
bitflags::bitflags! {
    /// Flags representing CPU state
    pub struct CpuFlags: u32 {
        /// CPU is present in the system
        const PRESENT = 1 << 0;
        /// CPU is online and schedulable
        const ONLINE = 1 << 1;
        /// CPU is being brought up
        const STARTING = 1 << 2;
        /// CPU is being brought down
        const DYING = 1 << 3;
        /// CPU is in idle state
        const IDLE = 1 << 4;
        /// CPU has been hot-added
        const HOTPLUGGED = 1 << 5;
    }
}

/// Error types for CPU operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuError {
    /// Invalid CPU ID
    InvalidCpuId,
    /// CPU not present
    NotPresent,
    /// CPU already online
    AlreadyOnline,
    /// CPU already offline
    AlreadyOffline,
    /// Operation failed
    OperationFailed,
    /// No more CPUs available
    NoMoreCpus,
    /// Hotplug operation not supported
    HotplugNotSupported,
}

impl core::fmt::Display for CpuError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CpuError::InvalidCpuId => write!(f, "Invalid CPU ID"),
            CpuError::NotPresent => write!(f, "CPU not present"),
            CpuError::AlreadyOnline => write!(f, "CPU already online"),
            CpuError::AlreadyOffline => write!(f, "CPU already offline"),
            CpuError::OperationFailed => write!(f, "Operation failed"),
            CpuError::NoMoreCpus => write!(f, "No more CPUs available"),
            CpuError::HotplugNotSupported => write!(f, "Hotplug not supported"),
        }
    }
}

/// Result type for CPU operations
pub type CpuResult<T> = Result<T, CpuError>;

/// Per-CPU data structure
#[repr(C)]
pub struct CpuData {
    /// CPU logical ID
    pub id: usize,
    /// CPU physical APIC ID
    pub apic_id: u32,
    /// Current CPU flags
    pub flags: AtomicUsize,
    /// CPU frequency in MHz (0 if unknown)
    pub frequency_mhz: u32,
    /// Stack pointer for this CPU
    pub stack_ptr: *mut u8,
    /// Stack size in bytes
    pub stack_size: usize,
    /// Per-CPU storage pointer
    pub per_cpu_data: *mut u8,
    /// Per-CPU storage size
    pub per_cpu_size: usize,
}

unsafe impl Send for CpuData {}
unsafe impl Sync for CpuData {}

impl CpuData {
    /// Create a new CpuData instance
    pub const fn new(id: usize) -> Self {
        Self {
            id,
            apic_id: 0,
            flags: AtomicUsize::new(0),
            frequency_mhz: 0,
            stack_ptr: core::ptr::null_mut(),
            stack_size: 0,
            per_cpu_data: core::ptr::null_mut(),
            per_cpu_size: 0,
        }
    }

    /// Get CPU flags
    pub fn flags(&self) -> CpuFlags {
        CpuFlags::from_bits_truncate(self.flags.load(Ordering::Relaxed))
    }

    /// Set CPU flags
    pub fn set_flags(&self, flags: CpuFlags) {
        self.flags.store(flags.bits() as usize, Ordering::Relaxed);
    }

    /// Update CPU flags atomically
    pub fn update_flags<F>(&self, f: F)
    where
        F: FnOnce(CpuFlags) -> CpuFlags,
    {
        let mut current = self.flags.load(Ordering::Relaxed);
        loop {
            let old_flags = CpuFlags::from_bits_truncate(current);
            let new_flags = f(old_flags);
            match self.flags.compare_exchange(
                current,
                new_flags.bits() as usize,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current = x,
            }
        }
    }

    /// Check if CPU is present
    pub fn is_present(&self) -> bool {
        self.flags().contains(CpuFlags::PRESENT)
    }

    /// Check if CPU is online
    pub fn is_online(&self) -> bool {
        self.flags().contains(CpuFlags::ONLINE)
    }

    /// Check if CPU is starting
    pub fn is_starting(&self) -> bool {
        self.flags().contains(CpuFlags::STARTING)
    }

    /// Check if CPU is dying
    pub fn is_dying(&self) -> bool {
        self.flags().contains(CpuFlags::DYING)
    }

    /// Check if CPU is idle
    pub fn is_idle(&self) -> bool {
        self.flags().contains(CpuFlags::IDLE)
    }

    /// Check if CPU was hotplugged
    pub fn is_hotplugged(&self) -> bool {
        self.flags().contains(CpuFlags::HOTPLUGGED)
    }
}

/// Global CPU registry
pub struct CpuRegistry {
    /// Array of CPU data structures
    cpus: [Option<CpuData>; MAX_CPUS],
    /// Number of present CPUs
    present_count: AtomicUsize,
    /// Number of online CPUs
    online_count: AtomicUsize,
    /// Boot CPU ID
    boot_cpu: usize,
    /// Hotplug support enabled
    hotplug_enabled: AtomicBool,
    /// Lock for registry modifications
    lock: Mutex<()>,
}

unsafe impl Send for CpuRegistry {}
unsafe impl Sync for CpuRegistry {}

impl CpuRegistry {
    /// Create a new CPU registry
    pub const fn new() -> Self {
        // Initialize array with None values
        const INIT: Option<CpuData> = None;
        Self {
            cpus: [INIT; MAX_CPUS],
            present_count: AtomicUsize::new(0),
            online_count: AtomicUsize::new(0),
            boot_cpu: 0,
            hotplug_enabled: AtomicBool::new(false),
            lock: Mutex::new(()),
        }
    }

    /// Initialize the boot CPU
    pub fn init_boot_cpu(&mut self, id: usize, apic_id: u32) -> CpuResult<()> {
        if id >= MAX_CPUS {
            return Err(CpuError::InvalidCpuId);
        }

        let _guard = self.lock.lock();

        if self.cpus[id].is_some() {
            return Err(CpuError::AlreadyOnline);
        }

        let mut cpu_data = CpuData::new(id);
        cpu_data.apic_id = apic_id;
        cpu_data.set_flags(CpuFlags::PRESENT | CpuFlags::ONLINE);

        self.cpus[id] = Some(cpu_data);
        self.present_count.fetch_add(1, Ordering::Relaxed);
        self.online_count.fetch_add(1, Ordering::Relaxed);
        self.boot_cpu = id;

        log::info!("Initialized boot CPU {} (APIC ID: {})", id, apic_id);
        Ok(())
    }

    /// Register a secondary CPU
    pub fn register_cpu(&mut self, id: usize, apic_id: u32, hotplugged: bool) -> CpuResult<()> {
        if id >= MAX_CPUS {
            return Err(CpuError::InvalidCpuId);
        }

        let _guard = self.lock.lock();

        if self.cpus[id].is_some() {
            return Err(CpuError::AlreadyOnline);
        }

        let mut cpu_data = CpuData::new(id);
        cpu_data.apic_id = apic_id;
        let mut flags = CpuFlags::PRESENT;
        if hotplugged {
            flags |= CpuFlags::HOTPLUGGED;
        }
        cpu_data.set_flags(flags);

        self.cpus[id] = Some(cpu_data);
        self.present_count.fetch_add(1, Ordering::Relaxed);

        log::info!(
            "Registered CPU {} (APIC ID: {}, hotplugged: {})",
            id,
            apic_id,
            hotplugged
        );
        Ok(())
    }

    /// Bring a CPU online (cpu_up)
    pub fn cpu_up(&mut self, id: usize) -> CpuResult<()> {
        if id >= MAX_CPUS {
            return Err(CpuError::InvalidCpuId);
        }

        let _guard = self.lock.lock();

        let cpu_data = match &mut self.cpus[id] {
            Some(cpu) => cpu,
            None => return Err(CpuError::NotPresent),
        };

        if cpu_data.is_online() {
            return Err(CpuError::AlreadyOnline);
        }

        if !cpu_data.is_present() {
            return Err(CpuError::NotPresent);
        }

        // Mark CPU as starting
        cpu_data.update_flags(|f| f | CpuFlags::STARTING);

        // Platform-specific CPU startup (to be implemented by platform)
        // This is where we would call into architecture-specific code
        #[cfg(target_arch = "x86_64")]
        {
            // Call architecture-specific CPU bringup
            if let Err(_) = self.arch_cpu_up(id) {
                cpu_data.update_flags(|f| f - CpuFlags::STARTING);
                return Err(CpuError::OperationFailed);
            }
        }

        // Mark CPU as online
        cpu_data.update_flags(|f| (f - CpuFlags::STARTING) | CpuFlags::ONLINE);
        self.online_count.fetch_add(1, Ordering::Relaxed);

        log::info!("CPU {} is now online", id);
        Ok(())
    }

    /// Bring a CPU offline (cpu_down)
    pub fn cpu_down(&mut self, id: usize) -> CpuResult<()> {
        if id >= MAX_CPUS {
            return Err(CpuError::InvalidCpuId);
        }

        // Cannot offline the boot CPU
        if id == self.boot_cpu {
            log::warn!("Cannot offline boot CPU {}", id);
            return Err(CpuError::OperationFailed);
        }

        let _guard = self.lock.lock();

        let cpu_data = match &mut self.cpus[id] {
            Some(cpu) => cpu,
            None => return Err(CpuError::NotPresent),
        };

        if !cpu_data.is_online() {
            return Err(CpuError::AlreadyOffline);
        }

        // Mark CPU as dying
        cpu_data.update_flags(|f| f | CpuFlags::DYING);

        // Platform-specific CPU shutdown (to be implemented by platform)
        #[cfg(target_arch = "x86_64")]
        {
            if let Err(_) = self.arch_cpu_down(id) {
                cpu_data.update_flags(|f| f - CpuFlags::DYING);
                return Err(CpuError::OperationFailed);
            }
        }

        // Mark CPU as offline
        cpu_data.update_flags(|f| (f - CpuFlags::DYING) - CpuFlags::ONLINE);
        self.online_count.fetch_sub(1, Ordering::Relaxed);

        log::info!("CPU {} is now offline", id);
        Ok(())
    }

    /// Enable CPU hotplug support
    pub fn enable_hotplug(&mut self) {
        self.hotplug_enabled.store(true, Ordering::Relaxed);
        log::info!("CPU hotplug support enabled");
    }

    /// Check if hotplug is enabled
    pub fn is_hotplug_enabled(&self) -> bool {
        self.hotplug_enabled.load(Ordering::Relaxed)
    }

    /// Get CPU data by ID
    pub fn get_cpu(&self, id: usize) -> Option<&CpuData> {
        if id >= MAX_CPUS {
            return None;
        }
        self.cpus[id].as_ref()
    }

    /// Get mutable CPU data by ID
    pub fn get_cpu_mut(&mut self, id: usize) -> Option<&mut CpuData> {
        if id >= MAX_CPUS {
            return None;
        }
        self.cpus[id].as_mut()
    }

    /// Get the boot CPU ID
    pub fn boot_cpu_id(&self) -> usize {
        self.boot_cpu
    }

    /// Get the current CPU ID (platform-specific implementation)
    pub fn current_cpu_id() -> usize {
        #[cfg(target_arch = "x86_64")]
        {
            use core::arch::x86_64::__rdgsbase;
            // Assuming per-CPU base is stored in GS base
            unsafe { __rdgsbase() as usize & 0xFFFF }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            0
        }
    }

    /// Get number of present CPUs
    pub fn present_count(&self) -> usize {
        self.present_count.load(Ordering::Relaxed)
    }

    /// Get number of online CPUs
    pub fn online_count(&self) -> usize {
        self.online_count.load(Ordering::Relaxed)
    }

    /// Get list of online CPU IDs
    pub fn online_cpus(&self) -> Vec<usize> {
        let mut cpus = Vec::new();
        for i in 0..MAX_CPUS {
            if let Some(cpu) = &self.cpus[i] {
                if cpu.is_online() {
                    cpus.push(i);
                }
            }
        }
        cpus
    }

    /// Get list of present CPU IDs
    pub fn present_cpus(&self) -> Vec<usize> {
        let mut cpus = Vec::new();
        for i in 0..MAX_CPUS {
            if let Some(cpu) = &self.cpus[i] {
                if cpu.is_present() {
                    cpus.push(i);
                }
            }
        }
        cpus
    }

    /// Check if a CPU is online
    pub fn is_cpu_online(&self, id: usize) -> bool {
        self.get_cpu(id)
            .map(|cpu| cpu.is_online())
            .unwrap_or(false)
    }

    /// Architecture-specific CPU bringup (x86_64 stub)
    #[cfg(target_arch = "x86_64")]
    fn arch_cpu_up(&self, _id: usize) -> Result<(), ()> {
        // Stub: In real implementation, this would send INIT-SIPI sequence
        // to the target CPU via APIC
        log::debug!("Architecture CPU up called for CPU {}", _id);
        Ok(())
    }

    /// Architecture-specific CPU shutdown (x86_64 stub)
    #[cfg(target_arch = "x86_64")]
    fn arch_cpu_down(&self, _id: usize) -> Result<(), ()> {
        // Stub: In real implementation, this would halt the CPU
        // and put it in a low-power state
        log::debug!("Architecture CPU down called for CPU {}", _id);
        Ok(())
    }
}

/// Global CPU registry instance
static CPU_REGISTRY: Mutex<CpuRegistry> = Mutex::new(CpuRegistry::new());

/// Initialize the SMP subsystem
pub fn init_smp(boot_cpu_id: usize, boot_apic_id: u32) -> CpuResult<()> {
    let mut registry = CPU_REGISTRY.lock();
    registry.init_boot_cpu(boot_cpu_id, boot_apic_id)?;
    log::info!("SMP subsystem initialized with boot CPU {}", boot_cpu_id);
    Ok(())
}

/// Bring a CPU online
pub fn cpu_up(id: usize) -> CpuResult<()> {
    let mut registry = CPU_REGISTRY.lock();
    registry.cpu_up(id)
}

/// Bring a CPU offline
pub fn cpu_down(id: usize) -> CpuResult<()> {
    let mut registry = CPU_REGISTRY.lock();
    registry.cpu_down(id)
}

/// Register a secondary CPU
pub fn register_cpu(id: usize, apic_id: u32, hotplugged: bool) -> CpuResult<()> {
    let mut registry = CPU_REGISTRY.lock();
    registry.register_cpu(id, apic_id, hotplugged)
}

/// Enable CPU hotplug support
pub fn enable_hotplug() {
    let mut registry = CPU_REGISTRY.lock();
    registry.enable_hotplug();
}

/// Check if hotplug is enabled
pub fn is_hotplug_enabled() -> bool {
    let registry = CPU_REGISTRY.lock();
    registry.is_hotplug_enabled()
}

/// Get the boot CPU ID
pub fn boot_cpu_id() -> usize {
    let registry = CPU_REGISTRY.lock();
    registry.boot_cpu_id()
}

/// Get the current CPU ID
pub fn current_cpu_id() -> usize {
    CpuRegistry::current_cpu_id()
}

/// Get number of present CPUs
pub fn present_cpu_count() -> usize {
    let registry = CPU_REGISTRY.lock();
    registry.present_count()
}

/// Get number of online CPUs
pub fn online_cpu_count() -> usize {
    let registry = CPU_REGISTRY.lock();
    registry.online_count()
}

/// Get list of online CPUs
pub fn online_cpus() -> Vec<usize> {
    let registry = CPU_REGISTRY.lock();
    registry.online_cpus()
}

/// Check if a CPU is online
pub fn is_cpu_online(id: usize) -> bool {
    let registry = CPU_REGISTRY.lock();
    registry.is_cpu_online(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_data_creation() {
        let cpu = CpuData::new(0);
        assert_eq!(cpu.id, 0);
        assert!(!cpu.is_present());
        assert!(!cpu.is_online());
    }

    #[test]
    fn test_cpu_flags() {
        let cpu = CpuData::new(1);
        cpu.set_flags(CpuFlags::PRESENT | CpuFlags::ONLINE);
        assert!(cpu.is_present());
        assert!(cpu.is_online());
        assert!(!cpu.is_idle());
    }

    #[test]
    fn test_registry_init() {
        let mut registry = CpuRegistry::new();
        assert_eq!(registry.present_count(), 0);
        assert_eq!(registry.online_count(), 0);

        registry.init_boot_cpu(0, 0).unwrap();
        assert_eq!(registry.present_count(), 1);
        assert_eq!(registry.online_count(), 1);
        assert!(registry.is_cpu_online(0));
    }

    #[test]
    fn test_cpu_up_down() {
        let mut registry = CpuRegistry::new();
        registry.init_boot_cpu(0, 0).unwrap();
        registry.register_cpu(1, 1, false).unwrap();

        assert!(!registry.is_cpu_online(1));
        registry.cpu_up(1).unwrap();
        assert!(registry.is_cpu_online(1));
        assert_eq!(registry.online_count(), 2);

        registry.cpu_down(1).unwrap();
        assert!(!registry.is_cpu_online(1));
        assert_eq!(registry.online_count(), 1);
    }

    #[test]
    fn test_cannot_offline_boot_cpu() {
        let mut registry = CpuRegistry::new();
        registry.init_boot_cpu(0, 0).unwrap();

        let result = registry.cpu_down(0);
        assert!(result.is_err());
    }
}
