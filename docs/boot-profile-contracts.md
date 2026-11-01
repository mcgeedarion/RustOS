# Boot-profile contracts

RustOS has three subsystem configurations selected in `src/lib.rs`. This contract describes their actual boundaries at `08a14f6` and prevents a slim-profile result from being mistaken for full-kernel validation.

## Selection

| Slim flags | Selected graph | Execution contract |
| --- | --- | --- |
| Neither | Full subsystem graph | Real MM/VFS/process/scheduler integration and userspace execution |
| `boot_minimal` only | Minimal common boot | Early console, BootInfo diagnostics, markers, idle |
| `userspace_boot` only | Init-image diagnostics | CPIO lookup and ELF metadata validation only |
| Both | Compile-time error | No supported combined profile |

`full_kernel` is a default feature that enables `uefi_boot`; it is not the sole switch controlling full-module inclusion. `uefi_boot` selects firmware-related behavior without proving any subsystem or userspace contract.

## Minimal boot

`boot_minimal::enter` initializes the early console, reports architecture and available BootInfo fields, emits timing markers, prints `RustOS: BOOT_MINIMAL_OK`, and idles. It does not initialize the full scheduler, filesystems, device graph, or a user process.

Its `BOOT_MMU_ON` timing marker records an observable post-handoff point rather than a newly executed MMU transition inside the common function. `BOOT_INITRAMFS_LOADED` is emitted for the minimal marker protocol even if no archive is parsed; neither is proof of filesystem readiness.

```bash
cargo xtask check --arch x86_64 --features boot_minimal
cargo xtask smoke --arch x86_64 --features boot_minimal
```

The global allocator for this configuration is a 64 KiB bump allocator. Its deallocation behavior must not be generalized to the full kernel's reclaiming heap.

## Init-image diagnostics

`userspace_boot::enter` requires a registered initramfs, retrieves `/init`, and calls the diagnostic ELF validator from `userspace_shims`. A valid image leads to `USERSPACE_BOOT_UNSUPPORTED` and an idle loop, not PID creation, scheduler activation, or user-mode entry.

```bash
cargo xtask build-init --arch x86_64
cargo xtask run --arch x86_64 --features userspace_boot --initrd
```

This configuration uses a 1 MiB bump heap and shim FS/process surfaces. `cargo xtask smoke --features userspace_boot` is intentionally rejected as a success path.

## Full kernel

The full graph declares architecture HALs, memory, processes, VFS, drivers, networking, IPC, security, timers, and optional test/debug components. Architecture-specific `kernel_main` functions orchestrate subsystem initialization and attempt the real init-process path.

```bash
cargo xtask check --arch x86_64
cargo xtask smoke --arch x86_64
```

The full smoke command requires init to execute and reap distinct smoke and benchmark children and complete the strict serial protocol. At this revision the audited x86_64 full compilation fails before that runtime contract can be demonstrated; implemented exec/fork code alone is insufficient.

## Release-boot semantics

`cargo xtask ... --profile release-boot` selects both the size-oriented Cargo profile and `release-boot,boot_minimal` without default features. It is therefore a minimal boot image, not a lean version of a fully initialized production OS.

The empty `release-boot` feature by itself does not disable debug features, and Cargo features are additive. Use xtask's profile logic and inspect the resulting command rather than inferring exclusions from a feature's name.
