# Atomic Ordering Guidelines for RustOS

This document establishes standardized atomic ordering patterns to prevent concurrency bugs and ensure consistent memory semantics across the kernel.

## Quick Reference

| Use Case | Load | Store | CAS | Rationale |
|----------|------|-------|-----|-----------|
| **Counters (informational)** | `Relaxed` | `Relaxed` | N/A | No ordering needed, purely informational |
| **Flags (one-way state)** | `Acquire` | `Release` | `AcqRel` | Ensures visibility of prior writes |
| **Lock acquisition** | `Acquire` | N/A | `AcqRel` | Must see all prior protected data |
| **Lock release** | N/A | `Release` | N/A | Must publish all protected writes |
| **Reference counting** | `Acquire` | `Release` | `AcqRel` | Prevents use-after-free |
| **Simple flags (no data dependency)** | `Relaxed` | `Relaxed` | N/A | When no other memory ops depend on flag |

## Standardized Patterns

### Pattern 1: Informational Counters

Use for statistics, monitoring, `/proc/meminfo` data:

```rust
use core::sync::atomic::{AtomicUsize, Ordering};

static BYTES_ALLOCATED: AtomicUsize = AtomicUsize::new(0);

// Update counter - Relaxed is sufficient
BYTES_ALLOCATED.fetch_add(size, Ordering::Relaxed);

// Read counter - Relaxed is sufficient  
let bytes = BYTES_ALLOCATED.load(Ordering::Relaxed);
```

**Rationale**: These counters have no ordering dependencies with other memory operations. They're purely informational.

### Pattern 2: Initialization Flags

Use for one-time initialization checks:

```rust
use core::sync::atomic::{AtomicBool, Ordering};

static INITIALIZED: AtomicBool = AtomicBool::new(false);

// Check if initialized (Acquire ensures we see prior init writes)
if !INITIALIZED.load(Ordering::Acquire) {
    // ... initialize ...
    // Release ensures our init writes are visible
    INITIALIZED.store(true, Ordering::Release);
}

// Or use compare_exchange for atomic check-and-set
if INITIALIZED
    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
    .is_err()
{
    // Already initialized by another CPU
    return;
}
```

**Rationale**: Acquire/Release ensures that all writes during initialization are visible to CPUs that observe the flag.

### Pattern 3: Lock-Free Data Structures

Use for bump allocators, queues, and similar structures:

```rust
use core::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

loop {
    let current = NEXT.load(Ordering::Relaxed);
    let new_value = current + size;
    
    // AcqRel: Acquire ensures we see prior allocations,
    //         Release publishes our allocation to others
    if NEXT.compare_exchange(current, new_value, Ordering::AcqRel, Ordering::Acquire).is_ok() {
        break;
    }
}
```

**Rationale**: The CAS needs both acquire and release semantics to maintain consistency.

### Pattern 4: Per-CPU Data Publication

Use when publishing data that other CPUs will read:

```rust
use core::sync::atomic::{AtomicU64, Ordering};

static LAPIC_PHYS: AtomicU64 = AtomicU64::new(0);

// Writer (during boot, before SMP)
LAPIC_PHYS.store(phys_addr, Ordering::Release);

// Reader (from any CPU)
let addr = LAPIC_PHYS.load(Ordering::Acquire);
```

**Rationale**: Release ensures the address and any associated data structures are fully initialized before publication.

## Common Anti-Patterns to Avoid

### ❌ Using SeqCst Everywhere

```rust
// BAD: Unnecessary performance cost
counter.fetch_add(1, Ordering::SeqCst);

// GOOD: Use appropriate ordering
counter.fetch_add(1, Ordering::Relaxed); // for simple counters
```

**Why**: `SeqCst` is the strongest (and slowest) ordering. Only use when you truly need sequential consistency across all CPUs.

### ❌ Using Relaxed for Synchronization

```rust
// BAD: May not synchronize properly
if !READY.load(Ordering::Relaxed) {
    // May not see data written before READY was set!
    process_data();
}

// GOOD: Use Acquire for synchronization
if !READY.load(Ordering::Acquire) {
    // Guaranteed to see all writes before RELEASE store to READY
    process_data();
}
```

**Why**: Relaxed provides no synchronization guarantees.

### ❌ Mismatched Orderings

```rust
// BAD: Inconsistent pattern makes reasoning difficult
WRITER: flag.store(true, Ordering::Release);
READER: if flag.load(Ordering::Relaxed) { ... } // Won't see writer's data!

// GOOD: Match Release with Acquire
WRITER: flag.store(true, Ordering::Release);
READER: if flag.load(Ordering::Acquire) { ... } // Sees writer's data
```

## Architecture-Specific Notes

### x86-64

x86 has strong memory ordering by default:
- Loads are not reordered with other loads
- Stores are not reordered with other stores  
- Loads are not reordered with stores
- **But**: Stores MAY be reordered with earlier loads

This means:
- `Relaxed` loads/stores often compile to normal MOV instructions
- `Acquire` adds a load barrier (LFENCE) only when needed
- `Release` on x86 is often free (stores aren't reordered anyway)

### AArch64

AArch64 has weaker memory ordering:
- Both loads and stores can be reordered
- Explicit barriers are more critical

This means:
- Always use proper orderings, never assume "it works"
- `Acquire`/`Release` will generate explicit DMB instructions
- Performance difference between orderings is more pronounced

## Migration Guide

When updating existing code:

1. **Identify the purpose** of each atomic operation
2. **Classify** into one of the patterns above
3. **Update** to use the recommended ordering
4. **Document** non-obvious cases with comments

Example migration:

```rust
// Before: Inconsistent orderings
static HEAP_BYTES: AtomicUsize = AtomicUsize::new(0);
HEAP_BYTES.store(bytes, Ordering::SeqCst); // Overkill
let b = HEAP_BYTES.load(Ordering::Relaxed); // OK for reads

// After: Consistent, documented
/// Total bytes committed to heap (for /proc/meminfo)
/// Relaxed ordering: purely informational, no synchronization
static HEAP_BYTES: AtomicUsize = AtomicUsize::new(0);
HEAP_BYTES.store(bytes, Ordering::Relaxed);
let b = HEAP_BYTES.load(Ordering::Relaxed);
```

## Testing Recommendations

1. **Run with Miri** (when possible) to detect ordering violations
2. **Stress test** SMP scenarios with concurrent access
3. **Add assertions** in debug builds to verify invariants
4. **Document** why each atomic uses its specific ordering

## References

- [Rustonomicon: Atomics](https://doc.rust-lang.org/nomicon/atomics.html)
- [Parker, McKenney: What Every Programmer Should Know About Memory](https://www.kernel.org/doc/Documentation/memory-barriers.txt)
- [Rust Atomic Ordering Documentation](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html)
