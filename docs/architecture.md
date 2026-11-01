# Kernel architecture

RustOS declares a hybrid-kernel design in `src/kernel/architecture.rs`: privileged core services stay in the kernel, while schemes and IPC provide a boundary for userspace servers. This is the intended architecture at `08a14f6`, not a claim that all proposed servers or the full boot path currently work.

## Repository layers

```text
src/main.rs
  architecture firmware entry
    BootInfo + selected kernel_main
      boot_minimal        early console + idle
      userspace_boot      CPIO/ELF inspection + explicit unsupported marker
      full graph          MM, scheduler, VFS, drivers, IPC, syscalls

userspace programs
  architecture syscall ABI
    syscall::dispatch_arch -> canonical routers
      process / memory / filesystem / IPC / time
        kernel schemes or IPC-backed userspace service
```

The selection rule is in `src/lib.rs`, not the presence of a `full_kernel` cfg around every subsystem. With neither `boot_minimal` nor `userspace_boot`, the full graph is selected even if default Cargo features were disabled.

## Kernel roots

| Root | Responsibility | Documentation |
| --- | --- | --- |
| `arch`, `init`, `boot_perf`, `console`, `kernel`, `error` | Boot contracts, architecture dispatch, diagnostics, common error surface | [Platforms](firmware-and-platforms.md), [debugging](debugging.md) |
| `allocator`, `mm` | Global heap, physical pages, virtual mappings, stacks | [Memory](memory-management.md) |
| `proc`, `syscall`, `exec` | Process lifetime, scheduling, ABI boundary, ELF validation | [Processes](processes-and-scheduling.md), [syscalls](syscalls.md) |
| `fs`, `block` | VFS, schemes, FDs, mounts, block devices | [Storage](filesystems-and-storage.md) |
| `ipc`, `io_uring`, `sync` | Messaging, asynchronous requests, wait/wake primitives | [IPC](ipc-and-async-io.md), [synchronization](synchronization.md) |
| `device`, `drivers`, `display`, `input`, `tty` | Enumeration, hardware access, graphics/input/terminal surfaces | [Drivers](drivers-and-devices.md) |
| `net`, `security` | Protocol stack and policy enforcement | [Networking](networking.md), [security](security.md) |
| `firmware`, `irq`, `smp`, `time` | Platform tables, interrupts, CPU bring-up, clock/timer services | [Platforms](firmware-and-platforms.md) |
| `kcore` / compatibility alias `core` | Collections, CPU-local helpers, errors, panic helpers | [Synchronization](synchronization.md), [ADR 0002](adr/0002-unified-error-handling.md) |
| `debug`, `kmtest`, `fault_inject` | Optional observability and testing surfaces | [Debugging](debugging.md), [coverage](code_coverage.md) |

Some roots compile only in full builds, and `kmtest` and `fault_inject` add their own feature gates. Slim `mm` is an inline compatibility surface exposing the common bump allocator, not the full MM tree.

## Workspace boundaries

The root Cargo workspace contains the kernel, `xtask`, and seven library crates. The `userspace` and `fuzz` trees have separate workspace manifests and must be invoked separately.

- **`scheme-api`**: Shared no_std protocol types, errors, messages, flags, and raw syscall interface. This is the interface crate used across kernel/userspace boundaries.
- **`vfs-core`**: Generic filesystem traits and an independent runtime registry. It is not the kernel's `fs::scheme_table`.
- **`fs-ext4`, `fs-fat32`, `fs-tmpfs`**: Extraction-stage filesystem adapters. The first two contain `kernel_impl` conditional branches and fallback behavior; tmpfs is still a skeleton.
- **`mm-core`**: Address and memory-management abstractions with NUMA/page metadata surfaces. The operational PMM remains under `src/mm`.
- **`sync-primitives`**: Independently testable queued spinlock, RCU, and reader/writer primitives. The kernel also retains its own `src/sync` implementation.

The kernel depends on these libraries and re-exports `mm_core_exports`, `sync_primitives_exports`, and `vfs_core_exports`. Dependency or re-export presence is not proof that every legacy implementation has been replaced.

## Important integration boundaries

The existing roadmap guard uses the labels “Primary architecture: x86_64” and “Secondary architecture: aarch64.” These are prioritization labels rather than runtime certification; RISC-V is a third source target with separately documented compile blockers.

- **Syscall translation**: Architecture numbers are translated before canonical routing. Passing an AArch64 or RISC-V number directly to an x86-oriented router is not a portable call.
- **Descriptor identity**: A process-visible FD, a scheme backing FD, a scheme-local `FileId`, and a raw VFS handle are different namespaces. Use the conversion and duplication helpers rather than treating them as interchangeable integers.
- **Process identity and lifetime**: PCB snapshots support table inspection; identity-checked reaping removes a zombie only if it is still the same object. Scheduler raw pointers require longer-lived task storage than the PID index.
- **Memory ownership**: Kernel stacks retain physical backing after retirement. Reclamation requires a stronger scheduler/TLB quiescence contract than “the process exited.”
- **Service isolation**: IPC endpoints, driver handles, and `IpcProxyScheme` are present, but the userspace NIC prototype is not a packaged, proven server.

## Current consistency limits

The x86_64 full check fails at this revision, so this guide documents source relationships rather than a working complete system. Duplicate socket modules, missing declarations, empty integration files, and undeclared feature gates are tracked in [the source audit](source-audit.md).

Legacy compatibility surfaces are retained deliberately in the documentation. [ADR 0001](adr/0001-workspace-crate-architecture.md) and [architecture improvements](architecture-improvements.md) describe what must be reconciled before the workspace split can be considered complete.

## Code organisation rules

Keep hardware register/entry code under the relevant architecture or driver, syscall argument validation at the boundary, and object ownership in the responsible subsystem. Explicitly record every `mod`, `#[path]`, `include!`, and feature change; a file added to the tree is not automatically compiled.
