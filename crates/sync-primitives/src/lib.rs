//! Advanced Synchronization Primitives for RustOS
//!
//! This crate provides:
//! - Queued spinlocks for high-contention paths
//! - Read-Copy-Update (RCU) for lock-free reads
//! - Enhanced mutex and rwlock wrappers

#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::sync::atomic::{AtomicBool, AtomicUsize, AtomicU32, Ordering};
use core::cell::UnsafeCell;
use core::ptr;
use core::marker::PhantomData;

/// Queued spinlock node for fair locking
#[repr(C)]
pub struct QspinlockNode {
    next: *mut QspinlockNode,
    locked: AtomicBool,
}

unsafe impl Send for QspinlockNode {}
unsafe impl Sync for QspinlockNode {}

impl QspinlockNode {
    pub const fn new() -> Self {
        Self {
            next: ptr::null_mut(),
            locked: AtomicBool::new(false),
        }
    }
}

/// Queued spinlock for high-contention scenarios
///
/// Uses MCS-style queuing for fairness and reduced contention.
pub struct Qspinlock {
    tail: AtomicUsize,
}

unsafe impl Send for Qspinlock {}
unsafe impl Sync for Qspinlock {}

impl Qspinlock {
    pub const fn new() -> Self {
        Self {
            tail: AtomicUsize::new(0),
        }
    }
    
    /// Acquire the lock with a local node
    pub fn lock(&self, node: &QspinlockNode) {
        let node_ptr = node as *const QspinlockNode as usize;
        
        // Try to acquire immediately
        if self.tail.compare_exchange(0, node_ptr, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            return;
        }
        
        // Set predecessor's next pointer
        let prev_tail = self.tail.swap(node_ptr, Ordering::AcqRel);
        unsafe {
            if prev_tail != 0 {
                let prev_node = &*(prev_tail as *const QspinlockNode);
                prev_node.next.store(node_ptr as *mut _, Ordering::Release);
            }
        }
        
        // Wait for our turn
        while !node.locked.load(Ordering::Acquire) {
            core::hint::spin_loop();
        }
    }
    
    /// Release the lock
    pub fn unlock(&self, node: &QspinlockNode) {
        // Clear our node
        node.locked.store(false, Ordering::Release);
        
        // If we're not the tail, successor will handle cleanup
        // Otherwise, try to reset tail to 0
        let _ = self.tail.compare_exchange(
            node as *const QspinlockNode as usize,
            0,
            Ordering::Release,
            Ordering::Relaxed,
        );
    }
}

/// RCU (Read-Copy-Update) read-side guard
pub struct RcuReadGuard<'a> {
    _marker: PhantomData<&'a ()>,
}

impl<'a> Drop for RcuReadGuard<'a> {
    fn drop(&mut self) {
        // End RCU read-side critical section
        rcu_read_unlock();
    }
}

/// RCU protected data wrapper
pub struct RcuProtected<T> {
    data: UnsafeCell<Option<T>>,
    version: AtomicUsize,
}

unsafe impl<T: Send + Sync> Send for RcuProtected<T> {}
unsafe impl<T: Send + Sync> Sync for RcuProtected<T> {}

impl<T> RcuProtected<T> {
    pub const fn new(data: T) -> Self {
        Self {
            data: UnsafeCell::new(Some(data)),
            version: AtomicUsize::new(0),
        }
    }
    
    /// Start an RCU read-side critical section
    pub fn read(&self) -> RcuReadGuard {
        rcu_read_lock();
        RcuReadGuard { _marker: PhantomData }
    }
    
    /// Get immutable reference during read-side critical section
    /// SAFETY: Must be called within RCU read-side critical section
    pub unsafe fn get_ref<'a>(&'a self, _guard: &'a RcuReadGuard) -> Option<&'a T> {
        (*self.data.get()).as_ref()
    }
    
