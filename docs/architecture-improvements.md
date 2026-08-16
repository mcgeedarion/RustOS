# RustOS Architecture Improvement Recommendations

**Generated:** 2026-01-XX  
**Review Scope:** Project structure, architecture patterns, and code organization  
**Current State:** Monolithic kernel with feature-gated build profiles, 19K+ lines in filesystem module, 10K+ lines in process management

---

## Executive Summary

RustOS demonstrates solid foundational architecture with HAL traits, modular subsystems, and thoughtful feature gating. However, several architectural improvements would significantly enhance maintainability, testability, and extensibility:

### Critical Issues
1. **Monolithic filesystem module** (55 files, 19K lines) creates compilation bottlenecks and hinders independent testing
2. **No workspace crate separation** for major subsystems limits code reuse and incremental compilation
3. **Inconsistent error handling** across syscall paths with mixed errno/KernelError patterns
4. **Missing VFS registration system** prevents runtime filesystem driver loading

### High-Priority Recommendations
1. Split `src/fs/` into separate crates under `crates/filesystems/`
2. Create workspace crates for `mm`, `proc`, `vfs`, and `sync`
3. Implement unified error type with automatic errno conversion
4. Add dynamic VFS registration trait for filesystem drivers

---

## 1. Filesystem Refactoring: Split Monolithic Module

### Current State
```
src/fs/
├── mod.rs (55 module declarations)
├── ext4.rs (40,950 lines)
├── fat32.rs (26,291 lines)
├── btrfs/ (directory)
├── ext2/ (directory)
├── vfs.rs
├── mount.rs
└── ... (50+ additional files)
Total: ~19,094 lines across 55 modules
```

**Problems:**
- Single compilation unit bottleneck
- Cannot test filesystems independently
- No clear separation between VFS abstraction and concrete implementations
- Difficult to add new filesystems without touching core code

### Recommended Structure

```
crates/
├── vfs-core/           # Abstract VFS traits and types
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs      # Export VfsOps, VfsNode, MountPoint traits
│       ├── path.rs     # Path resolution logic
│       └── mount.rs    # Mount table management
│
├── filesystems/        # Parent crate for all FS implementations
│   ├── ext4/
│   │   ├── Cargo.toml  # [dependencies] vfs-core = { path = "../vfs-core" }
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── superblock.rs
│   │       ├── inode.rs
│   │       └── block_group.rs
│   │
│   ├── fat32/
│   ├── btrfs/
│   ├── tmpfs/
│   └── procfs/
│
└── rustos-kernel/      # Main kernel crate
    └── src/
        └── fs.rs       # Re-exports: pub use vfs_core::*; pub use filesystems::*;
```

### Implementation Steps

#### Phase 1: Extract VFS Core (Week 1-2)
```rust
// crates/vfs-core/src/lib.rs
#![no_std]

use alloc::sync::Arc;
use crate::path::Path;

pub trait VfsOps: Send + Sync {
    fn name(&self) -> &'static str;
    fn mount(&self, source: Option<&str>, flags: MountFlags) -> KResult<VfsNode>;
    fn lookup(&self, parent: &VfsNode, path: &Path) -> KResult<VfsNode>;
    // ... open, read, write, readdir, etc.
}

pub trait VfsNode: Send + Sync {
    fn metadata(&self) -> KResult<Metadata>;
    fn open(&self, flags: OpenFlags) -> KResult<Arc<dyn VfsFile>>;
    // ...
}

pub struct MountPoint {
    pub root: Arc<dyn VfsNode>,
    pub fs_type: &'static str,
    pub device: Option<String>,
    pub flags: MountFlags,
}
```

#### Phase 2: Migrate Filesystems (Week 3-6)
- Move each filesystem to its own crate
- Update dependencies to reference `vfs-core`
- Maintain backward compatibility via re-exports

#### Phase 3: Build System Updates (Week 7)
```toml
# Workspace Cargo.toml
[workspace]
members = [
    ".",
    "xtask",
    "crates/scheme-api",
    "crates/vfs-core",
    "crates/filesystems/ext4",
    "crates/filesystems/fat32",
    "crates/filesystems/tmpfs",
    # ...
]

# crates/filesystems/ext4/Cargo.toml
[package]
name = "ext4-fs"
version = "0.1.0"
edition = "2021"

[dependencies]
vfs-core = { path = "../../vfs-core" }
spin = { version = "0.9", default-features = false }
```

### Benefits
- ✅ Independent compilation (faster builds)
- ✅ Per-filesystem testing without kernel boot
- ✅ Clear API boundaries via traits
- ✅ Easier to add community-contributed filesystems
- ✅ Reduced coupling between VFS and implementations

---

## 2. Unified Error Handling

### Current State Analysis

**Good:** Central `KernelError` enum exists in `src/core/error.rs` with proper errno mapping.

**Issues Found:**
```rust
// Inconsistent patterns across codebase:

// Pattern 1: Direct KernelError (correct)
fn allocate_pages(count: usize) -> KResult<*mut u8> {
    if count == 0 { return Err(KernelError::InvalidArgument); }
    // ...
}

// Pattern 2: Raw errno returns (problematic)
/// Returns fd on success, negative errno on failure
pub fn sys_open(path: *const c_char, flags: i32) -> i64 {
    // Mixed error handling throughout syscall layer
}

// Pattern 3: Implicit conversions missing
// Some call sites manually convert KernelError to errno
```

