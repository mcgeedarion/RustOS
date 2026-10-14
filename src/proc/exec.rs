extern crate alloc;

use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};

use crate::arch::{
    api::{PageFlags, Paging},
    Arch,
};
use crate::proc::{
    context::UserContext,
    process::{Pcb, State},
    proc_table,
    scheduler,
};

const PAGE_SIZE: usize = 4096;
const MAX_CSTR_LEN: usize = 4096;
const MAX_EXEC_STRINGS: usize = 4096;
const MAX_EXEC_BYTES: usize = 128 * 1024;
const MAX_EXEC_IMAGE_BYTES: usize = 256 * 1024 * 1024;
const MAX_EXEC_PAGES: usize = MAX_EXEC_IMAGE_BYTES / PAGE_SIZE;
const STACK_BYTES: usize = 256 * 1024;

const EFAULT: isize = -14;
const ENOENT: isize = -2;
const ENOMEM: isize = -12;
const E2BIG: isize = -7;
const ENOEXEC: isize = -8;
const ESRCH: isize = -3;
const ENOSYS: isize = -38;

const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const AT_NULL: usize = 0;
const AT_PHDR: usize = 3;
const AT_PHENT: usize = 4;
const AT_PHNUM: usize = 5;
const AT_PAGESZ: usize = 6;
const AT_ENTRY: usize = 9;

#[cfg(target_arch = "x86_64")]
const USER_STACK_TOP: usize = 0x0000_7fff_ffff_f000;
#[cfg(target_arch = "aarch64")]
const USER_STACK_TOP: usize = 0x0000_ffff_ffff_f000;
#[cfg(target_arch = "riscv64")]
const USER_STACK_TOP: usize = 0x0000_003f_ffff_f000;

#[derive(Clone, Debug)]
struct ExecPage {
    va: usize,
    writable: bool,
    executable: bool,
    pa: Option<usize>,
}

struct PreparedImage {
    root: usize,
    entry: usize,
    brk_base: usize,
    phdr_va: usize,
    phentsize: usize,
    phnum: usize,
    vmas: Vec<crate::mm::mmap::Vma>,
}

/// Copy a bounded NUL-terminated userspace byte string into kernel-owned memory.
/// This byte-oriented helper does not require argv/envp to be valid UTF-8.
fn read_cstr_bytes_safe(addr: usize) -> Option<Vec<u8>> {
    if addr == 0 {
        return None;
    }

    let mut bytes = Vec::new();
    for offset in 0..MAX_CSTR_LEN {
        let current = addr.checked_add(offset)?;
        let mut byte = 0u8;
        if crate::kernel::uaccess::copy_from_user(
            &mut byte as *mut u8,
            current,
            1,
        )
        .is_err()
        {
            return None;
        }
        if byte == 0 {
            return Some(bytes);
        }
        bytes.push(byte);
    }

    None
}

/// Copy a path into a UTF-8 kernel-owned string. Paths are interpreted as UTF-8
/// by the VFS; argv and envp values are copied as bytes instead.
pub fn read_cstr_safe(addr: usize) -> Option<String> {
    String::from_utf8(read_cstr_bytes_safe(addr)?).ok()
}

fn read_cstr_vector_safe(
    addr: usize,
    remaining_bytes: &mut usize,
) -> Result<Vec<Vec<u8>>, isize> {
    if addr == 0 {
        return Ok(Vec::new());
    }

    let pointer_size = core::mem::size_of::<usize>();
    let mut strings = Vec::new();
    for index in 0..MAX_EXEC_STRINGS {
        let offset = index.checked_mul(pointer_size).ok_or(EFAULT)?;
        let pointer_addr = addr.checked_add(offset).ok_or(EFAULT)?;
        let mut raw = [0u8; core::mem::size_of::<usize>()];
        if crate::kernel::uaccess::copy_from_user(
            raw.as_mut_ptr(),
            pointer_addr,
            pointer_size,
        )
        .is_err()
        {
            return Err(EFAULT);
        }

        let string_addr = usize::from_ne_bytes(raw);
        if string_addr == 0 {
            return Ok(strings);
        }

        let value = read_cstr_bytes_safe(string_addr).ok_or(EFAULT)?;
        let cost = value.len().checked_add(1).ok_or(E2BIG)?;
        if cost > *remaining_bytes {
            return Err(E2BIG);
        }
        *remaining_bytes -= cost;
        strings.push(value);
    }

    Err(E2BIG)
}

