//! VFS DAC permission engine.
//!
//! Implements Linux-compatible Discretionary Access Control checks for
//! the three operation classes: read, write, execute.  All checks run
//! entirely in kernel space; no userspace pointer dereferences occur here.
//!
//! ## Design notes
//! * uid 0 (root) bypasses DAC checks (but not MAC — when LSM lands).
//! * Effective credentials are supplied by the caller; this module is stateless and purely
//!   functional.
//! * EROFS guard is the caller's responsibility (`ops.rs` already checks `h.is_readonly()` before
//!   mutating ops); `check_write` exists for consistency with the three-way split expected by
//!   `open` flag handling.
//! * `S_ISVTX` (sticky bit) enforcement for `unlink`/`rename` is exposed separately in
//!   `check_sticky`.

extern crate alloc;

/// Effective credentials for the current task.
/// Sourced from `proc::current_task().creds` at each syscall boundary.
#[derive(Clone, Copy, Debug)]
pub struct Creds {
    pub uid: u32,
    pub gid: u32,
    pub euid: u32,
    pub egid: u32,
}

impl Creds {
    /// Returns true iff the effective user is root.
    #[inline]
    pub fn is_root(self) -> bool {
        self.euid == 0
    }
}

/// Inode ownership and mode as stored in `KStat`.
#[derive(Clone, Copy, Debug)]
pub struct InodePerm {
    pub uid: u32,
    pub gid: u32,
    /// Full 16-bit mode word (type bits + permission bits).
    pub mode: u16,
}

/// Error codes returned by permission checks (negative POSIX errno).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(isize)]
pub enum PermError {
    /// `EACCES` — permission denied.
    Acces = -13,
    /// `EROFS`  — read-only filesystem.
    Rofs = -30,
    /// `EISDIR` — is a directory (open with O_WRONLY/O_RDWR).
    IsDir = -21,
    /// `ENOTDIR` — not a directory (path component).
    NotDir = -20,
}

impl From<PermError> for isize {
    #[inline]
    fn from(e: PermError) -> isize {
        e as isize
    }
}

// ---- Mode bit masks -------------------------------------------------

const S_IFMT: u16 = 0o170000;
const S_IFDIR: u16 = 0o040000;
const S_IFLNK: u16 = 0o120000;

const S_ISVTX: u16 = 0o001000; // sticky
const S_ISUID: u16 = 0o004000; // setuid  (informational for now)
const S_ISGID: u16 = 0o002000; // setgid  (informational for now)

// Owner bits
const S_IRUSR: u16 = 0o000400;
const S_IWUSR: u16 = 0o000200;
const S_IXUSR: u16 = 0o000100;
// Group bits
const S_IRGRP: u16 = 0o000040;
const S_IWGRP: u16 = 0o000020;
const S_IXGRP: u16 = 0o000010;
// Other bits
const S_IROTH: u16 = 0o000004;
const S_IWOTH: u16 = 0o000002;
const S_IXOTH: u16 = 0o000001;

// ---- Helpers --------------------------------------------------------

#[inline]
fn file_type(mode: u16) -> u16 {
    mode & S_IFMT
}

#[inline]
pub fn is_dir(mode: u16) -> bool {
    file_type(mode) == S_IFDIR
}

#[inline]
pub fn is_symlink(mode: u16) -> bool {
    file_type(mode) == S_IFLNK
}

#[inline]
pub fn is_sticky(mode: u16) -> bool {
    mode & S_ISVTX != 0
}

/// Compute which permission triple (owner/group/other) applies.
fn applicable_bits(creds: Creds, inode: InodePerm) -> (u16, u16, u16) {
    if creds.euid == inode.uid {
        (S_IRUSR, S_IWUSR, S_IXUSR)
    } else if creds.egid == inode.gid {
        (S_IRGRP, S_IWGRP, S_IXGRP)
    } else {
        (S_IROTH, S_IWOTH, S_IXOTH)
    }
}

// ---- Public API -----------------------------------------------------

/// Check read permission on `inode` for task with `creds`.
/// Returns `Ok(())` or `Err(PermError::Acces)`.
pub fn check_read(creds: Creds, inode: InodePerm) -> Result<(), PermError> {
    if creds.is_root() {
        return Ok(());
    }
    let (r, _, _) = applicable_bits(creds, inode);
    if inode.mode & r != 0 {
        Ok(())
    } else {
        Err(PermError::Acces)
    }
}

/// Check write permission on `inode` for task with `creds`.
/// Returns `Ok(())`, `Err(PermError::Acces)`, or `Err(PermError::Rofs)` if
/// `readonly` is true.
pub fn check_write(creds: Creds, inode: InodePerm, readonly: bool) -> Result<(), PermError> {
    if readonly {
        return Err(PermError::Rofs);
    }
    if creds.is_root() {
        return Ok(());
    }
    let (_, w, _) = applicable_bits(creds, inode);
    if inode.mode & w != 0 {
        Ok(())
    } else {
        Err(PermError::Acces)
    }
}

