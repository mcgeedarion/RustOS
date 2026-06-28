//! Minimal userspace services for the `userspace_boot` profile.
//!
//! The full `fs`/`proc` module graph is intentionally kept out of the narrow
//! userspace boot image while it is being stabilised.  This module provides the
//! small surface needed by `userspace_boot`: an initramfs mount hook and a
//! conservative `/init` ELF loader plan.  The loader now does more than validate
//! bytes: it builds the PID-1 transition state that the real scheduler/page-table
//! implementation will consume.

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
        use alloc::{string::String, vec::Vec};

        const ELF_MAGIC: &[u8; 4] = b"\x7FELF";
        const ELFCLASS64: u8 = 2;
        const ELFDATA2LSB: u8 = 1;
        const EV_CURRENT: u8 = 1;
        const ET_EXEC: u16 = 2;
        const ET_DYN: u16 = 3;
        const EM_X86_64: u16 = 62;
        const EM_AARCH64: u16 = 183;
        const PT_LOAD: u32 = 1;
        const PF_X: u32 = 0x1;
        const PF_W: u32 = 0x2;
        const PF_R: u32 = 0x4;
        const PAGE_SIZE: usize = 4096;
        const INIT_PID: u32 = 1;
        const INIT_PPID: u32 = 0;
        const USER_STACK_TOP: usize = 0x0000_7FFF_FFFF_F000;
        const USER_STACK_SIZE: usize = PAGE_SIZE * 4;

        /// A prepared PID-1 transition.  This is the handoff object the future
        /// scheduler path can enqueue without reparsing the ELF image.
        #[derive(Debug)]
        pub struct UserTaskImage {
            pub pid: u32,
            pub ppid: u32,
            pub path: String,
            pub entry: usize,
            pub user_sp: usize,
            pub address_space: UserAddressSpacePlan,
            pub argv_count: usize,
            pub envp_count: usize,
        }

        /// Page-table work required before entering ring 3.
        #[derive(Debug)]
        pub struct UserAddressSpacePlan {
            /// Physical CR3/PML4 address once real page-table allocation lands.
            pub cr3: Option<usize>,
            pub mappings: Vec<UserMapping>,
        }

        /// A single user mapping the real page-table backend must install.
        #[derive(Debug, Clone)]
        pub struct UserMapping {
            pub kind: MappingKind,
            pub va_start: usize,
            pub page_count: usize,
            pub permissions: UserPerms,
            pub file_offset: usize,
            pub file_size: usize,
            pub memory_size: usize,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum MappingKind {
            Load,
            Stack,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct UserPerms {
            pub read: bool,
            pub write: bool,
            pub execute: bool,
        }

        impl UserPerms {
            fn from_elf_flags(flags: u32) -> Self {
                Self {
                    read: flags & PF_R != 0,
                    write: flags & PF_W != 0,
                    execute: flags & PF_X != 0,
                }
            }
        }

        /// Validate, load-plan, and register a userspace image for the
        /// `userspace_boot` path.
        ///
        /// This is the first concrete slice of the full scheduler/page-table/
        /// ring-3 path: it produces a PID-1 task image containing entry point,
        /// initial userspace stack pointer, and all PT_LOAD/stack mappings.  The
        /// next patch can replace `cr3: None` with real PML4 allocation and map
        /// these `UserMapping`s before invoking an architecture user-entry hook.
        pub fn spawn_user_process_from_bytes(
            path: &str,
            elf: &[u8],
            argv: &[&str],
            envp: &[&str],
        ) -> bool {
            match prepare_user_process_from_bytes(path, elf, argv, envp) {
                Ok(mut task) => {
                    crate::serial_println!(
                        "userspace: prepared PID {} {} entry={:#x} sp={:#x} mappings={}",
                        task.pid,
                        task.path.as_str(),
                        task.entry,
                        task.user_sp,
                        task.address_space.mappings.len()
                    );
                    finalize_and_enter_user(&mut task, elf)
                },
                Err(reason) => {
                    crate::serial_println!("userspace: rejected {}: {}", path, reason.as_str());
                    false
                },
            }
        }

        fn prepare_user_process_from_bytes(
            path: &str,
            elf: &[u8],
            argv: &[&str],
            envp: &[&str],
        ) -> Result<UserTaskImage, String> {
            let loaded = parse_elf64(elf)?;
            let mut mappings = Vec::new();
            for segment in loaded.load_segments {
                mappings.push(UserMapping {
                    kind: MappingKind::Load,
                    va_start: align_down(segment.vaddr, PAGE_SIZE),
                    page_count: pages_spanned(segment.vaddr, segment.mem_size)?,
                    permissions: segment.permissions,
                    file_offset: segment.file_offset,
                    file_size: segment.file_size,
                    memory_size: segment.mem_size,
                });
            }

            mappings.push(UserMapping {
                kind: MappingKind::Stack,
                va_start: USER_STACK_TOP - USER_STACK_SIZE,
                page_count: USER_STACK_SIZE / PAGE_SIZE,
                permissions: UserPerms {
                    read: true,
                    write: true,
                    execute: false,
                },
                file_offset: 0,
                file_size: 0,
                memory_size: USER_STACK_SIZE,
            });

            let user_sp = build_initial_stack_pointer(argv, envp)?;
            Ok(UserTaskImage {
                pid: INIT_PID,
                ppid: INIT_PPID,
                path: String::from(path),
                entry: loaded.entry,
                user_sp,
                address_space: UserAddressSpacePlan {
                    cr3: None,
                    mappings,
                },
                argv_count: argv.len(),
                envp_count: envp.len(),
            })
        }

        struct LoadedElfPlan {
            entry: usize,
            load_segments: Vec<LoadSegment>,
        }

        struct LoadSegment {
            vaddr: usize,
            mem_size: usize,
            file_offset: usize,
            file_size: usize,
            permissions: UserPerms,
        }

        fn parse_elf64(data: &[u8]) -> Result<LoadedElfPlan, String> {
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

            let entry = read_u64(data, 24)? as usize;
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

            let mut load_segments = Vec::new();
            for i in 0..phnum_usize {
                let off = phoff + i * phentsize;
                let p_type = read_u32(data, off)?;
                if p_type == PT_LOAD {
                    let flags = read_u32(data, off + 4)?;
                    let file_offset = read_u64(data, off + 8)? as usize;
                    let vaddr = read_u64(data, off + 16)? as usize;
                    let file_size = read_u64(data, off + 32)? as usize;
                    let mem_size = read_u64(data, off + 40)? as usize;
                    let file_end = file_offset
                        .checked_add(file_size)
                        .ok_or_else(|| String::from("PT_LOAD segment overflows"))?;
                    if file_end > data.len() {
                        return Err(String::from("PT_LOAD segment extends past file"));
                    }
                    if file_size > mem_size {
                        return Err(String::from("PT_LOAD file size exceeds memory size"));
                    }
                    if mem_size == 0 {
                        continue;
                    }
                    load_segments.push(LoadSegment {
                        vaddr,
                        mem_size,
                        file_offset,
                        file_size,
                        permissions: UserPerms::from_elf_flags(flags),
                    });
                }
            }
            if load_segments.is_empty() {
                return Err(String::from("ELF contains no PT_LOAD segments"));
            }

            Ok(LoadedElfPlan {
                entry,
                load_segments,
            })
        }

        fn build_initial_stack_pointer(argv: &[&str], envp: &[&str]) -> Result<usize, String> {
            let string_bytes = argv
                .iter()
                .chain(envp.iter())
                .try_fold(0usize, |acc, value| {
                    acc.checked_add(value.len().checked_add(1)?)
                })
                .ok_or_else(|| String::from("initial stack strings overflow"))?;
            let pointer_words = 1usize
                .checked_add(argv.len())
                .and_then(|v| v.checked_add(1))
                .and_then(|v| v.checked_add(envp.len()))
                .and_then(|v| v.checked_add(1))
                .and_then(|v| v.checked_add(2))
                .ok_or_else(|| String::from("initial stack pointer table overflow"))?;
            let table_bytes = pointer_words
                .checked_mul(core::mem::size_of::<usize>())
                .ok_or_else(|| String::from("initial stack pointer table overflow"))?;
            let total = string_bytes
                .checked_add(table_bytes)
                .ok_or_else(|| String::from("initial stack size overflow"))?;
            if total > USER_STACK_SIZE {
                return Err(String::from("initial stack does not fit"));
            }
            Ok(align_down(USER_STACK_TOP - total, 16))
        }

        fn pages_spanned(va: usize, len: usize) -> Result<usize, String> {
            let start = align_down(va, PAGE_SIZE);
            let end = va
                .checked_add(len)
                .and_then(|v| v.checked_add(PAGE_SIZE - 1))
                .map(|v| align_down(v, PAGE_SIZE))
                .ok_or_else(|| String::from("mapping range overflows"))?;
            end.checked_sub(start)
                .map(|bytes| bytes / PAGE_SIZE)
                .ok_or_else(|| String::from("mapping range underflows"))
        }

        const fn align_down(value: usize, align: usize) -> usize {
            value & !(align - 1)
        }

        #[cfg(target_arch = "x86_64")]
        fn finalize_and_enter_user(task: &mut UserTaskImage, elf: &[u8]) -> bool {
            let cr3 =
                match x86_64_user_entry::build_address_space(&task.address_space.mappings, elf) {
                    Ok(cr3) => cr3,
                    Err(reason) => {
                        crate::serial_println!(
                            "userspace: transition blocked: {}",
                            reason.as_str()
                        );
                        return false;
                    },
                };
            task.address_space.cr3 = Some(cr3);
            crate::serial_println!(
                "userspace: entering ring3 cr3={:#x} rip={:#x} rsp={:#x}",
                cr3,
                task.entry,
                task.user_sp
            );
            unsafe { x86_64_user_entry::enter_ring3(cr3, task.entry, task.user_sp) }
        }

        #[cfg(not(target_arch = "x86_64"))]
        fn finalize_and_enter_user(_task: &mut UserTaskImage, _elf: &[u8]) -> bool {
            crate::serial_println!(
                "userspace: transition blocked: ring-3 transition backend is currently implemented for x86_64 only"
            );
            false
        }

        #[cfg(target_arch = "x86_64")]
        mod x86_64_user_entry {
            use super::{MappingKind, UserMapping, UserPerms, PAGE_SIZE};
            use alloc::string::String;
            use core::arch::asm;
            use core::cmp::{max, min};
            use core::sync::atomic::{AtomicUsize, Ordering};

            const BOOT_PAGE_POOL_PAGES: usize = 256;
            const PTE_PRESENT: u64 = 1 << 0;
            const PTE_WRITABLE: u64 = 1 << 1;
            const PTE_USER: u64 = 1 << 2;
            const PTE_ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
            const USER_CS: u64 = 0x1b;
            const USER_SS: u64 = 0x23;
            const USER_RFLAGS: u64 = 0x202;

            #[repr(align(4096))]
            #[derive(Clone, Copy)]
            struct Page([u8; PAGE_SIZE]);

            static mut PAGE_POOL: [Page; BOOT_PAGE_POOL_PAGES] =
                [Page([0; PAGE_SIZE]); BOOT_PAGE_POOL_PAGES];
            static NEXT_PAGE: AtomicUsize = AtomicUsize::new(0);

            pub fn build_address_space(
                mappings: &[UserMapping],
                elf: &[u8],
            ) -> Result<usize, String> {
                let cr3 = alloc_zeroed_page()?;
                let kernel_cr3 = current_cr3();
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        kernel_cr3 as *const u8,
                        cr3 as *mut u8,
                        PAGE_SIZE,
                    );
                }

                for mapping in mappings {
                    match mapping.kind {
                        MappingKind::Load => map_load_mapping(cr3, mapping, elf)?,
                        MappingKind::Stack => map_zero_mapping(cr3, mapping)?,
                    }
                }

                Ok(cr3)
            }

            fn map_load_mapping(
                cr3: usize,
                mapping: &UserMapping,
                elf: &[u8],
            ) -> Result<(), String> {
                let segment_va = mapping.va_start;
                let file_start = mapping.file_offset;
                let file_end = file_start
                    .checked_add(mapping.file_size)
                    .ok_or_else(|| String::from("PT_LOAD file range overflows"))?;
                if file_end > elf.len() {
                    return Err(String::from("PT_LOAD file range extends past ELF"));
                }

                for page_index in 0..mapping.page_count {
                    let page_va = mapping
                        .va_start
                        .checked_add(page_index * PAGE_SIZE)
                        .ok_or_else(|| String::from("user mapping VA overflows"))?;
                    let page_pa = alloc_zeroed_page()?;
                    map_page(cr3, page_va, page_pa, pte_flags(mapping.permissions));

                    let page_start = page_va;
                    let page_end = page_va + PAGE_SIZE;
                    let seg_file_start_va = segment_va;
                    let seg_file_end_va = segment_va + mapping.file_size;
                    let copy_start_va = max(page_start, seg_file_start_va);
                    let copy_end_va = min(page_end, seg_file_end_va);
                    if copy_start_va < copy_end_va {
                        let src_off = file_start + (copy_start_va - seg_file_start_va);
                        let len = copy_end_va - copy_start_va;
                        let dst_off = copy_start_va - page_start;
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                elf.as_ptr().add(src_off),
                                (page_pa + dst_off) as *mut u8,
                                len,
                            );
                        }
                    }
                }
                Ok(())
            }

            fn map_zero_mapping(cr3: usize, mapping: &UserMapping) -> Result<(), String> {
                for page_index in 0..mapping.page_count {
                    let page_va = mapping
                        .va_start
                        .checked_add(page_index * PAGE_SIZE)
                        .ok_or_else(|| String::from("user stack VA overflows"))?;
                    let page_pa = alloc_zeroed_page()?;
                    map_page(cr3, page_va, page_pa, pte_flags(mapping.permissions));
                }
                Ok(())
            }

            fn pte_flags(perms: UserPerms) -> u64 {
                let mut flags = PTE_PRESENT | PTE_USER;
                if perms.write {
                    flags |= PTE_WRITABLE;
                }
                flags
            }

            fn alloc_zeroed_page() -> Result<usize, String> {
                let index = NEXT_PAGE.fetch_add(1, Ordering::SeqCst);
                if index >= BOOT_PAGE_POOL_PAGES {
                    return Err(String::from("userspace boot page pool exhausted"));
                }
                let ptr = unsafe { core::ptr::addr_of_mut!(PAGE_POOL[index].0) as *mut u8 };
                unsafe {
                    core::ptr::write_bytes(ptr, 0, PAGE_SIZE);
                }
                Ok(ptr as usize)
            }

            fn current_cr3() -> usize {
                let cr3: usize;
                unsafe {
                    asm!("mov {}, cr3", out(reg) cr3, options(nostack, nomem));
                }
                cr3 & !(PAGE_SIZE - 1)
            }

            fn map_page(cr3: usize, va: usize, pa: usize, flags: u64) {
                unsafe {
                    let pte = walk_mut(cr3, va);
                    *pte = (pa as u64 & PTE_ADDR_MASK) | flags | PTE_PRESENT;
                }
            }

            unsafe fn walk_mut(cr3: usize, va: usize) -> *mut u64 {
                let pml4_idx = (va >> 39) & 0x1ff;
                let pdpt_idx = (va >> 30) & 0x1ff;
                let pd_idx = (va >> 21) & 0x1ff;
                let pt_idx = (va >> 12) & 0x1ff;

                let pml4e = pte_ptr(cr3, pml4_idx);
                let pdpt = next_table(pml4e);
                let pdpte = pte_ptr(pdpt, pdpt_idx);
                let pd = next_table(pdpte);
                let pde = pte_ptr(pd, pd_idx);
                let pt = next_table(pde);
                pte_ptr(pt, pt_idx)
            }

            unsafe fn next_table(entry: *mut u64) -> usize {
                if *entry & PTE_PRESENT == 0 {
                    let table =
                        alloc_zeroed_page().expect("userspace boot page-table pool exhausted");
                    *entry = (table as u64 & PTE_ADDR_MASK) | PTE_PRESENT | PTE_WRITABLE | PTE_USER;
                    table
                } else {
                    *entry |= PTE_USER;
                    (*entry & PTE_ADDR_MASK) as usize
                }
            }

            unsafe fn pte_ptr(table_pa: usize, idx: usize) -> *mut u64 {
                (table_pa + idx * core::mem::size_of::<u64>()) as *mut u64
            }

            pub unsafe fn enter_ring3(cr3: usize, entry: usize, user_sp: usize) -> ! {
                asm!(
                    "cli",
                    "mov cr3, {cr3}",
                    "mov ax, {ss:x}",
                    "mov ds, ax",
                    "mov es, ax",
                    "push {ss}",
                    "push {rsp}",
                    "push {rflags}",
                    "push {cs}",
                    "push {rip}",
                    "iretq",
                    cr3 = in(reg) cr3,
                    ss = in(reg) USER_SS,
                    cs = in(reg) USER_CS,
                    rflags = in(reg) USER_RFLAGS,
                    rsp = in(reg) user_sp as u64,
                    rip = in(reg) entry as u64,
                    options(noreturn)
                )
            }
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
