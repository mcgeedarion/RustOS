//! VFS-layer `open(2)` pre-flight error code audit.
//!
//! Centralises all POSIX-mandated error conditions for `open` so that each
//! backend driver does not have to re-implement them.  The dispatcher in
//! `ops.rs` calls `open_preflight` **before** forwarding to the backend.
//!
//! ## Error codes produced (Linux ABI)
//! | Errno        | Value | Condition                                         |
//! |--------------|-------|---------------------------------------------------|
//! | `ENOENT`     | -2    | Path does not exist and `O_CREAT` not set         |
//! | `EEXIST`     | -17   | `O_CREAT | O_EXCL` and file already exists        |
//! | `EISDIR`     | -21   | Final component is a directory + `O_WRONLY/RDWR`  |
//! | `ENOTDIR`    | -20   | Non-directory component used as directory         |
//! | `EACCES`     | -13   | DAC permission denied                             |
//! | `EROFS`      | -30   | Write on read-only mount                          |
//! | `ELOOP`      | -40   | Symlink loop (handled by symlink resolver)        |
//! | `ENAMETOOLONG` | -36 | Path or component exceeds kernel limits           |

extern crate alloc;

use crate::fs::vfs::perm::{check_open_type, check_read, check_write, Creds, InodePerm};

/// Maximum total path length (matches `PATH_MAX` on Linux).
pub const PATH_MAX: usize = 4096;
/// Maximum single filename component length (`NAME_MAX`).
pub const NAME_MAX: usize = 255;

/// Open flags (Linux ABI subset used in pre-flight).
pub mod flags {
    pub const O_RDONLY: u32 = 0x0000;
    pub const O_WRONLY: u32 = 0x0001;
    pub const O_RDWR: u32 = 0x0002;
    pub const O_CREAT: u32 = 0x0040;
    pub const O_EXCL: u32 = 0x0080;
    pub const O_TRUNC: u32 = 0x0200;
    pub const O_APPEND: u32 = 0x0400;
    pub const O_NOFOLLOW: u32 = 0x0002_0000;

    #[inline]
    pub fn is_write(f: u32) -> bool {
        f & (O_WRONLY | O_RDWR) != 0
    }
    #[inline]
    pub fn is_creat(f: u32) -> bool {
        f & O_CREAT != 0
    }
    #[inline]
    pub fn is_excl(f: u32) -> bool {
        f & O_EXCL != 0
    }
}

/// Inode metadata supplied by the caller after path resolution.
/// The caller is responsible for resolving symlinks (using `symlink::resolve`)
/// before invoking `open_preflight` — this struct describes the **final** node.
#[derive(Clone, Copy, Debug)]
pub struct InodeInfo {
    /// `true` iff the node exists on the filesystem.
    pub exists: bool,
    /// Mode word (file type + permission bits). Only meaningful if `exists`.
    pub mode: u16,
    /// Inode ownership. Only meaningful if `exists`.
    pub perm: InodePerm,
    /// `true` if the mount point is read-only.
    pub readonly: bool,
}

/// Outcome returned on success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenAction {
    /// Open the existing file.
    OpenExisting,
    /// Create a new file (O_CREAT and file does not exist).
    Create,
    /// Truncate existing file (O_TRUNC was set).
    Truncate,
}

/// Pre-flight check for `open(2)`.
///
/// Returns the `OpenAction` the backend should perform, or a negative
/// POSIX errno as `isize`.
pub fn open_preflight(
    path: &str,
    flags: u32,
    creds: Creds,
    info: InodeInfo,
) -> Result<OpenAction, isize> {
    // 1. ENAMETOOLONG
    if path.len() > PATH_MAX {
        return Err(-36);
    }
    for component in path.split('/') {
        if component.len() > NAME_MAX {
            return Err(-36);
        }
    }

    // 2. ENOENT / O_CREAT
    if !info.exists {
        if self::flags::is_creat(flags) {
            // Caller will create. Check write perm on parent dir is the
            // backend's responsibility (we only have the target here).
            if info.readonly {
                return Err(-30); // EROFS
            }
            return Ok(OpenAction::Create);
        }
        return Err(-2); // ENOENT
    }

    // 3. EEXIST: O_CREAT | O_EXCL and file already exists.
    if self::flags::is_creat(flags) && self::flags::is_excl(flags) {
        return Err(-17); // EEXIST
    }

    // 4. EISDIR / ENOTDIR: type checks.
    check_open_type(info.mode, flags, true).map_err(isize::from)?;

    // 5. DAC permission checks.
    if self::flags::is_write(flags) {
        check_write(creds, info.perm, info.readonly).map_err(isize::from)?;
    } else {
        check_read(creds, info.perm).map_err(isize::from)?;
    }

    // 6. O_TRUNC implies write intent even if not O_WRONLY.
    if flags & self::flags::O_TRUNC != 0 {
        if info.readonly {
            return Err(-30); // EROFS
        }
        return Ok(OpenAction::Truncate);
    }

    Ok(OpenAction::OpenExisting)
}

