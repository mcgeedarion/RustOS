# Compiler Optimizations Guide

This document describes the compiler optimizations implemented in RustOS, based on IR-level optimization techniques.

## Overview

RustOS leverages several compiler optimization passes that operate on the Intermediate Representation (IR) to transform literal translations of source code into efficient machine code. These optimizations are enabled through a combination of:

1. **Build configuration** (Cargo profiles, LTO, PGO)
2. **Rust language features** (target features, purity hints)
3. **Code attributes** (`#[inline]`, `#[cold]`, `#[must_use]`)
4. **LLVM passes** (automatic via optimization levels)

## Implemented Optimizations

### 1. Link-Time Optimization (LTO)

**Status**: ✅ Enabled

LTO allows the optimizer to see across crate boundaries during the linking phase, enabling:
- Cross-module inlining
- Dead code elimination across crates
- Specialization based on call-site context

**Configuration** (`Cargo.toml`):
```toml
[profile.release]
lto = "fat"  # Full LTO for maximum optimization
codegen-units = 1  # Single codegen unit for better optimization opportunities

[profile.release-boot]
lto = "fat"
opt-level = "z"  # Optimize for size
```

### 2. Profile-Guided Optimization (PGO)

**Status**: ⚙️ Infrastructure Ready

PGO allows the compiler to optimize based on actual runtime behavior:
- Hot/cold path identification
- Better branch prediction hints
- Improved code layout for cache locality
- Aggressive inlining on hot paths

**Usage**:
```bash
# Step 1: Build with instrumentation
cargo build --profile release-pgo-generate --features userspace_boot

# Step 2: Run workloads to generate profiling data
# (profraw files will be generated in the working directory)

# Step 3: Merge profraw files into profdata
llvm-profdata merge -sparse *.profraw -o rustos.profdata

# Step 4: Rebuild using the profile data
# Add to Cargo.toml:
# profile-use = "rustos.profdata"
```

### 3. Target Feature Enablement

**Status**: ✅ Enabled for x86_64

Enables SIMD auto-vectorization and target-specific optimizations by exposing CPU features to the compiler:

**Features enabled** (`src/lib.rs`):
- `avx_target_feature` - AVX instructions
- `avx512_target_feature` - AVX-512 instructions  
- `sse4a_target_feature` - SSE4a instructions
- `stdarch_x86_avx512` - AVX-512 intrinsics
- `reexport_target_features` - Propagate target features

**Runtime detection** (`src/arch/x86_64/cpu.rs`):
```rust
pub fn has_avx() -> bool { ... }
pub fn has_avx2() -> bool { ... }
pub fn has_avx512f() -> bool { ... }
pub fn has_sse41() -> bool { ... }
pub fn has_sse42() -> bool { ... }
```

### 4. Function Attributes

#### Inline Hints
**Status**: ✅ Used throughout

- `#[inline]` - Suggests inlining for small functions
- `#[inline(always)]` - Forces inlining for critical paths
- `#[inline(never)]` - Prevents inlining to reduce code size

#### Cold Path Marking
**Status**: ✅ Applied to panic handlers

The `#[cold]` attribute marks error paths as unlikely, enabling:
- Better branch prediction
- Code layout optimization (cold paths moved away from hot paths)
- Reduced instruction cache pressure on hot paths

**Example** (`src/kernel/panic.rs`):
```rust
#[cold]
#[inline(never)]
fn panic(info: &core::panic::PanicInfo) -> ! { ... }

#[cold]
#[inline(never)]
fn alloc_error(layout: core::alloc::Layout) -> ! { ... }
```

#### Must Use Annotations
**Status**: ✅ Applied to pure functions

The `#[must_use]` attribute indicates functions without side effects, enabling:
- Common subexpression elimination
- Dead code elimination
- Better scheduling decisions

**Example** (`src/arch/x86_64/cpu.rs`):
```rust
#[must_use]
pub fn has_avx() -> bool { ... }
```

### 5. Strength Reduction

**Status**: ✅ Automatic via LLVM

