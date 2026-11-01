//! Disposable host-image regressions exercising the production driver.
extern crate std;
use super::*;
use std::{format, fs, process::Command, path::{Path, PathBuf}, vec::Vec, string::String};

fn command(program: &str, args: &[&str]) {
    let out = Command::new(program).args(args).output().unwrap();
    assert!(out.status.success(), "{} {:?}\n{}\n{}", program, args,
        String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}
fn image(base: &Path, name: &str, profile: &str, many: bool) -> PathBuf {
    let root = base.join(format!("root-{name}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("seed"), b"seed data").unwrap();
    let large: Vec<u8> = (0..32768).map(|i| (i % 251) as u8).collect();
    fs::write(root.join("regular"), &large).unwrap();
    if many {
        for i in 0..250 { fs::write(root.join(format!("entry-{i:04}")), b"x").unwrap(); }
    }
    let path = base.join(format!("{name}.img"));
    fs::File::create(&path).unwrap().set_len(128 * 1024 * 1024).unwrap();
    command("mkfs.btrfs", &["-q", "-f", "-m", profile, "-d", "single", "-r",
        root.to_str().unwrap(), path.to_str().unwrap()]);
    path
}
fn check(path: &Path) {
    command("btrfs", &["check", "--readonly", path.to_str().unwrap()]);
}

#[test]
fn production_image_operations_and_error_paths() {
    let base = std::env::temp_dir().join(format!("rustos-btrfs-regression-{}", std::process::id()));
    fs::create_dir_all(&base).unwrap();
    for profile in ["single", "dup"] {
        let image = image(&base, profile, profile, false);
        check(&image);
        crate::drivers::block::open(&image);
        let mut fs = mount::open(0).expect("mount real image");
        assert!(fs.writable, "simple exclusive leaf must be writable");
        assert_eq!(fs.read_all("/seed").unwrap(), b"seed data");
        let expected: Vec<u8> = (0..32768).map(|i| (i % 251) as u8).collect();
        assert_eq!(fs.read_all("/regular").unwrap(), expected);
        assert_eq!(fs.stat("/seed").unwrap().size, 9);
        assert_eq!(fs.create("/seed"), Err(-17));
        fs.mkdir("/dir").unwrap();
        fs.create("/dir/file").unwrap();
        fs.write_all("/dir/file", b"hello filesystem").unwrap();
        check(&image);
        fs.link("/dir/file", "/hard").unwrap();
        assert_eq!(fs.stat("/hard").unwrap().nlink, 2);
        fs.symlink("/dir/file", "/symbolic").unwrap();
        check(&image);
        assert_eq!(fs.readlink("/symbolic").unwrap(), "/dir/file");
        assert_eq!(fs.readlink("/hard"), Err(-22));
        fs.rename("/dir/file", "/dir/renamed").unwrap();
        check(&image);
        assert_eq!(fs.read_all("/hard").unwrap(), b"hello filesystem");
        assert_eq!(fs.stat("/dir/file").unwrap_err(), -2);
        fs.truncate("/hard", 5).unwrap();
        fs.truncate("/hard", 9).unwrap();
        assert_eq!(fs.read_all("/dir/renamed").unwrap(), b"hello\0\0\0\0");
        fs.chmod("/hard", 0o600).unwrap();
        fs.chown("/hard", 123, 456).unwrap();
        fs.set_times("/hard", 1_234_567_890, 9_876_543_210).unwrap();
        check(&image);
        let st = fs.stat("/hard").unwrap();
        assert_eq!((st.mode, st.uid, st.gid, st.atime, st.mtime), (0o100600,123,456,1_234_567_890,9_876_543_210));
        assert_eq!(fs.rmdir("/dir"), Err(-39));
        assert_eq!(fs.rename("/dir", "/dir/subdir"), Err(-22));
        assert_eq!(fs.unlink("/dir"), Err(-21));
        assert_eq!(fs.create("/hard/invalid"), Err(-20));
        assert_eq!(fs.write_all("/hard", &std::vec![1; 4096]), Err(-27));
        assert_eq!(fs.write_all("/regular", b"would leak extents"), Err(-95));
        assert_eq!(fs.unlink("/regular"), Err(-95));
        assert_eq!(fs.read_all("/regular").unwrap(), expected);

        // Transaction drop/closure error restores all changes without disk I/O.
        let original = fs.read_node_bytes(fs.fs_tree_root).unwrap();
        let result: Result<(), isize> = transaction::transaction(&mut fs, |tx| {
            tx.create("/rolled-back")?;
            tx.write_all("/hard", b"discard me")?;
            Err(-22)
        });
        assert_eq!(result, Err(-22));
        assert_eq!(fs.read_node_bytes(fs.fs_tree_root).unwrap(), original);
        assert_eq!(fs.stat("/rolled-back").unwrap_err(), -2);
        transaction::transaction(&mut fs, |tx| {
            tx.create("/committed")?;
            tx.write_all("/committed", b"transaction data")
        }).unwrap();
        check(&image);
        fs.create("/replacement").unwrap();
        fs.write_all("/replacement", b"old destination").unwrap();
        fs.rename("/committed", "/replacement").unwrap();
        check(&image);
        assert_eq!(fs.read_all("/replacement").unwrap(), b"transaction data");

        // Remount through public absolute-path API and exercise cross-mount refusal.
        superblock::BTRFS_MOUNTS.lock().clear();
        mount::mount("/mnt", 0).unwrap();
        mount::mount("/other", 0).unwrap();
        assert_eq!(api::btrfs_read_all("/mnt/replacement").unwrap(), b"transaction data");
        assert_eq!(api::btrfs_rename("/mnt/replacement", "/other/replacement"), Err(-18));
        assert_eq!(api::btrfs_read_all("/mnt-no-prefix/replacement"), Err(-19));
        superblock::BTRFS_MOUNTS.lock().clear();
        drop(fs);
        let mut fs = mount::open(0).unwrap();
        assert_eq!(fs.read_all("/hard").unwrap(), b"hello\0\0\0\0");
        assert_eq!(fs.stat("/hard").unwrap().mtime, 9_876_543_210);
        fs.unlink("/hard").unwrap();
        assert_eq!(fs.stat("/dir/renamed").unwrap().nlink, 1);
        fs.unlink("/dir/renamed").unwrap();
        fs.rmdir("/dir").unwrap();
        fs.unlink("/symbolic").unwrap();
        check(&image);

        // Capacity failure is rollback-safe and reports ENOSPC.
        let mut made = 0;
        loop {
            let before = fs.read_node_bytes(fs.fs_tree_root).unwrap();
            match fs.create(&format!("/capacity-{made:04}")) {
                Ok(()) => { made += 1; assert!(made < 200); },
                Err(-28) => { assert_eq!(fs.read_node_bytes(fs.fs_tree_root).unwrap(), before); break; },
                other => panic!("unexpected capacity result {other:?}"),
            }
        }
        assert!(made > 0);
        check(&image);
        for i in 0..made { fs.unlink(&format!("/capacity-{i:04}")).unwrap(); }
        let old = fs.read_node_bytes(fs.fs_tree_root).unwrap();
        crate::drivers::block::fail_next_write();
        assert_eq!(fs.create("/io-failure"), Err(-5));
        assert!(!fs.writable);
        assert_eq!(fs.read_node_bytes(fs.fs_tree_root).unwrap(), old);
        check(&image);
    }

    // Multi-level traversal remains usable even though splits/CoW are not writable.
    let large = image(&base, "multilevel", "single", true);
    crate::drivers::block::open(&large);
    let mut fs = mount::open(0).unwrap();
    assert!(!fs.writable);
    assert_eq!(fs.read_all("/entry-0249").unwrap(), b"x");
    assert_eq!(fs.readdir("/").unwrap().len(), 252);
    assert_eq!(fs.create("/no-split"), Err(-95));
    check(&large);
    fs::remove_dir_all(base).unwrap();
}
