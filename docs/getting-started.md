# Getting started

Use `cargo xtask` from the repository root for the supported build and run commands. At the audited revision `08a14f6`, slim-profile checks work independently of the full graph, while the default full x86_64 check fails in existing source.

## Tooling

The repository pins `nightly-2026-06-07` in `rust-toolchain.toml`; the required components are `rust-src`, `llvm-tools-preview`, `rustfmt`, and `clippy`. The xtask alias explicitly builds an `x86_64-unknown-linux-gnu` host executable, so the documented on-ramp is a Linux x86_64 development environment rather than a verified native Windows or macOS workflow.

```bash
rustup show
rustc --version
cargo --version
cargo xtask help
```

Kernel builds use nightly `build-std`; custom JSON targets also require `-Z json-target-spec`, which xtask supplies. QEMU runs additionally need the matching `qemu-system-*` executable and firmware; xtask packs newc archives in Rust, while the alternative shell packaging scripts use host tools such as `cpio`.

For an Ubuntu-style development environment, the repository's workflows install packages from this group. Package availability is host-dependent; these commands are prerequisites, not test results.

```bash
sudo apt-get update
sudo apt-get install -y build-essential clang llvm lld ripgrep cpio \
  mtools musl-tools qemu-system-x86 qemu-system-arm qemu-system-misc \
  ovmf qemu-efi-aarch64
```

## First compile check

Start with an explicitly scoped minimal build rather than expecting the default full graph to compile. This checks Rust compilation, not firmware launch or a running userspace process.

```bash
cargo xtask check --arch x86_64 --features boot_minimal
cargo xtask build --arch x86_64 --features boot_minimal
```

For other target families, use the corresponding architecture selection. UEFI is the default boot mode, and AArch64 additionally accepts `--boot baremetal`.

```bash
cargo xtask check --arch aarch64 --features boot_minimal
cargo xtask check --arch riscv64 --features boot_minimal
```

The AArch64 slim checks passed during this audit. The RISC-V slim checks fail at the existing unresolved `SIZE` constant in `arch/riscv64/mem_layout.rs`; the command is shown for reproduction, not as a passing path.

## Minimal emulator smoke

Use this command only as a first-stage boot test. `BOOT_MINIMAL_OK` verifies that selected path, not MM/VFS/process or userspace behavior.

```bash
cargo xtask smoke --arch x86_64 --features boot_minimal
```

For interactive launch, use `cargo xtask run --arch x86_64 --features boot_minimal`. The x86_64 path resolves OVMF from `OVMF_CODE`, installed firmware paths, or its download cache; an explicit `OVMF_CODE` avoids depending on remote mirrors.

## Inspect an init image

The diagnostic profile uses the real CPIO reader and ELF metadata checks but intentionally does not execute `/init`. Expect `USERSPACE_BOOT_UNSUPPORTED`; xtask rejects this profile as a successful smoke contract.

```bash
cargo xtask build-init --arch x86_64
cargo xtask run --arch x86_64 --features userspace_boot --initrd
```

## Work on the full kernel

The commands below select the full graph and its real userspace-execution contract. They are reproduction commands for ongoing integration work, not a promise of a passing build at this revision.

```bash
cargo xtask check --arch x86_64
cargo xtask smoke --arch x86_64
```

Do not add `boot_minimal` to a failing full command and report the result as a fix. Read [current blockers](source-audit.md), [build options](build-reference.md), and [the smoke protocol](userspace-smoke.md) before changing a gate.

## Fast host checks

These tests avoid the full kernel's compilation blockers and exercise specific production units or parsers. Their scope and observed results are recorded in [coverage](code_coverage.md) and [the audit](source-audit.md).

```bash
cargo xtask test
bash scripts/ci/test-region-heap.sh
bash scripts/ci/test-pmm-init.sh
bash scripts/ci/test-kstack-ownership.sh
bash scripts/ci/test-exit-lifecycle.sh
python3 scripts/ci/test-boot-marks.py
python3 -m unittest discover -s scripts/ci/tests -p 'test_*.py'
```

Raw `cargo test` inherits the default UEFI target and is not the host-test entry point. For independently testable workspace crates, pass an explicit host target and standard-library build configuration as shown in [the build reference](build-reference.md).
