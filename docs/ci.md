# CI Serial Sentinel Contract

RustOS emits a stable, machine-readable sentinel string on the serial
console just before the kernel enters its idle loop.  CI jobs and
automated test scripts **must** use this sentinel to detect a successful
end-to-end boot instead of parsing verbose log output.

---

## Status badge

Add this to your `README.md` to show live CI status for the x86\_64 UEFI
boot smoke job:

```markdown
[![Boot Smoke – x86_64 UEFI](https://github.com/mcgeedarion/RustOS/actions/workflows/boot-smoke.yml/badge.svg?branch=main&job=x86_64-uefi)](https://github.com/mcgeedarion/RustOS/actions/workflows/boot-smoke.yml)
```

Rendered:

[![Boot Smoke – x86_64 UEFI](https://github.com/mcgeedarion/RustOS/actions/workflows/boot-smoke.yml/badge.svg?branch=main)](https://github.com/mcgeedarion/RustOS/actions/workflows/boot-smoke.yml)

---

## Sentinel strings

| Configuration | Sentinel | Source location |
|---|---|---|
| Default (full kernel, x86\_64) | `RUSTOS_BOOT_OK v1` | `src/arch/x86_64/kernel_main.rs` |
| `boot_minimal` (all arches) | `RustOS: BOOT_MINIMAL_OK` | `src/boot_minimal.rs` |

---

## Position in the boot sequence

For the **default** configuration the sentinel is printed at step 14
of the x86\_64 boot sequence, immediately after:

1. All hardware subsystems are initialised (serial, GDT/IDT, PMM, ACPI, PCIe, APIC).
2. Storage is probed and the root filesystem is mounted.
3. The init process (PID 1) has been spawned (or the fallback idle path
   has been chosen).

The sentinel is therefore a guarantee that **all critical boot phases
completed without a panic or triple-fault**.

For `boot_minimal` builds the sentinel is printed after the common
minimal path completes, just before the CPU parks in a halt loop.

---

## Stability guarantee

- The sentinel string **will not change** without a deliberate version bump
  in both the Rust source and this document.
- The current version token is **`v1`**. When the format changes the token
  becomes `v2`, etc., allowing CI to support both during a transition window.
- The sentinel is printed **exactly once** per boot on the boot CPU.
  It is not printed on AP bring-up, nor repeated by the scheduler.

---

## CI usage

### Bash / GitHub Actions

```bash
# Pass if the sentinel appears anywhere in the serial log.
grep -q 'RUSTOS_BOOT_OK' qemu-x86_64-boot.log
```

The `boot-smoke` workflow (`.github/workflows/boot-smoke.yml`) contains an
"Assert boot sentinel: RUSTOS\_BOOT\_OK" step that performs exactly this
check as the **canonical CI gate** for the x86\_64 UEFI path.

### Matching with version awareness

If your script needs to distinguish versions:

```bash
grep -qE 'RUSTOS_BOOT_OK v[0-9]+' qemu-x86_64-boot.log
```

### QEMU timeout handling

QEMU is launched with `timeout 60` for x86\_64 (45 s for aarch64, 60 s
for riscv64). A timeout exit code of `124` is treated as success at the
QEMU level — the kernel is still running in the idle loop — but the
subsequent `grep` step will fail if the sentinel was never emitted,
correctly failing the job.

---

## Caching strategy

The `boot-smoke` workflow uses three `actions/cache` layers:

| Layer | Path | Cache key |
|---|---|---|
| Registry index + crate sources | `~/.cargo/registry` | `cargo-registry-<OS>-<Cargo.lock hash>` |
| Git-sourced deps | `~/.cargo/git/db` | same as registry |
| Compiled artifacts | `target/` | `cargo-target-<arch>-<OS>-<toolchain hash>-<Cargo.lock hash>` |

The `target/` cache key includes both `rust-toolchain.toml` and
`Cargo.lock`, so a nightly date bump or dependency update automatically
invalidates the artifact cache while the registry layer remains warm.

---

## Version history

| Version | Sentinel text | Introduced |
|---|---|---|
| v1 | `RUSTOS_BOOT_OK v1` | 2026-06-23 |
