//! Sparse, inline and uncompressed regular file extents.
extern crate alloc;
use alloc::vec::Vec;
pub use super::superblock::BtrfsFileExtentItem;
use super::{checksum, inode, mount, superblock::{BtrfsFs, BtrfsKey}, tree::Items};

pub(super) fn inline_only(items: &Items, ino: u64) -> Result<(), isize> {
    for (key, data) in items.iter().filter(|(k, _)| k.objectid == ino && k.ty == 108) {
        let extent = BtrfsFileExtentItem::from_bytes(data).ok_or(-5isize)?;
        if key.offset != 0 || extent.ty != 0 { return Err(-95); }
    }
    Ok(())
}
pub(super) fn replace_inline(items: &mut Items, ino: u64, data: &[u8], generation: u64,
                            max_inline: usize) -> Result<(), isize> {
    if data.len() > max_inline { return Err(-27); }
    inline_only(items, ino)?;
    let mut ii = inode::read(items, ino)?;
    inode::mutable(&ii)?;
    items.retain(|k, _| !(k.objectid == ino && k.ty == 108));
    if !data.is_empty() {
        let mut extent = alloc::vec![0; 21];
        extent[..8].copy_from_slice(&generation.to_le_bytes());
        extent[8..16].copy_from_slice(&(data.len() as u64).to_le_bytes());
        extent.extend_from_slice(data);
        items.insert(BtrfsKey::new(ino, 108, 0), extent);
    }
    ii.size = data.len() as u64;
    ii.nbytes = data.len() as u64;
    ii.sequence = ii.sequence.wrapping_add(1);
    inode::write(items, ino, &ii);
    Ok(())
}

impl BtrfsFs {
    pub(super) fn read_data(&self, items: &Items, ino: u64) -> Result<Vec<u8>, isize> {
        let ii = inode::read(items, ino)?;
        let len = usize::try_from(ii.size).map_err(|_| -27isize)?;
        // Kernel read_all is bounded; no untrusted inode can force arbitrary allocation.
        if len > 64 * 1024 * 1024 { return Err(-27); }
        let mut result = Vec::new();
        result.try_reserve_exact(len).map_err(|_| -12isize)?;
        result.resize(len, 0);
        let mut last_end = 0u64;
        let mut csums = None;
        for (key, data) in items.iter().filter(|(k, _)| k.objectid == ino && k.ty == 108) {
            let extent = BtrfsFileExtentItem::from_bytes(data).ok_or(-5isize)?;
            if extent.compression != 0 || extent.encryption != 0 || extent.other_encoding != 0 { return Err(-95); }
            let extent_len = if extent.ty == 0 { extent.ram_bytes } else { extent.num_bytes };
            if key.offset < last_end { return Err(-5); }
            last_end = key.offset.checked_add(extent_len).ok_or(-5isize)?;
            if key.offset >= ii.size { continue; }
            let copy = usize::try_from(extent_len.min(ii.size - key.offset)).map_err(|_| -5isize)?;
            let start = key.offset as usize;
            match extent.ty {
                0 => {
                    if key.offset != 0 || extent.inline_data.len() as u64 != extent.ram_bytes { return Err(-5); }
                    result[start..start + copy].copy_from_slice(&extent.inline_data[..copy]);
                },
                2 => {}, // Unwritten preallocation reads as zeros, never stale disk bytes.
                1 if extent.disk_bytenr == 0 => {},
                1 => {
                    if extent.extent_offset.checked_add(extent.num_bytes).ok_or(-5isize)? > extent.disk_num_bytes {
                        return Err(-5);
                    }
                    let logical = extent.disk_bytenr.checked_add(extent.extent_offset).ok_or(-5isize)?;
                    let sector = self.superblock.sectorsize as u64;
                    let aligned = logical / sector * sector;
                    let offset = usize::try_from(logical - aligned).map_err(|_| -5isize)?;
                    let size = (offset + copy).checked_add(sector as usize - 1).ok_or(-5isize)?
                        / sector as usize * sector as usize;
                    let bytes = mount::read_logical(self, aligned, size).ok_or(-5isize)?;
                    if ii.flags & 1 == 0 {
                        if csums.is_none() {
                            if self.csum_tree_root == 0 { return Err(-5); }
                            csums = Some(self.tree_items(self.csum_tree_root)?);
                        }
                        for (i, block) in bytes.chunks_exact(sector as usize).enumerate() {
                            let address = aligned + i as u64 * sector;
                            let (ck, cv) = csums.as_ref().unwrap().range(..=BtrfsKey::new(u64::MAX - 9, 128, address))
                                .next_back().ok_or(-5isize)?;
                            if ck.objectid != u64::MAX - 9 || ck.ty != 128
                                || (address - ck.offset) % sector != 0 { return Err(-5); }
                            let p = usize::try_from((address - ck.offset) / sector).map_err(|_| -5isize)?
                                .checked_mul(4).ok_or(-5isize)?;
                            let csum = cv.get(p..p + 4).ok_or(-5isize)?;
                            if csum != checksum::crc32c(block).to_le_bytes() { return Err(-5); }
                        }
                    }
                    result[start..start + copy].copy_from_slice(&bytes[offset..offset + copy]);
                },
                _ => return Err(-95),
            }
        }
        Ok(result)
    }
}
