//! Load Balancer Framework for RustOS
//!
//! This module provides workload distribution across multiple CPUs in SMP systems.
//! It includes:
//! - Per-CPU run queues
//! - Work stealing scheduler
//! - Load metrics and balancing decisions
//! - Task migration support

#![no_std]
#![warn(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use spin::Mutex;

/// Maximum number of work items per CPU queue
pub const MAX_WORK_ITEMS_PER_CPU: usize = 1024;

/// Load threshold for triggering rebalancing (percentage)
pub const LOAD_THRESHOLD_HIGH: usize = 80;
pub const LOAD_THRESHOLD_LOW: usize = 20;

/// Error types for load balancer operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadBalancerError {
    /// Invalid CPU ID
    InvalidCpuId,
    /// Queue full
    QueueFull,
    /// Queue empty
    QueueEmpty,
    /// Migration failed
    MigrationFailed,
    /// Not enough capacity
    InsufficientCapacity,
}

impl core::fmt::Display for LoadBalancerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LoadBalancerError::InvalidCpuId => write!(f, "Invalid CPU ID"),
            LoadBalancerError::QueueFull => write!(f, "Work queue full"),
            LoadBalancerError::QueueEmpty => write!(f, "Work queue empty"),
            LoadBalancerError::MigrationFailed => write!(f, "Task migration failed"),
            LoadBalancerError::InsufficientCapacity => write!(f, "Insufficient capacity"),
        }
    }
}

/// Result type for load balancer operations
pub type LoadBalancerResult<T> = Result<T, LoadBalancerError>;

bitflags::bitflags! {
    /// Work item flags
    pub struct WorkFlags: u32 {
        /// Work is high priority
        const HIGH_PRIORITY = 1 << 0;
        /// Work can be stolen from other CPUs
        const STEALABLE = 1 << 1;
        /// Work is CPU-affine (should not migrate)
        const CPU_AFFINE = 1 << 2;
        /// Work is currently executing
        const RUNNING = 1 << 3;
        /// Work has been migrated
        const MIGRATED = 1 << 4;
    }
}

/// A work item that can be scheduled on any CPU
pub trait WorkItem: Send + Sync {
    /// Execute the work item
    fn execute(&mut self);
    
    /// Get work priority (higher = more urgent)
    fn priority(&self) -> u32 {
        0
    }
    
    /// Get estimated execution time in microseconds
    fn estimated_duration_us(&self) -> u64 {
        1000 // Default 1ms
    }
    
    /// Check if work can be migrated to another CPU
    fn is_migratable(&self) -> bool {
        true
    }
    
    /// Get preferred CPU affinity (-1 for no preference)
    fn cpu_affinity(&self) -> i32 {
        -1
    }
}

/// Per-CPU work queue
pub struct CpuWorkQueue {
    /// Queue of pending work items
    work_items: Mutex<Vec<Box<dyn WorkItem>>>,
    /// Number of items currently executing
    running_count: AtomicUsize,
    /// Total items processed by this CPU
    total_processed: AtomicUsize,
    /// Total execution time in microseconds
    total_exec_time_us: AtomicUsize,
    /// Current load percentage (0-100)
    current_load: AtomicUsize,
    /// CPU ID this queue belongs to
    cpu_id: usize,
}

unsafe impl Send for CpuWorkQueue {}
unsafe impl Sync for CpuWorkQueue {}

impl CpuWorkQueue {
    /// Create a new CPU work queue
    pub fn new(cpu_id: usize) -> Self {
        Self {
            work_items: Mutex::new(Vec::new()),
            running_count: AtomicUsize::new(0),
            total_processed: AtomicUsize::new(0),
            total_exec_time_us: AtomicUsize::new(0),
            current_load: AtomicUsize::new(0),
            cpu_id,
        }
    }
    
    /// Submit work to this CPU's queue
    pub fn submit(&self, work: Box<dyn WorkItem>) -> LoadBalancerResult<()> {
        let mut queue = self.work_items.lock();
        
        if queue.len() >= MAX_WORK_ITEMS_PER_CPU {
            return Err(LoadBalancerError::QueueFull);
        }
        
        queue.push(work);
        self.update_load();
        Ok(())
    }
    
    /// Get next work item from queue (called by CPU executor)
    pub fn pop_work(&self) -> Option<Box<dyn WorkItem>> {
        let mut queue = self.work_items.lock();
        
        if let Some(work) = queue.pop() {
            self.running_count.fetch_add(1, Ordering::Relaxed);
            return Some(work);
        }
        
        None
    }
    
