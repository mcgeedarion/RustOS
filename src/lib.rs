//! RustOS kernel crate root.

#![no_std]
#![feature(alloc_error_handler)]
#![feature(naked_functions)]
#![feature(asm_const)]
#![feature(thread_local)]
// These lints are intentionally set at the crate root so they apply
// globally.  They will surface the known high-complexity functions
// (dispatch_with_rip, sys_ptrace_impl, sys_clone3, bpf_run) during
// normal `cargo clippy` runs, making complexity regressions visible
// in CI before they are merged.
// Rationale per lint:
//   cognitive_complexity  — catches functions with deeply nested
//                           conditionals / match arms (the primary issue
//                           in the syscall dispatcher).
//   too_many_arguments    — flags functions with more than 7 parameters;
//                           dispatch_with_rip (8 params) is the first
//                           violation.
//   match_same_arms       — warns when two match arms have identical
//                           bodies (NR 118 / NR 119 duplication).
//   large_enum_variant    — prevents accidentally large enum variants
//                           from inflating stack usage in the hot dispatch
//                           path.
#![deny(clippy::cognitive_complexity)]
#![deny(clippy::too_many_arguments)]
#![warn(clippy::match_same_arms)]
#![warn(clippy::large_enum_variant)]

// Always pull in alloc — userspace_boot.rs needs it for the bump allocator's
// GlobalAlloc impl, and the full kernel needs it for collections everywhere.
extern crate alloc;

// Organised by kernel layer (outermost = most dependent on others):
//   core        — Zero-dependency foundation (error types, panic, cpu-local,
//                 intrusive collections).  Everything may depend on this.
//   arch        — Architecture-specific code (x86_64, riscv64)
//   firmware    — Platform firmware interfaces (ACPI, Device Tree)
//   device      — Hardware-neutral bus manager (PCI, future: platform, USB)
//   irq         — Interrupt controllers (PLIC, CLINT; arch-gated)
//   mm          — Memory management (PMM, heap, slab, mmap, swap, allocator)
//   sync        — Synchronisation primitives (spinlock, mutex, condvar)
//   drivers     — Hardware drivers (virtio, NIC, AHCI, NVMe, PCIe, USB, …)
//   display     — Display stack (DRM/KMS object model + Wayland compositor)
//   fs          — Filesystem layer (VFS, ext2, FAT32, initramfs mount)
//   net         — Network stack (TCP/UDP/IP, DHCP, DNS, sockets)
//   block       — Block layer (I/O scheduler, bio abstraction)
//   tty         — TTY/PTY subsystem (ldisc, termios, pts, serial COM1)
//   input       — Input event subsystem  [cfg(feature = "input_events")]
//   console     — Kernel console (printk destination)
//   proc        — Process management (scheduler, exec, wait, signals,
//   namespaces)   syscall     — Syscall dispatch table
//   ipc         — IPC (pipes, FIFOs, System V IPC, POSIX MQ)
//   io_uring    — io_uring async I/O ring
//   time        — Timekeeping (clocksource, timerfd, itimers)
//   smp         — SMP / multi-core bringup
//   security   — Security hardening (ASLR, stack canaries, seccomp, cgroups)
//   init        — Early-boot: initramfs, ELF loader, crt0
//   exec        — Executable format parsers (ELF-64)
//   debug       — Debugging infrastructure  [cfg(gdbstub | debug | trace)]
//   kernel      — Core kernel utilities (panic, rand, uaccess, utils)
//   kmtest      — Kernel test harness  [cfg(feature = "kmtest")]
//   boot_perf   — Boot performance markers (boot_mark! macro)

pub mod arch;
pub mod boot_perf;
pub mod console;
pub mod init;
pub mod kernel;

#[cfg(feature = "boot_minimal")]
pub mod boot_minimal;

#[cfg(feature = "userspace_boot")]
pub mod userspace_boot;

// ---------------------------------------------------------------------------
// Stub modules compiled in under userspace_boot.
//
// src/userspace_boot.rs calls into crate::fs::initramfs and crate::proc::exec
// but the real subsystems are gated out (they depend on the full mm/driver
// stack which is still API-inconsistent — see Cargo.toml comment).  These
// thin shims make the crate compile under the default feature set without
// touching the ~310-error legacy path.
//
// Replace with `pub use` of the real modules once fs/proc are un-gated
// (Phase 3).
// ---------------------------------------------------------------------------

/// Filesystem shim — no-op initramfs mount stub.
///
/// Replaces `pub mod fs` from the full kernel when only `userspace_boot` is
/// active.  The real VFS/ext2/initramfs stack is compiled in only when
/// neither `boot_minimal` nor `userspace_boot` is set (see gates below).
#[cfg(feature = "userspace_boot")]
pub mod fs {
    pub mod initramfs {
        /// No-op stub.  The CPIO archive is accessed directly through
        /// `crate::init::initramfs::load()` + `InitramfsHandle::file()`;
        /// a full VFS mount is not required for the userspace_boot milestone.
        pub fn mount_initramfs() {}
    }
}

/// Process-management shim — no-op exec stub.
///
/// Replaces `pub mod proc` from the full kernel when only `userspace_boot` is
/// active.  Returns `false` (spawn failed) so the caller panics with an
/// actionable message rather than silently continuing.
#[cfg(feature = "userspace_boot")]
pub mod proc {
    pub mod exec {
        /// Stub implementation.  Always returns `false` so `userspace_boot::enter`
        /// emits a clear panic message.  Wire up the real PCB/scheduler path
        /// in Phase 3 when `proc` is un-gated.
        pub fn spawn_user_process_from_bytes(
            _path: &str,
            _elf: &[u8],
            _argv: &[&str],
            _envp: &[&str],
        ) -> bool {
            false
        }
    }
}

#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod block;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod core;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod device;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod display;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod drivers;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod exec;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod firmware;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod fs;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub use exec::elf;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub use init::initramfs;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub use kernel::rand;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub use kernel::uaccess;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub use mm::allocator;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod io_uring;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod ipc;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod irq;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod mm;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod net;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod proc;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod security;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod smp;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod sync;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod syscall;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod time;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod tty;

// Feature-gated subsystems — only compiled when the matching flag is set.
#[cfg(feature = "input_events")]
pub mod input;

#[cfg(any(feature = "gdbstub", feature = "debug", feature = "trace"))]
pub mod debug;
#[cfg(feature = "gdbstub")]
pub use debug::gdbstub;

#[cfg(feature = "kmtest")]
pub mod kmtest;

pub use kernel_main::kernel_main;
mod kernel_main;
