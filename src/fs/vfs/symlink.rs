//! Symlink resolution with POSIX `ELOOP` detection.
//!
//! ## POSIX requirements implemented
//! * Track resolution depth; return `ELOOP` after `MAX_SYMLINK_DEPTH` (40) levels.
//! * `readlink()` returns the raw target string — does **not** resolve.
//! * `EINVAL` from `readlink()` when the target is not a symlink.
//! * Dangling symlinks: `ENOENT` on dereference, but `readlink()` still succeeds.
//! * `O_NOFOLLOW`: return `ELOOP` if the final path component is a symlink.
//! * Absolute symlink targets restart resolution from VFS root (`/`).

extern crate alloc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Maximum number of symlink dereferences in a single path resolution
/// (matches Linux `fs/namei.c` `MAXSYMLINKS`).
pub const MAX_SYMLINK_DEPTH: u32 = 40;

/// `O_NOFOLLOW` open flag bit (matches Linux ABI).
const O_NOFOLLOW: u32 = 0x0002_0000;

/// Error codes returned by this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(isize)]
pub enum ResolveError {
    /// `ENOENT` — no such file or directory.
    NoEnt  = -2,
    /// `ENOTDIR` — a non-directory component appears in the path.
    NotDir = -20,
    /// `ELOOP`  — too many levels of symbolic links.
    Loop   = -40,
    /// `EINVAL` — not a symbolic link (for `readlink`).
    Inval  = -22,
    /// `EIO`    — I/O error during symlink read.
    Io     = -5,
}

impl From<ResolveError> for isize {
    #[inline]
    fn from(e: ResolveError) -> isize {
        e as isize
    }
}

/// Trait the resolver calls for filesystem operations. Implement this
/// for the VFS mount table to decouple the resolution algorithm.
pub trait FsOps {
    /// Returns `true` if `path` names an existing node.
    fn exists(&self, path: &str) -> bool;
    /// Returns `true` if `path` names a directory.
    fn is_dir(&self, path: &str) -> bool;
    /// Returns `true` if `path` names a symlink.
    fn is_symlink(&self, path: &str) -> bool;
    /// Reads the raw target of a symlink. Returns `Err(ResolveError::NoEnt)`
    /// for a dangling symlink that was valid at stat time, or
    /// `Err(ResolveError::Inval)` if the node is not a symlink.
    fn readlink_raw(&self, path: &str) -> Result<String, ResolveError>;
}

/// Context carried through one top-level `resolve` call.
struct Ctx<'a> {
    ops: &'a dyn FsOps,
    depth: u32,
}

impl<'a> Ctx<'a> {
    fn new(ops: &'a dyn FsOps) -> Self {
        Ctx { ops, depth: 0 }
    }

    fn bump_depth(&mut self) -> Result<(), ResolveError> {
        self.depth += 1;
        if self.depth > MAX_SYMLINK_DEPTH {
            Err(ResolveError::Loop)
        } else {
            Ok(())
        }
    }
}

/// Walk a single component of a split path, handling symlinks inline.
///
/// Returns the canonical prefix so far (updated in-place) and may push
/// additional components onto `remaining` when a symlink is encountered.
fn walk_component(
    ctx: &mut Ctx<'_>,
    current: &mut String,
    component: &str,
    remaining: &mut Vec<String>,
    is_last: bool,
    flags: u32,
) -> Result<(), ResolveError> {
    // Build candidate path for this component.
    let candidate = if current == "/" {
        alloc::format!("/{}", component)
    } else {
        alloc::format!("{}/{}", current, component)
    };

    if !ctx.ops.exists(&candidate) {
        return Err(ResolveError::NoEnt);
    }

    if ctx.ops.is_symlink(&candidate) {
        // O_NOFOLLOW on the final component.
        if is_last && (flags & O_NOFOLLOW != 0) {
            return Err(ResolveError::Loop);
        }
        ctx.bump_depth()?;
        let target = ctx.ops.readlink_raw(&candidate)?;
        // Absolute symlink: restart from root.
        if target.starts_with('/') {
            *current = "/".to_string();
            let parts: Vec<String> = target
                .trim_start_matches('/')
                .split('/')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            // Prepend symlink components before the remaining ones.
            let mut new_remaining = parts;
            new_remaining.extend(remaining.drain(..));
            *remaining = new_remaining;
        } else {
            // Relative symlink: resolve relative to current.
            let parts: Vec<String> = target
                .split('/')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            let mut new_remaining = parts;
            new_remaining.extend(remaining.drain(..));
            *remaining = new_remaining;
        }
    } else {
        // Not a symlink: ordinary advance.
        // Verify intermediate components are directories.
        if !is_last && !ctx.ops.is_dir(&candidate) {
            return Err(ResolveError::NotDir);
        }
        *current = candidate;
    }

    Ok(())
}

/// Fully resolve `path` against the filesystem described by `ops`.
///
/// * Symlinks are followed up to `MAX_SYMLINK_DEPTH` times.
/// * `flags` should carry the open(2) flags; only `O_NOFOLLOW` is inspected.
/// * Returns the canonical absolute path of the target, or an error.
pub fn resolve(
    ops: &dyn FsOps,
    path: &str,
    flags: u32,
) -> Result<String, ResolveError> {
    if path.is_empty() {
        return Err(ResolveError::NoEnt);
    }

    let mut ctx = Ctx::new(ops);

    // Split the path into components, preserving the leading '/' anchor.
    let mut current: String = if path.starts_with('/') {
        "/".to_string()
    } else {
        // Relative paths are not supported at VFS layer (should be made
        // absolute by the syscall layer using CWD before calling here).
        return Err(ResolveError::NoEnt);
    };

    let mut components: Vec<String> = path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .map(|s| s.to_string())
        .collect();

    // Resolve ".."
    let components = normalise_dotdot(components);
    let mut remaining: Vec<String> = components;

    while !remaining.is_empty() {
        let component = remaining.remove(0);
        let is_last = remaining.is_empty();
        walk_component(&mut ctx, &mut current, &component, &mut remaining, is_last, flags)?;
    }

    Ok(current)
}

