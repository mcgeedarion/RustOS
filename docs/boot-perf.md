# Boot Performance Instrumentation

RustOS ships a lightweight, zero-allocation boot timing subsystem that emits
machine-parseable milestone markers on the serial console during every boot.
CI validates the explicit **minimal boot** contract and logs only the timing
segments that path actually measures. The parser's default **full/userspace**
contract remains strict: a minimal boot is not evidence of init execution.
The workflow validates evidence and reports ticks; it does not currently
enforce a performance regression threshold.

---

## Wire Format

Every marker emitted by `boot_mark!` follows this exact grammar:

```
BOOT_MARK label=<LABEL> ticks=<DECIMAL_U64>
```

| Field   | Type           | Description                                         |
|---------|----------------|-----------------------------------------------------|
| `BOOT_MARK` | literal    | Fixed prefix at the start of the record             |
| `label` | `[A-Z0-9_]+`  | Milestone identifier (see table below)              |
| `ticks` | `u64` decimal | Raw counter value from the hardware timer register  |

The emitter separates fields with a single ASCII space. The parser also accepts
multiple spaces or tabs between fields, but not prefixes, quotes, extra fields,
or trailing text. The complete tick value must be an unsigned decimal integer
in `0..18446744073709551615`; leading zeroes are decimal, not octal.
Signed, fractional, overflowing, and partially numeric values are rejected.

Serial transport can produce LF, CRLF, or LFCR line endings. The parser trims
carriage returns only at line boundaries and handles a final record without
a newline. Embedded carriage returns cannot turn a malformed value into a valid
one. Non-marker console output is ignored except for failure diagnostics.

### Full-contract fixture example (not a claim of current userspace support)

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
| `BOOT_ENTRY`             | Early common kernel entry; the current minimal path emits it again on entry to `boot_minimal::enter` |
| `BOOT_MMU_ON`            | In minimal boot, an observable point after firmware/arch mappings are live, not the instant the MMU was enabled |
| `BOOT_INITRAMFS_LOADED`  | An initramfs milestone in a real userspace path; **not meaningful in minimal boot** |
| `BOOT_INIT_EXEC`         | Required only by the full contract; existing architecture boot paths place it after successful process spawning, not at a measured first userspace instruction |
| `BOOT_MINIMAL_OK`        | Untimed minimal-path completion sentinel, emitted as `RustOS: BOOT_MINIMAL_OK` |

> **Legacy placeholder:** `boot_minimal` still emits `BOOT_INITRAMFS_LOADED`,
> although it does not parse or mount an initramfs. Minimal parsing does **not**
> require this marker and excludes it from timing rows and totals. If present,
> its syntax, range, uniqueness, and ordering are still validated. No initramfs
> or userspace performance is inferred from it.
>
> The transitional `userspace_boot` diagnostic path cannot execute `/init` and
> does not emit `BOOT_INIT_EXEC`. It must fail the full parser contract rather
> than claim successful execution. A successful synthetic full fixture tests
> the parser only; it does not demonstrate an operational userspace boot.

### Parser contracts

| Mode | Required evidence, in serial order | Reported timing |
|------|-----------------------------------|-----------------|
| `--mode minimal` | `BOOT_ENTRY`, `BOOT_MMU_ON`, then an exact `RustOS: BOOT_MINIMAL_OK` or bare `BOOT_MINIMAL_OK` line | Entry-to-MMU ticks only; success sentinel is untimed |
| `--mode full` (also the default) | `BOOT_ENTRY`, `BOOT_MMU_ON`, `BOOT_INITRAMFS_LOADED`, `BOOT_INIT_EXEC` | All four milestones and entry-to-init-exec total |

Full mode rejects explicit minimal-path evidence (the minimal banner or
success sentinel), even if all four marker labels also appear. Minimal mode
rejects `BOOT_INIT_EXEC` rather than mixing contracts. There is no automatic
fallback from full to minimal.

### Ordering, duplicates, and failure handling

- Parse one boot per log. Required milestones must appear in their defined
  serial order, and **all** marker ticks, including optional/unknown labels
  and the legacy placeholder, must be nondecreasing. Equal ticks are allowed;
  counter wrap/reset and negative deltas are not. Python integer arithmetic
  preserves exact deltas throughout the full `u64` range.
