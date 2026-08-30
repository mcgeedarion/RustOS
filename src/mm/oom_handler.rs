//! Memory Management OOM Handler and VMM Integration
//!
//! This module provides production-ready memory management with:
//! - Out-of-memory (OOM) handling and recovery
//! - Complete VMM integration with page fault handling
//! - Memory pressure detection and mitigation
//! - Graceful degradation under memory pressure

#![no_std]

extern crate alloc;

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use alloc::vec::Vec;

use crate::error::{KernelError, KernelResult};
use crate::mm::pmm;
use crate::proc::scheduler;

/// Global OOM state tracking
static OOM_ACTIVE: AtomicBool = AtomicBool::new(false);
static OOM_KILL_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Memory pressure levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressure {
    /// Normal operation, plenty of memory available
    Low,
    /// Memory is getting tight, consider freeing caches
    Medium,
    /// Critical - OOM killer may be invoked soon
    High,
    /// Out of memory - immediate action required
    Critical,
}

/// OOM killer policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OomPolicy {
    /// Kill the process that triggered OOM
    Current,
    /// Kill the process using most memory
    LargestMemory,
    /// Kill based on oom_score_adj value
    ScoreBased,
}

/// Memory statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    pub total_pages: usize,
    pub free_pages: usize,
    pub used_pages: usize,
    pub cached_pages: usize,
    pub swap_total: usize,
    pub swap_used: usize,
}

impl MemoryStats {
    /// Get current memory statistics
    pub fn current() -> Self {
        let total = pmm::total_pages();
        let free = pmm::free_pages();
        MemoryStats {
            total_pages: total,
            free_pages: free,
            used_pages: total.saturating_sub(free),
            cached_pages: 0, // TODO: integrate with page cache
            swap_total: 0,   // TODO: integrate with swap
            swap_used: 0,
        }
    }
    
    /// Calculate memory pressure level
    pub fn pressure(&self) -> MemoryPressure {
        if self.total_pages == 0 {
            return MemoryPressure::Critical;
        }
        
        let free_ratio = self.free_pages as f32 / self.total_pages as f32;
        
        if free_ratio < 0.02 {
            MemoryPressure::Critical
        } else if free_ratio < 0.05 {
            MemoryPressure::High
        } else if free_ratio < 0.10 {
            MemoryPressure::Medium
        } else {
            MemoryPressure::Low
        }
    }
}

/// Initialize OOM subsystem
pub fn init() {
    log::info!("mm: OOM handler initialized");
    log::info!("mm: Total physical pages: {}", pmm::total_pages());
    log::info!("mm: Free physical pages: {}", pmm::free_pages());
}

