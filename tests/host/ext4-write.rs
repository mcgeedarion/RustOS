//! Host regression exercises actual production ext4 code against disposable
//! mke2fs images. Fake device operations copy bytes, not filesystem semantics.
extern crate alloc;
extern crate self as spin;
extern crate self as log;
#[macro_export] macro_rules! info { ($($args:tt)*) => {{}}; }
#[macro_export] macro_rules! warn { ($($args:tt)*) => {{}}; }
#[macro_export] macro_rules! debug { ($($args:tt)*) => {{}}; }
#[macro_export] macro_rules! error { ($($args:tt)*) => {{}}; }
pub struct Mutex<T>(std::sync::Mutex<T>);
impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self { Self(std::sync::Mutex::new(value)) }
    pub fn lock(&self) -> std::sync::MutexGuard<'_, T> { self.0.lock().unwrap() }
}
static DISK: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static FAIL_WRITE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub mod drivers {
    pub mod virtio_blk {
        pub fn is_present() -> bool { true }
        pub fn read_sectors(lba: u64, count: u32, bytes: &mut [u8]) -> bool {
            let disk = crate::DISK.lock();
            let off = lba as usize * 512;
            let len = count as usize * 512;
            if bytes.len() != len { return false; }
            match disk.get(off..off + len) {
                Some(src) => { bytes.copy_from_slice(src); true }
                None => false,
            }
        }
    }
    pub mod block {
        pub fn write_sectors_raw(lba: u64, bytes: &[u8]) -> Result<(), &'static str> {
            if crate::FAIL_WRITE.load(std::sync::atomic::Ordering::Relaxed) { return Err("injected"); }
            let mut disk = crate::DISK.lock();
            let off = lba as usize * 512;
            match disk.get_mut(off..off + bytes.len()) {
                Some(dst) => { dst.copy_from_slice(bytes); Ok(()) }
                None => Err("bounds"),
            }
        }
    }
}
pub mod block { pub mod virtio_blk { pub fn flush_cache() -> bool { true } } }
#[path = "../../src/fs/ext4.rs"]
pub mod ext4;
#[path = "../../src/fs/ext4_write.rs"]
pub mod ext4_write;
#[path = "../../src/fs/jbd2.rs"]
pub mod jbd2;
pub mod fs { pub use crate::{ext4, ext4_write, jbd2}; }

fn be32(bytes: &mut [u8], off: usize, value: u32) { bytes[off..off + 4].copy_from_slice(&value.to_be_bytes()); }
fn journal_fixture() -> Vec<u8> {
    let mut bytes = vec![0; 8 * 1024];
    be32(&mut bytes, 0, 0xc03b3998);
    be32(&mut bytes, 4, 4);
    be32(&mut bytes, 12, 1024);
    be32(&mut bytes, 16, 8);
    be32(&mut bytes, 20, 1);
    be32(&mut bytes, 24, 7);
    be32(&mut bytes, 28, 1);
    be32(&mut bytes, 1024, 0xc03b3998);
    be32(&mut bytes, 1028, 1);
    be32(&mut bytes, 1032, 7);
    be32(&mut bytes, 1036, 1); // destination block
    bytes[1042..1044].copy_from_slice(&10u16.to_be_bytes()); // SAME_UUID|LAST_TAG
    bytes[2048..3072].fill(0x5a);
    be32(&mut bytes, 2048, 0xc03b3998); // payload looks like a header
    be32(&mut bytes, 3072, 0xc03b3998);
    be32(&mut bytes, 3076, 2);
    be32(&mut bytes, 3080, 7);
    bytes
}