    /// Update data (writer side)
    pub fn update<F>(&self, updater: F) 
    where
        F: FnOnce(&mut T)
    {
        // Increment version
        self.version.fetch_add(1, Ordering::SeqCst);
        
        unsafe {
            if let Some(ref mut data) = *self.data.get() {
                updater(data);
            }
        }
        
        // Synchronize (wait for readers to finish)
        rcu_synchronize();
    }
}

// Global RCU state
static RCU_ACTIVE_READERS: AtomicUsize = AtomicUsize::new(0);
static RCU_UPDATING: AtomicBool = AtomicBool::new(false);

fn rcu_read_lock() {
    RCU_ACTIVE_READERS.fetch_add(1, Ordering::SeqCst);
}

fn rcu_read_unlock() {
    RCU_ACTIVE_READERS.fetch_sub(1, Ordering::SeqCst);
}

fn rcu_synchronize() {
    RCU_UPDATING.store(true, Ordering::SeqCst);
    core::hint::spin_loop();
    
    // Wait for all readers to finish
    while RCU_ACTIVE_READERS.load(Ordering::SeqCst) > 0 {
        core::hint::spin_loop();
    }
    
    RCU_UPDATING.store(false, Ordering::SeqCst);
}

/// Ticket spinlock for fairness
pub struct TicketSpinlock {
    next_ticket: AtomicU32,
    now_serving: AtomicU32,
}

unsafe impl Send for TicketSpinlock {}
unsafe impl Sync for TicketSpinlock {}

impl TicketSpinlock {
    pub const fn new() -> Self {
        Self {
            next_ticket: AtomicU32::new(0),
            now_serving: AtomicU32::new(0),
        }
    }
    
    pub fn lock(&self) -> TicketGuard {
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        
        while self.now_serving.load(Ordering::Acquire) != ticket {
            core::hint::spin_loop();
        }
        
        TicketGuard { lock: self }
    }
}

pub struct TicketGuard<'a> {
    lock: &'a TicketSpinlock,
}

impl<'a> Drop for TicketGuard<'a> {
    fn drop(&mut self) {
        self.lock.now_serving.fetch_add(1, Ordering::Release);
    }
}

/// Per-CPU data structure
#[cfg(target_arch = "x86_64")]
pub struct PerCpuData<T> {
    data: [UnsafeCell<Option<T>>; 256], // Max CPUs
}

#[cfg(not(target_arch = "x86_64"))]
pub struct PerCpuData<T> {
    data: [UnsafeCell<Option<T>>; 64], // Default max CPUs
}

unsafe impl<T: Send> Send for PerCpuData<T> {}
unsafe impl<T: Send + Sync> Sync for PerCpuData<T> {}

impl<T> PerCpuData<T> {
    pub const fn new() -> Self {
        const INIT: UnsafeCell<Option<()>> = UnsafeCell::new(None);
        Self {
            #[cfg(target_arch = "x86_64")]
            data: [INIT; 256],
            #[cfg(not(target_arch = "x86_64"))]
            data: [INIT; 64],
        }
    }
    
    pub fn get(&self, cpu_id: usize) -> Option<&T> {
        unsafe { (*self.data[cpu_id].get()).as_ref() }
    }
    
    pub fn set(&self, cpu_id: usize, data: T) {
        unsafe {
            *self.data[cpu_id].get() = Some(data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_qspinlock() {
        let lock = Qspinlock::new();
        let node = QspinlockNode::new();
        
        lock.lock(&node);
        // Critical section
        lock.unlock(&node);
    }
    
    #[test]
    fn test_rcu() {
        let rcu_data = RcuProtected::new(42);
        
        {
            let guard = rcu_data.read();
            unsafe {
                let val = rcu_data.get_ref(&guard);
                assert_eq!(val, Some(&42));
            }
        }
        
        rcu_data.update(|v| *v += 1);
        
        {
            let guard = rcu_data.read();
            unsafe {
                let val = rcu_data.get_ref(&guard);
                assert_eq!(val, Some(&43));
            }
        }
    }
}
