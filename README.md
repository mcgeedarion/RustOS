# RustOS

RustOS is an experimental Rust operating-system kernel and workspace. The current repository state favors a **full-kernel default build** while retaining slim boot profiles for UEFI smoke testing and incremental userspace handoff work.

## Current state

- **Default Cargo feature set:** `full_kernel`, which expands to `uefi_boot` and `userspace_boot` in `Cargo.toml`.
- **Supported validation path:** use `cargo xtask ...` commands instead of raw Cargo so target triples, features, initrd embedding, ESP staging, and QEMU smoke markers stay consistent.
- **Primary architecture:** `x86_64` UEFI.
- **Secondary architecture:** `aarch64` UEFI/bare-metal bring-up.
- **Boot profiles:** `boot_minimal` remains the lean firmware-handoff and idle-loop path; `userspace_boot` remains a transitional handoff profile with `userspace_shims`.
- **Full kernel graph:** MM, VFS/filesystems, process, syscall, IPC, networking, security, driver, debug, and test modules are present for integration, but many subsystems remain partial or experimental unless documented otherwise in `docs/status.md`.

## Repository layout

| Path | Purpose |
| --- | --- |
| `src/` | Kernel source: architecture glue, boot paths, MM, FS, proc, syscall, IPC, net, drivers, security, debug, and tests. |
| `src/arch/x86_64/` | x86-64 UEFI boot, paging, interrupts, syscall, CPU, and platform support. |
| `src/arch/aarch64/` | AArch64 UEFI/bare-metal boot, paging, interrupts, CPU, SMP, and platform support. |
| `crates/` | Workspace support crates: VFS, filesystem, MM, synchronization, and scheme APIs. |
| `userspace/` | Early userspace programs: init, hello, smoke test, kmtest launcher, libc shim, and driver experiments. |
| `xtask/` | Canonical developer automation for build, check, image, run, smoke, docs, and local CI workflows. |
| `scripts/` | CI and helper scripts for boot logs, initramfs packing, userspace builds, validation, and stability runs. |
| `linker/` | Bare-metal ELF linker scripts. UEFI PE/COFF loader output uses target/link configuration instead. |
| `docs/` | Consolidated architecture, status, syscall, milestone, CI, boot, testing, and policy documentation. |

## Prerequisites

Install the tools needed for the workflow you plan to run:

- Rust nightly as pinned by `rust-toolchain.toml`.
- Rust targets used by the selected architecture, such as `x86_64-unknown-uefi` and `aarch64-unknown-uefi`.
- QEMU:
  - `qemu-system-x86_64` for x86-64.
  - `qemu-system-aarch64` plus AAVMF/QEMU EFI firmware for AArch64.
- Optional helpers:
  - `mtools` for FAT ESP image creation; `xtask` has a built-in fallback writer.
  - A musl toolchain for C userspace/initramfs experiments.

For x86-64 UEFI boot, `xtask` checks `OVMF_CODE`, common system OVMF paths, and a local `.ovmf/` cache. For AArch64 UEFI, set `QEMU_EFI` when firmware is not discoverable.

## Quick start

```sh
cargo xtask check --arch x86_64
cargo xtask smoke --arch x86_64
```

Useful commands:

| Command | Description |
| --- | --- |
| `cargo xtask build --arch x86_64` | Compile the kernel for an architecture. |
| `cargo xtask check --arch x86_64` | Type-check the canonical kernel target/profile. |
| `cargo xtask image --arch x86_64` | Build and stage a UEFI ESP disk image. |
| `cargo xtask run --arch x86_64` | Build, image, and boot under QEMU. |
| `cargo xtask smoke --arch x86_64` | Boot under QEMU and require a serial success marker. |
| `cargo xtask debug --arch x86_64` | Launch QEMU with a GDB server on `tcp::1234`. |
| `cargo xtask build-init --arch x86_64` | Build early userspace `/init` and pack `initramfs.cpio`. |
| `cargo xtask test` | Run host-side Rust tests. |
| `cargo xtask lint-modules` | Run module-hygiene checks. |
| `cargo xtask roadmap-check` | Validate required roadmap documentation coverage. |
| `cargo xtask ci-local` | Run the fast local gate. |

Use `cargo xtask help` for the full command reference.

## Build profiles and features

| Feature/profile | Purpose |
| --- | --- |
| `full_kernel` | Default feature; currently expands to `uefi_boot` and `userspace_boot`. |
| `uefi_boot` | Build as a UEFI loader/application and enable `boot_minimal`. |
| `boot_minimal` | Firmware handoff, `BootInfo`, early console, boot marker, and idle loop. |
| `userspace_boot` | Transitional userspace handoff with initramfs support and thin fs/proc shims. |
| `release-boot` | Size-oriented boot profile/feature for lean artifacts. |
| `kmtest` | Optional in-kernel test harness. |
| `fault-inject` | Compile-time fault-injection hooks for selected PMM, VMM, and syscall paths. |
| `debug`, `gdbstub`, `trace` | Debugging, GDB remote serial protocol, and trace-drain support. |
| `boot_debug` | Verbose boot logging gate; keep disabled for timing-sensitive smoke/perf runs. |
| `syscall-trace` | Optional per-syscall counters and hardware-cycle timing. |

Examples:

```sh
cargo xtask build --arch x86_64 --profile release-boot
cargo xtask check --arch x86_64 --features userspace_boot --initrd
cargo xtask smoke --arch aarch64
cargo xtask run --arch x86_64 --features boot_debug
```

## Userspace and initramfs

Build early userspace and pack an initramfs:

```sh
cargo xtask build-init --arch x86_64
```

Boot with the userspace handoff path and initramfs:

```sh
cargo xtask run --arch x86_64 --features userspace_boot --initrd
```

## Documentation map

- `docs/status.md` is the authoritative subsystem status registry.
- `docs/architecture.md` describes architecture contracts and code organization rules.
- `docs/milestones.md` tracks M1-M5 product milestones and serial sentinels.
- `docs/syscalls.md` records syscall maturity and EFAULT-safety expectations.
- `docs/ci.md`, `docs/getting-started.md`, and `docs/code_coverage.md` cover developer validation.
- Boot performance and size notes live in `docs/boot-perf.md`, `docs/boot-image-size.md`, and `docs/boot-optimization-checklist.md`.

Update these documents with code changes that alter subsystem maturity, supported workflows, or validation contracts.

## License

RustOS is licensed under the MIT License. See `Cargo.toml` for package metadata.