### Recommended Solution

#### Step 1: Enhance KernelError with From Implementations
```rust
// src/core/error.rs - additions

impl From<core::alloc::AllocError> for KernelError {
    fn from(_: core::alloc::AllocError) -> Self {
        KernelError::OutOfMemory
    }
}

impl From<core::num::TryFromIntError> for KernelError {
    fn from(_: core::num::TryFromIntError) -> Self {
        KernelError::Overflow
    }
}

// Add filesystem-specific errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    NotAFile,
    NotADirectory,
    FilesystemCorrupted,
    JournalError(Jbd2Error),
    Wrap(KernelError),
}

impl From<FsError> for KernelError {
    fn from(err: FsError) -> Self {
        match err {
            FsError::NotAFile => KernelError::InvalidArgument,
            FsError::NotADirectory => KernelError::InvalidArgument,
            FsError::FilesystemCorrupted => KernelError::IoError,
            FsError::JournalError(_) => KernelError::IoError,
            FsError::Wrap(e) => e,
        }
    }
}
```

#### Step 2: Syscall Wrapper Macro
```rust
// src/syscall/error_wrapper.rs
#[macro_export]
macro_rules! syscall_fn {
    ($name:ident, $f:expr) => {
        pub unsafe extern "C" fn $name(/* args */) -> i64 {
            match $f(/* args */) {
                Ok(result) => result as i64,
                Err(e) => e.to_errno(),
            }
        }
    };
}

// Usage:
syscall_fn!(sys_open, |path, flags, mode| {
    let path = unsafe { CStr::from_ptr(path) }.to_str()?;
    let fd = process.open_file(path, flags, mode)?;
    Ok(fd)
});
```

#### Step 3: Result Type Aliases per Subsystem
```rust
// Each subsystem gets specific error types that convert to KernelError
pub type VfsResult<T> = Result<T, VfsError>;
pub type MmResult<T> = Result<T, MmError>;
pub type ProcResult<T> = Result<T, ProcError>;

// All convert automatically via Into<KernelError>
```

### Migration Path
1. Audit all `-> i64` syscall returns (estimated 50+ functions)
2. Convert to `-> KResult<i64>` or appropriate typed result
3. Apply `syscall_fn!` macro uniformly
4. Remove manual errno conversions

---

## 3. Workspace Crate Separation

### Current Workspace Structure
```toml
[workspace]
members = [
    ".",                    # Entire kernel as single crate
    "xtask",
    "crates/scheme-api",   # Only one extracted crate
]
```

### Recommended Workspace Expansion

```
/workspace
├── Cargo.toml              # Workspace root
├── crates/
│   ├── scheme-api/         # Existing
│   ├── vfs-core/           # NEW: VFS traits
│   ├── mm-core/            # NEW: Memory management traits
│   ├── sync-primitives/    # NEW: Lock abstractions
│   └── filesystems/        # NEW: FS implementations
│       ├── ext4/
│       ├── fat32/
│       └── ...
│
└── src/                    # Kernel binary only
    ├── lib.rs              # Re-exports from crates
    └── main.rs
```

### New Crate Definitions

#### `crates/mm-core/Cargo.toml`
```toml
[package]
name = "mm-core"
version = "0.1.0"
edition = "2021"

[dependencies]
spin = { version = "0.9", default-features = false }

[features]
default = []
kasan = []
```

#### `crates/mm-core/src/lib.rs`
```rust
#![no_std]

pub trait PhysicalMemoryManager {
    fn allocate_block(&self) -> Option<PhysAddr>;
    fn allocate_blocks(&self, count: usize) -> Option<Vec<PhysAddr>>;
    fn free_block(&self, addr: PhysAddr);
}

pub trait VirtualMemoryManager {
    fn map(&self, vaddr: VirtAddr, paddr: PhysAddr, flags: MapFlags) -> KResult<()>;
    fn unmap(&self, vaddr: VirtAddr, size: usize) -> KResult<()>;
    fn protect(&self, vaddr: VirtAddr, size: usize, flags: ProtFlags) -> KResult<()>;
}

pub struct MemoryPolicy {
    pub numa_node: u8,
    pub preferred_zone: ZoneType,
    pub reclaim_priority: u8,
}
```

#### `crates/sync-primitives/Cargo.toml`
```toml
[package]
name = "sync-primitives"
version = "0.1.0"
edition = "2021"

[dependencies]
spin = { version = "0.9", default-features = false }

[features]
default = ["spin_mutex"]
queued_spinlock = []
rcu = []
```

### Benefits
- **Faster CI:** Changed crates rebuild independently
- **Documentation:** Per-crate rustdoc generation
- **Testing:** Unit tests run without QEMU
- **Reusability:** Crates usable in userspace tools
- **Clear APIs:** Public interfaces enforced by crate boundaries

---

## 4. VFS Registration System

