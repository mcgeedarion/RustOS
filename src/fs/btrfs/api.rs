//! Public operations accept absolute VFS paths and select the longest mount.
extern crate alloc;
use alloc::{string::{String, ToString}, vec::Vec};
use super::superblock::{BtrfsFs, BTRFS_MOUNTS};

pub fn mount(path: &str) -> Option<()> { super::mount::mount(path, 0).ok() }

fn select<'a>(mounts: &'a alloc::collections::BTreeMap<String, BtrfsFs>, path: &str)
    -> Result<(&'a str, String), isize>
{
    if !path.starts_with('/') { return Err(-22); }
    let name = mounts.keys().filter(|m| m.as_str() == "/" || path == m.as_str()
        || path.strip_prefix(m.as_str()).is_some_and(|tail| tail.starts_with('/')))
        .max_by_key(|m| m.len()).ok_or(-19isize)?;
    let relative = if name == "/" { path } else { &path[name.len()..] };
    Ok((name.as_str(), if relative.is_empty() { "/".to_string() } else { relative.to_string() }))
}
fn with_fs<T>(path: &str, f: impl FnOnce(&BtrfsFs, &str) -> Result<T, isize>) -> Result<T, isize> {
    let mounts = BTRFS_MOUNTS.lock();
    let (name, relative) = select(&mounts, path)?;
    f(mounts.get(name).ok_or(-19isize)?, &relative)
}
fn with_fs_mut<T>(path: &str, f: impl FnOnce(&mut BtrfsFs, &str) -> Result<T, isize>) -> Result<T, isize> {
    let mut mounts = BTRFS_MOUNTS.lock();
    let (name, relative) = select(&mounts, path)?;
    let name = name.to_string();
    f(mounts.get_mut(&name).ok_or(-19isize)?, &relative)
}
fn with_two(old: &str, new: &str, f: impl FnOnce(&mut BtrfsFs, &str, &str) -> Result<(), isize>)
    -> Result<(), isize>
{
    let mut mounts = BTRFS_MOUNTS.lock();
    let (a, ar) = select(&mounts, old)?;
    let (b, br) = select(&mounts, new)?;
    if a != b { return Err(-18); }
    let name = a.to_string();
    f(mounts.get_mut(&name).ok_or(-19isize)?, &ar, &br)
}
pub fn btrfs_stat(path: &str) -> Result<crate::fs::vfs_ops::KStat, isize> {
    with_fs(path, |fs, p| fs.stat(p))
}
pub fn btrfs_read_all(path: &str) -> Result<Vec<u8>, isize> { with_fs(path, |fs, p| fs.read_all(p)) }
pub fn btrfs_readdir(path: &str) -> Result<Vec<crate::fs::vfs_ops::DirEntry>, isize> {
    with_fs(path, |fs, p| fs.readdir(p))
}
pub fn btrfs_readlink(path: &str) -> Result<String, isize> { with_fs(path, |fs, p| fs.readlink(p)) }
pub fn btrfs_statfs(path: &str) -> Result<crate::fs::vfs_ops::KStatfs, isize> {
    with_fs(path, |fs, _| Ok(fs.statfs()))
}
pub fn btrfs_write_all(path: &str, data: &[u8]) -> Result<(), isize> {
    with_fs_mut(path, |fs, p| fs.write_all(p, data))
}
pub fn btrfs_create(path: &str) -> Result<(), isize> { with_fs_mut(path, |fs, p| fs.create(p)) }
pub fn btrfs_mkdir(path: &str) -> Result<(), isize> { with_fs_mut(path, |fs, p| fs.mkdir(p)) }
pub fn btrfs_unlink(path: &str) -> Result<(), isize> { with_fs_mut(path, |fs, p| fs.unlink(p)) }
pub fn btrfs_rmdir(path: &str) -> Result<(), isize> { with_fs_mut(path, |fs, p| fs.rmdir(p)) }
pub fn btrfs_truncate(path: &str, len: u64) -> Result<(), isize> {
    with_fs_mut(path, |fs, p| fs.truncate(p, len))
}
pub fn btrfs_rename(old: &str, new: &str) -> Result<(), isize> {
    with_two(old, new, |fs, a, b| fs.rename(a, b))
}
pub fn btrfs_link(old: &str, new: &str) -> Result<(), isize> {
    with_two(old, new, |fs, a, b| fs.link(a, b))
}
pub fn btrfs_symlink(target: &str, path: &str) -> Result<(), isize> {
    with_fs_mut(path, |fs, p| fs.symlink(target, p))
}
pub fn btrfs_chmod(path: &str, mode: u32) -> Result<(), isize> {
    with_fs_mut(path, |fs, p| fs.chmod(p, mode))
}
pub fn btrfs_chown(path: &str, uid: u32, gid: u32) -> Result<(), isize> {
    with_fs_mut(path, |fs, p| fs.chown(p, uid, gid))
}
pub fn btrfs_set_times(path: &str, atime: u64, mtime: u64) -> Result<(), isize> {
    with_fs_mut(path, |fs, p| fs.set_times(p, atime, mtime))
}
pub fn sync_inode(path: &str) -> Result<(), isize> {
    // Block I/O is synchronous at the driver interface. No device-cache flush
    // primitive exists here, so this validates the inode but promises no barrier.
    with_fs(path, |fs, p| fs.stat(p).map(|_| ()))
}
