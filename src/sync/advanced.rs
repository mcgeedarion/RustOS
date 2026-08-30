// src/sync/advanced.rs
//! Advanced Synchronization Primitives
//!
//! Implements ticket locks, reader-writer locks, and RCU (Read-Copy-Update)
//! for high-performance concurrent access patterns.

use core::sync::atomic::{AtomicU32, AtomicUsize, AtomicBool, Ordering};
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::ptr::NonNull;
use alloc::boxed::Box;

/// Ticket Lock Implementation
/// 
/// Provides fair FIFO ordering for lock acquisition, preventing starvation.
/// Useful in high-contention scenarios where spinlock fairness matters.
pub struct TicketLock {
    next_ticket: AtomicU32,
    now_serving: AtomicU32,
}

impl TicketLock {
    pub const fn new() -> Self {
        Self {
            next_ticket: AtomicU32::new(0),
            now_serving: AtomicU32::new(0),
        }
    }

    /// Acquire the lock, returns the ticket number
    pub fn lock(&self) -> u32 {
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        
        // Spin until our ticket is served
        while self.now_serving.load(Ordering::Acquire) != ticket {
            core::hint::spin_loop();
        }
        
        ticket
    }

    /// Release the lock, incrementing now_serving
    pub fn unlock(&self, _ticket: u32) {
        self.now_serving.fetch_add(1, Ordering::Release);
    }

    /// Try to acquire the lock without blocking
    pub fn try_lock(&self) -> Option<u32> {
        let current = self.now_serving.load(Ordering::Acquire);
        let expected = self.next_ticket.load(Ordering::Relaxed);
        
        if current == expected {
            // We can acquire if we're the next ticket
            if self.next_ticket.compare_exchange(
                expected,
                expected + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ).is_ok() {
                return Some(current);
            }
        }
        None
    }

    /// Check if the lock is currently held
    pub fn is_locked(&self) -> bool {
        self.now_serving.load(Ordering::Acquire) != self.next_ticket.load(Ordering::Relaxed)
    }
}

unsafe impl Sync for TicketLock {}
unsafe impl Send for TicketLock {}

/// Guard for TicketLock
pub struct TicketLockGuard<'a> {
    lock: &'a TicketLock,
    ticket: u32,
}

impl<'a> TicketLockGuard<'a> {
    fn new(lock: &'a TicketLock, ticket: u32) -> Self {
        Self { lock, ticket }
    }
}

impl<'a> Drop for TicketLockGuard<'a> {
    fn drop(&mut self) {
        self.lock.unlock(self.ticket);
    }
}

/// Reader-Writer Lock (Multiple Readers, Single Writer)
/// 
/// Allows multiple concurrent readers or a single exclusive writer.
/// Writers have priority to prevent starvation.
pub struct RwLock<T: ?Sized> {
    readers: AtomicUsize,
    writer: AtomicBool,
    write_waiters: AtomicUsize,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            readers: AtomicUsize::new(0),
            writer: AtomicBool::new(false),
            write_waiters: AtomicUsize::new(0),
            data: UnsafeCell::new(data),
        }
    }

    /// Acquire read lock
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        // If there are waiting writers, block new readers to prevent starvation
        while self.write_waiters.load(Ordering::Acquire) > 0 
            || self.writer.load(Ordering::Acquire) 
        {
            core::hint::spin_loop();
        }
        
        self.readers.fetch_add(1, Ordering::Acquire);
        
        RwLockReadGuard {
            data: unsafe { &*self.data.get() },
            readers: &self.readers,
        }
    }

    /// Acquire write lock
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        self.write_waiters.fetch_add(1, Ordering::Relaxed);
        
        // Wait until no readers and no other writer
        while self.readers.load(Ordering::Acquire) > 0 
            || self.writer.load(Ordering::Acquire) 
        {
            core::hint::spin_loop();
        }
        
        self.write_waiters.fetch_sub(1, Ordering::Relaxed);
        self.writer.store(true, Ordering::Release);
        
        RwLockWriteGuard {
            data: unsafe { &mut *self.data.get() },
            writer: &self.writer,
        }
    }

    /// Try to acquire read lock
    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
        if self.write_waiters.load(Ordering::Acquire) > 0 
            || self.writer.load(Ordering::Acquire) 
        {
            return None;
        }
        
        self.readers.fetch_add(1, Ordering::Acquire);
        
        Some(RwLockReadGuard {
            data: unsafe { &*self.data.get() },
            readers: &self.readers,
        })
    }

    /// Try to acquire write lock
    pub fn try_write(&self) -> Option<RwLockWriteGuard<'_, T>> {
        if self.readers.load(Ordering::Acquire) > 0 
            || self.writer.load(Ordering::Acquire) 
        {
            return None;
        }
        
        if self.writer.compare_exchange(
            false, true,
            Ordering::Acquire,
            Ordering::Relaxed,
        ).is_err() {
            return None;
        }
        
        Some(RwLockWriteGuard {
            data: unsafe { &mut *self.data.get() },
            writer: &self.writer,
        })
    }

    /// Get mutable reference without locking (requires exclusive access to RwLock itself)
    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }

    /// Consume the lock and return the inner data
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