### Current Limitation
Filesystems are statically linked with no runtime discovery mechanism.

### Proposed Design

```rust
// crates/vfs-core/src/registry.rs

use alloc::vec::Vec;
use spin::Mutex;

pub struct FilesystemRegistry {
    registered: Mutex<Vec<&'static dyn FileSystemDriver>>,
}

pub trait FileSystemDriver: Send + Sync {
    fn name(&self) -> &'static str;
    fn probe(&self, data: &[u8]) -> bool;  // Superblock magic check
    fn mount(&self, device: &dyn BlockDevice, options: MountOptions) 
        -> KResult<Arc<dyn VfsOps>>;
}

impl FilesystemRegistry {
    pub const fn new() -> Self {
        Self { registered: Mutex::new(Vec::new()) }
    }
    
    pub fn register(&self, driver: &'static dyn FileSystemDriver) {
        self.registered.lock().push(driver);
    }
    
    pub fn get_by_name(&self, name: &str) -> Option<&dyn FileSystemDriver> {
        self.registered.lock().iter().find(|d| d.name() == name).copied()
    }
    
    pub fn auto_detect(&self, data: &[u8]) -> Option<&dyn FileSystemDriver> {
        self.registered.lock().iter().find(|d| d.probe(data)).copied()
    }
}

// Global registry
pub static VFS_REGISTRY: FilesystemRegistry = FilesystemRegistry::new();

// Registration macro
#[macro_export]
macro_rules! register_filesystem {
    ($driver:expr) => {
        #[ctor::ctor]
        fn __register_filesystem() {
            $crate::registry::VFS_REGISTRY.register($driver);
        }
    };
}

// Usage in ext4 crate
// crates/filesystems/ext4/src/lib.rs
pub struct Ext4Driver;

impl FileSystemDriver for Ext4Driver {
    fn name(&self) -> &'static str { "ext4" }
    fn probe(&self, data: &[u8]) -> bool { /* check ext4 magic */ }
    fn mount(&self, device: &dyn BlockDevice, options: MountOptions) 
        -> KResult<Arc<dyn VfsOps>> { /* ... */ }
}

register_filesystem!(&Ext4Driver);
```

### Mount Command Integration
```rust
// src/fs/mount.rs
pub fn mount(
    source: &str,
    target: &Path,
    fs_type: Option<&str>,
    flags: MountFlags,
    options: MountOptions,
) -> KResult<()> {
    let driver = match fs_type {
        Some(name) => VFS_REGISTRY.get_by_name(name)
            .ok_or(KernelError::NotFound)?,
        None => {
            // Auto-detect from superblock
            let superblock = read_superblock(source)?;
            VFS_REGISTRY.auto_detect(&superblock)
                .ok_or(KernelError::InvalidArgument)?
        }
    };
    
    let device = get_block_device(source)?;
    let vfs_ops = driver.mount(device.as_ref(), options)?;
    
    MOUNT_TABLE.lock().insert(target.clone(), MountPoint {
        root: vfs_ops.root()?,
        device,
        driver,
    });
    
    Ok(())
}
```

---

## 5. Process Builder Pattern

### Current State
Process creation scattered across `fork.rs`, `clone.rs`, `exec.rs` with inconsistent parameter passing.

### Proposed Builder Pattern

```rust
// src/proc/process_builder.rs

pub struct ProcessBuilder<'a> {
    executable: &'a Path,
    arguments: Vec<&'a str>,
    environment: Vec<(&'a str, &'a str)>,
    working_dir: Option<&'a Path>,
    credentials: Option<Credentials>,
    namespaces: NamespaceSet,
    file_descriptors: FdTable,
    resource_limits: ResourceLimits,
    personality: Personality,
    flags: CloneFlags,
}

impl<'a> ProcessBuilder<'a> {
    pub fn new(executable: &'a Path) -> Self {
        Self {
            executable,
            arguments: Vec::new(),
            environment: current_env(),
            working_dir: None,
            credentials: None,
            namespaces: NamespaceSet::default(),
            file_descriptors: FdTable::inherit_current(),
            resource_limits: ResourceLimits::inherit(),
            personality: Personality::native(),
            flags: CloneFlags::empty(),
        }
    }
    
    pub fn arg<S: AsRef<str>>(mut self, arg: S) -> Self {
        self.arguments.push(arg.as_ref());
        self
    }
    
    pub fn args<I, S>(mut self, args: I) -> Self 
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.arguments.extend(args.into_iter().map(|s| s.as_ref()));
        self
    }
    
    pub fn env<K, V>(mut self, key: K, value: V) -> Self 
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.environment.push((key.as_ref(), value.as_ref()));
        self
    }
    
    pub fn cwd(mut self, path: &'a Path) -> Self {
        self.working_dir = Some(path);
        self
    }
    
    pub fn credentials(mut self, creds: Credentials) -> Self {
        self.credentials = Some(creds);
        self
    }
    
    pub fn new_namespace(mut self, ns_type: NamespaceType) -> Self {
        self.namespaces.set_new(ns_type);
        self
    }
    
    pub fn stdin_fd(mut self, fd: RawFd) -> Self {
        self.file_descriptors.set(0, fd);
        self
    }
    
    pub fn stdout_fd(mut self, fd: RawFd) -> Self {
        self.file_descriptors.set(1, fd);
        self
    }
    
    pub fn stderr_fd(mut self, fd: RawFd) -> Self {
        self.file_descriptors.set(2, fd);
        self
    }
    
    pub fn spawn(self) -> KResult<Pid> {
        let task = Task::new(
            self.executable,
            self.arguments,
            self.environment,
            self.credentials.unwrap_or_else(Credentials::current),
            self.namespaces,
            self.file_descriptors,
            self.resource_limits,
        )?;
        
        if let Some(cwd) = self.working_dir {
            task.set_cwd(cwd)?;
        }
        
        task.apply_personality(self.personality);
        
        let pid = task.pid();
        schedule_task(task);
        
        Ok(pid)
    }
    
    pub fn exec(self) -> KResult<()> {
        // For current process replacement
        let task = current_task();
        task.execve(
            self.executable,
            self.arguments,
            self.environment,
        )?;
        Ok(())
    }
}

// Usage examples:

// Simple process spawn
let pid = ProcessBuilder::new("/bin/sh")
    .arg("-c")
    .arg("echo hello")
    .spawn()?;

// Process with redirected I/O
let pid = ProcessBuilder::new("/usr/bin/cat")
    .arg("/etc/passwd")
    .stdin_fd(null_fd)
    .stdout_fd(log_fd)
    .stderr_fd(log_fd)
    .credentials(service_creds)
    .new_namespace(NamespaceType::Network)
    .spawn()?;

// Current process exec
ProcessBuilder::new("/sbin/init")
    .arg("--system")
    .env("PATH", "/usr/bin:/bin")
    .exec()?;
```

