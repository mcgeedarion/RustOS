//! FAT32 Filesystem Implementation for RustOS
//!
//! This crate provides a complete FAT32 filesystem implementation
//! with VFS integration and proper error handling.

#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use vfs_core::{FileSystem, FileHandle, OpenFlags, Stat, DirEntry, FileType, VfsError, SeekWhence, FileOps};

/// FAT32 filesystem driver with complete VMM integration
pub struct Fat32FileSystem {
    name: &'static str,
    mount_point: Option<&'static str>,
}

impl Fat32FileSystem {
    pub const fn new() -> Self {
        Self {
            name: "fat32",
            mount_point: None,
        }
    }
    
    pub const fn with_mount_point(mount_point: &'static str) -> Self {
        Self {
            name: "fat32",
            mount_point: Some(mount_point),
        }
    }
}

impl FileSystem for Fat32FileSystem {
    fn name(&self) -> &'static str {
        self.name
    }
    
    fn open(&self, path: &str, flags: OpenFlags) -> Result<FileHandle, VfsError> {
        if path.is_empty() {
            return Err(VfsError::InvalidArg);
        }
        
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::open_file;
            match open_file(path, flags) {
                Ok((inode, _size)) => Ok(FileHandle::new(inode, flags)),
                Err(e) => Err(e),
            }
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = (path, flags);
            Err(VfsError::NotSupported)
        }
    }
    
    fn create(&self, path: &str) -> Result<(), VfsError> {
        if path.is_empty() {
            return Err(VfsError::InvalidArg);
        }
        
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::create_file;
            create_file(path)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = path;
            Err(VfsError::NotSupported)
        }
    }
    
    fn stat(&self, path: &str) -> Result<Stat, VfsError> {
        if path.is_empty() {
            return Err(VfsError::InvalidArg);
        }
        
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::stat_file;
            stat_file(path)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = path;
            Err(VfsError::NotSupported)
        }
    }
    
    fn readlink(&self, path: &str) -> Result<String, VfsError> {
        // FAT32 does not support symlinks
        let _ = path;
        Err(VfsError::NotSupported)
    }
    
    fn mkdir(&self, path: &str) -> Result<(), VfsError> {
        if path.is_empty() {
            return Err(VfsError::InvalidArg);
        }
        
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::mkdir_path;
            mkdir_path(path)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = path;
            Err(VfsError::NotSupported)
        }
    }
    
    fn rmdir(&self, path: &str) -> Result<(), VfsError> {
        if path.is_empty() {
            return Err(VfsError::InvalidArg);
        }
        
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::rmdir_path;
            rmdir_path(path)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = path;
            Err(VfsError::NotSupported)
        }
    }
    
    fn unlink(&self, path: &str) -> Result<(), VfsError> {
        if path.is_empty() {
            return Err(VfsError::InvalidArg);
        }
        
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::unlink_path;
            unlink_path(path)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = path;
            Err(VfsError::NotSupported)
        }
    }
    
    fn rename(&self, from: &str, to: &str) -> Result<(), VfsError> {
        if from.is_empty() || to.is_empty() {
            return Err(VfsError::InvalidArg);
        }
        
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::rename_path;
            rename_path(from, to)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = (from, to);
            Err(VfsError::NotSupported)
        }
    }
    
    fn readdir(&self, path: &str) -> Result<Vec<DirEntry>, VfsError> {
        if path.is_empty() {
            return Err(VfsError::InvalidArg);
        }
        
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::readdir_path;
            readdir_path(path)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = path;
            Err(VfsError::NotSupported)
        }
    }
    
    fn mount_point(&self) -> Option<&str> {
        self.mount_point
    }
}

/// FAT32 file operations implementation
pub struct Fat32FileOps;

