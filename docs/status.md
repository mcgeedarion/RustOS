# RustOS Subsystem Status

_Last reviewed: 2026-07-01._

This file is the authoritative high-level status registry. The repository is in
a **boot-first stabilization** state: default builds exercise the UEFI minimal
boot path, `userspace_boot` provides a deliberately thin handoff profile, and
the full kernel graph is available for integration work when both
`boot_minimal` and `userspace_boot` are disabled.

## Legend

| Status | Meaning |
|---|---|
| `real` | Implemented and used by a supported validation path |
| `partial` | Core path exists; important edge cases or integration work remain |
| `stub` | API/module surface exists but returns fixed values, no-op behavior, or `ENOSYS`-style placeholders |
| `experimental` | Substantial code exists but it is not part of the default smoke contract |
| `planned` | Tracked but not implemented in this repository yet |

## Build profiles

| Feature selection | Status | Notes |
|---|---|---|
| default `uefi_boot` | real | Enables `boot_minimal`; this is the default local/CI smoke path |
| `boot_minimal` | real | Firmware handoff, `BootInfo`, early console, boot markers, idle loop |
| `userspace_boot` | partial | Uses `src/userspace_shims.rs` for minimal fs/proc surfaces |
| full kernel graph | experimental | Active when neither `boot_minimal` nor `userspace_boot` is selected |
| `release-boot` profile/feature | partial | Lean image profile exists; size baselines still need to be refreshed |

## Boot & Init

| Module | Status | Feature gate | Notes |
|---|---|---|---|
| `src/boot_minimal.rs` | real | `boot_minimal` | Emits the minimal boot success path and parks the CPU |
| `src/kernel_main.rs` | partial | all profiles | Common architecture-independent entry dispatcher and boot markers |
| `src/init/` | partial | all profiles | `BootInfo`, initramfs metadata, loader/scheme scaffolding |
| `src/userspace_boot.rs` | partial | `userspace_boot` | Handoff marker path with thin shim services |
| `src/userspace_shims.rs` | stub | `userspace_boot` | Intentional temporary fs/proc surface for M2 work |

## Architecture

| Module | Status | Notes |
|---|---|---|
| `src/arch/x86_64/` | partial | Primary UEFI boot path; syscall/interrupt/paging code exists for full-kernel work |
| `src/arch/aarch64/` | partial | UEFI and bare-metal paths exist; kept aligned with shared boot contracts |

## Memory Management

| Module | Status | Notes |
|---|---|---|
| `src/mm/` physical memory | partial | Allocator and boot-memory code exist; integration remains full-kernel only |
| `src/mm/` virtual memory/mmap | partial | VMA, mmap, page-fault, and protection modules exist; edge-case correctness remains under development |
| `src/mm/` slab/heap | partial | Kernel allocation code exists; production hardening remains |
| COW/swap/mlock/pkeys/KASAN helpers | experimental | Code exists but is not part of the default smoke contract |
