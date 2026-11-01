//! Directory hash collisions, index items and inode back-references.
extern crate alloc;
use alloc::{string::String, vec::Vec};
pub use super::superblock::BtrfsDirItem;
use super::{inode, superblock::{btrfs_name_hash, BtrfsKey, S_IFDIR, S_IFMT},
            tree::{encode_key, Items}};

pub(super) fn records(data: &[u8]) -> Result<Vec<(usize, usize, BtrfsDirItem)>, isize> {
    let mut result = Vec::new();
    let mut p = 0;
    while p < data.len() {
        let d = BtrfsDirItem::from_bytes(&data[p..]).ok_or(-5isize)?;
        let end = p + 30 + d.name_len as usize + d.data_len as usize;
        if end > data.len() || d.name_len == 0 { return Err(-5); }
        result.push((p, end, d));
        p = end;
    }
    Ok(result)
}
pub(super) fn name_valid(name: &str) -> Result<(), isize> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        Err(-22)
    } else if name.len() > 255 { Err(-36) } else { Ok(()) }
}
pub(super) fn lookup(items: &Items, parent: u64, name: &str) -> Result<u64, isize> {
    if inode::read(items, parent)?.mode & S_IFMT != S_IFDIR { return Err(-20); }
    let data = items.get(&BtrfsKey::new(parent, 84, btrfs_name_hash(name.as_bytes()))).ok_or(-2isize)?;
    for (_, _, d) in records(data)? {
        if d.name == name {
            if d.child_key.ty != 1 || d.child_key.offset != 0 { return Err(-95); }
            return Ok(d.child_key.objectid);
        }
    }
    Err(-2)
}
pub(super) fn resolve(items: &Items, root: u64, path: &str) -> Result<u64, isize> {
    let mut stack = alloc::vec![root];
    for name in path.split('/').filter(|s| !s.is_empty()) {
        let parent = *stack.last().unwrap();
        if inode::read(items, parent)?.mode & S_IFMT != S_IFDIR { return Err(-20); }
        match name {
            "." => {},
            ".." => { if stack.len() > 1 { stack.pop(); } },
            _ => { stack.push(lookup(items, parent, name)?); },
        }
    }
    let ino = *stack.last().unwrap();
    if path.ends_with('/') && inode::read(items, ino)?.mode & S_IFMT != S_IFDIR { return Err(-20); }
    Ok(ino)
}
pub(super) fn parent<'a>(items: &Items, root: u64, path: &'a str) -> Result<(u64, &'a str), isize> {
    let path = path.trim_end_matches('/');
    let (p, name) = path.rsplit_once('/').unwrap_or(("", path));
    name_valid(name)?;
    let parent = resolve(items, root, p)?;
    let ii = inode::read(items, parent)?;
    if ii.mode & S_IFMT != S_IFDIR { return Err(-20); }
    inode::mutable(&ii)?;
    Ok((parent, name))
}
pub(super) fn empty(items: &Items, ino: u64) -> bool {
    !items.keys().any(|k| k.objectid == ino && (k.ty == 84 || k.ty == 96))
}

pub(super) fn insert(items: &mut Items, parent: u64, name: &str, ino: u64, ty: u8,
                     generation: u64) -> Result<(), isize> {
    name_valid(name)?;
    match lookup(items, parent, name) { Err(-2) => {}, Ok(_) => return Err(-17), Err(e) => return Err(e) }
    let index = items.keys().filter(|k| k.objectid == parent && k.ty == 96)
        .map(|k| k.offset).max().unwrap_or(1).checked_add(1).ok_or(-28isize)?;
    let mut data = alloc::vec![0; 30 + name.len()];
    encode_key(&BtrfsKey::new(ino, 1, 0), &mut data[..17]);
    data[17..25].copy_from_slice(&generation.to_le_bytes());
    data[27..29].copy_from_slice(&(name.len() as u16).to_le_bytes());
    data[29] = ty;
    data[30..].copy_from_slice(name.as_bytes());
    items.entry(BtrfsKey::new(parent, 84, btrfs_name_hash(name.as_bytes())))
        .or_default().extend_from_slice(&data);
    items.insert(BtrfsKey::new(parent, 96, index), data);
    let mut reference = alloc::vec![0; 10 + name.len()];
    reference[..8].copy_from_slice(&index.to_le_bytes());
    reference[8..10].copy_from_slice(&(name.len() as u16).to_le_bytes());
    reference[10..].copy_from_slice(name.as_bytes());
    items.entry(BtrfsKey::new(ino, 12, parent)).or_default().extend_from_slice(&reference);
    let mut pi = inode::read(items, parent)?;
    inode::mutable(&pi)?;
    pi.size = pi.size.checked_add((name.len() * 2) as u64).ok_or(-28isize)?;
    inode::write(items, parent, &pi);
    Ok(())
}

pub(super) fn remove(items: &mut Items, parent: u64, name: &str) -> Result<u64, isize> {
    let ino = lookup(items, parent, name)?;
    let hash_key = BtrfsKey::new(parent, 84, btrfs_name_hash(name.as_bytes()));
    let data = items.get_mut(&hash_key).ok_or(-5isize)?;
    let (start, end, _) = records(data)?.into_iter().find(|(_, _, d)| d.name == name).ok_or(-5isize)?;
    data.drain(start..end);
    if data.is_empty() { items.remove(&hash_key); }
    let ref_key = BtrfsKey::new(ino, 12, parent);
    let data = items.get_mut(&ref_key).ok_or(-95isize)?; // extended refs require separate support
    let mut p = 0;
    let mut found = None;
    while p < data.len() {
        if data.len() - p < 10 { return Err(-5); }
        let len = u16::from_le_bytes(data[p + 8..p + 10].try_into().unwrap()) as usize;
        let end = p + 10 + len;
        if end > data.len() { return Err(-5); }
        if &data[p + 10..end] == name.as_bytes() {
            found = Some((p, end, u64::from_le_bytes(data[p..p + 8].try_into().unwrap())));
            break;
        }
        p = end;
    }
    let (start, end, index) = found.ok_or(-5isize)?;
    data.drain(start..end);
    if data.is_empty() { items.remove(&ref_key); }
    let index_data = items.remove(&BtrfsKey::new(parent, 96, index)).ok_or(-5isize)?;
    let d = BtrfsDirItem::from_bytes(&index_data).ok_or(-5isize)?;
    if d.name != name || d.child_key.objectid != ino { return Err(-5); }
    let mut pi = inode::read(items, parent)?;
    inode::mutable(&pi)?;
    pi.size = pi.size.checked_sub((name.len() * 2) as u64).ok_or(-5isize)?;
    inode::write(items, parent, &pi);
    Ok(ino)
}

/// Test ancestry using inode backrefs, including non-canonical input paths.
pub(super) fn descendant(items: &Items, root: u64, ancestor: u64, mut child: u64) -> Result<bool, isize> {
    for _ in 0..1024 {
        if child == ancestor { return Ok(true); }
        if child == root { return Ok(false); }
        child = items.keys().find(|k| k.objectid == child && k.ty == 12).ok_or(-5isize)?.offset;
    }
    Err(-40)
}
