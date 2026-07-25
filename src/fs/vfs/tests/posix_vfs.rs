//! VFS POSIX-compliance integration tests.
//!
//! Each test group maps 1-to-1 to a task in issue #140.
//! All tests run in userspace (no kernel context required); the modules
//! under test are pure logic that takes caller-supplied metadata.

extern crate alloc;
use alloc::string::ToString;

// =====================================================================
// 1. open error codes
// =====================================================================

#[cfg(test)]
mod open_errors {
    use crate::fs::vfs::open_check::flags::*;
    use crate::fs::vfs::open_check::{open_preflight, InodeInfo, OpenAction, PATH_MAX};
    use crate::fs::vfs::perm::{Creds, InodePerm};

    fn ucreds() -> Creds {
        Creds {
            uid: 1000,
            gid: 1000,
            euid: 1000,
            egid: 1000,
        }
    }
    fn rcreds() -> Creds {
        Creds {
            uid: 0,
            gid: 0,
            euid: 0,
            egid: 0,
        }
    }

    fn existing(uid: u32, mode: u16, ro: bool) -> InodeInfo {
        InodeInfo {
            exists: true,
            mode,
            perm: InodePerm {
                uid,
                gid: uid,
                mode,
            },
            readonly: ro,
        }
    }
    fn missing_rw() -> InodeInfo {
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
    fn enoent_when_missing_no_creat() {
        assert_eq!(
            open_preflight("/x", O_RDONLY, ucreds(), missing_rw()),
            Err(-2)
        );
    }

    #[test]
    fn eacces_no_read_permission() {
        // file owned by root, mode 0o600 — other user has no read.
        assert_eq!(
            open_preflight("/s", O_RDONLY, ucreds(), existing(0, 0o100600, false)),
            Err(-13)
        );
    }

    #[test]
    fn eisdir_open_dir_wronly() {
        assert_eq!(
            open_preflight("/d", O_WRONLY, rcreds(), existing(0, 0o040755, false)),
            Err(-21)
        );
    }

    #[test]
    fn eexist_creat_excl_existing() {
        assert_eq!(
            open_preflight(
                "/f",
                O_CREAT | O_EXCL,
                ucreds(),
                existing(1000, 0o100644, false)
            ),
            Err(-17)
        );
    }

    #[test]
    fn erofs_write_on_readonly_mount() {
        assert_eq!(
            open_preflight("/ro/f", O_WRONLY, ucreds(), existing(1000, 0o100644, true)),
            Err(-30)
        );
    }

    #[test]
    fn enametoolong() {
        let long = alloc::string::String::from_utf8(alloc::vec![b'x'; PATH_MAX + 1]).unwrap();
        assert_eq!(
            open_preflight(&long, O_RDONLY, ucreds(), missing_rw()),
            Err(-36)
        );
    }

    #[test]
    fn open_ok_rdonly() {
        assert_eq!(
            open_preflight(
                "/etc/hosts",
                O_RDONLY,
                ucreds(),
                existing(1000, 0o100644, false)
            ),
            Ok(OpenAction::OpenExisting)
        );
    }

    #[test]
    fn open_creat_creates() {
        assert_eq!(
            open_preflight("/new", O_CREAT | O_WRONLY, ucreds(), missing_rw()),
            Ok(OpenAction::Create)
        );
    }
}

// =====================================================================
// 2. Permission matrix (owner / group / other × mode 0000/0444/0644/0755)
// =====================================================================

#[cfg(test)]
mod permission_matrix {
    use crate::fs::vfs::perm::{check_exec, check_read, check_write, Creds, InodePerm};

    fn owner() -> Creds {
        Creds {
            uid: 1000,
            gid: 1000,
            euid: 1000,
            egid: 1000,
        }
    }
    fn other() -> Creds {
        Creds {
            uid: 2000,
            gid: 2000,
            euid: 2000,
            egid: 2000,
        }
    }
    fn perm(uid: u32, gid: u32, mode: u16) -> InodePerm {
        InodePerm { uid, gid, mode }
    }

    // mode 0o000: nobody can read
    #[test]
    fn mode_0000_owner_no_read() {
        assert!(check_read(owner(), perm(1000, 1000, 0o100000)).is_err());
    }
    #[test]
    fn mode_0000_other_no_read() {
        assert!(check_read(other(), perm(1000, 1000, 0o100000)).is_err());
    }