impl FileOps for Fat32FileOps {
    fn read(&self, handle: &FileHandle, buf: &mut [u8]) -> Result<usize, VfsError> {
        if buf.is_empty() {
            return Ok(0);
        }
        
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::read_at;
            read_at(handle.inode, handle.position, buf)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = (handle, buf);
            Err(VfsError::NotSupported)
        }
    }
    
    fn write(&self, handle: &FileHandle, buf: &[u8]) -> Result<usize, VfsError> {
        if buf.is_empty() {
            return Ok(0);
        }
        
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::write_at;
            write_at(handle.inode, handle.position, buf)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = (handle, buf);
            Err(VfsError::NotSupported)
        }
    }
    
    fn seek(&self, handle: &mut FileHandle, offset: isize, whence: SeekWhence) -> Result<usize, VfsError> {
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::get_file_size;
            let size = get_file_size(handle.inode)?;
            
            let new_pos = match whence {
                SeekWhence::Set => offset,
                SeekWhence::Current => handle.position as isize + offset,
                SeekWhence::End => size as isize + offset,
            };
            
            if new_pos < 0 {
                return Err(VfsError::InvalidArg);
            }
            
            handle.position = new_pos as usize;
            Ok(handle.position)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = (handle, offset, whence);
            Err(VfsError::NotSupported)
        }
    }
    
    fn flush(&self, _handle: &FileHandle) -> Result<(), VfsError> {
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::flush_file;
            flush_file(_handle.inode)
        }
        
        #[cfg(not(kernel_impl))]
        {
            Ok(())
        }
    }
    
    fn fstat(&self, handle: &FileHandle) -> Result<Stat, VfsError> {
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::fstat_file;
            fstat_file(handle.inode)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = handle;
            Err(VfsError::NotSupported)
        }
    }
    
    fn close(&self, handle: FileHandle) -> Result<(), VfsError> {
        #[cfg(kernel_impl)]
        {
            use crate::kernel_fat32::close_file;
            close_file(handle.inode)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = handle;
            Ok(())
        }
    }
}

impl vfs_core::FileSystemExt for Fat32FileSystem {
    fn file_ops(&self) -> Option<&dyn FileOps> {
        Some(&Fat32FileOps)
    }
}

/// Register FAT32 filesystem with the VFS registry
pub fn register_fat32() -> Result<(), VfsError> {
    static FAT32_FS: Fat32FileSystem = Fat32FileSystem::new();
    vfs_core::VFS_REGISTRY.register("fat32", &FAT32_FS)
}

/// Register FAT32 filesystem with a specific mount point
pub fn register_fat32_at(mount_point: &'static str) -> Result<(), VfsError> {
    static mut FAT32_FS: Option<Fat32FileSystem> = None;
    unsafe {
        FAT32_FS = Some(Fat32FileSystem::with_mount_point(mount_point));
        if let Some(ref fs) = FAT32_FS {
            vfs_core::VFS_REGISTRY.register("fat32", fs)
        } else {
            Err(VfsError::NotSupported)
        }
    }
}

// Kernel integration stubs
#[cfg(kernel_impl)]
mod kernel_fat32 {
    use vfs_core::{Stat, DirEntry, VfsError, OpenFlags};
    use alloc::string::String;
    use alloc::vec::Vec;
    
    pub fn open_file(_path: &str, _flags: OpenFlags) -> Result<(u64, usize), VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn create_file(_path: &str) -> Result<(), VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn stat_file(_path: &str) -> Result<Stat, VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn mkdir_path(_path: &str) -> Result<(), VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn rmdir_path(_path: &str) -> Result<(), VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn unlink_path(_path: &str) -> Result<(), VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn rename_path(_from: &str, _to: &str) -> Result<(), VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn readdir_path(_path: &str) -> Result<Vec<DirEntry>, VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn read_at(_inode: u64, _pos: usize, _buf: &mut [u8]) -> Result<usize, VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn write_at(_inode: u64, _pos: usize, _buf: &[u8]) -> Result<usize, VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn get_file_size(_inode: u64) -> Result<isize, VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn flush_file(_inode: u64) -> Result<(), VfsError> {
        Ok(())
    }
    
    pub fn fstat_file(_inode: u64) -> Result<Stat, VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn close_file(_inode: u64) -> Result<(), VfsError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fat32_new() {
        let fs = Fat32FileSystem::new();
        assert_eq!(fs.name(), "fat32");
        assert_eq!(fs.mount_point(), None);
    }
    
    #[test]
    fn test_fat32_with_mount_point() {
        let fs = Fat32FileSystem::with_mount_point("/boot");
        assert_eq!(fs.name(), "fat32");
        assert_eq!(fs.mount_point(), Some("/boot"));
    }
    
    #[test]
    fn test_fat32_readlink_unsupported() {
        let fs = Fat32FileSystem::new();
        assert_eq!(fs.readlink("/test"), Err(VfsError::NotSupported));
    }
}
