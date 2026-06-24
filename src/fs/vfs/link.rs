//! POSIX-compliant `link(2)` VFS helper.
//!
//! ## POSIX requirements implemented
//! * `EPERM`  — target is a directory (hard links to dirs disallowed).
//! * `EXDEV`  — source and destination are on different mounts.
//! * `ENOENT` — source does not exist.
//! * `EEXIST` — destination already exists.
//! * `EMLINK` — nlink would exceed `LINK_MAX`.
//! * `EROFS`  — filesystem is read-only.
//!
//! The actual `nlink` increment and directory entry creation are performed
//! by the backend after this preflight returns `Ok(())`.

extern crate alloc;

/// Linux `LINK_MAX` (per `limits.h`).
pub const LINK_MAX: u32 = 65000;

const S_IFMT:  u16 = 0o170000;
const S_IFDIR: u16 = 0o040000;

#[inline]
fn is_dir(mode: u16) -> bool {
    mode & S_IFMT == S_IFDIR
}

/// Metadata the caller must provide for the `link(2)` preflight.
#[derive(Clone, Copy, Debug)]
pub struct LinkArgs {
    /// Device number of the source path's mount.
    pub src_dev: u64,
    /// Device number of the destination path's mount.
    pub dst_dev: u64,
    /// `true` if the source path exists.
    pub src_exists: bool,
    /// Mode of the source inode (only meaningful when `src_exists`).
    pub src_mode: u16,
    /// Current `nlink` of the source inode.
    pub src_nlink: u32,
    /// `true` if the destination path already exists.
    pub dst_exists: bool,
    /// `true` if the mount point is read-only.
    pub readonly: bool,
}

/// Validate a `link(2)` operation.
///
/// Returns `Ok(())` when the backend should proceed with:
///   1. Creating a new directory entry at `new_path` pointing to the source inode.
///   2. Atomically incrementing the inode's `nlink`.
pub fn link_preflight(args: LinkArgs) -> Result<(), isize> {
    // EROFS
    if args.readonly {
        return Err(-30);
    }
    // ENOENT: source must exist.
    if !args.src_exists {
        return Err(-2);
    }
    // EPERM: hard links to directories are not permitted.
    if is_dir(args.src_mode) {
        return Err(-1); // EPERM
    }
    // EXDEV: both paths must be on the same mount.
    if args.src_dev != args.dst_dev {
        return Err(-18); // EXDEV
    }
    // EEXIST: destination must not already exist.
    if args.dst_exists {
        return Err(-17); // EEXIST
    }
    // EMLINK: nlink must not overflow LINK_MAX.
    if args.src_nlink >= LINK_MAX {
        return Err(-31); // EMLINK
    }
    Ok(())
}

// ---- Tests ----------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> LinkArgs {
        LinkArgs {
            src_dev: 1,
            dst_dev: 1,
            src_exists: true,
            src_mode: 0o100644,
            src_nlink: 1,
            dst_exists: false,
            readonly: false,
        }
    }

    #[test]
    fn link_ok() {
        assert!(link_preflight(base()).is_ok());
    }

    #[test]
    fn link_eperm_on_dir() {
        let mut a = base();
        a.src_mode = 0o040755;
        assert_eq!(link_preflight(a), Err(-1));
    }

    #[test]
    fn link_exdev() {
        let mut a = base();
        a.dst_dev = 2;
        assert_eq!(link_preflight(a), Err(-18));
    }

    #[test]
    fn link_eexist() {
        let mut a = base();
        a.dst_exists = true;
        assert_eq!(link_preflight(a), Err(-17));
    }

    #[test]
    fn link_enoent() {
        let mut a = base();
        a.src_exists = false;
        assert_eq!(link_preflight(a), Err(-2));
    }

    #[test]
    fn link_emlink() {
        let mut a = base();
        a.src_nlink = LINK_MAX;
        assert_eq!(link_preflight(a), Err(-31));
    }

    #[test]
    fn link_erofs() {
        let mut a = base();
        a.readonly = true;
        assert_eq!(link_preflight(a), Err(-30));
    }
}
