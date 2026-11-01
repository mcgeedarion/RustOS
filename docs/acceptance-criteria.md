# Acceptance criteria

These are executable acceptance requirements, not assertions that the current kernel meets them. The observed status at `08a14f6` is recorded in [the source audit](source-audit.md), including full-build failures and unexecuted runtime gates.

## Build and boot evidence

- **Profile identity**: Record revision, architecture, boot mode, toolchain, Cargo profile, and exact feature set. Minimal, diagnostic, and full results remain separate.
- **Full build**: Compile and link the actual full graph before accepting a full-kernel runtime claim. Slim-profile success cannot substitute for unresolved full dependencies.
- **Minimal boot**: Require the minimal timing contract and one `BOOT_MINIMAL_OK` sentinel from a single boot. This proves neither userspace execution nor full subsystem initialization.
- **Diagnostic boot**: Treat valid CPIO/ELF inspection as diagnostic coverage only. `USERSPACE_BOOT_UNSUPPORTED` is explicitly not an execution pass.
- **Artifacts**: Preserve serial output, metadata, build logs, image identity, and failure context. Missing output, skipped steps, or timeout are not success.

## Userspace and subsystem behavior

- **Process lifecycle**: Execute init and distinct smoke/bench children, validate exact-child wait status, and require the full ordered marker protocol.
- **File semantics**: Assert actual bytes and side effects across create/read/write/seek/dup/rename/unlink and closed/bad FD handling. A zero return from a simulated backend is insufficient.
- **IPC**: Assert a real pipe or endpoint payload exchange, buffer/error semantics, wakeup behavior, and cleanup. Count an emitted marker only when it follows assertions.
- **User memory**: Test unmapped, misaligned where relevant, overflowed, and permission-invalid pointers. Require error returns without kernel corruption or unintended state mutation.
- **Lifetime**: Validate exit/reap without use-after-free, with a distinct future gate for eventual reclamation. Conservative retention is safer than premature free but is not leak-free completion.
- **Architecture parity**: Demonstrate each supported architecture independently, including syscall numbers, trap return, signal behavior, and memory permissions.

## Fault recovery

Every requested injected scenario must fire in the guest, recover through the intended resource path, and run a post-recovery workload successfully. Require `fired`, `recovered`, and `post_recovery_pass ... exit=0` for that same scenario.

The supported scenario vocabulary is documented in [fault recovery](fault-recovery.md). Host engine tests, armed-but-unused budgets, and unconditional success strings cannot satisfy this gate.

## Parser and filesystem robustness

Fuzz production CPIO, ELF, path, and ext4 metadata code with bounded inputs and retained crash artifacts. Report actual production-source execution coverage rather than harness-only coverage.

Filesystem acceptance additionally requires mounted-backend semantics and relevant recovery/persistence tests. Parser acceptance alone must not promote FAT32 simulated I/O or workspace adapter skeletons into working filesystems.

## Performance

Use the schema-2 getpid benchmark with correct iteration counts, batch-average sample statistics, matching metadata, and real guest execution. Compare like-for-like architecture, image/profile, host, QEMU, compiler, and clock context.

Boot raw ticks are not nanoseconds without conversion, and disk-image capacity is not kernel payload size. No performance improvement or target threshold is considered achieved without repeatable measurements.

## Production readiness

Require declared-feature/build consistency, negative security tests, bounded resource behavior, correct device ownership, safe teardown, and reproducible runtime evidence. Workflow names, documentation keywords, source comments, and the current production-check script are not substitutes for these requirements.
