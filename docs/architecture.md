# RustOS Architecture Support Policy

_Last reviewed: 2026-10-13._

RustOS currently has two maintained architecture paths. The repository is in a
boot-first stabilization state: x86_64 UEFI is the primary path and AArch64 is
kept buildable for UEFI and bare-metal bring-up. The default selects the full
subsystem graph; minimal and diagnostic profiles are explicit opt-ins. A full
subsystem build does not yet prove that userspace execution works.

## Primary architecture: x86_64

- **Boot method:** UEFI PE/COFF image using the `x86_64-unknown-uefi` target.
- **Default feature path:** `full_kernel`, with UEFI firmware entry independent
  of the selected subsystem graph.
- **Minimal smoke command:** `cargo xtask smoke --arch x86_64 --features boot_minimal`.
- **Full smoke command:** `cargo xtask smoke --arch x86_64`.
- **Success contract:** minimal smoke requires `BOOT_MINIMAL_OK`; full smoke
  requires the exact-PID userspace execution protocol, benchmark JSON, and timing
  parser success from the same fresh QEMU log. The full path is blocked until
  real ELF mapping, user-stack setup, and process scheduling are implemented.
- **QEMU contract:** Q35 machine, OVMF firmware, 256 MiB RAM, serial captured to
  `target/smoke-x86_64.log`.

## Secondary architecture: aarch64

- **Boot methods:** UEFI PE/COFF via `aarch64-unknown-uefi`, and bare-metal ELF
  via `targets/aarch64-kernel.json` plus `linker/aarch64.ld`.
- **Minimal smoke command:** `cargo xtask smoke --arch aarch64 --features boot_minimal`.
- **Full smoke command:** `cargo xtask smoke --arch aarch64` (execution currently blocked).
- **Firmware requirement:** install AAVMF/QEMU EFI firmware or set `QEMU_EFI`.
- **Status:** kept aligned with the shared `BootInfo`, early console, boot
  marker, and idle-loop contracts. Full userspace execution remains blocked until
  the architecture-aware process/address-space backend is integrated.

---

## Build-profile/module policy

The crate root intentionally selects one of three module graphs:

| Feature selection | Active path | Current purpose |
|---|---|---|
| `boot_minimal` only | first-stage boot only | firmware handoff, `BootInfo`, early console, boot marker, idle loop |
| `userspace_boot` only | initramfs/ELF diagnostics | validates image metadata, then reports execution unavailable; never counts as a full userspace execution pass |
| both slim features | rejected | the kernel crate emits a compile-time error to prevent ambiguous module selection |
| `full_kernel` / Cargo defaults | full kernel graph with UEFI entry | compiles mm/fs/proc/net/security/drivers/syscalls; end-to-end userspace execution remains blocked |

`full_kernel` is the default and enables `uefi_boot` without selecting `boot_minimal`.
Use `--features boot_minimal` for the separate minimal smoke contract and
`--features userspace_boot` for diagnostic-only experiments.

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
