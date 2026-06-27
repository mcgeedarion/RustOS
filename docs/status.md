# RustOS Subsystem Status

This file is the authoritative registry of every subsystem's implementation
state. Update it alongside any code change that promotes or demotes a module.

**Legend**

| Status | Meaning |
|--------|---------|
| `real` | Implemented, tested, and CI-gated |
| `partial` | Core path works; edge cases / error paths incomplete |
| `stub` | API surface exists; returns fixed values or `ENOSYS` |
| `experimental` | Works in isolation; not yet integrated or CI-gated |
| `planned` | Tracked; no code yet |

---

## Boot & Init

| Module | Status | Feature gate | Notes |
|--------|--------|--------------|-------|
| `src/boot_minimal.rs` | real | `boot_minimal` | Emits `BOOT_MINIMAL_OK` sentinel |
| `src/userspace_boot.rs` | partial | `userspace_boot` | Thin shims while mm/fs/proc stabilise |
| `src/kernel_main.rs` | partial | _(default)_ | Full init sequence not yet complete |
| `src/firmware/` | partial | _(default)_ | UEFI + SBI boot paths; multiboot2 stub |
| `src/init/` | partial | `userspace_boot` | Kernel-side init launcher |

## Memory Management

| Module | Status | Feature gate | Notes |
|--------|--------|--------------|-------|
| `src/mm/` PMM | real | _(default)_ | Bitmap allocator; fault-inject gated |
| `src/mm/` VMM | partial | _(default)_ | 4-level paging x86_64; aarch64 partial |
| `src/mm/` slab | partial | _(default)_ | Basic slab; no per-CPU caches yet |
| `src/mm/` COW | stub | `full-kernel` | `copy_on_write()` returns `ENOSYS` |

## Syscall

| Module | Status | Feature gate | Notes |
|--------|--------|--------------|-------|
| `src/syscall/` dispatch | partial | _(default)_ | x86_64 SYSCALL/SYSRET; EFAULT not uniform |
| `write` (fd 1/2) | partial | _(default)_ | Console path only; no file-backed write |
| `open` / `close` | stub | `userspace_boot` | Returns fd 0–2 only |
| `fork` | stub | `userspace_boot` | Returns `ENOSYS` |
| `execve` | stub | `userspace_boot` | ELF loader partial |
| `exit` / `exit_group` | partial | _(default)_ | Process teardown incomplete |
| `waitpid` / `wait4` | stub | `userspace_boot` | Returns `ECHILD` |
| `mmap` / `munmap` | partial | `full-kernel` | Anonymous mmap; file-backed stub |
| `brk` | partial | `full-kernel` | — |
| Syscall-trace counters | real | `syscall-trace` | Per-syscall ns timing via /proc/syscall_stats |

## File System

| Module | Status | Feature gate | Notes |
|--------|--------|--------------|-------|
| `src/fs/` VFS | partial | `full-kernel` | Inode/dentry layer incomplete |
| initramfs (cpio newc) | partial | `userspace_boot` | Discovery path; missing ESP-less path |
| ext2 | experimental | `full-kernel` | Read-only; not CI-gated |
| ext4 | planned | `full-kernel` | — |
| devfs | stub | `userspace_boot` | `/dev/null`, `/dev/zero` stubs only |
| procfs | stub | `userspace_boot` | Returns static strings; WARN emitted at runtime |

## Process & IPC

| Module | Status | Feature gate | Notes |
|--------|--------|--------------|-------|
| `src/proc/` | stub | `userspace_boot` | Process table stub; emits WARN at runtime |
| `src/ipc/` pipe | stub | `full-kernel` | `pipe()` returns `ENOSYS` |
| `src/ipc/` socket | planned | `full-kernel` | — |
| Scheduler | partial | _(default)_ | Round-robin; no priority or CFS |

## Drivers & Devices

| Module | Status | Feature gate | Notes |
|--------|--------|--------------|-------|
| `src/drivers/` VirtIO | partial | _(default)_ | Block + net devices; GPU stub |
| `src/drivers/` AHCI | experimental | `full-kernel` | Not CI-gated |
| `src/drivers/` NVMe | planned | `full-kernel` | — |
| `src/tty/` | partial | _(default)_ | Serial console; PTY stub |
| `src/input/` | stub | `full-kernel` | Returns no events |

## Display & Compositor

| Module | Status | Feature gate | Notes |
|--------|--------|--------------|-------|
| `src/display/` DRM/KMS | experimental | `wayland-compositor` | Not CI-gated; no privilege drop yet |
| Wayland compositor | experimental | `wayland-compositor` | MVP feature set; seccomp/focus/ping missing |

## Networking

| Module | Status | Feature gate | Notes |
|--------|--------|--------------|-------|
| `src/net/` | experimental | `net` | VirtIO NIC; no TCP/IP stack yet |

## Security

| Module | Status | Feature gate | Notes |
|--------|--------|--------------|-------|
| `src/security/` | stub | `full-kernel` | LSM hooks wired; no policy engine |
| seccomp | planned | `full-kernel` | — |

## Fault Injection

| Module | Status | Feature gate | Notes |
|--------|--------|--------------|-------|
| `src/fault_inject/` PMM OOM | real | `fault-inject` | CI-gated via fault-inject workflow |
| `src/fault_inject/` VMM map | real | `fault-inject` | — |
| `src/fault_inject/` syscall | partial | `fault-inject` | EMFILE path done; fork/exec alloc paths pending |

---

## CI stub guard

`scripts/ci/check-stubs.sh` enforces that no module whose status is `stub`
appears in a `full-kernel` feature build. Run it locally with:

```sh
bash scripts/ci/check-stubs.sh
```
