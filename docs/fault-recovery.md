# Fault-recovery validation catalog

This catalog is the acceptance contract for future guest fault-integration work.
The current fault-injection CI job tests the atomic one-shot engine on the host.
It does not exercise PMM, VMM, syscall, or process recovery inside a booted kernel.
The kernel-integration workflow must remain disabled until an executable guest
harness can inject each scenario, observe fired and recovered markers, and prove
that follow-up workloads succeed.

## Scenario catalog

| Scenario | Intended injection | Expected recovery | Required guest follow-up |
|---|---|---|---|
| pmm_alloc_oom_once | Fail one physical-page allocation | Return allocation failure without corrupting allocator metadata | Allocate and free a fresh page; run a small userspace workload |
| pmm_contiguous_alloc_once | Fail one contiguous allocation request | Return a bounded allocation error and restore temporary state | Retry a smaller allocation and free it |
| vmm_map_once | Fail one page-table mapping | Unwind allocated tables/frames and report mapping failure | Map a different page, read/write it, then unmap |
| vmm_reserve_once | Fail one virtual-region reservation | Leave VMA metadata unchanged | Reserve and release a valid region |
| syscall_emfile_once | Make one file-descriptor allocation fail with EMFILE | Preserve existing descriptor state | Close a descriptor and verify open/read/write/close succeeds |
| syscall_eagain_once | Return one transient EAGAIN from a retryable operation | Leave queue and process state consistent | Retry IPC send/receive and verify payload equality |

## Marker protocol

Guest integration must emit exact standalone marker records with a stable scenario
identifier. A fired marker must only be printed when the corresponding production
fault site actually injects the failure. A recovered marker must only be printed
after the affected subsystem completes its cleanup/recovery path.

~~~~text
RUSTOS_FAULT marker=fired scenario=<scenario>
RUSTOS_FAULT marker=recovered scenario=<scenario>
RUSTOS_FAULT marker=post_recovery_pass scenario=<scenario> workload=<name> exit=0
~~~~

The post-recovery marker requires a real workload result, not merely a return from
the fault-injection helper. The guest harness must fail on a missing marker, a
kernel panic, a nonzero workload exit, or a result from the wrong scenario.

## Current evidence boundary

scripts/ci/tests/fault_engine.rs emits scope=host_engine fired/recovered records
to make host test logs searchable. Those records verify one-shot engine behavior
only. They are deliberately not interchangeable with the guest marker protocol
above.

.github/workflows/fault-inject.yml keeps kernel integration behind the unsupported
readiness guard. Do not enable that job until production callsites exist for the
catalogued failures and the real guest harness retains QEMU logs, metadata, and
post-recovery workload results.
