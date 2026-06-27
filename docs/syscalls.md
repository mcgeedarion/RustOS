# RustOS Syscall Correctness Matrix

This table tracks every syscall by its Linux x86_64 number, current
implementation status, test coverage, and whether it safely returns `-EFAULT`
on bad user pointers rather than panicking.

**Status legend**

| Status | Meaning |
|--------|---------|
| `real` | Correct implementation, in-kernel test exists |
| `partial` | Happy path works; error paths or edge cases missing |
| `stub` | Returns `ENOSYS` (or a fixed success value) |
| `planned` | Tracked; no code yet |

**EFAULT-safe**: `✓` means the syscall returns `-EFAULT` on any bad userspace
pointer rather than panicking or triple-faulting.

---

## Minimum init syscall set (M3 gate)

These seven syscalls are required before `/init` can run a real workload.
All must be `real` and `EFAULT-safe` before M3 is closed.

| Syscall | Number | Status | In-kernel test | EFAULT-safe | Notes |
|---------|--------|--------|---------------|-------------|-------|
| `write` | 1 | partial | ✗ | ✗ | Console fd 1/2 only; no file-backed |
| `open` | 2 | stub | ✗ | ✗ | Returns fd 0-2 only |
| `close` | 3 | stub | ✗ | ✗ | — |
| `fork` | 57 | stub | ✗ | ✗ | Returns ENOSYS |
| `execve` | 59 | partial | ✗ | ✗ | ELF loader wired; arg passing incomplete |
| `exit` | 60 | partial | ✗ | n/a | Process teardown incomplete |
| `wait4` | 61 | stub | ✗ | ✗ | Returns ECHILD |

## Extended syscall set (M4 gate)

| Syscall | Number | Status | In-kernel test | EFAULT-safe | Notes |
|---------|--------|--------|---------------|-------------|-------|
| `read` | 0 | partial | ✗ | ✗ | — |
| `mmap` | 9 | partial | ✗ | ✗ | Anonymous only |
| `mprotect` | 10 | stub | ✗ | ✗ | — |
| `munmap` | 11 | partial | ✗ | ✗ | — |
| `brk` | 12 | partial | ✗ | n/a | — |
| `ioctl` | 16 | stub | ✗ | ✗ | Returns ENOTTY |
| `pipe` | 22 | stub | ✗ | ✗ | Returns ENOSYS |
| `dup` / `dup2` | 32/33 | stub | ✗ | ✗ | — |
| `getpid` | 39 | stub | ✗ | n/a | Returns 1 |
| `socket` | 41 | stub | ✗ | ✗ | Returns ENOSYS |
| `exit_group` | 231 | partial | ✗ | n/a | — |

---

## Safety invariants

1. **No panic on bad user pointer.** Every syscall that reads from or writes
   to a userspace address must validate it through the architecture's user-access
   helpers and return `-EFAULT` on failure.  
   Tracking issue: add `validate_user_ptr()` helper and audit all syscalls.

2. **No panic on malformed arguments.** Syscalls must handle `fd < 0`,
   `len == 0`, `NULL` pointers, and out-of-range flags without calling
   `unwrap()` or `expect()`.

3. **`syscall-trace` feature only in profiling builds.** The per-syscall
   counter and cycle-timing path is `#[cfg(feature = "syscall-trace")]` only;
   release and smoke builds must not pay the overhead.

---

## Adding a new syscall

1. Add the dispatch arm in `src/syscall/dispatch.rs`.
2. Add an in-kernel test in `src/kmtest/syscall_<name>.rs`.
3. Mark it `real` in this table only after the test passes in CI.
4. Confirm EFAULT-safe by adding a bad-pointer test case.