The compiler automatically transforms expensive operations into cheaper equivalents:
- Multiplication → Addition/Shift (for loop induction variables)
- Division → Multiplication by reciprocal (for known divisors)
- Array indexing → Pointer arithmetic

**Example pattern** (automatic):
```rust
// Source: arr[i] becomes base + i * sizeof(T)
// Optimized: pointer incremented by sizeof(T) each iteration
for i in 0..len {
    unsafe { *ptr.add(i) }  // Becomes pointer bump
}
```

### 6. Loop Optimizations

**Status**: ✅ Automatic via LLVM

The following loop optimizations are performed automatically:

- **Loop Invariant Code Motion (LICM)**: Hoists computations outside loops
- **Loop Unrolling**: Reduces loop overhead by duplicating body
- **Loop Vectorization**: Converts scalar operations to SIMD
- **Loop Fusion**: Merges adjacent loops over same data
- **Loop Interchange**: Optimizes memory access patterns for cache

**Requirements for vectorization**:
- No loop-carried dependencies
- Known iteration count
- No early exits
- Proper memory alignment
- No aliasing between arrays

### 7. Alias Analysis

**Status**: ✅ Automatic via LLVM

Rust's ownership model provides strong aliasing guarantees:
- Mutable references are unique
- Immutable references cannot alias mutable ones
- Box<T> guarantees unique ownership

This enables:
- Load/store elimination
- Memory operation reordering
- Register allocation improvements

### 8. Constant Propagation & Folding

**Status**: ✅ Automatic via LLVM

SSA form enables efficient:
- **Constant propagation**: Replaces variables with known constants
- **Constant folding**: Pre-computes constant expressions
- **Sparse Conditional Constant Propagation (SCCP)**: Combines control flow analysis with constant propagation

### 9. Dead Code Elimination

**Status**: ✅ Automatic via LLVM

Removes:
- Unreachable code branches
- Unused function results (with `#[must_use]`)
- Functions with no callers (with LTO)
- Computations whose results are never used

### 10. Control Flow Optimizations

**Status**: ✅ Automatic via LLVM

- **Jump Threading**: Bypasses intermediate branches
- **Tail Merging**: Combines identical instruction sequences
- **Dead Branch Elimination**: Removes provably false conditionals
- **Block Coalescing**: Merges basic blocks where possible

## Optimization Checklist

### Build Configuration
- [x] LTO enabled (`lto = "fat"`)
- [x] Single codegen unit (`codegen-units = 1`)
- [x] High optimization level (`opt-level = 3` or `"z"`)
- [x] Panic strategy set to `abort`
- [x] PGO infrastructure ready
- [x] Debug info stripped for release-boot

### Code Attributes
- [x] `#[inline]` on small hot functions
- [x] `#[inline(never)]` on cold paths
- [x] `#[cold]` on error handlers
- [x] `#[must_use]` on pure functions
- [ ] `#[track_caller]` for better panic messages (future)

### Target Features
- [x] AVX/AVX2/AVX-512 feature gates
- [x] Runtime CPU feature detection
- [x] `reexport_target_features` enabled
- [ ] Dynamic dispatch based on CPUID (future)

### Memory & Cache
- [x] Static relocation model
- [x] Local-exec TLS model
- [x] Kernel code model
- [x] Redzone disabled
- [ ] Cache-aware loop tiling (application-specific)

## Future Improvements

1. **Function Specialization**: Clone functions with constant arguments baked in
2. **Manual Vectorization**: Use explicit SIMD intrinsics where auto-vectorization fails
3. **Cache Line Alignment**: Align hot data structures to cache line boundaries
4. **Prefetching**: Insert software prefetch hints for predictable access patterns
5. **Branch Probability Hints**: Use `likely`/`unlikely` intrinsics for critical branches

## References

- [Rust Compiler Optimization Guide](https://doc.rust-lang.org/book/ch12-00-an-error.html)
- [LLVM Optimization Passes](https://llvm.org/docs/Passes.html)
- [Profile-Guided Optimization](https://doc.rust-lang.org/rustc/profile-guided-optimization.html)
- [Target Features](https://doc.rust-lang.org/reference/attributes/codegen.html#the-target_feature-attribute)
