# Getting Started — RustOS Developer On-Ramp

_Last reviewed: 2026-07-01._

The canonical local validation command is:

```bash
cargo xtask smoke --arch x86_64
```

It builds the default x86_64 UEFI/minimal-boot image, assembles a FAT ESP disk
image, boots it in QEMU, captures serial output, and checks for a supported boot
sentinel.

## Prerequisites

### Rust toolchain

The repository pins nightly Rust in `rust-toolchain.toml`. `rustup` installs the
specified toolchain/components automatically when you run Cargo in the repo.

### QEMU and firmware

| Need | Debian/Ubuntu | Fedora/RHEL | Arch | macOS/Homebrew |
|---|---|---|---|---|
| x86_64 QEMU | `sudo apt install qemu-system-x86` | `sudo dnf install qemu-system-x86` | `sudo pacman -S qemu-system-x86` | `brew install qemu` |
| x86_64 OVMF | `sudo apt install ovmf` | `sudo dnf install edk2-ovmf` | `sudo pacman -S edk2-ovmf` | bundled with QEMU |
| AArch64 QEMU/firmware | `sudo apt install qemu-system-arm qemu-efi-aarch64` | distro equivalent | distro equivalent | install QEMU and set `QEMU_EFI` if needed |

`xtask` resolves x86_64 OVMF from `OVMF_CODE`, known system paths, a cached
`.ovmf/OVMF_CODE.fd`, or an automatic Fedora RPM download when helper tools are
available. For AArch64, set `QEMU_EFI` if the known firmware paths are absent.

## Golden path

```bash
git clone https://github.com/mcgeedarion/RustOS.git
cd RustOS
cargo xtask smoke --arch x86_64
```

A successful smoke run writes serial output to `target/smoke-x86_64.log` and
passes if it contains one of:

```text
BOOT_MINIMAL_OK
FULL_OS_USERSPACE_OK
entering cpu_idle
```

## Common commands

```bash
# Compile only, using the same target/feature handling as the boot path
cargo xtask check --arch x86_64

# Build the EFI artifact only
cargo xtask build --arch x86_64

# Build kernel + stage ESP + assemble boot-x86_64.img
cargo xtask image --arch x86_64

# Interactive QEMU run instead of marker-checked smoke
cargo xtask run --arch x86_64

# QEMU run with GDB server on tcp::1234
cargo xtask debug --arch x86_64

# Build userspace init and pack initramfs.cpio
cargo xtask build-init --arch x86_64

# Validate roadmap/status/syscall/fault documentation contracts
cargo xtask roadmap-check

# Fast local aggregate gate
cargo xtask ci-local
```

## Feature/profile notes

| Selection | Purpose |
|---|---|
| default `uefi_boot` | Enables `boot_minimal`; current default smoke path |
| `--features boot_minimal` | Explicit minimal boot path |
| `--features userspace_boot --initrd` | Thin userspace-handoff experiments |
| `--profile release-boot` | Lean image profile; size baselines are documented separately |

## Disk image layout

```text
boot-x86_64.img
└── EFI/
    └── BOOT/
        └── BOOTX64.EFI
STARTUP.NSH
```

The image is assembled by the built-in FAT16 writer; `mtools` is not required
for the default path.

## Troubleshooting

### `qemu-system-x86_64: not found`

Install QEMU or set `QEMU` to a specific binary path.

### OVMF firmware not found

Install an OVMF package or set `OVMF_CODE=/path/to/OVMF_CODE.fd`.

### AArch64 firmware not found

Install AAVMF/QEMU EFI firmware or set `QEMU_EFI=/path/to/QEMU_EFI.fd`.

### Build fails because `core`/`alloc` is unavailable

Ensure `rustup` is installed and run any Cargo command from the repository root
so the pinned nightly and `rust-src` component can be installed.

## Where to look next

- Current subsystem maturity: `docs/status.md`
- Milestone definitions: `docs/milestones.md`
- Architecture policy: `docs/architecture.md`
- Syscall matrix: `docs/syscalls.md`
