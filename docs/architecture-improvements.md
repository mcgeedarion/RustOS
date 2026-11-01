# Architecture improvements

This backlog is grounded in the current declarations and execution paths at `08a14f6`. It separates completed source repairs from remaining integration rather than presenting an architectural proposal as already implemented.

## Repairs now present

- **Reclaiming full heap**: `RegionHeap` replaces the old full-profile bump behavior and has focused allocation/coalescing/concurrency tests.
- **Whole-map PMM setup**: Metadata sizing precedes region publication, and bootstrap contiguous allocation has host regressions.
- **Stack ownership**: Generation-tagged registry records retain backing beyond temporary handles and defer physical release.
- **Exit/reap lifetime**: Owned snapshots and identity-checked removal avoid stale PID assumptions; retired task storage protects scheduler references.
- **Exec/ABI boundaries**: Staged ELF replacement and explicit foreign-number translation are present, even though full integration fails.

These improvements should be preserved during future consolidation. Their host-test evidence does not prove every architecture or concurrent runtime path.

## High-priority consistency work

- **Module graph**: Resolve duplicate socket candidates, missing PLIC source, empty exported modules, undeclared feature gates, and intentionally unregistered implementation files.
- **Includes versus modules**: Preserve the fact that four syscall compatibility files use `include!`. Moving them to submodules requires explicit imports/visibility/router changes.
- **Descriptor model**: Centralize conversions among process FDs, scheme backing FDs, local FileIds, and raw VFS handles.
- **Memory model**: Reconcile separate VMA/fork/fault implementations without reintroducing unsupported CoW or unsafe reclamation claims.
- **Error model**: Consolidate overlapping error families and errno-sign conversions only with boundary tests.
- **Workspace ownership**: Make extraction crates genuinely consumable and remove or document compatibility aliases rather than maintaining divergent hidden implementations.

## Safety and service boundaries

The intended hybrid boundary needs an executable service lifecycle: process creation, capability assignment, endpoint ownership, notification/reply handling, restart policy, and resource cleanup. `init/service_manager.rs` stores this policy/state but does not itself prove a complete launched/restarted driver service.

Resource reclamation requires a stronger proof than logical removal from a table. Define per-object ownership, permitted raw references, lock order, quiescence, and TLB requirements before adding a background reaper.

## Tooling and evidence

- **Tests**: Restore agreement between guest smoke behavior and host fixtures, and add failure-path assertions rather than success-only mocks.
- **CI**: Keep full builds and full execution separate from minimal firmware checks; disclose patch-applied workflow scope.
- **Metrics**: Align boot regression inputs with actual parser output and measure payload rather than fixed image capacity when evaluating size.
- **Documentation**: Regenerate the complete source index and preserve explicit status for empty/unregistered/simulated paths.

Use [ADR 0001](adr/0001-workspace-crate-architecture.md), [ADR 0002](adr/0002-unified-error-handling.md), and [ADR 0003](adr/0003-vfs-registration.md) as design intent with current implementation notes. None is a substitute for the full compiler/runtime gates.
