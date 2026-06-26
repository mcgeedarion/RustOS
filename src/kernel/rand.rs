//! Kernel randomness subsystem — canonical location: src/kernel/rand.rs

use core::sync::atomic::{AtomicU64, Ordering};

static STATE: AtomicU64 = AtomicU64::new(0);

#[cfg(target_arch = "x86_64")]
macro_rules! read_cycle_counter {
    () => {{
        let lo: u32;
        let hi: u32;
        unsafe {
            core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack));
        }
        ((hi as u64) << 32) | (lo as u64)
    }};
}

#[cfg(target_arch = "aarch64")]
macro_rules! read_cycle_counter {
    () => {{
        let cnt: u64;
        unsafe {
            core::arch::asm!("mrs {}, cntvct_el0", out(reg) cnt, options(nomem, nostack));
        }
        cnt
    }};
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
macro_rules! read_cycle_counter {
    () => {{ 0u64 }};
}

/// Mix a hardware cycle counter into the global PRNG state and return
/// a pseudo-random u64.
pub fn next_u64() -> u64 {
    let seed = read_cycle_counter!();
    // xorshift64 mix
    let mut x = STATE.load(Ordering::Relaxed) ^ seed;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    STATE.store(x, Ordering::Relaxed);
    x
}

/// Initialise the PRNG state from a seed (called once at boot).
pub fn seed(val: u64) {
    STATE.store(val, Ordering::Relaxed);
}
