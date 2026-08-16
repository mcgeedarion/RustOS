# RustOS

RustOS is a Rust-based operating system kernel focused on boot-first stabilization, explicit architecture contracts, and incremental userspace bring-up. The current default path builds a UEFI boot image that exercises the minimal firmware handoff, shared boot information, early serial console, boot markers, and idle loop while the larger kernel subsystem graph continues to mature.

## Current status

RustOS is not yet a general-purpose operating system. The repository is organized around milestone-driven validation:

- **Default UEFI boot path:** `uefi_boot`, which enables `boot_minimal`.
- **Primary architecture:** `x86_64` UEFI.
- **Secondary architecture:** `aarch64` UEFI and bare-metal bring-up.
- **Userspace handoff:** `userspace_boot` is available as a thin, transitional profile for initramfs and `/init` work.
- **Full kernel graph:** memory management, filesystems, process management, syscalls, networking, security, drivers, and debugging code exists for integration work, but much of it is still partial or experimental.

For detailed status, see:

- [`docs/status.md`](docs/status.md) for subsystem maturity.
- [`docs/architecture.md`](docs/architecture.md) for supported architecture contracts.
- [`docs/syscalls.md`](docs/syscalls.md) for syscall implementation status.
- [`docs/milestones.md`](docs/milestones.md) for milestone tracking.

## Repository layout

| Path | Purpose |
| --- | --- |
| `src/` | Kernel source, including architecture glue, boot paths, MM, FS, proc, syscall, driver, security, and debug modules. |
| `src/arch/x86_64/` | x86-64 UEFI boot, paging, interrupts, syscall, CPU, and platform support. |
| `src/arch/aarch64/` | AArch64 UEFI/bare-metal boot, paging, interrupts, CPU, SMP, and platform support. |
| `userspace/` | Early userspace programs, musl/sysroot support, init, shell, smoke binaries, and driver experiments. |
| `xtask/` | Canonical build, run, image, smoke, documentation, and local CI automation. |
| `tools/` and `scripts/` | Helper scripts for userspace builds, initramfs packing, CI boot runs, and regression checks. |
| `docs/` | Architecture, syscall, milestone, fault-injection, boot-size, and boot-performance documentation. |
| `crates/scheme-api/` | Shared scheme API crate used by the kernel workspace. |

## Prerequisites

At minimum, install:

- A Rust toolchain with nightly support for `-Z build-std` workflows.
- `rustup` targets used by the selected architecture, such as `x86_64-unknown-uefi` and/or `aarch64-unknown-uefi`.
- QEMU for booting images locally:
  - `qemu-system-x86_64` for x86-64.
  - `qemu-system-aarch64` plus AAVMF/QEMU EFI firmware for AArch64.
- Optional but useful tools:
  - `mtools` for FAT ESP image creation; `xtask` can fall back to a built-in FAT writer when unavailable.
  - A musl toolchain for C userspace helpers and initramfs experiments.

For x86-64 UEFI boot, `xtask` looks for common system OVMF firmware paths and can cache a fallback firmware image under `.ovmf/` when needed. You can also set `OVMF_CODE` explicitly. For AArch64 UEFI, set `QEMU_EFI` if firmware is not discoverable by your system setup.

## Quick start

Build and run the default x86-64 UEFI path under QEMU:

```sh
cargo xtask run --arch x86_64
```

This command builds the kernel, stages an EFI binary under `target/esp/x86_64/EFI/BOOT/BOOTX64.EFI`, assembles `boot-x86_64.img`, and launches QEMU with serial output attached to the terminal.

Run a boot smoke test and assert a serial success marker:

```sh
cargo xtask smoke --arch x86_64
```

Type-check the default kernel boot path:

```sh
cargo xtask check --arch x86_64
```

Run the fast local CI gate:

```sh
cargo xtask ci-local
```

## Common `xtask` commands

| Command | Description |
| --- | --- |
| `cargo xtask build --arch x86_64` | Compile the kernel for the selected architecture. |
| `cargo xtask check --arch x86_64` | Type-check the kernel with the canonical target and feature handling. |
| `cargo xtask image --arch x86_64` | Build the kernel, stage the ESP, and assemble a FAT boot image. |
| `cargo xtask run --arch x86_64` | Build, image, and launch the OS under QEMU. |
| `cargo xtask smoke --arch x86_64` | Boot under QEMU, capture serial output, and require a boot success marker. |
| `cargo xtask debug --arch x86_64` | Launch QEMU with a GDB server on `tcp::1234`. |
| `cargo xtask build-init --arch x86_64` | Build early userspace `/init` and pack `initramfs.cpio`. |
| `cargo xtask test` | Run host-side Rust unit tests. |
| `cargo xtask lint-modules` | Run module-hygiene checks. |
| `cargo xtask roadmap-check` | Validate status, syscall, milestone, architecture, and fault-injection docs. |
| `cargo xtask ci-local` | Run the fast local gate: check, host tests, module lint, stub guard, and roadmap checks. |

