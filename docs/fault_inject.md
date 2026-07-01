# Fault Injection Framework

_Last reviewed: 2026-07-01._

The fault injection framework lets you deliberately introduce failures into
critical kernel subsystems during testing to verify that every error path
returns a proper `Err(…)` rather than panicking or silently corrupting state.

## Compile-time gate

The framework is gated behind the `fault-inject` Cargo feature and is compiled only in the full-kernel graph. Production-oriented boot builds should leave the feature disabled:

```toml
# Development / CI (fault injection enabled)
cargo xtask check --arch x86_64 --features fault-inject,kmtest

# Production (framework completely absent)
cargo xtask build --arch x86_64 --profile release-boot
```

The default feature set does not include `fault-inject`; enable it only for targeted full-kernel error-path validation.

---

## Architecture

```
src/fault_inject/
├── mod.rs          ← FaultPoint struct + macro API + static catalogue
└── hooks.rs        ← Thin per-subsystem hook functions

src/mm/
├── pmm_fault.rs    ← PMM integration hooks
└── vmm_fault.rs    ← VMM integration hooks

src/syscall/
└── fault.rs        ← Syscall-path integration hooks + FaultErrno type

src/kmtest/
└── fault_inject_tests.rs  ← 7 automated test cases
```

### `FaultPoint`

Each subsystem registers one or more `static FaultPoint` instances in
`src/fault_inject/mod.rs::points`.  A `FaultPoint` has:

| Field | Type | Purpose |
|---|---|---|
| `name` | `&'static str` | Human-readable label for log messages |
| `armed` | `AtomicBool` | Whether the point is currently active |
| `budget` | `AtomicI64` | Remaining successful calls before failure fires |
| `triggers` | `AtomicU64` | Total number of times failure was injected |

---

## Fault point catalogue

| Constant | Subsystem | Simulated failure |
|---|---|---|
| `FAULT_PMM_ALLOC` | Physical Memory Manager | `alloc_frame()` → OOM (`None`) |
| `FAULT_PMM_CONTIGUOUS` | Physical Memory Manager | contiguous alloc → `ENOMEM` |
| `FAULT_VMM_MAP` | Virtual Memory Manager | `map_page()` → map failure |
| `FAULT_VMM_REGION` | Virtual Memory Manager | region reservation → `ENOMEM` |
| `FAULT_SYSCALL_RESOURCE` | Syscall dispatch | generic resource exhaustion (`EAGAIN`) |
| `FAULT_SYSCALL_FDTABLE` | Syscall / fd-table | fd-table insert → `EMFILE` |

---

## API reference

### Arming a fault point

```rust
// Fail immediately on the very next call:
FAULT_PMM_ALLOC.arm(0);

// Allow 2 successful calls, then fail the 3rd:
FAULT_PMM_CONTIGUOUS.arm(2);
```

### Checking / disarming

```rust
// One-shot check (auto-disarms after firing):
if FAULT_VMM_MAP.check() { return Err(MapError::NoMemory); }

// Persistent check (keeps firing until explicitly disarmed):
if FAULT_VMM_MAP.check_persistent() { return Err(MapError::NoMemory); }

// Explicit disarm:
FAULT_VMM_MAP.disarm();

// Query how many times a point has fired:
let n = FAULT_PMM_ALLOC.trigger_count();
```

### `maybe_inject!` macro

The cleanest way to insert a fault check into existing code:

```rust
// Expands to: if FAULT_PMM_ALLOC.check() { return None; }
// when feature = "fault-inject", and to nothing otherwise.
maybe_inject!(FAULT_PMM_ALLOC, None);
```

### Per-subsystem hooks

Subsystems expose typed wrapper functions so callers do not need to import
the raw `FaultPoint` constants:

```rust
// In pmm.rs:
#[cfg(feature = "fault-inject")]
if crate::mm::pmm_fault::check_alloc_frame() {
    return None; // OOM
}

// In vmm.rs:
#[cfg(feature = "fault-inject")]
if crate::mm::vmm_fault::check_map_page() {
    return Err(VmmError::MapFailed);
}

// In syscall handler:
#[cfg(feature = "fault-inject")]
crate::syscall::fault::check_resource()?;
```

---

## Test suite

Fault-injection tests live in `src/kmtest/fault_inject_tests.rs` and compile when both `fault-inject` and `kmtest` are enabled in the full-kernel graph.

| Test | Scenario covered |
|---|---|
| `pmm_oom_injection` | PMM OOM — fires immediately, one-shot |
| `pmm_contiguous_skip_budget` | PMM contiguous alloc — skip-budget (fires on Nth call) |
| `vmm_map_failure` | VMM map failure — fires immediately, one-shot |
| `vmm_region_failure` | VMM region alloc failure — fires immediately, one-shot |
| `syscall_resource_exhaustion` | Syscall EAGAIN — correct `Err` return, no panic |
| `syscall_fdtable_exhaustion` | Syscall EMFILE — correct `Err` return, no panic |
| `persistent_fault_mode` | Persistent mode — fires until explicit `disarm()` |

### Acceptance criteria

- [x] Injected failures produce correct error returns (not panics)
- [x] Framework is absent in release/production builds
- [x] 7 fault scenarios covered (≥ 3 required)
- [x] All three subsystems (PMM, VMM, Syscall) represented
