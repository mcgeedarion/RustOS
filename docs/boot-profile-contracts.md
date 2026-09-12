# Boot Profile Contracts

The slim profiles must build independently of the full MM, VFS, and scheduler.
Compilation is a separate contract from boot success or userspace execution.

## Supported selections

| Selection | Compiled services | Runtime contract |
|---|---|---|
| `--no-default-features --features boot_minimal` | Firmware handoff, early console, bump allocator | Minimal boot may emit `BOOT_MINIMAL_OK`; this does not prove userspace execution |
| `--no-default-features --features userspace_boot` | Firmware handoff, raw CPIO lookup, ELF diagnostics | Missing archive/init and invalid ELF produce diagnostics; a valid image returns execution unavailable |
| Cargo defaults | `full_kernel` currently enables `uefi_boot` and `userspace_boot` | Same transitional diagnostic contract, despite the historical feature name |
| Neither slim flag | Full subsystem graph | Separate experimental integration work; not made production-ready by these fixes |

HAL implementations, their imports/reexports, the `Arch` alias, and HAL-backed
address helpers share the same full-graph gate. The trait definitions remain
available to all profiles; no fake paging or process implementation is supplied.

Firmware initramfs registration and the embedded-archive fallback remain enabled
whenever `userspace_boot` is active, including Cargo defaults that also select
`boot_minimal`. Pure minimal boot still omits that diagnostic-only work.

## Reproducible validation

The `Focused host regressions` workflow builds all three supported UEFI
selections in debug and release on x86_64 and AArch64. These are actual
`cargo build` invocations so a missing firmware entrypoint cannot hide behind
a successful `cargo check`.

Run the diagnostic-path unit tests without privileged I/O:

```sh
cargo test --locked -p rustos-kernel --lib --no-default-features \
  --features userspace_boot --target x86_64-unknown-linux-gnu \
  -Z build-std=std,test
```

The tests cover real CPIO lookup and ELF validation, including truncation,
architecture mismatch, segment/header overflow, a dynamic-linker requirement,
and explicit rejection of process execution even for a valid static image.
They do not execute the kernel scheduler or syscalls.

For runtime validation, test both missing-archive and archive-present boots.
With a bounded, architecture-matching synthetic ELF in `/init`, the diagnostic
profile must reach `USERSPACE_BOOT_UNSUPPORTED` and print the unavailable-backend
error, with no `BOOT_INIT_EXEC`, PID-created, or userspace-success marker.

## Userspace promotion gate

Do not emit `BOOT_INIT_EXEC`, a PID-created claim, or `FULL_OS_USERSPACE_OK`
from ELF validation or a successful build. Promotion requires a real address
space, mapped segments, stack and CPU context, scheduler integration, and a
sentinel emitted by the running userspace process itself.