Use `cargo xtask help` to print the full command reference.

## Build profiles and features

RustOS intentionally separates boot validation from the full kernel module graph:

| Feature/profile | Purpose |
| --- | --- |
| `uefi_boot` | Default feature; enables the minimal UEFI boot path. |
| `boot_minimal` | Firmware handoff, `BootInfo`, early console, boot marker, and idle loop. |
| `userspace_boot` | Transitional userspace-handoff path using thin fs/proc shims. |
| `release-boot` | Lean release boot image profile/feature for size-oriented boot artifacts. |
| `kmtest` | Optional in-kernel test harness. |
| `fault-inject` | Compile-time fault-injection hooks for selected PMM, VMM, and syscall paths. |
| `debug`, `gdbstub`, `trace` | Debugging, GDB remote serial protocol, and trace-drain support. |
| `boot_debug` | Gates verbose `log::debug!`/`log::trace!` output during boot; off by default for performance. |
| `syscall-trace` | Per-syscall invocation counters and hardware-cycle timing via `/proc/syscall_stats`. |

Examples:

```sh
cargo xtask build --arch x86_64 --profile release-boot
cargo xtask check --arch x86_64 --features userspace_boot --initrd
cargo xtask smoke --arch aarch64
cargo xtask run --arch x86_64 --features boot_debug  # Enable verbose boot logging
```

## Userspace and initramfs

Build the early userspace init program and pack a CPIO initramfs:

```sh
cargo xtask build-init --arch x86_64
```

The generated archive is written to `initramfs.cpio`. To boot with an initramfs-backed userspace handoff, use the `userspace_boot` feature and `--initrd`:

```sh
cargo xtask run --arch x86_64 --features userspace_boot --initrd
```

Additional userspace helper scripts are available in `tools/`, including `tools/build_userspace.sh` and `tools/build_initramfs.sh`.

## Validation

Recommended checks before sending changes:

```sh
cargo xtask check --arch x86_64
cargo xtask test
cargo xtask roadmap-check
cargo xtask smoke --arch x86_64
```

### Boot performance validation

RustOS includes boot timing instrumentation that emits parseable markers on the serial console. See [`docs/boot-perf.md`](docs/boot-perf.md) for the marker format and [`docs/boot-optimization-checklist.md`](docs/boot-optimization-checklist.md) for optimization guidance.

Run boot performance parsing on a QEMU serial log:

```sh
bash scripts/ci/parse-boot-marks.sh target/smoke-x86_64.log
```

The CI regression gate (`scripts/ci/boot-regression.sh`) compares current boot times against baselines and fails if any phase exceeds its baseline by more than 20%.

Some checks require QEMU, UEFI firmware, musl tooling, or architecture-specific firmware to be installed. Prefer `xtask` commands over raw `cargo` invocations so target triples, feature selection, boot-image staging, and serial-marker checks remain consistent with the repository contract.

## Documentation and contribution notes

- Update `docs/status.md` when promoting or demoting a subsystem.
- Update `docs/syscalls.md` when syscall semantics change.
- Keep architecture-specific code under `src/arch/<arch>/` and share state through common boot abstractions where possible.
- Boot success should be determined by serial sentinels such as `BOOT_MINIMAL_OK`, `FULL_OS_USERSPACE_OK`, or `entering cpu_idle`, not by fixed sleeps.
- Avoid enabling fault injection, syscall tracing, GDB stubs, or test harnesses in lean release boot images unless intentionally validating those paths.
- Gate verbose boot logging behind the `boot_debug` feature; keep it disabled for performance measurements and CI smoke runs.
- When optimizing boot performance, work through the checklist in `docs/boot-optimization-checklist.md` and update baselines in `docs/boot-perf-baseline.txt` only via intentional `[perf-update]` commits.

## License

RustOS is licensed under the MIT License. See the package metadata in [`Cargo.toml`](Cargo.toml) for repository and package information.