### Integration Points
- Replace direct `fork()` calls with builder
- Unify `clone()` flags through builder API
- Simplify `execve()` parameter validation

---

## 6. Memory Policy Abstraction for NUMA

### Current State
NUMA node tracking exists in PMM but no policy abstraction.

### Recommended Design

```rust
// src/mm/policy.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocationPolicy {
    /// Allocate from any node (default)
    Interleave,
    
    /// Prefer specified node, fallback to others
    Preferred(u8),
    
    /// Strictly allocate from specified nodes
    Bind(NodeSet),
    
    /// Round-robin across nodes
    NodeInterleave(NodeSet),
    
    /// First-touch: allocate on faulting node
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReclaimPolicy {
    /// Aggressive reclaim under pressure
    Aggressive,
    /// Balanced approach
    Balanced,
    /// Minimize reclaim, prefer OOM
    Conservative,
}

pub struct MemoryPolicy {
    pub allocation: AllocationPolicy,
    pub reclaim: ReclaimPolicy,
    pub zone_filter: ZoneFilter,
    pub migrate_on_fault: bool,
}

impl MemoryPolicy {
    pub const DEFAULT: Self = Self {
        allocation: AllocationPolicy::Interleave,
        reclaim: ReclaimPolicy::Balanced,
        zone_filter: ZoneFilter::All,
        migrate_on_fault: true,
    };
    
    pub fn for_kernel() -> Self {
        Self {
            allocation: AllocationPolicy::Preferred(0),
            reclaim: ReclaimPolicy::Conservative,
            zone_filter: ZoneFilter::Dma32 | ZoneFilter::Normal,
            migrate_on_fault: false,
        }
    }
    
    pub fn for_userspace() -> Self {
        Self {
            allocation: AllocationPolicy::Local,
            reclaim: ReclaimPolicy::Balanced,
            zone_filter: ZoneFilter::All,
            migrate_on_fault: true,
        }
    }
}

// Integration with allocator
impl Allocator {
    pub fn allocate_with_policy(&self, size: usize, policy: &MemoryPolicy) 
        -> KResult<*mut u8> 
    {
        let nodes = match policy.allocation {
            AllocationPolicy::Interleave => NodeSet::all(),
            AllocationPolicy::Preferred(node) => NodeSet::single(node),
            AllocationPolicy::Bind(nodes) => nodes,
            AllocationPolicy::NodeInterleave(nodes) => nodes,
            AllocationPolicy::Local => NodeSet::current_node(),
        };
        
        for node in nodes.iter() {
            if let Some(addr) = self.allocate_from_node(size, node) {
                return Ok(addr);
            }
        }
        
        // Fallback based on reclaim policy
        if policy.reclaim == ReclaimPolicy::Aggressive {
            self.reclaim_and_allocate(size, &nodes)?;
        }
        
        Err(KernelError::OutOfMemory)
    }
}

// Per-process memory policy
pub struct TaskMemoryPolicy {
    inherit_from_parent: bool,
    policy: MemoryPolicy,
    mems_allowed: NodeSet,
    mems_preferred: NodeSet,
}

impl Task {
    pub fn set_mempolicy(&mut self, policy: MemoryPolicy) -> KResult<()> {
        self.memory_policy.policy = policy;
        Ok(())
    }
    
    pub fn get_mempolicy(&self) -> &MemoryPolicy {
        &self.memory_policy.policy
    }
}
```