/// Collapse `..` components without touching the filesystem.
fn normalise_dotdot(components: Vec<String>) -> Vec<String> {
    let mut stack: Vec<String> = Vec::new();
    for c in components {
        if c == ".." {
            stack.pop();
        } else {
            stack.push(c);
        }
    }
    stack
}

/// `readlink(2)` — return the raw symlink target without resolving.
///
/// Returns `Err(ResolveError::Inval)` if `path` does not name a symlink.
/// Returns `Err(ResolveError::NoEnt)` if `path` does not exist at all.
pub fn vfs_readlink(
    ops: &dyn FsOps,
    path: &str,
) -> Result<String, ResolveError> {
    if !ops.exists(path) {
        return Err(ResolveError::NoEnt);
    }
    if !ops.is_symlink(path) {
        return Err(ResolveError::Inval);
    }
    // Even dangling symlinks must return the raw target here;
    // readlink_raw returns the stored target regardless of whether the
    // final destination exists.
    ops.readlink_raw(path)
}

// ---- Tests ----------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;

    /// Simple in-memory filesystem for testing.
    struct MemFs {
        // path -> None means regular file/dir, Some(target) means symlink.
        nodes: BTreeMap<String, Option<String>>,
        dirs: BTreeMap<String, bool>,
    }

    impl MemFs {
        fn new() -> Self {
            let mut dirs = BTreeMap::new();
            dirs.insert("/".to_string(), true);
            MemFs {
                nodes: BTreeMap::new(),
                dirs,
            }
        }
        fn add_file(&mut self, path: &str) {
            self.nodes.insert(path.to_string(), None);
        }
        fn add_dir(&mut self, path: &str) {
            self.nodes.insert(path.to_string(), None);
            self.dirs.insert(path.to_string(), true);
        }
        fn add_symlink(&mut self, path: &str, target: &str) {
            self.nodes.insert(path.to_string(), Some(target.to_string()));
        }
    }

    impl FsOps for MemFs {
        fn exists(&self, path: &str) -> bool {
            path == "/" || self.nodes.contains_key(path)
        }
        fn is_dir(&self, path: &str) -> bool {
            self.dirs.contains_key(path)
        }
        fn is_symlink(&self, path: &str) -> bool {
            self.nodes.get(path).and_then(|v| v.as_ref()).is_some()
        }
        fn readlink_raw(&self, path: &str) -> Result<String, ResolveError> {
            match self.nodes.get(path) {
                Some(Some(t)) => Ok(t.clone()),
                Some(None) => Err(ResolveError::Inval),
                None => Err(ResolveError::NoEnt),
            }
        }
    }

    #[test]
    fn resolve_plain_file() {
        let mut fs = MemFs::new();
        fs.add_dir("/etc");
        fs.add_file("/etc/hosts");
        let r = resolve(&fs, "/etc/hosts", 0);
        assert_eq!(r, Ok("/etc/hosts".to_string()));
    }

    #[test]
    fn resolve_enoent() {
        let fs = MemFs::new();
        assert_eq!(resolve(&fs, "/nonexistent", 0), Err(ResolveError::NoEnt));
    }

    #[test]
    fn resolve_symlink_basic() {
        let mut fs = MemFs::new();
        fs.add_dir("/etc");
        fs.add_file("/etc/hosts");
        fs.add_symlink("/etc/hosts.link", "/etc/hosts");
        let r = resolve(&fs, "/etc/hosts.link", 0);
        assert_eq!(r, Ok("/etc/hosts".to_string()));
    }

    #[test]
    fn resolve_o_nofollow_eloop() {
        let mut fs = MemFs::new();
        fs.add_symlink("/a", "/b");
        assert_eq!(
            resolve(&fs, "/a", O_NOFOLLOW),
            Err(ResolveError::Loop)
        );
    }

    #[test]
    fn resolve_eloop_at_depth_40() {
        // Build a 41-deep symlink chain: /s0 -> /s1 -> ... -> /s40
        let mut fs = MemFs::new();
        for i in 0..=40u32 {
            if i < 40 {
                fs.add_symlink(
                    &alloc::format!("/s{}", i),
                    &alloc::format!("/s{}", i + 1),
                );
            } else {
                fs.add_file("/s40");
            }
        }
        // /s0 chain length is 40 hops — exactly the limit, should succeed.
        // Shift the start to make the chain 41 hops (exceeds limit).
        fs.add_symlink("/start", "/s0");
        let r = resolve(&fs, "/start", 0);
        assert_eq!(r, Err(ResolveError::Loop));
    }

    #[test]
    fn readlink_not_symlink_einval() {
        let mut fs = MemFs::new();
        fs.add_file("/regular");
        assert_eq!(vfs_readlink(&fs, "/regular"), Err(ResolveError::Inval));
    }

    #[test]
    fn readlink_returns_raw_target() {
        let mut fs = MemFs::new();
        fs.add_symlink("/link", "/some/target");
        assert_eq!(vfs_readlink(&fs, "/link"), Ok("/some/target".to_string()));
    }

    #[test]
    fn readlink_dangling_symlink_ok() {
        let mut fs = MemFs::new();
        fs.add_symlink("/dangling", "/does/not/exist");
        // readlink of a dangling symlink must succeed (raw target).
        assert_eq!(
            vfs_readlink(&fs, "/dangling"),
            Ok("/does/not/exist".to_string())
        );
    }
}
