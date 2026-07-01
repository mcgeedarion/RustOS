# Boot Performance Instrumentation

RustOS ships a lightweight, zero-allocation boot timing subsystem that emits
machine-parseable milestone markers on the serial console during every boot.
CI parses these markers and logs inter-milestone deltas so regressions are
detectable across commits.

---

## Wire Format

Every marker emitted by `boot_mark!` follows this exact grammar:

```
BOOT_MARK label=<LABEL> ticks=<DECIMAL_U64>
```

| Field   | Type           | Description                                         |
|---------|----------------|-----------------------------------------------------|
| `BOOT_MARK` | literal    | Fixed prefix; used as grep anchor                   |
| `label` | `[A-Z0-9_]+`  | Milestone identifier (see table below)              |
| `ticks` | `u64` decimal | Raw counter value from the hardware timer register  |

Fields are separated by a single ASCII space.  The line is terminated by `\n`
(appended by `serial_println!`).  No other whitespace or quoting is permitted.

### Example output

```
BOOT_MARK label=BOOT_ENTRY ticks=12345678
BOOT_MARK label=BOOT_MMU_ON ticks=12350000
BOOT_MARK label=BOOT_INITRAMFS_LOADED ticks=12360000
BOOT_MARK label=BOOT_INIT_EXEC ticks=12380000
```

---

## Defined Milestones

| Label                    | Instrumentation point                                                        |
|--------------------------|------------------------------------------------------------------------------|
| `BOOT_ENTRY`             | First Rust instruction in `kernel_main` / `boot_minimal::enter` / `userspace_boot::enter` — immediately after arch stub hands off to common code |
| `BOOT_MMU_ON`            | First observable point in the common boot path after virtual memory is live  |
| `BOOT_INITRAMFS_LOADED`  | After `fs::initramfs::mount_initramfs()` returns (VFS tree populated)        |
| `BOOT_INIT_EXEC`         | Immediately before the boot CPU parks itself; PID 1 is enqueued and will receive its first tick on the next scheduler dispatch |

> **boot_minimal note** — `boot_minimal` builds do not parse an initramfs.
> `BOOT_INITRAMFS_LOADED` is still emitted (immediately after `BOOT_MMU_ON`)
> to satisfy the four-milestone contract used by the CI parser.

---

## Hardware Counter Sources

| Architecture | Instruction   | Counter                           | Typical frequency       |
|--------------|---------------|-----------------------------------|-------------------------|
| x86\_64      | `rdtsc`       | `IA32_TSC` (Time Stamp Counter)   | CPU core clock (GHz)    |
| AArch64      | `mrs cntvct_el0` | Virtual timer counter          | `CNTFRQ_EL0` Hz (usually 25–100 MHz) |

Both counters are:
- **Monotonic** — never decrease within a single boot.
- **Readable without privilege escalation** — accessible from EL1 / kernel mode without extra setup.
- **Available before the memory allocator** — the `boot_mark!` macro calls only `read_hw_counter()` and `serial_println!`; no heap allocation is performed.

> **Converting ticks to wall time** — divide the delta by the counter
> frequency.  On x86_64 you can read `CPUID.15H` (crystal frequency) or
> `CPUID.16H` (nominal core clock) to derive the TSC frequency at boot.

---

## Using `boot_mark!`

The macro is defined in `src/boot_perf.rs` and re-exported at crate root via
`#[macro_export]`.

```rust
// anywhere in kernel code that has serial output available
crate::boot_mark!("MY_CUSTOM_MARKER");
```

This expands to:

```rust
{
    let _ticks = crate::boot_perf::read_hw_counter();
    crate::serial_println!("BOOT_MARK label={} ticks={}", "MY_CUSTOM_MARKER", _ticks);
}
```

The macro is intentionally inline-only (`#[inline(always)]` on
`read_hw_counter`) to minimise the overhead between the counter read and the
print.  Adding a new milestone requires only one line of Rust at the callsite.

---

## CI Integration

### Workflow

A boot-performance CI job is planned. When enabled, it should:

1. Builds the kernel for x86_64 and aarch64.
2. Boots each image under QEMU and captures serial output.
3. Calls `scripts/ci/parse-boot-marks.sh <log>` which:
   - Extracts all `BOOT_MARK` lines.
   - Asserts all four required milestones are present (exits 1 otherwise).
   - Prints a formatted table of absolute tick counts and inter-milestone deltas.
4. Uploads the raw QEMU logs as build artefacts for trend inspection.

### Parser script

```bash
scripts/ci/parse-boot-marks.sh qemu-x86_64-boot.log
```

Sample output:

```
=======================================
  RustOS boot performance markers
=======================================
MILESTONE                       TICKS (abs)         DELTA (ticks)
-----------------------------------------------------------------------
BOOT_ENTRY                         12345678          (baseline)
BOOT_MMU_ON                        12350000                  4322
BOOT_INITRAMFS_LOADED              12360000                 10000
BOOT_INIT_EXEC                     12380000                 20000
-----------------------------------------------------------------------
TOTAL (ENTRY → INIT_EXEC)                                   34322
=======================================

BOOT_PERF OK — all 4 milestones present.
```

The script exits with code `1` and prints a diagnostic to stderr if any
milestone is absent, failing the CI job.

### Regression detection

To detect regressions, compare the delta for a segment across commits.  A
suitable strategy is:

1. Record the baseline delta for each segment on `main`.
2. Add a threshold check in `parse-boot-marks.sh` (or a wrapper script) that
   fails if any delta exceeds the baseline by more than an agreed percentage.
3. Store the baseline in a checked-in file (e.g. `perf-baselines/x86_64.txt`)
   and update it when an intentional performance change lands.

---

## Adding New Milestones

1. Choose a `SCREAMING_SNAKE_CASE` name prefixed with `BOOT_`.
2. Insert `crate::boot_mark!("BOOT_MY_STAGE");` at the desired callsite.
3. Update the **Defined Milestones** table above.
4. Update `ORDERED` in `scripts/ci/parse-boot-marks.sh` if the new milestone
   should be required by CI.

---

## Design Notes

- **No static storage** — `boot_mark!` does not write to any global array;
  the serial line *is* the record.  This avoids synchronisation hazards
  during early boot before locks are initialised.
- **No floating point** — tick counts are printed as raw integers; unit
  conversion is left to the CI script running on the host.
- **Stable format** — the `BOOT_MARK label= ticks=` grammar is considered
  stable from current onward.  Parsers may rely on it.  Changing the
  format requires a documentation update and a CI script update.
