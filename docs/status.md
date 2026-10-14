# RustOS Subsystem Status

_Last reviewed: 2026-10-13._

This file is the authoritative high-level status registry. The default Cargo
feature set is `full_kernel`, which now selects the normal subsystem graph with
UEFI firmware entry independent of the minimal and diagnostic profiles.

Selecting the full graph is a build-configuration fact, not evidence that a
full userspace boot is supported. The real execution gate requires the kernel
to launch `/init`, successfully exec `/smoke`, reap that exact child, and
observe a normal exit with status zero. The repository now contains an ELF image builder, initial-stack construction, `spawn_user_process()`, and an `execve` image-replacement path. These are implementation paths, not verified end-to-end support: process scheduling, architecture return state, address-space lifetime, child reaping, and the full QEMU execution contract still require runtime validation. Do not promote full boot, fork/exec, or userspace compatibility claims until the per-architecture QEMU execution contract passes.

## Legend

| Status | Meaning |
|---|---|
| `real` | Implemented and used by a supported validation path |
| `partial` | Core path exists; important edge cases or integration work remain |
| `stub` | API/module surface exists but returns fixed values, no-op behavior, or `ENOSYS`-style placeholders |
| `experimental` | Substantial code exists but it is not part of the default smoke contract |
| `planned` | Tracked but not implemented in this repository yet |

## Build profiles

| Feature selection | Status | Notes |
|---|---|---|
| default `full_kernel` | partial, execution gate blocked | Selects the full subsystem cfg graph; this does not prove userspace process execution works |
| `boot_minimal` | real | Firmware handoff, `BootInfo`, early console, boot markers, idle loop |
| `userspace_boot` | partial | Raw initramfs/ELF diagnostics only; valid images report execution unavailable because full MM/VFS/scheduler services are excluded |
| full kernel graph | partial/experimental | Large module graph exists, but many subsystems are not yet supported smoke-contract claims |
| `release-boot` profile/feature | real | Lean image profile with LTO, opt-level=z, single codegen unit; size baselines tracked in CI |
| `boot_debug` feature | real | Gates verbose `log::debug!`/`log::trace!` during boot; off by default for performance |
| `syscall-trace` feature | partial | Per-syscall counters and TSC/PMCCNTR_EL0 timing via `/proc/syscall_stats` |

## Boot & Init

| Module | Status | Feature gate | Notes |
|---|---|---|---|
| `src/boot_minimal.rs` | real | `boot_minimal` | Emits the minimal boot success path and parks the CPU |
| `src/kernel_main.rs` | partial | all profiles | Common architecture-independent entry dispatcher and boot markers |
| `src/init/` | partial | all profiles | `BootInfo`, initramfs metadata, loader/scheme scaffolding |
| `src/userspace_boot.rs` | partial | `userspace_boot` | Checks for an initramfs and `/init`, then reports validation errors or unavailable execution; no PID-created or `BOOT_INIT_EXEC` success claim |
| `src/userspace_shims.rs` | stub | `userspace_boot` | Uses the real raw CPIO parser; validates ELF metadata and returns explicit `ExecutionUnavailable`, without pretend VFS mounting or scheduling |

## Architecture

| Module | Status | Notes |
|---|---|---|
| `src/arch/x86_64/` | partial | Primary UEFI boot path; syscall/interrupt/paging code exists for full-kernel work |
| `src/arch/aarch64/` | partial | UEFI and bare-metal paths exist; kept aligned with shared boot contracts |

## Memory Management

| Module | Status | Notes |
|---|---|---|
| `src/mm/` physical memory | partial | Allocator and boot-memory code exist; integration remains full-kernel only |
| `src/mm/` virtual memory/mmap | partial | VMA, mmap, page-fault, and protection modules exist; edge-case correctness remains under development |
| `src/mm/` slab/heap | partial | Kernel allocation code exists; production hardening remains |
| `crates/mm-core/` | partial | Workspace MM abstractions exist; full kernel integration continues in `src/mm/` |
| COW/swap/mlock/pkeys/KASAN helpers | experimental | Code exists but is not part of the default smoke contract |

## Syscalls

| Module | Status | Notes |
|---|---|---|
| `src/syscall/` dispatcher/routers | partial | Canonical router table plus an explicit AArch64/RISC-V number translation boundary; only a subset is mapped |
| `src/syscall/stubs.rs` | **real** | Documented ENOSYS stubs for deprecated/complex syscalls (remap_file_pages, kexec_file_load, bpf, userfaultfd) — intentionally unimplemented, not blockers for basic operation |
| `src/syscall/profile.rs` | partial | `syscall-trace` counters exist; performance numbers need fresh collection |
| POSIX compatibility modules | partial | `posix_full`, `musl_compat`, `openat2_mincore`, signal/fault helpers exist |

See `docs/syscalls.md` for syscall-by-syscall status.

## Filesystems & I/O

