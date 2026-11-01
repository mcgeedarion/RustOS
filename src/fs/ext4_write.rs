//! Synchronous ext4 writes on the bounded cached image.
//!
//! Supported: initialized extent roots (up to four runs), direct-block inodes,
//! linear directories, links, permissions, ownership, timestamps, allocation
//! and truncation. The write backend rejects unsupported feature profiles before
//! mutation. Writes are synchronous; failures never report success. Non-journaled
//! images are marked unclean before writes, and require fsck after interrupted I/O.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use super::ext4::write_backend::{self, Operation};

pub type WriteResult<T> = Result<T, i32>;

fn run(op: Operation<'_>) -> WriteResult<i32> {
    write_backend::run(
        op,
        |lba, bytes| crate::drivers::block::write_sectors_raw(lba, bytes).map_err(|_| -5),
        || if crate::block::virtio_blk::flush_cache() { Ok(()) } else { Err(-5) },
    )
}

pub fn flush_dirty() -> i32 {
    if crate::block::virtio_blk::flush_cache() { 0 } else { -5 }
}
pub fn write(path: &str, buf: &[u8], offset: u64) -> WriteResult<i32> {
    run(Operation::Write(path, buf, offset))
}
pub fn truncate(path: &str, len: u64) -> WriteResult<i32> {
    run(Operation::Truncate(path, len))
}
pub fn create(path: &str, mode: u16) -> WriteResult<i32> {
    run(Operation::Create(path, mode))
}
pub fn unlink(path: &str) -> WriteResult<i32> { run(Operation::Unlink(path, false)) }
pub fn rmdir(path: &str) -> WriteResult<i32> { run(Operation::Unlink(path, true)) }
pub fn mkdir(path: &str, mode: u16) -> WriteResult<i32> { run(Operation::Mkdir(path, mode)) }
pub fn rename(old: &str, new: &str) -> WriteResult<i32> { run(Operation::Rename(old, new)) }
pub fn link(old: &str, new: &str) -> WriteResult<i32> { run(Operation::Link(old, new)) }
pub fn symlink(target: &str, path: &str) -> WriteResult<i32> {
    run(Operation::Symlink(target, path))
}
pub fn chmod(path: &str, mode: u16) -> WriteResult<i32> { run(Operation::Chmod(path, mode)) }
pub fn chown(path: &str, uid: u32, gid: u32) -> WriteResult<i32> {
    run(Operation::Chown(path, uid, gid))
}
pub fn utimens(path: &str, atime_ns: u64, mtime_ns: u64) -> WriteResult<i32> {
    run(Operation::Utimens(path, atime_ns, mtime_ns))
}
pub fn fsync(path: &str) -> WriteResult<i32> {
    if super::ext4::stat(path).is_none() { return Err(-2); }
    match flush_dirty() { 0 => Ok(0), error => Err(error) }
}
// Extended attributes require separate EA checksum/refcount handling.
pub fn getxattr(_path: &str, _name: &str) -> Result<Vec<u8>, i32> { Err(-95) }
pub fn setxattr(_path: &str, _name: &str, _value: &[u8], _flags: u32) -> WriteResult<i32> { Err(-95) }
pub fn listxattr(_path: &str) -> Result<Vec<String>, i32> { Err(-95) }
pub fn removexattr(_path: &str, _name: &str) -> WriteResult<i32> { Err(-95) }