fn align_up(value: usize, alignment: usize) -> Option<usize> {
    value.checked_add(alignment - 1).map(|v| v & !(alignment - 1))
}

fn cleanup_allocated_pages(root: usize, pages: &[ExecPage]) {
    <Arch as Paging>::free_page_table(root);
    for page in pages {
        if let Some(pa) = page.pa {
            crate::mm::pmm::free_page(pa);
        }
    }
}

/// Copy each mapped image VMA out of a user root, then release its root page
/// table. This also handles an old image retained until exec has switched stacks.
fn destroy_address_space(root: usize, vmas: &[crate::mm::mmap::Vma]) {
    if root == 0 {
        return;
    }

    for vma in vmas {
        if matches!(&vma.kind, crate::mm::mmap::VmaKind::PhysMap(_)) {
            continue;
        }
        for va in (vma.start..vma.end).step_by(PAGE_SIZE) {
            if let Some(pa) = <Arch as Paging>::virt_to_phys(root, va) {
                if <Arch as Paging>::unmap_page(root, va).is_some() {
                    crate::mm::pmm::free_page(pa & !(PAGE_SIZE - 1));
                }
            }
        }
    }

    <Arch as Paging>::free_page_table(root);
}

struct DeferredImage {
    root: usize,
    vmas: Vec<crate::mm::mmap::Vma>,
}

// The old image cannot be reclaimed inside execve itself because Rust and the
// architecture syscall stub are still executing on the old userspace stack.
// The first syscall from the new image reaps these now-inactive mappings.
static DEFERRED_EXEC_IMAGES: spin::Mutex<Vec<DeferredImage>> = spin::Mutex::new(Vec::new());

fn defer_old_image(root: usize, vmas: Vec<crate::mm::mmap::Vma>) {
    if root != 0 {
        DEFERRED_EXEC_IMAGES.lock().push(DeferredImage { root, vmas });
    }
}

pub fn reap_deferred_exec_images() {
    let pending = {
        let mut guard = DEFERRED_EXEC_IMAGES.lock();
        core::mem::take(&mut *guard)
    };
    for image in pending {
        destroy_address_space(image.root, &image.vmas);
    }
}

fn vmas_from_pages(pages: &[ExecPage]) -> Vec<crate::mm::mmap::Vma> {
    use crate::mm::mmap::{Vma, VmaKind, MAP_PRIVATE, PROT_EXEC, PROT_READ, PROT_WRITE};

    let mut vmas = Vec::new();
    let Some(first) = pages.first() else {
        return vmas;
    };

    let mut start = first.va;
    let mut end = first.va + PAGE_SIZE;
    let mut writable = first.writable;
    let mut executable = first.executable;

    for page in &pages[1..] {
        if page.va == end && page.writable == writable && page.executable == executable {
            end += PAGE_SIZE;
            continue;
        }

        let mut prot = PROT_READ;
        if writable { prot |= PROT_WRITE; }
        if executable { prot |= PROT_EXEC; }
        vmas.push(Vma {
            start,
            end,
            prot,
            flags: MAP_PRIVATE,
            kind: VmaKind::Anonymous,
            file_offset: 0,
            locked: false,
        });

        start = page.va;
        end = page.va + PAGE_SIZE;
        writable = page.writable;
        executable = page.executable;
    }

    let mut prot = PROT_READ;
    if writable { prot |= PROT_WRITE; }
    if executable { prot |= PROT_EXEC; }
    vmas.push(Vma {
        start,
        end,
        prot,
        flags: MAP_PRIVATE,
        kind: VmaKind::Anonymous,
        file_offset: 0,
        locked: false,
    });

    vmas
}