/// Read guard for RwLock
pub struct RwLockReadGuard<'a, T: ?Sized> {
    data: &'a T,
    readers: &'a AtomicUsize,
}

impl<'a, T: ?Sized> core::ops::Deref for RwLockReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data
    }
}

impl<'a, T: ?Sized> Drop for RwLockReadGuard<'a, T> {
    fn drop(&mut self) {
        self.readers.fetch_sub(1, Ordering::Release);
    }
}

/// Write guard for RwLock
pub struct RwLockWriteGuard<'a, T: ?Sized> {
    data: &'a mut T,
    writer: &'a AtomicBool,
}

impl<'a, T: ?Sized> core::ops::Deref for RwLockWriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data
    }
}

impl<'a, T: ?Sized> core::ops::DerefMut for RwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.data
    }
}

impl<'a, T: ?Sized> Drop for RwLockWriteGuard<'a, T> {
    fn drop(&mut self) {
        self.writer.store(false, Ordering::Release);
    }
}

/// RCU (Read-Copy-Update) Pointer
/// 
/// Lock-free data structure for read-heavy workloads.
/// Readers access data without synchronization.
/// Writers create a copy, update it, then atomically swap the pointer.
/// 
/// Note: This is a simplified RCU implementation. Production use would need
/// proper grace period tracking and memory reclamation.
pub struct RcuPointer<T> {
    ptr: AtomicUsize,
    _marker: PhantomData<Box<T>>,
}

unsafe impl<T: Send + Sync> Send for RcuPointer<T> {}
unsafe impl<T: Send + Sync> Sync for RcuPointer<T> {}

impl<T> RcuPointer<T> {
    /// Create a new RCU pointer
    pub fn new(data: T) -> Self {
        let boxed = Box::new(data);
        let ptr = Box::into_raw(boxed) as usize;
        
        Self {
            ptr: AtomicUsize::new(ptr),
            _marker: PhantomData,
        }
    }

    /// Get a reference to the current data (read-side critical section)
    /// 
    /// Safety: Caller must ensure the data isn't freed while reference is held.
    /// In proper RCU, this requires being in an RCU read-side critical section.
    pub fn read(&self) -> Option<&T> {
        let ptr = self.ptr.load(Ordering::Acquire);
        if ptr == 0 {
            return None;
        }
        unsafe { Some(&*(ptr as *const T)) }
    }