| Module | Status | Notes |
|---|---|---|
| `crates/vfs-core/` | partial | VFS traits and operations exist; writable guest workloads and IPC integration are not yet evidenced by the full-kernel gate |
| `crates/fs-ext4/` | partial | Crate-level host path returns NotSupported without `kernel_impl`; integrated `src/fs/ext4.rs` is read-only and does not establish journal replay or writable ext4 support |
| `crates/fs-fat32/` | partial | FAT32 adapter exists; mount/read/write support is not promoted without runtime coverage for its supported paths |
| `src/fs/vfs/` | partial | VFS data structures and POSIX operation helpers exist |
| `src/fs/initramfs.rs` | partial | Initramfs support exists for boot handoff work |
| `src/fs/devfs.rs`, `src/fs/procfs.rs`, `src/fs/sysfs.rs` | partial | Kernel pseudo-filesystem scaffolding exists; not fully validated as production interfaces |
| `src/fs/ext2/`, `src/fs/ext4.rs`, `src/fs/fat32.rs` | partial | Filesystem implementations integrated with new crate-based architecture |
| `src/fs/jbd2.rs` | partial | JBD2 code exists; journal recovery and replay are not promoted without executable filesystem-image regression tests |
| exFAT/NTFS/NFS/CDFS/overlayfs modules | experimental | Code exists for bring-up/integration, not a supported compatibility claim |
| `src/io_uring/` | experimental | Ring and operation modules exist; not part of current milestone gates |

## Process, IPC, and Scheduling

| Module | Status | Notes |
|---|---|---|
| `src/proc/` | partial | ELF image construction, initial stack setup, process spawn, fork mapping-copy, exec image replacement, and wait/exit paths exist; full userspace runtime remains unverified |
| `src/proc/signal.rs` | partial | Signal-handling code exists; full delivery, return, exec-reset and userspace ABI semantics are not end-to-end gated |
| `src/proc/wait.rs` | partial | Wait APIs exist; exact-child reap and status handling remain blocked on working userspace process creation/exec |
| `src/proc/creds.rs` | partial | Credential, process-group and session code exists; end-to-end compatibility is not yet runtime-gated |
| `src/proc/futex.rs` | partial | Futex code exists; correctness/performance validation remains |
| `src/fs/pipe.rs`, `src/net/socket/unix/` | partial | Pipe and Unix-socket code exists but is not fully milestone-gated |
| `crates/smp-core/` | experimental | Crate exists in-tree but is not currently a workspace member |
| `crates/sync-primitives/` | partial | Workspace synchronization primitives exist |
| `crates/load-balancer/` | experimental | Crate exists in-tree but is not currently a workspace member |
| IPC completeness helpers | partial | POSIX IPC code and tests exist; full compatibility remains under validation |

## Drivers & Devices

| Module | Status | Notes |
|---|---|---|
| `src/drivers/virtio/`, `src/drivers/virtio_blk.rs` | partial | VirtIO infrastructure and block code exist |
| `src/drivers/net/` | experimental | VirtIO/e1000e/NIC code exists; not a supported network milestone yet |
| `src/drivers/block/` | experimental | AHCI/NVMe modules exist; not CI-gated as storage support |
| `src/drivers/gpu/` | experimental | GOP/VGA/virtio/DRM/AMDGPU scaffolding exists; no supported Wayland compositor path yet |
| `src/drivers/input/`, `src/input/` | experimental | Input/event modules exist; real path requires optional feature/testing |
| `src/tty/` | partial | TTY and serial-console code exists in the full kernel graph |

## Networking

| Module | Status | Notes |
|---|---|---|
| `src/net/` Ethernet/IP/ARP/ICMP/UDP/TCP/DHCP/DNS | experimental | Protocol code exists but is not yet a supported M5 network claim |
| `src/net/socket/` | experimental | TCP/UDP/Unix socket modules exist; syscall integration remains partial |

## Security & Namespaces

| Module | Status | Notes |
|---|---|---|
| `src/security/` | experimental | Capabilities, seccomp, namespaces, LSM, ASLR, BPF, Landlock, cgroups scaffolding exists |
| `src/proc/*_ns.rs`, `src/security/ns/` | partial | Namespace-related code exists but is not complete compatibility support |

## Debugging, Tests, and Fault Injection

| Module | Status | Feature gate | Notes |
|---|---|---|---|
| `src/debug/` | partial | `debug`, `gdbstub`, `trace` | GDB RSP, trace, oops/debug console modules exist |
| `src/kmtest/` | **real** | `kmtest` | In-kernel tests with filesystem integrity, OOM recovery, and long-running stability tests |
| `userspace/smoke/src/main.rs` | partial | n/a | Architecture-specific syscall/entry stubs and assertions exist; the kernel still cannot exec `/smoke`, so no end-to-end userspace pass is established |
| `userspace/bench/src/main.rs` | implemented, runtime pending | n/a | Fixed warmups/iterations, `getpid` workload, monotonic timing and JSON statistics; execution and reviewed baseline remain pending |
| `scripts/stability_test.sh` | partial | n/a | Stability script exists; the current full-kernel execution gate has not established an end-to-end reliability result |
| `src/fault_inject/` | partial | `fault-inject` | PMM/VMM/syscall fault points exist for controlled error-path testing |
| `scripts/ci/check-stubs.sh` | real | n/a | Local documented-stub guard |
| `cargo xtask roadmap-check` | real | n/a | Documentation contract guard |