    // mode 0o444: everyone can read, nobody can write
    #[test]
    fn mode_0444_owner_read_ok() {
        assert!(check_read(owner(), perm(1000, 1000, 0o100444)).is_ok());
    }
    #[test]
    fn mode_0444_other_read_ok() {
        assert!(check_read(other(), perm(1000, 1000, 0o100444)).is_ok());
    }
    #[test]
    fn mode_0444_owner_no_write() {
        assert!(check_write(owner(), perm(1000, 1000, 0o100444), false).is_err());
    }
    #[test]
    fn mode_0444_other_no_write() {
        assert!(check_write(other(), perm(1000, 1000, 0o100444), false).is_err());
    }

    // mode 0o644: owner rw, others r only
    #[test]
    fn mode_0644_owner_write_ok() {
        assert!(check_write(owner(), perm(1000, 1000, 0o100644), false).is_ok());
    }
    #[test]
    fn mode_0644_other_no_write() {
        assert!(check_write(other(), perm(1000, 1000, 0o100644), false).is_err());
    }
    #[test]
    fn mode_0644_other_read_ok() {
        assert!(check_read(other(), perm(1000, 1000, 0o100644)).is_ok());
    }

    // mode 0o755: owner rwx, others rx
    #[test]
    fn mode_0755_owner_exec_ok() {
        assert!(check_exec(owner(), perm(1000, 1000, 0o100755)).is_ok());
    }
    #[test]
    fn mode_0755_other_exec_ok() {
        assert!(check_exec(other(), perm(1000, 1000, 0o100755)).is_ok());
    }
    #[test]
    fn mode_0755_other_no_write() {
        assert!(check_write(other(), perm(1000, 1000, 0o100755), false).is_err());
    }
}

// =====================================================================
// 3. unlink: deferred deletion, EISDIR, sticky-bit
// =====================================================================

#[cfg(test)]
mod unlink_tests {
    use crate::fs::vfs::inode_ref::{dec_open, inc_open, InodeKey};
    use crate::fs::vfs::perm::{Creds, InodePerm};
    use crate::fs::vfs::unlink::{unlink_preflight, UnlinkOutcome, UnlinkTarget};

    fn key(n: u64) -> InodeKey {
        InodeKey { dev: 3, ino: n }
    }
    fn root() -> Creds {
        Creds {
            uid: 0,
            gid: 0,
            euid: 0,
            egid: 0,
        }
    }
    fn other() -> Creds {
        Creds {
            uid: 9000,
            gid: 9000,
            euid: 9000,
            egid: 9000,
        }
    }

    fn target(key: InodeKey, mode: u16, nlink: u32, sticky: bool) -> UnlinkTarget {
        let dir_mode = if sticky { 0o041777u16 } else { 0o040755u16 };
        UnlinkTarget {
            key,
            target_mode: mode,
            target_uid: 1000,
            parent_perm: InodePerm {
                uid: 0,
                gid: 0,
                mode: dir_mode,
            },
            readonly: false,
            nlink,
        }
    }

    #[test]
    fn deferred_free_when_fd_open() {
        let k = key(3001);
        inc_open(k);
        assert_eq!(
            unlink_preflight(root(), target(k, 0o100644, 1)),
            Ok(UnlinkOutcome::DeferFree)
        );
        dec_open(k); // cleanup
    }

    #[test]
    fn immediate_free_no_fds() {
        let k = key(3002);
        assert_eq!(
            unlink_preflight(root(), target(k, 0o100644, 1)),
            Ok(UnlinkOutcome::RemoveNow)
        );
    }

    #[test]
    fn eisdir_on_directory() {
        let k = key(3003);
        assert_eq!(unlink_preflight(root(), target(k, 0o040755, 2)), Err(-21));
    }

    #[test]
    fn sticky_eacces_non_owner() {
        let k = key(3004);
        // sticky dir, file owned by 1000, caller is 9000
        assert_eq!(
            unlink_preflight(other(), target(k, 0o100644, 1, true)),
            Err(-13)
        );
    }

