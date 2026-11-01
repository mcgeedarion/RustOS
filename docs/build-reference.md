# Build reference

This reference follows `Cargo.toml`, `.cargo/config.toml`, `rust-toolchain.toml`, `build.rs`, and `xtask/src/main.rs` at `08a14f6`. Source comments that still call plain `cargo build` a lean image are superseded by the actual default feature selection.

## Targets and profiles

| Architecture | Accepted xtask boot mode | Kernel target | EFI filename |
| --- | --- | --- | --- |
| `x86_64` | `uefi` | `x86_64-unknown-uefi` | `BOOTX64.EFI` |
| `aarch64` | `uefi` | `aarch64-unknown-uefi` | `BOOTAA64.EFI` |
| `aarch64` | `baremetal` / `bare-metal` | `targets/aarch64-kernel.json` | Not an EFI artifact |
| `riscv64` | `uefi` | `targets/riscv64-uefi-loader.json` | `BOOTRISCV64.EFI` |

Other JSON target files exist in `targets/`, but their existence does not add accepted xtask architecture/boot combinations. The CLI rejects x86_64 and RISC-V `--boot baremetal`.

| Option | Actual selection |
| --- | --- |
| No profile option | `dev`, output under `debug` |
| `--debug` | `dev` |
| `--profile dev` / `debug` | `dev` |
| `--profile release` | Root `[profile.release]` |
| `--profile release-boot` | Root size-oriented profile, plus xtask-selected `release-boot,boot_minimal` features |

Cargo profiles and Cargo features are separate controls. Running raw Cargo with `--profile release-boot` does not automatically reproduce xtask's minimal feature selection.

## Root features

| Feature | Dependency or effect |
| --- | --- |
| `default` | Enables `full_kernel` |
| `full_kernel` | Enables `uefi_boot`; full tree is selected by absence of both slim flags |
| `uefi_boot` | Firmware/entry configuration; not a synonym for minimal boot |
| `boot_minimal` | Common early-console/BootInfo/idle path; mutually exclusive with `userspace_boot` |
| `userspace_boot` | Initramfs/ELF diagnostics with lightweight FS/process shims |
| `input_events` | Input-event integrations and evdev-related paths |
| `amdgpu` | Optional AMD GEM backend source |
| `cgroups` | Security cgroup module declaration |
| `gdbstub` | Selects the currently incomplete in-kernel GDB module |
| `debug` | Enables `gdbstub` and `trace` |
| `debugstub` | Compatibility alias for `gdbstub` |
| `trace` | Trace/ftrace code; function instrumentation requires compiler flags separately |
| `kmtest` | In-kernel test registry and test syscalls in full builds |
| `fault-inject` | Atomic fault-point engine in full builds; not a working guest fault gate |
| `syscall-trace` | Syscall counts and cumulative hardware ticks |
| `boot_debug` | Explicitly gated diagnostic paths; not a global override of every logging call |
| `release-boot` | Empty marker feature; xtask applies the relevant profile and minimal graph |

Kernel cfg expressions also mention `debug_kernel`, `device_tree`, `fault_injection`, `kasan`, `smp`, `uefi`, `wayland`, `amd-isp4`, `vfs_symlink`, `pcie`, and `swap`, which are not declared root features. Do not document commands enabling these as if Cargo accepted them; `provide_start` is a separate, valid feature of `userspace/libc-shim`, not a root feature.

## CLI commands

The canonical command list is emitted by `cargo xtask help`. Build-family options include `--arch`, `--boot`, `--profile`, `--features <csv>`, and `--initrd`; command-specific argument handling is implemented directly in xtask rather than by a shared CLI schema.

```bash
cargo xtask help
cargo xtask check --arch x86_64 --features boot_minimal
cargo xtask build --arch x86_64 --profile release --features boot_minimal
cargo xtask image --arch x86_64 --features boot_minimal
cargo xtask run --arch x86_64 --features boot_minimal
cargo xtask smoke --arch x86_64 --features boot_minimal
cargo xtask build-init --arch aarch64
cargo xtask test
cargo xtask lint-modules
cargo xtask roadmap-check
```

`qemu` is an alias for the run path; inspect `help` for the debug launch and port options. Unknown options are rejected, and flags from a raw QEMU or Cargo command are not automatically accepted by xtask.

## Artifact locations

- **Kernel**: `target/<target-name>/<debug|release|release-boot>/rustos` or `rustos.efi`, depending on target.
- **ESP staging**: `target/esp/<arch>/EFI/BOOT/BOOT*.EFI`.
- **Disk image**: `boot-<arch>.img`, assembled by xtask's partitioned FAT-image builder.
- **Initramfs**: Root `initramfs.cpio`, staged by xtask from `userspace/{init,smoke,bench}/target/<target>/release/` binaries.
- **Smoke evidence**: `target/smoke-<arch>.log` and `target/smoke-<arch>-metadata.json`.
- **OVMF cache**: `.ovmf/OVMF_CODE.fd`, used only when an explicit or system firmware image is unavailable.

Builds with an initrd pass `RUSTOS_INITRAMFS` and its fingerprint into the kernel build. `build.rs` writes generated `initramfs_embed.rs` in Cargo's `OUT_DIR`; it is not a checked-in source module.

## Host testing

The host alias selects Linux x86_64 regardless of the kernel's target. Portable crate tests should use the same explicit target and override the freestanding standard-library selection.

```bash
cargo test -p scheme-api --target x86_64-unknown-linux-gnu \
  -Z build-std=std,panic_abort
cargo test -p sync-primitives --target x86_64-unknown-linux-gnu \
  -Z build-std=std,panic_abort
```

The focused shell harnesses compile selected production source directly and supply narrowly scoped dependencies. A zero-test crate invocation is a successful build/test invocation, not behavioral coverage.

## C runtime and linker boundaries

`build.rs` compiles five freestanding C runtime files for non-UEFI targets when `boot_minimal` is absent. It prefers Clang for AArch64/RISC-V cross C builds when available, while honoring explicit `CC`/target-specific overrides.

UEFI builds do not use the bare-metal ELF linker layout. Cross-target source checks, final linking, image construction, firmware launch, and userspace execution are separate verification steps; consult [the audit](source-audit.md) for which were actually executed.
