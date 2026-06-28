//! Minimal userspace services for the `userspace_boot` profile.
//!
//! The full `fs`/`proc` module graph is intentionally kept out of the narrow
//! userspace boot image while it is being stabilised.  This module provides the
//! small surface needed by `userspace_boot`: an initramfs mount hook and a
//! conservative `/init` ELF validator.  The validator is deliberately strict so
//! that the kernel only reports a userspace handoff after it has found an ELF64
//! executable for the running architecture.

pub mod fs {
    pub mod initramfs {
        /// Mount the initramfs for the userspace-boot profile.
        ///
        /// `userspace_boot` reads `/init` directly from the raw CPIO archive via
        /// `crate::init::initramfs::load()`, so the narrow profile does not need
        /// to populate the full VFS tree yet.  Keep this hook explicit so the
        /// call site remains identical to the full-kernel boot path.
        pub fn mount_initramfs() {
            if crate::init::initramfs::has_initramfs_range() {
                crate::serial_println!("initramfs: range registered; VFS mount deferred");
            } else {
                crate::serial_println!("initramfs: no range registered before mount hook");
            }
        }
    }
}

pub mod proc {
    pub mod exec {
        use alloc::string::String;

        const ELF_MAGIC: &[u8; 4] = b"\x7FELF";
        const ELFCLASS64: u8 = 2;
        const ELFDATA2LSB: u8 = 1;
        const EV_CURRENT: u8 = 1;
        const ET_EXEC: u16 = 2;
        const ET_DYN: u16 = 3;
        const EM_X86_64: u16 = 62;
        const EM_AARCH64: u16 = 183;
        const PT_LOAD: u32 = 1;

        /// Validate and register a userspace image for the `userspace_boot` path.
        ///
        /// The narrow profile does not compile the full process scheduler yet,
        /// but it does perform the same first gate a real exec path needs: parse
        /// the ELF header, verify the target architecture, and ensure at least
        /// one loadable segment is present.  This keeps `/init` handoff failures
        /// explicit instead of reporting success for arbitrary bytes.
        pub fn spawn_user_process_from_bytes(
            path: &str,
            elf: &[u8],
            argv: &[&str],
            envp: &[&str],
        ) -> bool {
            match validate_elf64(elf) {
                Ok(info) => {
                    crate::serial_println!(
                        "userspace: validated {} entry={:#x} phnum={} argv={} envp={}",
                        path,
                        info.entry,
                        info.phnum,
                        argv.len(),
                        envp.len()
                    );
                    crate::serial_println!(
                        "userspace: scheduler handoff pending full proc graph; PID 1 image accepted"
                    );
                    true
                },
                Err(reason) => {
                    crate::serial_println!("userspace: rejected {}: {}", path, reason.as_str());
                    false
                },
            }
        }

        struct ElfInfo {
            entry: u64,
            phnum: u16,
        }

        fn validate_elf64(data: &[u8]) -> Result<ElfInfo, String> {
            if data.len() < 64 {
                return Err(String::from("ELF image is smaller than ELF64 header"));
            }
            if &data[0..4] != ELF_MAGIC {
                return Err(String::from("bad ELF magic"));
            }
            if data[4] != ELFCLASS64 {
                return Err(String::from("not an ELF64 image"));
            }
            if data[5] != ELFDATA2LSB {
                return Err(String::from("not little-endian ELF"));
            }
            if data[6] != EV_CURRENT {
                return Err(String::from("unsupported ELF version"));
            }

            let e_type = read_u16(data, 16)?;
            if e_type != ET_EXEC && e_type != ET_DYN {
                return Err(String::from("ELF is not ET_EXEC or ET_DYN"));
            }

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

            let entry = read_u64(data, 24)?;
            let phoff = read_u64(data, 32)? as usize;
            let phentsize = read_u16(data, 54)? as usize;
            let phnum = read_u16(data, 56)?;
            if phentsize < 56 || phnum == 0 {
                return Err(String::from("invalid ELF program-header table"));
            }

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

            let mut load_segments = 0usize;
            for i in 0..phnum_usize {
                let off = phoff + i * phentsize;
                let p_type = read_u32(data, off)?;
                if p_type == PT_LOAD {
                    let filesz = read_u64(data, off + 32)? as usize;
                    let offset = read_u64(data, off + 8)? as usize;
                    let end = offset
                        .checked_add(filesz)
                        .ok_or_else(|| String::from("PT_LOAD segment overflows"))?;
                    if end > data.len() {
                        return Err(String::from("PT_LOAD segment extends past file"));
                    }
                    load_segments += 1;
                }
            }
            if load_segments == 0 {
                return Err(String::from("ELF contains no PT_LOAD segments"));
            }

            Ok(ElfInfo { entry, phnum })
        }

        fn read_u16(data: &[u8], off: usize) -> Result<u16, String> {
            let bytes = data
                .get(off..off + 2)
                .ok_or_else(|| String::from("ELF read out of bounds"))?;
            Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
        }

        fn read_u32(data: &[u8], off: usize) -> Result<u32, String> {
            let bytes = data
                .get(off..off + 4)
                .ok_or_else(|| String::from("ELF read out of bounds"))?;
            Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        }

        fn read_u64(data: &[u8], off: usize) -> Result<u64, String> {
            let bytes = data
                .get(off..off + 8)
                .ok_or_else(|| String::from("ELF read out of bounds"))?;
            Ok(u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]))
        }
    }
}
