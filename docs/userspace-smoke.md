# Userspace smoke runtime contract

## Prerequisites

- Build freestanding userspace binaries for the same architecture as the kernel.
- Stage `/init`, `/bin/smoke`, and `/bench` in the initramfs.
- Provide a writable `/tmp` directory for the regular-file roundtrip.
- Implement architecture-aware child creation, execve, wait4, open/openat, read, write, lseek, close, unlinkat, and the tested errno behavior.
- Provide an ELF loader that maps PT_LOAD segments, constructs an ABI-valid initial stack, and enters userspace correctly.
- Run under the intended QEMU/firmware profile with a bounded timeout and retain serial logs and run metadata.

## Execution contract

`BOOT_INIT_EXEC` is emitted when PID 1 begins executing. `USERSPACE_SMOKE_START`
and `USERSPACE_SMOKE_PASS` are emitted by the smoke ELF at its start and only
after all smoke assertions pass. `FULL_OS_USERSPACE_OK` is emitted by init only
after the smoke child and benchmark child have each been exec'd, waited for by
their exact PID, and observed to terminate normally with status zero. The legacy
`SMOKE OK: userspace_smoke` line is preserved. Timing markers do not substitute
for these execution markers.

Every return from `execve` is failure, including a zero return. The child reports
the failure and exits nonzero. PID 1 retries `wait4` on `EINTR`, rejects a wait
result other than the exact child PID, rejects signal termination, and requires
exit status zero.

## Validation limits and outstanding integration work

The source-level smoke checks cover process creation and normal child exit on
x86_64, AArch64, and RISC-V 64, descriptor/errno behavior, and regular-file
create/write/seek/read/unlink plus `ENOENT` after unlink. These assertions prove
runtime behavior only when the workload is actually executed by the guest.

The kernel's current `src/proc/exec.rs` still returns `ENOSYS` for a valid
`execve`, and process ELF segment mapping and initial userspace stack setup are
intentionally incomplete. The full userspace QEMU gate remains blocked until
process creation, ELF replacement, exact-child reaping, and architecture-specific
return-state setup are implemented and exercised. Build success, marker string
presence, or parser unit tests alone do not establish end-to-end support. Validate
each architecture independently.
