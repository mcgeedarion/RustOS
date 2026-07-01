# Syscall Performance Profile

_Last reviewed: 2026-07-01._

RustOS has optional syscall profiling hooks behind the `syscall-trace` feature,
but the historical numeric profile in this document is no longer treated as a
current benchmark. Use the workflow below to collect fresh data before making
performance claims.

## Profiling infrastructure

`syscall-trace` is off by default. When enabled, syscall code can collect:

| Counter | Purpose |
|---|---|
| invocations | Number of calls seen per syscall |
| total cycles/ticks | Cumulative hardware-counter delta |
| average cycles/ticks | Derived at read/dump time |

The feature is documented as zero-overhead when disabled: normal smoke and
release-boot builds should not enable it.

## Build and run pattern

```bash
cargo xtask build --arch x86_64 --profile release --features syscall-trace
# Run the relevant QEMU/userspace workload, then collect /proc/syscall_stats
# or call the serial dump helper from a debug path.
```

For userspace-handoff experiments, combine the profiling feature with the boot
profile being tested only when the module graph supports that combination.

## Data freshness policy

- Do not keep stale top-N syscall tables in this document.
- Add a dated table only when the exact workload, target, QEMU/CPU settings, and
  commit context are known.
- Performance changes should include before/after measurements and should state
  whether numbers are hardware cycles, architectural timer ticks, or another
  unit.

## Current status

| Item | Status |
|---|---|
| `syscall-trace` feature exists | yes |
| Profiling path is default-enabled | no |
| Fresh benchmark table checked in | no |
| Desired CI regression gate | planned |

## Future work

1. Add a repeatable syscall workload for the M3 init syscall set.
2. Teach CI or a developer script to extract `/proc/syscall_stats` from a QEMU
   run.
3. Add optional thresholds only after a stable baseline exists.
