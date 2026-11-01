# Boot performance markers

`src/boot_perf.rs` emits raw hardware-counter records, and `scripts/ci/parse-boot-marks.py` validates their sequence. At `08a14f6`, parser tests passed, but this audit did not collect a QEMU boot-time measurement.

## Format and counters

```text
BOOT_MARK label=BOOT_ENTRY ticks=12345678
```

This is a format example, not measured evidence. x86_64 reads TSC, AArch64 reads `cntvct_el0`, and RISC-V reads the `time` CSR.

The `ticks` field is an unsigned integer, not a duration in nanoseconds. A frequency/clock conversion and a comparable environment are required before reporting wall-clock time or comparing architectures.

## Parser contracts

| Mode | Required timed markers | Completion |
| --- | --- | --- |
| `minimal` | `BOOT_ENTRY`, `BOOT_MMU_ON` | One separate `BOOT_MINIMAL_OK` sentinel |
| `full` | `BOOT_ENTRY`, `BOOT_MMU_ON`, `BOOT_INITRAMFS_LOADED`, `BOOT_INIT_EXEC` | Timing validation only; execution parser must also pass |

The parser permits two initial `BOOT_ENTRY` records for the supported early-entry handoff pattern and uses the first as baseline. It rejects malformed markers, invalid ordering, inappropriate duplicate records, and minimal evidence in full mode.

The legacy minimal `BOOT_INITRAMFS_LOADED` marker is an untimed compatibility placeholder. It does not prove CPIO parsing, and the minimal `BOOT_MMU_ON` records an observed handoff point rather than necessarily enabling the MMU there.

```bash
python3 scripts/ci/test-boot-marks.py
bash scripts/ci/parse-boot-marks.sh --mode minimal target/smoke-x86_64.log
bash scripts/ci/parse-boot-marks.sh --mode full target/smoke-x86_64.log
```

Select the mode matching the actual image. Default parser mode is `full`, so minimal logs need an explicit mode.

## CI and interpretation

`boot-perf.yml` runs parser tests and minimal-profile x86_64/AArch64 boot jobs. A passing timing sequence does not establish real init execution; [userspace smoke](userspace-smoke.md) validates that separate contract.

`scripts/ci/boot-regression.sh` expects JSON containing `total_boot_ns`, while the current boot marker parser prints a text table of raw ticks and does not produce that schema. The regression script also skips when the baseline value is zero; do not describe it as a currently connected, authoritative timing regression gate.

Keep raw serial, parser output, architecture, CPU/QEMU mode, firmware, source revision, build profile, and clock conversion with any reported measurement. Compare repeated like-for-like runs rather than treating one marker delta as a performance guarantee.
