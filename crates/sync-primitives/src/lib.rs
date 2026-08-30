//! Advanced Synchronization Primitives for RustOS
//!
//! This crate provides:
//! - Queued spinlocks for high-contention paths
//! - Read-Copy-Update (RCU) for lock-free reads
//! - Ticket spinlocks for fair locking
//! - Reader-Writer locks for concurrent read access
//! - Per-CPU data structures

#![no_std]
#![warn(missing_docs)]

extern crate alloc;

use core::sync::atomic::{AtomicBool, AtomicUsize, AtomicU32, AtomicU64, Ordering};
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
                let prev_node = prev_tail as *mut QspinlockNode;
                (*prev_node).next = node_ptr as *mut QspinlockNode;
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
        #[cfg(target_arch = "x86_64")]
        const INIT: UnsafeCell<Option<()>> = UnsafeCell::new(None);
        #[cfg(not(target_arch = "x86_64"))]
        const INIT: UnsafeCell<Option<()>> = UnsafeCell::new(None);
        
        Self {
            #[cfg(target_arch = "x86_64")]
            data: unsafe { core::mem::zeroed() },
            #[cfg(not(target_arch = "x86_64"))]
            data: unsafe { core::mem::zeroed() },
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

/// Reader-Writer spinlock for concurrent read access
///
/// Allows multiple readers or a single writer.
/// Readers are preferred to prevent writer starvation.
pub struct RwSpinlock {
    readers: AtomicUsize,
    writer: AtomicBool,
    write_waiters: AtomicUsize,
}

unsafe impl Send for RwSpinlock {}
unsafe impl Sync for RwSpinlock {}

impl RwSpinlock {
    /// Create a new RW spinlock
    pub const fn new() -> Self {
        Self {
            readers: AtomicUsize::new(0),
            writer: AtomicBool::new(false),
            write_waiters: AtomicUsize::new(0),
        }
    }
    
    /// Acquire read lock
    pub fn read_lock(&self) -> ReadGuard {
        loop {
            let current_readers = self.readers.fetch_add(1, Ordering::Acquire);
            
            // Check if a writer is active
            if self.writer.load(Ordering::Acquire) {
                // Rollback and retry
                self.readers.fetch_sub(1, Ordering::Release);
                core::hint::spin_loop();
                continue;
            }
            
            return ReadGuard { lock: self };
        }
    }
    
    /// Acquire write lock
    pub fn write_lock(&self) -> WriteGuard {
        self.write_waiters.fetch_add(1, Ordering::Relaxed);
        
        loop {
            // Check if we can acquire the write lock
            if !self.writer.swap(true, Ordering::Acquire) {
                // Got the writer flag, now wait for readers
                while self.readers.load(Ordering::Acquire) > 0 {
                    core::hint::spin_loop();
                }
                
                // Successfully acquired write lock
                self.write_waiters.fetch_sub(1, Ordering::Relaxed);
                return WriteGuard { lock: self };
            }
            
            // Release writer flag and wait
            self.writer.store(false, Ordering::Release);
            core::hint::spin_loop();
        }
    }
    
    /// Try to acquire read lock without blocking
    pub fn try_read_lock(&self) -> Option<ReadGuard> {
        if self.writer.load(Ordering::Acquire) {
            return None;
        }
        
        let current_readers = self.readers.fetch_add(1, Ordering::Acquire);
        if self.writer.load(Ordering::Acquire) {
            // Writer became active during our attempt
            self.readers.fetch_sub(1, Ordering::Release);
            return None;
        }
        
        Some(ReadGuard { lock: self })
    }
    
    /// Try to acquire write lock without blocking
    pub fn try_write_lock(&self) -> Option<WriteGuard> {
        if self.writer.swap(true, Ordering::Acquire) {
            return None;
        }
        
        if self.readers.load(Ordering::Acquire) > 0 {
            self.writer.store(false, Ordering::Release);
            return None;
        }
        
        Some(WriteGuard { lock: self })
    }
}

/// Read guard for RW spinlock
pub struct ReadGuard<'a> {
    lock: &'a RwSpinlock,
}

impl<'a> Drop for ReadGuard<'a> {
    fn drop(&mut self) {
        self.lock.readers.fetch_sub(1, Ordering::Release);
    }
}

