# ADR 0001: Workspace Crate Architecture

## Status
Accepted

## Context
The RustOS kernel had grown into a monolithic structure with:
- 55+ files in src/fs/ totaling ~19K lines
- Tightly coupled subsystems making independent testing difficult
- No clear separation between core abstractions and implementations
- Difficulty in reusing components across different build profiles

## Decision
We will restructure the codebase into a Cargo workspace with separate crates:

### New Workspace Crates
1. **vfs-core**: Core VFS traits, error types, and registration system
2. **fs-ext4**: EXT4 filesystem implementation
3. **fs-fat32**: FAT32 filesystem implementation  
4. **fs-tmpfs**: Tmpfs implementation
5. **mm-core**: Memory management abstractions with NUMA support
6. **sync-primitives**: Advanced synchronization (queued spinlocks, RCU)

### Benefits
- Clear separation between interfaces and implementations
- Independent compilation and testing of subsystems
- Reduced compile times through incremental builds
- Better encapsulation of implementation details
- Easier to swap filesystem implementations

### Migration Strategy
1. Phase 1: Create new crate skeletons with minimal APIs
2. Phase 2: Migrate core traits to vfs-core
3. Phase 3: Move filesystem implementations incrementally
4. Phase 4: Update kernel to use workspace crates
5. Phase 5: Remove legacy monolithic modules

## Consequences
### Positive
- Improved modularity and maintainability
- Faster incremental builds
- Clearer dependency boundaries
- Better testability

### Negative
- Initial migration effort required
- Some code duplication during transition
- Need to maintain backward compatibility shims

### Risks
- Breaking existing functionality during migration
- Performance regression from additional abstraction layers

## Mitigation
- Comprehensive test suite before and after migration
- Performance benchmarks to catch regressions
- Gradual migration with feature flags
