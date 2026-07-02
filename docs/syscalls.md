# RustOS Syscall Correctness Matrix

_Last reviewed: 2026-07-01._

Syscall code currently lives in the **full kernel graph** and is not compiled by
the default `uefi_boot`/`boot_minimal` profile. The table below reflects the
current implementation maturity of the syscall surface, not a Linux
compatibility claim.

## Status legend

| Status | Meaning |
|---|---|
| `real` | Correct implementation with in-kernel or host-side coverage |
| `partial` | Main path exists, but semantics, edge cases, or bad-pointer handling are incomplete |
| `stub` | Routed to a fixed result or `ENOSYS`/similar placeholder |
| `planned` | Not implemented yet |

**EFAULT-safe** means every userspace pointer is validated and the syscall
returns `-EFAULT` instead of panicking or faulting the kernel. A `partial` value
means some pointer paths are guarded but the syscall has not been fully audited.

## Current syscall infrastructure

| Component | Status | Notes |
|---|---|---|
| `src/syscall/routers.rs` | partial | Routing layer exists for the full-kernel graph |
| `src/syscall/nr.rs` | partial | Linux syscall number definitions exist |
| `src/syscall/stubs.rs` | stub | Explicit placeholder routes remain part of the current state |
| `src/syscall/fault.rs` | partial | Fault-injection hooks for resource/fd-table errors |
| `src/syscall/profile.rs` | partial | Optional `syscall-trace` counters and timing hooks |
| `src/arch/x86_64/syscall.rs` | partial | x86_64 syscall entry glue exists |
| `src/arch/aarch64/syscall.rs` | partial | AArch64 syscall entry glue exists |

## Minimum init syscall set (M3 gate)

All rows in this section must become `real` and EFAULT-safe before M3 can be
closed.

| Syscall | Linux x86_64 number | Status | EFAULT-safe | Notes |
|---|---:|---|---|---|
| `read` | 0 | partial | partial | File/socket/pipe paths exist in the tree; semantics need full audit |
| `write` | 1 | partial | partial | Console/file-backed behavior is still being unified |
| `open` / `openat` | 2 / 257 | partial | partial | VFS/open helpers exist; compatibility and edge cases remain |
| `close` | 3 | partial | n/a | fd lifecycle exists but needs broader coverage |
| `fork` / `clone` | 57 / 56 | partial | n/a | Process/task creation code exists; POSIX semantics incomplete |
| `execve` | 59 | partial | partial | ELF loader path exists; argv/envp/auxv correctness remains |
| `exit` / `exit_group` | 60 / 231 | partial | n/a | Teardown paths exist; group semantics need validation |
| `wait4` | 61 | partial | partial | Wait/child collection code exists; options/status edge cases remain |

## Extended M4/M5 syscall areas

| Area | Representative syscalls | Status | Notes |
|---|---|---|---|
| Memory mapping | `mmap`, `mprotect`, `munmap`, `brk`, `mincore` | partial | VMA/mmap modules exist; file-backed and protection edge cases remain |
| File metadata/directories | `stat`, `fstat`, `getdents`, `fcntl`, `ioctl` | partial | VFS helpers exist but compatibility is incomplete |
| Polling and events | `poll`, `eventfd`, `timerfd`, `signalfd`, `inotify`, `fanotify` | partial | Modules exist; userspace ABI coverage is incomplete |
| Pipes and fd movement | `pipe`, `dup`, `dup2`, `splice`, `close_range` | partial | Implementations/scaffolding exist with incomplete semantics |
| Sockets | `socket`, `bind`, `listen`, `accept`, `connect`, UDP/TCP ops | experimental | Network/socket modules exist but are not M5-supported yet |
| Scheduling/time | `nanosleep`, futex, rusage/rlimit/time helpers | partial | Core modules exist; conformance needs tests |
| Security/namespaces | seccomp/capability/namespace-related calls | experimental | Security scaffolding exists; policy semantics are incomplete |
| io_uring | setup/enter/register and selected ops | experimental | Ring and op modules exist; not a compatibility guarantee |

## Safety invariants

1. No syscall should panic on a bad userspace pointer.
2. Malformed arguments (`fd < 0`, invalid flags, zero lengths, null pointers
   where not allowed) must return errno values instead of `unwrap()`/`expect()`.
3. `syscall-trace` must stay off by default and must not add overhead to normal
   smoke or release-boot images.
4. New syscall work should add or update `kmtest` coverage before changing a
   row to `real`.

## Adding or promoting a syscall

1. Add/update the router and implementation under `src/syscall/` or the owning
   subsystem module.
2. Add bad-pointer and malformed-argument coverage where applicable.
3. Update this matrix and `docs/status.md` in the same change.
4. Run `cargo xtask roadmap-check` and the most relevant `cargo xtask check` or
   smoke command.
