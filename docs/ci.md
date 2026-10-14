# CI and Local Validation Contract

_CI coverage boundary updated: 2026-10-13._

RustOS uses `cargo xtask` as the canonical automation layer. Raw `cargo` commands
are useful for debugging, but CI and pre-push validation should go through
`xtask` so target specs, features, firmware staging, FAT image creation, and
serial-log checks stay consistent.

## Primary commands

| Command | Purpose |
|---|---|
| `cargo xtask check --arch x86_64` | Type-check the default full subsystem graph with x86_64 UEFI entry |
| `cargo xtask smoke --arch x86_64` | Build the full graph and initramfs, boot QEMU under a deadline, require the full userspace execution protocol, run the timing parser on that real log, and write JSON run metadata |
| `cargo xtask smoke --arch aarch64` | Same full execution contract for AArch64 UEFI, subject to firmware availability |
| `cargo xtask smoke --arch x86_64 --features boot_minimal` | Explicit minimal-only QEMU smoke, accepting only `BOOT_MINIMAL_OK` |
| `cargo xtask build-init --arch x86_64` | Build `/init` and `/smoke`, stage both in the initramfs, and pack `initramfs.cpio` |
| `cargo xtask roadmap-check` | Validate roadmap/status/syscall/fault docs contain required topics |
| `bash scripts/ci/check-stubs.sh` | Guard documented stub classifications |
| `cargo xtask ci-local` | Fast aggregate local gate: check, host tests, module lint, stub guard, docs guard |
| `bash scripts/ci/check-rust-format.sh` | Check all tracked Rust files under src/crates/xtask without traversing absent experimental module declarations |
| `bash scripts/ci/parse-boot-marks.sh --mode minimal <log>` | Validate the minimal boot contract and entry-to-MMU timing |
| `bash scripts/ci/parse-boot-marks.sh <log>` | Strict full/userspace marker contract, including `BOOT_INIT_EXEC` |
| `bash scripts/ci/boot-regression.sh <log>` | Compare boot times against baselines; fails if any phase exceeds baseline by >20% |

## Serial success and execution markers

`cargo xtask smoke` captures a fresh QEMU serial log at
`target/smoke-<arch>.log`. It truncates any prior log before launching QEMU,
enforces an architecture-specific deadline, and writes
`target/smoke-<arch>-metadata.json` with the selected profile, architecture,
host environment, elapsed time, deadline, result, timing-parser result, log path,
and expected contract.

The explicit minimal profile accepts only `BOOT_MINIMAL_OK`. It does not accept
the generic idle-loop message, `FULL_OS_USERSPACE_OK`, or a stale marker from a
previous run.

The default full-kernel profile requires all of these execution records:

```text
RUSTOS_EXEC marker=smoke_started
RUSTOS_EXEC marker=smoke_tests_passed
RUSTOS_EXEC marker=smoke_child_reaped pid=<pid> exit=0
RUSTOS_EXEC marker=init_complete exit=0
```

These records are emitted across the real userspace child path. `/init` must
successfully execute `/smoke`, wait for the same child PID, and observe normal
termination with status zero before it emits completion. An exec failure,
failed wait, signal termination, or nonzero exit is a hard failure. Reaching an
idle loop is never evidence of a successful userspace boot.

Boot timing markers remain a separate contract. After each QEMU run, xtask
applies `scripts/ci/parse-boot-marks.sh` with `--mode minimal` or `--mode full`
to that run's actual serial log. A timing parser pass does not substitute for
the userspace execution markers, and a marker parser unit test does not count
as runtime evidence. The full smoke gate is expected to remain red until
working process creation and image replacement are integrated.

## Boot performance regression checks

RustOS emits machine-parseable boot timing markers (`BOOT_MARK label=<LABEL> ticks=<N>`)
during every boot. See [`docs/boot-perf.md`](boot-perf.md) for the complete format specification.

The CI boot-performance workflow:

1. Boots each supported architecture under QEMU and captures serial output.
2. Runs `scripts/ci/parse-boot-marks.sh --mode minimal` for the explicitly selected minimal image.
3. Requires entry/MMU markers and `BOOT_MINIMAL_OK`, rejecting malformed, out-of-order, or failing logs.
4. Reports entry-to-MMU ticks only; it does not claim initramfs or userspace performance.

The full parser contract remains strict and cannot be satisfied by a minimal
boot. Baseline comparison is separate tooling, not a measurement performed by
the current marker-validation workflow.

Baseline files are stored in `docs/` (e.g., `boot-perf-baseline.txt`) and should only be updated via intentional `[perf-update]` commits.

To run locally:

```sh
cargo xtask smoke --arch x86_64 --features boot_minimal
bash scripts/ci/parse-boot-marks.sh --mode minimal target/smoke-x86_64.log
cargo xtask smoke --arch x86_64
# Full smoke also runs the full timing parser against its fresh serial log
bash scripts/ci/boot-regression.sh target/smoke-x86_64.log
```

## QEMU defaults

| Architecture | Machine | Firmware | Timeout | Log path |
|---|---|---|---:|---|
| x86_64 | `q35` | `OVMF_CODE` or discovered OVMF | 60s | `target/smoke-x86_64.log` and `target/smoke-x86_64-metadata.json` |
| aarch64 | `virt` | `QEMU_EFI` or discovered AAVMF/QEMU EFI | 45s | `target/smoke-aarch64.log` and `target/smoke-aarch64-metadata.json` |

## Adding CI coverage

1. Prefer adding an `xtask` subcommand or extending an existing one over adding
   a one-off shell command.
2. Capture serial output to a file and assert markers, not sleeps.
3. Update `docs/status.md`, `docs/milestones.md`, and this file when the gate
   changes what is considered supported.
4. For boot performance changes, update baselines only via `[perf-update]` commits after validating that regressions are intentional.

## Coverage boundaries and unavailable gates

- **Regression Tests:** Reuses the runnable host tests and UEFI profile build matrix rather than referring to absent integration, fuzz, and benchmark scripts. This is not full-kernel or filesystem compatibility coverage.
- **Fault Injection Engine:** Runs six host tests against the actual fault-point implementation. PMM/VMM/syscall injection is not exercised because the supported boot profiles exclude those services.
- **Kernel fault integration:** Explicitly skipped by default. `RUSTOS_ENABLE_UNSUPPORTED_KERNEL_FAULT_GATE=true` enables a failing readiness guard, not a fake test run; replace that guard only after implementing a real harness with mandatory injected/results markers.
- **Panic format:** Boots the supported diagnostic profile without an initramfs and validates its real, intentional panic on x86_64 and AArch64. This does not test an OOM or full debug register-dump path.
- **Formatting and lint:** All existing tracked Rust sources in the original kernel/workspace formatting scope remain checked. Clippy keeps `-D warnings`; missing experimental module files are not fabricated just to satisfy rustfmt's default traversal.
- **Legacy kmtest label:** The existing separate kmtest workflow still uses boot-smoke plumbing and must not be counted as executed kernel suites. The main CI job now labels its repeat explicitly as minimal smoke.
- **Workflow structure:** Pinned actionlint validates YAML, expressions, and local workflow references. Its optional shellcheck/pyflakes integrations are not part of this new structural check.

## Image size checks

The `release-boot` profile produces lean boot images optimized for size. A CI size-check step (planned) will:

- Build the kernel with `--profile release-boot --features uefi_boot`.
- Measure the resulting `boot-x86_64.img` size.
- Fail if the image exceeds a configured threshold (e.g., 2 MB).
- Track size trends across commits to detect bloat early.

Run locally:

```sh
cargo xtask build --arch x86_64 --profile release-boot
ls -lh target/x86_64-unknown-uefi/release-boot/rustos.efi
```

Use `cargo bloat --release --target x86_64-unknown-uefi` to analyze which functions contribute most to binary size.
