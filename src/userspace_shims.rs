//! Full userspace services replacing the `userspace_boot` shims.
//!
//! This module now provides complete initramfs mounting and process spawning
//! functionality, integrating with the full VFS and process scheduler graphs.
//! The shim implementations have been replaced with real kernel subsystems.

// ============================================================================
// Filesystem Integration - Real initramfs VFS mounting
// ============================================================================

pub mod fs {
    use crate::init::initramfs::{self, InitramfsHandle};

    /// Mount the initramfs into the VFS namespace.
    ///
    /// This function parses the CPIO archive and populates the VFS tree
    /// so that all files in the initramfs are accessible through normal
    /// filesystem operations. This replaces the shim that only registered
    /// the memory range without actually mounting the filesystem.
    pub fn mount_initramfs() {
        crate::serial_println!("initramfs: mounting full VFS tree");

        // Load the raw CPIO archive
        let ram = match initramfs::load() {
            Some(handle) => handle,
            None => {
                crate::serial_println!("initramfs: no initramfs range available");
                return;
            }
        };

        // Parse and mount all entries into the VFS
        let mount_result = mount_cpio_to_vfs(&ram);
        
        match mount_result {
            Ok(count) => {
                crate::serial_println!("initramfs: mounted {} entries into VFS", count);
            }
            Err(e) => {
                crate::serial_println!("initramfs: mount error: {}", e);
            }
        }
    }

    /// Parse CPIO archive and create VFS nodes for each entry.
    ///
    /// Returns the number of successfully mounted entries.
    fn mount_cpio_to_vfs(ram: &InitramfsHandle) -> Result<usize, &'static str> {
        let mut count = 0;
        
        // Iterate through all CPIO entries using the provided iterator
        for entry in ram.iter() {
            match entry {
                Ok(cpio_entry) => {
                    let name = cpio_entry.name();
                    let mode = cpio_entry.mode();
                    
                    // Skip empty names or TRAILER!!!
                    if name.is_empty() || name.contains("TRAILER") {
                        continue;
                    }

                    // Determine file type from mode and create appropriate VFS node
                    let result = if mode & 0o170000 == 0o040000 {
                        // Directory
                        crate::fs::vfs_ops::create_directory(name, mode & 0o777)
                    } else if mode & 0o170000 == 0o100000 {
                        // Regular file - extract contents
                        if let Some(data) = cpio_entry.data() {
                            crate::fs::vfs_ops::create_file_with_data(name, mode & 0o777, data)
                        } else {
                            continue;
                        }
                    } else if mode & 0o170000 == 0o120000 {
                        // Symlink - extract target
                        if let Some(data) = cpio_entry.data() {
                            if let Ok(target) = core::str::from_utf8(data) {
                                crate::fs::vfs_ops::create_symlink(name, target)
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    } else {
                        // Unsupported file type (device nodes, sockets, etc.)
                        continue;
                    };

                    // Count successful insertions
                    if result.is_ok() {
                        count += 1;
                    }
                }
                Err(_) => {
                    // Skip malformed entries
                    continue;
                }
            }
        }

        Ok(count)
    }

    /// Check if initramfs range is registered (legacy shim compatibility)
    pub fn has_initramfs_range() -> bool {
        initramfs::load().is_some()
    }
}

// ============================================================================
// Process Management - Real process spawning with full scheduler integration
// ============================================================================

pub mod proc {
    pub mod exec {
        use alloc::string::String;
        use crate::proc::scheduler;

        const ELF_MAGIC: &[u8; 4] = b"\x7FELF";
        const ELFCLASS64: u8 = 2;
        const ELFDATA2LSB: u8 = 1;
        const EV_CURRENT: u8 = 1;
        const ET_EXEC: u16 = 2;
        const ET_DYN: u16 = 3;
        const EM_X86_64: u16 = 62;
        const EM_AARCH64: u16 = 183;
        const PT_LOAD: u32 = 1;
        const PT_INTERP: u32 = 3;

        /// Spawn a user process from ELF bytes with full scheduler integration.
        ///
        /// This function:
        /// 1. Validates the ELF64 binary
        /// 2. Allocates a new PID and Process structure
        /// 3. Maps PT_LOAD segments into the process address space
        /// 4. Sets up the initial stack with argv/envp/auxv
        /// 5. Enqueues the process on the scheduler run-queue
        ///
        /// Returns true on success, false on any error (with diagnostic output).
        pub fn spawn_user_process_from_bytes(
            path: &str,
            elf: &[u8],
            argv: &[&str],
            envp: &[&str],
        ) -> bool {
            crate::serial_println!("exec: spawning {} with {} args, {} env vars", 
                                   path, argv.len(), envp.len());

            // Step 1: Validate ELF and extract metadata
            let elf_info = match validate_elf64_comprehensive(elf) {
                Ok(info) => info,
                Err(reason) => {
                    crate::serial_println!("exec: rejected {}: {}", path, reason.as_str());
                    return false;
                }
            };

            // Step 2: Check for dynamic linker (not supported for init)
            if elf_info.has_interp {
                crate::serial_println!("exec: rejected {}: dynamically linked binaries not supported for init", path);
                return false;
            }

            // Step 3: Allocate PID
            let pid = match scheduler::alloc_pid() {
                Some(p) => p,
                None => {
                    crate::serial_println!("exec: failed to allocate PID for {}", path);
                    return false;
                }
            };

            // Step 4-7: Delegate to full kernel exec implementation
            // This integrates with the complete process/mm subsystems
            let spawned = crate::proc::exec::spawn_user_process_from_bytes_full(
                pid,
                path,
                elf,
                argv,
                envp,
                elf_info.entry,
            );

            if !spawned {
                scheduler::free_pid(pid);
                return false;
            }

            crate::serial_println!("exec: successfully spawned {} as PID {}", path, pid);
            true
        }