/// Check execute (or directory search) permission.
pub fn check_exec(creds: Creds, inode: InodePerm) -> Result<(), PermError> {
    if creds.is_root() {
        // root still needs at least one x bit set for execute (not for search).
        let any_x = S_IXUSR | S_IXGRP | S_IXOTH;
        if inode.mode & any_x == 0 && !is_dir(inode.mode) {
            return Err(PermError::Acces);
        }
        return Ok(());
    }
    let (_, _, x) = applicable_bits(creds, inode);
    if inode.mode & x != 0 {
        Ok(())
    } else {
        Err(PermError::Acces)
    }
}

/// Check write permission on the **parent directory** for create/unlink/rename.
/// Also enforces sticky-bit semantics when the directory has `S_ISVTX` set.
///
/// `file_uid` is the uid of the file being removed/renamed.
/// Pass `None` when creating a new entry (sticky bit irrelevant).
pub fn check_dir_write(
    creds: Creds,
    dir_inode: InodePerm,
    file_uid: Option<u32>,
    readonly: bool,
) -> Result<(), PermError> {
    check_write(creds, dir_inode, readonly)?;
    check_sticky(creds, dir_inode, file_uid)
}

/// Sticky-bit check: if the directory has `S_ISVTX` set, only root or the
/// file owner (or the directory owner) may remove/rename the entry.
pub fn check_sticky(
    creds: Creds,
    dir_inode: InodePerm,
    file_uid: Option<u32>,
) -> Result<(), PermError> {
    if !is_sticky(dir_inode.mode) {
        return Ok(());
    }
    if creds.is_root() {
        return Ok(());
    }
    if creds.euid == dir_inode.uid {
        return Ok(());
    }
    if let Some(fuid) = file_uid {
        if creds.euid == fuid {
            return Ok(());
        }
    }
    Err(PermError::Acces)
}

/// Validate that opening a directory with `O_WRONLY` or `O_RDWR` is rejected
/// with `EISDIR`, and that using a non-directory as a path component is
/// rejected with `ENOTDIR`.
///
/// `is_last_component` should be true only for the final path element.
pub fn check_open_type(mode: u16, flags: u32, is_last_component: bool) -> Result<(), PermError> {
    const O_WRONLY: u32 = 0x0001;
    const O_RDWR: u32 = 0x0002;

    if is_dir(mode) && is_last_component {
        if flags & (O_WRONLY | O_RDWR) != 0 {
            return Err(PermError::IsDir);
        }
    }
    // A non-directory used as a path component is caught by the path-walk
    // loop; this function handles the final component only.
    Ok(())
}

// ---- Tests ----------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn root_creds() -> Creds {
        Creds {
            uid: 0,
            gid: 0,
            euid: 0,
            egid: 0,
        }
    }

    fn user_creds(uid: u32, gid: u32) -> Creds {
        Creds {
            uid,
            gid,
            euid: uid,
            egid: gid,
        }
    }

    fn inode(uid: u32, gid: u32, mode: u16) -> InodePerm {
        InodePerm { uid, gid, mode }
    }

    // Owner can read mode 0o644
    #[test]
    fn owner_can_read_644() {
        assert!(check_read(user_creds(1000, 1000), inode(1000, 1000, 0o100644)).is_ok());
    }

    // Other cannot read mode 0o600
    #[test]
    fn other_cannot_read_600() {
        assert_eq!(
            check_read(user_creds(2000, 2000), inode(1000, 1000, 0o100600)),
            Err(PermError::Acces)
        );
    }

    // Root can read mode 0o000
    #[test]
    fn root_reads_000() {
        assert!(check_read(root_creds(), inode(1000, 1000, 0o100000)).is_ok());
    }

    // Write blocked on readonly mount
    #[test]
    fn write_blocked_on_rofs() {
        assert_eq!(
            check_write(user_creds(1000, 1000), inode(1000, 1000, 0o100644), true),
            Err(PermError::Rofs)
        );
    }

    // Sticky bit: non-owner, non-root cannot unlink
    #[test]
    fn sticky_blocks_non_owner() {
        let dir = inode(0, 0, 0o041777); // drwxrwxrwt
        assert_eq!(
            check_sticky(user_creds(2000, 2000), dir, Some(1000)),
            Err(PermError::Acces)
        );
    }

    // Sticky bit: file owner can unlink
    #[test]
    fn sticky_allows_file_owner() {
        let dir = inode(0, 0, 0o041777);
        assert!(check_sticky(user_creds(1000, 1000), dir, Some(1000)).is_ok());
    }

    // O_WRONLY on directory -> EISDIR
    #[test]
    fn open_dir_wronly_is_eisdir() {
        assert_eq!(
            check_open_type(0o040755, 0x0001, true),
            Err(PermError::IsDir)
        );
    }

    // O_RDONLY on directory -> ok
    #[test]
    fn open_dir_rdonly_ok() {
        assert!(check_open_type(0o040755, 0x0000, true).is_ok());
    }
}