/// Validate, allocate, map and populate a static ELF image in a fresh user
/// address space. Every frame starts zeroed, so bytes from p_filesz to p_memsz
/// (including BSS) remain zero. Overlapping segment pages share a single frame
/// and permissions are merged before any mapping is installed.
fn build_elf_image(data: &[u8]) -> Result<PreparedImage, isize> {
    let entry = crate::exec::elf::validate_executable_elf(data).map_err(|_| ENOEXEC)? as usize;
    let header = crate::exec::elf::parse_elf_header(data).map_err(|_| ENOEXEC)?;
    let phdrs = crate::exec::elf::parse_phdrs_with_hdr(data, &header).ok_or(ENOEXEC)?;

    let phoff = { header.e_phoff };
    let phentsize = { header.e_phentsize } as usize;
    let phnum = { header.e_phnum } as usize;
    let mut phdr_va = 0usize;
    let mut brk_base = 0usize;
    let mut total_mem = 0usize;
    let mut pages: Vec<ExecPage> = Vec::new();

    for ph in &phdrs {
        let p_type = { ph.p_type };
        if p_type != PT_LOAD {
            continue;
        }
        let p_flags = { ph.p_flags };
        let p_offset = { ph.p_offset };
        let p_vaddr = { ph.p_vaddr };
        let p_filesz = { ph.p_filesz };
        let p_memsz = { ph.p_memsz };

        if p_memsz == 0 {
            continue;
        }
        let memsz = usize::try_from(p_memsz).map_err(|_| ENOEXEC)?;
        total_mem = total_mem.checked_add(memsz).ok_or(E2BIG)?;
        if total_mem > MAX_EXEC_IMAGE_BYTES {
            return Err(E2BIG);
        }

        let va = usize::try_from(p_vaddr).map_err(|_| ENOEXEC)?;
        let seg_end = va.checked_add(memsz).ok_or(ENOEXEC)?;
        let start = va & !(PAGE_SIZE - 1);
        let end = align_up(seg_end, PAGE_SIZE).ok_or(ENOEXEC)?;
        brk_base = brk_base.max(end);

        let offset = usize::try_from(p_offset).map_err(|_| ENOEXEC)?;
        let filesz = usize::try_from(p_filesz).map_err(|_| ENOEXEC)?;
        let file_end = offset.checked_add(filesz).ok_or(ENOEXEC)?;
        if file_end > data.len() {
            return Err(ENOEXEC);
        }

        let phdr_table_bytes = phnum.checked_mul(phentsize).ok_or(ENOEXEC)?;
        let phdr_table_bytes_u64 = u64::try_from(phdr_table_bytes).map_err(|_| ENOEXEC)?;
        let phdr_table_end = phoff.checked_add(phdr_table_bytes_u64);
        let segment_file_end = p_offset.checked_add(p_filesz);
        if phoff >= p_offset
            && phdr_table_end.is_some()
            && segment_file_end.is_some()
            && phdr_table_end.unwrap_or(u64::MAX) <= segment_file_end.unwrap_or(0)
        {
            let delta = usize::try_from(phoff - p_offset).map_err(|_| ENOEXEC)?;
            phdr_va = va.checked_add(delta).ok_or(ENOEXEC)?;
        }

        for page_va in (start..end).step_by(PAGE_SIZE) {
            pages.push(ExecPage {
                va: page_va,
                writable: p_flags & PF_W != 0,
                executable: p_flags & PF_X != 0,
                pa: None,
            });
            if pages.len() > MAX_EXEC_PAGES {
                return Err(E2BIG);
            }
        }
    }

    if pages.is_empty() || brk_base == 0 {
        return Err(ENOEXEC);
    }

    pages.sort_unstable_by_key(|page| page.va);
    let mut merged: Vec<ExecPage> = Vec::with_capacity(pages.len());
    for page in pages {
        if let Some(last) = merged.last_mut() {
            if last.va == page.va {
                last.writable |= page.writable;
                last.executable |= page.executable;
                continue;
            }
        }
        merged.push(page);
    }

    for page in &merged {
        if page.writable && page.executable {
            return Err(ENOEXEC);
        }
    }

    let root = <Arch as Paging>::new_user_address_space().ok_or(ENOMEM)?;
    for index in 0..merged.len() {
        let pa = match crate::mm::pmm::alloc_page() {
            Some(pa) => pa,
            None => {
                cleanup_allocated_pages(root, &merged);
                return Err(ENOMEM);
            }
        };
        unsafe { core::ptr::write_bytes(pa as *mut u8, 0, PAGE_SIZE); }
        merged[index].pa = Some(pa);
    }

    for page in &merged {
        let pa = page.pa.ok_or(ENOMEM)?;
        let mut flags = PageFlags::PRESENT | PageFlags::USER;
        if page.writable {
            flags |= PageFlags::WRITE;
        }
        if !page.executable {
            flags |= PageFlags::NX;
        }
        if !<Arch as Paging>::map_page(root, page.va, pa, flags) {
            cleanup_allocated_pages(root, &merged);
            return Err(ENOMEM);
        }
    }

    // Map exact file bytes into the pages. Since every frame was zeroed first,
    // all untouched portions and the remainder of p_memsz form a zero-filled BSS.
    for ph in &phdrs {
        if { ph.p_type } != PT_LOAD {
            continue;
        }
        let p_offset = { ph.p_offset } as usize;
        let p_vaddr = { ph.p_vaddr } as usize;
        let p_filesz = { ph.p_filesz } as usize;
        let mut copied = 0usize;
        while copied < p_filesz {
            let va = p_vaddr.checked_add(copied).ok_or(ENOEXEC)?;
            let page_va = va & !(PAGE_SIZE - 1);
            let page = match merged.binary_search_by_key(&page_va, |page| page.va) {
                Ok(index) => &merged[index],
                Err(_) => {
                    cleanup_allocated_pages(root, &merged);
                    return Err(ENOEXEC);
                }
            };
            let pa = match page.pa {
                Some(pa) => pa,
                None => {
                    cleanup_allocated_pages(root, &merged);
                    return Err(ENOMEM);
                }
            };
            let page_offset = va & (PAGE_SIZE - 1);
            let chunk = core::cmp::min(PAGE_SIZE - page_offset, p_filesz - copied);
            let file_offset = p_offset.checked_add(copied).ok_or(ENOEXEC)?;
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr().add(file_offset),
                    (pa + page_offset) as *mut u8,
                    chunk,
                );
            }
            copied += chunk;
        }
    }

    Ok(PreparedImage {
        root,
        entry,
        brk_base,
        phdr_va,
        phentsize,
        phnum,
        vmas: vmas_from_pages(&merged),
    })
}

