// openat2 and mincore ABI validation, included in syscall/mod.rs.

fn sys_openat2_impl(dirfd: i32, path: usize, how: usize, size: usize) -> isize {
    if size < 24 { return einval(); }
    if size > 4096 { return -7; }
    let mut bytes = [0u8; 24];
    if crate::uaccess::copy_from_user(&mut bytes, how).is_err() { return efault(); }
    for i in 24..size {
        let Some(ptr) = how.checked_add(i) else { return efault(); };
        let mut byte = [0u8; 1];
        if crate::uaccess::copy_from_user(&mut byte, ptr).is_err() { return efault(); }
        if byte[0] != 0 { return -7; }
    }
    let flags = u64::from_ne_bytes(bytes[..8].try_into().unwrap());
    let mode = u64::from_ne_bytes(bytes[8..16].try_into().unwrap());
    let resolve = u64::from_ne_bytes(bytes[16..].try_into().unwrap());
    if flags > u32::MAX as u64 || mode & !0o7777 != 0 || resolve & !0x3f != 0 ||
        resolve & 0x18 == 0x18 || (mode != 0 && flags & (0x40 | 0x400000) == 0) {
        return einval();
    }
    if resolve & 0x20 != 0 { return -11; }
    // The current VFS resolver has no locked path-walk restriction context.
    // Lexical checks alone cannot enforce BENEATH, IN_ROOT, NO_XDEV or symlink
    // restrictions against intermediate symlinks and concurrent renames.
    if resolve != 0 { return enotsup(); }
    sys_openat_impl(dirfd, path, flags as i32, mode as u32)
}

fn sys_mincore(addr: usize, length: usize, out: usize) -> isize {
    use crate::arch::api::Paging;
    const PAGE: usize = 4096;
    if addr & (PAGE - 1) != 0 { return einval(); }
    let Some(end) = addr.checked_add(length) else { return -12; };
    let Some(rounded) = length.checked_add(PAGE - 1) else { return -12; };
    let pages = rounded / PAGE;
    if pages == 0 { return 0; }
    if !crate::uaccess::validate_user_ptr(out, pages) { return efault(); }
    let pid = scheduler::current_pid();
    for i in 0..pages {
        let va = addr + i * PAGE;
        let present = match scheduler::with_proc(pid, |p| {
            let _guard = p.mm_lock.read();
            let mapped = p.vmas.iter().any(|v| v.start <= va && v.end >= (va + PAGE).min(end));
            if !mapped { return None; }
            Some(<crate::arch::Arch as Paging>::virt_to_phys(p.user_pagetable, va).is_some() as u8)
        }).flatten() { Some(p) => p, None => return -12 };
        if crate::uaccess::copy_to_user(out + i, &[present]).is_err() { return efault(); }
    }
    0
}
