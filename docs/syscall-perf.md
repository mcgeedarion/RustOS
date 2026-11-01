# Syscall benchmark contract and separate profiling facility

## Collecting benchmark evidence

```bash
cargo xtask smoke --arch x86_64
python3 scripts/ci/parse_smoke_log.py target/smoke-x86_64.log \
  --metadata target/smoke-x86_64-metadata.json --arch x86_64
```

The full-kernel smoke runs `/bench` as a separate userspace child. PID 1 must
reap that specific child with exit status zero. A JSON record alone is not a
completed smoke run, and a passing smoke is not a performance improvement claim.
No latency thresholds are enforced. Retain the serial log, xtask metadata,
revision, QEMU configuration, and repeated before/after runs.

## Schema-version-2 JSON

The producer is `userspace/bench/src/main.rs`. It emits one standalone JSON
object, then the standalone sentinel `RUSTOS_BENCH marker=complete`.
The following table follows exact producer key emission order. JSON object
order is not semantically significant: the parser checks exact key sets, not
order. Unknown and duplicate keys are rejected.

| Key | Meaning and validation |
| --- | --- |
| `schema_version` | Integer `2`, identifying the record contract. |
| `benchmark` | String `getpid`, identifying the measured operation. |
| `benchmark_version` | Integer `2`, identifying this batched workload. |
| `architecture` | `x86_64`, `aarch64`, or `riscv64`; matches `--arch` when supplied and optional metadata. |
| `environment` | Required object documented below. |
| `warmup_iterations` | Integer `1000`: unmeasured getpid calls before sampling. |
| `measured_iterations` | Integer `8192`: 256 samples times 32 getpid operations. |
| `successful_calls` | Integer `8192`: every measured getpid returned a positive PID. |
| `operation_counts` | Exactly `getpid: 8192`, then `clock_gettime: 512` in emission order. These counts exclude warmup. |
| `elapsed_ns` | Sum of the 256 nonnegative batch clock deltas, in nanoseconds. |
| `mean_ns_per_call` | Integer `elapsed_ns / 8192`, rounded down. |
| `sample_count` | Integer `256`. |
| `sample_batch_size` | Integer `32`. |
| `latency_ns_per_call` | Object emitted in order `min`, `p50`, `p90`, `p99`, `max`; integer nanoseconds per call. |
| `pid_checksum` | Positive u64 wrapping sum of PIDs from both warmup and measured calls, not just measured calls. |

All numeric fields above are unsigned 64-bit JSON integers. Booleans, floats,
strings, negative values, out-of-range integers, and non-finite constants are
not accepted as numbers. Timer resolution may legitimately produce zero.

### Warmup, sampling, and percentile definitions

The 1,000 warmup calls run outside measured timing. For each of 256 samples,
the program reads `CLOCK_MONOTONIC`, executes 32 getpid syscalls, and reads the
clock again. It adds the batch delta to `elapsed_ns` and stores
`floor(batch_delta_ns / 32)` as that sample's per-call latency. The 512 clock
calls are two per sample. Clock syscall and loop overhead are not subtracted;
batching amortizes timing overhead but does not eliminate it. Sorting happens
after the timed work.

The 256 integer samples are sorted ascending. Percentile `p` selects zero-based
index `floor((256 - 1) * p / 100)` without interpolation: `p50` uses index 127,
`p90` index 229, and `p99` index 252. Min and max use indices 0 and 255.
These are percentiles of batch-average latency, not 8,192 individually timed
syscalls. The required ordering is `min <= p50 <= p90 <= p99 <= max`; ties are
valid. The integer overall mean must also lie between min and max.

### Required environment metadata

The environment object emits these exact keys in this order:

| Key | Source/value |
| --- | --- |
| `guest` | `RustOS` |
| `execution_context` | `QEMU guest` |
| `build_profile` | `release`: the benchmark's profile, not necessarily the kernel's. |
| `clock` | `CLOCK_MONOTONIC` |
| `syscall_abi` | `raw_linux_compatible` |
| `host_os` | Build-time `RUSTOS_HOST_OS`, set by xtask from the host OS. |
| `host_arch` | Build-time `RUSTOS_HOST_ARCH`, set by xtask from the host architecture. |
| `compiler_version` | Build-time `RUSTOS_COMPILER_VERSION`, set by xtask from rustc. |
| `qemu_version` | Build-time `RUSTOS_QEMU_VERSION`, set by xtask from the selected QEMU executable. |

`userspace/bench/build.rs` propagates these variables into the benchmark build.
Direct builds without them may emit `unknown`. The parser requires nonempty
strings and the five fixed values, but accepts `unknown` for host/build strings.
An unknown value does not establish a reproducible environment. Build-host
metadata does not automatically describe another host running a copied image.

## Strict log validation

`scripts/ci/parse_smoke_log.py` uses only the Python standard library and scans
the whole log before reporting success. It tolerates LF, CRLF, and LFCR at line
boundaries; it never removes an embedded CR to repair a corrupted record.

It requires one ordered full-kernel sequence:

```text
BOOT_INIT_EXEC
USERSPACE_SMOKE_START
RUSTOS_EXEC marker=smoke_started
RUSTOS_SMOKE marker=fs_roundtrip_pass
RUSTOS_SMOKE marker=ipc_roundtrip_pass
USERSPACE_SMOKE_PASS
RUSTOS_EXEC marker=smoke_tests_passed
RUSTOS_EXEC marker=smoke_child_reaped pid=<positive-pid> exit=0
<one valid schema-version-2 benchmark JSON record>
RUSTOS_BENCH marker=complete
RUSTOS_EXEC marker=bench_child_reaped pid=<different-positive-pid> exit=0
RUSTOS_EXEC marker=init_complete exit=0
FULL_OS_USERSPACE_OK
```

Quoted, prefixed, indented, suffixed, duplicate, missing, or reordered records
cannot substitute for exact records. Minimal boot success is not full-kernel
proof. Any kernel panic, smoke/test failure, `*_failed` diagnostic, or
`USERSPACE_BOOT_UNSUPPORTED` anywhere, including after the last milestone,
rejects the log. Schema, nested keys, counts, integer ranges, percentile order,
mean arithmetic, and environment presence are checked.

Optional xtask metadata must contain `result: "passed"`, a matching
`architecture`, and `contract: "full_userspace_exec"`.

`--require-fault-scenarios <csv>` additionally requires each requested catalogue
scenario's exact `fired -> recovered -> post_recovery_pass` sequence. The final
record must name a workload and contain `exit=0`. Duplicate/out-of-order guest
fault records, unknown scenarios, and host-engine-scoped records are rejected.
This parser does not arm faults or implement guest follow-up workloads. See
`fault-recovery.md` for the guest evidence boundary.

## Separate syscall-trace profiling facility

`syscall-trace` is optional and off by default. Kernel hooks collect per-syscall
invocation counts and cumulative hardware cycles/ticks; average values are
derived at read/dump time. This is separate from the userspace CLOCK_MONOTONIC
benchmark and its JSON contract. Do not mix the facilities' units or interpret
profiling counters as benchmark completion evidence.

```bash
cargo xtask build --arch x86_64 --profile release --features syscall-trace
# Run the workload; collect /proc/syscall_stats or a debug serial dump.
```

Normal benchmark comparisons should keep profiling instrumentation disabled.
Record the workload, target, kernel/userspace feature sets, revision,
host/QEMU configuration, units, and repeated-run variance. Do not add stale
top-N tables or numeric regression thresholds without a stable baseline.