---

## 7. Queued Spinlocks and RCU

### Current State
Only basic `spin::Mutex` and `spin::RwLock` in use.

### Queued Spinlock Implementation

```rust
// crates/sync-primitives/src/qspinlock.rs

use core::sync::atomic::{AtomicU32, Ordering};
use core::hint::spin_loop;

const _Q_LOCKED: u32 = 1 << 0;
const _Q_PENDING: u32 = 1 << 1;
const _Q_HEAD_MASK: u32 = 0x0000FF00;
const _Q_TAIL_MASK: u32 = 0xFFFF0000;

#[derive(Debug)]
pub struct QueueNode {
    next: AtomicU32,
    locked: AtomicU32,
}

impl QueueNode {
    const fn new() -> Self {
        Self {
            next: AtomicU32::new(0),
            locked: AtomicU32::new(0),
        }
    }
}

pub struct QSpinLock {
    val: AtomicU32,
}

impl QSpinLock {
    pub const fn new() -> Self {
        Self { val: AtomicU32::new(0) }
    }
    
    pub fn lock(&self, node: &QueueNode) {
        let mut val = self.val.load(Ordering::Relaxed);
        
        // Fast path: uncontended
        if val == 0 {
            if self.val.compare_exchange_weak(0, _Q_LOCKED, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                return;
            }
            val = self.val.load(Ordering::Relaxed);
        }
        
        // Slow path: queue
        let tail = self.encode_tail(cpu_id(), node_index(node));
        let old = self.val.fetch_or(tail | _Q_PENDING, Ordering::Relaxed);
        
        // Wait for our turn
        while !self.is_head(old, tail) {
            spin_loop();
            old = self.val.load(Ordering::Relaxed);
        }
        
        // Acquire lock
        while self.val.swap(_Q_LOCKED, Ordering::Acquire) & _Q_PENDING != 0 {
            spin_loop();
        }
    }
    
    pub fn unlock(&self) {
        self.val.store(0, Ordering::Release);
    }
    
    fn encode_tail(&self, cpu: u8, index: u8) -> u32 {
        ((cpu as u32) << 8) | ((index as u32) << 2)
    }
    
    fn is_head(&self, val: u32, tail: u32) -> bool {
        (val & _Q_HEAD_MASK) == ((tail << 8) & _Q_HEAD_MASK)
    }
}

unsafe impl Send for QSpinLock {}
unsafe impl Sync for QSpinLock {}
```

### RCU Implementation

```rust
// crates/sync-primitives/src/rcu.rs

use core::sync::atomic::{AtomicUsize, Ordering};
use core::ptr::NonNull;

pub struct RcuHead {
    next: AtomicUsize,
}

pub struct RcuState {
    grace_period: AtomicUsize,
    callbacks: AtomicUsize,
}

impl RcuState {
    pub const fn new() -> Self {
        Self {
            grace_period: AtomicUsize::new(0),
            callbacks: AtomicUsize::new(0),
        }
    }
}

pub fn rcu_read_lock() -> RcuReadGuard {
    RcuReadGuard::new()
}

pub struct RcuReadGuard {
    _marker: PhantomData<()>,
}

impl RcuReadGuard {
    fn new() -> Self {
        Self { _marker: PhantomData }
    }
}

impl Drop for RcuReadGuard {
    fn drop(&mut self) {
        // Reader-side cleanup if needed
    }
}

pub unsafe fn rcu_dereference<T>(ptr: *const T) -> *const T {
    core::ptr::read_volatile(&ptr)
}

pub fn synchronize_rcu(state: &RcuState) {
    let gp = state.grace_period.load(Ordering::Relaxed);
    state.grace_period.store(gp + 1, Ordering::Release);
    
    // Wait for all readers to complete
    // (implementation depends on CPU tracking)
    while readers_active() {
        spin_loop();
    }
}

pub fn call_rcu(head: &mut RcuHead, callback: unsafe fn(&mut RcuHead)) {
    // Queue callback for after grace period
}

// Usage example:
struct MyData {
    value: i32,
    rcu: RcuHead,
}

unsafe fn update_data(ptr: *mut MyData, new_value: i32) {
    let new_data = Box::new(MyData { value: new_value, rcu: RcuHead { next: AtomicUsize::new(0) } });
    
    let old_ptr = core::ptr::replace(&mut (*ptr), *new_data);
    
    synchronize_rcu(&RCU_STATE);
    
    // Safe to free old_data now
    drop(Box::from_raw(old_ptr));
}
```

---

## 8. Property-Based Testing Framework

### Current Gap
No property-based testing infrastructure.

### Recommended Setup

