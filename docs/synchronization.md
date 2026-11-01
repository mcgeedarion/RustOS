# Synchronization

RustOS has kernel-local synchronization primitives, the external `spin` dependency, and a portable `sync-primitives` workspace crate. Their APIs and test coverage differ, so this guide names each surface explicitly rather than implying one finished replacement.

## Implementations

| Surface | Current code |
| --- | --- |
| `src/sync/spinlock.rs` | `SpinLock`, `IrqSpinLock`, guards, interrupt-save/restore helpers |
| `src/sync/wait_queue.rs` | Readiness masks, waiters, wake forwarding, deadlines/cancellation, `CancellationToken` |
| `src/sync/poll_source.rs` | `PollSource`, wait helpers, always-ready adapter |
| `src/sync/semaphore.rs` | Counted acquire/release operations |
| `src/sync/futex.rs` | Futex wait/wake/requeue and cleanup surface |
| `src/proc/futex.rs` | Separate syscall/process futex and robust-list implementation |
| `src/sync/mutex.rs`, `rwlock.rs` | Empty files; their declarations do not provide working custom locks |
| `src/sync/advanced.rs` | Additional ticket/RW/RCU implementation, not declared by `sync/mod.rs` |
| `crates/sync-primitives/src/lib.rs` | Portable queued spinlock, RCU and RW primitives; eight host tests passed |
| `spin::Mutex`, `spin::RwLock`, `spin::Once` | External primitives used directly throughout the kernel |

## Locking and lifetime rules in current code

- **Heap/PMM locks**: The global allocator and PMM combine interrupt-state guards with internal serialization. Atomic free-list fields are manipulated under locks; they are not evidence of a lock-free allocator.
- **Process-table reads**: `with_procs_ro` takes an owned snapshot before invoking callbacks. PCB locks must not be acquired while blocking under the global table lock.
- **Zombie unlink**: `try_reap_zombie` uses `try_lock` for PCB inspection under the table lock, then drops the removed owner outside the critical section. Callers retry rather than assuming the PID still denotes the same object.
- **Kernel-stack ownership**: Generation tokens prevent stale handles from retiring a reused slot. Retired frames are not released until an external quiescence condition is established.
- **Wait/wake state**: Readiness bits, queue registration, rechecking readiness, and cancellation must be considered together. A queue wake alone does not prove the underlying object is still valid.
- **RCU**: Pointer publication and a grace-period interface are different from proving that all readers have quiesced. Do not use a timing delay or an unrelated atomic counter as a substitute for a lifetime protocol.

## Per-CPU and SMP state

`smp/percpu.rs` and `core/cpu_local.rs` both provide CPU-local facilities, with architecture-specific register bases underneath. Boot-time code cannot assume per-CPU state is installed before GDT/CPU-local initialization; the PMM explicitly avoids that dependency during bootstrap.

`smp/ipi.rs` provides reschedule, shootdown, and halt-related paths, while `smp/hotplug.rs` manages CPU-state transitions and callbacks. Source implementations do not demonstrate completed cross-CPU task/stack reclamation.

## Review and testing

Every shared object needs a documented owner, lock order, publication operation, lifetime rule, and teardown condition. Use [atomic ordering guidelines](ATOMIC_ORDERING_GUIDELINES.md) for review questions, not as a claim that every atomic in the tree has been verified.

Portable synchronization tests and the focused heap/stack/exit tests passed in the audit. They do not replace architecture-specific interrupt, migration, IPI, and scheduler stress tests, which remain separate runtime work.
