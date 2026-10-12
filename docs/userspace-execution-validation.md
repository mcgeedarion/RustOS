# Userspace execution validation: first implementation commit

This commit applies the userspace portion of the execution-boundary draft.
It is not the entire 17-file draft and does not establish that RustOS boots
or executes these programs. Rust compilation and QEMU validation have not
been run for this commit.

## Source changes

- Share architecture-specific syscall wrappers and naked ELF entry stubs.
- Init executes /bin/smoke in a child. Any execve return exits 127; init waits
  for that exact PID and requires status zero before emitting completion.
- Smoke requires process creation/exit on every architecture, descriptor and
  errno assertions, and a regular-file create/write/seek/read/unlink roundtrip.
- Keep legacy SMOKE OK lines and add standalone execution markers.

Required ordered execution lines:

```
BOOT_INIT_EXEC
USERSPACE_SMOKE_START
USERSPACE_SMOKE_PASS
FULL_OS_USERSPACE_OK
```

BOOT_INIT_EXEC is emitted by init after entering userspace. Kernel BOOT_MARK
records remain untouched and must not be counted as standalone execution
lines. A trusted runner must reject missing, duplicate or out-of-order lines,
failed assertions, panic/fault output, timeouts and unexpected init exits.
FULL_OS_USERSPACE_OK follows normal, zero-status termination of the smoke
child, but the runner must still observe successful init termination.

## Runtime prerequisites and outstanding work

Package the smoke ELF as /bin/smoke. Use a fresh writable root with no existing
/rustos-smoke.tmp; O_EXCL intentionally rejects a stale or pre-existing file.
The suite never treats missing support or a failed open as a skip.

This commit does not change firmware/minimal feature selection, common kernel
syscall dispatch, the AArch64 SVC return-PC handling, or xtask. Those remain
separate kernel integration changes. The current diagnostic boot profile
cannot execute these programs, and native generic syscall numbers still need
the explicit kernel dispatch boundary before AArch64/RISC-V execution can be
claimed. RISC-V gp initialization or disabled gp relaxation must also be
audited before its startup stub is accepted as supported.

Builds, architecture disassembly, host regression tests and real full-kernel
QEMU runs are required. No fuzzing campaign, arbitrary-fault recovery,
filesystem/IPC compatibility promotion or syscall benchmark is claimed here.
