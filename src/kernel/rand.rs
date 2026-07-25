//! Kernel randomness subsystem — canonical location: src/kernel/rand.rs

use core::sync::atomic::{AtomicU64, Ordering};

static STATE: AtomicU64 = AtomicU64::new(0);

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

pub fn seed_from_hw() {
    let raw = hw_seed_raw();
    let seed = raw ^ 0xDEAD_BEEF_CAFE_BABE_u64;
    let seed = if seed == 0 {
        0xDEAD_BEEF_CAFE_BABE_u64
    } else {
        seed
    };
    STATE.store(seed, Ordering::Relaxed);
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

pub fn next_u64() -> u64 {
    loop {
        let s = STATE.load(Ordering::Relaxed);
        let s1 = s ^ (s << 13);
        let s1 = s1 ^ (s1 >> 7);
        let s1 = s1 ^ (s1 << 17);
        match STATE.compare_exchange_weak(s, s1, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return s1,
            Err(_) => core::hint::spin_loop(),
        }
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
