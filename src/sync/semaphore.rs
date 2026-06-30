use core::sync::atomic::{AtomicIsize, Ordering};

/// Minimal counting semaphore for in-kernel tests and simple non-IRQ paths.
pub struct Semaphore {
    count: AtomicIsize,
}

impl Semaphore {
    pub const fn new(count: isize) -> Self {
        Self {
            count: AtomicIsize::new(count),
        }
    }

    pub fn try_down(&self) -> bool {
        let mut current = self.count.load(Ordering::Acquire);
        while current > 0 {
            match self.count.compare_exchange_weak(
                current,
                current - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(next) => current = next,
            }
        }
        false
    }

    pub fn down(&self) {
        while !self.try_down() {
            core::hint::spin_loop();
        }
    }

    pub fn up(&self) {
        self.count.fetch_add(1, Ordering::Release);
    }
}
