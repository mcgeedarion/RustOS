//! /proc/syscall_stats — exposes per-syscall counters collected by
//! `crate::syscall::profile` when the `syscall-trace` feature is enabled.
//!
//! When the feature is **off** the file still exists but always returns an
//! informative placeholder so user-space tools don't break.
//!
//! ## File format
//!
//! ```text
//! # syscall_stats: NR invocations total_cycles avg_cycles
//! 0 14023 2803201 199
//! 1 8901  1120453 125
//! ...
//! ```
//!
//! Only syscall numbers with at least one invocation are emitted.

use alloc::{string::String, vec::Vec};

/// Generate the full text content of /proc/syscall_stats.
pub fn read_syscall_stats() -> String {
    #[cfg(feature = "syscall-trace")]
    {
        use crate::syscall::profile::{SYSCALL_STATS, TABLE_SIZE};
        use core::sync::atomic::Ordering;

        let mut lines: Vec<(u64, u64, usize)> = SYSCALL_STATS
            .iter()
            .enumerate()
            .filter_map(|(nr, stat)| {
                let count = stat.count.load(Ordering::Relaxed);
                if count == 0 {
                    return None;
                }
                let cycles = stat.cycles.load(Ordering::Relaxed);
                Some((cycles, count, nr))
            })
            .collect();

        // Sort descending by total cycles so hot paths appear first.
        lines.sort_unstable_by(|a, b| b.0.cmp(&a.0));

        let mut out = String::from("# syscall_stats: NR invocations total_cycles avg_cycles\n");
        for (cycles, count, nr) in lines {
            let avg = if count > 0 { cycles / count } else { 0 };
            out.push_str(&alloc::format!("{nr} {count} {cycles} {avg}\n"));
        }
        out
    }
    #[cfg(not(feature = "syscall-trace"))]
    {
        String::from(
            "# syscall-trace feature not enabled.\n\
             # Rebuild with --features syscall-trace to collect data.\n",
        )
    }
}

// The procfs generator calls `read_syscall_stats` directly for `/proc/syscall_stats`.
