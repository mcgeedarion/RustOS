//! Single-device CRC32C mount, chunk bootstrap and checked block I/O.
extern crate alloc;
use alloc::{collections::BTreeMap, string::ToString, vec::Vec};
use super::superblock::{BtrfsChunkItem, BtrfsFs, BtrfsKey, BtrfsRootItem, BtrfsSuperblock, BTRFS_MOUNTS};

pub fn mount(path: &str, lba_start: u64) -> Result<(), &'static str> {
    if !path.starts_with('/') || path.contains("//") { return Err("invalid mount path"); }
    let fs = open(lba_start)?;
    let key = if path == "/" { "/" } else { path.trim_end_matches('/') };
    BTRFS_MOUNTS.lock().insert(key.to_string(), fs);
    Ok(())
}

pub(super) fn open(lba_start: u64) -> Result<BtrfsFs, &'static str> {
    let raw = read_sectors_raw(lba_start.checked_add(128).ok_or("partition overflow")?, 8)
        .ok_or("superblock read failed")?;
    let sb = BtrfsSuperblock::from_bytes(&raw).ok_or("invalid or unsupported superblock")?;
    let mut fs = BtrfsFs {
        root_tree_root: sb.root, fs_tree_root: 0, csum_tree_root: 0, root_dir: 256,
        chunk_map: parse_sys_chunk_array(&sb).ok_or("invalid system chunk array")?,
        superblock: sb, path_cache: BTreeMap::new(), alloc_cursor: 0,
        lba_start, writable: false, staged: None,
    };
    let chunks = fs.tree_items(fs.superblock.chunk_root).map_err(|_| "invalid chunk tree")?;
    for (key, data) in chunks {
        if key.ty != 228 { continue; }
        let chunk = BtrfsChunkItem::from_bytes(&data).ok_or("invalid chunk")?;
        check_chunk(&chunk).ok_or("unsupported chunk profile")?;
        let end = key.offset.checked_add(chunk.length).ok_or("chunk overflow")?;
        fs.chunk_map.retain(|(start, _, _)| *start != key.offset);
        fs.chunk_map.push((key.offset, end, chunk));
    }
    fs.chunk_map.sort_by_key(|(start, _, _)| *start);
    for pair in fs.chunk_map.windows(2) {
        if pair[0].1 > pair[1].0 { return Err("overlapping chunks"); }
    }
    let roots = fs.tree_items(fs.root_tree_root).map_err(|_| "invalid root tree")?;
    let root_item = |id| -> Option<BtrfsRootItem> {
        roots.iter().filter(|(k, _)| k.objectid == id && k.ty == 132)
            .next_back().and_then(|(_, d)| BtrfsRootItem::from_bytes(d))
    };
    let ri = root_item(5).ok_or("missing default FS tree")?;
    fs.fs_tree_root = ri.bytenr;
    fs.root_dir = ri.root_dirid;
    fs.csum_tree_root = root_item(7).map_or(0, |r| r.bytenr);
    fs.read_node_bytes(fs.fs_tree_root).map_err(|_| "invalid FS root")?;

    // In-place bounded mutations must never touch a shared tree block. Require
    // one root ref, no snapshots/subvolumes/relocation roots, and an explicit
    // extent-tree record with precisely one TREE_BLOCK_REF to FS_TREE (5).
    // mkfs always creates the DATA_RELOC_TREE (-9), even with no relocation
    // in progress. It is a distinct extent and does not imply shared FS nodes.
    let no_subvols = !roots.keys().any(|k| k.ty == 132
        && (k.objectid == 8 || (k.objectid >= 256 && k.objectid != u64::MAX - 8)));
    if ri.level == 0 && ri.refs == 1 && ri.last_snapshot == 0 && ri.flags == 0
        && no_subvols && fs.superblock.compat_ro_flags & !3 == 0
    {
        if let Some(extent_root) = root_item(2) {
            let extents = fs.tree_items(extent_root.bytenr).map_err(|_| "invalid extent tree")?;
            let skinny = fs.superblock.incompat_flags & (1 << 8) != 0;
            let key = BtrfsKey::new(fs.fs_tree_root, if skinny {169} else {168},
                                   if skinny {0} else {fs.superblock.nodesize as u64});
            if let Some(e) = extents.get(&key) {
                let p = if skinny {24} else {42};
                fs.writable = e.len() == p + 9
                    && e[..8] == 1u64.to_le_bytes()
                    && u64::from_le_bytes(e[16..24].try_into().unwrap()) & 2 != 0
                    && e[p] == 176 && e[p + 1..p + 9] == 5u64.to_le_bytes();
            }
        }
    }
    Ok(fs)
}