    /// Mark work as completed
    pub fn complete_work(&self, exec_time_us: u64) {
        self.running_count.fetch_sub(1, Ordering::Relaxed);
        self.total_processed.fetch_add(1, Ordering::Relaxed);
        self.total_exec_time_us
            .fetch_add(exec_time_us as usize, Ordering::Relaxed);
        self.update_load();
    }
    
    /// Steal work from this queue (called by other CPUs)
    pub fn steal_work(&self) -> Option<Box<dyn WorkItem>> {
        let mut queue = self.work_items.lock();
        
        // Only steal from the back (oldest work)
        if queue.len() > 1 {
            // Keep at least one item for the owner
            if let Some(idx) = queue.iter().position(|w| {
                w.is_migratable() && !w.priority() > 0 // Prefer lower priority work for stealing
            }) {
                let work = queue.remove(idx);
                self.update_load();
                return Some(work);
            }
        }
        
        None
    }
    
    /// Get queue length
    pub fn len(&self) -> usize {
        self.work_items.lock().len()
    }
    
    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.work_items.lock().is_empty()
    }
    
    /// Get current load percentage
    pub fn load_percentage(&self) -> usize {
        self.current_load.load(Ordering::Relaxed)
    }
    
    /// Update load calculation
    fn update_load(&self) {
        let queue_len = self.work_items.lock().len();
        let running = self.running_count.load(Ordering::Relaxed);
        
        // Calculate load as percentage of max capacity
        let load = ((queue_len + running) * 100) / MAX_WORK_ITEMS_PER_CPU;
        self.current_load.store(load.min(100), Ordering::Relaxed);
    }
    
    /// Get statistics for this CPU queue
    pub fn get_stats(&self) -> CpuQueueStats {
        CpuQueueStats {
            cpu_id: self.cpu_id,
            pending_items: self.work_items.lock().len(),
            running_items: self.running_count.load(Ordering::Relaxed),
            total_processed: self.total_processed.load(Ordering::Relaxed),
            total_exec_time_us: self.total_exec_time_us.load(Ordering::Relaxed),
            current_load: self.current_load.load(Ordering::Relaxed),
        }
    }
}

/// Statistics for a CPU work queue
#[derive(Debug, Clone)]
pub struct CpuQueueStats {
    /// CPU ID
    pub cpu_id: usize,
    /// Number of pending work items
    pub pending_items: usize,
    /// Number of currently running items
    pub running_items: usize,
    /// Total items processed
    pub total_processed: usize,
    /// Total execution time in microseconds
    pub total_exec_time_us: usize,
    /// Current load percentage
    pub current_load: usize,
}

/// Global load balancer
pub struct LoadBalancer {
    /// Per-CPU work queues
    cpu_queues: [Option<CpuWorkQueue>; smp_core::MAX_CPUS],
    /// Balancing enabled flag
    balancing_enabled: AtomicBool,
    /// Last balance timestamp (in ticks)
    last_balance_tick: AtomicUsize,
    /// Balance interval in ticks
    balance_interval_ticks: AtomicUsize,
    /// Lock for rebalancing operations
    rebalance_lock: Mutex<()>,
}

unsafe impl Send for LoadBalancer {}
unsafe impl Sync for LoadBalancer {}

impl LoadBalancer {
    /// Create a new load balancer
    pub const fn new() -> Self {
        // Initialize with None values
        const INIT: Option<CpuWorkQueue> = None;
        Self {
            cpu_queues: [INIT; smp_core::MAX_CPUS],
            balancing_enabled: AtomicBool::new(false),
            last_balance_tick: AtomicUsize::new(0),
            balance_interval_ticks: AtomicUsize::new(100), // Balance every 100 ticks
            rebalance_lock: Mutex::new(()),
        }
    }
    
    /// Initialize a CPU queue
    pub fn init_cpu_queue(&mut self, cpu_id: usize) -> LoadBalancerResult<()> {
        if cpu_id >= smp_core::MAX_CPUS {
            return Err(LoadBalancerError::InvalidCpuId);
        }
        
        self.cpu_queues[cpu_id] = Some(CpuWorkQueue::new(cpu_id));
        log::info!("Initialized work queue for CPU {}", cpu_id);
        Ok(())
    }
    
