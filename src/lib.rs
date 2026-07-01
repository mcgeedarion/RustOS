//! RustOS kernel crate root.
//!
//! # Build profiles
//!
//! Three mutually-exclusive feature combinations select the active module
//! tree:
//!
//! | Features active              | Profile                     |
//! |------------------------------|-----------------------------|
//! | `boot_minimal`               | First-stage boot only       |
//! | `userspace_boot`             | Boot + thin userspace shims |
//! | neither (default / release)  | Full kernel                 |
//!
//! The `arch`, `boot_perf`, `console`, `init`, `kernel`, and `smp` modules
//! compile under all three profiles because the panic handler, early console,
//! and BootInfo parser are needed everywhere.

#![no_std]
#![no_main]
#![allow(dead_code)]
#![allow(unused_imports)]
#![feature(alloc_error_handler)]
#![feature(naked_functions)]

extern crate alloc;

// ---------------------------------------------------------------------------
// Modules compiled under ALL build profiles
// ---------------------------------------------------------------------------

/// Architecture dispatch layer — cfg-routes to x86_64 or aarch64.
pub mod arch;

/// Full-kernel global allocator compatibility wiring.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod allocator;

/// Boot performance markers — `boot_mark!` macro, `read_hw_counter()`.
pub mod boot_perf;

/// Kernel console — `kprint!`, `kprintln!`, `serial_println!` macros.
pub mod console;

/// Early-boot / init subsystem — BootInfo, initramfs, loader, schemes.
pub mod init;

/// Optional initramfs bytes embedded by `xtask --initrd` builds.
pub mod embedded_initramfs {
    include!(concat!(env!("OUT_DIR"), "/initramfs_embed.rs"));
}

/// Compatibility re-export: `crate::initramfs::…` resolves to
/// `crate::init::initramfs::…`. Kept so any call sites outside the two
/// `uefi_entry.rs` files that haven't been updated to the `init::initramfs`
/// path don't silently break.
pub use init::initramfs;

/// Core kernel utilities — panic handler, rand, uaccess, architecture info.
pub mod kernel;

/// Compatibility alias for full-kernel modules that still import user-copy
/// helpers from `crate::uaccess` while the canonical home is
/// `crate::kernel::uaccess`.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub use kernel::uaccess;

/// Compatibility alias for full-kernel modules that still import RNG helpers
/// from `crate::rand` while the canonical home is `crate::kernel::rand`.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub use kernel::rand;

/// Symmetric multi-processing — per-CPU blocks, IPI send/dispatch.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod smp;

// ---------------------------------------------------------------------------
// boot_minimal profile
// ---------------------------------------------------------------------------

/// Shared first-stage boot path; includes the bump-allocator
/// `#[global_allocator]` for minimal images.
#[cfg(feature = "boot_minimal")]
pub mod boot_minimal;

// ---------------------------------------------------------------------------
// userspace_boot profile
// ---------------------------------------------------------------------------

/// Thin userspace-handoff boot path with minimal userspace services.
#[cfg(feature = "userspace_boot")]
pub mod userspace_boot;

/// Minimal fs/proc surfaces used by the `userspace_boot` profile while the
/// full kernel module graph remains gated out.
#[cfg(feature = "userspace_boot")]
pub mod userspace_shims;

#[cfg(feature = "userspace_boot")]
pub use userspace_shims::{fs, proc};

// ---------------------------------------------------------------------------
// Full-kernel modules (not boot_minimal, not userspace_boot)
// ---------------------------------------------------------------------------

/// Block device layer — request queues, bio, partition tables.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod block;

/// Kernel-internal core types (aliased as `kcore` to avoid shadowing `core`).
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
#[path = "core/mod.rs"]
pub mod kcore;

/// Compatibility alias for full-kernel modules that still refer to
/// `crate::core::...` while the canonical module name remains `kcore`.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub use kcore as core;

/// Debug subsystem — oops, backtraces, GDB RSP stub.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod debug;

/// Device model — device tree, platform bus, driver registration.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod device;

/// Display / DRM subsystem — connectors, modes, HDMI 2.1, ACP.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod display;

/// Peripheral drivers — AHCI, NVMe, virtio, keyboard, GPU.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod drivers;

/// ELF exec — `execve`, PT_LOAD mapping, aux-vector setup.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod exec;

/// Fault injection framework (requires `--features fault-inject`).
#[cfg(all(
    feature = "fault-inject",
    not(any(feature = "boot_minimal", feature = "userspace_boot"))
))]
pub mod fault_inject;

/// Firmware interfaces — ACPI, PSCI, device-tree parser stub.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod firmware;

/// Virtual filesystem — VFS, ext2, tmpfs, devfs, scheme table.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod fs;

/// Evdev / virtio-input event subsystem.
///
/// `input_events` enables the real device path; `kmtest` also compiles this
/// module so full-kernel tests that exercise `/dev/input/event*` helpers keep
/// their historical `crate::input` surface available.
#[cfg(all(
    any(feature = "input_events", feature = "kmtest"),
    not(any(feature = "boot_minimal", feature = "userspace_boot"))
))]
pub mod input;

/// io_uring — submission/completion ring, sqe/cqe dispatch.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod io_uring;

/// Inter-process communication — pipes, Unix sockets, futex.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod ipc;

/// Interrupt request routing — IRQ descriptors, APIC/GIC registration.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod irq;

/// Architecture-independent kernel entry-point dispatcher.
pub mod kernel_main;

/// In-kernel test harness (requires `--features kmtest`).
#[cfg(all(
    feature = "kmtest",
    not(any(feature = "boot_minimal", feature = "userspace_boot"))
))]
pub mod kmtest;

/// Memory management — physical allocator, VMM, page tables, heap.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod mm;

/// Networking — TCP/IP stack, socket layer, NIC drivers.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod net;

/// Process management — scheduler, signals, wait queues, namespaces.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod proc;

/// Security — capabilities, seccomp, namespace isolation.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod security;

/// Synchronisation primitives — Mutex, RwLock, Once, condvar.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod sync;

/// System call dispatch table and per-syscall implementations.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod syscall;

/// Timekeeping — monotonic clock, wall clock, POSIX timers, timerfd.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod time;

/// TTY / PTY subsystem — line discipline, pts, serial console.
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod tty;