## Error Handling

| Area | Status | Notes |
|---|---|---|
| `src/proc/dynlink.rs` | **partial** | All unwrap() calls replaced with Result propagation |
| `src/debug/gdbstub/target.rs` | **partial** | Safe defaults instead of unwrap() |
| `src/io_uring/syscall.rs` | **partial** | Error recovery paths with safe defaults |
| Macro adoption | **partial** | New code uses try_opt!, try_result!, or ? operator |

See `docs/ERROR_HANDLING_AUDIT.md` for detailed audit results.

## Runtime promotion gates

| Gate | Current status | Evidence required before promotion |
|---|---|---|
| Full-kernel QEMU execution | blocked | Fresh QEMU log with all required `RUSTOS_EXEC` child markers, exact-PID wait/reap, normal exit status zero, and timing-parser success |
| CPIO/ELF/VFS/ext4 parser fuzzing | harnesses and seed corpora added, runtime pending | Explicit AddressSanitizer campaigns against production modules, retained logs/artifacts, minimized crash regressions |
| Fault recovery in the guest | not supported yet | Catalogued fault injection, fired/recovered markers, and post-recovery filesystem/process workload success |
| Writable filesystem and IPC workloads | not supported yet | A guest creates, writes, reads, renames and unlinks a file; IPC send/receive completes with checked exit statuses |
| Syscall benchmark workload | implemented, runtime pending | `/bench` emits schema-version-2 JSON with fixed iterations, counts, 256 batch latency samples (min/p50/p90/p99/max), and host/compiler/QEMU/guest metadata; strict smoke requires its completion marker |
| Reviewed benchmark baseline | not established | Repeated successful runs, variance characterization, and review of an equivalent architecture/compiler/QEMU configuration |

Do not upgrade any row to `real` based on source inspection, parser unit tests,
or a workflow definition alone. Require completed executable gates and retain
the logs and metadata used to make the decision.

## Current improvement roadmap

| Priority | Work item | Status | Validation |
|---:|---|---|---|
| 1 | Keep x86_64 minimal UEFI smoke green | independent gate | `cargo xtask smoke --arch x86_64 --features boot_minimal`; full-graph default smoke is a separate gate |
| 2 | Keep AArch64 minimal boot contracts buildable/smokeable where firmware exists | independent gate | `cargo xtask smoke --arch aarch64 --features boot_minimal` |
| 3 | Integrate the full graph with real process creation and image replacement | blocked | Full `cargo xtask smoke --arch x86_64` must pass the RUSTOS_EXEC child protocol plus the real-log timing parser |
| 4 | Make minimum init syscalls real and EFAULT-safe | partial | `docs/syscalls.md` plus kmtest coverage |
| 5 | Refresh boot-image and boot-performance baselines | advisory / review required | Parse actual QEMU logs; do not claim userspace timing until `BOOT_INIT_EXEC` and the execution markers are independently proven |
| 6 | Promote selected full-kernel subsystems from experimental to supported | **in progress** | VFS/ext4/FAT32, SMP, IPC completeness completed |
| 7 | Complete error handling audit across remaining modules | **in progress** | See ERROR_HANDLING_AUDIT.md |
| 8 | Expand testing coverage with syscall tests and stability scripts | **in progress** | `userspace/smoke`, `src/kmtest/`, scripts/stability_test.sh |
| 9 | Document intentionally unimplemented syscalls (ENOSYS by design) | **completed** | stubs.rs lines 408/422/430/438 — remap_file_pages, kexec_file_load, bpf, userfaultfd |
| 10 | Implement filesystem integrity tests for VFS/ext4/FAT32 | **completed** | `src/kmtest/fs_integrity.rs` with 15+ in-kernel integrity tests |
| 11 | Add long-running stability validation for memory and filesystem | **completed** | `src/kmtest/fs_integrity.rs`, `src/kmtest/oom_recovery.rs` with extended cycling tests |
| 12 | Add syscall conformance matrix with EFAULT hardening plan | **completed** | See `docs/M3_M5_ROADMAP.md` for detailed matrix and implementation patterns |
| 13 | Plan filesystem corruption/recovery tests for M4 gate | **completed** | Image corruption framework documented in `docs/M3_M5_ROADMAP.md` |
| 14 | Plan network stack promotion for M5 gate | **completed** | Three-phase ARP/ICMP→UDP→TCP plan in `docs/M3_M5_ROADMAP.md` |

## Contribution rules

1. Update this file when promoting or demoting a subsystem.
2. Update `docs/syscalls.md` with syscall semantic changes.
3. Boot success must be decided by serial sentinels, not fixed sleeps.
4. Prefer `cargo xtask check`, `cargo xtask smoke`, and `cargo xtask ci-local`
   over raw `cargo` commands for validation.
5. Performance docs must distinguish measured data from planned work.
6. All new code must use proper error handling (no unwrap()/expect() in data paths).