// ---- Tests ----------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::flags::*;
    use super::*;
    use crate::fs::vfs::perm::{Creds, InodePerm};

    fn root_creds() -> Creds {
        Creds {
            uid: 0,
            gid: 0,
            euid: 0,
            egid: 0,
        }
    }
    fn user_creds() -> Creds {
        Creds {
            uid: 1000,
            gid: 1000,
            euid: 1000,
            egid: 1000,
        }
    }
    fn rw_file(uid: u32, mode: u16) -> InodeInfo {
        InodeInfo {
            exists: true,
            mode,
            perm: InodePerm {
                uid,
                gid: uid,
                mode,
            },
            readonly: false,
        }
    }
    fn missing() -> InodeInfo {
        InodeInfo {
            exists: false,
            mode: 0,
            perm: InodePerm {
                uid: 0,
                gid: 0,
                mode: 0,
            },
            readonly: false,
        }
    }

    #[test]
    fn open_existing_rdonly_ok() {
        let r = open_preflight(
            "/etc/hosts",
            O_RDONLY,
            user_creds(),
            rw_file(1000, 0o100644),
        );
        assert_eq!(r, Ok(OpenAction::OpenExisting));
    }

    #[test]
    fn open_missing_no_creat_enoent() {
        let r = open_preflight("/nope", O_RDONLY, user_creds(), missing());
        assert_eq!(r, Err(-2));
    }

    #[test]
    fn open_missing_creat_ok() {
        let r = open_preflight("/new", O_CREAT | O_WRONLY, user_creds(), missing());
        assert_eq!(r, Ok(OpenAction::Create));
    }

    #[test]
    fn open_creat_excl_existing_eexist() {
        let r = open_preflight(
            "/exists",
            O_CREAT | O_EXCL,
            user_creds(),
            rw_file(1000, 0o100644),
        );
        assert_eq!(r, Err(-17));
    }

    #[test]
    fn open_dir_wronly_eisdir() {
        let info = InodeInfo {
            exists: true,
            mode: 0o040755, // directory
            perm: InodePerm {
                uid: 0,
                gid: 0,
                mode: 0o040755,
            },
            readonly: false,
        };
        let r = open_preflight("/etc", O_WRONLY, root_creds(), info);
        assert_eq!(r, Err(-21));
    }

    #[test]
    fn open_no_read_perm_eacces() {
        let r = open_preflight("/secret", O_RDONLY, user_creds(), rw_file(0, 0o100600));
        assert_eq!(r, Err(-13));
    }

    #[test]
    fn open_write_on_rofs_erofs() {
        let info = InodeInfo {
            exists: true,
            mode: 0o100644,
            perm: InodePerm {
                uid: 1000,
                gid: 1000,
                mode: 0o100644,
            },
            readonly: true,
        };
        let r = open_preflight("/ro/file", O_WRONLY, user_creds(), info);
        assert_eq!(r, Err(-30));
    }

    #[test]
    fn open_enametoolong() {
        let long = alloc::string::String::from_utf8(alloc::vec![b'a'; PATH_MAX + 1]).unwrap();
        let r = open_preflight(&long, O_RDONLY, user_creds(), missing());
        assert_eq!(r, Err(-36));
    }

    #[test]
    fn open_trunc_sets_truncate_action() {
        let r = open_preflight(
            "/f",
            O_WRONLY | O_TRUNC,
            user_creds(),
            rw_file(1000, 0o100644),
        );
        assert_eq!(r, Ok(OpenAction::Truncate));
    }
}
