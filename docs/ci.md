# CI and Local Validation Contract

_Last reviewed: 2026-07-01._

RustOS uses `cargo xtask` as the canonical automation layer. Raw `cargo` commands
are useful for debugging, but CI and pre-push validation should go through
`xtask` so target specs, features, firmware staging, FAT image creation, and
serial-log checks stay consistent.

## Primary commands

| Command | Purpose |
|---|---|
| `cargo xtask check --arch x86_64` | Type-check the default x86_64 UEFI/minimal path |
| `cargo xtask smoke --arch x86_64` | Build, image, boot QEMU, and assert a serial marker |
| `cargo xtask smoke --arch aarch64` | Same contract for AArch64 UEFI, subject to local firmware availability |
| `cargo xtask build-init --arch x86_64` | Build userspace init and pack `initramfs.cpio` |
| `cargo xtask roadmap-check` | Validate roadmap/status/syscall/fault docs contain required topics |
| `bash scripts/ci/check-stubs.sh` | Guard documented stub classifications |
| `cargo xtask ci-local` | Fast aggregate local gate: check, host tests, module lint, stub guard, docs guard |
| `bash scripts/ci/parse-boot-marks.sh <log>` | Parse boot performance markers from QEMU serial log |
| `bash scripts/ci/boot-regression.sh <log>` | Compare boot times against baselines; fails if any phase exceeds baseline by >20% |

## Serial success markers

`cargo xtask smoke` captures QEMU serial output to `target/smoke-<arch>.log` and
passes when any of these strings is present:

```text
BOOT_MINIMAL_OK|FULL_OS_USERSPACE_OK|entering cpu_idle
```

Timeout status from QEMU is tolerated because a successfully booted kernel may
be parked in the idle loop; the serial marker check is the actual pass/fail
signal.

## Boot performance regression checks

RustOS emits machine-parseable boot timing markers (`BOOT_MARK label=<LABEL> ticks=<N>`)
during every boot. See [`docs/boot-perf.md`](boot-perf.md) for the complete format specification.

The CI boot-performance workflow:

1. Boots each supported architecture under QEMU and captures serial output.
2. Runs `scripts/ci/parse-boot-marks.sh` to extract milestone tick counts and deltas.
3. Optionally runs `scripts/ci/boot-regression.sh` to compare against stored baselines.
4. Fails the job if any milestone is missing or if any phase exceeds its baseline by more than 20%.

Baseline files are stored in `docs/` (e.g., `boot-perf-baseline.txt`) and should only be updated via intentional `[perf-update]` commits.

To run locally:

```sh
cargo xtask smoke --arch x86_64
bash scripts/ci/parse-boot-marks.sh target/smoke-x86_64.log
bash scripts/ci/boot-regression.sh target/smoke-x86_64.log
```

## QEMU defaults

| Architecture | Machine | Firmware | Timeout | Log path |
|---|---|---|---:|---|
| x86_64 | `q35` | `OVMF_CODE` or discovered OVMF | 60s | `target/smoke-x86_64.log` |
| aarch64 | `virt` | `QEMU_EFI` or discovered AAVMF/QEMU EFI | 45s | `target/smoke-aarch64.log` |

## Adding CI coverage

1. Prefer adding an `xtask` subcommand or extending an existing one over adding
   a one-off shell command.
2. Capture serial output to a file and assert markers, not sleeps.
3. Update `docs/status.md`, `docs/milestones.md`, and this file when the gate
   changes what is considered supported.
4. For boot performance changes, update baselines only via `[perf-update]` commits after validating that regressions are intentional.

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
