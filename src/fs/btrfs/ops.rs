//! Real namespace operations for the bounded, exclusively owned leaf profile.
extern crate alloc;
use alloc::{string::String, vec::Vec};
use super::{directory as dir, extent, inode, superblock::{BtrfsFs, BtrfsInodeItem, S_IFMT, S_IFDIR, S_IFREG, S_IFLNK}, tree::Items};

fn file_type(mode: u32) -> u8 {
    match mode & S_IFMT { S_IFDIR => 2, S_IFLNK => 7, _ => 1 }
}
fn delete_link(items: &mut Items, parent: u64, name: &str, want_dir: bool) -> Result<(), isize> {
    let ino = dir::lookup(items, parent, name)?;
    let mut ii = inode::read(items, ino)?;
    inode::mutable(&ii)?;
    let is_dir = ii.mode & S_IFMT == S_IFDIR;
    if want_dir != is_dir { return Err(if is_dir { -21 } else { -20 }); }
    if is_dir && !dir::empty(items, ino) { return Err(-39); }
    if ii.nlink == 0 { return Err(-5); }
    if ii.nlink == 1 { extent::inline_only(items, ino)?; }
    dir::remove(items, parent, name)?;
    ii.nlink -= 1;
    if ii.nlink == 0 { items.retain(|k, _| k.objectid != ino); }
    else { inode::write(items, ino, &ii); }
    Ok(())
}

