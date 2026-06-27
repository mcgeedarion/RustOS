# RustOS Architecture Support Policy

## Primary architecture: x86_64

- **Boot method**: UEFI (PE/COFF image via `x86_64-unknown-uefi` target)
- **CI requirement**: All pushes and PRs **must** pass the smoke test
  (`RUSTOS_BOOT_OK` / `BOOT_MINIMAL_OK` sentinel in QEMU serial output).
- **Feature requirement**: All Cargo features must compile on x86_64.
- **QEMU command**: `cargo xtask smoke --arch x86_64` (local validation)

## Secondary architecture: aarch64

- **Boot methods**: UEFI (via `aarch64-unknown-uefi`) and bare-metal ELF
  (via `targets/aarch64-kernel.json`)
- **CI requirement**: `boot_minimal` must boot and emit `BOOT_MINIMAL_OK`.
  Failures are **blocking** for M1; best-effort for M2+.
- **Feature requirement**: `boot_minimal` and `userspace_boot` features must
  compile. `full-kernel`, `net`, `wayland-compositor` are compile-tested only.
- **QEMU command**: `cargo xtask smoke --arch aarch64`

## Planned: RISC-V (riscv64)

- No runtime CI yet.  
- A `targets/riscv64-kernel.json` target spec will be added when SBI boot is
  plumbed.  
- PRs touching `src/arch/riscv64/` are welcome; they need only pass
  `cargo xtask build --arch riscv64` (compile-only).

---

## Code organisation rules

1. **ISA-specific code** lives exclusively under `src/arch/<arch>/`.
2. **Common kernel code** calls arch-specific functionality only through the
   `Arch` trait defined in `src/arch/mod.rs`.
3. A PR that only modifies `src/arch/x86_64/` must not break aarch64
   compilation — enforced by `cargo xtask build --arch aarch64` in CI.
4. No `#[cfg(target_arch = "...")]` outside `src/arch/` except for thin shims
   in `src/main.rs` / `src/lib.rs` that select the arch module.

## Adding a new architecture

1. Add a target JSON spec under `targets/`.
2. Implement the `Arch` trait under `src/arch/<arch>/`.
3. Add a compile-only CI step to `ci.yml`.
4. Document boot status and CI tier in this file.
