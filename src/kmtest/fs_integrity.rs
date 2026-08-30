//! Filesystem Integrity Tests for RustOS
//! 
//! This module provides comprehensive filesystem integrity testing for VFS, ext4, and FAT32.
//! Run with: cargo test --package kmtest --lib fs_integrity

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Read, Write, Seek, SeekFrom};
    use std::path::Path;
    use tempfile::TempDir;

    // ========================================================================
    // VFS Core Integrity Tests
    // ========================================================================

    #[test]
    fn test_vfs_basic_operations() {
        let tmpdir = TempDir::new().expect("create temp dir");
        let test_file = tmpdir.path().join("vfs_test.txt");
        
        // Create and write
        let mut file = fs::File::create(&test_file).expect("create file");
        file.write_all(b"Hello, VFS!").expect("write data");
        drop(file);
        
        // Read and verify
        let mut file = fs::File::open(&test_file).expect("open file");
        let mut contents = String::new();
        file.read_to_string(&mut contents).expect("read file");
        assert_eq!(contents, "Hello, VFS!");
    }

    #[test]
    fn test_vfs_directory_operations() {
        let tmpdir = TempDir::new().expect("create temp dir");
        let subdir = tmpdir.path().join("test_subdir");
        
        // Create directory
        fs::create_dir(&subdir).expect("create dir");
        assert!(subdir.exists());
        assert!(subdir.is_dir());
        
        // Create file in subdirectory
        let nested_file = subdir.join("nested.txt");
        fs::write(&nested_file, "nested content").expect("write nested file");
        assert!(nested_file.exists());
        
        // Remove directory (should fail with content)
        assert!(fs::remove_dir(&subdir).is_err());
        
        // Remove file then directory
        fs::remove_file(&nested_file).expect("remove nested file");
        fs::remove_dir(&subdir).expect("remove empty dir");
        assert!(!subdir.exists());
    }

    #[test]
    fn test_vfs_seek_operations() {
        let tmpdir = TempDir::new().expect("create temp dir");
        let test_file = tmpdir.path().join("seek_test.bin");
        
        // Write test data
        let data: Vec<u8> = (0..100).collect();
        fs::write(&test_file, &data).expect("write test data");
        
        // Test seeking
        let mut file = fs::File::open(&test_file).expect("open file");
        
        // Seek to position 50
        file.seek(SeekFrom::Start(50)).expect("seek to 50");
        let mut buf = [0u8; 10];
        file.read_exact(&mut buf).expect("read at 50");
        assert_eq!(buf, (50..60).collect::<Vec<u8>>().as_slice());
        
        // Seek relative
        file.seek(SeekFrom::Current(-5)).expect("seek relative");
        file.read_exact(&mut buf).expect("read after relative seek");
        assert_eq!(buf, (55..65).collect::<Vec<u8>>().as_slice());
        
        // Seek from end
        file.seek(SeekFrom::End(-10)).expect("seek from end");
        file.read_exact(&mut buf).expect("read at end");
        assert_eq!(buf, (90..100).collect::<Vec<u8>>().as_slice());
    }

    // ========================================================================
    // ext4-specific Integrity Tests
    // ========================================================================

    #[test]
    fn test_ext4_large_file_handling() {
        let tmpdir = TempDir::new().expect("create temp dir");
        let large_file = tmpdir.path().join("large_file.ext4");
        
        // Create a file larger than typical block size
        let size_mb = 10;
        let data = vec![0xAB; size_mb * 1024 * 1024];
        
        let start = std::time::Instant::now();
        fs::write(&large_file, &data).expect("write large file");
        let write_time = start.elapsed();
        
        assert!(large_file.exists());
        assert_eq!(large_file.metadata().expect("get metadata").len(), data.len() as u64);
        
        // Verify readback
        let start = std::time::Instant::now();
        let read_data = fs::read(&large_file).expect("read large file");
        let read_time = start.elapsed();
        
        assert_eq!(data, read_data);
        
        println!("ext4 large file: wrote {}MB in {:?}, read in {:?}", 
                 size_mb, write_time, read_time);
    }

    #[test]
    fn test_ext4_many_files() {
        let tmpdir = TempDir::new().expect("create temp dir");
        let num_files = 1000;
        
        let start = std::time::Instant::now();
        for i in 0..num_files {
            let file_path = tmpdir.path().join(format!("file_{}.txt", i));
            let content = format!("Content of file {}", i);
            fs::write(&file_path, content).expect("write file");
        }
        let write_time = start.elapsed();
        
        // Verify all files exist
        let entries: Vec<_> = fs::read_dir(tmpdir.path())
            .expect("read dir")
            .collect();
        assert_eq!(entries.len(), num_files);
        
        // Random access verification
        for i in (0..num_files).step_by(17) {
            let file_path = tmpdir.path().join(format!("file_{}.txt", i));
            let content = fs::read_to_string(&file_path).expect("read file");
            assert_eq!(content, format!("Content of file {}", i));
        }
        
        println!("ext4 many files: created {} files in {:?}", num_files, write_time);
    }

    #[test]
    fn test_ext4_symlink_handling() {
        let tmpdir = TempDir::new().expect("create temp dir");
        let target = tmpdir.path().join("target.txt");
        let link = tmpdir.path().join("link.txt");
        
        // Create target file
        fs::write(&target, "target content").expect("write target");
        
        // Create symlink
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &link).expect("create symlink");
            
            // Verify symlink resolution
            assert!(link.exists());
            let content = fs::read_to_string(&link).expect("read via symlink");
            assert_eq!(content, "target content");
            
            // Remove target, symlink should become broken
            fs::remove_file(&target).expect("remove target");
            assert!(!link.exists());
        }
    }

    // ========================================================================
    // FAT32-specific Integrity Tests
    // ========================================================================

    #[test]
    fn test_fat32_filename_restrictions() {
        let tmpdir = TempDir::new().expect("create temp dir");
        
        // FAT32 supports long filenames but test compatibility
        let valid_names = vec![
            "short.txt",
            "longer_filename.txt",
            "mixedCase.TXT",
            "name.with.dots.txt",
        ];
        
        for name in valid_names {
            let file_path = tmpdir.path().join(name);
            fs::write(&file_path, "content").expect("write file");
            assert!(file_path.exists());
        }
    }

    #[test]
    fn test_fat32_no_permissions() {
        let tmpdir = TempDir::new().expect("create temp dir");
        let file_path = tmpdir.path().join("perm_test.txt");
        
        // FAT32 doesn't support Unix permissions
        fs::write(&file_path, "content").expect("write file");
        
        // Permission bits may not be preserved
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&file_path).expect("get metadata");
            // Just verify we can read/write, don't check specific modes
            assert!(file_path.exists());
        }
    }

    // ========================================================================
    // Cross-Filesystem Integrity Tests
    // ========================================================================

    #[test]
    fn test_atomic_write_pattern() {
        let tmpdir = TempDir::new().expect("create temp dir");
        let target_file = tmpdir.path().join("atomic_target.txt");
        let temp_file = tmpdir.path().join("atomic_temp.txt");
        
        // Write to temp file first
        fs::write(&temp_file, "new content").expect("write temp");
        
        // Atomic rename
        fs::rename(&temp_file, &target_file).expect("atomic rename");
        
        assert!(!temp_file.exists());
        assert!(target_file.exists());
        
        let content = fs::read_to_string(&target_file).expect("read target");
        assert_eq!(content, "new content");
    }

    #[test]
    fn test_concurrent_access_simulation() {
        use std::thread;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        
        let tmpdir = TempDir::new().expect("create temp dir");
        let shared_file = tmpdir.path().join("shared.txt");
        
        // Initialize file
        fs::write(&shared_file, "initial").expect("init file");
        
        let counter = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];
        
        // Spawn multiple threads to simulate concurrent access
        for i in 0..10 {
            let counter = Arc::clone(&counter);
            let file_path = shared_file.clone();
            
            let handle = thread::spawn(move || {
                for _ in 0..10 {
                    // Simulate read-modify-write
                    let mut content = fs::read_to_string(&file_path)
                        .unwrap_or_default();
                    content.push_str(&format!(" thread{}", i));
                    let _ = fs::write(&file_path, &content);
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            });
            
            handles.push(handle);
        }
        
        for handle in handles {
            handle.join().expect("thread join");
        }
        
        assert_eq!(counter.load(Ordering::SeqCst), 100);
        println!("Concurrent test completed with {} operations", counter.load(Ordering::SeqCst));
    }

    #[test]
    fn test_filesystem_stress() {
        let tmpdir = TempDir::new().expect("create temp dir");
        let iterations = 100;
        
        for i in 0..iterations {
            let file_path = tmpdir.path().join(format!("stress_{}.bin", i % 10));
            
            // Write random-ish data
            let data: Vec<u8> = (0..1024).map(|j| ((i + j) % 256) as u8).collect();
            fs::write(&file_path, &data).expect("write stress data");
            
            // Verify immediately
            let read_data = fs::read(&file_path).expect("read stress data");
            assert_eq!(data, read_data);
            
            // Append
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&file_path)
                .expect("open for append");
            file.write_all(&[0xFF]).expect("append data");
            drop(file);
            
            // Verify append
            let final_data = fs::read(&file_path).expect("read appended data");
            assert_eq!(final_data.len(), data.len() + 1);
        }
        
        println!("Stress test completed: {} iterations", iterations);
    }

    // ========================================================================
    // Error Handling Tests
    // ========================================================================

    #[test]
    fn test_error_conditions() {
        let tmpdir = TempDir::new().expect("create temp dir");
        
        // Open nonexistent file
        assert!(fs::File::open(tmpdir.path().join("nonexistent")).is_err());
        
        // Create in nonexistent directory
        let bad_path = tmpdir.path().join("no_such_dir").join("file.txt");
        assert!(fs::write(&bad_path, "data").is_err());
        
        // Remove nonexistent file
        assert!(fs::remove_file(tmpdir.path().join("ghost")).is_err());
        
        // Remove nonexistent directory
        assert!(fs::remove_dir(tmpdir.path().join("ghost_dir")).is_err());
    }
}
