# Atomic ordering guidelines

This is a review policy for RustOS's shared state, not a claim that every atomic operation has been proved correct. The current code combines atomics, interrupt masking, locks, per-CPU state, and explicit object retention.

## Review the protocol before the ordering

- **Ownership**: Identify who allocates, publishes, reads, mutates, retires, and frees the object.
- **Concurrency**: List CPUs, interrupt contexts, scheduler callbacks, and user-triggered operations that can race.
- **Synchronization**: Specify the lock or atomic operation that establishes visibility of each non-atomic field.
- **Lifetime**: Define how a reader keeps storage alive and how retirement waits for every possible reader.
- **Failure**: Check rollback, retry, spurious failure, saturation/wraparound, and cancellation.

Changing `Relaxed` to `SeqCst` does not by itself fix a dangling pointer, a missing ownership reference, or an invalid lock order. Interrupt masking on one CPU does not stop other CPUs.

## Ordering vocabulary

| Ordering | Appropriate review question |
| --- | --- |
| Relaxed | Is only this atomic value needed, with no associated-data publication requirement? |
| Release | Which preceding initialization or mutation is being published? |
| Acquire | Which matching publication makes the associated data visible? |
| AcqRel | Does this read-modify-write both consume previous state and publish new state? |
| SeqCst | Is a single global order actually required, and are ownership/lifetime still correct? |

Use a matching release/acquire protocol when publishing associated data and explain it at the call site. For compare-exchange, reason separately about success and failure ordering and retry behavior.

## Current code examples

- **Heap/PMM**: Internal locks serialize allocator state while interrupt guards prevent same-CPU re-entry. Atomic fields under that protocol do not make allocation lock-free.
- **SMP startup**: `AP_GO` and online-CPU counters participate in a release/acquire handoff. A CPU count is not a grace-period proof for retired task memory.
- **Fault budgets**: `FaultPoint` uses armed/budget/trigger atomics with one-shot/persistent semantics. Host tests cover specific behavior, not every concurrent rearming interleaving.
- **Profiling**: Independent relaxed counts and tick totals are diagnostic snapshots, not transactionally consistent observations.
- **Stack ownership**: Generation tokens reject stale slot identity, while backing remains retained until an explicit stopped-machine operation.
- **Process reaping**: Owned snapshots and identity checks solve lifetime and ABA-like PID concerns that stronger ordering alone cannot solve.

## Unsafe-code review

State the pointer validity, alignment, aliasing, mapping, lifetime, and synchronization obligations near each unsafe operation. Check generated architecture transitions and interrupt state as part of the protocol rather than treating Rust atomics as a substitute for hardware barriers or TLB invalidation.

Require focused regression tests and real multi-CPU/interrupt stress where relevant. The [synchronization guide](synchronization.md) and [process-lifetime guide](processes-and-scheduling.md) identify the current uncompleted reclamation boundary.
