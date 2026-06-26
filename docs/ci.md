# CI Serial Sentinel Contract

RustOS emits a stable, machine-readable sentinel string on the serial
console just before the kernel enters its idle loop.  CI jobs and
automated test scripts **must** use these sentinels (not timing-based
heuristics) to determine whether a boot succeeded.

---

## Sentinel Strings

| Sentinel | When emitted |
|---|---|
| `BOOT_MINIMAL_OK` | Kernel reached idle loop (boot_minimal feature) |
| `FULL_OS_USERSPACE_OK` | Userspace `/init` ran to completion |
| `entering cpu_idle` | General idle loop reached (all targets) |

Any of these strings appearing in the serial output is sufficient to
consider the boot successful.

---

## Implementation Notes

### Sentinel placement

The sentinel is emitted from `kernel_main()` in `src/kernel_main.rs`
via `early_putchar` / the platform serial driver, immediately before
entering the idle loop.  It must not be gated behind any feature flag
that could be omitted in CI builds.

### QEMU timeout handling

QEMU is launched with `timeout 60` for x86\_64 (45 s for aarch64). A timeout exit code of `124` is treated as success at the
QEMU level — the kernel is still running in the idle loop — but the
subsequent `grep` step will fail if the sentinel was never emitted,
correctly failing the job.

### Grep pattern

The grep step uses:

```bash
grep -qE 'BOOT_MINIMAL_OK|FULL_OS_USERSPACE_OK|entering cpu_idle' "$SERIAL_LOG"
```

The pattern is intentionally broad so that any of the three sentinels
(from different feature configurations) triggers a pass.

---

## Adding a New Target

1. Emit one of the sentinel strings on the serial console before the
   idle loop.
2. Add the target to the CI matrix in `.github/workflows/kernel-test.yml`
   (or `build.yml` / `qemu-smoke.yml` as appropriate).
3. Ensure the QEMU launch command writes serial output to `$SERIAL_LOG`.
4. Confirm the grep step uses the shared pattern above.
