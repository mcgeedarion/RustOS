# Guest fault-recovery contract

This is the required evidence protocol for future guest fault integration, not a report of working kernel recovery. At `08a14f6`, the atomic engine is host-tested, but the production hook wrappers and guest test registration are incomplete.

## Scenario vocabulary

| Scenario | Required failure path |
| --- | --- |
| `pmm_alloc_oom_once` | One single-frame PMM allocation failure |
| `pmm_contiguous_alloc_once` | One contiguous PMM allocation failure |
| `vmm_map_once` | One page-map failure |
| `vmm_reserve_once` | One virtual-region reservation failure |
| `syscall_emfile_once` | Descriptor exhaustion/error rollback |
| `syscall_eagain_once` | Transient resource failure and recovery |

Each requested scenario needs the complete ordered chain below from the same real guest run. The snippet is a format example and must never be presented as captured test evidence.

```text
RUSTOS_FAULT marker=fired scenario=pmm_alloc_oom_once
RUSTOS_FAULT marker=recovered scenario=pmm_alloc_oom_once
RUSTOS_FAULT marker=post_recovery_pass scenario=pmm_alloc_oom_once workload=allocation_probe exit=0
```

`fired` proves the selected production check actually consumed a failure. `recovered` follows error handling and invariant/accounting checks; `post_recovery_pass` follows a later successful workload rather than merely resetting the fault budget.

## Validation command

The strict parser also requires the normal full userspace smoke/benchmark protocol. The fault flags do not create a standalone synthetic-log acceptance mode.

```bash
python3 scripts/ci/parse_smoke_log.py target/smoke-x86_64.log \
  --metadata target/smoke-x86_64-metadata.json --arch x86_64 \
  --require-fault-scenarios \
pmm_alloc_oom_once,pmm_contiguous_alloc_once,vmm_map_once,vmm_reserve_once,syscall_emfile_once,syscall_eagain_once
```

Unknown, duplicated, incomplete, or out-of-order scenario records fail validation. Host-scope logs, extra/malformed fields, nonzero workload exits, panic output, and diagnostic/minimal boot evidence are not acceptable substitutes.

## Outstanding work

- **Hook wiring**: Declare and call the production PMM/VMM/syscall wrappers at real resource boundaries.
- **Test registration**: Connect the guest fault suite to a buildable full kernel and an actual runtime harness.
- **Rollback checks**: Validate page/region/FD accounting, object ownership, error values, and cleanup after partial progress.
- **Recovery workload**: Demonstrate continued allocation, mapping, process, filesystem, and IPC activity after injection.
- **Artifacts**: Retain serial, metadata, fault configuration, assertion output, and exact source revision.

The current source cannot be described as passing this gate. See [fault_inject.md](fault_inject.md) for the narrower engine implementation and [source-audit.md](source-audit.md) for executed tests.
