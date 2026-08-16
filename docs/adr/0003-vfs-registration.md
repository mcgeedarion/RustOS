# ADR 0003: VFS Runtime Registration System

## Status
Accepted

## Context
Filesystem support in RustOS was compile-time only:
- Filesystems hardcoded into VFS layer
- No ability to add filesystems at runtime
- Difficult to support loadable filesystem modules
- Auto-detection of filesystem types not available

## Decision
Implement a runtime VFS registration system:

### Design
```rust
pub struct VfsRegistry {
    filesystems: spin::Mutex<BTreeMap<&'static str, &'static dyn FileSystem>>,
}

impl VfsRegistry {
    pub fn register(&self, name: &'static str, fs: &'static dyn FileSystem) -> Result<(), VfsError>;
    pub fn get(&self, name: &str) -> Option<&'static dyn FileSystem>;
    pub fn auto_detect(&self, data: &[u8]) -> Option<&'static str>;
}
```

### Filesystem Trait
All filesystems implement the `FileSystem` trait:
```rust
pub trait FileSystem: Send + Sync {
    fn name(&self) -> &'static str;
    fn open(&self, path: &str, flags: OpenFlags) -> Result<FileHandle, VfsError>;
    fn stat(&self, path: &str) -> Result<Stat, VfsError>;
    // ... other operations
}
```

### Auto-Detection
Magic number detection for common filesystems:
- EXT2/3/4: 0x53EF at offset 1080
- FAT32: 0xEB5890 boot signature
- NTFS: "NTFS" at offset 3

### Usage
```rust
// Register filesystem at initialization
pub fn register_ext4() -> Result<(), VfsError> {
    static EXT4_FS: Ext4FileSystem = Ext4FileSystem::new();
    vfs_core::VFS_REGISTRY.register("ext4", &EXT4_FS)
}

// Auto-detect and mount
let fs_type = vfs_core::VFS_REGISTRY.auto_detect(superblock_data);
if let Some(name) = fs_type {
    if let Some(fs) = vfs_core::VFS_REGISTRY.get(name) {
        // Mount using detected filesystem
    }
}
```

## Consequences
### Positive
- Runtime filesystem loading capability
- Cleaner separation between VFS and implementations
- Auto-detection simplifies mount operations
- Easier to add new filesystems

### Negative
- Small runtime overhead from dynamic dispatch
- Requires 'static lifetime for filesystem instances

## Implementation
See `crates/vfs-core/src/lib.rs` for the complete implementation.
