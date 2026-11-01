# Userspace programs

The userspace tree contains freestanding programs and ABI support outside the kernel workspace. Several packages declare their own workspace roots, so the aggregate `userspace/Cargo.toml` is not a currently working “build everything” entry point.

## Packages and workspace state

| Path | Role | Packaging boundary |
| --- | --- | --- |
| `userspace/init` | PID 1, smoke/benchmark child execution and exact-child reaping | Independent workspace; xtask builds it directly |
| `userspace/smoke` | Syscall and FS/IPC assertion workload | Independent workspace; xtask stages it as `/bin/smoke` |
| `userspace/bench` | Syscall microbenchmark, raw JSON measurements and metadata | Independent workspace; xtask stages it as `/bench` |
| `userspace/raw-syscall` | Architecture-specific raw syscall constants/wrappers | Independent workspace dependency |
| `userspace/libc-shim` | Small libc-shaped compatibility helpers | Not a complete libc; `provide_start` is its own feature |
| `userspace/hello` | Minimal example program | Not included by xtask's initramfs builder |
| `userspace/kmtest` | Private kmtest syscall client | Not included by xtask's default initramfs builder |
| `userspace/drivers` | VirtIO-net userspace prototype | Outside the aggregate member list and default archive |

`cargo metadata --manifest-path userspace/Cargo.toml --no-deps` fails with multiple workspace roots at this revision. Metadata queries for init, smoke, bench, and raw-syscall individually succeed, and the separate x86_64 `build-init` command also built all three packaged programs during this audit.

## Build and archive contents

The intended command is `cargo xtask build-init --arch <arch>`. It selects `x86_64-unknown-none`, `aarch64-unknown-none`, or `riscv64gc-unknown-none-elf`, installs the target, and builds init, smoke, and bench separately.

```bash
cargo xtask build-init --arch x86_64
```

xtask expects artifacts under each package's own `target/<target>/release/` directory. It stages `/init`, `/bin/smoke`, `/bench`, `/etc/os-release`, and directories including `/tmp`, then writes root `initramfs.cpio` using its own newc encoder.

The archive builder sorts entries and encodes directory/file metadata, avoiding a dependency on host `cpio` for this path. Alternative shell image/packaging scripts are separate workflows and should not be assumed equivalent.

## ABI and process startup

`raw-syscall` supplies architecture-specific syscall numbers and register-level wrappers for the current subset. The kernel translates AArch64/RISC-V numbers at `dispatch_arch`; number similarity is not enough to establish semantic compatibility.

The real exec loader prepares argc/argv/envp/auxv and enters the target-specific user context. The `userspace_boot` profile instead validates ELF metadata and stops with an unsupported marker; it never proves that the freestanding entry point ran.

## Init, smoke, and benchmark lifecycle

Init launches separate smoke and benchmark processes and waits for their exact positive PIDs. A successful smoke sequence requires assertions from the child plus matching zero-status wait records, and a benchmark sequence requires valid measurement metadata/statistics and its own matched child record.

The smoke checks cover error returns, file contents and FD semantics, memory operations, file mutation, and a pipe round trip. The host mock tests are currently stale relative to this expanded workload; see [the observed audit results](source-audit.md).

The benchmark emits a schema-versioned JSON record with totals, batch-average percentile summaries, and environment metadata; it does not emit the complete raw sample array. It is not a performance result until the binary executes under the intended full kernel and the strict parser validates the same run.
