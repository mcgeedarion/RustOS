# Filesystems and storage

RustOS contains a kernel VFS, a kernel scheme namespace, per-process descriptor tables, and separate workspace filesystem interfaces. This guide describes their distinct roles at `08a14f6`; none should be collapsed into a claim that all listed filesystems are mounted, persistent, or runtime-validated.

## Namespaces and descriptor flow

```text
process FD
  fs/process_fd.rs
    backing FD
      scheme_fd.rs -> Scheme + FileId
      VFS/raw or special descriptor handling
        backend filesystem / pipe / socket / timer / device
```

- **Process table**: `process_fd.rs` owns process-local descriptor allocation, duplication, close-on-exec, nonblocking flags, fork copying, and cleanup. Closing a user FD must translate and release its backing exactly once.
- **Scheme namespace**: `scheme_table.rs` defines the kernel `Scheme` trait and the `SCHEME_TABLE` registry. URL-like names select a scheme and pass the remaining path to it.
- **Backing store**: `scheme_fd.rs` associates scheme-local file IDs with shared scheme objects and allocates/recycles backing descriptors.
- **Path VFS**: `vfs/ops.rs`, `vfs/fd.rs`, and `mount.rs` route path operations and mount dispatch. `vfs_ops` is a compatibility re-export, not a separate filesystem.
- **Workspace registry**: `vfs-core::VFS_REGISTRY` stores `FileSystem` providers. It is independent of `SCHEME_TABLE` and does not automatically replace kernel path routing.

## Paths, permissions, and operations

The VFS subtree separates open preflight, DAC permission checks, hard-link and rename preconditions, unlink lifetime tracking, and symlink traversal. `path.rs`, `stat_syscalls.rs`, `io_syscalls.rs`, `fcntl.rs`, and `getdents.rs` provide additional syscall, path, descriptor, and directory-iteration glue.

`vfs/inode_ref.rs` tracks open references so unlink can defer inode deletion until open handles are gone. The pure preflight helpers have tests, but the standalone `vfs/tests/posix_vfs.rs` suite is not declared by the current VFS root and should not be counted as automatically executed.

Advanced rename flags still have explicit unsupported cases. A passing path resolver or permission test does not establish crash consistency, mounted-backend semantics, or full POSIX behavior.

## Backend state

| Backend/source | Implementation present | Limitation to retain in documentation |
| --- | --- | --- |
| `fs/initramfs.rs`, `init/initramfs/mod.rs` | CPIO parsing and filesystem installation/read paths | Raw archive reader and VFS materialization are different layers; symlink materialization has an undeclared `vfs_symlink` gate |
| `fs/ramfs.rs` | In-memory files, directories, links, metadata, anonymous objects, scheme adapter | RAM-backed storage, not disk persistence |
| `fs/tmpfs.rs` | Per-mount accounting/size limit, files/directories/links, scheme adapter | Not the same as workspace `fs-tmpfs`; no proven swap-backed tmpfs contract |
| `fs/ext2/` | Block-backed filesystem operations, metadata, allocation, directory, and symlink interfaces | Requires a working block path and runtime validation |
| `fs/ext4.rs`, `fs/jbd2.rs` | Metadata parsing, filesystem operations, journal validation/replay helpers | Journal code includes simplifying assumptions and integration errors; not a proven crash-consistent filesystem |
| `fs/fat32.rs` | BPB/FAT/directory and operation logic | `blk_read` zero-fills and `blk_write` silently succeeds without actual block I/O |
| `fs/btrfs/` | Tree, extent, inode, directory, subvolume, transaction, space and metadata code | Source implementation only; no audited mounted-filesystem or recovery pass |
| `fs/overlayfs.rs` | Lower/upper routing and copy-up-related operations | Backend semantics and advanced rename support need validation |
| `fs/nfs.rs` | Network-filesystem client and scheme interfaces | Depends on the unresolved network stack; no remote interoperability claim |
| `procfs`, `sysfs`, `devfs`, `cgroupfs`, `procfs_binfmt`, `proc_debug` | Synthetic filesystem interfaces and control/debug paths | Access control and each individual path must be reviewed; not general device/filesystem support |

## Special descriptors

`pipe`, `eventfd`, `epoll`, `signalfd`, `timerfd`, `pidfd`, `memfd`, `inotify`, `flock`, `splice`, `close_range`, `shm`, and io_uring FD adapters are implemented as separate units. Poll/read/write/close routing must respect each descriptor namespace and wakeup source.

`fs/timerfd.rs` and `time/timerfd.rs` are different implementations, and both remain in the tree. Treat the syscall router as the authority for which path serves a call rather than assuming similarly named modules are aliases.

## Boot scheme registration

`init/schemes.rs` registers `blk`, `file`, `tty`, `dev`, `proc`, `sys`, `ram`, `tmp`, `net`, `tcp`, `udp`, `nfs`, `ipc`, `pipe`, `cgroup`, and `overlay`. It places lower-level and namespace providers before dependent schemes.

Registration is source wiring, not evidence that an underlying device or service is operational. In particular, network schemes inherit the socket integration problem, and the IPC proxy requires a real endpoint/server lifecycle.

## Block layer and workspace adapters

`block/mod.rs` provides the block scheme and device interfaces; `block/virtio_blk.rs` is the raw VirtIO block implementation. `drivers/virtio_blk.rs` is a compatibility shim, while `drivers/block/` contains multi-sector helpers and AHCI/NVMe implementations.

The workspace `fs-ext4` and `fs-fat32` crates retain `kernel_impl` conditionals and fallback behavior instead of providing an automatically linked replacement for the kernel backends. `fs-tmpfs` has an interface skeleton; its name must not be confused with the substantial kernel `fs/tmpfs.rs`.

The guest smoke workload validates a regular-file write/read/rename/unlink sequence and a pipe payload round trip. Those assertions exist in source, but the full guest gate remains unproven and the current host smoke mocks fail; see [runtime smoke](userspace-smoke.md).