    /// Submit work to the best available CPU
    pub fn submit_work(&self, work: Box<dyn WorkItem>) -> LoadBalancerResult<usize> {
        let affinity = work.cpu_affinity();
        
        // If work has CPU affinity, try to schedule there first
        if affinity >= 0 {
            let cpu_id = affinity as usize;
            if cpu_id < smp_core::MAX_CPUS {
                if let Some(queue) = &self.cpu_queues[cpu_id] {
                    if let Ok(_) = queue.submit(work) {
                        return Ok(cpu_id);
                    }
                }
            }
        }
        
        // Find the least loaded CPU
        let target_cpu = self.find_least_loaded_cpu()?;
        
        if let Some(queue) = &self.cpu_queues[target_cpu] {
            queue.submit(work)?;
            Ok(target_cpu)
        } else {
            Err(LoadBalancerError::InsufficientCapacity)
        }
    }
    
    /// Submit work to a specific CPU
    pub fn submit_to_cpu(&self, cpu_id: usize, work: Box<dyn WorkItem>) -> LoadBalancerResult<()> {
        if cpu_id >= smp_core::MAX_CPUS {
            return Err(LoadBalancerError::InvalidCpuId);
        }
        
        if let Some(queue) = &self.cpu_queues[cpu_id] {
            queue.submit(work)
        } else {
            Err(LoadBalancerError::InvalidCpuId)
        }
    }
    
    /// Find the least loaded CPU
    pub fn find_least_loaded_cpu(&self) -> LoadBalancerResult<usize> {
        let online_cpus = smp_core::online_cpus();
        
        if online_cpus.is_empty() {
            return Err(LoadBalancerError::InsufficientCapacity);
        }
        
        let mut best_cpu = online_cpus[0];
        let mut min_load = usize::MAX;
        
        for &cpu_id in &online_cpus {
            if let Some(queue) = &self.cpu_queues[cpu_id] {
                let load = queue.load_percentage();
                if load < min_load {
                    min_load = load;
                    best_cpu = cpu_id;
                }
            }
        }
        
        Ok(best_cpu)
    }
    
    /// Perform load balancing across CPUs
    pub fn balance_load(&self) {
        let _guard = self.rebalance_lock.lock();
        
        let online_cpus = smp_core::online_cpus();
        if online_cpus.len() < 2 {
            return; // No need to balance with single CPU
        }
        
        // Find overloaded and underloaded CPUs
        let mut overloaded: Vec<(usize, usize)> = Vec::new(); // (cpu_id, load)
        let mut underloaded: Vec<(usize, usize)> = Vec::new();
        
        for &cpu_id in &online_cpus {
            if let Some(queue) = &self.cpu_queues[cpu_id] {
                let load = queue.load_percentage();
                if load > LOAD_THRESHOLD_HIGH {
                    overloaded.push((cpu_id, load));
                } else if load < LOAD_THRESHOLD_LOW {
                    underloaded.push((cpu_id, load));
                }
            }
        }
        
        // Try to migrate work from overloaded to underloaded CPUs
        for (over_cpu, _) in overloaded {
            if let Some(queue) = &self.cpu_queues[over_cpu] {
                for (under_cpu, _) in &underloaded {
                    if let Some(target_queue) = &self.cpu_queues[*under_cpu] {
                        if target_queue.load_percentage() > LOAD_THRESHOLD_HIGH {
                            continue; // Target is now overloaded
                        }
                        
                        // Try to steal and migrate work
                        if let Some(work) = queue.steal_work() {
                            if work.is_migratable() {
                                let _ = target_queue.submit(work);
                                log::debug!(
                                    "Migrated work from CPU {} to CPU {}",
                                    over_cpu,
                                    under_cpu
                                );
                            } else {
                                // Put it back if not migratable
                                let _ = queue.submit(work);
                            }
                        }
                    }
                }
            }
        }
        
        log::debug!("Load balancing completed");
    }
    
    /// Check if balancing should run
    pub fn should_balance(&self, current_tick: usize) -> bool {
        if !self.balancing_enabled.load(Ordering::Relaxed) {
            return false;
        }
        
        let last_tick = self.last_balance_tick.load(Ordering::Relaxed);
        let interval = self.balance_interval_ticks.load(Ordering::Relaxed);
        
        if current_tick.saturating_sub(last_tick) >= interval {
            self.last_balance_tick.store(current_tick, Ordering::Relaxed);
            return true;
        }
        
        false
    }
    
    /// Enable load balancing
    pub fn enable_balancing(&self) {
        self.balancing_enabled.store(true, Ordering::Relaxed);
        log::info!("Load balancing enabled");
    }
    
    /// Disable load balancing
    pub fn disable_balancing(&self) {
        self.balancing_enabled.store(false, Ordering::Relaxed);
        log::info!("Load balancing disabled");
    }
    
