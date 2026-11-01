# ADR 0001: Workspace crate architecture

Status: retained design direction, partially implemented at `08a14f6`. This updated record distinguishes the intended dependency split from the current source graph.

## Context and decision

Separate portable interfaces and implementations from architecture-dependent kernel code so they can be built and tested independently. Keep firmware entry, scheduler integration, interrupt handling, and privileged memory/device operations in the kernel until a concrete safe boundary exists.

The root workspace currently includes the kernel, xtask, `scheme-api`, `vfs-core`, `fs-ext4`, `fs-fat32`, `fs-tmpfs`, `mm-core`, and `sync-primitives`. The kernel depends on the seven library crates and keeps compatibility re-exports.

## Current implementation

- **Shared API**: `scheme-api` is a real shared protocol crate.
- **VFS interfaces**: `vfs-core` defines traits and an independent registry, while kernel VFS/schemes remain active separate systems.
- **Filesystem extraction**: ext4/FAT32 adapters retain `kernel_impl` conditionals and fallbacks; workspace tmpfs remains a skeleton.
- **Memory/synchronization**: `mm-core` and `sync-primitives` coexist with operational kernel MM and synchronization code. Re-exporting them is not complete migration.
- **Userspace**: Independent init/smoke/bench/raw-syscall workspace roots build separately. The aggregate userspace workspace metadata is inconsistent.

## Consequences and remaining work

Portable host tests can run without compiling the full graph, which is useful while kernel integration fails. However, parallel implementations, duplicate registries, and error types create divergence risks.

Do not introduce a larger crate split merely by moving files and preserving unresolved imports. Define ownership, API/error boundaries, feature propagation, tests, and removal of superseded paths before declaring migration complete.

The [architecture guide](../architecture.md), [source audit](../source-audit.md), and [workspace backlog](../architecture-improvements.md) describe the current state. This ADR does not certify a completed microkernel or fully userspace-driver architecture.
