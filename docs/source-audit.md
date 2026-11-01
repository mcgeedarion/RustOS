# Source audit and validation

This documentation audit was performed on 2026-11-01 against `main` at `08a14f61030facca4d8f52223f29a6479881eab8`. It changes documentation only and records existing implementation and validation gaps instead of silently repairing kernel code.

## Scope and method

- **Inventory**: All 510 tracked Rust, C, and assembly files were enumerated, including 473 Rust files under `src/`. The [source-file index](module-map.md) lists each file and includes workspace crates, userspace, fuzz harnesses, host tests, and build tooling.
- **Structural review**: Module declarations, explicit paths, includes, feature gates, manifests, target specifications, and workflow/script entry points were inspected. Empty and unregistered sources are kept visible.
- **Execution-path review**: Boot/profile dispatch, heaps/PMM/stacks, process creation and exit/reaping, syscall translation, VFS/FD routing, driver boundaries, security limitations, and smoke/benchmark evidence were traced.
- **Behavioral checks**: The pinned toolchain and available host environment were used for compilation and focused tests. The result matrix below separates executed checks from source-only conclusions.

This was a documentation-oriented static audit plus selected executable checks, not a line-by-line memory-safety proof or an exhaustive hardware test. Symbol inventories are lexical navigation aids; they do not establish conditional-compilation reachability or API completeness.

## Observed validation

The host was Linux x86_64 using `nightly-2026-06-07`, reporting `rustc 1.98.0-nightly (61d7280f3 2026-06-06)`. No QEMU executable was installed for this audit, and no emulator or physical-hardware pass is claimed.

| Check | Observed result | Scope |
| --- | --- | --- |
| `cargo xtask check --arch x86_64` | FAIL; compiler reported 1,082 errors, many cascading | Full default graph; primary blockers include duplicate socket modules and unresolved imports |
| `cargo xtask check --arch x86_64 --features boot_minimal` | PASS | Type checking only; no linking or boot proof |
| `cargo xtask check --arch x86_64 --features userspace_boot` | PASS | Diagnostic graph only |
| AArch64 UEFI `check` with `boot_minimal`, then `userspace_boot` | PASS, both | Type checking only |
| RISC-V UEFI `check` with `boot_minimal`, then `userspace_boot` | FAIL, both | Unresolved `SIZE` in `arch/riscv64/mem_layout.rs:81` |
| `cargo xtask test` | PASS, 16 tests | Hosted diagnostic-profile tests, not the full kernel |
| `cargo xtask lint-modules` | PASS | Heuristic module-hygiene check, not a compiler or reachability proof |
| `cargo xtask build-init --arch x86_64` | PASS | Built three freestanding programs and packed initramfs; no execution proof |
| Minimal x86_64 UEFI `cargo doc --lib --no-deps` | PASS with 2 warnings | Selected slim API graph only; existing source intra-doc-link warnings |
| Aggregate `userspace/Cargo.toml` metadata | FAIL | Multiple workspace roots; individual init/smoke/bench/raw-syscall metadata succeeds |
| `scripts/ci/test-region-heap.sh` | PASS, 3 tests | Actual `RegionHeap` allocation/reclamation behavior |
| `scripts/ci/test-pmm-init.sh` | PASS, 2 tests | Production PMM initialization/contiguous allocation with host backing |
| `scripts/ci/test-kstack-ownership.sh` | PASS, 4 tests | Ownership registry, generation tokens, deferred reclamation model |
| `scripts/ci/test-exit-lifecycle.sh` | PASS, 2 tests | Production exit/table/wait logic with controlled dependencies |
| Direct host `userspace/smoke/tests/assertions.rs` | FAIL, all 11 tests | Mock syscall expectations are behind the expanded smoke workload |
| Direct host `scripts/ci/tests/fault_engine.rs` | PASS, 7 tests | Atomic fault engine, not guest PMM/VMM/syscall injection |
| `python3 scripts/ci/test-boot-marks.py` | PASS, 14 tests | Boot marker parser |
| Python CI test discovery | PASS, 45 tests | Smoke/fault-log parsing, panic parsing, fuzz-coverage guard |
| `scheme-api` host tests | PASS, 2 unit tests and 1 doctest | Shared API helpers |
| `sync-primitives` host tests | PASS, 8 tests | Portable synchronization implementations |
| `fs-ext4`, `fs-fat32` host tests | PASS, 3 tests each | Adapter/probe behavior, not mounted-disk runtime |
| `vfs-core`, `mm-core`, `fs-tmpfs` host tests | PASS invocation, 0 tests each | Compilation only, no behavioral tests executed |
| `scripts/ci/check-stubs.sh` | PASS with warnings | Textual gate/marker guard, not implementation validation |
| `cargo xtask roadmap-check` after refresh | PASS | All five required-topic documentation checks |
| Documentation link/index/whitespace checks | PASS | All 41 Markdown files, 510 source rows exactly once, local link targets, balanced fences, `git diff --check` |
| Markdown lint | PASS | Markdown structure/style; line-length and table-alignment rules disabled for long reference tables |
| Full QEMU, physical hardware, guest fault recovery, bounded fuzz campaigns | NOT RUN | No runtime, recovery, performance, or fuzz-coverage claim |

