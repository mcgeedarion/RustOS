# Subsystem status

This is the source-state summary for `08a14f6`, audited on 2026-11-01. “Implemented source” means an implementation is present; it does not mean the full kernel compiles or that an emulator or hardware test passed.

## Build and execution status

- **Full kernel**: Cargo defaults select `full_kernel -> uefi_boot` with neither slim flag active. The audited x86_64 UEFI check fails in the existing source, including the conflicting `net::socket` module candidates.
- **Minimal boot**: `boot_minimal` selects firmware handoff, early console, BootInfo reporting, and an idle loop. Its success sentinel is not userspace or full-subsystem coverage.
- **Init-image diagnostics**: `userspace_boot` locates `/init` and validates ELF metadata. It deliberately prints `USERSPACE_BOOT_UNSUPPORTED` instead of entering userspace.
- **Validation evidence**: Host checks, compilation, and runtime are reported separately in [the audit results](source-audit.md). No full-kernel QEMU success or hardware support claim is established by this documentation refresh.

## Source implementation matrix

| Area | Present in the source | Important boundary |
| --- | --- | --- |
| Architecture and boot | x86_64, AArch64, and RISC-V dispatch, UEFI entries, HALs, traps, paging, user-entry code | Build contracts differ by architecture; generic IRQ RISC-V module declares a missing `plic.rs` |
| Global allocation | Full-profile `RegionHeap` with free-region coalescing; slim-profile bump heaps | The full global allocator is not `mm::slab` or a workspace buddy allocator |
| PMM | Full-map initialization, bootstrap allocation, contiguous runs, buddy lists, page metadata/refcounts | Region registration is boot-only; late growth/hot-add is explicitly rejected |
| Kernel stacks | Three contiguous frames, guard mapping, generation-tagged ownership, retirement | Retired backing is retained until an external stopped-machine reclamation protocol is established |
| Virtual memory | VMAs, mmap/munmap/mprotect, CoW helpers, user-copy walks, RSS, pkeys | Some parallel MM implementations and fault wrappers are not declared in the live module tree |
| Processes | PCB table, scheduler queues, fork/exec, signals, credentials, exact-child wait/reap | `proc/clone.rs` is empty; RISC-V user signal-handler delivery is deliberately disabled |
| Exit lifetime | Snapshot table readers, identity-checked zombie removal, retained retired tasks | Logical PID removal is not safe physical reclamation of tasks or kernel stacks |
| VFS and schemes | Path operations, per-process FDs, backing FDs, URL schemes, mounts | Kernel VFS, kernel scheme table, and workspace VFS registry are separate systems |
| Filesystems | ext2/ext4, ramfs/tmpfs, procfs/sysfs, overlay, NFS, FAT32/Btrfs sources | FAT32 raw I/O is simulated; workspace filesystem crates are not production backend replacements |
| IPC and async I/O | Endpoints, proxy schemes, pipes, message queues, shared memory, semaphores, io_uring | Real guest pipe workload exists, but is not demonstrated by passing host parser tests |
| Networking | Ethernet, ARP, IPv4/IPv6, ICMP, UDP/TCP, DHCP/DNS, socket subtree | Both `socket.rs` and `socket/mod.rs` exist for one `mod socket;`, blocking full compilation |
| Device drivers | PCI/MSI-X, VirtIO, AHCI/NVMe, NICs, input, graphics, terminals | Presence does not establish hardware compatibility; VirtIO-MMIO network backend is a compatibility stub |
| Security | LSM/DAC, capsets, seccomp, namespaces, ASLR, canaries, PTI, SMEP/SMAP, BPF/Landlock subsets | BPF execution and seccomp notification/tracing have explicit unsupported cases |
| Debugging | Panic formatting, trace ring, profiling, RSP implementation files | `debug/gdbstub/mod.rs` is empty, so the RSP files are not wired into that module |
| Fault injection | Atomic fault-point engine, host engine tests, catalogue and parser contract | Hook wrapper files and guest fault-test file are not registered; no guest recovery gate is established |
| Userspace | init, smoke, bench, hello, kmtest, libc-shim, raw-syscall | x86_64 per-package init archive builds; aggregate workspace and smoke host mocks are inconsistent |

## Corrections to older descriptions

- **Exec is no longer absent**: `proc/exec.rs` contains staged ELF loading, stack preparation, image installation, and architecture-specific entry paths. The unresolved issue is integration and validation, not the absence of all exec code.
- **The full heap is reclaiming**: `allocator.rs` now installs `RegionHeap`; only slim profiles use the non-reclaiming bump allocator. See [memory management](memory-management.md).
- **PMM initialization uses the complete map**: Metadata sizing occurs before usable regions are published. The focused PMM host regression tests exercise this path.
- **Reaping preserves lifetime**: Exit and wait avoid the old recursive cleanup path and retain scheduler-visible task storage. Safe eventual reclamation remains unfinished.
- **Feature names are not promises**: `wayland`, `amd-isp4`, `swap`, and other cfg names occur in kernel source without root Cargo feature declarations. They are not supported `--features` switches.
- **Wayland remains unbuildable through its advertised gate**: Source files exist, but no root `wayland` feature is declared. `fault-inject` is a declared feature with host-engine evidence, not a completed guest-recovery contract.
- **Architecture names matter**: x86_64 and aarch64 slim UEFI checks passed; riscv64 slim checks failed. These are compile results, not runtime support tiers.

The detailed file index includes inactive, duplicate, empty, and test-only sources instead of hiding them. Use [module-map.md](module-map.md) for coverage and [milestones.md](milestones.md) for acceptance work.
