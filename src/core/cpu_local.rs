//! Per-CPU variable accessor — `CpuLocal<T>`.
//!
//! # Approach
//!
//! Each logical CPU stores a pointer to its own `PerCpuBlock` in a
//! dedicated register:
//!
//! | Architecture | Register | Convention |
//! |---|---|---|
//! | x86_64  | `%gs`  | `SWAPGS` swaps to the kernel GS base on entry |
//!
//! `CpuLocal<T>` is a zero-sized marker; the actual per-CPU memory is
//! allocated inside `PerCpuBlock` by the SMP bringup code
//! (`src/smp/percpu.rs`).  This module only provides the *accessor* API
//! so there are no circular dependencies.
//!
//! # Safety contract
//!
//! * `CpuLocal::get()` is `unsafe` — the caller must guarantee that preemption is disabled
//!   (interrupts off or a preemption guard held) for the duration of the access, or that `T` is
//!   `Sync + Copy`.
//! * The per-CPU pointer **must** be initialised by SMP code before any `get()` call.  Calling
//!   `get()` before init is undefined behaviour.

use core::marker::PhantomData;

/// Maximum number of per-CPU slots.  Increase as subsystems register more.
pub const MAX_CPU_LOCAL_SLOTS: usize = 64;

/// Use the SMP block rather than interpreting an incompatible layout at GS/TP.
pub use crate::smp::percpu::PercpuBlock as PerCpuBlock;

impl PerCpuBlock {
    #[inline]
    pub unsafe fn read_slot(&self, index: usize) -> usize {
        assert!(index < MAX_CPU_LOCAL_SLOTS);
        unsafe { *self.cpu_local_slots[index].get() }
    }

    /// Write `value` into slot `index`.
    #[inline]
    pub unsafe fn write_slot(&self, index: usize, value: usize) {
        assert!(index < MAX_CPU_LOCAL_SLOTS);
        unsafe { *self.cpu_local_slots[index].get() = value };
    }
}

/// A compile-time-assigned index into the per-CPU slot array.
#[derive(Clone, Copy)]
pub struct CpuLocalKey {
    index: usize,
}

impl CpuLocalKey {
    #[doc(hidden)]
    pub const fn new(index: usize) -> Self {
        assert!(index < MAX_CPU_LOCAL_SLOTS);
        Self { index }
    }
}

/// Declare a per-CPU variable slot.
#[macro_export]
macro_rules! cpu_local_key {
    ($idx:expr) => {
        $crate::core::cpu_local::CpuLocalKey::new($idx)
    };
}

/// Zero-sized handle to a per-CPU value of type `T` stored in a slot.
pub struct CpuLocal<T: Copy + 'static> {
    key: CpuLocalKey,
    _marker: PhantomData<T>,
}

// SAFETY: the per-CPU discipline guarantees each CPU has exclusive access.
unsafe impl<T: Copy + 'static> Sync for CpuLocal<T> {}

impl<T: Copy + 'static> CpuLocal<T> {
    pub const fn new(key: CpuLocalKey) -> Self {
        assert!(core::mem::size_of::<T>() <= core::mem::size_of::<usize>());
        Self {
            key,
            _marker: PhantomData,
        }
    }

    /// Return the per-CPU value for the **current** CPU.
    #[inline]
    pub unsafe fn get(&self) -> T {
        let block = unsafe { current_cpu_block() };
        assert!(!block.is_null(), "CPU-local access before per-CPU initialization");
        let raw = unsafe { (*block).read_slot(self.key.index) };
        unsafe { core::mem::transmute_copy(&raw) }
    }

    /// Store `value` into the per-CPU slot for the **current** CPU.
    #[inline]
    pub unsafe fn set(&self, value: T) {
        let block = unsafe { current_cpu_block() };
        assert!(!block.is_null(), "CPU-local access before per-CPU initialization");
        let mut raw = 0usize;
        // transmute_copy::<T, usize> would read beyond a small T such as bool.
        unsafe {
            core::ptr::copy_nonoverlapping(
                (&value as *const T).cast::<u8>(),
                (&mut raw as *mut usize).cast::<u8>(),
                core::mem::size_of::<T>(),
            );
        }
        unsafe { (*block).write_slot(self.key.index, raw) };
    }
}

/// Retrieve the `PerCpuBlock` pointer for the executing CPU.
#[inline]
pub unsafe fn current_cpu_block() -> *mut PerCpuBlock {
    crate::smp::percpu::current_block()
}
