# RustOS Product Milestones

_Last reviewed: 2026-08-30._

The project is currently between **M2** and **M3**: the default Cargo feature set is `full_kernel`
(expanding to `uefi_boot` and `userspace_boot`), while the full
mm/fs/proc/syscall graph is still being stabilized behind documented validation
gates.

| Milestone | Goal | Cargo feature path | CI/local sentinel | Current state |
|---|---|---|---|---|
| **M1** | Minimal UEFI boot reaches the idle loop on x86_64 and remains aligned on aarch64 | `boot_minimal` / `uefi_boot` | `BOOT_MINIMAL_OK` or `entering cpu_idle` | **Maintained smoke path** |
| **M2** | Build an initramfs and reach a userspace handoff marker | `userspace_boot` with `--initrd` when needed | `FULL_OS_USERSPACE_OK` | **Current integration focus**: thin fs/proc shims remain intentional |
| **M3** | Minimum init syscalls have real semantics and bad-pointer safety | full kernel graph, syscall modules | init exercises `open/fork/exec/wait/exit` without `ENOSYS` or panic | **In progress** |
| **M4** | VFS/initramfs/devfs/procfs are usable enough for real init workloads | full kernel graph | `/init`, `/dev/null`, `/proc/version` accessible | **In progress** |
| **M5** | Network, display, and input demos run as userspace processes | full kernel graph plus optional device features | shell/compositor/network demo sentinel | **Experimental/planned** |

## Sentinel contract

Sentinels are emitted on the serial console and are the only supported way to
decide whether a boot succeeded in automation.

```text
BOOT_MINIMAL_OK          # minimal boot path reached its success point
BOOT_INITRAMFS_MOUNTED   # VFS initramfs mount complete (M3)
BOOT_PROC_SPAWNED        # First user process enqueued (M3)
FULL_OS_USERSPACE_OK     # userspace handoff path reached its success point
entering cpu_idle        # generic idle-loop marker
```

`cargo xtask smoke` treats any of the five strings as success and writes the
serial log under `target/smoke-<arch>.log`.

See `docs/M3_M5_ROADMAP.md` for detailed milestone gates and validation commands.

## Promotion rule

Do not mark a milestone complete unless:

1. `docs/status.md` and `docs/syscalls.md` agree with the code state.
2. The relevant `cargo xtask smoke` or `cargo xtask check` command succeeds.
3. Any claimed performance or image-size improvement has a checked-in
   measurement or a documented command to reproduce it.
