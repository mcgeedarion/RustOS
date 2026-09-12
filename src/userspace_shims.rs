//! Diagnostic services for the transitional `userspace_boot` profile.
//!
//! This profile excludes the full VFS, MM, and scheduler. Use the real raw
//! initramfs parser, but never pretend that a VFS mount or process spawn has
//! happened. A valid executable still produces `ExecutionUnavailable` until
//! the full execution path is integrated.

pub mod fs {
    /// Raw, zero-copy archive access only; this is not a mounted VFS.
    pub use crate::init::initramfs;
    pub use initramfs::has_initramfs_range;
}

pub mod proc {
    pub mod exec {
        use alloc::string::String;

        const ELF_MAGIC: &[u8; 4] = b"\x7FELF";
        const PT_LOAD: u32 = 1;
        const PT_INTERP: u32 = 3;

        #[derive(Debug, PartialEq, Eq)]
        pub enum SpawnError {
            InvalidElf(String),
            DynamicLinkerUnsupported,
            ExecutionUnavailable,
        }

        impl SpawnError {
            pub fn as_str(&self) -> &str {
                match self {
                    Self::InvalidElf(reason) => reason,
                    Self::DynamicLinkerUnsupported => {
                        "dynamically linked init binaries are not supported"
                    },
                    Self::ExecutionUnavailable => {
                        "userspace_boot has no process execution backend; full MM and scheduler integration is required"
                    },
                }
            }
        }

        /// Validate an init image and report the unsupported execution boundary.
        ///
        /// No PID, address space, stack, or runnable process is allocated here.
        /// Success must not be reported until those operations really exist.
        pub fn spawn_user_process_from_bytes(
            _path: &str,
            elf: &[u8],
            _argv: &[&str],
            _envp: &[&str],
        ) -> Result<(), SpawnError> {
            let has_interp = validate_elf64(elf).map_err(SpawnError::InvalidElf)?;
            if has_interp {
                return Err(SpawnError::DynamicLinkerUnsupported);
            }
            Err(SpawnError::ExecutionUnavailable)
        }

        /// Validate bounded ELF64 header/segment metadata, not executable mappings.
        fn validate_elf64(data: &[u8]) -> Result<bool, String> {
            if data.len() < 64 {
                return Err(String::from("ELF image is smaller than ELF64 header"));
            }
            if &data[0..4] != ELF_MAGIC {
                return Err(String::from("bad ELF magic"));
            }
            if data[4] != 2 || data[5] != 1 || data[6] != 1 {
                return Err(String::from(
                    "expected a little-endian ELF64 version 1 image",
                ));
            }
            let e_type = read_u16(data, 16)?;
            if e_type != 2 && e_type != 3 {
                return Err(String::from("ELF is not ET_EXEC or ET_DYN"));
            }
            let machine = read_u16(data, 18)?;
            #[cfg(target_arch = "x86_64")]
            if machine != 62 {
                return Err(String::from("ELF machine does not match x86_64"));
            }
            #[cfg(target_arch = "aarch64")]
            if machine != 183 {
                return Err(String::from("ELF machine does not match aarch64"));
            }
            #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
            return Err(String::from("unsupported userspace_boot architecture"));

            let phoff = usize::try_from(read_u64(data, 32)?)
                .map_err(|_| String::from("ELF program-header offset overflows"))?;
            let phentsize = read_u16(data, 54)? as usize;
            let phnum = read_u16(data, 56)? as usize;
            if phentsize < 56 || phnum == 0 {
                return Err(String::from("invalid ELF program-header table"));
            }
            let phdr_end = phentsize
                .checked_mul(phnum)
                .and_then(|bytes| phoff.checked_add(bytes))
                .ok_or_else(|| String::from("ELF program-header table overflows"))?;
            if phdr_end > data.len() {
                return Err(String::from("ELF program-header table extends past file"));
            }

            let mut load_segments = 0;
            let mut has_interp = false;
            for i in 0..phnum {
                let off = phoff + i * phentsize;
                match read_u32(data, off)? {
                    PT_LOAD => {
                        let offset = read_u64(data, off + 8)?;
                        let vaddr = read_u64(data, off + 16)?;
                        let filesz = read_u64(data, off + 32)?;
                        let memsz = read_u64(data, off + 40)?;
                        if filesz > memsz {
                            return Err(String::from("PT_LOAD file size exceeds memory size"));
                        }
                        let end = offset
                            .checked_add(filesz)
                            .ok_or_else(|| String::from("PT_LOAD segment overflows"))?;
                        if end > data.len() as u64 {
                            return Err(String::from("PT_LOAD segment extends past file"));
                        }
                        vaddr
                            .checked_add(memsz)
                            .ok_or_else(|| String::from("PT_LOAD segment address overflows"))?;
                        load_segments += 1;
                    },
                    PT_INTERP => has_interp = true,
                    _ => {},
                }
            }
            if load_segments == 0 {
                return Err(String::from("ELF contains no PT_LOAD segments"));
            }
            Ok(has_interp)
        }

