# Syscall performance

RustOS has a guest getpid microbenchmark and a separate in-kernel syscall counter facility. At `08a14f6`, the x86_64 benchmark binary builds, but no guest performance measurement was collected in this audit.

## Guest benchmark

`userspace/bench/src/main.rs` performs 1,000 warm-up calls and 8,192 measured getpid calls. It collects 256 timed batches of 32 calls, using two `clock_gettime` calls per batch.

| Schema-2 field | Required value or meaning |
| --- | --- |
| `schema_version`, `benchmark_version` | Both `2` |
| `benchmark` | `getpid` |
| `warmup_iterations` | `1000` |
| `measured_iterations`, `successful_calls` | Both `8192` |
| `operation_counts` | `getpid: 8192`, `clock_gettime: 512` |
| `sample_count`, `sample_batch_size` | `256`, `32` |
| `elapsed_ns` | Sum of measured batch durations |
| `mean_ns_per_call` | Integer `elapsed_ns / 8192` |
| `latency_ns_per_call` | Ordered `min`, `p50`, `p90`, `p99`, `max` of batch-average samples |
| `pid_checksum` | Positive checksum of successful PID results |
| `environment` | Guest, execution context, build profile, clock, ABI, host, compiler, QEMU metadata |

Percentiles describe batch-average per-call values, not a histogram of individual syscall latencies. The JSON emits totals and percentile summaries, not the complete raw sample array.

The record identifies `RustOS`, `QEMU guest`, `release`, `CLOCK_MONOTONIC`, and `raw_linux_compatible` in the corresponding environment fields. xtask injects available host/compiler/QEMU strings during benchmark compilation; metadata can say `unavailable` and is not proof that QEMU was run.

## Execution validation

```bash
cargo xtask build-init --arch x86_64
cargo xtask smoke --arch x86_64
python3 scripts/ci/parse_smoke_log.py target/smoke-x86_64.log \
  --metadata target/smoke-x86_64-metadata.json --arch x86_64
```

The strict parser checks exact keys, integer bounds, fixed counts, mean consistency, percentile ordering/bounds, architecture, nonempty environment values, and the full child-execution sequence. A JSON line by itself is not an accepted benchmark run.

## Kernel counters

`src/syscall/profile.rs` exposes a 512-entry table of invocation counts and cumulative counter deltas when `syscall-trace` is enabled. Numbers outside the table are not counted.

x86_64 reads TSC, AArch64 reads `PMCCNTR_EL0`, and other targets use the current zero fallback. These values are not automatically nanoseconds, and AArch64 counter availability/configuration requires its own runtime validation.

`src/fs/proc/syscall_stats.rs` formats `/proc/syscall_stats` as `NR invocations total_cycles avg_cycles`, sorted by cumulative count-of-ticks. Without the feature it emits explanatory text instead of measurements.

## Interpretation

Compare the same target, host, QEMU configuration, compiler, build/profile/features, clock, and workload. Counter overhead, guest clock resolution, and batched timing all affect interpretation.

No specific latency, percentage improvement, or cross-architecture performance ranking is established by this documentation. The full build must first pass and execute the benchmark under the intended runtime contract.
