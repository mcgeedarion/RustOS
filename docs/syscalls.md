# System calls

The syscall implementation is a Linux-shaped subset with explicit architecture translation and additional RustOS interfaces. This guide describes routing at `08a14f6`; a syscall number, constant, or match arm is not proof that the target implementation compiles or has complete Linux semantics.

## Entry and routing

```text
architecture syscall/trap entry
  saved user context
  syscall::dispatch_arch(SyscallAbi, number, arguments, return_pc)
    architecture-number translation
    shared dispatch context and policy
      filesystem / process / memory / IPC / misc routers
      selected direct compatibility handlers
```

`src/syscall/mod.rs` contains `SyscallAbi`, `dispatch_arch`, canonical dispatch, and many implementation helpers. `dispatcher_context.rs`, `driver.rs`, `errno.rs`, `nr.rs`, `profile.rs`, `routers.rs`, `scheme.rs`, `signal_nr.rs`, and feature-gated `kmtest.rs` are declared submodules.

Four additional files are textually compiled into the parent with `include!`: `p0_gaps.rs`, `openat2_mincore.rs`, `stubs.rs`, and `posix_full.rs`. They are active source fragments, not independent module namespaces, and their functions participate in the parent dispatch implementation.

The current tree also contains undeclared alternatives such as `syscall/fs.rs`, `net.rs`, `sched.rs`, `fault.rs`, and `musl_compat.rs`. Do not include their functions in a supported-API list unless they are actually wired through the module graph and router.

## Architecture translation

x86_64 numbers pass directly to canonical dispatch. AArch64 and RISC-V share an explicit translation subset; unlisted numbers return `-38` (`ENOSYS`) rather than falling through to an unrelated x86_64 operation.

| AArch64/RISC-V number | Canonical number | Operation |
| --- | --- | --- |
| 23 / 24 / 25 | 32 / 292 / 72 | dup / dup3 / fcntl |
| 29 | 16 | ioctl |
| 35 / 38 | 263 / 264 | unlinkat / renameat |
| 56 / 57 / 59 | 257 / 3 / 293 | openat / close / pipe2 |
| 62 / 63 / 64 | 8 / 0 / 1 | lseek / read / write |
| 65 / 66 | 19 / 20 | readv / writev |
| 67 / 68 | 17 / 18 | pread64 / pwrite64 |
| 82 / 83 | 74 / 75 | fsync / fdatasync |
| 93 / 94 | 60 / 231 | exit / exit_group |
| 101 / 113 / 124 | 35 / 228 / 24 | nanosleep / clock_gettime / sched_yield |
| 134 / 135 | 13 / 14 | rt_sigaction / rt_sigprocmask |
| 160 / 172 / 173 | 63 / 39 / 110 | uname / getpid / getppid |
| 214 / 215 | 12 / 11 | brk / munmap |
| 220 | 57 | Fork-compatible clone subset |
| 221 / 222 / 226 | 59 / 9 / 10 | execve / mmap / mprotect |
| 260 / 278 / 291 | 61 / 318 / 332 | wait4 / getrandom / statx |
| `0x8000_0000` / `0x8000_0001` | Same | Private kmtest interfaces |

For number 220, dispatch replaces architecture-specific clone arguments with zeros before the current fork-compatible route. This is not general `clone`, thread creation, or `clone3` support.

## Subsystem ownership

- **Filesystem router**: Read/write/stat/path operations, descriptor duplication, pipes, polling, timer/event/signal FDs, inotify/fanotify, sockets, io_uring, and related controls. Actual backing code spans `fs`, `net`, and `io_uring`.
- **Process router**: Identity, fork/exec, exit/wait, signals, scheduling, credentials, limits, ptrace, namespaces and related calls. The `clone3` arm refers into an empty `proc/clone.rs`.
- **Memory router**: mmap, protection/unmap, brk and related VM controls. Mapping and usercopy semantics are architecture-sensitive.
- **IPC router**: SysV/POSIX queue, semaphore, and shared-memory operations. Userspace-driver endpoint syscalls additionally live in IPC/device glue.
- **Misc/time helpers**: Clocks, uname, random data, resource/compatibility interfaces, and selected extended calls. Accepted no-ops and explicit unsupported paths must be documented individually.

Seccomp policy and optional profiling surround dispatch. `dispatch_arch` also invokes deferred exec-image cleanup before servicing the next call.

## User-memory and descriptor rules

Use `kernel/uaccess.rs` for range validation and copying rather than turning arbitrary user addresses into Rust slices. Validate array-length multiplication, string bounds, pointer-plus-size overflow, read/write permissions, and errors before changing kernel state.

Translate process-visible FDs through `fs/process_fd.rs` before using backing stores. A successful `open`, `dup`, or `close` must preserve the correct shared object, open flags, offset, and lifetime rather than merely returning a plausible integer.

EFAULT-safe handling is a required property, not a blanket certification of every routed syscall. Negative errno is the syscall boundary convention; the tree has several internal error families and conversion layers, as described in [ADR 0002](adr/0002-unified-error-handling.md).

## Maturity and tests

The full x86_64 compile check fails in the audited revision, and RISC-V slim checks fail independently. RISC-V user signal-handler delivery is intentionally deferred until its signal-frame and return protocol is complete.

Guest smoke assertions provide a concrete subset contract; they are not a broad POSIX/Linux conformance suite. Host parser tests, hosted diagnostic tests, and per-file unit harnesses have different coverage and must remain separately reported in [coverage](code_coverage.md).