#[test]
fn journal_replay_is_bounded_committed_and_revoke_aware() {
    let journal = journal_fixture();
    let mut fs = vec![0; 4096];
    let report = jbd2::replay_journal_image(&mut fs, &journal).unwrap();
    assert_eq!(report.transactions_replayed, 1);
    assert_eq!(&fs[1024..2048], &journal[2048..3072]);
    let mut clean = journal.clone();
    be32(&mut clean, 28, 0);
    fs.fill(0);
    assert_eq!(jbd2::replay_journal_image(&mut fs, &clean).unwrap().blocks_replayed, 0);
    assert!(fs.iter().all(|&b| b == 0));
    let mut incomplete = journal.clone();
    incomplete[3072..4096].fill(0);
    assert_eq!(jbd2::replay_journal_image(&mut fs, &incomplete).unwrap().blocks_replayed, 0);
    let mut revoked = journal.clone();
    be32(&mut revoked, 40, 1); // REVOKE supported
    be32(&mut revoked, 4096, 0xc03b3998);
    be32(&mut revoked, 4100, 5);
    be32(&mut revoked, 4104, 8);
    be32(&mut revoked, 4108, 20);
    be32(&mut revoked, 4112, 1);
    be32(&mut revoked, 5120, 0xc03b3998);
    be32(&mut revoked, 5124, 2);
    be32(&mut revoked, 5128, 8);
    let report = jbd2::replay_journal_image(&mut fs, &revoked).unwrap();
    assert_eq!(report.transactions_replayed, 2);
    assert_eq!(report.blocks_replayed, 0);
    let mut invalid = journal.clone();
    be32(&mut invalid, 1036, 99);
    assert_eq!(jbd2::replay_journal_image(&mut fs, &invalid).unwrap_err(), jbd2::ReplayError::OutOfBounds);
    assert!(fs.iter().all(|&b| b == 0), "bounds failures must not partially replay");
    let mut checksum = journal.clone();
    be32(&mut checksum, 40, 8);
    assert!(matches!(jbd2::replay_journal_image(&mut fs, &checksum), Err(jbd2::ReplayError::UnsupportedFeature(_))));
    let mut escaped = journal.clone();
    escaped[1042..1044].copy_from_slice(&11u16.to_be_bytes());
    escaped[2048..2052].fill(0);
    jbd2::replay_journal_image(&mut fs, &escaped).unwrap();
    assert_eq!(&fs[1024..1028], &0xc03b3998u32.to_be_bytes());
    assert_eq!(jbd2::journal_start_write(), Err(jbd2::JournalError::UnsupportedFeature));
}