- Preserve the **first** `BOOT_ENTRY` as the baseline. The current kernel
  emits a second entry mark in the minimal dispatcher; allow exactly one
  nondecreasing repeat before any other marker. This intentionally includes
  dispatcher overhead rather than silently replacing the earlier timestamp.
  All other duplicate markers, later entry repeats, repeated success sentinels,
  and markers after minimal completion are errors.
- Only whole marker/sentinel lines count as evidence. A message such as
  `expected BOOT_MINIMAL_OK` or `BOOT_MARK ... ticks=123junk` cannot pass.
  A malformed line beginning with `BOOT_MARK` is an error, not a skipped record.
- The entire log is checked before any success table is printed. Panic,
  panicked, fatal, error, fail/failed/failure diagnostics, structured `PANIC_*`,
  `*_FAIL`/`*_FAILED`/`*_FAILURE`/`*_ERROR`/`*_UNSUPPORTED` tokens, and the legacy
  spaced `K E R N E L  P A N I C` banner fail the gate, including **after**
  completion. This is deliberately fail-closed even if later evidence looks
  successful. Ordinary QEMU timeout termination and CPU-reset debug messages
  are not success evidence or failure diagnostics by themselves.

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

`.github/workflows/boot-perf.yml`:

1. Runs host-only parser regression fixtures before the QEMU jobs.
2. Explicitly builds `boot_minimal` images for x86_64 and aarch64.
3. Boots each image under QEMU and captures serial output. A timeout is tolerated
   because minimal boot parks the CPU; a timeout alone never passes the gate.
4. Calls `bash scripts/ci/parse-boot-marks.sh --mode minimal <log>`, requiring
   the real minimal milestones and completion sentinel with no failure evidence.
5. Uploads the raw QEMU logs as build artefacts, including on failure.

### Parser script

```bash
# Minimal boot CI (Python 3.9+ and Bash; no third-party packages):
bash scripts/ci/parse-boot-marks.sh --mode minimal qemu-x86_64-boot.log

# Strict full/userspace contract; never use minimal mode to excuse a full boot:
bash scripts/ci/parse-boot-marks.sh --mode full qemu-full-boot.log
bash scripts/ci/parse-boot-marks.sh qemu-full-boot.log  # same strict default
```

Minimal fixture:

```
BOOT_MARK label=BOOT_ENTRY ticks=100
BOOT_MARK label=BOOT_ENTRY ticks=120
BOOT_MARK label=BOOT_MMU_ON ticks=140
BOOT_MARK label=BOOT_INITRAMFS_LOADED ticks=180
RustOS: BOOT_MINIMAL_OK
```

This reports `BOOT_ENTRY = 100`, `BOOT_MMU_ON = 140`, and a total of **40 ticks**
from entry to MMU. It notes that the second entry is not the baseline and the
legacy initramfs placeholder is excluded. It does not report the 80-tick span to
the placeholder as initramfs or total minimal-boot performance.

The script exits `0` only after validating the whole requested contract,
`1` for an invalid/unreadable log (diagnostic on stderr, no success table),
and `2` for an invalid invocation. It does not emit JSON.

### Host fixture regression tests

```bash
python3 scripts/ci/test-boot-marks.py
```

The standard-library suite creates log fixtures and executes the actual shell
entry point in a subprocess, from outside the repository working directory.
It covers positive minimal/full logs, real minimal log shape (including the
legacy placeholder), strict default mode, CRLF/LFCR, unterminated final lines,
duplicate entries, missing/out-of-order milestones, regressing counters,
panics/failures before and after success, malformed/suffixed/quoted evidence,
unsigned-64-bit boundaries and exact arithmetic, unrelated firmware/QEMU
output, and invalid invocations. No QEMU, Rust build, or network is required.

### Regression detection

To detect performance regressions, compare like-for-like deltas across commits
with the same architecture, counter frequency, QEMU configuration, and contract.
Do not compare minimal totals with full/userspace totals. A suitable strategy is:

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
4. Update the appropriate `ORDERED` contract in
   `scripts/ci/parse-boot-marks.py` and its host fixtures if the new milestone
   should be required by CI. Never require a synthetic marker for an operation
   that the selected boot profile does not perform.

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
