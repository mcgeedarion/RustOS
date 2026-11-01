# ADR 0002: Unified error handling

Status: retained design direction, incomplete consolidation at `08a14f6`. The source has several error families, and this record must not be read as proof that they already share one representation.

## Decision

Use typed internal errors where they preserve subsystem meaning and perform explicit conversion at syscall/protocol boundaries. Negative errno is the syscall-facing convention; internal discriminants or positive codes must not be returned without applying the intended conversion.

## Current error surfaces

| Surface | Current role |
| --- | --- |
| `src/error.rs` | Declared `KernelError`, `KernelResult`, `SyscallResult`, and conversion helpers |
| `src/core/error.rs` | Separate smaller `KernelError` and `KResult` used through `kcore`/`core` |
| `src/error_production.rs` | Additional error/context implementation not declared by the crate root |
| `scheme-api::SchemeError` | Shared scheme/protocol errors |
| `vfs-core::VfsError` | Workspace VFS errors; integer conversions use positive discriminants |
| `mm-core::MmError` | Workspace MM errors; integer conversions use positive discriminants |
| `syscall/errno.rs` | Syscall negative-error helpers/constants |
| Subsystem-local enums | ELF, usercopy, io_uring, device and other contextual errors |

These surfaces are not aliases of one completed type. In particular, converting a workspace error to an integer is not automatically the same as producing a negative syscall return.

## Required migration behavior

- **Sign tests**: Test successful values and error values at each syscall/protocol adapter.
- **Information preservation**: Retain useful context internally without leaking kernel addresses or private data to userspace.
- **No false success**: Do not replace unsupported behavior with `Ok(())`, zero, or a fabricated object merely to satisfy a caller.
- **Rollback**: Propagate errors only after restoring ownership/accounting invariants for partial operations.
- **Compatibility**: Update consumers and tests before removing a legacy error type.

The [syscall guide](../syscalls.md) describes boundary checks, and [production requirements](../production_requirements.md) explain why a uniform type alone is insufficient for reliability. Error consolidation remains source work, not a documentation-only completion.
