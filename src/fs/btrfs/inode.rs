//! Inode serialization and lookup in a checked item set.
pub use super::superblock::BtrfsInodeItem;
use super::{superblock::BtrfsKey, tree::Items};

pub(super) fn read(items: &Items, ino: u64) -> Result<BtrfsInodeItem, isize> {
    let data = items.get(&BtrfsKey::new(ino, 1, 0)).ok_or(-2isize)?;
    if data.len() != 160 { return Err(-5); }
    Ok(BtrfsInodeItem::from_bytes(data))
}
pub(super) fn write(items: &mut Items, ino: u64, inode: &BtrfsInodeItem) {
    items.insert(BtrfsKey::new(ino, 1, 0), inode.to_bytes());
}
pub(super) fn mutable(inode: &BtrfsInodeItem) -> Result<(), isize> {
    // Readonly, immutable and append-only inodes cannot be arbitrarily replaced.
    if inode.flags & ((1 << 2) | (1 << 6) | (1 << 7)) != 0 { Err(-1) } else { Ok(()) }
}
