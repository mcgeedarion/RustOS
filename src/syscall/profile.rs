//! Lightweight per-syscall profiling infrastructure.
//!
//! ## Design goals
//!
//! * **Zero overhead when disabled** — every public symbol is a no-op `#[inline(always)]` stub when
//!   `cfg(not(feature = "syscall-trace"))` is active.  The compiler folds all call sites away
//!   entirely.
//!
//! * **Near-zero overhead when enabled** — entry/exit hooks do one `AtomicU64::fetch_add(Relaxed)`
//!   for the counter and two `rdtsc` reads for timing.  No locks, no allocations.
//!
//! * **Multi-arch** — TSC on x86_64 and PMCCNTR_EL0 on AArch64. `PMCCNTR_EL0` on AArch64.  Falls
//!   back to a zero constant on unknown targets so the code compiles everywhere.
//!
//! ## Usage
//!
//! ```rust
//! let token = syscall_profile_enter(nr);
//! let ret   = do_real_syscall(nr, ...);
//! syscall_profile_exit(nr, token);
//! ```
//!
//! Or use the `SYSCALL_PROFILE!` macro defined at the bottom of this file.

// The maximum syscall number we track inline.  Linux x86_64 tops out around
// 450.  Numbers >= TABLE_SIZE are silently ignored (not counted).
pub const TABLE_SIZE: usize = 512;

// ---------------------------------------------------------------------------
// Enabled path
// ---------------------------------------------------------------------------
#[cfg(feature = "syscall-trace")]
mod enabled {
    use super::TABLE_SIZE;
    use core::sync::atomic::{AtomicU64, Ordering};

    /// One entry per syscall number.
    #[repr(C)]
    pub struct SyscallStat {
        /// Total invocation count.
        pub count: AtomicU64,
        /// Cumulative cycles (TSC delta on x86_64, hardware counter elsewhere).
        pub cycles: AtomicU64,
    }

    // AtomicU64 is not Copy/Clone, so we initialise with a const fn.
    impl SyscallStat {
        pub const fn new() -> Self {
            Self {
                count: AtomicU64::new(0),
                cycles: AtomicU64::new(0),
            }
        }
    }

    // Flat table — cache-line pad not needed at 16 bytes/entry on read-only
    // hot paths; we use Relaxed ordering so cacheline bouncing is minimal.
    #[used]
    pub static SYSCALL_STATS: [SyscallStat; TABLE_SIZE] = {
        // const evaluation: build array of zeroed entries.
        const ZERO: SyscallStat = SyscallStat::new();
        [ZERO; TABLE_SIZE]
    };

    // -----------------------------------------------------------------------
    // Architecture-specific cycle counter
    // -----------------------------------------------------------------------

    /// Read a high-resolution hardware cycle counter.
    #[inline(always)]
    pub fn read_cycles() -> u64 {
        #[cfg(target_arch = "x86_64")]
        {
            // SAFETY: rdtsc is always available on x86_64 and has no side
            // effects visible outside this CPU core.
            unsafe { core::arch::x86_64::_rdtsc() }
        }
        #[cfg(target_arch = "aarch64")]
        {
            let v: u64;
            unsafe {
                core::arch::asm!("mrs {}, PMCCNTR_EL0", out(reg) v, options(nomem, nostack));
            }
            v
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            0u64
        }
    }

    // -----------------------------------------------------------------------
    // Public entry / exit hooks
    // -----------------------------------------------------------------------

    /// Called at the very start of `dispatch_with_rip`, before routing.
    /// Returns the entry TSC value so `syscall_profile_exit` can diff it.
    #[inline(always)]
    pub fn syscall_profile_enter(nr: usize) -> u64 {
        if nr < TABLE_SIZE {
            SYSCALL_STATS[nr].count.fetch_add(1, Ordering::Relaxed);
        }
        read_cycles()
    }

    /// Called after the syscall returns, with the token from `enter`.
    #[inline(always)]
    pub fn syscall_profile_exit(nr: usize, enter_tsc: u64) {
        if nr < TABLE_SIZE {
            let delta = read_cycles().wrapping_sub(enter_tsc);
            SYSCALL_STATS[nr].cycles.fetch_add(delta, Ordering::Relaxed);
        }
    }

