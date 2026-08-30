//! VFS Core - Virtual Filesystem abstractions for RustOS
//!
//! This crate provides the core traits, types, and abstractions for the
//! virtual filesystem layer. Filesystem implementations should implement
//! these traits to integrate with the VFS.

#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

pub use bitflags::bitflags;
pub use thiserror::Error;

/// VFS error types with automatic errno conversion
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    #[error("File not found")]
    NotFound = 2,
    
    #[error("Permission denied")]
    PermissionDenied = 13,
    
    #[error("File exists")]
    AlreadyExists = 17,
    
    #[error("Not a directory")]
    NotADirectory = 20,
    
    #[error("Is a directory")]
    IsADirectory = 21,
    
    #[error("Invalid argument")]
    InvalidArg = 22,
    
    #[error("Too many open files")]
    TooManyOpenFiles = 24,
    
    #[error("Read-only file system")]
    ReadOnly = 30,
    
    #[error("Operation not supported")]
    NotSupported = 95,
    
    #[error("Out of memory")]
    OutOfMemory = 12,
    
    #[error("I/O error")]
    Io = 5,
    
    #[error("Resource temporarily unavailable")]
    WouldBlock = 11,
    
    #[error("Bad file descriptor")]
    BadFd = 9,
    
    #[error("File too large")]
    FileTooLarge = 27,
    
    #[error("No space left on device")]
    NoSpace = 28,
    
    #[error("Cross-device link")]
    CrossDevice = 18,
    
    #[error("Directory not empty")]
    NotEmpty = 39,
}

impl From<VfsError> for isize {
    fn from(err: VfsError) -> Self {
        err as isize
    }
}

impl From<VfsError> for i32 {
    fn from(err: VfsError) -> Self {
        err as i32
    }
}

/// File metadata information
#[derive(Debug, Clone)]
pub struct Stat {
    pub inode: u64,
    pub size: usize,
    pub mode: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub blksize: usize,
    pub blocks: usize,
}

impl Stat {
    pub fn is_dir(&self) -> bool {
        (self.mode & 0o170000) == 0o040000
    }
    
    pub fn is_file(&self) -> bool {
        (self.mode & 0o170000) == 0o100000
    }
    
    pub fn is_symlink(&self) -> bool {
        (self.mode & 0o170000) == 0o120000
    }
}

/// Open flags for file operations
bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct OpenFlags: u32 {
        const O_RDONLY = 0x0000;
        const O_WRONLY = 0x0001;
        const O_RDWR = 0x0002;
        const O_ACCMODE = 0x0003;
        const O_CREAT = 0x0040;
        const O_EXCL = 0x0080;
        const O_NOCTTY = 0x0100;
        const O_TRUNC = 0x0200;
        const O_APPEND = 0x0400;
        const O_NONBLOCK = 0x0800;
        const O_DIRECTORY = 0x10000;
        const O_NOFOLLOW = 0x20000;
        const O_CLOEXEC = 0x80000;
    }
}

/// File handle representing an open file with proper resource management
pub struct FileHandle {
    pub inode: u64,
    pub flags: OpenFlags,
    pub position: usize,
    pub fs_data: *mut (),
    _marker: core::marker::PhantomData<*mut ()>,
}

unsafe impl Send for FileHandle {}
unsafe impl Sync for FileHandle {}

impl FileHandle {
    pub fn new(inode: u64, flags: OpenFlags) -> Self {
        Self {
            inode,
            flags,
            position: 0,
            fs_data: core::ptr::null_mut(),
            _marker: core::marker::PhantomData,
        }
    }
    
    pub fn with_data(inode: u64, flags: OpenFlags, data: *mut ()) -> Self {
        Self {
            inode,
            flags,
            position: 0,
            fs_data: data,
            _marker: core::marker::PhantomData,
        }
    }
}

impl Drop for FileHandle {
    fn drop(&mut self) {
        // FileHandle cleanup is managed by the filesystem implementation
    }
}

