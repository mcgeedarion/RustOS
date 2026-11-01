//! A host adapter around the actual production modules, not a rewritten driver.
#![allow(dead_code, unused_imports, unused_mut)]
extern crate alloc;
#[path = "../mod.rs"]
mod btrfs;
mod fs {
    pub mod vfs_ops {
        #[derive(Debug, Default)]
        pub struct KStat {
            pub ino: u64, pub mode: u16, pub nlink: u32, pub uid: u32, pub gid: u32,
            pub size: u64, pub atime: u64, pub mtime: u64, pub ctime: u64,
            pub blksize: u64, pub blocks: u64, pub is_dir: bool,
        }
        pub struct KStatfs {
            pub f_type: u64, pub f_bsize: u64, pub f_blocks: u64,
            pub f_bfree: u64, pub f_bavail: u64, pub f_namelen: u64,
        }
        #[derive(Debug)]
        pub struct DirEntry {
            pub name: String, pub ino: u64, pub is_dir: bool, pub mode: u16, pub size: u64,
        }
    }
}
mod drivers {
    pub mod block {
        use std::{fs::{File, OpenOptions}, io::{Read, Seek, SeekFrom, Write}, sync::Mutex};
        static DEVICE: Mutex<Option<File>> = Mutex::new(None);
        static FAIL_WRITE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        pub fn open(path: &std::path::Path) {
            *DEVICE.lock().unwrap() = Some(OpenOptions::new().read(true).write(true).open(path).unwrap());
        }
        pub fn fail_next_write() { FAIL_WRITE.store(true, std::sync::atomic::Ordering::SeqCst); }
        pub fn read_sectors(lba: u64, count: u32, buf: &mut [u8]) -> bool {
            let mut device = DEVICE.lock().unwrap();
            let Some(file) = device.as_mut() else { return false; };
            let Some(offset) = lba.checked_mul(512) else { return false; };
            let n = count as usize * 512;
            if buf.len() < n { return false; }
            file.seek(SeekFrom::Start(offset)).is_ok() && file.read_exact(&mut buf[..n]).is_ok()
        }
        pub fn read_sectors_vec(lba: u64, count: u32) -> Option<Vec<u8>> {
            let mut v = vec![0; count as usize * 512];
            if read_sectors(lba, count, &mut v) { Some(v) } else { None }
        }
        pub fn write_sectors(lba: u64, count: u32, buf: &[u8]) -> bool {
            if FAIL_WRITE.swap(false, std::sync::atomic::Ordering::SeqCst) { return false; }
            let mut device = DEVICE.lock().unwrap();
            let Some(file) = device.as_mut() else { return false; };
            let Some(offset) = lba.checked_mul(512) else { return false; };
            let n = count as usize * 512;
            if buf.len() < n { return false; }
            file.seek(SeekFrom::Start(offset)).is_ok() && file.write_all(&buf[..n]).is_ok()
                && file.sync_data().is_ok()
        }
        pub fn write_sectors_raw(lba: u64, buf: &[u8]) -> Result<(), &'static str> {
            if buf.len() % 512 != 0 { return Err("alignment"); }
            if write_sectors(lba, (buf.len() / 512) as u32, buf) { Ok(()) } else { Err("io") }
        }
    }
}
fn main() {}
