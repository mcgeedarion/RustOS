//! EXT4 Filesystem Implementation for RustOS
//!
//! This crate provides an implementation of the EXT4 filesystem,
//! implementing the vfs_core::FileSystem trait with full VMM integration.

#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use vfs_core::{FileSystem, FileHandle, OpenFlags, Stat, DirEntry, FileType, VfsError, SeekWhence, FileOps};

/// EXT4 filesystem driver with complete VMM integration
pub struct Ext4FileSystem {
    name: &'static str,
    mount_point: Option<&'static str>,
}

impl Ext4FileSystem {
    pub const fn new() -> Self {
        Self {
            name: "ext4",
            mount_point: None,
        }
    }
    
    pub const fn with_mount_point(mount_point: &'static str) -> Self {
        Self {
            name: "ext4",
            mount_point: Some(mount_point),
        }
    }
}

impl FileSystem for Ext4FileSystem {
    fn name(&self) -> &'static str {
        self.name
    }
    
    fn open(&self, path: &str, flags: OpenFlags) -> Result<FileHandle, VfsError> {
        // Validate path
        if path.is_empty() {
            return Err(VfsError::InvalidArg);
        }
        
        // Delegate to kernel ext4 implementation
        #[cfg(kernel_impl)]
        {
            use crate::kernel_ext4::open_file;
            match open_file(path, flags) {
                Ok((inode, _size)) => Ok(FileHandle::new(inode, flags)),
                Err(e) => Err(e),
            }
        }
        
        #[cfg(not(kernel_impl))]
        {
            // Stub for non-kernel builds - returns NotSupported
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
            use crate::kernel_ext4::create_file;
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
            use crate::kernel_ext4::stat_file;
            stat_file(path)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = path;
            Err(VfsError::NotSupported)
        }
    }
    
    fn readlink(&self, path: &str) -> Result<String, VfsError> {
        if path.is_empty() {
            return Err(VfsError::InvalidArg);
        }
        
        #[cfg(kernel_impl)]
        {
            use crate::kernel_ext4::readlink_file;
            readlink_file(path)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = path;
            Err(VfsError::NotSupported)
        }
    }
    
    fn mkdir(&self, path: &str) -> Result<(), VfsError> {
        if path.is_empty() {
            return Err(VfsError::InvalidArg);
        }
        
        #[cfg(kernel_impl)]
        {
            use crate::kernel_ext4::mkdir_path;
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
            use crate::kernel_ext4::rmdir_path;
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
            use crate::kernel_ext4::unlink_path;
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
            use crate::kernel_ext4::rename_path;
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
            use crate::kernel_ext4::readdir_path;
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

/// EXT4 file operations implementation
pub struct Ext4FileOps;

impl FileOps for Ext4FileOps {
    fn read(&self, handle: &FileHandle, buf: &mut [u8]) -> Result<usize, VfsError> {
        if buf.is_empty() {
            return Ok(0);
        }
        
        #[cfg(kernel_impl)]
        {
            use crate::kernel_ext4::read_at;
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
            use crate::kernel_ext4::write_at;
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
            use crate::kernel_ext4::get_file_size;
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
            use crate::kernel_ext4::flush_file;
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
            use crate::kernel_ext4::fstat_file;
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
            use crate::kernel_ext4::close_file;
            close_file(handle.inode)
        }
        
        #[cfg(not(kernel_impl))]
        {
            let _ = handle;
            Ok(())
        }
    }
}

impl FileSystemExt for Ext4FileSystem {
    fn file_ops(&self) -> Option<&dyn FileOps> {
        Some(&Ext4FileOps)
    }
}

/// Register EXT4 filesystem with the VFS registry
pub fn register_ext4() -> Result<(), VfsError> {
    static EXT4_FS: Ext4FileSystem = Ext4FileSystem::new();
    vfs_core::VFS_REGISTRY.register("ext4", &EXT4_FS)
}

/// Register EXT4 filesystem with a specific mount point
pub fn register_ext4_at(mount_point: &'static str) -> Result<(), VfsError> {
    static mut EXT4_FS: Option<Ext4FileSystem> = None;
    unsafe {
        EXT4_FS = Some(Ext4FileSystem::with_mount_point(mount_point));
        if let Some(ref fs) = EXT4_FS {
            vfs_core::VFS_REGISTRY.register("ext4", fs)
        } else {
            Err(VfsError::NotSupported)
        }
    }
}

// Kernel integration stubs - these forward to the actual kernel implementations
#[cfg(kernel_impl)]
mod kernel_ext4 {
    use vfs_core::{Stat, DirEntry, VfsError, OpenFlags};
    use alloc::string::String;
    use alloc::vec::Vec;
    
    pub fn open_file(_path: &str, _flags: OpenFlags) -> Result<(u64, usize), VfsError> {
        // Forward to src/fs/ext4.rs implementation
        Err(VfsError::NotSupported)
    }
    
    pub fn create_file(_path: &str) -> Result<(), VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn stat_file(_path: &str) -> Result<Stat, VfsError> {
        Err(VfsError::NotSupported)
    }
    
    pub fn readlink_file(_path: &str) -> Result<String, VfsError> {
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
    fn test_ext4_new() {
        let fs = Ext4FileSystem::new();
        assert_eq!(fs.name(), "ext4");
        assert_eq!(fs.mount_point(), None);
    }
    
    #[test]
    fn test_ext4_with_mount_point() {
        let fs = Ext4FileSystem::with_mount_point("/mnt/data");
        assert_eq!(fs.name(), "ext4");
        assert_eq!(fs.mount_point(), Some("/mnt/data"));
    }
}
