# Memory management

The full memory subsystem is selected only when both slim-profile flags are absent. This guide follows `allocator.rs` and `src/mm/` at `08a14f6`; the [audit](source-audit.md) records focused host evidence without claiming a working full kernel.

## Allocator layers

| Layer | Implementation | Contract |
| --- | --- | --- |
| Minimal heap | `boot_minimal.rs` + `mm/bump_allocator.rs` | 64 KiB, bump allocation, no reclamation |
| Diagnostic heap | `userspace_boot.rs` + common bump allocator | 1 MiB, diagnostic-profile allocation only |
| Full global heap | `allocator.rs` + `allocator/region_heap.rs` | 4 MiB initial static region, allocation and deallocation with free-region coalescing |
| Heap accounting/growth helpers | `mm/heap.rs` | Tracking and PMM-backed growth/shrink interfaces |
| Compatibility aliases | `mm/allocator/mod.rs` | Re-exports heap helpers as `buddy`, `fixed_size_block`, and `stats`; not distinct global allocators |
| Slab allocation | `mm/slab.rs` | Separate cache implementation; not the installed Rust global allocator |
| Workspace interfaces | `crates/mm-core/src/lib.rs` | Address, flags, page metadata, NUMA, and allocator abstractions |

`allocator::heap_init` uses `spin::Once` and registers the initial region under the allocator lock. Allocation and free operations use `InterruptGuard` to preserve the caller's interrupt-enable state while serializing access.

`RegionHeap` tests cover reclamation/coalescing, small allocations and large alignment, and concurrent non-overlap/accounting. Those tests do not prove that every kernel allocation path handles exhaustion correctly.

## Physical pages

`mm/boot_memory.rs` supplies boot-region descriptions; architecture memory-discovery code produces the map consumed by `pmm::init_from_regions`. The PMM uses 4 KiB pages, supports eight NUMA-node slots, and clips registered address spans to its 64 GiB address limit.

- **Bootstrap pool**: A statically backed 16,384-page pool provides early pages and contiguous runs before buddy initialization. Failed requests do not advance the bump cursor.
- **Metadata sizing**: `init_from_regions` scans the complete usable map for its highest supported PFN before seeding regions. Metadata addresses remain stable after publication.
- **Buddy state**: `MAX_ORDER = 11` means order indices 0 through 10. PMM locking and interrupt masking serialize list manipulation; atomic fields do not make the algorithm lock-free.
- **Contiguous allocation**: `alloc_pages_contig` obtains a power-of-two buddy block when available, returns excess tail pages, and can use the contiguous bootstrap pool. Requests above the supported order fail.
- **Freeing**: Page reference counts, bootstrap ownership bits, kernel-image exclusions, and alignment assertions govern release. Metadata-reserved bootstrap pages are not ordinary free-list entries.
- **Initialization limits**: Map initialization is one-time and boot-only. Late region registration or metadata growth is rejected rather than advertised as memory hot-add.

The host PMM harness redirects physical memory accesses into host backing and exercises production initialization and accounting. It does not test firmware map correctness, real page-table mappings, or NUMA hardware.

## Kernel stacks and deferred ownership

`mm/kstack.rs` allocates three contiguous frames: a guard frame followed by two writable, non-executable stack frames. It uses the physical-map virtual addresses, so independently allocated noncontiguous frames are not an acceptable fallback.

The registry in `mm/kstack/ownership.rs` owns backing records and uses generation-tagged tokens. A dropped `KstackInfo` does not free a stack; `retire` and the legacy `free_kstack(top)` only mark the record retired.

`reclaim_retired_on_stopped_machine` is unsafe and has no automatic caller installed. All CPUs, saved continuations, runqueues, TSS/IST references, allocation races, and TLB reuse must satisfy its documented exclusion conditions before physical frames can be reclaimed.

Mapping rollback restores direct-map mappings before returning frames to the PMM. If restoration fails, ownership is retained instead of freeing pages whose mappings may still be invalid.

## Virtual memory and user access

- **`mm/mmap/`**: Anonymous regions, mapping operations, protection changes, fault handling, VMA data, and MM locking. This is the syscall-facing mapping subtree.
- **`mm/vma.rs`**: A separate VMA representation also remains declared. Do not interchange it with `mm::mmap::Vma` or assume all callers use one consolidated model.
- **`mm/cow_fault.rs`, `proc/cow_fault.rs`**: CoW support exists, but the current syscall fork path explicitly copies resident private pages rather than promising CoW across every architecture.
- **`mm/page_fault.rs`**: Shared page-fault classification/dispatch with architecture trap integration. Fault-to-signal behavior must be checked on the relevant target.
- **`kernel/uaccess.rs`**: User-range validation, page-table checks, copy helpers, typed user pointers/slices, and architecture protection handling. Raw user pointers are not trusted kernel slices.
- **`mm/phys.rs`, `mm/vm_rw.rs`**: Direct-map translation and virtual-memory read/write support. Physical-address identity assumptions still exist in several allocators and drivers.
- **`mm/mlock.rs`, `mm/rss.rs`, `mm/pkeys.rs`**: Locking, resident-set accounting, and protection-key interfaces. Their presence is not a complete Linux VM-ABI claim.

## Pressure, swap, and diagnostics

`mm::init` declares initialization calls for slab, swap, and the OOM handler, and `kasan`, `slab`, `swap`, and `oom_handler` are real declared modules in full builds. However, source-level initialization hooks and implementations are not evidence of tested pressure recovery or correct boot ordering on all platforms.

The OOM handler contains a branch gated by the undeclared root feature `swap`, while `mm::swap` itself is declared without that feature gate. `mm/core_dump.rs` is empty, and the fault wrapper files `pmm_fault.rs` and `vmm_fault.rs` are not declared by `mm/mod.rs`.

See [fault recovery](fault-recovery.md) for the missing end-to-end evidence and [process lifetime](processes-and-scheduling.md) for retained address spaces after exit. Any memory-leak or reclamation claim must account for these deliberate retention queues.