    /// Update the data by creating a copy, modifying it, and swapping
    /// 
    /// Returns the old value which should be freed after a grace period.
    pub fn update<F>(&self, f: F) -> Option<*mut T>
    where
        F: FnOnce(&T) -> T,
    {
        loop {
            let current_ptr = self.ptr.load(Ordering::Acquire);
            if current_ptr == 0 {
                return None;
            }
            
            let current_ref = unsafe { &*(current_ptr as *const T) };
            let new_data = f(current_ref);
            let new_box = Box::new(new_data);
            let new_ptr = Box::into_raw(new_box) as usize;
            
            // Try to swap
            match self.ptr.compare_exchange(
                current_ptr,
                new_ptr,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    // Swap succeeded, return old pointer for later freeing
                    return Some(current_ptr as *mut T);
                },
                Err(_) => {
                    // Another thread updated, free our allocation and retry
                    unsafe { drop(Box::from_raw(new_ptr as *mut T)); }
                    // Loop and retry
                }
            }
        }
    }

    /// Replace the current value unconditionally
    pub fn replace(&self, new_data: T) -> Option<*mut T> {
        let new_box = Box::new(new_data);
        let new_ptr = Box::into_raw(new_box) as usize;
        
        let old_ptr = self.ptr.swap(new_ptr, Ordering::AcqRel);
        
        if old_ptr == 0 {
            None
        } else {
            Some(old_ptr as *mut T)
        }
    }

    /// Get the raw pointer (for advanced use)
    pub fn as_ptr(&self) -> *const T {
        self.ptr.load(Ordering::Acquire) as *const T
    }
}

impl<T> Drop for RcuPointer<T> {
    fn drop(&mut self) {
        let ptr = self.ptr.load(Ordering::Relaxed);
        if ptr != 0 {
            unsafe { drop(Box::from_raw(ptr as *mut T)); }
        }
    }
}

/// RCU Read-Side Critical Section Guard
/// 
/// Marks the beginning and end of an RCU read-side critical section.
/// In a full implementation, this would track grace periods.
pub struct RcuReadGuard<'a, T> {
    data: &'a T,
    _lifetime: PhantomData<&'a ()>,
}

impl<'a, T> core::ops::Deref for RcuReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data
    }
}

/// Simplified grace period tracker
/// 
/// In production, this would wait for all pre-existing readers to complete.
pub struct GracePeriod {
    counter: AtomicUsize,
}

impl GracePeriod {
    pub const fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }

    /// Start a grace period
    pub fn start(&self) {
        self.counter.fetch_add(1, Ordering::SeqCst);
    }

    /// End a grace period (simplified - just decrements)
    pub fn end(&self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }

    /// Check if grace period has elapsed
    pub fn elapsed(&self) -> bool {
        self.counter.load(Ordering::SeqCst) == 0
    }

    /// Wait for grace period (busy-wait, simplified)
    pub fn synchronize(&self) {
        while !self.elapsed() {
            core::hint::spin_loop();
        }
    }
}

/// Free old RCU data after grace period
pub unsafe fn rcu_free<T>(ptr: *mut T, gp: &GracePeriod) {
    gp.synchronize();
    drop(Box::from_raw(ptr));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ticket_lock_basic() {
        let lock = TicketLock::new();
        
        let ticket = lock.lock();
        assert!(lock.is_locked());
        lock.unlock(ticket);
        assert!(!lock.is_locked());
    }

    #[test]
    fn test_ticket_lock_try_lock() {
        let lock = TicketLock::new();
        
        assert!(lock.try_lock().is_some());
        assert!(lock.try_lock().is_none()); // Should fail
        
        let ticket = lock.lock();
        lock.unlock(ticket);
    }

    #[test]
    fn test_rwlock_read_multiple() {
        let rwlock = RwLock::new(42);
        
        let r1 = rwlock.read();
        let r2 = rwlock.read();
        
        assert_eq!(*r1, 42);
        assert_eq!(*r2, 42);
        // Both reads should succeed simultaneously
    }

    #[test]
    fn test_rwlock_write_exclusive() {
        let rwlock = RwLock::new(42);
        
        let mut w = rwlock.write();
        *w = 100;
        drop(w);
        
        let r = rwlock.read();
        assert_eq!(*r, 100);
    }

    #[test]
    fn test_rcu_pointer() {
        let rcu = RcuPointer::new(42);
        
        let val = rcu.read().unwrap();
        assert_eq!(*val, 42);
        
        // Update
        let old = rcu.update(|x| x + 10);
        assert!(old.is_some());
        
        let val = rcu.read().unwrap();
        assert_eq!(*val, 52);
    }
}
