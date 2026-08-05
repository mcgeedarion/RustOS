# CI and Local Validation Contract

_Last reviewed: 2026-07-01._

RustOS uses `cargo xtask` as the canonical automation layer. Raw `cargo` commands
are useful for debugging, but CI and pre-push validation should go through
`xtask` so target specs, features, firmware staging, FAT image creation, and
serial-log checks stay consistent.

## Primary commands

| Command | Purpose |
|---|---|
| `cargo xtask check --arch x86_64` | Type-check the default x86_64 UEFI/minimal path |
| `cargo xtask smoke --arch x86_64` | Build, image, boot QEMU, and assert a serial marker |
| `cargo xtask smoke --arch aarch64` | Same contract for AArch64 UEFI, subject to local firmware availability |
| `cargo xtask build-init --arch x86_64` | Build userspace init and pack `initramfs.cpio` |
| `cargo xtask roadmap-check` | Validate roadmap/status/syscall/fault docs contain required topics |
| `bash scripts/ci/check-stubs.sh` | Guard documented stub classifications |
| `cargo xtask ci-local` | Fast aggregate local gate: check, host tests, module lint, stub guard, docs guard |

## Serial success markers

`cargo xtask smoke` captures QEMU serial output to `target/smoke-<arch>.log` and
passes when any of these strings is present:

```text
BOOT_MINIMAL_OK|FULL_OS_USERSPACE_OK|entering cpu_idle
```

Timeout status from QEMU is tolerated because a successfully booted kernel may
be parked in the idle loop; the serial marker check is the actual pass/fail
signal.

## QEMU defaults

| Architecture | Machine | Firmware | Timeout | Log path |
|---|---|---|---:|---|
| x86_64 | `q35` | `OVMF_CODE` or discovered OVMF | 60s | `target/smoke-x86_64.log` |
| aarch64 | `virt` | `QEMU_EFI` or discovered AAVMF/QEMU EFI | 45s | `target/smoke-aarch64.log` |

## Adding CI coverage

1. Prefer adding an `xtask` subcommand or extending an existing one over adding
   a one-off shell command.
2. Capture serial output to a file and assert markers, not sleeps.
3. Update `docs/status.md`, `docs/milestones.md`, and this file when the gate
   changes what is considered supported.
