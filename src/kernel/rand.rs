//! Kernel randomness subsystem — canonical location: src/kernel/rand.rs
//!
//! Provides a fast xorshift-based PRNG with improved entropy mixing from
//! multiple hardware sources. For cryptographic security, combine with
//! additional entropy sources or use a CSPRNG.

use core::sync::atomic::{AtomicU64, Ordering};

/// Per-CPU RNG state would be ideal, but for simplicity we use a single
/// atomic state. In high-contention scenarios, consider per-CPU storage.
static STATE: AtomicU64 = AtomicU64::new(0);

/// Cache-line padded atomic to prevent false sharing on multi-core systems
#[repr(C)]
struct PaddedAtomicU64 {
    inner: AtomicU64,
    _pad: [u8; 64 - core::mem::size_of::<AtomicU64>()],
}

#[cfg(target_arch = "x86_64")]
macro_rules! rdrand_asm {
    () => {{
        let result: u64; let ok: u8;
        unsafe {
            core::arch::asm!(
                "xor {cnt:e}, {cnt:e}", "2:", "rdrand {v}", "jc 3f",
                "inc {cnt:e}", "cmp {cnt:e}, 10", "jl 2b",
                "xor {ok}, {ok}", "jmp 4f", "3:", "mov {ok}, 1", "4:",
                v = out(reg) result, ok = out(reg_byte) ok, cnt = out(reg) _,
                options(nostack, nomem)
            );
        }
        (result, ok != 0)
    }};
}

/// Reads APIC ID on x86_64 for additional entropy
#[cfg(target_arch = "x86_64")]
fn read_apic_id() -> u32 {
    #[cfg(feature = "debug")]
    {
        let eax: u32;
        unsafe {
            core::arch::asm!(
                "cpuid",
                in("eax") 1,
                out("ebx") eax,
                out("ecx") _,
                out("edx") _,
                options(nostack, preserves_flags)
            );
        }
        // APIC ID is in bits 31:24 of EBX for CPUID leaf 1
        (eax >> 24) & 0xFF
    }
    #[cfg(not(feature = "debug"))]
    {
        // Simplified version without cpuid overhead
        0
    }
}

pub fn seed_from_hw() {
    let raw = hw_seed_raw();
    
    #[cfg(target_arch = "x86_64")]
    {
        // Combine multiple entropy sources for better seeding
        let (rdrand_val, rdrand_ok) = rdrand_asm!();
        let apic_id = read_apic_id() as u64;
        
        let mut seed = raw ^ 0xDEAD_BEEF_CAFE_BABE_u64;
        if rdrand_ok {
            // Mix RDRAND output with golden ratio constant for better distribution
            seed = seed.wrapping_add(rdrand_val.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        }
        seed ^= apic_id.wrapping_mul(0x7F4A_7C15_9E37_79B9);
        
        let seed = if seed == 0 {
            0xDEAD_BEEF_CAFE_BABE_u64
        } else {
            seed
        };
        STATE.store(seed, Ordering::Release);
    }
    
    #[cfg(target_arch = "aarch64")]
    {
        let seed = if raw == 0 {
            0xDEAD_BEEF_CAFE_BABE_u64
        } else {
            raw ^ 0xDEAD_BEEF_CAFE_BABE_u64
        };
        STATE.store(seed, Ordering::Release);
    }
}

#[deprecated(note = "renamed to seed_from_hw()")]
#[inline]
pub fn seed_from_tsc() {
    seed_from_hw();
}

#[inline]
fn hw_seed_raw() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let lo: u32;
        let hi: u32;
        core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nostack, nomem, preserves_flags));
        (hi as u64) << 32 | lo as u64
    }
    #[cfg(target_arch = "aarch64")]
    {
        crate::arch::aarch64::hal::time_now_cycles()
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    compile_error!("rand::hw_seed_raw: unsupported architecture")
}

pub fn arch_entropy() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        let (r, ok) = rdrand_asm!();
        if ok {
            return r;
        }
        crate::println!(
            "WARN: rand: RDRAND failed 10\u{d7} \u{2014} falling back to xorshift entropy"
        );
        next_u64()
    }
    // Mix the architecture timer into the xorshift state before sampling so
    // that the output is not linearly predictable from user-visible counters.
    #[cfg(target_arch = "aarch64")]
    {
        let hw = crate::arch::aarch64::hal::time_now_cycles();
        let s = STATE.load(Ordering::Relaxed);
        let mixed = (s ^ hw).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let _ = STATE.compare_exchange_weak(s, mixed, Ordering::Relaxed, Ordering::Relaxed);
        next_u64() ^ hw.rotate_left(17)
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    compile_error!("rand::arch_entropy: unsupported architecture")
}

/// Generates the next random u64 using xorshift algorithm.
/// Uses proper memory ordering to prevent race conditions under contention.
pub fn next_u64() -> u64 {
    loop {
        // Use Acquire ordering to ensure we see all prior writes to STATE
        let s = STATE.load(Ordering::Acquire);
        let mut z = s;
        
        // Xorshift algorithm with better constants for improved statistical properties
        z ^= z << 13;
        z ^= z >> 7;
        z ^= z << 17;
        
        // Use Release ordering to publish the new state
        match STATE.compare_exchange_weak(s, z, Ordering::Release, Ordering::Acquire) {
            Ok(_) => return z,
            Err(_) => {
                // On contention, use spin_loop hint to reduce power consumption
                // and improve fairness with other CPUs
                core::hint::spin_loop();
            }
        }
    }
}

/// Re-seeds the RNG with additional entropy. Call periodically or on demand
/// to refresh the PRNG state with fresh hardware entropy.
pub fn reseed_from_hw() {
    let raw = hw_seed_raw();
    
    #[cfg(target_arch = "x86_64")]
    {
        let (rdrand_val, rdrand_ok) = rdrand_asm!();
        let apic_id = read_apic_id() as u64;
        
        let mut new_seed = raw ^ 0xDEAD_BEEF_CAFE_BABE_u64;
        if rdrand_ok {
            new_seed = new_seed.wrapping_add(rdrand_val.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        }
        new_seed ^= apic_id.wrapping_mul(0x7F4A_7C15_9E37_79B9);
        
        if new_seed == 0 {
            new_seed = 0xDEAD_BEEF_CAFE_BABE_u64;
        }
        
        // Mix with existing state rather than replacing entirely
        let old_state = STATE.fetch_xor(new_seed, Ordering::AcqRel);
        let _ = old_state; // Suppress unused warning
    }
    
    #[cfg(target_arch = "aarch64")]
    {
        let new_seed = if raw == 0 {
            0xDEAD_BEEF_CAFE_BABE_u64
        } else {
            raw ^ 0xDEAD_BEEF_CAFE_BABE_u64
        };
        
        let old_state = STATE.fetch_xor(new_seed, Ordering::AcqRel);
        let _ = old_state;
    }
}

#[cfg(target_arch = "x86_64")]
pub fn rdrand64() -> u64 {
    let (r, ok) = rdrand_asm!();
    if ok {
        r
    } else {
        next_u64()
    }
}

#[cfg(target_arch = "x86_64")]
pub fn rdrand_or_lfsr() -> u64 {
    rdrand64()
}
