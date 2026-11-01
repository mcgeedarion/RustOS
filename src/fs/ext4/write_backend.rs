//! Conservative, real ext4 mutations. No device access occurs until an entire
//! operation succeeds against a private image. Feature limitations are checked
//! before any write; this is not a general ext4 implementation.
use super::*;
use alloc::vec;

const EIO: i32 = -5;
const ENOENT: i32 = -2;
const EINVAL: i32 = -22;
const ENOSPC: i32 = -28;
const ENOTSUP: i32 = -95;
type Result<T> = core::result::Result<T, i32>;

pub(crate) enum Operation<'a> {
    Write(&'a str, &'a [u8], u64),
    Truncate(&'a str, u64),
    Create(&'a str, u16),
    Mkdir(&'a str, u16),
    Unlink(&'a str, bool),
    Rename(&'a str, &'a str),
    Link(&'a str, &'a str),
    Symlink(&'a str, &'a str),
    Chmod(&'a str, u16),
    Chown(&'a str, u32, u32),
    Utimens(&'a str, u64, u64),
}

fn put16(data: &mut [u8], off: usize, value: u16) -> Result<()> {
    data.get_mut(off..off.checked_add(2).ok_or(EIO)?).ok_or(EIO)?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}
fn put32(data: &mut [u8], off: usize, value: u32) -> Result<()> {
    data.get_mut(off..off.checked_add(4).ok_or(EIO)?).ok_or(EIO)?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}
fn adjust16(data: &mut [u8], off: usize, delta: i32) -> Result<()> {
    let old = read_u16_le(data, off).ok_or(EIO)? as i32;
    let new = old.checked_add(delta).ok_or(EIO)?;
    put16(data, off, u16::try_from(new).map_err(|_| EIO)?)
}
fn adjust32(data: &mut [u8], off: usize, delta: i32) -> Result<()> {
    let old = read_u32_le(data, off).ok_or(EIO)? as i64;
    put32(data, off, u32::try_from(old + delta as i64).map_err(|_| EIO)?)
}

impl Ext4Fs {
    fn writable_profile(&self) -> Result<()> {
        // No checksums, lazy/uninitialized allocation maps, bigalloc, MMP,
        // orphan_file, quotas or other features that require additional updates.
        // A journal is not silently bypassed: journaled profiles are rejected
        // until their on-disk commit protocol is supported here.
        let compat = read_u32_le(&self.data, 1024 + 0x5c).ok_or(EIO)?;
        if compat & !(0x08 | 0x10 | 0x20) != 0
            || self.feature_incompat & !(0x02 | 0x40 | 0x80 | 0x200) != 0
            || self.feature_ro_compat & !(0x01 | 0x02 | 0x08 | 0x20 | 0x40) != 0
        { return Err(ENOTSUP); }
        if read_u16_le(&self.data, 1024 + 0x3a) != Some(1) { return Err(-30); }
        if self.total_blocks.checked_mul(self.block_size as u64) != Some(self.data.len() as u64)
            || self.inodes_per_grp == 0 || self.blocks_per_grp == 0
            || self.inodes_per_grp > self.block_size * 8
            || self.blocks_per_grp > self.block_size * 8
        { return Err(EIO); }
        for g in 0..self.total_groups {
            let desc = self.bgd_offset(g).ok_or(EIO)?;
            if read_u16_le(&self.data, desc + 18).ok_or(EIO)? & 3 != 0 {
                return Err(ENOTSUP);
            }
            self.block_slice(self.bgd_block_bitmap(g).ok_or(EIO)?).ok_or(EIO)?;
            self.block_slice(self.bgd_inode_bitmap(g).ok_or(EIO)?).ok_or(EIO)?;
        }
        Ok(())
    }

    fn mutable_inode(&self, ino: u32) -> Result<Inode> {
        let inode = self.inode(ino).ok_or(EIO)?;
        // Immutable/append-only, indexed directories, huge-file block units,
        // inline data and external EA blocks need specialized handling.
        if inode.flags & !EXT4_INODE_EXTENTS != 0
            || inode.file_acl_lo != 0 || inode.file_acl_hi != 0 { return Err(ENOTSUP); }
        Ok(inode)
    }
    fn store_inode(&mut self, ino: u32, inode: &Inode) -> Result<()> {
        let off = self.inode_offset(ino).ok_or(EIO)?;
        let size = core::mem::size_of::<Inode>();
        let dst = self.data.get_mut(off..off.checked_add(size).ok_or(EIO)?).ok_or(EIO)?;
        // SAFETY: packed on-disk structure contains only integer fields; the
        // destination is bounds checked and at least size_of::<Inode>() bytes.
        unsafe { core::ptr::copy_nonoverlapping(inode as *const Inode as *const u8, dst.as_mut_ptr(), size); }
        Ok(())
    }
    fn block_offset(&self, block: u64) -> Result<usize> {
        if block < self.first_data_blk as u64 || block >= self.total_blocks { return Err(EIO); }
        let off = usize::try_from(block).map_err(|_| EIO)?.checked_mul(self.block_size).ok_or(EIO)?;
        self.data.get(off..off.checked_add(self.block_size).ok_or(EIO)?).ok_or(EIO)?;
        Ok(off)
    }
    fn bitmap_bit(&self, bitmap: u64, bit: usize) -> Result<bool> {
        if bit >= self.block_size * 8 { return Err(EIO); }
        let off = self.block_offset(bitmap)? + bit / 8;
        Ok(self.data[off] & (1 << (bit % 8)) != 0)
    }
    fn bitmap_set(&mut self, bitmap: u64, bit: usize, used: bool) -> Result<()> {
        if bit >= self.block_size * 8 { return Err(EIO); }
        let off = self.block_offset(bitmap)? + bit / 8;
        if used { self.data[off] |= 1 << (bit % 8); }
        else { self.data[off] &= !(1 << (bit % 8)); }
        Ok(())
    }
    fn allocate_block(&mut self, hint: u64) -> Result<u64> {
        if self.free_blocks == 0 { return Err(ENOSPC); }
        let hint = (hint as usize).max(self.first_data_blk).min(self.total_blocks as usize);
        for number in (hint..self.total_blocks as usize).chain(self.first_data_blk..hint) {
            let g = (number - self.first_data_blk) / self.blocks_per_grp;
            let bit = (number - self.first_data_blk) % self.blocks_per_grp;
            let bitmap = self.bgd_block_bitmap(g).ok_or(EIO)?;
                if !self.bitmap_bit(bitmap, bit)? {
                    let block = number as u64;
                    self.bitmap_set(bitmap, bit, true)?;
                    let desc = self.bgd_offset(g).ok_or(EIO)?;
                    adjust16(&mut self.data, desc + 12, -1)?;
                    adjust32(&mut self.data, 1024 + 12, -1)?;
                    self.free_blocks = self.free_blocks.checked_sub(1).ok_or(EIO)?;
                    let off = self.block_offset(block)?;
                    self.data[off..off + self.block_size].fill(0);
                    return Ok(block);
                }
        }
        Err(ENOSPC)
    }
    fn release_block(&mut self, block: u64) -> Result<()> {
        self.block_offset(block)?;
        let relative = block as usize - self.first_data_blk;
        let g = relative / self.blocks_per_grp;
        let bit = relative % self.blocks_per_grp;
        let bitmap = self.bgd_block_bitmap(g).ok_or(EIO)?;
        if !self.bitmap_bit(bitmap, bit)? { return Err(EIO); }
        self.bitmap_set(bitmap, bit, false)?;
        let desc = self.bgd_offset(g).ok_or(EIO)?;
        adjust16(&mut self.data, desc + 12, 1)?;
        adjust32(&mut self.data, 1024 + 12, 1)?;
        self.free_blocks += 1;
        Ok(())
    }
    fn allocate_inode(&mut self, mode: u16) -> Result<u32> {
        let total = read_u32_le(&self.data, 1024).ok_or(EIO)? as usize;
        let first = read_u32_le(&self.data, 1024 + 0x54).ok_or(EIO)? as usize;
        for ino in first.max(11)..=total {
            let idx = ino - 1;
            let g = idx / self.inodes_per_grp;
            let bit = idx % self.inodes_per_grp;
            let bitmap = self.bgd_inode_bitmap(g).ok_or(EIO)?;
            if self.bitmap_bit(bitmap, bit)? { continue; }
            let off = self.inode_offset(ino as u32).ok_or(EIO)?;
            self.bitmap_set(bitmap, bit, true)?;
            let desc = self.bgd_offset(g).ok_or(EIO)?;
            adjust16(&mut self.data, desc + 14, -1)?;
            adjust32(&mut self.data, 1024 + 16, -1)?;
            if mode & 0xf000 == 0x4000 { adjust16(&mut self.data, desc + 16, 1)?; }
            // itable_unused is a high-water mark, not a free-inode counter.
            let unused = self.inodes_per_grp - bit - 1;
            let old = read_u16_le(&self.data, desc + 28).ok_or(EIO)? as usize;
            if unused < old { put16(&mut self.data, desc + 28, unused as u16)?; }
            self.data[off..off + self.inode_size].fill(0);
            let mut inode = self.inode(ino as u32).ok_or(EIO)?;
            inode.mode = mode;
            inode.links_count = if mode & 0xf000 == 0x4000 { 2 } else { 1 };
            if self.feature_incompat & INCOMPAT_EXTENTS != 0 {
                inode.flags = EXT4_INODE_EXTENTS;
                put16(&mut inode.block, 0, 0xf30a)?;
                put16(&mut inode.block, 4, 4)?;
            }
            if self.inode_size >= 160 { inode.extra_isize = 32; }
            self.store_inode(ino as u32, &inode)?;
            return Ok(ino as u32);
        }
        Err(ENOSPC)
    }
    fn file_blocks_for_write(&self, inode: &Inode) -> Result<Vec<u64>> {
        let mut out = Vec::new();
        if inode.mode & 0xf000 == 0xa000 && inode.blocks_lo == 0 { return Ok(out); }
        if inode.flags & EXT4_INODE_EXTENTS != 0 {
            if read_u16_le(&inode.block, 0) != Some(0xf30a)
                || read_u16_le(&inode.block, 6) != Some(0) { return Err(ENOTSUP); }
            let entries = read_u16_le(&inode.block, 2).ok_or(EIO)? as usize;
            if entries > 4 { return Err(EIO); }
            for i in 0..entries {
                let off = 12 + 12 * i;
                let logical = read_u32_le(&inode.block, off).ok_or(EIO)? as usize;
                let len = read_u16_le(&inode.block, off + 4).ok_or(EIO)? as usize;
                // Dense initialized extents only. Sparse input and unwritten
                // extents remain readable but cannot yet be mutated.
                if logical != out.len() || len == 0 || len > 32768 { return Err(ENOTSUP); }
                let block = read_u32_le(&inode.block, off + 8).ok_or(EIO)? as u64
                    | ((read_u16_le(&inode.block, off + 6).ok_or(EIO)? as u64) << 32);
                if out.len().checked_add(len).ok_or(EIO)? > MAX_FILE_SIZE / self.block_size {
                    return Err(EIO);
                }
                for n in 0..len { out.push(block.checked_add(n as u64).ok_or(EIO)?); }
            }
        } else {
            for off in [48, 52, 56] {
                if read_u32_le(&inode.block, off) != Some(0) { return Err(ENOTSUP); }
            }
            let count = (self.inode_size_bytes(inode) as usize).div_ceil(self.block_size);
            if count > 12 { return Err(ENOTSUP); }
            for i in 0..count {
                let block = read_u32_le(&inode.block, i * 4).ok_or(EIO)? as u64;
                if block == 0 { return Err(ENOTSUP); }
                out.push(block);
            }
        }
        let expected = (self.inode_size_bytes(inode) as usize).div_ceil(self.block_size);
        if out.len() != expected { return Err(ENOTSUP); }
        for (i, &block) in out.iter().enumerate() {
            self.block_offset(block)?;
            if out[..i].contains(&block) { return Err(EIO); }
        }
        Ok(out)
    }
    fn replace_data(&mut self, ino: u32, bytes: &[u8]) -> Result<()> {
        if bytes.len() > MAX_FILE_SIZE { return Err(-27); }
        let mut inode = self.mutable_inode(ino)?;
        let mut blocks = self.file_blocks_for_write(&inode)?;
        let count = bytes.len().div_ceil(self.block_size);
        while blocks.len() > count { self.release_block(blocks.pop().ok_or(EIO)?)?; }
        while blocks.len() < count {
            let hint = blocks.last().copied().unwrap_or(self.first_data_blk as u64).saturating_add(1);
            blocks.push(self.allocate_block(hint)?);
        }
        inode.block.fill(0);
        if inode.flags & EXT4_INODE_EXTENTS != 0 {
            put16(&mut inode.block, 0, 0xf30a)?;
            put16(&mut inode.block, 4, 4)?;
            let mut i = 0;
            let mut runs = 0;
            while i < blocks.len() {
                if runs == 4 { return Err(ENOTSUP); }
                let start = i;
                while i + 1 < blocks.len() && blocks[i + 1] == blocks[i] + 1 && i + 1 - start < 32768 { i += 1; }
                let off = 12 + runs * 12;
                put32(&mut inode.block, off, start as u32)?;
                put16(&mut inode.block, off + 4, (i - start + 1) as u16)?;
                put16(&mut inode.block, off + 6, (blocks[start] >> 32) as u16)?;
                put32(&mut inode.block, off + 8, blocks[start] as u32)?;
                runs += 1;
                i += 1;
            }
            put16(&mut inode.block, 2, runs as u16)?;
        } else {
            if blocks.len() > 12 { return Err(ENOTSUP); }
            for (i, &block) in blocks.iter().enumerate() { put32(&mut inode.block, i * 4, block as u32)?; }
        }
        for (i, &block) in blocks.iter().enumerate() {
            let off = self.block_offset(block)?;
            let start = i * self.block_size;
            let n = self.block_size.min(bytes.len() - start);
            self.data[off..off + self.block_size].fill(0);
            self.data[off..off + n].copy_from_slice(&bytes[start..start + n]);
        }
        inode.size_lo = bytes.len() as u32;
        inode.size_hi = 0;
        inode.blocks_lo = (blocks.len() * (self.block_size / 512)) as u32;
        inode.blocks_hi = 0;
        self.store_inode(ino, &inode)
    }
    fn directory(&self, ino: u32) -> Result<Vec<(u32, Vec<u8>, u8)>> {
        let inode = self.mutable_inode(ino)?;
        if inode.mode & 0xf000 != 0x4000 { return Err(-20); }
        let mut out = Vec::new();
        for block in self.file_blocks_for_write(&inode)? {
            let bytes = self.block_slice(block).ok_or(EIO)?;
            let mut off = 0;
            while off < bytes.len() {
                let entry = self.parse_dir_entry(bytes, off).ok_or(EIO)?;
                if entry.ino != 0 { out.push((entry.ino, entry.name.to_vec(), entry.file_type)); }
                off += entry.rec_len;
            }
        }
        Ok(out)
    }
    fn store_directory(&mut self, ino: u32, entries: &[(u32, Vec<u8>, u8)]) -> Result<()> {
        let mut bytes = vec![0; self.block_size];
        let mut off = 0;
        let mut previous = None;
        for (child, name, kind) in entries {
            let len = (8 + name.len() + 3) & !3;
            if name.is_empty() || name.len() > 255 || name.contains(&0) || name.contains(&b'/') { return Err(EINVAL); }
            if off % self.block_size + len > self.block_size {
                let prev = previous.ok_or(EIO)?;
                let remaining = bytes.len() - prev;
                put16(&mut bytes, prev + 4, remaining as u16)?;
                off = bytes.len();
                bytes.resize(bytes.len() + self.block_size, 0);
            }
            if off == bytes.len() { bytes.resize(bytes.len() + self.block_size, 0); }
            put32(&mut bytes, off, *child)?;
            put16(&mut bytes, off + 4, len as u16)?;
            bytes[off + 6] = name.len() as u8;
            bytes[off + 7] = if self.feature_incompat & 2 != 0 { *kind } else { 0 };
            bytes[off + 8..off + 8 + name.len()].copy_from_slice(name);
            previous = Some(off);
            off += len;
        }
        if let Some(prev) = previous {
            let remaining = bytes.len() - prev;
            put16(&mut bytes, prev + 4, remaining as u16)?;
        } else { put16(&mut bytes, 4, self.block_size as u16)?; }
        self.replace_data(ino, &bytes)
    }
    fn parent<'a>(&self, path: &'a str) -> Result<(u32, &'a str)> {
        if !path.starts_with('/') || path.as_bytes().contains(&0) { return Err(EINVAL); }
        let path = path.trim_end_matches('/');
        let (parent, name) = path.rsplit_once('/').ok_or(EINVAL)?;
        if name.is_empty() || name == "." || name == ".." || name.len() > 255 { return Err(EINVAL); }
        let ino = self.lookup_path(if parent.is_empty() { "/" } else { parent }).ok_or(ENOENT)?;
        self.directory(ino)?;
        Ok((ino, name))
    }
    fn add_entry(&mut self, parent: u32, name: &str, ino: u32, kind: u8) -> Result<()> {
        let mut entries = self.directory(parent)?;
        if entries.iter().any(|entry| entry.1 == name.as_bytes()) { return Err(-17); }
        entries.push((ino, name.as_bytes().to_vec(), kind));
        self.store_directory(parent, &entries)
    }
    fn remove_entry(&mut self, parent: u32, name: &str) -> Result<u32> {
        let mut entries = self.directory(parent)?;
        let index = entries.iter().position(|entry| entry.1 == name.as_bytes()).ok_or(ENOENT)?;
        let ino = entries.remove(index).0;
        self.store_directory(parent, &entries)?;
        Ok(ino)
    }
    fn adjust_links(&mut self, ino: u32, delta: i32) -> Result<()> {
        let mut inode = self.mutable_inode(ino)?;
        inode.links_count = u16::try_from(inode.links_count as i32 + delta).map_err(|_| -31)?;
        self.store_inode(ino, &inode)
    }
    fn release_inode(&mut self, ino: u32) -> Result<()> {
        let inode = self.mutable_inode(ino)?;
        for block in self.file_blocks_for_write(&inode)? { self.release_block(block)?; }
        let index = ino as usize - 1;
        let g = index / self.inodes_per_grp;
        let bitmap = self.bgd_inode_bitmap(g).ok_or(EIO)?;
        if !self.bitmap_bit(bitmap, index % self.inodes_per_grp)? { return Err(EIO); }
        self.bitmap_set(bitmap, index % self.inodes_per_grp, false)?;
        let desc = self.bgd_offset(g).ok_or(EIO)?;
        adjust16(&mut self.data, desc + 14, 1)?;
        adjust32(&mut self.data, 1024 + 16, 1)?;
        if inode.mode & 0xf000 == 0x4000 { adjust16(&mut self.data, desc + 16, -1)?; }
        let off = self.inode_offset(ino).ok_or(EIO)?;
        self.data[off..off + self.inode_size].fill(0);
        Ok(())
    }
    fn unlink_entry(&mut self, path: &str, directory: bool) -> Result<()> {
        let (parent, name) = self.parent(path)?;
        let ino = self.lookup_dir(parent, name).ok_or(ENOENT)?;
        let inode = self.mutable_inode(ino)?;
        let is_directory = inode.mode & 0xf000 == 0x4000;
        if directory != is_directory { return Err(if is_directory { -21 } else { -20 }); }
        if directory && self.directory(ino)?.iter().any(|entry| entry.1 != b"." && entry.1 != b"..") {
            return Err(-39);
        }
        self.remove_entry(parent, name)?;
        if directory {
            self.adjust_links(parent, -1)?;
            self.release_inode(ino)?;
        } else if inode.links_count > 1 { self.adjust_links(ino, -1)?; }
        else { self.release_inode(ino)?; }
        Ok(())
    }
    fn apply(&mut self, op: Operation<'_>) -> Result<i32> {
        match op {
            Operation::Write(path, buf, offset) => {
                let ino = self.lookup_path(path).ok_or(ENOENT)?;
                let inode = self.mutable_inode(ino)?;
                if inode.mode & 0xf000 != 0x8000 { return Err(-21); }
                if buf.is_empty() { return Ok(0); }
                let offset = usize::try_from(offset).map_err(|_| -27)?;
                let end = offset.checked_add(buf.len()).filter(|&n| n <= MAX_FILE_SIZE).ok_or(-27)?;
                self.file_blocks_for_write(&inode)?;
                let mut data = self.read_inode_data(&inode);
                if data.len() < end { data.resize(end, 0); }
                data[offset..end].copy_from_slice(buf);
                self.replace_data(ino, &data)?;
                return i32::try_from(buf.len()).map_err(|_| -27);
            }
            Operation::Truncate(path, size) => {
                let size = usize::try_from(size).ok().filter(|&n| n <= MAX_FILE_SIZE).ok_or(-27)?;
                let ino = self.lookup_path(path).ok_or(ENOENT)?;
                let inode = self.mutable_inode(ino)?;
                if inode.mode & 0xf000 != 0x8000 { return Err(-21); }
                self.file_blocks_for_write(&inode)?;
                let mut data = self.read_inode_data(&inode);
                data.resize(size, 0);
                self.replace_data(ino, &data)?;
            }
            Operation::Create(path, mode) | Operation::Mkdir(path, mode) => {
                let dir = matches!(op, Operation::Mkdir(..));
                let (parent, name) = self.parent(path)?;
                if self.lookup_dir(parent, name).is_some() { return Err(-17); }
                let ino = self.allocate_inode((mode & 0xfff) | if dir { 0x4000 } else { 0x8000 })?;
                if dir {
                    self.store_directory(ino, &[(ino, b".".to_vec(), 2), (parent, b"..".to_vec(), 2)])?;
                    self.adjust_links(parent, 1)?;
                }
                self.add_entry(parent, name, ino, if dir { 2 } else { 1 })?;
            }
            Operation::Unlink(path, directory) => self.unlink_entry(path, directory)?,
            Operation::Link(old, new) => {
                let ino = self.lookup_dir_raw(old).ok_or(ENOENT)?;
                let inode = self.mutable_inode(ino)?;
                if inode.mode & 0xf000 == 0x4000 { return Err(-1); }
                let (parent, name) = self.parent(new)?;
                self.add_entry(parent, name, ino, file_type(inode.mode))?;
                self.adjust_links(ino, 1)?;
            }
            Operation::Symlink(target, path) => {
                if target.is_empty() || target.as_bytes().contains(&0) || target.len() > 4095 { return Err(EINVAL); }
                let (parent, name) = self.parent(path)?;
                let ino = self.allocate_inode(0xa1ff)?;
                if target.len() <= 60 {
                    let mut inode = self.mutable_inode(ino)?;
                    inode.flags = 0;
                    inode.block.fill(0);
                    inode.block[..target.len()].copy_from_slice(target.as_bytes());
                    inode.size_lo = target.len() as u32;
                    self.store_inode(ino, &inode)?;
                } else { self.replace_data(ino, target.as_bytes())?; }
                self.add_entry(parent, name, ino, 7)?;
            }
            Operation::Rename(old, new) => {
                let (old_parent, old_name) = self.parent(old)?;
                let ino = self.lookup_dir(old_parent, old_name).ok_or(ENOENT)?;
                let inode = self.mutable_inode(ino)?;
                let (new_parent, new_name) = self.parent(new)?;
                if self.lookup_dir(new_parent, new_name) == Some(ino) { return Ok(0); }
                let directory = inode.mode & 0xf000 == 0x4000;
                if directory {
                    let mut ancestor = new_parent;
                    for _ in 0..=read_u32_le(&self.data, 1024).ok_or(EIO)? {
                        if ancestor == ino { return Err(EINVAL); }
                        if ancestor == 2 { break; }
                        let next = self.lookup_dir(ancestor, "..").ok_or(EIO)?;
                        if next == ancestor { return Err(EIO); }
                        ancestor = next;
                    }
                    if ancestor != 2 { return Err(EIO); }
                }
                if self.lookup_dir(new_parent, new_name).is_some() { self.unlink_entry(new, directory)?; }
                self.remove_entry(old_parent, old_name)?;
                self.add_entry(new_parent, new_name, ino, file_type(inode.mode))?;
                if directory && old_parent != new_parent {
                    let mut entries = self.directory(ino)?;
                    entries.iter_mut().find(|entry| entry.1 == b"..").ok_or(EIO)?.0 = new_parent;
                    self.store_directory(ino, &entries)?;
                    self.adjust_links(old_parent, -1)?;
                    self.adjust_links(new_parent, 1)?;
                }
            }
            Operation::Chmod(path, mode) => {
                let ino = self.lookup_path(path).ok_or(ENOENT)?;
                let mut inode = self.mutable_inode(ino)?;
                inode.mode = (inode.mode & 0xf000) | (mode & 0xfff);
                self.store_inode(ino, &inode)?;
            }
            Operation::Chown(path, uid, gid) => {
                let ino = self.lookup_path(path).ok_or(ENOENT)?;
                let mut inode = self.mutable_inode(ino)?;
                if uid != u32::MAX { inode.uid_lo = uid as u16; inode.uid_hi = (uid >> 16) as u16; }
                if gid != u32::MAX { inode.gid_lo = gid as u16; inode.gid_hi = (gid >> 16) as u16; }
                inode.mode &= !0xc00; // clear setuid/setgid on ownership change
                self.store_inode(ino, &inode)?;
            }
            Operation::Utimens(path, atime, mtime) => {
                if atime / 1_000_000_000 > u32::MAX as u64 || mtime / 1_000_000_000 > u32::MAX as u64 { return Err(EINVAL); }
                let ino = self.lookup_path(path).ok_or(ENOENT)?;
                let mut inode = self.mutable_inode(ino)?;
                inode.atime = (atime / 1_000_000_000) as u32;
                inode.mtime = (mtime / 1_000_000_000) as u32;
                if inode.extra_isize >= 24 {
                    inode.atime_extra = ((atime % 1_000_000_000) as u32) << 2;
                    inode.mtime_extra = ((mtime % 1_000_000_000) as u32) << 2;
                }
                self.store_inode(ino, &inode)?;
            }
        }
        Ok(0)
    }
}

fn file_type(mode: u16) -> u8 {
    match mode & 0xf000 { 0x8000 => 1, 0x4000 => 2, 0xa000 => 7, _ => 0 }
}

/// Serialize edits and device persistence under the same FS lock. No nested
/// global journal/FS locks and no asynchronous dirty-state races.
pub(crate) fn run(
    operation: Operation<'_>,
    mut write: impl FnMut(u64, &[u8]) -> Result<()>,
    mut flush: impl FnMut() -> Result<()>,
) -> Result<i32> {
    let mut guard = FS.lock();
    let fs = guard.as_mut().ok_or(-30)?;
    fs.writable_profile()?;
    let mut next = fs.clone();
    let result = next.apply(operation)?;
    if fs.data == next.data { return Ok(result); }
    // Mark the disk and in-memory mount unclean before the first changed block.
    // Any I/O failure leaves writes disabled and forces external fsck. This is
    // deliberately not represented as a crash-atomic journal commit.
    put16(&mut fs.data, 1024 + 0x3a, 0)?;
    write(2, &fs.data[1024..2048])?;
    flush()?;
    put16(&mut next.data, 1024 + 0x3a, 0)?;
    for off in (0..next.data.len()).step_by(next.block_size) {
        let end = off + next.block_size;
        if next.data[off..end] != fs.data[off..end] {
            write((off / 512) as u64, &next.data[off..end])?;
        }
    }
    flush()?;
    put16(&mut next.data, 1024 + 0x3a, 1)?;
    write(2, &next.data[1024..2048])?;
    flush()?;
    *fs = next;
    Ok(result)
}
