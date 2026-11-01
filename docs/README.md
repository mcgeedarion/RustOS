# RustOS documentation

This documentation describes the source at `08a14f6` on `main`, audited on 2026-11-01. It distinguishes code that exists, code selected by a build profile, executed host checks, and guest behavior that still needs evidence.

The default Cargo configuration selects the full subsystem graph, not the minimal kernel. That graph currently fails the audited x86_64 compilation check; the explicit slim profiles have a different scope and must not be used as evidence that full userspace execution works.

## Start here

- **Current state**: [Subsystem status](status.md), [source audit and validation results](source-audit.md), and [milestones](milestones.md).
- **Build and run**: [Getting started](getting-started.md), [build reference](build-reference.md), and [boot-profile contracts](boot-profile-contracts.md).
- **Architecture**: [Kernel architecture](architecture.md), [firmware and platforms](firmware-and-platforms.md), and the [complete source-file index](module-map.md).
- **Contributor workflow**: [Documentation maintenance](maintenance.md), [CI](ci.md), [coverage boundaries](code_coverage.md), and [rustdoc generation](rustdoc_generation.md).

## Subsystem guides

| Area | Guide |
| --- | --- |
| Physical memory, heaps, mappings, kernel stacks | [Memory management](memory-management.md) |
| Tasks, scheduler, fork, exec, signals, exit, wait | [Processes and scheduling](processes-and-scheduling.md) |
| ABI translation, syscall routers, errno, user copies | [System calls](syscalls.md) |
| VFS, schemes, descriptors, filesystem backends, block I/O | [Filesystems and storage](filesystems-and-storage.md) |
| Endpoints, pipes, SysV/POSIX IPC, io_uring | [IPC and asynchronous I/O](ipc-and-async-io.md) |
| Ethernet, IP, TCP/UDP, DNS/DHCP, sockets | [Networking](networking.md) |
| PCI, storage/NIC drivers, graphics, input, terminals, media | [Drivers and devices](drivers-and-devices.md) |
| Locks, wait queues, RCU surfaces, per-CPU state | [Synchronization](synchronization.md) |
| Permissions, capabilities, namespaces, seccomp, hardening | [Security](security.md) |
| Freestanding programs and initramfs packaging | [Userspace](userspace.md) |
| Panic output, tracing, GDB, diagnostic limits | [Debugging](debugging.md) |

## Validation and performance

- **Runtime acceptance**: [Acceptance criteria](acceptance-criteria.md), [userspace smoke protocol](userspace-smoke.md), and [production requirements](production_requirements.md).
- **Fault testing**: [Fault-injection engine](fault_inject.md) and [guest fault-recovery contract](fault-recovery.md).
- **Performance evidence**: [Boot timing](boot-perf.md), [image-size accounting](boot-image-size.md), [syscall benchmark](syscall-perf.md), [compiler settings](compiler-optimizations.md), and [boot optimization checklist](boot-optimization-checklist.md).
- **Concurrency review**: [Atomic ordering guidelines](ATOMIC_ORDERING_GUIDELINES.md).

## Design decisions and remaining work

- **Architecture backlog**: [Architecture improvements](architecture-improvements.md) and [M3 to M5 roadmap](M3_M5_ROADMAP.md).
- **Workspace split**: [ADR 0001](adr/0001-workspace-crate-architecture.md).
- **Error conventions**: [ADR 0002](adr/0002-unified-error-handling.md).
- **Filesystem registration**: [ADR 0003](adr/0003-vfs-registration.md).

Source comments, historical roadmaps, feature names, and workflow names are not proof of completed functionality. For disagreements, inspect the module declarations, implementation, and recorded validation in [the audit](source-audit.md).