fn write_user_bytes(root: usize, mut va: usize, mut data: &[u8]) -> Result<(), isize> {
    while !data.is_empty() {
        let pa = <Arch as Paging>::virt_to_phys(root, va).ok_or(EFAULT)?;
        let page_offset = va & (PAGE_SIZE - 1);
        let count = core::cmp::min(PAGE_SIZE - page_offset, data.len());
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), pa as *mut u8, count);
        }
        va = va.checked_add(count).ok_or(EFAULT)?;
        data = &data[count..];
    }
    Ok(())
}

/// Build argc/argv/envp/auxv on a writable userspace stack. The stack is
/// 16-byte aligned at entry on all supported 64-bit ABIs.
fn build_initial_stack(
    image: &mut PreparedImage,
    argv: &[Vec<u8>],
    envp: &[Vec<u8>],
) -> Result<usize, isize> {
    use crate::mm::mmap::{Vma, VmaKind, MAP_ANON, MAP_GROWSDOWN, MAP_PRIVATE, PROT_READ, PROT_WRITE};

    let stack_bottom = USER_STACK_TOP.checked_sub(STACK_BYTES).ok_or(ENOMEM)?;
    for va in (stack_bottom..USER_STACK_TOP).step_by(PAGE_SIZE) {
        if <Arch as Paging>::virt_to_phys(image.root, va).is_some() {
            return Err(ENOEXEC);
        }
    }

    let bottom = crate::mm::mmap::alloc_user_stack(image.root, USER_STACK_TOP, STACK_BYTES)
        .map_err(|errno| errno as isize)?;
    image.vmas.push(Vma {
        start: bottom,
        end: USER_STACK_TOP,
        prot: PROT_READ | PROT_WRITE,
        flags: MAP_PRIVATE | MAP_ANON | MAP_GROWSDOWN,
        kind: VmaKind::Stack,
        file_offset: 0,
        locked: false,
    });

    let mut sp = USER_STACK_TOP;
    let mut argv_ptrs = Vec::with_capacity(argv.len());
    for arg in argv.iter().rev() {
        let needed = arg.len().checked_add(1).ok_or(E2BIG)?;
        sp = sp.checked_sub(needed).ok_or(E2BIG)?;
        if sp < bottom { return Err(E2BIG); }
        write_user_bytes(image.root, sp, arg)?;
        write_user_bytes(image.root, sp + arg.len(), &[0])?;
        argv_ptrs.push(sp);
    }
    argv_ptrs.reverse();

    let mut env_ptrs = Vec::with_capacity(envp.len());
    for env in envp.iter().rev() {
        let needed = env.len().checked_add(1).ok_or(E2BIG)?;
        sp = sp.checked_sub(needed).ok_or(E2BIG)?;
        if sp < bottom { return Err(E2BIG); }
        write_user_bytes(image.root, sp, env)?;
        write_user_bytes(image.root, sp + env.len(), &[0])?;
        env_ptrs.push(sp);
    }
    env_ptrs.reverse();

    let mut words = Vec::with_capacity(1 + argv.len() + 1 + envp.len() + 1 + 10);
    words.push(argv.len());
    words.extend(argv_ptrs);
    words.push(0);
    words.extend(env_ptrs);
    words.push(0);
    words.extend([AT_PAGESZ, PAGE_SIZE, AT_ENTRY, image.entry]);
    if image.phdr_va != 0 {
        words.extend([AT_PHDR, image.phdr_va, AT_PHENT, image.phentsize, AT_PHNUM, image.phnum]);
    }
    words.extend([AT_NULL, 0]);

    let table_bytes_len = words.len().checked_mul(core::mem::size_of::<usize>()).ok_or(E2BIG)?;
    let table_start = sp.checked_sub(table_bytes_len).ok_or(E2BIG)? & !15usize;
    if table_start < bottom {
        return Err(E2BIG);
    }

    let mut bytes = Vec::with_capacity(table_bytes_len);
    for word in words {
        bytes.extend_from_slice(&word.to_ne_bytes());
    }
    write_user_bytes(image.root, table_start, &bytes)?;
    Ok(table_start)
}

