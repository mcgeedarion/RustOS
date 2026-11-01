# M3 to M5 implementation roadmap

This is an updated backlog based on the `08a14f6` source, not a completion report or an approved release schedule. It supersedes earlier descriptions that treated absent integration as a small final step or described all exec support as missing.

## Prerequisite: make the actual graph build

- **Network ownership**: Choose and reconcile the two socket layouts and the undeclared TCP state-machine dependency.
- **Architecture consistency**: Fix RISC-V `SIZE`, missing generic IRQ PLIC declaration, and architecture-specific imports/types.
- **Empty surfaces**: Implement or honestly remove routes depending on empty clone, lock, core-dump, and GDB files.
- **Workspace/configuration**: Reconcile aggregate userspace workspace roots, undeclared feature names, full-build errors, and docs.rs all-features conflict.
- **Test fixtures**: Update host smoke mocks to match real file/pipe operations without weakening assertions.

These are source changes beyond the scope of this documentation refresh. Completion requires full-profile checks, not only the already smaller boot graphs.

## M3: core semantics and lifetime

- **Memory**: Extend PMM/heap pressure and rollback tests to real guest execution; validate mappings, permissions, bounds, and accounting.
- **Process lifetime**: Add a proved scheduler/TLB quiescence protocol before freeing retired tasks, stacks, and address spaces.
- **Exec/fork**: Run the staged image replacement and copying fork paths, including failure rollback, exact-child waits, FD inheritance, and close-on-exec.
- **VFS/IPC**: Validate real guest file mutation and pipe payload markers, mount/backend semantics, and endpoint cancellation/cleanup.
- **Architecture ABI**: Test number translation and user return on each target; finish RISC-V signal-frame/return before enabling handler redirection.

Host regression suites are useful foundations, not a substitute for this phase's runtime work. Keep the source improvements and unmet execution gate visible together.

## M4: devices, services, and tooling

- **Driver validation**: Record exact device/transport support with bounded I/O, reset, timeout, DMA, and interrupt tests.
- **Userspace services**: Connect service metadata/restart policy, process launch, endpoint ownership, driver handles, and teardown into an executable lifecycle.
- **Networking**: Establish NIC-to-socket interoperability after module reconciliation, including malformed packets and queue pressure.
- **Graphics/input**: Validate framebuffer/DRM/input flows separately; reconcile `wayland` and `amd-isp4` feature declarations before advertising those paths.
- **Debugging**: Wire GDB and serial REPL modules, preserve panic protocols, and separate QEMU debug-server use from in-kernel stub functionality.
- **Workspace split**: Replace or retire duplicate implementations only after adapter consumers and tests agree.

## M5: resilience and release evidence

- **Fault recovery**: Wire all six production fault scenarios, prove they fire, verify rollback, and run post-recovery workloads.
- **Security**: Validate capability/namespace/seccomp/LSM enforcement and negative cases at actual call sites.
- **Persistence**: Replace simulated filesystem/block edges and test backend durability/crash recovery.
- **Performance**: Run the real schema-2 benchmark and boot-timing measurements with comparable environment metadata.
- **Readiness**: Maintain reproducible build/runtime artifacts, supported-target/device boundaries, and a truthful limitations register.

Promotion uses [acceptance-criteria.md](acceptance-criteria.md), not milestone names or an aggregate source-line count. Work may be reordered for dependencies, but an unsupported path must never be reclassified by changing only its documentation.