/// Directory entry
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub inode: u64,
    pub file_type: FileType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    File,
    Directory,
    Symlink,
    BlockDevice,
    CharDevice,
    Fifo,
    Socket,
}

/// Seek positioning for file operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekWhence {
    Set = 0,
    Current = 1,
    End = 2,
}

/// File operations for read/write/seek on open files
pub trait FileOps: Send + Sync {
    fn read(&self, handle: &FileHandle, buf: &mut [u8]) -> Result<usize, VfsError>;
    fn write(&self, handle: &FileHandle, buf: &[u8]) -> Result<usize, VfsError>;
    fn seek(&self, handle: &mut FileHandle, offset: isize, whence: SeekWhence) -> Result<usize, VfsError>;
    fn flush(&self, handle: &FileHandle) -> Result<(), VfsError>;
    fn fstat(&self, handle: &FileHandle) -> Result<Stat, VfsError>;
    fn close(&self, handle: FileHandle) -> Result<(), VfsError>;
}

/// Extended filesystem operations with file-level access
pub trait FileSystemExt: FileSystem {
    fn file_ops(&self) -> Option<&dyn FileOps>;
}

/// Filesystem operations trait that all filesystems must implement
pub trait FileSystem: Send + Sync {
    fn name(&self) -> &'static str;
    fn open(&self, path: &str, flags: OpenFlags) -> Result<FileHandle, VfsError>;
    fn create(&self, path: &str) -> Result<(), VfsError>;
    fn stat(&self, path: &str) -> Result<Stat, VfsError>;
    fn readlink(&self, path: &str) -> Result<String, VfsError>;
    fn mkdir(&self, path: &str) -> Result<(), VfsError>;
    fn rmdir(&self, path: &str) -> Result<(), VfsError>;
    fn unlink(&self, path: &str) -> Result<(), VfsError>;
    fn rename(&self, from: &str, to: &str) -> Result<(), VfsError>;
    fn readdir(&self, path: &str) -> Result<Vec<DirEntry>, VfsError>;
    fn mount_point(&self) -> Option<&str>;
}

/// VFS registration system for runtime filesystem driver registration
pub struct VfsRegistry {
    filesystems: spin::Mutex<alloc::collections::BTreeMap<&'static str, &'static dyn FileSystem>>,
}

impl VfsRegistry {
    pub const fn new() -> Self {
        Self {
            filesystems: spin::Mutex::new(alloc::collections::BTreeMap::new()),
        }
    }
    
    pub fn register(&self, name: &'static str, fs: &'static dyn FileSystem) -> Result<(), VfsError> {
        let mut filesystems = self.filesystems.lock();
        if filesystems.contains_key(name) {
            return Err(VfsError::AlreadyExists);
        }
        filesystems.insert(name, fs);
        Ok(())
    }
    
    pub fn get(&self, name: &str) -> Option<&'static dyn FileSystem> {
        let filesystems = self.filesystems.lock();
        filesystems.get(name).copied()
    }
    
    pub fn auto_detect(&self, data: &[u8]) -> Option<&'static str> {
        if data.len() >= 1082 && data[1080..1082] == [0x53, 0xEF] {
            return Some("ext4");
        }
        if data.len() >= 3 && &data[0..3] == b"\xEB\x58\x90" {
            return Some("fat32");
        }
        None
    }
    
    pub fn list_filesystems(&self) -> Vec<&'static str> {
        let filesystems = self.filesystems.lock();
        filesystems.keys().copied().collect()
    }
}

pub static VFS_REGISTRY: VfsRegistry = VfsRegistry::new();

#[macro_export]
macro_rules! vfs_result {
    ($expr:expr) => {
        match $expr {
            Ok(v) => Ok(v),
            Err(e) => Err(e.into()),
        }
    };
}

#[macro_export]
macro_rules! try_opt_vfs {
    ($expr:expr, $err:expr) => {
        match $expr {
            Some(val) => val,
            None => return Err($err),
        }
    };
}

#[macro_export]
macro_rules! try_vfs {
    ($expr:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => return Err(e),
        }
    };
}