unsafe fn enter_user_context(root: usize, context: *const UserContext) -> ! {
    #[cfg(target_arch = "x86_64")]
    crate::arch::x86_64::uentry::sysret_to_user_context(root, context);
    #[cfg(target_arch = "aarch64")]
    crate::arch::aarch64::uentry::eret_to_user_context(root, context);
    #[cfg(target_arch = "riscv64")]
    crate::arch::riscv64::uentry::sret_to_user_context(root, context);
}

fn make_user_process(
    pid: usize,
    ppid: usize,
    path: &str,
    image: PreparedImage,
    user_sp: usize,
) -> Pcb {
    let mut pcb = Pcb::zeroed();
    pcb.pid = pid;
    pcb.ppid = ppid;
    pcb.tgid = pid;
    pcb.tg_id = pid;
    pcb.pgid = pid as u32;
    pcb.sid = pid as u32;
    pcb.state = State::Ready;
    pcb.exe_path = Some(path.to_string());
    pcb.user_pagetable = image.root;
    pcb.vmas = image.vmas;
    pcb.pc = image.entry;
    pcb.sp = user_sp;
    pcb.ctx.pc = image.entry;
    pcb.ctx.sp = user_sp;
    pcb.brk_base = image.brk_base;
    pcb.brk = image.brk_base;
    pcb.user_context = UserContext::zero();
    pcb.user_context.pc = image.entry;
    pcb.user_context.sp = user_sp;
    pcb
}

/// Spawn an ELF as a new schedulable userspace process. The image remains
/// separate from the kernel and other processes until it is fully prepared.
pub fn spawn_user_process(path: &str, argv: &[&str], envp: &[&str]) -> bool {
    let bytes = match crate::fs::vfs_ops::read_all(path) {
        Ok(bytes) => bytes,
        Err(errno) => {
            crate::serial_println!("exec: cannot read {} (errno={})", path, errno);
            return false;
        }
    };
    spawn_user_process_from_bytes(proc_table::next_pid() as usize, 0, path, &bytes, argv, envp)
}

/// Load and enqueue a process from in-memory ELF bytes. The caller supplies the
/// PID so boot and fork code can share the same allocator policy.
pub fn spawn_user_process_from_bytes(
    pid: usize,
    ppid: usize,
    path: &str,
    data: &[u8],
    argv: &[&str],
    envp: &[&str],
) -> bool {
    let mut image = match build_elf_image(data) {
        Ok(image) => image,
        Err(errno) => {
            crate::serial_println!("exec: invalid ELF {} (errno={})", path, errno);
            return false;
        }
    };
    let argv_bytes: Vec<Vec<u8>> = argv.iter().map(|s| s.as_bytes().to_vec()).collect();
    let env_bytes: Vec<Vec<u8>> = envp.iter().map(|s| s.as_bytes().to_vec()).collect();
    let user_sp = match build_initial_stack(&mut image, &argv_bytes, &env_bytes) {
        Ok(sp) => sp,
        Err(errno) => {
            destroy_address_space(image.root, &image.vmas);
            crate::serial_println!("exec: failed to create stack for {} (errno={})", path, errno);
            return false;
        }
    };

    let pcb = make_user_process(pid, ppid, path, image, user_sp);
    proc_table::enqueue(pcb);
    let task = scheduler::task_ptr_for_pid(pid);
    if task.is_null() {
        crate::serial_println!("exec: PID {} was not registered with the scheduler", pid);
        return false;
    }
    scheduler::enqueue_task(task);
    true
}

