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
            pub segment_va: usize,
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

        #[cfg(target_arch = "aarch64")]
        fn aarch64_log(message: &str) {
            crate::arch::aarch64::serial::write_bytes(message.as_bytes());
            crate::arch::aarch64::serial::write_byte(b'\n');
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
            #[cfg(target_arch = "aarch64")]
            aarch64_log("aarch64-userspace: spawn begin");
            match prepare_user_process_from_bytes(path, elf, argv, envp) {
                Ok(mut task) => {
                    #[cfg(target_arch = "aarch64")]
                    aarch64_log("aarch64-userspace: task image prepared");
                    #[cfg(target_arch = "aarch64")]
                    aarch64_log("userspace: prepared PID 1 /init");
                    #[cfg(not(target_arch = "aarch64"))]
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
                    #[cfg(target_arch = "aarch64")]
                    {
                        let _ = (path, reason);
                        aarch64_log("aarch64-userspace: spawn rejected");
                        aarch64_log("userspace: rejected /init");
                    }
                    #[cfg(not(target_arch = "aarch64"))]
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
            #[cfg(target_arch = "aarch64")]
            aarch64_log("aarch64-userspace: parse elf");
            let loaded = parse_elf64(elf)?;
            #[cfg(target_arch = "aarch64")]
            aarch64_log("aarch64-userspace: elf parsed");
            let mut mappings = Vec::new();
            for segment in loaded.load_segments {
                mappings.push(UserMapping {
                    kind: MappingKind::Load,
                    va_start: align_down(segment.vaddr, PAGE_SIZE),
                    segment_va: segment.vaddr,
                    page_count: pages_spanned(segment.vaddr, segment.mem_size)?,
                    permissions: segment.permissions,
                    file_offset: segment.file_offset,
                    file_size: segment.file_size,
                    memory_size: segment.mem_size,
                });
            }

            #[cfg(target_arch = "aarch64")]
            aarch64_log("aarch64-userspace: load mappings planned");

            mappings.push(UserMapping {
                kind: MappingKind::Stack,
                va_start: USER_STACK_TOP - USER_STACK_SIZE,
                segment_va: USER_STACK_TOP - USER_STACK_SIZE,
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

            #[cfg(target_arch = "aarch64")]
            aarch64_log("aarch64-userspace: mappings planned");
            let user_sp = build_initial_stack_pointer(argv, envp)?;
            #[cfg(target_arch = "aarch64")]
            aarch64_log("aarch64-userspace: task image ready");
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

        #[derive(Clone, Copy)]
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
            #[cfg(target_arch = "aarch64")]
            aarch64_log("aarch64-userspace: elf len ok");
            if data[0] != 0x7f || data[1] != b'E' || data[2] != b'L' || data[3] != b'F' {
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
            #[cfg(target_arch = "aarch64")]
            aarch64_log("aarch64-userspace: elf ident ok");

            let e_type = read_u16(data, 16)?;
            #[cfg(target_arch = "aarch64")]
            aarch64_log("aarch64-userspace: elf type read");
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
            #[cfg(target_arch = "aarch64")]
            aarch64_log("aarch64-userspace: elf machine ok");
            #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
            let _ = machine;

            let entry = read_u64(data, 24)? as usize;
            #[cfg(target_arch = "aarch64")]
            aarch64_log("aarch64-userspace: elf entry read");
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
            #[cfg(target_arch = "aarch64")]
            aarch64_log("aarch64-userspace: phdr table ok");

            let mut load_segments = Vec::new();
            for i in 0..phnum_usize {
                #[cfg(target_arch = "aarch64")]
                aarch64_log("aarch64-userspace: phdr next");
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
                    #[cfg(target_arch = "aarch64")]
                    aarch64_log("aarch64-userspace: load segment recorded");
                }
            }
            if load_segments.is_empty() {
                return Err(String::from("ELF contains no PT_LOAD segments"));
            }

            #[cfg(target_arch = "aarch64")]
            aarch64_log("aarch64-userspace: elf plan complete");
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

        #[cfg(target_arch = "aarch64")]
        fn finalize_and_enter_user(task: &mut UserTaskImage, elf: &[u8]) -> bool {
            aarch64_log("aarch64-userspace: build address space");
            let ttbr0 =
                match aarch64_user_entry::build_address_space(&task.address_space.mappings, elf) {
                    Ok(ttbr0) => ttbr0,
                    Err(reason) => {
                        let _ = reason;
                        aarch64_log("aarch64-userspace: address space failed");
                        aarch64_log("userspace: transition blocked");
                        return false;
                    },
                };
            aarch64_log("aarch64-userspace: address space built");
            task.address_space.cr3 = Some(ttbr0);
            aarch64_log("aarch64-userspace: eret to EL0");
            aarch64_log("userspace: entering EL0");
            unsafe { aarch64_user_entry::enter_el0(ttbr0, task.entry, task.user_sp) }
        }

        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        fn finalize_and_enter_user(_task: &mut UserTaskImage, _elf: &[u8]) -> bool {
            crate::serial_println!(
                "userspace: transition blocked: ring-3 transition backend is currently implemented for x86_64/aarch64 only"
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
            const PTE_HUGE: u64 = 1 << 7;
            const PTE_ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
            const USER_CS: u64 = 0x1b;
            const USER_SS: u64 = 0x23;
            const USER_RFLAGS: u64 = 0x202;
            const KERNEL_CS: u64 = 0x08;
            const MSR_EFER: u32 = 0xC000_0080;
            const MSR_STAR: u32 = 0xC000_0081;
            const MSR_LSTAR: u32 = 0xC000_0082;
            const MSR_FMASK: u32 = 0xC000_0084;
            const EFER_SCE: u64 = 1;
            const SYS_WRITE: usize = 1;
            const SYS_BRK: usize = 12;
            const SYS_EXIT_GROUP: usize = 231;
            const INIT_BRK: usize = 0x0000_5555_0000_0000;
            const KCODE64: u64 = (1 << 47) | (1 << 44) | (1 << 43) | (1 << 53);
            const KDATA: u64 = (1 << 47) | (1 << 44) | (1 << 41);
            const UCODE64: u64 = (1 << 47) | (3 << 45) | (1 << 44) | (1 << 43) | (1 << 53);
            const UDATA: u64 = (1 << 47) | (3 << 45) | (1 << 44) | (1 << 41);

            #[repr(align(4096))]
            #[derive(Clone, Copy)]
            struct Page([u8; PAGE_SIZE]);

            static mut PAGE_POOL: [Page; BOOT_PAGE_POOL_PAGES] =
                [Page([0; PAGE_SIZE]); BOOT_PAGE_POOL_PAGES];
            static NEXT_PAGE: AtomicUsize = AtomicUsize::new(0);
            static USER_BOOT_GDT: [u64; 5] = [0, KCODE64, KDATA, UCODE64, UDATA];

            #[repr(C, packed)]
            struct DescriptorTablePointer {
                limit: u16,
                base: u64,
            }

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
                let segment_va = mapping.segment_va;
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
                let pdpt = next_table(pml4e, PageTableLevel::Pml4);
                let pdpte = pte_ptr(pdpt, pdpt_idx);
                let pd = next_table(pdpte, PageTableLevel::Pdpt);
                let pde = pte_ptr(pd, pd_idx);
                let pt = next_table(pde, PageTableLevel::Pd);
                pte_ptr(pt, pt_idx)
            }

            #[derive(Clone, Copy)]
            enum PageTableLevel {
                Pml4,
                Pdpt,
                Pd,
            }

            unsafe fn next_table(entry: *mut u64, level: PageTableLevel) -> usize {
                if *entry & PTE_PRESENT == 0 {
                    let table =
                        alloc_zeroed_page().expect("userspace boot page-table pool exhausted");
                    *entry = (table as u64 & PTE_ADDR_MASK) | PTE_PRESENT | PTE_WRITABLE | PTE_USER;
                    table
                } else {
                    let table = (*entry & PTE_ADDR_MASK) as usize;
                    if is_boot_pool_page(table) {
                        *entry |= PTE_USER;
                        table
                    } else if *entry & PTE_HUGE != 0 {
                        let split = split_huge_mapping(*entry, level);
                        *entry =
                            (split as u64 & PTE_ADDR_MASK) | PTE_PRESENT | PTE_WRITABLE | PTE_USER;
                        split
                    } else {
                        let cloned =
                            alloc_zeroed_page().expect("userspace boot page-table pool exhausted");
                        core::ptr::copy_nonoverlapping(
                            table as *const u8,
                            cloned as *mut u8,
                            PAGE_SIZE,
                        );
                        *entry = (cloned as u64 & PTE_ADDR_MASK)
                            | ((*entry | PTE_USER | PTE_WRITABLE) & !PTE_ADDR_MASK);
                        cloned
                    }
                }
            }

            unsafe fn split_huge_mapping(entry: u64, level: PageTableLevel) -> usize {
                let table = alloc_zeroed_page().expect("userspace boot page-table pool exhausted");
                let base = entry & PTE_ADDR_MASK;
                let inherited = entry & !PTE_ADDR_MASK;
                match level {
                    PageTableLevel::Pml4 => table,
                    PageTableLevel::Pdpt => {
                        let flags = (inherited | PTE_PRESENT | PTE_WRITABLE) & !PTE_ADDR_MASK;
                        for index in 0..512usize {
                            let child = pte_ptr(table, index);
                            *child = base + ((index as u64) << 21) | flags | PTE_HUGE;
                        }
                        table
                    },
                    PageTableLevel::Pd => {
                        let flags =
                            (inherited | PTE_PRESENT | PTE_WRITABLE) & !(PTE_ADDR_MASK | PTE_HUGE);
                        for index in 0..512usize {
                            let child = pte_ptr(table, index);
                            *child = base + ((index as u64) << 12) | flags;
                        }
                        table
                    },
                }
            }

            fn is_boot_pool_page(addr: usize) -> bool {
                let start = core::ptr::addr_of!(PAGE_POOL) as usize;
                let end = start + BOOT_PAGE_POOL_PAGES * PAGE_SIZE;
                addr >= start && addr < end && addr & (PAGE_SIZE - 1) == 0
            }

            unsafe fn pte_ptr(table_pa: usize, idx: usize) -> *mut u64 {
                (table_pa + idx * core::mem::size_of::<u64>()) as *mut u64
            }

            pub unsafe fn enter_ring3(cr3: usize, entry: usize, user_sp: usize) -> ! {
                load_user_boot_gdt();
                setup_syscall_entry();
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

            unsafe fn load_user_boot_gdt() {
                let ptr = DescriptorTablePointer {
                    limit: (core::mem::size_of_val(&USER_BOOT_GDT) - 1) as u16,
                    base: core::ptr::addr_of!(USER_BOOT_GDT) as u64,
                };
                asm!("lgdt [{}]", in(reg) &ptr, options(readonly, nostack, preserves_flags));
            }

            unsafe fn setup_syscall_entry() {
                let efer = rdmsr(MSR_EFER) | EFER_SCE;
                wrmsr(MSR_EFER, efer);
                let star = ((USER_CS - 16) << 48) | (KERNEL_CS << 32);
                wrmsr(MSR_STAR, star);
                wrmsr(MSR_LSTAR, syscall_entry as *const () as usize as u64);
                wrmsr(MSR_FMASK, 0);
            }

            unsafe fn rdmsr(msr: u32) -> u64 {
                let lo: u32;
                let hi: u32;
                asm!(
                    "rdmsr",
                    in("ecx") msr,
                    out("eax") lo,
                    out("edx") hi,
                    options(nostack, nomem, preserves_flags)
                );
                ((hi as u64) << 32) | lo as u64
            }

            unsafe fn wrmsr(msr: u32, value: u64) {
                asm!(
                    "wrmsr",
                    in("ecx") msr,
                    in("eax") value as u32,
                    in("edx") (value >> 32) as u32,
                    options(nostack, nomem, preserves_flags)
                );
            }

            #[unsafe(naked)]
            unsafe extern "C" fn syscall_entry() -> ! {
                core::arch::naked_asm!(
                    "push rcx",
                    "push r11",
                    "mov r10, rdx",
                    "mov rcx, rax",
                    "mov rdx, rdi",
                    "mov r8, rsi",
                    "mov r9, r10",
                    "sub rsp, 32",
                    "call {handler}",
                    "add rsp, 32",
                    "pop r11",
                    "pop rcx",
                    "sysretq",
                    handler = sym syscall_handler,
                );
            }

            extern "C" fn syscall_handler(
                syscall: usize,
                arg0: usize,
                arg1: usize,
                arg2: usize,
            ) -> usize {
                match syscall {
                    SYS_WRITE => sys_write(arg0, arg1 as *const u8, arg2),
                    SYS_BRK => INIT_BRK,
                    SYS_EXIT_GROUP => {
                        crate::serial_println!("userspace: PID 1 exit_group({})", arg0);
                        loop {
                            unsafe {
                                asm!("hlt", options(nomem, nostack));
                            }
                        }
                    },
                    _ => usize::MAX,
                }
            }

            fn sys_write(fd: usize, ptr: *const u8, len: usize) -> usize {
                if fd != 1 && fd != 2 {
                    return usize::MAX;
                }
                if ptr.is_null() {
                    return usize::MAX;
                }
                let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
                for byte in bytes {
                    crate::arch::x86_64::serial::write_byte(*byte);
                }
                len
            }
        }

        #[cfg(target_arch = "aarch64")]
        mod aarch64_user_entry {
            use super::{MappingKind, UserMapping, UserPerms, PAGE_SIZE};
            use alloc::string::String;
            use core::arch::{asm, global_asm};
            use core::cmp::{max, min};
            use core::sync::atomic::{AtomicUsize, Ordering};

            const BOOT_PAGE_POOL_PAGES: usize = 256;
            const DESC_VALID: usize = 1 << 0;
            const DESC_TABLE: usize = 1 << 1;
            const DESC_PAGE: usize = 1 << 1;
            const ATTR_NORMAL: usize = 0 << 2;
            const AP_RW_EL0: usize = 1 << 6;
            const AP_RO_EL0: usize = 3 << 6;
            const SH_INNER: usize = 3 << 8;
            const AF: usize = 1 << 10;
            const PXN: usize = 1 << 53;
            const UXN: usize = 1 << 54;
            const ADDR_MASK: usize = 0x0000_ffff_ffff_f000;
            const SYS_WRITE: usize = 64;
            const SYS_BRK: usize = 214;
            const SYS_EXIT_GROUP: usize = 94;
            const INIT_BRK: usize = 0x0000_5555_0000_0000;
            const SPSR_EL0T_DAIF_MASKED: usize = 0x3c0;
            const EC_MASK: usize = 0x3f;
            const EC_SHIFT: usize = 26;
            const EC_SVC64: usize = 0x15;

            #[repr(align(4096))]
            #[derive(Clone, Copy)]
            struct Page([u8; PAGE_SIZE]);

            static mut PAGE_POOL: [Page; BOOT_PAGE_POOL_PAGES] =
                [Page([0; PAGE_SIZE]); BOOT_PAGE_POOL_PAGES];
            static NEXT_PAGE: AtomicUsize = AtomicUsize::new(0);

            #[repr(C)]
            struct Aarch64BootFrame {
                x: [usize; 31],
                current_sp: usize,
                sp_el0: usize,
                elr_el1: usize,
                spsr_el1: usize,
                esr_el1: usize,
                far_el1: usize,
                ttbr0_el1: usize,
                ttbr1_el1: usize,
            }

            extern "C" {
                static __rustos_userspace_boot_vectors: u8;
            }

            global_asm!(
                r#"
                .section .text.rustos_userspace_boot_vectors,"ax"
                .balign 2048
                .global __rustos_userspace_boot_vectors
            __rustos_userspace_boot_vectors:
                b __rustos_userspace_boot_sync_entry
                .space 0x80 - 4
                b .
                .space 0x80 - 4
                b .
                .space 0x80 - 4
                b .
                .space 0x80 - 4
                b __rustos_userspace_boot_sync_entry
                .space 0x80 - 4
                b .
                .space 0x80 - 4
                b .
                .space 0x80 - 4
                b .
                .space 0x80 - 4
                b __rustos_userspace_boot_sync_entry
                .space 0x80 - 4
                b .
                .space 0x80 - 4
                b .
                .space 0x80 - 4
                b .
                .space 0x80 - 4
                b __rustos_userspace_boot_sync_entry
                .space 0x80 - 4
                b .
                .space 0x80 - 4
                b .
                .space 0x80 - 4
                b .

                .balign 16
            __rustos_userspace_boot_sync_entry:
                sub sp, sp, #320
                stp x0, x1, [sp, #0]
                stp x2, x3, [sp, #16]
                stp x4, x5, [sp, #32]
                stp x6, x7, [sp, #48]
                stp x8, x9, [sp, #64]
                stp x10, x11, [sp, #80]
                stp x12, x13, [sp, #96]
                stp x14, x15, [sp, #112]
                stp x16, x17, [sp, #128]
                stp x18, x19, [sp, #144]
                stp x20, x21, [sp, #160]
                stp x22, x23, [sp, #176]
                stp x24, x25, [sp, #192]
                stp x26, x27, [sp, #208]
                stp x28, x29, [sp, #224]
                str x30, [sp, #240]
                add x9, sp, #320
                str x9, [sp, #248]
                mrs x9, sp_el0
                str x9, [sp, #256]
                mrs x9, elr_el1
                str x9, [sp, #264]
                mrs x9, spsr_el1
                str x9, [sp, #272]
                mrs x9, esr_el1
                str x9, [sp, #280]
                mrs x9, far_el1
                str x9, [sp, #288]
                mrs x9, ttbr0_el1
                str x9, [sp, #296]
                mrs x9, ttbr1_el1
                str x9, [sp, #304]
                mov x0, sp
                bl rustos_aarch64_userspace_boot_exception
                ldr x9, [sp, #256]
                msr sp_el0, x9
                ldr x9, [sp, #264]
                msr elr_el1, x9
                ldr x9, [sp, #272]
                msr spsr_el1, x9
                ldp x28, x29, [sp, #224]
                ldr x30, [sp, #240]
                ldp x26, x27, [sp, #208]
                ldp x24, x25, [sp, #192]
                ldp x22, x23, [sp, #176]
                ldp x20, x21, [sp, #160]
                ldp x18, x19, [sp, #144]
                ldp x16, x17, [sp, #128]
                ldp x14, x15, [sp, #112]
                ldp x12, x13, [sp, #96]
                ldp x10, x11, [sp, #80]
                ldp x8, x9, [sp, #64]
                ldp x6, x7, [sp, #48]
                ldp x4, x5, [sp, #32]
                ldp x2, x3, [sp, #16]
                ldp x0, x1, [sp, #0]
                add sp, sp, #320
                eret
                "#
            );

            pub fn build_address_space(
                mappings: &[UserMapping],
                elf: &[u8],
            ) -> Result<usize, String> {
                crate::serial_println!("aarch64-userspace: clone ttbr0");
                let ttbr0 = alloc_zeroed_page()?;
                let current = current_ttbr0();
                if current != 0 {
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            current as *const u8,
                            ttbr0 as *mut u8,
                            PAGE_SIZE,
                        );
                    }
                }

                crate::serial_println!("aarch64-userspace: install user mappings");
                for mapping in mappings {
                    match mapping.kind {
                        MappingKind::Load => map_load_mapping(ttbr0, mapping, elf)?,
                        MappingKind::Stack => map_zero_mapping(ttbr0, mapping)?,
                    }
                }
                crate::serial_println!("aarch64-userspace: address space ready");
                Ok(ttbr0)
            }

            fn map_load_mapping(
                ttbr0: usize,
                mapping: &UserMapping,
                elf: &[u8],
            ) -> Result<(), String> {
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
                    map_page(ttbr0, page_va, page_pa, pte_flags(mapping.permissions));

                    let page_start = page_va;
                    let page_end = page_va + PAGE_SIZE;
                    let copy_start_va = max(page_start, mapping.segment_va);
                    let copy_end_va = min(page_end, mapping.segment_va + mapping.file_size);
                    if copy_start_va < copy_end_va {
                        let src_off = file_start + (copy_start_va - mapping.segment_va);
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

            fn map_zero_mapping(ttbr0: usize, mapping: &UserMapping) -> Result<(), String> {
                for page_index in 0..mapping.page_count {
                    let page_va = mapping
                        .va_start
                        .checked_add(page_index * PAGE_SIZE)
                        .ok_or_else(|| String::from("user stack VA overflows"))?;
                    let page_pa = alloc_zeroed_page()?;
                    map_page(ttbr0, page_va, page_pa, pte_flags(mapping.permissions));
                }
                Ok(())
            }

            fn pte_flags(perms: UserPerms) -> usize {
                let ap = if perms.write { AP_RW_EL0 } else { AP_RO_EL0 };
                let mut flags = DESC_PAGE | ATTR_NORMAL | ap | AF | SH_INNER | PXN;
                if !perms.execute {
                    flags |= UXN;
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

            fn current_ttbr0() -> usize {
                let ttbr0: usize;
                unsafe {
                    asm!("mrs {ttbr0}, ttbr0_el1", ttbr0 = out(reg) ttbr0, options(nostack, nomem));
                }
                ttbr0 & ADDR_MASK
            }

            fn map_page(root: usize, va: usize, pa: usize, flags: usize) {
                unsafe {
                    let pte = walk_mut(root, va);
                    *pte = (pa & ADDR_MASK) | flags | DESC_VALID;
                    asm!("dsb ishst", "tlbi vaae1is, {va}", "dsb ish", "isb", va = in(reg) va >> 12, options(nostack));
                }
            }

            unsafe fn walk_mut(root: usize, va: usize) -> *mut usize {
                let idx = [
                    (va >> 39) & 0x1ff,
                    (va >> 30) & 0x1ff,
                    (va >> 21) & 0x1ff,
                    (va >> 12) & 0x1ff,
                ];
                let mut table = root;
                for level in 0..3 {
                    let slot = pte_ptr(table, idx[level]);
                    table = next_table(slot, level);
                }
                pte_ptr(table, idx[3])
            }

            unsafe fn next_table(entry: *mut usize, level: usize) -> usize {
                let desc = *entry;
                if desc & DESC_VALID == 0 {
                    let table =
                        alloc_zeroed_page().expect("userspace boot page-table pool exhausted");
                    *entry = (table & ADDR_MASK) | DESC_VALID | DESC_TABLE;
                    table
                } else if desc & DESC_TABLE == 0 {
                    if level == 0 {
                        let table =
                            alloc_zeroed_page().expect("userspace boot page-table pool exhausted");
                        *entry = (table & ADDR_MASK) | DESC_VALID | DESC_TABLE;
                        return table;
                    }
                    let split = split_block(desc, level);
                    *entry = (split & ADDR_MASK) | DESC_VALID | DESC_TABLE;
                    split
                } else {
                    let table = desc & ADDR_MASK;
                    if is_boot_pool_page(table) {
                        table
                    } else {
                        let cloned =
                            alloc_zeroed_page().expect("userspace boot page-table pool exhausted");
                        core::ptr::copy_nonoverlapping(
                            table as *const u8,
                            cloned as *mut u8,
                            PAGE_SIZE,
                        );
                        *entry = (cloned & ADDR_MASK) | (desc & !ADDR_MASK);
                        cloned
                    }
                }
            }

            unsafe fn split_block(desc: usize, level: usize) -> usize {
                let table = alloc_zeroed_page().expect("userspace boot page-table pool exhausted");
                let base = desc & ADDR_MASK;
                let flags = desc & !ADDR_MASK;
                let shift = match level {
                    1 => 21,
                    2 => 12,
                    _ => 30,
                };
                for index in 0..512usize {
                    let child = pte_ptr(table, index);
                    let child_pa = base + (index << shift);
                    *child = (child_pa & ADDR_MASK)
                        | flags
                        | DESC_VALID
                        | if level == 2 { DESC_PAGE } else { 0 };
                }
                table
            }

            fn is_boot_pool_page(addr: usize) -> bool {
                let start = core::ptr::addr_of!(PAGE_POOL) as usize;
                let end = start + BOOT_PAGE_POOL_PAGES * PAGE_SIZE;
                addr >= start && addr < end && addr & (PAGE_SIZE - 1) == 0
            }

            unsafe fn pte_ptr(table: usize, idx: usize) -> *mut usize {
                (table + idx * core::mem::size_of::<usize>()) as *mut usize
            }

            pub unsafe fn enter_el0(ttbr0: usize, entry: usize, user_sp: usize) -> ! {
                let vectors = core::ptr::addr_of!(__rustos_userspace_boot_vectors) as usize;
                asm!(
                    "msr vbar_el1, {vectors}",
                    "msr ttbr0_el1, {ttbr0}",
                    "dsb ish",
                    "isb",
                    "msr elr_el1, {entry}",
                    "msr spsr_el1, {spsr}",
                    "msr sp_el0, {sp}",
                    "eret",
                    vectors = in(reg) vectors,
                    ttbr0 = in(reg) ttbr0,
                    entry = in(reg) entry,
                    spsr = in(reg) SPSR_EL0T_DAIF_MASKED,
                    sp = in(reg) user_sp,
                    options(noreturn)
                )
            }

            #[no_mangle]
            extern "C" fn rustos_aarch64_userspace_boot_exception(frame: &mut Aarch64BootFrame) {
                let ec = (frame.esr_el1 >> EC_SHIFT) & EC_MASK;
                if ec == EC_SVC64 {
                    handle_svc(frame);
                } else {
                    dump_exception(frame);
                    loop {
                        unsafe {
                            asm!("wfi", options(nomem, nostack));
                        }
                    }
                }
            }

            fn handle_svc(frame: &mut Aarch64BootFrame) {
                let syscall = frame.x[8];
                let result = match syscall {
                    SYS_WRITE => sys_write(frame.x[0], frame.x[1] as *const u8, frame.x[2]),
                    SYS_BRK => INIT_BRK,
                    SYS_EXIT_GROUP => {
                        serial_write_str("userspace: PID 1 exit_group\n");
                        loop {
                            unsafe {
                                asm!("wfi", options(nomem, nostack));
                            }
                        }
                    },
                    _ => usize::MAX,
                };
                frame.x[0] = result;
                frame.elr_el1 = frame.elr_el1.wrapping_add(4);
            }

            fn dump_exception(frame: &Aarch64BootFrame) {
                serial_write_str("aarch64-userspace: synchronous exception\n");
                serial_hex_line("ESR_EL1", frame.esr_el1);
                serial_hex_line("ELR_EL1", frame.elr_el1);
                serial_hex_line("FAR_EL1", frame.far_el1);
                serial_hex_line("SPSR_EL1", frame.spsr_el1);
                serial_hex_line("CURRENT_SP", frame.current_sp);
                serial_hex_line("SP_EL0", frame.sp_el0);
                serial_hex_line("TTBR0_EL1", frame.ttbr0_el1);
                serial_hex_line("TTBR1_EL1", frame.ttbr1_el1);
            }

            fn serial_hex_line(label: &str, value: usize) {
                serial_write_str(label);
                serial_write_str("=0x");
                serial_write_hex(value);
                serial_write_str("\n");
            }

            fn serial_write_hex(value: usize) {
                for shift in (0..64).step_by(4).rev() {
                    let nibble = ((value >> shift) & 0xf) as u8;
                    let byte = if nibble < 10 {
                        b'0' + nibble
                    } else {
                        b'a' + (nibble - 10)
                    };
                    crate::arch::aarch64::serial::write_byte(byte);
                }
            }

            fn serial_write_str(value: &str) {
                for byte in value.as_bytes() {
                    crate::arch::aarch64::serial::write_byte(*byte);
                }
            }

            fn sys_write(fd: usize, ptr: *const u8, len: usize) -> usize {
                if fd != 1 && fd != 2 || ptr.is_null() {
                    return usize::MAX;
                }
                let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
                for byte in bytes {
                    crate::arch::aarch64::serial::write_byte(*byte);
                }
                len
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
