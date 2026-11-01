# Test and coverage boundaries

RustOS uses hosted tests, direct-source harnesses, fuzz targets, emulator contracts, and textual guards. No whole-kernel coverage percentage or complete hardware test result was measured during the `08a14f6` documentation audit.

## Executed host suites

| Suite | Result | What it actually covers |
| --- | --- | --- |
| `cargo xtask test` | 16 passed | Hosted `userspace_boot`-scoped library tests |
| `test-region-heap.sh` | 3 passed | Production region allocator |
| `test-pmm-init.sh` | 2 passed | PMM source with host physical-memory backing |
| `test-kstack-ownership.sh` | 4 passed | Production ownership registry and simulated safe retirement/reclamation |
| `test-exit-lifecycle.sh` | 2 passed | Exit, process table, wait logic with substitute scheduler/resources |
| `fault_engine.rs` | 7 passed | Actual atomic fault-point engine |
| `test-boot-marks.py` | 14 passed | Strict timing-log parser |
| Python CI test discovery | 45 passed | Smoke/fault/panic parser and fuzz-coverage checks |
| `scheme-api` | 2 unit tests + 1 doctest passed | Protocol helpers |
| `sync-primitives` | 8 passed | Portable lock/RCU implementation tests |
| `fs-ext4`, `fs-fat32` | 3 passed each | Adapter/probe helpers |
| `vfs-core`, `mm-core`, `fs-tmpfs` | 0 tests | Successful compilation/invocation only |
| `userspace/smoke/tests/assertions.rs` | 11 failed | Stale host mocks; no passing smoke assertion claim |

The direct smoke test's mocks do not match the current sequence and payload expectations. The guest workload is more extensive than its old happy-path fixture; repairing the fixture is source work outside this documentation change.

## Kernel test selection

`src/kmtest/mod.rs` declares registration and suites for selected fs, MM, process, scheme-FD, virtqueue, smoke, and OOM-recovery tests. Those tests require the full graph plus `kmtest` and an executable harness; being present under `kmtest/` does not mean a host test command ran them.

`kmtest/fault_inject_tests.rs` is not declared by the test root. Similarly, the standalone VFS POSIX test file is not automatically part of the declared VFS tests.

## Fuzzing

The `fuzz` workspace has four libFuzzer targets. They import production parsing code with narrow host adapters rather than reimplementing the parser in the harness.

| Target | Production surface |
| --- | --- |
| `cpio_newc` | `src/init/initramfs/mod.rs` |
| `elf64` | `src/exec/elf.rs` metadata parsing/validation |
| `vfs_path` | `src/fs/path.rs` |
| `ext4_metadata` | `src/fs/ext4.rs` pure metadata decoding |

```bash
cargo install cargo-fuzz --locked
cargo fuzz run cpio_newc --sanitizer address -- \
  -max_len=4096 -max_total_time=15 -dict=fuzz/dictionaries/cpio_newc.dict
bash scripts/ci/fuzz-coverage.sh
```

Use the checked-in workflow for bounded timeouts and the other targets. No fuzz campaign or numeric parser coverage measurement was executed during this audit; host tests of the coverage guard do not count as fuzzing.

## What is not established

- **Full execution**: No passing full-kernel compile or QEMU userspace run in this audit.
- **Concurrency safety**: Host lock/lifetime tests do not exercise hardware interrupts, all CPU interleavings, TLBs, or real context switches.
- **Fault recovery**: Engine budgets firing on the host are not injected failures in PMM, VMM, descriptors, or a running process.
- **Persistence**: Metadata/probe tests do not validate crash recovery or mounted filesystem semantics.
- **Device support**: No physical NIC, disk, GPU, input, or camera validation.
- **Coverage totals**: No aggregate percentage, branch count, or claim that all declared tests are currently runnable.

## Evidence standard

Record source revision, command, target/features, result, and whether the harness uses production code or substitutes. Keep failures and unexecuted gates visible in [the audit](source-audit.md) instead of merging them into a single misleading pass count.
