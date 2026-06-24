//! Inode reference-count table for deferred deletion.
//!
//! ## POSIX requirement
//! `unlink(2)` must remove the directory entry immediately but must not free
//! the inode (or its data blocks) until **both** conditions hold:
//!   1. `nlink == 0` (no remaining hard links), **and**
//!   2. No open file descriptions refer to the inode.
//!
//! This module maintains an in-kernel table of `(dev, ino) → open_count`
//! pairs.  The VFS ops layer calls `inc_open` on `open()` and `dec_open` on
//! `close()`.  `unlink` calls `mark_unlinked`; the combination of
//! `nlink == 0` and `open_count == 0` triggers actual block reclaim via the
//! backend's `inode_free` hook.
//!
//! ## Locking
//! A single `SpinMutex` guards the entire table.  Contention is expected to
//! be low because the hot path (read / write on an already-open fd) never
//! touches this table.

extern crate alloc;
use alloc::collections::BTreeMap;
use crate::sync::SpinMutex; // kernel spin-lock already used elsewhere

/// Composite device+inode key (matches Linux `struct inode` identity).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InodeKey {
    /// Device number (major/minor packed as u64, or mount-point index).
    pub dev: u64,
    /// Inode number on that device.
    pub ino: u64,
}

#[derive(Debug, Default)]
struct Entry {
    /// Number of open file descriptions currently referencing this inode.
    open_count: u32,
    /// True once all hard links have been removed via `unlink`/`rmdir`.
    unlinked: bool,
}

/// Global inode reference table.
static INODE_REFS: SpinMutex<BTreeMap<InodeKey, Entry>> =
    SpinMutex::new(BTreeMap::new());

// ---- Public API -----------------------------------------------------

/// Called by `open()` after a successful inode lookup.
/// Increments the open-fd counter for `key`.
pub fn inc_open(key: InodeKey) {
    let mut table = INODE_REFS.lock();
    table.entry(key).or_default().open_count += 1;
}

/// Called by `close()` (last fd referencing this description).
/// Returns `true` if the inode should now be freed (nlink==0 and last fd).
pub fn dec_open(key: InodeKey) -> bool {
    let mut table = INODE_REFS.lock();
    if let Some(entry) = table.get_mut(&key) {
        if entry.open_count > 0 {
            entry.open_count -= 1;
        }
        let should_free = entry.unlinked && entry.open_count == 0;
        if should_free {
            table.remove(&key);
        }
        return should_free;
    }
    false
}

/// Called by `unlink()` / `rmdir()` after the directory entry is removed
/// and `nlink` has been decremented to 0 at the backend.
///
/// Returns `true` if the inode can be freed immediately (no open fds).
pub fn mark_unlinked(key: InodeKey) -> bool {
    let mut table = INODE_REFS.lock();
    let entry = table.entry(key).or_default();
    entry.unlinked = true;
    let should_free = entry.open_count == 0;
    if should_free {
        table.remove(&key);
    }
    should_free
}

/// Query whether an inode currently has open file descriptions.
/// Used by `unlink` to decide whether to proceed with block reclaim.
pub fn has_open_fds(key: InodeKey) -> bool {
    let table = INODE_REFS.lock();
    table.get(&key).map_or(false, |e| e.open_count > 0)
}

/// Returns the current open-fd count. Primarily for testing.
#[cfg(test)]
pub fn open_count(key: InodeKey) -> u32 {
    let table = INODE_REFS.lock();
    table.get(&key).map_or(0, |e| e.open_count)
}

// ---- Tests ----------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn key(ino: u64) -> InodeKey {
        InodeKey { dev: 1, ino }
    }

    #[test]
    fn inc_dec_basic() {
        let k = key(1001);
        inc_open(k);
        inc_open(k);
        assert_eq!(open_count(k), 2);
        assert!(!dec_open(k)); // still 1 open fd, not freed
        assert_eq!(open_count(k), 1);
    }

    #[test]
    fn unlink_with_open_fd_deferred() {
        let k = key(1002);
        inc_open(k);
        // nlink drops to 0, but fd is open — must NOT free yet
        let should_free = mark_unlinked(k);
        assert!(!should_free, "must defer when fd is open");
        assert!(has_open_fds(k));
        // last close triggers free
        let freed = dec_open(k);
        assert!(freed, "must free after last close");
    }

    #[test]
    fn unlink_no_open_fd_immediate() {
        let k = key(1003);
        // No open fds — unlink should free immediately
        let should_free = mark_unlinked(k);
        assert!(should_free);
    }

    #[test]
    fn unlink_then_open_is_not_tracked() {
        // After unlink, a new open on the same ino should still work
        // (the table entry was removed). Simulate by just inc_open again.
        let k = key(1004);
        mark_unlinked(k); // immediate free, entry removed
        inc_open(k);       // new open after removal → fresh entry
        assert_eq!(open_count(k), 1);
        assert!(!dec_open(k)); // not unlinked this time
    }
}
