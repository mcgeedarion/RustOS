//! POSIX-compliant `rename(2)` VFS helper.
//!
//! ## POSIX requirements implemented
//! * Atomic replacement: no window where neither name exists.
//! * Cross-directory rename supported (different parent inodes, same mount).
//! * `EXDEV`      — cross-mount rename.
//! * `ENOTEMPTY` / `EEXIST` — destination is a non-empty directory.
//! * `EISDIR`    — destination is directory but source is not.
//! * `ENOTDIR`   — source is directory but destination is a non-directory file.
//! * Self-rename  — `rename(a, a)` is a no-op, returns 0.
//! * `EINVAL`    — cannot rename ancestor into own descendant.

extern crate alloc;
use alloc::string::String;

const S_IFMT: u16 = 0o170000;
const S_IFDIR: u16 = 0o040000;

#[inline]
fn is_dir(mode: u16) -> bool {
    mode & S_IFMT == S_IFDIR
}

/// Whether the destination path is a subdirectory of the source path.
///
/// E.g. `src = "/a/b"`, `dst = "/a/b/c"` → true.
fn dst_is_descendant_of_src(src: &str, dst: &str) -> bool {
    if dst == src {
        return false; // self-rename handled separately
    }
    let prefix = if src.ends_with('/') {
        String::from(src)
    } else {
        alloc::format!("{}/", src)
    };
    dst.starts_with(prefix.as_str())
}

/// Metadata the caller must provide for `rename(2)` preflight.
#[derive(Clone, Copy, Debug)]
pub struct RenameArgs<'a> {
    /// Absolute source path.
    pub old_path: &'a str,
    /// Absolute destination path.
    pub new_path: &'a str,
    /// Device of the source mount.
    pub old_dev: u64,
    /// Device of the destination mount.
    pub new_dev: u64,
    /// Mode of the source inode.
    pub old_mode: u16,
    /// `true` if the destination already exists.
    pub new_exists: bool,
    /// Mode of the destination inode (only meaningful when `new_exists`).
    pub new_mode: u16,
    /// `true` if the destination directory is non-empty (only meaningful
    /// when `new_exists && is_dir(new_mode)`).
    pub new_dir_empty: bool,
    /// `true` if the mount is read-only.
    pub readonly: bool,
}

/// Outcome returned on success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenameOutcome {
    /// Self-rename: nothing to do.
    NoOp,
    /// Replace the destination atomically.
    Replace,
    /// Create new entry (destination does not exist).
    Create,
}

/// Validate a `rename(2)` operation.
pub fn rename_preflight(args: RenameArgs<'_>) -> Result<RenameOutcome, isize> {
    // EROFS
    if args.readonly {
        return Err(-30);
    }
    // XDEV: different mounts.
    if args.old_dev != args.new_dev {
        return Err(-18); // EXDEV
    }
    // Self-rename: no-op.
    if args.old_path == args.new_path {
        return Ok(RenameOutcome::NoOp);
    }
    // EINVAL: attempt to rename a directory into itself or a descendant.
    if is_dir(args.old_mode) && dst_is_descendant_of_src(args.old_path, args.new_path) {
        return Err(-22); // EINVAL
    }
    // Destination exists — check type compatibility.
    if args.new_exists {
        let src_is_dir = is_dir(args.old_mode);
        let dst_is_dir = is_dir(args.new_mode);

        if src_is_dir && !dst_is_dir {
            return Err(-20); // ENOTDIR (dst must be a dir when src is a dir)
        }
        if !src_is_dir && dst_is_dir {
            return Err(-21); // EISDIR (dst is a dir but src is not)
        }
        if dst_is_dir && !args.new_dir_empty {
            return Err(-39); // ENOTEMPTY
        }
        return Ok(RenameOutcome::Replace);
    }
    Ok(RenameOutcome::Create)
}

// ---- Tests ----------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn base<'a>(old: &'a str, new: &'a str) -> RenameArgs<'a> {
        RenameArgs {
            old_path: old,
            new_path: new,
            old_dev: 1,
            new_dev: 1,
            old_mode: 0o100644,
            new_exists: false,
            new_mode: 0,
            new_dir_empty: true,
            readonly: false,
        }
    }

    #[test]
    fn rename_create_ok() {
        let r = rename_preflight(base("/a/old", "/a/new"));
        assert_eq!(r, Ok(RenameOutcome::Create));
    }

    #[test]
    fn rename_self_noop() {
        let r = rename_preflight(base("/a/f", "/a/f"));
        assert_eq!(r, Ok(RenameOutcome::NoOp));
    }

    #[test]
    fn rename_exdev() {
        let mut a = base("/a", "/b");
        a.new_dev = 2;
        assert_eq!(rename_preflight(a), Err(-18));
    }

    #[test]
    fn rename_dir_into_descendant_einval() {
        let mut a = base("/a/b", "/a/b/c");
        a.old_mode = 0o040755;
        assert_eq!(rename_preflight(a), Err(-22));
    }

    #[test]
    fn rename_file_over_dir_eisdir() {
        let mut a = base("/f", "/d");
        a.new_exists = true;
        a.new_mode = 0o040755; // dst is dir
                               // src is file (old_mode default 0o100644)
        assert_eq!(rename_preflight(a), Err(-21));
    }

    #[test]
    fn rename_dir_over_file_enotdir() {
        let mut a = base("/d", "/f");
        a.old_mode = 0o040755; // src is dir
        a.new_exists = true;
        a.new_mode = 0o100644; // dst is file
        assert_eq!(rename_preflight(a), Err(-20));
    }

    #[test]
    fn rename_dir_over_nonempty_dir_enotempty() {
        let mut a = base("/d1", "/d2");
        a.old_mode = 0o040755;
        a.new_exists = true;
        a.new_mode = 0o040755;
        a.new_dir_empty = false; // non-empty
        assert_eq!(rename_preflight(a), Err(-39));
    }

    #[test]
    fn rename_dir_over_empty_dir_ok() {
        let mut a = base("/d1", "/d2");
        a.old_mode = 0o040755;
        a.new_exists = true;
        a.new_mode = 0o040755;
        a.new_dir_empty = true;
        assert_eq!(rename_preflight(a), Ok(RenameOutcome::Replace));
    }

    #[test]
    fn rename_erofs() {
        let mut a = base("/a", "/b");
        a.readonly = true;
        assert_eq!(rename_preflight(a), Err(-30));
    }
}