Reproduction commands and additional compile-check results are maintained in [CI](ci.md) and [coverage](code_coverage.md). Error counts are an observation from this exact run, not a stable backlog size; one primary fix may eliminate many secondary diagnostics.

## Confirmed inconsistencies

| Finding | Evidence in the tree | Documentation consequence |
| --- | --- | --- |
| Ambiguous network module | `net/mod.rs` declares `socket`; both `net/socket.rs` and `net/socket/mod.rs` exist | Neither implementation can be described as the successfully compiled full socket module |
| Missing RISC-V IRQ source | `irq/riscv64/mod.rs` declares `plic`; no `irq/riscv64/plic.rs` exists | Architecture-local controller code does not satisfy this separate declaration |
| Empty integration files | `proc/clone.rs`, `mm/core_dump.rs`, `sync/mutex.rs`, `sync/rwlock.rs`, and four GDB module files are empty | Route declarations and feature names do not establish implementations |
| Unregistered implementation files | Examples: `sync/advanced.rs`, `mm/pmm_fault.rs`, `mm/vmm_fault.rs`, `fault_inject/hooks.rs`, `kmtest/fault_inject_tests.rs` | Treat these as unintegrated sources, not automatically active kernel behavior |
| Undeclared feature names | Kernel cfgs mention features absent from root Cargo.toml | Do not advertise `--features wayland`, `amd-isp4`, `swap`, or similar commands |
| FAT32 backing I/O is simulated | `fs/fat32.rs::blk_read` zero-fills; `blk_write` accepts data without a real backend | No writable FAT32 persistence claim |
| Workspace migration is incomplete | Filesystem adapters have `kernel_impl` paths/fallbacks; independent registries coexist | Do not describe kernel VFS replacement as finished |
| Smoke mocks are stale | Expanded checks make more opens and validate real file/pipe data; host mock arrays/data are not aligned | All 11 host assertion cases currently fail, despite parser tests passing |
| Aggregate userspace workspace is inconsistent | Aggregate members include independent workspace roots | Use xtask's per-package build path; do not claim a working aggregate workspace build |
| Production guard is not authoritative | `verify-production-requirements.sh` derives its workspace directory as `scripts/` and uses textual searches | Its name and any prose checklist do not prove production readiness |

## Recent implementation reflected here

- **Heap repair**: `allocator.rs` installs a reclaiming region heap with interrupt-safe serialization and 4 MiB initial backing. Slim profiles still use their own bump heaps.
- **PMM repair**: `init_from_regions` determines metadata capacity from the full map before publishing free lists. Bootstrap contiguous allocation, page accounting, and bounds have focused host regressions.
- **Stack ownership**: Allocation records outlive handles; stack retirement is separate from physical freeing. Reclamation is explicitly unsafe and requires stopped-machine conditions not installed automatically.
- **Exit/reap repair**: Table readers use owned PCB snapshots, zombie removal checks object identity, and retired task objects remain owned to protect scheduler raw pointers. This is conservative retention, not completed reclamation.
- **Exec and smoke**: Real image-replacement code and file/pipe guest assertions exist. Their existence replaces older “entirely absent” descriptions without claiming a passing full execution gate.

## How to update this audit

Record the source revision, exact feature/target selection, command, exit result, and evidence scope for each new run. Only promote a runtime or hardware capability after the corresponding real execution path passes; see [maintenance](maintenance.md) and [acceptance criteria](acceptance-criteria.md).
