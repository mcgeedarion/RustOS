# Continuous integration

CI has multiple independent workflows with different contracts, not one universal “kernel passed” gate. This guide maps the checked-in workflows at `08a14f6`; it does not claim that remote workflow runs are currently green.

## Workflow map

| Workflow file(s) | Actual scope |
| --- | --- |
| `ci.yml`, `lint.yml`, `fmt.yml` | Formatting baseline, selected clippy targets, module hygiene; CI also has full build and smoke jobs |
| `build.yml`, `kernel-test.yml` | Reusable architecture build and selected minimal/kmtest launch paths |
| `focused-regressions.yml`, `regression-tests.yml` | Hosted filesystem/diagnostic tests, Python parsers, x86_64/AArch64 profile-build matrix |
| `allocator-repair.yml` | RegionHeap host tests followed by full x86_64 check |
| `pmm-init.yml` | PMM initialization host tests followed by full check |
| `kstack-ownership.yml` | Stack registry tests followed by full check |
| `exit-lifecycle.yml` | Exit/wait/table host tests followed by full check |
| `boot-smoke.yml`, `qemu-smoke.yml` | Explicitly selected minimal/release-boot image and firmware contracts |
| `full-kernel-smoke.yml` | Full x86_64 kernel, real init/smoke/bench lifecycle, strict log parser |
| `boot-perf.yml` | Boot marker parser unit tests and minimal x86_64/AArch64 timing logs |
| `image-size.yml` | x86_64 release-boot disk-image byte limit |
| `kmtest.yml` | Kernel-test selection/launch wrapper; does not make all test files reachable |
| `fault-inject.yml` | Real atomic fault-engine host tests; guest integration is unsupported |
| `parser-fuzz.yml` | Bounded CPIO/ELF/VFS/ext4 fuzz jobs and production-source coverage guard |
| `panic-format.yml` | Panic-output validation |
| `status-check.yml` | `cargo xtask roadmap-check`, a required-topic text check |
| `workflow-lint.yml` | Workflow syntax/static linting |
| `task3-smoke-markers.yml`, `task4-fault-sites.yml` | Special patch-application/validation workflows; they modify their checkout before testing |
| `labeler.yml`, `stale.yml` | Repository maintenance, not kernel validation |

The task3/task4 scripts are code-modifying helpers, not ordinary read-only tests. Do not run them merely to validate documentation or attribute their patched-checkout results to unmodified source.

## Local checks

```bash
cargo xtask test
cargo xtask lint-modules
cargo xtask roadmap-check
bash scripts/ci/check-stubs.sh
bash scripts/ci/test-region-heap.sh
bash scripts/ci/test-pmm-init.sh
bash scripts/ci/test-kstack-ownership.sh
bash scripts/ci/test-exit-lifecycle.sh
python3 scripts/ci/test-boot-marks.py
python3 -m unittest discover -s scripts/ci/tests -p 'test_*.py'
```

`cargo xtask ci-local` also includes the full x86_64 check, so it is expected to stop at the existing full-build failures in this revision. A successful isolated host test must not be substituted for that failed gate.

## Compile matrix

```bash
cargo xtask check --arch x86_64
cargo xtask check --arch x86_64 --features boot_minimal
cargo xtask check --arch x86_64 --features userspace_boot
cargo xtask check --arch aarch64 --features boot_minimal
cargo xtask check --arch aarch64 --features userspace_boot
cargo xtask check --arch riscv64 --features boot_minimal
cargo xtask check --arch riscv64 --features userspace_boot
```

During the audit, x86_64/AArch64 slim checks passed, the full x86_64 check failed, and both RISC-V slim checks failed on `mem_layout.rs::SIZE`. Only `check` was executed for these kernel combinations; final linking and QEMU execution are separate evidence.

## Full runtime evidence

```bash
cargo xtask smoke --arch x86_64
python3 scripts/ci/parse_smoke_log.py target/smoke-x86_64.log \
  --metadata target/smoke-x86_64-metadata.json --arch x86_64
bash scripts/ci/parse-boot-marks.sh --mode full target/smoke-x86_64.log
```

The serial log, metadata, timing-parser result, compiler/build output, and benchmark record must belong to the same run. The parser rejects minimal/diagnostic evidence, failure markers, duplicated or reordered execution milestones, malformed benchmark data, and invalid child reaping.

xtask sets its smoke timeout to 60 seconds for x86_64 and 45 seconds for the other architectures. CI job timeouts are separate outer limits, and neither a timeout nor missing artifacts may be recorded as success.

## Guards and limitations

- **Module lint**: Checks a visibility/re-export heuristic. It passes despite the duplicate socket module and therefore cannot replace compilation.
- **Roadmap check**: Looks for required literal topics in five docs. It protects navigation/content presence, not factual accuracy.
- **Stub guard**: Inspects nearby cfg lines and warning strings. Its warnings are informational, and it does not compile a symbol table or run a guest.
- **Production requirements script**: Has a workspace-root calculation problem and relies on string matching. It is not an authoritative production gate.
- **Fault workflow**: `RUSTOS_ENABLE_UNSUPPORTED_KERNEL_FAULT_GATE=true` selects a job that exits with failure. It does not enable a hidden working guest test.

Use [source-audit.md](source-audit.md) for observed local results and [code_coverage.md](code_coverage.md) for their exact boundaries. CI configuration describes intended checks; only retained run evidence establishes results.