        fn read<const N: usize>(data: &[u8], off: usize) -> Result<[u8; N], String> {
            let end = off
                .checked_add(N)
                .ok_or_else(|| String::from("ELF read offset overflows"))?;
            data.get(off..end)
                .and_then(|bytes| bytes.try_into().ok())
                .ok_or_else(|| String::from("ELF read out of bounds"))
        }

        fn read_u16(data: &[u8], off: usize) -> Result<u16, String> {
            read(data, off).map(u16::from_le_bytes)
        }

        fn read_u32(data: &[u8], off: usize) -> Result<u32, String> {
            read(data, off).map(u32::from_le_bytes)
        }

        fn read_u64(data: &[u8], off: usize) -> Result<u64, String> {
            read(data, off).map(u64::from_le_bytes)
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use alloc::vec;
            use alloc::vec::Vec;

            fn static_elf() -> Vec<u8> {
                let mut elf = vec![0; 128];
                elf[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
                elf[16..18].copy_from_slice(&2u16.to_le_bytes());
                #[cfg(target_arch = "x86_64")]
                let machine = 62u16;
                #[cfg(target_arch = "aarch64")]
                let machine = 183u16;
                elf[18..20].copy_from_slice(&machine.to_le_bytes());
                elf[20..24].copy_from_slice(&1u32.to_le_bytes());
                elf[24..32].copy_from_slice(&0x400078u64.to_le_bytes());
                elf[32..40].copy_from_slice(&64u64.to_le_bytes());
                elf[52..54].copy_from_slice(&64u16.to_le_bytes());
                elf[54..56].copy_from_slice(&56u16.to_le_bytes());
                elf[56..58].copy_from_slice(&1u16.to_le_bytes());
                elf[64..68].copy_from_slice(&PT_LOAD.to_le_bytes());
                elf[68..72].copy_from_slice(&5u32.to_le_bytes());
                elf[80..88].copy_from_slice(&0x400000u64.to_le_bytes());
                elf[96..104].copy_from_slice(&128u64.to_le_bytes());
                elf[104..112].copy_from_slice(&128u64.to_le_bytes());
                elf
            }

            fn spawn(elf: &[u8]) -> Result<(), SpawnError> {
                spawn_user_process_from_bytes("/init", elf, &["/init"], &[])
            }

            #[test]
            fn valid_elf_does_not_claim_process_execution() {
                assert_eq!(spawn(&static_elf()), Err(SpawnError::ExecutionUnavailable));
            }

            #[test]
            fn every_truncated_image_is_rejected() {
                let elf = static_elf();
                for len in 0..elf.len() {
                    assert!(matches!(spawn(&elf[..len]), Err(SpawnError::InvalidElf(_))));
                }
            }

            #[test]
            fn wrong_machine_is_rejected() {
                let mut elf = static_elf();
                elf[18..20].copy_from_slice(&0u16.to_le_bytes());
                assert!(matches!(spawn(&elf), Err(SpawnError::InvalidElf(_))));
            }

            #[test]
            fn overflowing_program_header_offset_is_rejected() {
                let mut elf = static_elf();
                elf[32..40].copy_from_slice(&u64::MAX.to_le_bytes());
                assert!(matches!(spawn(&elf), Err(SpawnError::InvalidElf(_))));
            }

            #[test]
            fn invalid_load_segment_bounds_are_rejected() {
                for (offset, value) in [(72, u64::MAX), (80, u64::MAX), (104, 1)] {
                    let mut elf = static_elf();
                    elf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
                    assert!(matches!(spawn(&elf), Err(SpawnError::InvalidElf(_))));
                }
            }

            #[test]
            fn dynamic_linker_is_reported_separately() {
                let mut elf = static_elf();
                elf.resize(184, 0);
                elf[56..58].copy_from_slice(&2u16.to_le_bytes());
                elf[120..124].copy_from_slice(&PT_INTERP.to_le_bytes());
                assert_eq!(spawn(&elf), Err(SpawnError::DynamicLinkerUnsupported));
            }
        }
    }
}