/// Compatibility entry point retained for code that already prevalidates an
/// ELF entry. The actual loader revalidates the image bytes and uses its parsed
/// entry rather than trusting the caller-supplied value.
pub fn spawn_user_process_from_bytes_full(
    pid: usize,
    path: &str,
    elf: &[u8],
    argv: &[&str],
    envp: &[&str],
    entry: u64,
) -> bool {
    match crate::exec::elf::validate_executable_elf(elf) {
        Ok(parsed_entry) if parsed_entry == entry => {
            spawn_user_process_from_bytes(pid, 0, path, elf, argv, envp)
        }
        _ => false,
    }
}

/// Replace the calling process image. All pathname, argv, and envp bytes are
/// copied before loading begins. Every validation or loading failure returns
/// a negative errno; success enters the new image without returning to caller.
pub fn sys_execve(path_va: usize, argv_va: usize, envp_va: usize) -> isize {
    let path = match read_cstr_safe(path_va) {
        Some(path) if !path.is_empty() => path,
        Some(_) => return ENOENT,
        None => return EFAULT,
    };
    let mut remaining = MAX_EXEC_BYTES;
    let argv = match read_cstr_vector_safe(argv_va, &mut remaining) {
        Ok(value) => value,
        Err(errno) => return errno,
    };
    let envp = match read_cstr_vector_safe(envp_va, &mut remaining) {
        Ok(value) => value,
        Err(errno) => return errno,
    };

    let pid = scheduler::current_pid() as usize;
    if pid == 0 {
        return ESRCH;
    }

    let cwd = scheduler::with_proc(pid, |pcb| pcb.cwd.clone()).unwrap_or_else(|| String::from("/"));
    let resolved_path = if path.starts_with('/') {
        path
    } else {
        alloc::format!("{}/{}", cwd.trim_end_matches('/'), path)
    };

    let bytes = match crate::fs::vfs_ops::read_all(&resolved_path) {
        Ok(bytes) => bytes,
        Err(errno) => return errno,
    };
    let mut image = match build_elf_image(&bytes) {
        Ok(image) => image,
        Err(errno) => return errno,
    };
    let user_sp = match build_initial_stack(&mut image, &argv, &envp) {
        Ok(sp) => sp,
        Err(errno) => {
            destroy_address_space(image.root, &image.vmas);
            return errno;
        }
    };

    let root = image.root;
    let entry = image.entry;
    let brk_base = image.brk_base;
    let new_vmas = image.vmas.clone();
    let mut new_context = UserContext::zero();
    new_context.pc = entry;
    new_context.sp = user_sp;

    let swapped = scheduler::with_proc_mut(pid, |pcb, _| {
        let _mm_guard = pcb.mm_lock.write();
        let old_root = core::mem::replace(&mut pcb.user_pagetable, root);
        let old_vmas = core::mem::replace(&mut pcb.vmas, new_vmas);
        pcb.pc = entry;
        pcb.sp = user_sp;
        pcb.ctx.pc = entry;
        pcb.ctx.sp = user_sp;
        pcb.brk_base = brk_base;
        pcb.brk = brk_base;
        pcb.exe_path = Some(resolved_path.clone());
        pcb.user_context = new_context;
        (old_root, old_vmas, core::ptr::addr_of!(pcb.user_context))
    });

    let (old_root, old_vmas, context_ptr) = match swapped {
        Some(value) => value,
        None => {
            destroy_address_space(root, &image.vmas);
            return ESRCH;
        }
    };
    defer_old_image(old_root, old_vmas);

    // From this point on the function must not return. The entry trampoline
    // abandons the old stack and does not access it after changing page tables.
    unsafe { enter_user_context(root, context_ptr) }
}

/// Called at the architecture dispatch boundary before handling a syscall.
/// It reclaims old images only after exec has transferred control to the new
/// stack/address space and the new process makes a subsequent system call.
pub fn reap_deferred_exec_images_at_syscall() {
    reap_deferred_exec_images();
}
