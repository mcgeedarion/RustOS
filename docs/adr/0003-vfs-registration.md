# ADR 0003: VFS and scheme registration

Status: retained registration direction with multiple coexisting registries at `08a14f6`. This record names the actual systems so callers do not confuse registration with mounted or operational support.

## Decision and context

Register providers explicitly during initialization instead of requiring each caller to know a concrete backend implementation. Keep protocol identifiers, process descriptors, backing handles, and mount routing distinct.

## Current registries

- **Kernel schemes**: `src/fs/scheme_table.rs` owns the kernel `Scheme` trait and `SCHEME_TABLE`. `src/init/schemes.rs` installs boot providers in a dependency-conscious sequence.
- **Scheme backing FDs**: `src/fs/scheme_fd.rs` associates shared scheme providers with local file IDs. Process-visible FDs are a separate layer.
- **Kernel VFS/mounts**: `src/fs/vfs/` and `src/fs/mount.rs` provide path and mount routing.
- **Workspace VFS**: `crates/vfs-core/src/lib.rs` defines `VFS_REGISTRY` and filesystem-provider registration. It is not the same registry as `SCHEME_TABLE`.
- **IPC proxy**: `src/fs/ipc_proxy_scheme.rs` forwards operations through a userspace-service endpoint. Registering a proxy does not launch or supervise the server.

The workspace filesystem adapters do not automatically replace kernel VFS routing or boot scheme initialization. Their probe/registration tests are narrower than a mounted filesystem integration test.

## Required invariants

Registration must have a documented name/identity, provider lifetime, duplicate-registration rule, and initialization order. Process exit or server failure must not leave stale endpoint/FD ownership behind.

Unregistering or replacing a provider requires a policy for open handles, queued operations, and blocked waiters. A successful registration call alone cannot establish those lifetime semantics.

See [filesystems and storage](../filesystems-and-storage.md), [IPC](../ipc-and-async-io.md), and [process lifetime](../processes-and-scheduling.md) for the active integration boundaries. Registry consolidation remains explicit future work rather than an assumed completed migration.