    // -----------------------------------------------------------------------
    // Debug dump helpers
    // -----------------------------------------------------------------------

    /// Formats the top-`n` syscalls by cumulative cycles into `buf`.
    /// Returns the number of bytes written.  Designed for serial-dump use.
    pub fn format_stats_top(buf: &mut [u8], top_n: usize) -> usize {
        use core::fmt::Write;
        struct Sink<'a> {
            buf: &'a mut [u8],
            pos: usize,
        }
        impl<'a> Write for Sink<'a> {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                let bytes = s.as_bytes();
                let avail = self.buf.len().saturating_sub(self.pos);
                let n = bytes.len().min(avail);
                self.buf[self.pos..self.pos + n].copy_from_slice(&bytes[..n]);
                self.pos += n;
                Ok(())
            }
        }

        // Snapshot all counters (no lock needed — Relaxed loads are fine for
        // a best-effort debug dump).
        let mut snap: [(u64, u64, usize); TABLE_SIZE] = [(0u64, 0u64, 0usize); TABLE_SIZE];
        for (i, stat) in SYSCALL_STATS.iter().enumerate() {
            snap[i] = (
                stat.cycles.load(Ordering::Relaxed),
                stat.count.load(Ordering::Relaxed),
                i,
            );
        }
        // Sort descending by cycles.
        snap.sort_unstable_by(|a, b| b.0.cmp(&a.0));

        let mut sink = Sink { buf, pos: 0 };
        let _ = write!(sink, "syscall_stats (top {top_n} by cycles)\n");
        let _ = write!(
            sink,
            "{:<6}  {:>12}  {:>16}  {:>14}\n",
            "NR", "invocations", "total_cycles", "avg_cycles"
        );
        let _ = write!(
            sink,
            "------  ------------  ----------------  --------------\n"
        );
        for &(cycles, count, nr) in snap.iter().take(top_n) {
            if count == 0 {
                break;
            }
            let avg = if count > 0 { cycles / count } else { 0 };
            let _ = write!(sink, "{nr:<6}  {count:>12}  {cycles:>16}  {avg:>14}\n");
        }
        sink.pos
    }

    /// Dump the top-20 syscalls over the debug serial port (or equivalent).
    /// Call from a debug command handler or on panic.
    pub fn dump_syscall_stats() {
        let mut buf = [0u8; 2048];
        let n = format_stats_top(&mut buf, 20);
        // Use the same serial write path the rest of the kernel uses.
        // If `crate::console` is not linked (early boot), this silently
        // does nothing.
        #[cfg(feature = "trace")]
        {
            if let Ok(s) = core::str::from_utf8(&buf[..n]) {
                crate::console::serial_print!("{}", s);
            }
        }
        let _ = n; // suppress unused warning when trace is off
    }
}

// Re-export the enabled symbols at crate level so call sites are identical
// regardless of the feature flag.
#[cfg(feature = "syscall-trace")]
pub use enabled::{
    dump_syscall_stats, format_stats_top, syscall_profile_enter, syscall_profile_exit, SyscallStat,
    SYSCALL_STATS,
};

// ---------------------------------------------------------------------------
// Disabled path — all no-ops, zero code generation
// ---------------------------------------------------------------------------

#[cfg(not(feature = "syscall-trace"))]
#[inline(always)]
pub fn syscall_profile_enter(_nr: usize) -> u64 {
    0
}

#[cfg(not(feature = "syscall-trace"))]
#[inline(always)]
pub fn syscall_profile_exit(_nr: usize, _token: u64) {}

#[cfg(not(feature = "syscall-trace"))]
#[inline(always)]
pub fn dump_syscall_stats() {}

// ---------------------------------------------------------------------------
// Convenience macro
// ---------------------------------------------------------------------------

/// Wraps a block with profiling enter/exit for the given syscall number.
///
/// ```rust
/// let ret = syscall_profile!(nr, { dispatch_to_handler(nr, ...) });
/// ```
///
/// When `syscall-trace` is off this expands to just the inner block with
/// no extra instructions.
#[macro_export]
macro_rules! syscall_profile {
    ($nr:expr, $body:block) => {{
        let __token = $crate::syscall::profile::syscall_profile_enter($nr);
        let __ret = $body;
        $crate::syscall::profile::syscall_profile_exit($nr, __token);
        __ret
    }};
}
