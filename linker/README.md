# Linker Scripts

_Last reviewed: 2026-07-01._

This directory contains linker scripts for kernel images. These scripts describe bare-metal ELF layout, not UEFI application layout.

## Artifact Split

Keep the boot architecture consistent by keeping the artifact formats separate:

- UEFI loader: a PE/COFF executable entered at `efi_main`. The AArch64 loader target uses `targets/aarch64-uefi-loader.json`, `lld-link`, `/subsystem:efi_application`, and `/entry:efi_main`.
- Kernel image: an ELF image entered at the architecture start symbol. The AArch64 kernel target uses `targets/aarch64-kernel.json`, `ld.lld`, and `linker/aarch64.ld`.
- Handoff: the loader gathers firmware state, exits boot services, builds the shared `BootInfo`, and jumps into the kernel entry path.

This gives each architecture the same conceptual flow without forcing UEFI-specific PE/COFF requirements into the kernel ELF linker scripts.

## Why `aarch64.ld` Stays ELF

`linker/aarch64.ld` declares `OUTPUT_FORMAT("elf64-littleaarch64")`, `OUTPUT_ARCH(aarch64)`, and `ENTRY(_start)`. It also controls kernel sections, the higher-half virtual base, BSS bounds, constructor arrays, the kernel test registry, and the early boot stack.

Those are ELF kernel concerns. UEFI firmware loads PE/COFF applications with PE subsystem metadata, so UEFI loaders should use a separate COFF-oriented target/link configuration instead of trying to make this GNU linker script produce a UEFI application.

## Cross-Architecture Pattern

For each supported architecture, prefer this pairing:

- `targets/<arch>-uefi-loader.json` builds the PE/COFF UEFI stage, when that architecture supports UEFI boot.
- `targets/<arch>-kernel.json` builds the ELF kernel stage and points at `linker/<arch>.ld`.
- The architecture boot glue normalizes platform-specific entry state into the shared kernel ABI, especially `BootInfo` and the common kernel entry.

Architectures can still keep non-UEFI bare-metal entry paths, such as AArch64 `_start`, for firmware like U-Boot or custom bring-up. Those paths should remain kernel ELF paths and continue to use the scripts in this directory.

## Build Profiles

The `release-boot` profile (configured in the root `Cargo.toml`) applies aggressive size optimizations:

- `opt-level = "z"` for size-focused optimization
- `lto = "fat"` for whole-program optimization
- `codegen-units = 1` to maximize inlining opportunities
- `strip = "debuginfo"` to remove debug symbols from the boot image

Use this profile for production boot images:

```sh
cargo xtask build --arch x86_64 --profile release-boot
```