        /// Comprehensive ELF64 validation with detailed metadata extraction.
        struct ElfInfo {
            entry: u64,
            phnum: u16,
            has_interp: bool,
            load_segments: usize,
            min_load_vaddr: u64,
            max_load_vaddr: u64,
        }

        fn validate_elf64_comprehensive(data: &[u8]) -> Result<ElfInfo, String> {
            // Basic size check
            if data.len() < 64 {
                return Err(String::from("ELF image is smaller than ELF64 header"));
            }

            // Magic number check
            if &data[0..4] != ELF_MAGIC {
                return Err(String::from("bad ELF magic"));
            }

            // Class check (must be 64-bit)
            if data[4] != ELFCLASS64 {
                return Err(String::from("not an ELF64 image"));
            }

            // Data encoding check (must be little-endian)
            if data[5] != ELFDATA2LSB {
                return Err(String::from("not little-endian ELF"));
            }

            // Version check
            if data[6] != EV_CURRENT {
                return Err(String::from("unsupported ELF version"));
            }

            // Type check (must be executable or shared object)
            let e_type = read_u16(data, 16)?;
            if e_type != ET_EXEC && e_type != ET_DYN {
                return Err(String::from("ELF is not ET_EXEC or ET_DYN"));
            }

            // Machine architecture check
            let machine = read_u16(data, 18)?;
            #[cfg(target_arch = "x86_64")]
            if machine != EM_X86_64 {
                return Err(String::from("ELF machine does not match x86_64"));
            }
            #[cfg(target_arch = "aarch64")]
            if machine != EM_AARCH64 {
                return Err(String::from("ELF machine does not match aarch64"));
            }
            #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
            let _ = machine;

            // Entry point and program headers
            let entry = read_u64(data, 24)?;
            let phoff = read_u64(data, 32)? as usize;
            let phentsize = read_u16(data, 54)? as usize;
            let phnum = read_u16(data, 56)?;

            if phentsize < 56 || phnum == 0 {
                return Err(String::from("invalid ELF program-header table"));
            }

            // Validate program header table bounds
            let phnum_usize = phnum as usize;
            let phdr_bytes = phentsize
                .checked_mul(phnum_usize)
                .ok_or_else(|| String::from("ELF program-header table overflows"))?;
            let phdr_end = phoff
                .checked_add(phdr_bytes)
                .ok_or_else(|| String::from("ELF program-header table overflows"))?;
            if phdr_end > data.len() {
                return Err(String::from("ELF program-header table extends past file"));
            }

            // Scan program headers
            let mut load_segments = 0usize;
            let mut has_interp = false;
            let mut min_load_vaddr = u64::MAX;
            let mut max_load_vaddr = 0u64;

            for i in 0..phnum_usize {
                let off = phoff + i * phentsize;
                let p_type = read_u32(data, off)?;

                match p_type {
                    PT_LOAD => {
                        let filesz = read_u64(data, off + 32)? as usize;
                        let memsz = read_u64(data, off + 40)? as usize;
                        let offset = read_u64(data, off + 8)? as usize;
                        let vaddr = read_u64(data, off + 16)?;

                        // Validate segment bounds
                        let end = offset
                            .checked_add(filesz)
                            .ok_or_else(|| String::from("PT_LOAD segment overflows"))?;
                        if end > data.len() {
                            return Err(String::from("PT_LOAD segment extends past file"));
                        }

                        // Track address range
                        if vaddr < min_load_vaddr {
                            min_load_vaddr = vaddr;
                        }
                        let seg_end = vaddr.checked_add(memsz.max(filesz as u64))
                            .ok_or_else(|| String::from("PT_LOAD segment address overflows"))?;
                        if seg_end > max_load_vaddr {
                            max_load_vaddr = seg_end;
                        }

                        load_segments += 1;
                    }
                    PT_INTERP => {
                        has_interp = true;
                    }
                    _ => {}
                }
            }

            if load_segments == 0 {
                return Err(String::from("ELF contains no PT_LOAD segments"));
            }

            Ok(ElfInfo {
                entry,
                phnum,
                has_interp,
                load_segments,
                min_load_vaddr,
                max_load_vaddr,
            })
        }

        fn read_u16(data: &[u8], off: usize) -> Result<u16, String> {
            data.get(off..off + 2)
                .ok_or_else(|| String::from("ELF read out of bounds"))
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
        }

        fn read_u32(data: &[u8], off: usize) -> Result<u32, String> {
            data.get(off..off + 4)
                .ok_or_else(|| String::from("ELF read out of bounds"))
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        }

        fn read_u64(data: &[u8], off: usize) -> Result<u64, String> {
            data.get(off..off + 8)
                .ok_or_else(|| String::from("ELF read out of bounds"))
                .map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
        }
    }
}
