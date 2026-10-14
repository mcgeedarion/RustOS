#![no_main]

extern crate alloc;

use std::string::String;
use libfuzzer_sys::fuzz_target;

#[path = "../../src/fs/vfs/symlink.rs"]
mod production_symlink;

struct FuzzFs<'a> {
    input: &'a [u8],
}

impl production_symlink::FsOps for FuzzFs<'_> {
    fn exists(&self, path: &str) -> bool {
        !path.split('/').any(|component| component == "missing")
    }

    fn is_dir(&self, path: &str) -> bool {
        !matches!(path.rsplit('/').next(), Some("file" | "not-dir"))
    }

    fn is_symlink(&self, path: &str) -> bool {
        matches!(path.rsplit('/').next(), Some("link" | "sym"))
    }

    fn readlink_raw(&self, _path: &str) -> Result<String, production_symlink::ResolveError> {
        let selector = self.input.first().copied().unwrap_or(0) & 3;
        let target = match selector {
            0 => "/target",
            1 => "../target",
            2 => "link",
            _ => "./target/../leaf",
        };
        Ok(target.to_string())
    }
}

fuzz_target!(|data: &[u8]| {
    let raw = String::from_utf8_lossy(data);
    let path = format!("/root/{raw}");
    let fs = FuzzFs { input: data };
    let _ = production_symlink::resolve(&fs, &path, 0);
    let _ = production_symlink::resolve(&fs, &path, 0x0002_0000);
});
