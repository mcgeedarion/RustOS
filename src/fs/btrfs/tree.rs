//! Checked B-tree traversal and bounded leaf editing.
//!
//! Leaf offsets are relative to the END of the 101-byte header, not the end
//! of the block. All reads verify checksums, key ordering and item bounds.
extern crate alloc;
use alloc::{collections::{BTreeMap, BTreeSet}, vec::Vec};
use super::{checksum, mount, superblock::BtrfsFs};
pub use super::superblock::{BtrfsKey, BtrfsKeyPtr};

pub(super) type Items = BTreeMap<BtrfsKey, Vec<u8>>;
pub(super) const HEADER: usize = 101;

pub(super) fn encode_key(key: &BtrfsKey, out: &mut [u8]) {
    out[..8].copy_from_slice(&key.objectid.to_le_bytes());
    out[8] = key.ty;
    out[9..17].copy_from_slice(&key.offset.to_le_bytes());
}

pub(super) fn parse_leaf(raw: &[u8]) -> Result<Items, isize> {
    if raw.len() < HEADER || raw[100] != 0 { return Err(-5); }
    let n = u32::from_le_bytes(raw[96..100].try_into().unwrap()) as usize;
    let table_end = HEADER.checked_add(n.checked_mul(25).ok_or(-5isize)?).ok_or(-5isize)?;
    if table_end > raw.len() { return Err(-5); }
    let mut items = Items::new();
    let mut previous = None;
    let mut data_end = raw.len();
    for i in 0..n {
        let p = HEADER + 25 * i;
        let key = BtrfsKey::from_bytes(&raw[p..p + 17]);
        if previous.as_ref().is_some_and(|old| old >= &key) { return Err(-5); }
        let offset = u32::from_le_bytes(raw[p + 17..p + 21].try_into().unwrap()) as usize;
        let size = u32::from_le_bytes(raw[p + 21..p + 25].try_into().unwrap()) as usize;
        let start = HEADER.checked_add(offset).ok_or(-5isize)?;
        let end = start.checked_add(size).ok_or(-5isize)?;
        if start < table_end || end > data_end { return Err(-5); }
        data_end = start;
        previous = Some(key.clone());
        items.insert(key, raw[start..end].to_vec());
    }
    Ok(items)
}

pub(super) fn encode_leaf(original: &[u8], items: &Items) -> Result<Vec<u8>, isize> {
    let table_end = HEADER.checked_add(items.len().checked_mul(25).ok_or(-28isize)?).ok_or(-28isize)?;
    let bytes = items.values().try_fold(table_end, |n, v| n.checked_add(v.len())).ok_or(-28isize)?;
    if bytes > original.len() { return Err(-28); }
    let mut raw = alloc::vec![0; original.len()];
    raw[..HEADER].copy_from_slice(&original[..HEADER]);
    raw[96..100].copy_from_slice(&(items.len() as u32).to_le_bytes());
    let mut tail = raw.len();
    for (i, (key, data)) in items.iter().enumerate() {
        tail -= data.len();
        let p = HEADER + i * 25;
        encode_key(key, &mut raw[p..p + 17]);
        raw[p + 17..p + 21].copy_from_slice(&((tail - HEADER) as u32).to_le_bytes());
        raw[p + 21..p + 25].copy_from_slice(&(data.len() as u32).to_le_bytes());
        raw[tail..tail + data.len()].copy_from_slice(data);
    }
    raw[..32].fill(0);
    let crc = checksum::crc32c(&raw[32..]);
    raw[..4].copy_from_slice(&crc.to_le_bytes());
    Ok(raw)
}

impl BtrfsFs {
    pub(super) fn read_node_bytes(&self, logical: u64) -> Result<Vec<u8>, isize> {
        let raw = if logical == self.fs_tree_root {
            match &self.staged { Some(raw) => raw.clone(), None => mount::read_node(self, logical).ok_or(-5isize)? }
        } else { mount::read_node(self, logical).ok_or(-5isize)? };
        if raw.len() < HEADER || !checksum::verify_header_csum(&raw)
            || raw[32..48] != self.superblock.fsid
            || u64::from_le_bytes(raw[48..56].try_into().unwrap()) != logical
            || raw[100] > 8
        { return Err(-5); }
        Ok(raw)
    }

    /// Return a complete tree or an error, never a silently truncated walk.
    pub(super) fn tree_items(&self, root: u64) -> Result<Items, isize> {
        let mut out = Items::new();
        let mut visited = BTreeSet::new();
        self.walk(root, None, &mut visited, &mut out)?;
        Ok(out)
    }

    fn walk(&self, logical: u64, expected: Option<(u8, u64, BtrfsKey)>,
            visited: &mut BTreeSet<u64>, out: &mut Items) -> Result<(), isize> {
        if visited.len() >= 65536 || !visited.insert(logical) { return Err(-5); }
        let raw = self.read_node_bytes(logical)?;
        let level = raw[100];
        let n = u32::from_le_bytes(raw[96..100].try_into().unwrap()) as usize;
        if let Some((expected_level, generation, key)) = expected {
            if level != expected_level || raw[80..88] != generation.to_le_bytes()
                || n == 0 || raw.len() < HEADER + 17
                || BtrfsKey::from_bytes(&raw[HEADER..HEADER + 17]) != key
            { return Err(-5); }
        }
        if level == 0 {
            for (key, value) in parse_leaf(&raw)? {
                if out.insert(key, value).is_some() { return Err(-5); }
            }
        } else {
            if n == 0 || n > (raw.len() - HEADER) / 33 { return Err(-5); }
            let mut previous = None;
            for i in 0..n {
                let p = HEADER + 33 * i;
                let key = BtrfsKey::from_bytes(&raw[p..p + 17]);
                if previous.as_ref().is_some_and(|old| old >= &key) { return Err(-5); }
                let child = u64::from_le_bytes(raw[p + 17..p + 25].try_into().unwrap());
                let generation = u64::from_le_bytes(raw[p + 25..p + 33].try_into().unwrap());
                previous = Some(key.clone());
                self.walk(child, Some((level - 1, generation, key)), visited, out)?;
            }
        }
        Ok(())
    }

    pub(super) fn lookup_item(&self, root: u64, key: BtrfsKey) -> Option<Vec<u8>> {
        self.tree_items(root).ok()?.remove(&key)
    }

    /// Mutate a validated, exclusively owned FS-tree leaf. No allocation is
    /// attempted: all capacity/semantic failures happen before disk writes.
    /// This is NOT crash-atomic CoW. A torn sector write still requires repair.
    pub(super) fn edit<T>(&mut self, f: impl FnOnce(&mut Items) -> Result<T, isize>) -> Result<T, isize> {
        if !self.writable { return Err(-95); }
        let old = self.read_node_bytes(self.fs_tree_root)?;
        let mut items = parse_leaf(&old)?;
        let result = f(&mut items)?;
        let new = encode_leaf(&old, &items)?;
        if self.staged.is_some() {
            self.staged = Some(new);
        } else {
            self.persist_leaf(&old, &new)?;
        }
        Ok(result)
    }

    pub(super) fn persist_leaf(&mut self, old: &[u8], new: &[u8]) -> Result<(), isize> {
        if mount::write_node(self, self.fs_tree_root, new).is_err() {
            // An I/O error may have partially changed the block. Best-effort
            // restoration is not an atomicity guarantee; disable further writes.
            let _ = mount::write_node(self, self.fs_tree_root, old);
            self.writable = false;
            return Err(-5);
        }
        self.path_cache.clear();
        Ok(())
    }
}
