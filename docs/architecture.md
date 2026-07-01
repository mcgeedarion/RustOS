# RustOS Architecture Support Policy

_Last reviewed: 2026-07-01._

RustOS currently has two maintained architecture paths. The repository is in a
boot-first stabilization state: x86_64 UEFI is the primary path, AArch64 is kept
buildable for UEFI and bare-metal bring-up, and the full kernel module graph is
compiled only when neither `boot_minimal` nor `userspace_boot` is selected.

## Primary architecture: x86_64

- **Boot method:** UEFI PE/COFF image using the `x86_64-unknown-uefi` target.
- **Default feature path:** `uefi_boot`, which aliases into `boot_minimal`.
- **Local smoke command:** `cargo xtask smoke --arch x86_64`.
- **Success contract:** serial output must contain `BOOT_MINIMAL_OK`,
  `FULL_OS_USERSPACE_OK`, or `entering cpu_idle`.
- **QEMU contract:** Q35 machine, OVMF firmware, 256 MiB RAM, serial captured to
  `target/smoke-x86_64.log`.

## Secondary architecture: aarch64

- **Boot methods:** UEFI PE/COFF via `aarch64-unknown-uefi`, and bare-metal ELF
  via `targets/aarch64-kernel.json` plus `linker/aarch64.ld`.
- **Local smoke command:** `cargo xtask smoke --arch aarch64`.
- **Firmware requirement:** install AAVMF/QEMU EFI firmware or set `QEMU_EFI`.
- **Status:** kept aligned with the shared `BootInfo`, early console, boot
  marker, and idle-loop contracts; full subsystem parity with x86_64 is not yet
  required.

## Not currently implemented: RISC-V

There is no RISC-V target spec, architecture module, or CI/runtime contract in
this repository today. Add those only when an SBI boot path and compile target
are introduced together.

---

## Build-profile/module policy

The crate root intentionally selects one of three module graphs:

| Feature selection | Active path | Current purpose |
|---|---|---|
| `boot_minimal` without `userspace_boot` | first-stage boot only | firmware handoff, `BootInfo`, early console, boot marker, idle loop |
| `userspace_boot` | boot plus thin shims | validates initramfs/userspace handoff without the unstable full module graph |
| neither `boot_minimal` nor `userspace_boot` | full kernel graph | compiles mm/fs/proc/net/security/drivers/syscalls for integration work |

`uefi_boot` is the default feature and enables `boot_minimal`. Use explicit
features when validating a non-default path.

## Code organisation rules

1. ISA-specific code lives under `src/arch/<arch>/`.
2. Shared boot and kernel code should use architecture traits/functions from
   `src/arch/` rather than open-coding target-specific branches.
3. Cross-architecture handoff state must flow through the shared `BootInfo`
   path in `src/init/`.
4. Avoid adding new `#[cfg(target_arch = "...")]` outside architecture glue,
   crate-root module selection, and minimal entry-point shims.

## Adding a new architecture

1. Add a target spec under `targets/` or use a supported Rust builtin target.
2. Add `src/arch/<arch>/` with entry, paging, serial, interrupt, and syscall
   glue as appropriate.
3. Teach `xtask` the new `--arch` value and valid boot modes.
4. Document smoke/build expectations here and in `docs/milestones.md`.
