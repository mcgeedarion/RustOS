# Boot Image Size

_Last reviewed: 2026-07-01._

This document tracks how to measure RustOS boot image size. The repository has a
`release-boot` profile intended for lean smoke/normal boot images, but the size
baseline must be refreshed on the machine/toolchain that is used for release or
CI reporting.

## Current build intent

| Profile | Intent | Feature expectations |
|---|---|---|
| `dev` / `debug` | Developer build with symbols | Useful for GDB/debugging |
| `release` | Optimized developer/integration build | Full optimization, symbols not stripped by profile |
| `release-boot` | Lean boot image | `opt-level = "z"`, fat LTO, one codegen unit, debuginfo stripped |

The default feature path is `uefi_boot`, which enables `boot_minimal`.

## Measurement commands

```bash
# Build default x86_64 UEFI images
cargo xtask build --arch x86_64 --profile dev
cargo xtask build --arch x86_64 --profile release
cargo xtask build --arch x86_64 --profile release-boot

# Compare byte sizes
stat -c '%n %s' \
  target/x86_64-unknown-uefi/debug/rustos.efi \
  target/x86_64-unknown-uefi/release/rustos.efi \
  target/x86_64-unknown-uefi/release-boot/rustos.efi

# Optional section inspection when llvm tools are installed
llvm-size -A target/x86_64-unknown-uefi/release-boot/rustos.efi
```

## Current baseline

No fresh checked-in byte baseline is claimed in this repository state. Record a
new table here after running the commands above in a reproducible environment.

| Profile | Path | Total size (bytes) | Status |
|---|---|---:|---|
| dev | `target/x86_64-unknown-uefi/debug/rustos.efi` | _measure locally_ | pending refresh |
| release | `target/x86_64-unknown-uefi/release/rustos.efi` | _measure locally_ | pending refresh |
| release-boot | `target/x86_64-unknown-uefi/release-boot/rustos.efi` | _measure locally_ | pending refresh |

## Update rule

When changing boot code, linker scripts, features, or profiles in a way that may
materially change image size:

1. Rebuild the affected profiles.
2. Update the baseline table with exact byte counts and the host/toolchain if
   the numbers are meant to be compared over time.
3. Keep `release-boot` free of `kmtest`, `fault-inject`, `syscall-trace`,
   `debug`, `gdbstub`, and `trace` unless explicitly measuring those features.
