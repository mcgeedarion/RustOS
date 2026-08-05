//! POSIX-compliant `unlink(2)` and `rmdir(2)` VFS helpers.
//!
//! ## POSIX requirements implemented
//! * Directory entry is removed immediately.
//! * Inode free is **deferred** until `nlink == 0` AND no open file descriptions remain (via
//!   `inode_ref::mark_unlinked`).
//! * `EISDIR`  — target is a directory; use `rmdir` instead.
//! * `ENOENT`  — target does not exist.
//! * `EACCES` / `EPERM` — write permission on parent dir missing, or sticky bit blocks the caller.

extern crate alloc;

use crate::fs::vfs::inode_ref::{mark_unlinked, InodeKey};
use crate::fs::vfs::perm::{check_dir_write, Creds, InodePerm, PermError};

/// Metadata the caller must supply about the target and its parent directory.
#[derive(Clone, Copy, Debug)]
pub struct UnlinkTarget {
    /// Device+inode of the target file.
    pub key: InodeKey,
    /// Mode of the target (used to reject directories).
    pub target_mode: u16,
    /// uid of the target file (for sticky-bit check).
    pub target_uid: u32,
    /// Permission info for the parent directory.
    pub parent_perm: InodePerm,
    /// `true` if the mount is read-only.
    pub readonly: bool,
    /// Current `nlink` count of the target inode.
    pub nlink: u32,
}

/// Result of a successful `unlink` preflight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnlinkOutcome {
    /// Remove the directory entry; inode data can be freed now.
    RemoveNow,
    /// Remove the directory entry; inode data must be kept until all fds close.
    DeferFree,
}

const S_IFMT: u16 = 0o170000;
const S_IFDIR: u16 = 0o040000;

#[inline]
fn is_dir(mode: u16) -> bool {
    mode & S_IFMT == S_IFDIR
}

/// Validate an `unlink(2)` operation and update the inode-ref table.
///
/// On `Ok(UnlinkOutcome::RemoveNow)` the caller should delete the directory
/// entry **and** ask the backend to free the inode.
/// On `Ok(UnlinkOutcome::DeferFree)` the caller should delete the directory
/// entry only; the backend frees the inode when `inode_ref::dec_open` later
/// returns `true`.
pub fn unlink_preflight(creds: Creds, target: UnlinkTarget) -> Result<UnlinkOutcome, isize> {
    // EISDIR: unlink may not remove directories.
    if is_dir(target.target_mode) {
        return Err(-21); // EISDIR
    }

    // Permission check: caller must have write on parent + pass sticky.
    check_dir_write(
        creds,
        target.parent_perm,
        Some(target.target_uid),
        target.readonly,
    )
    .map_err(isize::from)?;

    // Compute new nlink after removal.
    let new_nlink = target.nlink.saturating_sub(1);

    if new_nlink == 0 {
        // Last hard link removed — defer or free based on open fds.
        let free_now = mark_unlinked(target.key);
        if free_now {
            Ok(UnlinkOutcome::RemoveNow)
        } else {
            Ok(UnlinkOutcome::DeferFree)
        }
    } else {
        // Other hard links remain — just remove this directory entry.
        Ok(UnlinkOutcome::RemoveNow)
    }
}

// ---- Tests ----------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::vfs::inode_ref::{inc_open, InodeKey};
    use crate::fs::vfs::perm::{Creds, InodePerm};

    fn key(ino: u64) -> InodeKey {
        InodeKey { dev: 2, ino }
    }
    fn root() -> Creds {
        Creds {
            uid: 0,
            gid: 0,
            euid: 0,
            egid: 0,
        }
    }
    fn user() -> Creds {
        Creds {
            uid: 1000,
            gid: 1000,
            euid: 1000,
            egid: 1000,
        }
    }

    fn dir_perm(uid: u32, mode: u16) -> InodePerm {
        InodePerm {
            uid,
            gid: uid,
            mode,
        }
    }

    fn make_target(key: InodeKey, mode: u16, nlink: u32) -> UnlinkTarget {
        UnlinkTarget {
            key,
            target_mode: mode,
            target_uid: 1000,
            parent_perm: dir_perm(0, 0o040755),
            readonly: false,
            nlink,
        }
    }

    #[test]
    fn unlink_regular_file_no_fds() {
        let k = key(2001);
        let r = unlink_preflight(root(), make_target(k, 0o100644, 1));
        assert_eq!(r, Ok(UnlinkOutcome::RemoveNow));
    }

    #[test]
    fn unlink_regular_file_with_open_fd_deferred() {
        let k = key(2002);
        inc_open(k); // simulate open fd
        let r = unlink_preflight(root(), make_target(k, 0o100644, 1));
        assert_eq!(r, Ok(UnlinkOutcome::DeferFree));
    }

    #[test]
    fn unlink_dir_eisdir() {
        let k = key(2003);
        let r = unlink_preflight(root(), make_target(k, 0o040755, 2));
        assert_eq!(r, Err(-21)); // EISDIR
    }

    #[test]
    fn unlink_multi_link_remove_now() {
        // nlink == 3: removing one entry just removes that dentry, inode stays.
        let k = key(2004);
        let r = unlink_preflight(root(), make_target(k, 0o100644, 3));
        assert_eq!(r, Ok(UnlinkOutcome::RemoveNow));
    }

    #[test]
    fn unlink_sticky_blocks_non_owner() {
        let k = key(2005);
        let mut t = make_target(k, 0o100644, 1);
        // Sticky parent directory owned by root.
        t.parent_perm = dir_perm(0, 0o041777); // drwxrwxrwt
        t.target_uid = 1000;
        // Caller is uid 2000 — not file owner, not dir owner, not root.
        let other = Creds {
            uid: 2000,
            gid: 2000,
            euid: 2000,
            egid: 2000,
        };
        let r = unlink_preflight(other, t);
        assert_eq!(r, Err(-13)); // EACCES
    }

    #[test]
    fn unlink_readonly_erofs() {
        let k = key(2006);
        let mut t = make_target(k, 0o100644, 1);
        t.readonly = true;
        let r = unlink_preflight(root(), t);
        assert_eq!(r, Err(-30)); // EROFS
    }
}
