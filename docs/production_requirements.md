# Production requirements

RustOS is not established as production ready by this documentation audit. The full x86_64 graph fails compilation, RISC-V slim checks fail, and no guest/hardware/security certification was performed.

## Required properties

| Area | Required evidence |
| --- | --- |
| Reproducible build | Declared, consistent profiles/features/targets and successful compile/link/package steps |
| Boot and execution | Actual firmware handoff, subsystem initialization, init/smoke/bench execution and strict logs |
| Memory/process lifetime | Correct allocation/rollback, page permissions, safe exit/reap, bounded retained resources, proved reclamation |
| Filesystems/storage | Real block I/O, correct mounted semantics, durability/recovery where claimed |
| IPC/network | Ownership, queue bounds, timeout/cancel/close semantics, actual payload exchange |
| Drivers | Documented device identity, DMA/interrupt/reset/error behavior, tested transports |
| Security | Enforced permissions/capabilities/namespaces/policy, usercopy negatives, untrusted-input tests |
| Resilience | Real guest fault firing, rollback/invariants, successful post-recovery workload |
| Diagnostics | Useful panic/error context, bounded observability overhead, no false success markers |
| Operations | Retained artifacts, supported-platform limits, known gaps, reproducible regression commands |

## Current blockers

The source includes duplicate or unregistered modules, empty implementation files, simulated FAT32 I/O, incomplete filesystem extraction, undeclared optional features, and unsupported guest fault recovery. Conservative stack/task retention prevents premature freeing but does not yet supply bounded eventual reclamation.

Exec, fork, PMM, heap, and host-test implementations have advanced beyond older descriptions. That progress must be recorded without promoting code presence into a full operating-system guarantee.

## Checker limitations

`scripts/ci/verify-production-requirements.sh` uses textual searches and computes its workspace location from one parent of `scripts/ci`, yielding `scripts/` rather than the repository root. Its current output cannot establish the requirements in this page.

Likewise, the stub and roadmap guards only check text/module-adjacent patterns. A “passed” text check says nothing about active syscall semantics, hardware behavior, persistence, or security enforcement.

## Promotion process

Use [acceptance-criteria.md](acceptance-criteria.md) to define each concrete gate and [source-audit.md](source-audit.md) to report observations. Keep unsupported features disabled or explicitly documented, and never make a production claim by deleting known failures from the status page.