    /// Check if balancing is enabled
    pub fn is_balancing_enabled(&self) -> bool {
        self.balancing_enabled.load(Ordering::Relaxed)
    }
    
    /// Set balance interval in ticks
    pub fn set_balance_interval(&self, ticks: usize) {
        self.balance_interval_ticks.store(ticks, Ordering::Relaxed);
    }
    
    /// Get statistics for all CPUs
    pub fn get_all_stats(&self) -> Vec<CpuQueueStats> {
        let mut stats = Vec::new();
        
        for cpu_id in 0..smp_core::MAX_CPUS {
            if let Some(queue) = &self.cpu_queues[cpu_id] {
                stats.push(queue.get_stats());
            }
        }
        
        stats
    }
    
    /// Get total system load
    pub fn get_system_load(&self) -> usize {
        let online_cpus = smp_core::online_cpus();
        if online_cpus.is_empty() {
            return 0;
        }
        
        let mut total_load = 0;
        for &cpu_id in &online_cpus {
            if let Some(queue) = &self.cpu_queues[cpu_id] {
                total_load += queue.load_percentage();
            }
        }
        
        total_load / online_cpus.len()
    }
}

/// Global load balancer instance
static LOAD_BALANCER: Mutex<LoadBalancer> = Mutex::new(LoadBalancer::new());

/// Initialize the load balancer for a CPU
pub fn init_cpu(cpu_id: usize) -> LoadBalancerResult<()> {
    let mut balancer = LOAD_BALANCER.lock();
    balancer.init_cpu_queue(cpu_id)
}

/// Submit work to the load balancer
pub fn submit_work(work: Box<dyn WorkItem>) -> LoadBalancerResult<usize> {
    let balancer = LOAD_BALANCER.lock();
    balancer.submit_work(work)
}

/// Submit work to a specific CPU
pub fn submit_to_cpu(cpu_id: usize, work: Box<dyn WorkItem>) -> LoadBalancerResult<()> {
    let balancer = LOAD_BALANCER.lock();
    balancer.submit_to_cpu(cpu_id, work)
}

/// Trigger load balancing
pub fn trigger_balance() {
    let balancer = LOAD_BALANCER.lock();
    balancer.balance_load();
}

/// Enable load balancing
pub fn enable_balancing() {
    let balancer = LOAD_BALANCER.lock();
    balancer.enable_balancing();
}

/// Disable load balancing
pub fn disable_balancing() {
    let balancer = LOAD_BALANCER.lock();
    balancer.disable_balancing();
}

/// Get system load average
pub fn get_system_load() -> usize {
    let balancer = LOAD_BALANCER.lock();
    balancer.get_system_load()
}

/// Get statistics for all CPUs
pub fn get_cpu_stats() -> Vec<CpuQueueStats> {
    let balancer = LOAD_BALANCER.lock();
    balancer.get_all_stats()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    struct TestWork {
        id: u32,
    }
    
    impl WorkItem for TestWork {
        fn execute(&mut self) {
            // Test execution
        }
        
        fn priority(&self) -> u32 {
            self.id
        }
    }
    
    #[test]
    fn test_cpu_queue() {
        let queue = CpuWorkQueue::new(0);
        
        let work = Box::new(TestWork { id: 1 });
        assert!(queue.submit(work).is_ok());
        
        assert_eq!(queue.len(), 1);
        assert!(!queue.is_empty());
        
        let popped = queue.pop_work();
        assert!(popped.is_some());
        assert_eq!(queue.len(), 0);
    }
    
    #[test]
    fn test_queue_full() {
        let queue = CpuWorkQueue::new(1);
        
        // Fill the queue
        for i in 0..MAX_WORK_ITEMS_PER_CPU {
            let work = Box::new(TestWork { id: i as u32 });
            if queue.submit(work).is_err() {
                break;
            }
        }
        
        // Next submit should fail
        let work = Box::new(TestWork { id: 9999 });
        assert!(matches!(
            queue.submit(work),
            Err(LoadBalancerError::QueueFull)
        ));
    }
    
    #[test]
    fn test_load_calculation() {
        let queue = CpuWorkQueue::new(2);
        
        // Empty queue should have 0 load
        assert_eq!(queue.load_percentage(), 0);
        
        // Add some work
        for i in 0..10 {
            let work = Box::new(TestWork { id: i });
            let _ = queue.submit(work);
        }
        
        // Load should be > 0
        assert!(queue.load_percentage() > 0);
    }
}