```rust
// crates/test-helpers/src/property.rs
// Requires proptest or quickcheck adapted for no_std

use crate::arbitrary::*;

pub trait ArbitraryKernel {
    type Strategy;
    fn arbitrary_kernel() -> Self::Strategy;
}

// Example property tests for MM subsystem
#[cfg(test)]
mod mm_properties {
    use super::*;
    
    #[test]
    fn alloc_free_identity() {
        proptest!(|(size in 1..4096usize)| {
            let ptr = allocator.allocate(size)?;
            assert!(!ptr.is_null());
            
            allocator.free(ptr, size);
            
            // Should be able to reallocate same size
            let ptr2 = allocator.allocate(size)?;
            prop_assert!(!ptr2.is_null());
        });
    }
    
    #[test]
    fn page_alignment_property() {
        proptest!(|(size in 1..(PAGE_SIZE * 16) as usize)| {
            let ptr = allocator.allocate(size)?;
            prop_assert_eq!(ptr as usize % PAGE_SIZE, 0);
        });
    }
    
    #[test]
    fn virtual_memory_map_unmap_roundtrip(vaddr: VirtAddr, paddr: PhysAddr) {
        vmm.map(vaddr, paddr, MapFlags::RW)?;
        
        let mapped_paddr = vmm.translate(vaddr)?;
        prop_assert_eq!(mapped_paddr, paddr);
        
        vmm.unmap(vaddr, PAGE_SIZE)?;
        
        prop_assert!(vmm.translate(vaddr).is_err());
    }
}
```

### Integration with xtask
```rust
// xtask/src/main.rs - additions
enum TestCommand {
    Unit,          // cargo test
    Property,      // cargo test --features proptest
    Integration,   // QEMU-based
    FaultInjection,// With fault-inject feature
}

fn run_tests(cmd: TestCommand) -> Result<()> {
    match cmd {
        TestCommand::Property => {
            cargo_test(&["--features", "proptest,kmtest"])?;
        }
        // ...
    }
}
```

---

## 9. Comprehensive ADR Documentation

### Current State
Only `docs/architecture.md` exists with high-level overview.

### Recommended ADR Structure

```
docs/adr/
├── README.md                 # Index of all ADRs
├── template.md               # ADR template
├── 0001-use-rust-for-kernel.md
├── 0002-uefi-boot-strategy.md
├── 0003-feature-gated-build-profiles.md
├── 0004-vfs-trait-design.md          # NEW
├── 0005-workspace-crate-separation.md # NEW
├── 0006-error-handling-strategy.md    # NEW
├── 0007-numa-memory-policy.md         # NEW
└── 0008-concurrency-primitives.md     # NEW
```

#### ADR Template (`docs/adr/template.md`)
```markdown
# ADR NNNN: Title

**Status:** Proposed | Accepted | Deprecated | Superseded by ADR NNNN  
**Date:** YYYY-MM-DD  
**Deciders:** @maintainers  
**Technical Story:** [Link to issue/PR]

## Context and Problem Statement

[Describe the context, problem, and constraints]

## Decision Drivers

- Performance
- Maintainability
- Safety
- Compatibility
- Development velocity

## Considered Options

1. Option A
2. Option B
3. Option C

## Decision Outcome

Chosen option: "[Option X]", because [justification].

### Positive Consequences

- Benefit 1
- Benefit 2

### Negative Consequences

- Trade-off 1
- Trade-off 2

## Pros and Cons of the Options

### Option A

Good:
- Pro 1
- Pro 2

Bad:
- Con 1
- Con 2

### Option B

[Similar structure]

## Links

- [Implementation PR](link)
- [Related ADRs](link)
```

#### Example ADR (`docs/adr/0005-workspace-crate-separation.md`)
```markdown
# ADR 0005: Workspace Crate Separation

**Status:** Proposed  
**Date:** 2026-01-XX  
**Deciders:** Kernel Architecture Team

## Context and Problem Statement

The current monolithic kernel crate (19K+ lines in fs/, 10K+ in proc/) creates:
- Long compilation times (full rebuild on any change)
- Difficulty in testing subsystems independently
- Unclear API boundaries between components
- Barriers to community contributions

## Decision Drivers

- Build performance (CI time < 10 minutes)
- Testability (unit tests without QEMU)
- Modularity (clear separation of concerns)
- Backwards compatibility (minimal breaking changes)

## Considered Options

1. Keep monolithic structure
2. Split into workspace crates
3. Use Rust modules with internal visibility

## Decision Outcome

Chosen option: "Split into workspace crates"

Create separate crates for:
- `vfs-core`: VFS traits and types
- `mm-core`: Memory management abstractions
- `sync-primitives`: Lock implementations
- `filesystems/*`: Individual filesystem implementations

### Positive Consequences

- Incremental compilation (only changed crates rebuild)
- Per-crate documentation and testing
- Clear public APIs via crate boundaries
- Reusable components for userspace tools

### Negative Consequences

- Initial refactoring effort (~6 weeks)
- Slightly more complex dependency graph
- Need for careful versioning strategy

## Pros and Cons

### Option 1: Keep Monolithic

Good:
- Simple structure
- No refactoring needed

Bad:
- Compilation bottleneck
- Poor testability
- Unclear boundaries

### Option 2: Workspace Crates (CHOSEN)

Good:
- Faster builds
- Better testing
- Clear APIs
- Community-friendly

Bad:
- Refactoring overhead
- Dependency complexity

### Option 3: Internal Modules

Good:
- Less restructuring

Bad:
- No compilation benefit
- Weak encapsulation

## Implementation Plan

Phase 1 (Week 1-2): Extract vfs-core  
Phase 2 (Week 3-6): Migrate filesystems  
Phase 3 (Week 7): Update build system  
Phase 4 (Week 8): Documentation and testing

## Links

- Issue: #XXX - Workspace crate planning
- Related: ADR 0003 (Feature-gated builds)
```

---

## 10. Enhanced Testing Infrastructure with QEMU Integration

### Current State
Basic QEMU launch in xtask, limited test automation.

### Recommended Improvements

#### Structured Test Harness
```rust
// xtask/src/test.rs

pub struct QemuTestSuite {
    arch: Arch,
    features: Vec<String>,
    timeout_secs: u64,
    log_path: PathBuf,
}

impl QemuTestSuite {
    pub fn new(arch: Arch) -> Self {
        Self {
            arch,
            features: vec!["kmtest".to_string()],
            timeout_secs: 300,
            log_path: PathBuf::from("target/test.log"),
        }
    }
    
    pub fn with_features(mut self, features: Vec<String>) -> Self {
        self.features = features;
        self
    }
    
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
    
    pub fn run(&self, tests: &[&str]) -> TestResult {
        // Build kernel with test features
        self.build_kernel()?;
        
        // Launch QEMU with serial capture
        let mut qemu = self.launch_qemu()?;
        
        // Parse test output from serial
        let mut parser = TestOutputParser::new();
        let start = Instant::now();
        
        while start.elapsed().as_secs() < self.timeout_secs {
            let line = qemu.read_serial_line()?;
            
            if let Some(test_event) = parser.parse(&line) {
                match test_event {
                    TestEvent::Start(name) => { /* record start */ }
                    TestEvent::Pass(name) => { /* record pass */ }
                    TestEvent::Fail(name, reason) => { /* record fail */ }
                    TestEvent::Complete(summary) => { return Ok(summary); }
                }
            }
        }
        
        Err(TestError::Timeout)
    }
}

