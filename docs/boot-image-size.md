# Boot image size

This document tracks the size of RustOS boot images for different profiles and architectures.

The goal of the `release-boot` profile is to provide a lean, production-like
boot image for normal boots and CI smoke tests:

- No kernel-mode test harness (`kmtest` feature disabled)
- No fault injection framework (`fault-inject` feature disabled)
- No syscall profiling / tracing hooks (`syscall-trace` feature disabled)
- No GDB RSP stub or trace ring buffer (`debug`, `gdbstub`, `trace` features off)
- Debug symbols and DWARF sections removed from the boot image
- Codegen tuned for size: `opt-level = "z"`, `lto = "fat"`, `codegen-units = 1`

## Methodology

Unless otherwise noted, measurements are taken on:

- Host: x86_64 Linux (Ubuntu 24.04)
- Toolchain: Rust nightly pinned in `rust-toolchain.toml`
- Target: `x86_64-unknown-uefi` (UEFI loader)

The following tools are used:

- `du -b <file>` — total image size in bytes
- `llvm-size -A <file>` — per-section sizes for the kernel ELF

Commands:

```bash
# Debug/dev UEFI image (Phase 1 golden path)
cargo xtask build --arch x86_64 --profile dev --boot uefi

# Lean release-boot UEFI image (Phase 5)
cargo xtask build --arch x86_64 --profile release-boot --boot uefi

# Inspect sizes
llvm-size -A target/x86_64-unknown-uefi/debug/rustos.efi
llvm-size -A target/x86_64-unknown-uefi/release-boot/rustos.efi

du -b target/x86_64-unknown-uefi/debug/rustos.efi
du -b target/x86_64-unknown-uefi/release-boot/rustos.efi
```

## Current measurements (x86_64 UEFI)

> NOTE: The exact numbers will vary slightly between nightly toolchains and
> small code changes. This table is updated when `release-boot` is introduced
> and should be refreshed when making large architectural changes.

| Profile       | Path                                             | Total size (bytes) | Notes                              |
|---------------|--------------------------------------------------|--------------------:|------------------------------------|
| dev           | `target/x86_64-unknown-uefi/debug/rustos.efi`   | _TBD_              | Full debug, tests, verbose logging |
| release       | `target/x86_64-unknown-uefi/release/rustos.efi` | _TBD_              | Optimised + debug features         |
| release-boot  | `target/x86_64-unknown-uefi/release-boot/rustos.efi` | _TBD_          | Lean CI/normal boot image          |

## Section breakdown example

```text
$ llvm-size -A target/x86_64-unknown-uefi/debug/rustos.efi
   section                    size         addr
   .text                    XXXXXXX   0x00000000
   .rodata                  XXXXXXX   0x00000000
   .data                    XXXXXXX   0x00000000
   .bss                     XXXXXXX   0x00000000
   .debug_info              XXXXXXX   0x00000000
   .debug_line              XXXXXXX   0x00000000
   ...

$ llvm-size -A target/x86_64-unknown-uefi/release-boot/rustos.efi
   section                    size         addr
   .text                    YYYYYYY   0x00000000
   .rodata                  YYYYYYY   0x00000000
   .data                    YYYYYYY   0x00000000
   .bss                     YYYYYYY   0x00000000
   # No .debug_* sections in release-boot
```

The `release-boot` image should be measurably smaller than the dev/debug
image, primarily due to:

- Removal of `.debug_*` sections and symbol tables
- Exclusion of kmtest, fault-inject, syscall-trace, and debug/tracing code
- More aggressive cross-crate optimisation and code layout for size
