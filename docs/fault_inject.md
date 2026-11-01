# Fault-injection engine

The `fault-inject` feature exposes the atomic `FaultPoint` engine in full builds. Host tests exercise that engine, but the checked-in kernel hook wrappers and guest fault-test file are not wired into an executable end-to-end recovery gate.

## Engine API

`FaultPoint` stores an atomic skip budget and firing count and supports arming, disarming, one-shot checks, persistent checks, and inspection/reset helpers. `arm(N)` allows N successful checks before the next check injects failure; `maybe_inject!` and its persistent variant provide compile-gated call-site helpers.

The no-feature path must preserve normal execution without injected failure. An armed budget is only setup state; a test must verify a real call-site check consumed it.

## Catalogue

| Static | Intended fault site |
| --- | --- |
| `FAULT_PMM_ALLOC` | Single physical-frame allocation |
| `FAULT_PMM_CONTIGUOUS` | Contiguous physical-frame allocation |
| `FAULT_VMM_MAP` | Page mapping |
| `FAULT_VMM_REGION` | Virtual-region reservation/allocation |
| `FAULT_SYSCALL_RESOURCE` | Resource-related syscall failure |
| `FAULT_SYSCALL_FDTABLE` | Descriptor-table insertion |

These names are catalogue entries, not proof of active production call sites. `mm/pmm_fault.rs`, `mm/vmm_fault.rs`, `syscall/fault.rs`, and `fault_inject/hooks.rs` are not declared by their respective module roots in this revision.

## Host regression

```bash
rustc --edition=2021 --test --cfg 'feature="fault-inject"' \
  scripts/ci/tests/fault_engine.rs -o /tmp/rustos-fault-engine
/tmp/rustos-fault-engine
```

Seven tests passed in the audit. They verify the production atomic engine and budget behavior on the host, not PMM/VMM/syscall rollback in the running kernel.

## Guest integration state

`kmtest/fault_inject_tests.rs` exists but is not registered by `kmtest/mod.rs`. `kmtest/oom_recovery.rs` contains separate allocator-oriented tests; this is not equivalent to wiring the six fault scenarios.

The fault workflow explicitly reports the host-only boundary. Its opt-in unsupported kernel job exits with failure, and the task4 workflow applies source patches in its checkout before checking them; neither describes a working unmodified guest gate.

## Safe integration requirements

Place each injection check immediately before the production resource action and return the same error/rollback behavior as a real failure. Verify that the point fired, no partial object escaped, accounting stayed consistent, and normal work succeeded afterward.

Use [fault-recovery.md](fault-recovery.md) for the strict guest-log contract. Never replace a missing execution path with parser fixtures or a minimal-boot success marker.
