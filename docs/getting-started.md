# Getting Started — RustOS Developer On-Ramp

This document is the canonical local validation on-ramp. One documented command,
`cargo xtask smoke`, builds the kernel, assembles the UEFI ESP image, boots it in
QEMU, captures serial output, and validates the boot sentinel.

## Prerequisites

### Rust toolchain

The repository pins its toolchain via `rust-toolchain.toml`. `rustup` will
pick it up automatically.

```bash
# If you don't have rustup yet:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# The nightly toolchain + required components are installed automatically
# the first time you run any cargo command inside the repo.
```

### QEMU

| Platform | Command |
|---|---|
| Debian / Ubuntu | `sudo apt install qemu-system-x86` |
| Fedora / RHEL | `sudo dnf install qemu-system-x86` |
| Arch | `sudo pacman -S qemu-system-x86` |
| macOS (Homebrew) | `brew install qemu` |

### OVMF firmware (x86\_64 UEFI)

`cargo xtask run` resolves OVMF in this order — **you don't need to do
anything** on a typical Linux workstation:

1. `OVMF_CODE` environment variable (highest priority)
2. Well-known system paths (`/usr/share/OVMF/OVMF_CODE.fd`, etc.)
3. `.ovmf/OVMF_CODE.fd` in the workspace root (previously auto-downloaded)
4. **Auto-download** from Fedora mirrors into `.ovmf/OVMF_CODE.fd`
   (requires `curl` or `wget`, plus `rpm2cpio` and `cpio`)

If you prefer a manual install:

```bash
# Debian/Ubuntu
sudo apt install ovmf

# Fedora/RHEL
sudo dnf install edk2-ovmf

# Arch
sudo pacman -S edk2-ovmf

# macOS — QEMU bundles edk2-x86_64-code.fd; xtask finds it automatically.
```

## The Golden Path

```bash
git clone https://github.com/mcgeedarion/RustOS.git
cd RustOS
cargo xtask smoke --arch x86_64
```

That's it. This is the single documented local validation command. What happens under the hood:

```
[xtask] ==> Step 1/3: building x86_64 uefi kernel
[xtask]   cargo build --target x86_64-unknown-uefi --release --features uefi_boot ...
[xtask]   installed EFI: target/esp/x86_64/EFI/BOOT/BOOTX64.EFI
[xtask]   image ready: boot-x86_64.img
[xtask] ==> Step 2/3: resolving OVMF firmware
[xtask]   OVMF: found system firmware: /usr/share/OVMF/OVMF_CODE.fd
[xtask] ==> Step 3/3: launching QEMU (serial → target/smoke-x86_64.log)
[xtask]   image:    boot-x86_64.img
[xtask]   firmware: /usr/share/OVMF/OVMF_CODE.fd

BDSv2.0 ...
rustos: kernel_main reached
```

Serial output is captured and scanned for `BOOT_MINIMAL_OK`, `FULL_OS_USERSPACE_OK`, or `entering cpu_idle`. Use `cargo xtask run --arch x86_64` only for interactive debugging.

## Variants

```bash
# Debug build (no --release, symbols present for GDB)
cargo xtask run --arch x86_64 --debug

# Minimal boot smoke-test feature set
cargo xtask run --arch x86_64 --features boot_minimal

# Use a specific OVMF binary
OVMF_CODE=/path/to/OVMF_CODE.fd cargo xtask run --arch x86_64

# Use a specific QEMU binary
QEMU=/opt/qemu-9/bin/qemu-system-x86_64 cargo xtask run --arch x86_64
```

## Build without booting

```bash
# Build kernel EFI only
cargo xtask build --arch x86_64

# Build kernel + FAT disk image
cargo xtask image --arch x86_64
```

## Subcommand Reference

```
cargo xtask smoke         Build + boot + assert serial sentinel ← local validation
cargo xtask build         Compile the kernel only
cargo xtask image         Build a FAT ESP disk image
cargo xtask build-init    Build userspace + pack initramfs.cpio
cargo xtask run           Interactive QEMU run for debugging
cargo xtask help          Show all options
```

## CI Smoke Test

The `smoke` subcommand is the documented local validation command and the command used by CI. It builds the image, boots it, and
verifies that a known-good log line appears within a timeout:

```bash
cargo xtask smoke
```

The marker regex is configurable via `SMOKE_MARKER_RE` for CI environments.


## Local CI Gate

Prefer `xtask` over raw `cargo check` for kernel work. The kernel uses custom
target specifications and build-std flags, so `cargo xtask check` supplies the
nightly `-Z` options consistently.

```bash
# Fast pre-push check (kernel check, host tests, module/doc guards)
cargo xtask ci-local

# Validate the full roadmap/documentation contract
cargo xtask roadmap-check

# Type-check a specific milestone
cargo xtask check --arch x86_64 --features boot_minimal

# Run host-side Rust unit tests
cargo xtask test

# List host-side unit tests (extra args are forwarded to cargo test)
cargo xtask test -- --list
```

Use `cargo xtask test` instead of raw `cargo test`: this repository's default
Cargo target is the lean x86_64 UEFI image, while Rust's standard unit-test
runtime is a host-side `std` runtime.

Likewise, invoke automation with `cargo xtask ...` rather than raw
`cargo run -p xtask -- ...`. The repo-level Cargo config enables freestanding
`build-std` defaults for kernel builds, and the `xtask` alias supplies the
matching host-side `std` build flags for the automation binary.

Subsystem maturity and the current improvement roadmap live in
[`docs/status.md`](status.md).

## Disk Image Layout

```
boot-x86_64.img          (FAT16 ESP, 4 MiB)
└── EFI/
    └── BOOT/
        └── BOOTX64.EFI  (compiled from src/ targeting x86_64-unknown-uefi)
STARTUP.NSH              (UEFI shell fallback launcher)
```

The image is assembled by the built-in FAT16 writer (no mtools required)
or by mtools if available on PATH.

## QEMU Flags Explained

| Flag | Purpose |
|---|---|
| `-machine q35` | Modern PCIe chipset — required for UEFI |
| `-cpu qemu64` | Generic x86-64 CPU, maximally compatible |
| `-m 256M` | 256 MiB RAM |
| `-drive if=pflash,readonly=on` | OVMF firmware in pflash slot 0 |
| `-drive format=raw,if=virtio` | Boot disk via VirtIO (fast, no emulated IDE) |
| `-serial stdio` | Serial port → your terminal (kernel `println!` lands here) |
| `-display none` | No graphical window |
| `-no-reboot -no-shutdown` | VM stays alive after kernel halts (useful for debugging) |

## Troubleshooting

### `qemu-system-x86_64: not found`
Install QEMU (see Prerequisites above). You can also point `QEMU` at a
non-default binary:
```bash
QEMU=/usr/bin/qemu-system-x86_64 cargo xtask run --arch x86_64
```

### `OVMF firmware not found`
Run `sudo apt install ovmf` (or equivalent) **or** let xtask auto-download
it by ensuring `curl`/`wget` are on PATH. The downloaded file is cached at
`.ovmf/OVMF_CODE.fd` and reused on subsequent runs.

### Build fails with `error[E0463]: can't find crate for 'core'`
The nightly toolchain or `rust-src` component is missing:
```bash
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
```

### Kernel boots but serial output is blank
Confirm the kernel is compiled with `--features uefi_boot` (the default).
The serial driver is gated behind that feature. If using `--features
boot_minimal` check that `boot_minimal` wires up the early UART console.

### QEMU window appears (no serial on stdout)
Make sure you're using `cargo xtask run`, not invoking QEMU manually
without `-serial stdio -display none`.
