//! Compatibility exports. There is exactly one live ext2 filesystem type.
pub use super::inode::{Ext2DirEntry, Ext2Fs, Ext2Stat, Ext2Statfs};
pub const ENOSPC: i32 = 28;
