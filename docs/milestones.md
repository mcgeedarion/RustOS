# RustOS Product Milestones

_Last reviewed: 2026-10-13._

The project is working toward **M3**, but the end-to-end full-userspace gate
is not yet supported. The default `full_kernel` feature selects the normal
MM/VFS/process/syscall graph and UEFI entry without implying that process
creation or image replacement is functional. At review time,
`src/proc/exec.rs::spawn_user_process()` returns false and valid
`execve()` requests return `ENOSYS`. The full smoke path must fail until this
is replaced by a real executable process path.

| Milestone | Goal | Cargo feature path | CI/local sentinel | Current state |
|---|---|---|---|---|
| **M1** | Minimal UEFI boot reaches its explicit minimal completion marker | `boot_minimal` | `BOOT_MINIMAL_OK`, validated against the actual QEMU log | **Separate profile gate; do not infer full-kernel readiness** |
| **M2** | Build an initramfs, launch `/init`, exec `/smoke`, reap that exact child and validate exit status zero | default `full_kernel` graph with initramfs | Complete `RUSTOS_EXEC` protocol plus strict timing parser on the same QEMU log | **Blocked** until the kernel provides real spawn/exec semantics |
| **M3** | Minimum init syscalls have working semantics and bad-pointer safety | full kernel graph and architecture-aware dispatch | init exercises `open/fork/exec/wait/exit`; no `ENOSYS` for required operations; child exits normally with status zero | **Blocked** |
| **M4** | VFS/initramfs/devfs/procfs support real init workloads | full kernel graph | successful writable-file and IPC workloads plus `/dev/null` and `/proc/version` checks | **Not promoted** |
| **M5** | Network, display, and input demos run as userspace processes | full kernel graph plus optional device features | an executable demo completes under QEMU with retained logs and metadata | **Experimental/planned** |

## Marker contracts

Boot timing markers and userspace execution markers are separate contracts.
The timing parser validates `BOOT_ENTRY`, `BOOT_MMU_ON`, and, for the full path,
`BOOT_INITRAMFS_LOADED` and `BOOT_INIT_EXEC`. Those timestamps do not prove that
a userspace child ran successfully.

The minimal boot contract accepts only the explicit `BOOT_MINIMAL_OK` sentinel.
The generic idle-loop text `entering cpu_idle` is not a success condition.

The full execution contract uses these serial markers:

```text
RUSTOS_EXEC marker=init_started
RUSTOS_EXEC marker=smoke_started
RUSTOS_EXEC marker=smoke_tests_passed
RUSTOS_EXEC marker=smoke_child_reaped pid=<pid> exit=0
RUSTOS_EXEC marker=init_complete exit=0
```

The last two markers are emitted only after `execve("/smoke")` succeeds, the
parent waits for that exact child's PID, the child terminates normally, and its
exit code is zero. An exec failure, failed wait, signal termination, or
nonzero child status must not emit the completion markers. `SMOKE OK` and
`FULL_OS_USERSPACE_OK` are downstream aliases emitted only after the same
checks. Timing markers never substitute for this execution protocol.

`cargo xtask smoke` truncates stale logs, enforces a QEMU deadline, applies
the appropriate boot-timing parser to the actual generated QEMU serial log, and
writes `target/smoke-<arch>-metadata.json` alongside that log. A successful
parser unit test or a source-level marker search does not satisfy either
runtime contract.

See `docs/M3_M5_ROADMAP.md` for detailed milestone gates and validation commands.

## Promotion rule

Do not mark a milestone complete unless:

1. `docs/status.md` and `docs/syscalls.md` agree with the code state.
2. The relevant `cargo xtask smoke` or `cargo xtask check` command succeeds.
3. Any claimed performance or image-size improvement has a checked-in
   measurement or a documented command to reproduce it.