#[test]
fn production_write_roundtrip_and_atomic_rejection() {
    let path = std::env::var("EXT4_WRITE_IMAGE").expect("disposable image path");
    *DISK.lock() = std::fs::read(&path).unwrap();
    assert!(ext4::mount());
    assert_eq!(ext4_write::mkdir("/work", 0o755), Ok(0));
    assert_eq!(ext4_write::mkdir("/other", 0o755), Ok(0));
    assert_eq!(ext4_write::create("/work/file", 0o640), Ok(0));
    let payload: Vec<u8> = (0..18000).map(|i| (i % 251) as u8).collect();
    assert_eq!(ext4_write::write("/work/file", &payload, 0), Ok(18000));
    assert_eq!(ext4::read_file_by_ino(ext4::stat("/work/file").unwrap()).unwrap(), payload);
    assert_eq!(ext4_write::write("/work/file", b"tail", 19000), Ok(4));
    let actual = ext4::read_file_by_ino(ext4::stat("/work/file").unwrap()).unwrap();
    assert_eq!(&actual[18000..19000], &[0; 1000]);
    assert_eq!(&actual[19000..], b"tail");
    assert_eq!(ext4_write::link("/work/file", "/work/hard"), Ok(0));
    assert_eq!(ext4::stat("/work/file"), ext4::stat("/work/hard"));
    assert_eq!(ext4_write::symlink("file", "/work/short"), Ok(0));
    assert_eq!(ext4::stat("/work/short"), ext4::stat("/work/file"));
    let long_target = format!("/{}work/file", "./".repeat(50));
    assert_eq!(ext4_write::symlink(&long_target, "/long"), Ok(0));
    assert_eq!(ext4::stat("/long"), ext4::stat("/work/file"));
    assert_eq!(ext4_write::chmod("/work/file", 0o600), Ok(0));
    assert_eq!(ext4_write::chown("/work/file", 70000, 80000), Ok(0));
    assert_eq!(ext4_write::utimens("/work/file", 1234567890123, 9876543210987), Ok(0));
    let before = DISK.lock().clone();
    assert_eq!(ext4_write::create("/work/file", 0o600), Err(-17));
    assert_eq!(ext4_write::rmdir("/work"), Err(-39));
    assert_eq!(ext4_write::rename("/work", "/work/cycle"), Err(-22));
    assert_eq!(*DISK.lock(), before, "rejected operations must not touch disk");
    assert_eq!(ext4_write::truncate("/work/hard", 2049), Ok(0));
    assert_eq!(ext4_write::truncate("/work/hard", 6000), Ok(0));
    let data = ext4::read_file_by_ino(ext4::stat("/work/file").unwrap()).unwrap();
    assert_eq!(&data[..2049], &payload[..2049]);
    assert!(data[2049..].iter().all(|&b| b == 0));
    assert_eq!(ext4_write::unlink("/work/file"), Ok(0));
    assert!(ext4::stat("/work/hard").is_some());
    assert_eq!(ext4_write::unlink("/work/short"), Ok(0));
    assert_eq!(ext4_write::unlink("/long"), Ok(0));
    assert_eq!(ext4_write::rename("/work", "/other/moved"), Ok(0));
    assert!(ext4::stat("/work").is_none());
    assert_eq!(ext4_write::create("/replace", 0o644), Ok(0));
    assert_eq!(ext4_write::write("/replace", b"obsolete", 0), Ok(8));
    assert_eq!(ext4_write::rename("/other/moved/hard", "/replace"), Ok(0));
    assert_eq!(ext4_write::rmdir("/other/moved"), Ok(0));
    assert_eq!(ext4_write::rmdir("/other"), Ok(0));
    assert_eq!(ext4_write::create("/final", 0o644), Ok(0));
    assert_eq!(ext4_write::write("/final", b"ext4 write regression\n", 0), Ok(22));
    assert_eq!(ext4_write::fsync("/final"), Ok(0));
    assert_eq!(ext4_write::unlink("/replace"), Ok(0));
    // Directory growth beyond one block, then reclamation.
    assert_eq!(ext4_write::mkdir("/many", 0o755), Ok(0));
    for i in 0..100 { assert_eq!(ext4_write::create(&format!("/many/file-{i:04}"), 0o644), Ok(0)); }
    for i in 0..100 { assert_eq!(ext4_write::unlink(&format!("/many/file-{i:04}")), Ok(0)); }
    assert_eq!(ext4_write::rmdir("/many"), Ok(0));
    let before = DISK.lock().clone();
    assert_eq!(ext4_write::truncate("/final", before.len() as u64), Err(-28));
    assert_eq!(*DISK.lock(), before, "ENOSPC must roll back every bitmap and inode edit");
    assert_eq!(ext4::read_file_by_ino(ext4::stat("/final").unwrap()).unwrap(), b"ext4 write regression\n");
    assert!(ext4::mount(), "fresh remount reads persisted edits");
    assert_eq!(ext4::read_file_by_ino(ext4::stat("/final").unwrap()).unwrap(), b"ext4 write regression\n");
    std::fs::write(&path, &*DISK.lock()).unwrap();
    let before = DISK.lock().clone();
    FAIL_WRITE.store(true, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(ext4_write::create("/io-fail", 0o644), Err(-5));
    FAIL_WRITE.store(false, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(ext4_write::create("/disabled", 0o644), Err(-30));
    assert_eq!(*DISK.lock(), before);
    assert!(ext4::stat("/io-fail").is_none());
    // Feature rejection does not modify images.
    DISK.lock()[1024 + 0x64 + 1] |= 4; // metadata_csum (0x400)
    assert!(ext4::mount());
    let before = DISK.lock().clone();
    assert_eq!(ext4_write::create("/checksum", 0o644), Err(-95));
    assert_eq!(*DISK.lock(), before);
}