pub enum TestEvent {
    Start(String),
    Pass(String),
    Fail(String, String),
    Complete(TestSummary),
}

pub struct TestSummary {
    total: usize,
    passed: usize,
    failed: usize,
    skipped: usize,
}
```

#### Fault Injection Testing
```rust
// src/fault_inject/test_cases.rs

#[cfg(feature = "fault-inject")]
mod mm_fault_tests {
    use crate::fault_inject::set_injection_point;
    
    #[kmtest]
    fn pmm_oom_handling() {
        // Inject failure at PMM allocation point #3
        set_injection_point(PMM_ALLOC_BLOCK, 3, true);
        
        let result = allocate_pages(10);
        assert!(matches!(result, Err(KernelError::OutOfMemory)));
        
        // Clear injection
        set_injection_point(PMM_ALLOC_BLOCK, 3, false);
        
        // Verify normal operation resumes
        let ptr = allocate_pages(10);
        assert!(ptr.is_ok());
    }
    
    #[kmtest]
    fn vmm_map_failure_recovery() {
        set_injection_point(VMM_MAP_REGION, 1, true);
        
        let result = kernel_map(VirtAddr::new(0x1000), PhysAddr::new(0x2000));
        assert!(result.is_err());
        
        // Verify no leaked mappings
        assert_eq!(get_mapping_count(), expected_count);
    }
}

#[cfg(feature = "fault-inject")]
mod syscall_fault_tests {
    #[kmtest]
    fn open_emfile_injection() {
        set_injection_point(SYS_OPEN_CHECK_LIMITS, 0, true);
        
        let fd = sys_open(b"/dev/null\0".as_ptr() as _, O_RDONLY, 0);
        assert_eq!(fd, -EMFILE as i64);
    }
}
```

#### CI Integration Script
```bash
#!/bin/bash
# scripts/ci/run_tests.sh

set -e

echo "=== Running RustOS Test Suite ==="

# Unit tests (host)
cargo test --lib --features kmtest

# Kernel tests in QEMU
for arch in x86_64 aarch64; do
    echo "Testing $arch..."
    
    # Basic boot test
    cargo xtask smoke --arch $arch
    
    # Kernel module tests
    cargo xtask test --arch $arch --features "kmtest,fault-inject"
    
    # Property-based tests
    cargo xtask test --arch $arch --features "kmtest,proptest"
    
    # Fault injection stress test
    for i in {1..10}; do
        cargo xtask test --arch $arch \
            --features "kmtest,fault-inject" \
            --random-seed $RANDOM
    done
done

echo "=== All Tests Complete ==="
```

---

## 11. Build-Time Configuration Validation

### Current Gap
No validation of feature combinations or configuration consistency.

### Recommended Implementation

```rust
// build.rs - enhancements

use std::process;

fn main() {
    validate_feature_combinations();
    validate_architecture_config();
    check_required_tools();
    
    // Existing build logic...
    compile_crt();
}