/// Write guard for RW spinlock
pub struct WriteGuard<'a> {
    lock: &'a RwSpinlock,
}

impl<'a> Drop for WriteGuard<'a> {
    fn drop(&mut self) {
        self.lock.writer.store(false, Ordering::Release);
    }
}

/// Ticket-based RW lock with fairness guarantees
pub struct TicketRwLock {
    read_ticket: AtomicU64,
    read_serving: AtomicU64,
    write_ticket: AtomicU64,
    write_serving: AtomicU64,
    active_readers: AtomicU64,
}

unsafe impl Send for TicketRwLock {}
unsafe impl Sync for TicketRwLock {}

impl TicketRwLock {
    /// Create a new ticket RW lock
    pub const fn new() -> Self {
        Self {
            read_ticket: AtomicU64::new(0),
            read_serving: AtomicU64::new(0),
            write_ticket: AtomicU64::new(0),
            write_serving: AtomicU64::new(0),
            active_readers: AtomicU64::new(0),
        }
    }
    
    /// Acquire read lock
    pub fn read_lock(&self) -> TicketReadGuard {
        let ticket = self.read_ticket.fetch_add(1, Ordering::Relaxed);
        
        // Wait for our turn (readers can proceed if no exclusive writer)
        while self.write_serving.load(Ordering::Acquire) != ticket {
            core::hint::spin_loop();
        }
        
        self.active_readers.fetch_add(1, Ordering::Relaxed);
        
        // Allow next reader to start
        self.read_serving.fetch_add(1, Ordering::Release);
        
        TicketReadGuard { lock: self }
    }
    
    /// Acquire write lock
    pub fn write_lock(&self) -> TicketWriteGuard {
        let ticket = self.write_ticket.fetch_add(1, Ordering::Relaxed);
        
        // Wait for our turn
        while self.write_serving.load(Ordering::Acquire) != ticket {
            core::hint::spin_loop();
        }
        
        // Wait for all readers to finish
        while self.active_readers.load(Ordering::Acquire) > 0 {
            core::hint::spin_loop();
        }
        
        TicketWriteGuard { lock: self }
    }
}

/// Ticket read guard
pub struct TicketReadGuard<'a> {
    lock: &'a TicketRwLock,
}

impl<'a> Drop for TicketReadGuard<'a> {
    fn drop(&mut self) {
        self.lock.active_readers.fetch_sub(1, Ordering::Release);
    }
}

/// Ticket write guard
pub struct TicketWriteGuard<'a> {
    lock: &'a TicketRwLock,
}

impl<'a> Drop for TicketWriteGuard<'a> {
    fn drop(&mut self) {
        self.lock.write_serving.fetch_add(1, Ordering::Release);
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
    
    #[test]
    fn test_ticket_spinlock() {
        let lock = TicketSpinlock::new();
        let _guard = lock.lock();
        // Critical section - lock is held
        // Guard drops automatically, releasing lock
    }
    
    #[test]
    fn test_rw_spinlock() {
        let lock = RwSpinlock::new();
        
        // Multiple readers
        let r1 = lock.read_lock();
        let r2 = lock.read_lock();
        drop(r1);
        drop(r2);
        
        // Single writer
        let w = lock.write_lock();
        // Writer has exclusive access
        drop(w);
    }
    
    #[test]
    fn test_try_locks() {
        let lock = RwSpinlock::new();
        
        // Try read should succeed
        assert!(lock.try_read_lock().is_some());
        
        // Try write should fail (reader active)
        assert!(lock.try_write_lock().is_none());
        
        let lock2 = RwSpinlock::new();
        
        // Acquire write lock
        let _w = lock2.write_lock();
        
        // Try read should fail (writer active)
        assert!(lock2.try_read_lock().is_none());
        assert!(lock2.try_write_lock().is_none());
    }
    
    #[test]
    fn test_ticket_rwlock() {
        let lock = TicketRwLock::new();
        
        // Acquire and release read lock
        let r1 = lock.read_lock();
        drop(r1);
        
        // Acquire and release write lock
        let w = lock.write_lock();
        drop(w);
    }
}
