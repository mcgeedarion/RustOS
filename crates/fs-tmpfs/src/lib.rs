//! Tmpfs Filesystem Implementation for RustOS

#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use vfs_core::{FileSystem, FileHandle, OpenFlags, Stat, DirEntry, VfsError};

pub struct TmpfsFileSystem {
    name: &'static str,
    mount_point: Option<&'static str>,
}

impl TmpfsFileSystem {
    pub const fn new() -> Self {
        Self {
            name: "tmpfs",
            mount_point: None,
        }
    }
}

impl FileSystem for TmpfsFileSystem {
    fn name(&self) -> &'static str {
        self.name
    }
    
    fn open(&self, _path: &str, _flags: OpenFlags) -> Result<FileHandle, VfsError> {
        Err(VfsError::NotSupported)
    }
    
    fn create(&self, _path: &str) -> Result<(), VfsError> {
        Err(VfsError::NotSupported)
    }
    
    fn stat(&self, _path: &str) -> Result<Stat, VfsError> {
        Err(VfsError::NotSupported)
    }
    
    fn readlink(&self, _path: &str) -> Result<String, VfsError> {
        Err(VfsError::NotSupported)
    }
    
    fn mkdir(&self, _path: &str) -> Result<(), VfsError> {
        Err(VfsError::NotSupported)
    }
    
    fn rmdir(&self, _path: &str) -> Result<(), VfsError> {
        Err(VfsError::NotSupported)
    }
    
    fn unlink(&self, _path: &str) -> Result<(), VfsError> {
        Err(VfsError::NotSupported)
    }
    
    fn rename(&self, _from: &str, _to: &str) -> Result<(), VfsError> {
        Err(VfsError::NotSupported)
    }
    
    fn readdir(&self, _path: &str) -> Result<Vec<DirEntry>, VfsError> {
        Err(VfsError::NotSupported)
    }
    
    fn mount_point(&self) -> Option<&str> {
        self.mount_point
    }
}

pub fn register_tmpfs() -> Result<(), VfsError> {
    static TMPFS_FS: TmpfsFileSystem = TmpfsFileSystem::new();
    vfs_core::VFS_REGISTRY.register("tmpfs", &TMPFS_FS)
}