fn check_chunk(chunk: &BtrfsChunkItem) -> Option<()> {
    // SINGLE and same-device DUP only; no RAID address arithmetic is guessed.
    if chunk.length == 0 || chunk.ty & !0x27 != 0
        || (chunk.ty & 32 == 0 && chunk.num_stripes != 1)
        || (chunk.ty & 32 != 0 && chunk.num_stripes != 2)
        || chunk.stripes.iter().any(|(dev, off)| *dev != chunk.dev_id || off % 512 != 0)
    { return None; }
    Some(())
}

fn parse_sys_chunk_array(sb: &BtrfsSuperblock) -> Option<Vec<(u64, u64, BtrfsChunkItem)>> {
    let arr = sb.sys_chunk_array.get(..sb.sys_chunk_array_size as usize)?;
    let mut map = Vec::new();
    let mut off = 0;
    while off < arr.len() {
        let key = BtrfsKey::from_bytes(arr.get(off..off + 17)?);
        off += 17;
        if key.ty != 228 { return None; }
        let chunk = BtrfsChunkItem::from_bytes(&arr[off..])?;
        check_chunk(&chunk)?;
        off += 48 + chunk.num_stripes as usize * 32;
        map.push((key.offset, key.offset.checked_add(chunk.length)?, chunk));
    }
    if map.is_empty() || off != arr.len() { None } else { Some(map) }
}

pub(crate) fn read_sectors_raw(lba: u64, count: u32) -> Option<Vec<u8>> {
    crate::drivers::block::read_sectors_vec(lba, count)
}
pub(crate) fn write_sectors_raw(lba: u64, data: &[u8]) -> Result<(), &'static str> {
    crate::drivers::block::write_sectors_raw(lba, data)
}

impl BtrfsFs {
    pub(super) fn logical_to_physical(&self, logical: u64) -> Option<u64> {
        let (start, _, chunk) = self.chunk_map.iter().find(|(a, b, _)| logical >= *a && logical < *b)?;
        chunk.offset.checked_add(logical - start)?.checked_add(self.lba_start.checked_mul(512)?)
    }
}

pub(crate) fn read_logical(fs: &BtrfsFs, mut logical: u64, mut byte_len: usize) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    result.try_reserve_exact(byte_len).ok()?;
    while byte_len != 0 {
        let (_, end, _) = fs.chunk_map.iter().find(|(a, b, _)| logical >= *a && logical < *b)?;
        let take = byte_len.min(usize::try_from(end - logical).ok()?);
        let phys = fs.logical_to_physical(logical)?;
        let offset = (phys % 512) as usize;
        let sectors = u32::try_from(offset.checked_add(take)?.checked_add(511)? / 512).ok()?;
        let raw = read_sectors_raw(phys / 512, sectors)?;
        result.extend_from_slice(raw.get(offset..offset + take)?);
        byte_len -= take;
        logical = logical.checked_add(take as u64)?;
    }
    Some(result)
}

pub(crate) fn write_logical(fs: &BtrfsFs, logical: u64, data: &[u8]) -> Result<(), &'static str> {
    if logical % 512 != 0 || data.len() % 512 != 0 { return Err("unaligned write"); }
    let end = logical.checked_add(data.len() as u64).ok_or("write overflow")?;
    let (start, _, chunk) = fs.chunk_map.iter()
        .find(|(a, b, _)| logical >= *a && end <= *b).ok_or("unmapped write")?;
    for (_, stripe) in &chunk.stripes {
        let phys = stripe.checked_add(logical - start).and_then(|v| v.checked_add(fs.lba_start.checked_mul(512)?))
            .ok_or("physical overflow")?;
        write_sectors_raw(phys / 512, data)?;
    }
    Ok(())
}
pub(crate) fn read_node(fs: &BtrfsFs, logical: u64) -> Option<Vec<u8>> {
    read_logical(fs, logical, fs.superblock.nodesize as usize)
}
pub(crate) fn write_node(fs: &BtrfsFs, logical: u64, data: &[u8]) -> Result<(), &'static str> {
    if data.len() != fs.superblock.nodesize as usize { return Err("invalid node size"); }
    write_logical(fs, logical, data)
}