    // File is still readable via open fd after unlink (deferred delete).
    #[test]
    fn fd_still_valid_after_unlink() {
        let k = key(3005);
        inc_open(k);
        let outcome = unlink_preflight(root(), target(k, 0o100644, 1));
        assert_eq!(outcome, Ok(UnlinkOutcome::DeferFree));
        // inode should NOT be freed yet
        use crate::fs::vfs::inode_ref::has_open_fds;
        assert!(has_open_fds(k));
        // now close the last fd
        let freed = dec_open(k);
        assert!(freed);
    }
}

// =====================================================================
// 4. rename: create, no-op, EXDEV, EINVAL, EISDIR, ENOTDIR, ENOTEMPTY
// =====================================================================

#[cfg(test)]
mod rename_tests {
    use crate::fs::vfs::rename::{rename_preflight, RenameArgs, RenameOutcome};

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
    fn create_ok() {
        assert_eq!(
            rename_preflight(base("/a", "/b")),
            Ok(RenameOutcome::Create)
        );
    }
    #[test]
    fn self_noop() {
        assert_eq!(rename_preflight(base("/f", "/f")), Ok(RenameOutcome::NoOp));
    }
    #[test]
    fn exdev() {
        let mut a = base("/a", "/b");
        a.new_dev = 2;
        assert_eq!(rename_preflight(a), Err(-18));
    }
    #[test]
    fn einval_descend() {
        let mut a = base("/a/b", "/a/b/c");
        a.old_mode = 0o040755;
        assert_eq!(rename_preflight(a), Err(-22));
    }
    #[test]
    fn eisdir_file_over_dir() {
        let mut a = base("/f", "/d");
        a.new_exists = true;
        a.new_mode = 0o040755;
        assert_eq!(rename_preflight(a), Err(-21));
    }
    #[test]
    fn enotdir_dir_over_file() {
        let mut a = base("/d", "/f");
        a.old_mode = 0o040755;
        a.new_exists = true;
        a.new_mode = 0o100644;
        assert_eq!(rename_preflight(a), Err(-20));
    }
    #[test]
    fn enotempty_dir_over_nonempty_dir() {
        let mut a = base("/d1", "/d2");
        a.old_mode = 0o040755;
        a.new_exists = true;
        a.new_mode = 0o040755;
        a.new_dir_empty = false;
        assert_eq!(rename_preflight(a), Err(-39));
    }
    #[test]
    fn dir_over_empty_dir_ok() {
        let mut a = base("/d1", "/d2");
        a.old_mode = 0o040755;
        a.new_exists = true;
        a.new_mode = 0o040755;
        a.new_dir_empty = true;
        assert_eq!(rename_preflight(a), Ok(RenameOutcome::Replace));
    }
}

// =====================================================================
// 5. link: ok, EPERM on dir, EXDEV, EEXIST, EMLINK, ENOENT
// =====================================================================

#[cfg(test)]
mod link_tests {
    use crate::fs::vfs::link::{link_preflight, LinkArgs, LINK_MAX};

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
    fn ok() {
        assert!(link_preflight(base()).is_ok());
    }
    #[test]
    fn eperm_dir() {
        let mut a = base();
        a.src_mode = 0o040755;
        assert_eq!(link_preflight(a), Err(-1));
    }
    #[test]
    fn exdev() {
        let mut a = base();
        a.dst_dev = 2;
        assert_eq!(link_preflight(a), Err(-18));
    }
    #[test]
    fn eexist() {
        let mut a = base();
        a.dst_exists = true;
        assert_eq!(link_preflight(a), Err(-17));
    }
    #[test]
    fn enoent() {
        let mut a = base();
        a.src_exists = false;
        assert_eq!(link_preflight(a), Err(-2));
    }
    #[test]
    fn emlink() {
        let mut a = base();
        a.src_nlink = LINK_MAX;
        assert_eq!(link_preflight(a), Err(-31));
    }
}

// =====================================================================
// 6. readlink: ok, EINVAL, dangling symlink
// =====================================================================

#[cfg(test)]
mod readlink_tests {
    use crate::fs::vfs::symlink::{vfs_readlink, FsOps, ResolveError};
    use alloc::collections::BTreeMap;
    use alloc::string::{String, ToString};

