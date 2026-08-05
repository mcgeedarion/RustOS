//! Small path utilities shared by filesystem call sites.

extern crate alloc;

use alloc::string::String;

/// Return `path` with duplicate separators removed and a leading slash.
pub fn normalize(path: &str) -> String {
    let mut out = String::new();
    out.push('/');
    let mut first = true;
    for part in path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
    {
        if !first {
            out.push('/');
        }
        out.push_str(part);
        first = false;
    }
    out
}

/// Return true if `path` denotes the filesystem root.
pub fn is_root(path: &str) -> bool {
    path == "/" || path.is_empty()
}

/// Resolve a path through the VFS compatibility resolver.
///
/// Older syscall stubs import this helper from `crate::fs::path`; the active
/// symlink-aware implementation lives in `crate::fs::vfs::resolve_path`.
pub fn resolve_path(path: &str, flags: u32) -> Result<String, isize> {
    crate::fs::vfs::resolve_path(path, flags)
}
