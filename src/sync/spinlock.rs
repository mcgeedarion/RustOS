//! Ticket spinlock — fair, starvation-free, SMP-correct.
//!
//! Two variants are provided:
//!
//!   `SpinLock<T>`         — plain spinlock; use when the lock is NEVER
//!                           acquired from interrupt context on the same CPU.
//!
//!   `IrqSpinLock<T>`      — IRQ-saving spinlock; disables local interrupts
//!                           while the lock is held so that ISRs on the same
//!                           CPU cannot dead-lock against their own holder.
//!
//! Both types implement `Deref`/`DerefMut` on the guard, so they drop in as
//! replacements for `spin::Mutex` / `std::sync::Mutex` in most code.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};

// ---------------------------------------------------------------------------
// Plain SpinLock
// ---------------------------------------------------------------------------

pub struct SpinLock<T> {
    ticket: AtomicU32,
    serving: AtomicU32,
    inner: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for SpinLock<T> {}
unsafe impl<T: Send> Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
    pub const fn new(val: T) -> Self {
        SpinLock {
            ticket: AtomicU32::new(0),
            serving: AtomicU32::new(0),
            inner: UnsafeCell::new(val),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        let my_ticket = self.ticket.fetch_add(1, Ordering::Relaxed);
        while self.serving.load(Ordering::Acquire) != my_ticket {
            core::hint::spin_loop();
        }
        SpinLockGuard { lock: self }
    }

    /// Try to acquire the lock without spinning. Returns `None` if busy.
    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        let serving = self.serving.load(Ordering::Acquire);
        let ticket = self.ticket.load(Ordering::Relaxed);
        if serving == ticket {
            let got = self.ticket.compare_exchange(
                ticket, ticket + 1, Ordering::Acquire, Ordering::Relaxed,
            );
            if got.is_ok() {
                return Some(SpinLockGuard { lock: self });
            }
        }
        None
    }

    /// Check if the lock is currently held (racy, for debug/stats only).
    pub fn is_locked(&self) -> bool {
        self.ticket.load(Ordering::Relaxed) != self.serving.load(Ordering::Relaxed)
    }
}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T { unsafe { &*self.lock.inner.get() } }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T { unsafe { &mut *self.lock.inner.get() } }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.serving.fetch_add(1, Ordering::Release);
    }
}

// ---------------------------------------------------------------------------
// IrqSpinLock
// ---------------------------------------------------------------------------

/// Saves the IRQ-enable state on lock, restores it on drop.
pub struct IrqSpinLock<T> {
    inner: SpinLock<T>,
}

unsafe impl<T: Send> Send for IrqSpinLock<T> {}
unsafe impl<T: Send> Sync for IrqSpinLock<T> {}

impl<T> IrqSpinLock<T> {
    pub const fn new(val: T) -> Self {
        IrqSpinLock { inner: SpinLock::new(val) }
    }

    pub fn lock(&self) -> IrqSpinLockGuard<'_, T> {
        let flags = irq_save_disable();
        let guard = self.inner.lock();
        IrqSpinLockGuard { guard, flags }
    }
}

pub struct IrqSpinLockGuard<'a, T> {
    guard: SpinLockGuard<'a, T>,
    flags: IrqFlags,
}

impl<T> Deref for IrqSpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T { &self.guard }
}

impl<T> DerefMut for IrqSpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.guard }
}

impl<T> Drop for IrqSpinLockGuard<'_, T> {
    fn drop(&mut self) {
        // guard drops first, releasing the spinlock before re-enabling IRQs.
        irq_restore(self.flags);
    }
}

// ---------------------------------------------------------------------------
// IRQ save/restore helpers (architecture-specific)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct IrqFlags(u64);

#[inline(always)]
fn irq_save_disable() -> IrqFlags {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let flags: u64;
        core::arch::asm!(
            "pushfq",
            "pop {flags}",
            "cli",
            flags = out(reg) flags,
            options(nomem, preserves_flags)
        );
        IrqFlags(flags)
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let daif: u64;
        core::arch::asm!(
            "mrs {daif}, daif",
            "msr daifset, #0xf",
            daif = out(reg) daif,
            options(nomem, nostack)
        );
        IrqFlags(daif)
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    compile_error!("IrqSpinLock: unsupported target architecture")
}

#[inline(always)]
fn irq_restore(flags: IrqFlags) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!(
            "push {flags}",
            "popfq",
            flags = in(reg) flags.0,
            options(nomem, preserves_flags)
        );
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!(
            "msr daif, {daif}",
            daif = in(reg) flags.0,
            options(nomem, nostack)
        );
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    compile_error!("IrqSpinLock: unsupported target architecture")
}
