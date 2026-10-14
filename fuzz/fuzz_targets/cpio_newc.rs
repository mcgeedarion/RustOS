#![no_main]

use libfuzzer_sys::fuzz_target;

#[macro_export]
macro_rules! kprintln {
    ($($arg:tt)*) => {{}};
}

#[path = "../../src/init/initramfs/mod.rs"]
mod production_cpio;

fuzz_target!(|data: &[u8]| {
    let entries: Vec<_> = production_cpio::iter(data).collect();
    for entry in &entries {
        assert_eq!(entry.size, entry.data.len());
        let _ = production_cpio::find_file(data, entry.name);
        let _ = entry.permissions();
        let _ = entry.is_file();
        let _ = entry.is_dir();
        let _ = entry.is_symlink();
    }
    let _ = production_cpio::find_file(data, "/init");
    let _ = production_cpio::find_file(data, "/smoke");
});
