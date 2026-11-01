# Milestones

Milestones describe acceptance gates, not completion inferred from source size or previous roadmap labels. This status follows the `08a14f6` source audit and its executed checks.

## Current milestone state

| Milestone | Contract | Current evidence |
| --- | --- | --- |
| M1: boot and profile discipline | Reproducible target/profile builds, correct handoff and minimal marker protocol | x86_64/AArch64 slim checks pass; RISC-V slim checks fail; no local QEMU run in this audit |
| M2: real userspace lifecycle | Init creation, user transition, distinct smoke/bench execution, exact-child waits, `FULL_OS_USERSPACE_OK` | Exec/fork/wait source and x86_64 archive build exist; full kernel fails compilation |
| M3: reliable memory/process/VFS/IPC core | Correct error paths, descriptor and mapping semantics, bounded ownership/lifetime, filesystem and pipe assertions | Focused host heap/PMM/stack/exit tests pass; guest integration and eventual reclamation remain open |
| M4: device/services and observability | Validated drivers, service startup/restart, network/display capability, usable diagnostics | Driver/service/debug code exists, but multiple integration gaps and undeclared feature gates remain |
| M5: resilience and measured readiness | Guest fault recovery, security negatives, persistence/recovery, reproducible performance | Host engine/parser tests pass; no full guest recovery or production-readiness claim |

## Sentinel rules

`BOOT_MINIMAL_OK` belongs only to M1's minimal contract. `USERSPACE_BOOT_UNSUPPORTED` records diagnostic-only ELF inspection and must never satisfy M2.

`FULL_OS_USERSPACE_OK` is meaningful only after the complete ordered userspace protocol, actual assertions, exact distinct child reaping, and valid benchmark record. A final string alone cannot promote a milestone.

## Immediate sequencing

- **Build consistency**: Resolve duplicate socket ownership, missing/empty integration targets, unresolved full-build imports/types, and the RISC-V layout constant.
- **Test consistency**: Repair stale smoke mocks without weakening guest assertions; reconcile aggregate userspace workspace metadata.
- **Runtime gate**: Prove full x86_64 init/smoke/bench execution before claiming architecture parity.
- **Lifetime and failure**: Add real multi-CPU quiescence/reclamation and production fault-site recovery tests.
- **Service/device validation**: Establish specific hardware or virtual-device workloads and isolation properties.

See [M3_M5_ROADMAP.md](M3_M5_ROADMAP.md) for the remaining work and [acceptance criteria](acceptance-criteria.md) for promotion evidence. No deadline or completion percentage is implied by this ordering.