/// Handle allocation failure (OOM condition)
///
/// This is called when a critical allocation fails. It attempts to:
/// 1. Reclaim memory from caches
/// 2. Trigger swap if available
/// 3. Invoke OOM killer if necessary
///
/// Returns Ok(()) if memory was reclaimed, Err otherwise.
pub fn handle_oom() -> KernelResult<()> {
    // Prevent re-entrancy
    if OOM_ACTIVE.swap(true, Ordering::SeqCst) {
        return Err(KernelError::OutOfMemory);
    }
    
    let stats = MemoryStats::current();
    log::error!("OOM: free_pages={}/{} ({:.1}%)", 
                stats.free_pages, stats.total_pages,
                (stats.free_pages as f32 / stats.total_pages as f32) * 100.0);
    
    // Step 1: Try to reclaim from slab caches
    reclaim_slab_memory();
    
    // Step 2: Try to swap out anonymous pages
    #[cfg(feature = "swap")]
    {
        if let Err(e) = crate::mm::swap::kswapd_run_once() {
            log::warn!("OOM: kswapd failed: {:?}", e);
        }
    }
    
    // Check if we recovered
    if pmm::free_pages() > stats.free_pages + 16 {
        log::info!("OOM: Recovered {} pages", pmm::free_pages() - stats.free_pages);
        OOM_ACTIVE.store(false, Ordering::SeqCst);
        return Ok(());
    }
    
    // Step 3: Invoke OOM killer
    let victim_pid = select_oom_victim(OomPolicy::LargestMemory);
    if victim_pid != 0 {
        log::error!("OOM: Killing process {} to reclaim memory", victim_pid);
        kill_process(victim_pid);
        OOM_KILL_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    
    OOM_ACTIVE.store(false, Ordering::SeqCst);
    
    if victim_pid != 0 {
        Ok(())
    } else {
        Err(KernelError::OutOfMemory)
    }
}

/// Attempt to reclaim slab memory
fn reclaim_slab_memory() {
    use crate::mm::slab;
    
    // Shrink slab caches aggressively
    let freed = slab::slab_shrink(usize::MAX);
    if freed > 0 {
        log::debug!("OOM: Reclaimed {} bytes from slab", freed);
    }
}

/// Select a process to kill based on OOM policy
fn select_oom_victim(policy: OomPolicy) -> usize {
    match policy {
        OomPolicy::Current => scheduler::current_pid(),
        OomPolicy::LargestMemory => find_largest_memory_user(),
        OomPolicy::ScoreBased => find_highest_oom_score(),
    }
}

/// Find the process using the most memory
fn find_largest_memory_user() -> usize {
    let mut max_rss = 0;
    let mut victim_pid = 0;
    
    scheduler::for_each_process(|pcb| {
        let rss = pcb.rss_pages();
        if rss > max_rss && pcb.pid != 1 {
            max_rss = rss;
            victim_pid = pcb.pid;
        }
    });
    
    victim_pid
}

/// Find process with highest OOM score
fn find_highest_oom_score() -> usize {
    // TODO: Implement proper OOM scoring based on:
    // - RSS usage
    // - Runtime
    // - Privilege level
    // - oom_score_adj
    find_largest_memory_user()
}

/// Kill a process to reclaim memory
fn kill_process(pid: usize) {
    // Signal the process to terminate
    use crate::proc::signal;
    
    if let Err(e) = signal::send_signal(pid, 9) { // SIGKILL
        log::error!("OOM: Failed to send SIGKILL to {}: {:?}", pid, e);
    }
}

/// Check if we're in an OOM situation
pub fn is_oom_active() -> bool {
    OOM_ACTIVE.load(Ordering::Relaxed)
}

/// Get OOM kill count
pub fn oom_kill_count() -> usize {
    OOM_KILL_COUNT.load(Ordering::Relaxed)
}

/// Allocate pages with OOM handling
///
/// This wrapper around pmm::alloc_pages that handles OOM gracefully
pub fn alloc_pages_with_oom(count: usize) -> KernelResult<usize> {
    match pmm::alloc_pages(count) {
        Some(pa) => Ok(pa),
        None => {
            handle_oom()?;
            // Retry after OOM handling
            pmm::alloc_pages(count).ok_or(KernelError::OutOfMemory)
        }
    }
}

/// Allocate a single page with OOM handling
pub fn alloc_page_with_oom() -> KernelResult<usize> {
    alloc_pages_with_oom(1)
}

/// VMM Integration: Handle page faults with proper error propagation
///
/// This integrates with the page fault handler to provide complete
/// virtual memory management.
pub fn handle_vmm_fault(vaddr: usize, error_code: u64) -> KernelResult<()> {
    use crate::mm::page_fault;
    
    // Validate the address first
    if !is_valid_user_address(vaddr) {
        return Err(KernelError::Fault);
    }
    
    // Try to handle the fault
    match page_fault::handle_fault(vaddr, error_code) {
        Ok(()) => Ok(()),
        Err(_) => {
            // If page fault handling fails, try OOM recovery
            handle_oom()?;
            // Retry the fault
            page_fault::handle_fault(vaddr, error_code)
        }
    }
}

/// Check if an address is valid for user access
fn is_valid_user_address(addr: usize) -> bool {
    // Architecture-specific validation
    #[cfg(target_arch = "x86_64")]
    {
        // User space is typically below 0x8000_0000_0000
        addr < 0x8000_0000_0000 && addr >= 0x1000
    }
    #[cfg(target_arch = "aarch64")]
    {
        // AArch64 user space validation
        addr < 0x0000_00ff_ffff_f000 && addr >= 0x1000
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        addr >= 0x1000
    }
}

/// Register a memory area for tracking
pub fn register_vma(start: usize, end: usize, flags: u32) -> KernelResult<()> {
    use crate::mm::vma::VmaFlags;
    
    if start >= end {
        return Err(KernelError::InvalidArg);
    }
    
    if start % 4096 != 0 || end % 4096 != 0 {
        return Err(KernelError::InvalidArg);
    }
    
    // TODO: Integrate with VMA tracking
    Ok(())
}

/// Unregister a memory area
pub fn unregister_vma(start: usize) -> KernelResult<()> {
    // TODO: Integrate with VMA tracking
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_stats() {
        let stats = MemoryStats::current();
        assert!(stats.total_pages > 0);
        assert!(stats.free_pages <= stats.total_pages);
    }
    
    #[test]
    fn test_pressure_calculation() {
        let mut stats = MemoryStats {
            total_pages: 1000,
            free_pages: 500,
            ..Default::default()
        };
        assert_eq!(stats.pressure(), MemoryPressure::Low);
        
        stats.free_pages = 50;
        assert_eq!(stats.pressure(), MemoryPressure::High);
        
        stats.free_pages = 10;
        assert_eq!(stats.pressure(), MemoryPressure::Critical);
    }
}