fn validate_feature_combinations() {
    let boot_minimal = cfg!(feature = "boot_minimal");
    let userspace_boot = cfg!(feature = "userspace_boot");
    let uefi_boot = cfg!(feature = "uefi_boot");
    let full_kernel = !boot_minimal && !userspace_boot;
    
    // Validate mutually exclusive features
    if cfg!(feature = "gdbstub") && cfg!(feature = "release-boot") {
        panic!("Cannot enable gdbstub in release-boot profile");
    }
    
    // Validate required dependencies
    if cfg!(feature = "amdgpu") && !cfg!(feature = "input_events") {
        eprintln!("warning: amdgpu feature typically requires input_events");
    }
    
    // Warn about potentially problematic combinations
    if full_kernel && cfg!(target_arch = "aarch64") {
        eprintln!("warning: full kernel on aarch64 is experimental");
    }
    
    println!("cargo:rerun-if-env-changed=RUSTOS_FEATURES");
}

fn validate_architecture_config() {
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    
    match arch.as_str() {
        "x86_64" => {
            if !std::path::Path::new("linker/x86_64.ld").exists() {
                panic!("Missing x86_64 linker script");
            }
        }
        "aarch64" => {
            if !std::path::Path::new("linker/aarch64.ld").exists() {
                panic!("Missing aarch64 linker script");
            }
        }
        other => {
            panic!("Unsupported architecture: {}", other);
        }
    }
}

fn check_required_tools() {
    // Check for cc (C compiler)
    if let Err(_) = std::process::Command::new("cc").arg("--version").output() {
        panic!("C compiler (cc) not found in PATH");
    }
    
    // Check for required environment variables in CI
    if std::env::var("CI").is_ok() {
        if std::env::var("RUSTFLAGS").is_err() {
            eprintln!("warning: RUSTFLAGS not set in CI environment");
        }
    }
}
```

#### Configuration Schema File
```yaml
# .rustos-config.yaml
architecture:
  supported:
    - x86_64
    - aarch64
  
  defaults:
    x86_64: uefi_boot
    aarch64: boot_minimal

features:
  mutually_exclusive:
    - [release-boot, debug]
    - [boot_minimal, kmtest]
  
  required_together:
    - [amdgpu, input_events]
    - [gdbstub, trace]
  
  warnings:
    - feature: full_kernel
      on_arch: aarch64
      message: "Full kernel on aarch64 is experimental"

validation:
  max_boot_image_size: 16MB  # bytes
  required_sections:
    - .text
    - .rodata
    - .data
    - .bss
```

#### Pre-commit Hook
```bash
#!/bin/bash
# .git/hooks/pre-commit

echo "Validating RustOS configuration..."

# Run build.rs validation
cargo check --features boot_minimal --target x86_64-kernel.json 2>&1 | \
    grep -E "(error|panic)" && exit 1

# Check for common mistakes
if grep -r "TODO\|FIXME\|XXX" src/ --include="*.rs" | \
   grep -v "// TODO:" | grep -v "/// FIXME:"; then
    echo "Warning: Found unformatted TODO/FIXME comments"
fi

# Verify no dbg! macros in non-debug code
if grep -r "dbg!(" src/ --include="*.rs" | grep -v "#\[cfg(debug_assertions)\]"; then
    echo "Error: dbg! macro found outside debug code"
    exit 1
fi

echo "Configuration validation passed"
```

---

## Implementation Roadmap

### Phase 1: Foundation (Weeks 1-4)
- [ ] Extract `vfs-core` crate
- [ ] Implement unified error handling
- [ ] Create ADR documentation structure
- [ ] Add build-time validation

### Phase 2: Modularization (Weeks 5-10)
- [ ] Migrate filesystems to separate crates
- [ ] Create `mm-core` and `sync-primitives` crates
- [ ] Implement VFS registration system
- [ ] Develop process builder pattern

### Phase 3: Advanced Features (Weeks 11-16)
- [ ] Implement queued spinlocks
- [ ] Add RCU support
- [ ] Develop memory policy abstraction
- [ ] Create property-based testing framework

### Phase 4: Polish (Weeks 17-20)
- [ ] Enhance QEMU test integration
- [ ] Complete ADR documentation
- [ ] Optimize build pipeline
- [ ] Performance benchmarking

---

## Risk Assessment

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Breaking changes during refactoring | High | Medium | Feature flags, gradual migration |
| Performance regression from abstraction | Medium | Low | Benchmark-driven development |
| Increased complexity from crates | Low | Medium | Clear documentation, simple APIs |
| Timeline slippage | Medium | Medium | Phased approach, MVP first |

---

## Success Metrics

- **Build Time:** Reduce full rebuild from X min to Y min
- **Test Coverage:** Increase from X% to Y%
- **Compilation Units:** Split 19K line fs module into 10+ crates
- **API Clarity:** Document all public interfaces via rustdoc
- **Contributor Onboarding:** Reduce setup time from X days to Y days

---

## Conclusion

These architectural improvements will transform RustOS from a monolithic kernel into a modular, maintainable, and extensible operating system. The phased approach minimizes risk while delivering incremental value. Priority should be given to filesystem refactoring and workspace crate separation as they provide the foundation for all other improvements.

**Next Steps:**
1. Review and approve this recommendation document
2. Create GitHub issues for each improvement
3. Begin Phase 1 implementation
4. Establish weekly architecture review meetings