    struct MemFs {
        nodes: BTreeMap<String, Option<String>>,
    }
    impl MemFs {
        fn file(mut self, p: &str) -> Self {
            self.nodes.insert(p.to_string(), None);
            self
        }
        fn link(mut self, p: &str, t: &str) -> Self {
            self.nodes.insert(p.to_string(), Some(t.to_string()));
            self
        }
        fn new() -> Self {
            MemFs {
                nodes: BTreeMap::new(),
            }
        }
    }
    impl FsOps for MemFs {
        fn exists(&self, p: &str) -> bool {
            p == "/" || self.nodes.contains_key(p)
        }
        fn is_dir(&self, _: &str) -> bool {
            false
        }
        fn is_symlink(&self, p: &str) -> bool {
            self.nodes.get(p).and_then(|v| v.as_ref()).is_some()
        }
        fn readlink_raw(&self, p: &str) -> Result<String, ResolveError> {
            match self.nodes.get(p) {
                Some(Some(t)) => Ok(t.clone()),
                _ => Err(ResolveError::Inval),
            }
        }
    }

    #[test]
    fn readlink_ok() {
        let fs = MemFs::new().link("/link", "/target");
        assert_eq!(vfs_readlink(&fs, "/link"), Ok("/target".to_string()));
    }

    #[test]
    fn readlink_einval_regular_file() {
        let fs = MemFs::new().file("/regular");
        assert_eq!(vfs_readlink(&fs, "/regular"), Err(ResolveError::Inval));
    }

    #[test]
    fn readlink_dangling_returns_raw_target() {
        // Symlink exists, but its target does not — readlink still returns target.
        let fs = MemFs::new().link("/dangling", "/gone/path");
        assert_eq!(vfs_readlink(&fs, "/dangling"), Ok("/gone/path".to_string()));
    }

    #[test]
    fn readlink_enoent_missing_path() {
        let fs = MemFs::new();
        assert_eq!(vfs_readlink(&fs, "/nope"), Err(ResolveError::NoEnt));
    }
}

// =====================================================================
// 7. Symlink ELOOP at depth 41
// =====================================================================

#[cfg(test)]
mod symlink_loop_tests {
    use crate::fs::vfs::symlink::{resolve, FsOps, ResolveError, MAX_SYMLINK_DEPTH};
    use alloc::collections::BTreeMap;
    use alloc::format;
    use alloc::string::{String, ToString};

    struct MemFs {
        nodes: BTreeMap<String, Option<String>>,
    }
    impl MemFs {
        fn new() -> Self {
            MemFs {
                nodes: BTreeMap::new(),
            }
        }
        fn link(mut self, p: &str, t: &str) -> Self {
            self.nodes.insert(p.to_string(), Some(t.to_string()));
            self
        }
        fn file(mut self, p: &str) -> Self {
            self.nodes.insert(p.to_string(), None);
            self
        }
    }
    impl FsOps for MemFs {
        fn exists(&self, p: &str) -> bool {
            p == "/" || self.nodes.contains_key(p)
        }
        fn is_dir(&self, _: &str) -> bool {
            false
        }
        fn is_symlink(&self, p: &str) -> bool {
            self.nodes.get(p).and_then(|v| v.as_ref()).is_some()
        }
        fn readlink_raw(&self, p: &str) -> Result<String, ResolveError> {
            match self.nodes.get(p) {
                Some(Some(t)) => Ok(t.clone()),
                _ => Err(ResolveError::Inval),
            }
        }
    }

    /// Build a chain /s0->/s1->...->sN (N links total).
    fn build_chain(depth: u32) -> MemFs {
        let mut fs = MemFs::new();
        for i in 0..depth {
            let src = format!("/s{}", i);
            let dst = format!("/s{}", i + 1);
            fs = fs.link(&src, &dst);
        }
        // The final node is a regular file.
        let last = format!("/s{}", depth);
        fs.file(&last)
    }

    #[test]
    fn eloop_at_max_plus_one() {
        // MAX_SYMLINK_DEPTH + 1 hops -> ELOOP
        let fs = build_chain(MAX_SYMLINK_DEPTH + 1);
        assert_eq!(resolve(&fs, "/s0", 0), Err(ResolveError::Loop));
    }

    #[test]
    fn exactly_max_depth_succeeds() {
        // Exactly MAX_SYMLINK_DEPTH hops should resolve without error.
        let fs = build_chain(MAX_SYMLINK_DEPTH);
        let r = resolve(&fs, "/s0", 0);
        assert_eq!(r, Ok(format!("/s{}", MAX_SYMLINK_DEPTH)));
    }
}
