# RustOS Product Milestones

This table is the canonical definition of "done" for each development stage.
Every milestone maps to a Cargo feature flag and a CI-verifiable serial sentinel.
Do **not** close a milestone unless the sentinel appears in both `x86_64` and
`aarch64` QEMU runs (or the arch is explicitly marked best-effort for that
milestone).

| Milestone | Goal | Feature flag | CI sentinel | Arch required |
|-----------|------|--------------|-------------|---------------|
| **M1** | `boot_minimal` boots on x86_64 and aarch64 | `boot_minimal` | `BOOT_MINIMAL_OK` | x86_64 (required), aarch64 (required) |
| **M2** | `userspace_boot` launches `/init` | `userspace_boot` | `FULL_OS_USERSPACE_OK` | x86_64 (required), aarch64 (best-effort) |
| **M3** | Syscalls needed by init are real | `userspace_boot` | init completes `open/fork/exec/wait/exit` without ENOSYS | x86_64 (required) |
| **M4** | VFS + initramfs usable | `full-kernel` | `/init`, `/dev/null`, `/proc/version` accessible | x86_64 (required) |
| **M5** | Network / display / input demos | `full-kernel` + optional features | Shell or compositor runs as a userspace process | x86_64 (required) |

## How sentinels work

Sentinels are emitted from a single canonical location — the architecture
handoff in `kernel_main()` or `boot_minimal::entry()` — over the serial port.
CI greps for the exact string; no timing heuristics are used.

```
BOOT_MINIMAL_OK          # emitted at end of boot_minimal path
FULL_OS_USERSPACE_OK     # emitted after /init returns successfully
```

If a sentinel is missing after the 60-second QEMU timeout, the CI job **fails**.

## Current status

See [docs/status.md](status.md) for the per-subsystem implementation state
that feeds into each milestone.
