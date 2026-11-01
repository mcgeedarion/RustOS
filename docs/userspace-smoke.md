# Userspace smoke protocol

The full smoke protocol requires real init execution, guest assertions, distinct child processes, exact-child wait results, and a benchmark record. The protocol exists at `08a14f6`, but the audited full kernel fails compilation and no full QEMU pass is claimed.

## Producer paths

- **Kernel**: Full architecture entry, VFS/initramfs installation, process creation, scheduler, and user entry.
- **Init**: `userspace/init/src/main.rs` starts smoke and bench separately, validates exact child PIDs/statuses, and reports final completion.
- **Smoke**: `userspace/smoke/src/main.rs` and `checks.rs` perform the assertions. The program returns nonzero on failure.
- **Benchmark**: `userspace/bench/src/main.rs` emits schema-versioned measurements and exits.
- **Runner/parser**: xtask captures serial and metadata, and `scripts/ci/parse_smoke_log.py` validates the ordered execution record.

## Required single-boot sequence

The following is a protocol template, not a captured successful run. Replace PID placeholders with the actual distinct positive child PIDs and the JSON placeholder with the actual schema-2 benchmark record.

```text
BOOT_INIT_EXEC
USERSPACE_SMOKE_START
RUSTOS_EXEC marker=smoke_started
RUSTOS_SMOKE marker=fs_roundtrip_pass
RUSTOS_SMOKE marker=ipc_roundtrip_pass
USERSPACE_SMOKE_PASS
RUSTOS_EXEC marker=smoke_tests_passed
RUSTOS_EXEC marker=smoke_child_reaped pid=<smoke_pid> exit=0
<single schema_version=2 getpid benchmark JSON object>
RUSTOS_BENCH marker=complete
RUSTOS_EXEC marker=bench_child_reaped pid=<bench_pid> exit=0
RUSTOS_EXEC marker=init_complete exit=0
FULL_OS_USERSPACE_OK
```

The plain `BOOT_INIT_EXEC` execution record is separate from the timed `BOOT_MARK label=BOOT_INIT_EXEC ticks=...` record. Full smoke also runs the timing parser against the same serial log.

## Assertions in the workload

- **Basic descriptors**: Open expected content, compare actual read bytes, close, reject bad/closed FDs, and require failure for a nonexistent path.
- **Regular-file mutation**: Create a test file, write and read back a known payload, validate seeking/duplication/flags, rename, and unlink. Emit `fs_roundtrip_pass` only after assertions finish.
- **Pipe IPC**: Create pipe endpoints, write a known byte sequence, read and compare the exact payload, and close endpoints. Emit `ipc_roundtrip_pass` only after success.
- **Memory/system subset**: Exercise selected mapping, protection, unmapping, timing, identity, and error-return paths from the actual workload. This is not full Linux ABI coverage.
- **Process lifecycle**: Init checks matching wait results and zero status for each specific child. A final success string cannot replace the underlying child execution.

The host assertions use a mock backend and currently fail all 11 cases because the fixture is behind these checks. Parser unit tests pass, but they validate synthetic records rather than guest behavior.

## Reproduction

```bash
cargo xtask build-init --arch x86_64
cargo xtask smoke --arch x86_64
python3 scripts/ci/parse_smoke_log.py target/smoke-x86_64.log \
  --metadata target/smoke-x86_64-metadata.json --arch x86_64
bash scripts/ci/parse-boot-marks.sh --mode full target/smoke-x86_64.log
```

The x86_64 archive build passed during the audit; the full kernel did not. The smoke runner requires QEMU and appropriate firmware, which were not available for runtime validation in this audit.

## Rejection conditions

The parser rejects failures/panics, `USERSPACE_BOOT_UNSUPPORTED`, minimal success evidence, missing/duplicate/reordered milestones, invalid child status/PID syntax, reused child PIDs, malformed or inconsistent benchmark data, and mismatching metadata. Logs from multiple boots must not be concatenated to satisfy one protocol.

Retain raw serial, metadata, build output, timing-parser result, and parser output. For requested fault scenarios, the additional per-scenario chain in [fault recovery](fault-recovery.md) must also be present.