impl BtrfsFs {
    pub fn stat(&self, path: &str) -> Result<crate::fs::vfs_ops::KStat, isize> {
        let items = self.tree_items(self.fs_tree_root)?;
        let ino = dir::resolve(&items, self.root_dir, path)?;
        let ii = inode::read(&items, ino)?;
        let ns = |sec: u64, sub: u32| sec.saturating_mul(1_000_000_000).saturating_add(sub as u64);
        Ok(crate::fs::vfs_ops::KStat {
            ino, mode: ii.mode as u16, nlink: ii.nlink, uid: ii.uid, gid: ii.gid, size: ii.size,
            atime: ns(ii.atime_sec, ii.atime_nsec), mtime: ns(ii.mtime_sec, ii.mtime_nsec),
            ctime: ns(ii.ctime_sec, ii.ctime_nsec), blksize: self.superblock.sectorsize as u64,
            blocks: ii.nbytes.div_ceil(512), is_dir: ii.mode & S_IFMT == S_IFDIR,
        })
    }
    pub fn read_all(&self, path: &str) -> Result<Vec<u8>, isize> {
        let items = self.tree_items(self.fs_tree_root)?;
        let ino = dir::resolve(&items, self.root_dir, path)?;
        if inode::read(&items, ino)?.mode & S_IFMT == S_IFDIR { return Err(-21); }
        self.read_data(&items, ino)
    }
    pub fn readdir(&self, path: &str) -> Result<Vec<crate::fs::vfs_ops::DirEntry>, isize> {
        let items = self.tree_items(self.fs_tree_root)?;
        let ino = dir::resolve(&items, self.root_dir, path)?;
        if inode::read(&items, ino)?.mode & S_IFMT != S_IFDIR { return Err(-20); }
        let mut result = Vec::new();
        for (_, data) in items.iter().filter(|(k, _)| k.objectid == ino && k.ty == 96) {
            for (_, _, di) in dir::records(data)? {
                if di.child_key.ty != 1 { return Err(-95); }
                let ii = inode::read(&items, di.child_key.objectid)?;
                result.push(crate::fs::vfs_ops::DirEntry { name: di.name, ino: di.child_key.objectid,
                    mode: ii.mode as u16, size: ii.size, is_dir: ii.mode & S_IFMT == S_IFDIR });
            }
        }
        Ok(result)
    }
    pub fn readlink(&self, path: &str) -> Result<String, isize> {
        if self.stat(path)?.mode as u32 & S_IFMT != S_IFLNK { return Err(-22); }
        String::from_utf8(self.read_all(path)?).map_err(|_| -5)
    }
    pub fn statfs(&self) -> crate::fs::vfs_ops::KStatfs {
        let bs = self.superblock.sectorsize as u64;
        let free = self.superblock.total_bytes.saturating_sub(self.superblock.bytes_used) / bs;
        crate::fs::vfs_ops::KStatfs { f_type: 0x9123683e, f_bsize: bs,
            f_blocks: self.superblock.total_bytes / bs, f_bfree: free,
            // No extent allocator is exposed by this bounded write profile.
            f_bavail: 0, f_namelen: 255 }
    }
    fn create_kind(&mut self, path: &str, mode: u32, data: &[u8]) -> Result<(), isize> {
        let root = self.root_dir;
        let generation = self.superblock.generation;
        let max_inline = self.superblock.sectorsize as usize - 1;
        self.edit(|items| {
            let (parent, name) = dir::parent(items, root, path)?;
            let ino = items.keys().filter(|k| k.ty == 1).map(|k| k.objectid).max()
                .unwrap_or(256).checked_add(1).ok_or(-28isize)?;
            if ino >= u64::MAX - 255 { return Err(-28); }
            let ii = BtrfsInodeItem { generation, transid: generation, mode, nlink: 1, ..Default::default() };
            inode::write(items, ino, &ii);
            dir::insert(items, parent, name, ino, file_type(mode), generation)?;
            if !data.is_empty() { extent::replace_inline(items, ino, data, generation, max_inline)?; }
            Ok(())
        })
    }
    pub fn create(&mut self, path: &str) -> Result<(), isize> { self.create_kind(path, S_IFREG | 0o644, &[]) }
    pub fn mkdir(&mut self, path: &str) -> Result<(), isize> { self.create_kind(path, S_IFDIR | 0o755, &[]) }
    pub fn symlink(&mut self, target: &str, path: &str) -> Result<(), isize> {
        if target.is_empty() || target.contains('\0') { return Err(-22); }
        self.create_kind(path, S_IFLNK | 0o777, target.as_bytes())
    }
    pub fn write_all(&mut self, path: &str, data: &[u8]) -> Result<(), isize> {
        let root = self.root_dir;
        let generation = self.superblock.generation;
        let max_inline = self.superblock.sectorsize as usize - 1;
        self.edit(|items| {
            let ino = dir::resolve(items, root, path)?;
            if inode::read(items, ino)?.mode & S_IFMT != S_IFREG { return Err(-21); }
            extent::replace_inline(items, ino, data, generation, max_inline)
        })
    }
    pub fn truncate(&mut self, path: &str, new_len: u64) -> Result<(), isize> {
        if new_len >= self.superblock.sectorsize as u64 { return Err(-27); }
        let mut data = self.read_all(path)?;
        data.resize(new_len as usize, 0);
        self.write_all(path, &data)
    }
    pub fn unlink(&mut self, path: &str) -> Result<(), isize> {
        let root = self.root_dir;
        self.edit(|items| { let (p, n) = dir::parent(items, root, path)?; delete_link(items, p, n, false) })
    }
    pub fn rmdir(&mut self, path: &str) -> Result<(), isize> {
        let root = self.root_dir;
        self.edit(|items| { let (p, n) = dir::parent(items, root, path)?; delete_link(items, p, n, true) })
    }
    pub fn link(&mut self, existing: &str, new: &str) -> Result<(), isize> {
        let root = self.root_dir;
        let generation = self.superblock.generation;
        self.edit(|items| {
            let ino = dir::resolve(items, root, existing)?;
            let mut ii = inode::read(items, ino)?;
            inode::mutable(&ii)?;
            if ii.mode & S_IFMT == S_IFDIR { return Err(-1); }
            let (p, n) = dir::parent(items, root, new)?;
            dir::insert(items, p, n, ino, file_type(ii.mode), generation)?;
            ii.nlink = ii.nlink.checked_add(1).ok_or(-31isize)?;
            inode::write(items, ino, &ii);
            Ok(())
        })
    }
    pub fn rename(&mut self, old: &str, new: &str) -> Result<(), isize> {
        let root = self.root_dir;
        let generation = self.superblock.generation;
        self.edit(|items| {
            let (op, on) = dir::parent(items, root, old)?;
            let (np, nn) = dir::parent(items, root, new)?;
            let ino = dir::lookup(items, op, on)?;
            if op == np && on == nn { return Ok(()); }
            let ii = inode::read(items, ino)?;
            inode::mutable(&ii)?;
            let is_dir = ii.mode & S_IFMT == S_IFDIR;
            if is_dir && dir::descendant(items, root, ino, np)? { return Err(-22); }
            match dir::lookup(items, np, nn) {
                Ok(other) if other == ino => return Ok(()),
                Ok(_) => { delete_link(items, np, nn, is_dir)?; },
                Err(-2) => {},
                Err(e) => return Err(e),
            }
            dir::remove(items, op, on)?;
            dir::insert(items, np, nn, ino, file_type(ii.mode), generation)
        })
    }
    fn change_inode(&mut self, path: &str, f: impl FnOnce(&mut BtrfsInodeItem)) -> Result<(), isize> {
        let root = self.root_dir;
        self.edit(|items| {
            let ino = dir::resolve(items, root, path)?;
            let mut ii = inode::read(items, ino)?;
            inode::mutable(&ii)?;
            f(&mut ii);
            inode::write(items, ino, &ii);
            Ok(())
        })
    }
    pub fn chmod(&mut self, path: &str, mode: u32) -> Result<(), isize> {
        self.change_inode(path, |ii| ii.mode = (ii.mode & S_IFMT) | (mode & 0o7777))
    }
    pub fn chown(&mut self, path: &str, uid: u32, gid: u32) -> Result<(), isize> {
        self.change_inode(path, |ii| { if uid != u32::MAX { ii.uid = uid; } if gid != u32::MAX { ii.gid = gid; } })
    }
    /// VFS timestamps are nanoseconds, not seconds.
    pub fn set_times(&mut self, path: &str, atime: u64, mtime: u64) -> Result<(), isize> {
        self.change_inode(path, |ii| {
            ii.atime_sec = atime / 1_000_000_000; ii.atime_nsec = (atime % 1_000_000_000) as u32;
            ii.mtime_sec = mtime / 1_000_000_000; ii.mtime_nsec = (mtime % 1_000_000_000) as u32;
        })
    }
}
