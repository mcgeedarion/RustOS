# Processes and scheduling

The full process graph lives under `src/proc/`, with syscall-facing entry points routed through `src/syscall/`. At `08a14f6`, process creation, ELF replacement, exit, and wait have implementations, but full-kernel compilation and runtime integration remain incomplete.

## Process model and table

- **`process.rs`**: PCB/process state and `ProcLock` ownership. A PCB includes identity, credentials, mappings, user context, scheduling state, and resource metadata.
- **`task_types.rs`, `context.rs`, `thread.rs`**: Scheduler task records, saved contexts, and thread-group bookkeeping. Scheduler-visible task pointers have lifetime requirements beyond PID-table membership.
- **`proc_table.rs`**: Authoritative table of `Arc<ProcLock>` objects and PID allocation. Lookup clones an owner before taking the PCB lock.
- **`process_builder.rs`**: Additional process-construction API remains alongside the exec/fork paths. It is not evidence that every creation path shares one validated implementation.
- **`pid.rs`, `task_group.rs`, `sched_helpers.rs`**: Identity/group and scheduler-helper surfaces. Compatibility aliases such as `proc::table` resolve to `proc_table`.

`with_procs_ro` clones the table's owner vector and invokes its callback after releasing the table lock. `with_procs_mut` has a stricter contract: its callback must not acquire PCB locks or re-enter process-table helpers.

## Scheduling

`scheduler.rs` contains per-CPU runqueue logic, enqueue/block/wake/yield operations, tick accounting, migration/load-balancing code, and architecture context-switch integration. Policy queues distinguish deadline/CBS, FIFO/RR, normal CFS-inspired vruntime, batch, and idle tasks.

These are implementation mechanisms, not validated fairness or latency guarantees. Full SMP behavior, raw-pointer lifetime, and scheduler handoff need runtime testing; see [synchronization](synchronization.md).

## Fork and image replacement

`fork_syscall::sys_fork` snapshots the parent, creates a distinct user address space, copies resident private pages, copies the saved return context, duplicates FD state, and prepares a child return value of zero. Physical-map VMAs are handled as borrowed mappings, and unmapped/lazy pages stay uncommitted.

This path intentionally avoids claiming universally working CoW. Other fork/CoW helpers remain in `fork.rs` and `cow_fault.rs`, so a function name alone is insufficient to identify the syscall's actual path.

`proc/exec.rs` stages a new image before installing it. Its preparation covers validated ELF load segments, page allocation/mapping, BSS initialization, argument/environment copies, a 16-byte-aligned initial stack, and auxiliary-vector entries.

The stack includes `argc`, argv pointers and terminator, envp pointers and terminator, `AT_PAGESZ`, `AT_ENTRY`, optional program-header entries, and `AT_NULL`. `enter_user_context` dispatches to x86_64 SYSRET, AArch64 ERET, or RISC-V SRET helpers.

Old exec mappings are queued in `DEFERRED_EXEC_IMAGES`, and architecture syscall dispatch invokes the deferred-image reap hook on a later syscall. This design still needs concurrency and architecture execution evidence; it is not a general all-CPU quiescence proof.

`exec/elf.rs`, `init/loader/`, `proc/dynlink.rs`, and `proc/exec.rs` are distinct parsing/loading surfaces. Interpreter-related code exists, but a complete dynamic-libc compatibility contract is not established.

## Exit and exact-child reaping

Exit cleanup releases logical resources such as process FDs, IPC ownership, driver/scheme handles, futex state, and thread/group bookkeeping. It then publishes zombie state and notifies waiters rather than immediately destroying an executing address space or stack.

- **Task retention**: `RetiredTask` keeps the PCB owner and task allocation alive after logical removal from the process table. No automatic physical-resource reclaimer is installed.
- **Stack retirement**: `free_kstack` retires backing through the stack registry. It does not immediately unmap or free the current kernel stack.
- **Identity-checked unlink**: `try_reap_zombie(parent, expected)` checks `Arc` identity, parent identity, zombie state, and detached task state under the table lock. It uses a nonblocking PCB lock attempt and requires callers to rescan on failure.
- **Wait selection**: `wait4`/`waitpid` inspect owned child snapshots and consume only the selected zombie. Signed process-group selector boundaries have a host regression.
- **Partial waitid**: `waitid` shares snapshot/reap mechanics, but its ABI layout, flags, and stopped/continued semantics remain explicitly partial.
- **Group exit**: Cross-CPU stopping and cleanup serialization for running siblings still require integration. A retained object avoids one class of use-after-free without finishing the whole exit protocol.

The focused exit host harness executes the production table, exit, and wait units with controlled substitutes. It does not exercise real context switching, interrupt stacks, or concurrent multi-CPU exits.

## Signals, credentials, and resource state

`signal.rs`, `restart.rs`, `itimer.rs`, `nanosleep.rs`, `futex.rs`, `ptrace.rs`, `creds.rs`, `cwd.rs`, `rlimit.rs`, and `rusage.rs` own the corresponding process services. Namespace and cgroup support is distributed across `proc` and `security`, with overlapping representations that should not be treated as a completed consolidation.

RISC-V syscall return deliberately does not redirect execution to a user signal handler until signal-frame construction, restorer code, and `rt_sigreturn` work as one tested path. `proc/fault_signal.rs` is not declared in `proc/mod.rs`, and `proc/clone.rs` is empty even though the syscall router names `sys_clone3`.

Use [syscalls](syscalls.md) for the architecture translation subset and [userspace smoke](userspace-smoke.md) for the acceptance protocol. Do not infer clone3, signal-frame, or libc compatibility from the presence of constants or router arms.
