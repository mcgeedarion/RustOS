// ABI adapters for filesystem, clock and socket operations.

pub(super) fn sys_openat_impl(dirfd: i32, path: usize, flags: i32, mode: u32) -> isize {
    crate::fs::io_syscalls::sys_openat(dirfd, path, flags as u32, mode)
}
pub(super) fn sys_creat_impl(path: usize, mode: u32) -> isize {
    crate::fs::io_syscalls::sys_open(path, 0x241, mode)
}
pub(super) fn sys_ftruncate_impl(fd: usize, len: i64) -> isize {
    crate::fs::io_syscalls::sys_ftruncate(fd, len)
}

fn timestamp_ns(sec: i64, sub: i64, scale: u64) -> Result<u64, isize> {
    if sec < 0 || sub < 0 || sub as u64 >= 1_000_000_000 / scale { return Err(einval()); }
    (sec as u64).checked_mul(1_000_000_000)
        .and_then(|v| v.checked_add(sub as u64 * scale)).ok_or_else(einval)
}

fn apply_file_times(path: &str, times: usize, timeval: bool) -> isize {
    let now = crate::time::clock::clock_gettime(0).map(|ts| ts.to_ns()).unwrap_or(0);
    let mut values = [now; 2];
    if times != 0 {
        let mut bytes = [0u8; 32];
        if copy_from_user(&mut bytes, times).is_err() { return efault(); }
        let old = match crate::fs::vfs_ops::get_times(path) { Ok(t) => t, Err(e) => return e };
        for i in 0..2 {
            let sec = i64::from_ne_bytes(bytes[i * 16..i * 16 + 8].try_into().unwrap());
            let sub = i64::from_ne_bytes(bytes[i * 16 + 8..i * 16 + 16].try_into().unwrap());
            values[i] = if !timeval && sub == 1_073_741_823 { now }
                else if !timeval && sub == 1_073_741_822 { if i == 0 { old.0 } else { old.1 } }
                else { match timestamp_ns(sec, sub, if timeval { 1000 } else { 1 }) { Ok(v) => v, Err(e) => return e } };
        }
    }
    crate::fs::vfs_ops::utimens(path, values[0], values[1]).map_or_else(|e| e, |_| 0)
}

pub(super) fn sys_utimensat_impl(dirfd: i32, path_va: usize, times: usize, flags: i32) -> isize {
    if flags & !0x100 != 0 { return einval(); }
    let path = if path_va == 0 {
        match crate::fs::process_fd::proc_fd_path(scheduler::current_pid(), dirfd as usize) {
            Some(p) => p, None => return ebadf(),
        }
    } else { match resolve_at_path_for_stubs(dirfd, path_va) { Ok(p) => p, Err(e) => return e } };
    if flags & 0x100 != 0 {
        match crate::fs::vfs_ops::lstat(&path) {
            Ok(s) if s.mode & 0o170000 == 0o120000 => return enotsup(),
            Ok(_) => {}, Err(e) => return e,
        }
    }
    apply_file_times(&path, times, false)
}

pub(super) fn sys_futimesat_impl(dirfd: i32, path: usize, times: usize) -> isize {
    let path = match resolve_at_path_for_stubs(dirfd, path) { Ok(p) => p, Err(e) => return e };
    apply_file_times(&path, times, true)
}
pub(super) fn sys_utimes_impl(path: usize, times: usize) -> isize {
    sys_futimesat_impl(-100, path, times)
}
pub(super) fn sys_utime_impl(path: usize, times: usize) -> isize {
    let path = match resolve_at_path_for_stubs(-100, path) { Ok(p) => p, Err(e) => return e };
    if times == 0 { return apply_file_times(&path, 0, false); }
    let mut bytes = [0u8; 16];
    if copy_from_user(&mut bytes, times).is_err() { return efault(); }
    let atime = match timestamp_ns(i64::from_ne_bytes(bytes[..8].try_into().unwrap()), 0, 1) { Ok(t) => t, Err(e) => return e };
    let mtime = match timestamp_ns(i64::from_ne_bytes(bytes[8..].try_into().unwrap()), 0, 1) { Ok(t) => t, Err(e) => return e };
    crate::fs::vfs_ops::utimens(&path, atime, mtime).map_or_else(|e| e, |_| 0)
}

pub(super) fn sys_statx_impl(dirfd: i32, path_va: usize, flags: u32, mask: u32, out: usize) -> isize {
    if flags & !(0x100 | 0x800 | 0x1000 | 0x6000) != 0 || flags & 0x6000 == 0x6000 || mask & 0x80000000 != 0 {
        return einval();
    }
    let path = match crate::proc::exec::read_cstr_safe(path_va) {
        Some(p) if p.is_empty() && flags & 0x1000 != 0 => {
            match crate::fs::process_fd::proc_fd_path(scheduler::current_pid(), dirfd as usize) {
                Some(p) => p, None => return ebadf(),
            }
        },
        Some(p) if p.is_empty() => return -2,
        Some(_) => match resolve_at_path_for_stubs(dirfd, path_va) { Ok(p) => p, Err(e) => return e },
        None => return efault(),
    };
    let stat = match if flags & 0x100 != 0 { crate::fs::vfs_ops::lstat(&path) } else { crate::fs::vfs_ops::stat(&path) } {
        Ok(s) => s, Err(e) => return e,
    };
    let mut bytes = [0u8; 256];
    bytes[..4].copy_from_slice(&0x7ffu32.to_ne_bytes());
    bytes[4..8].copy_from_slice(&(stat.blksize as u32).to_ne_bytes());
    bytes[16..20].copy_from_slice(&stat.nlink.to_ne_bytes());
    bytes[20..24].copy_from_slice(&stat.uid.to_ne_bytes());
    bytes[24..28].copy_from_slice(&stat.gid.to_ne_bytes());
    bytes[28..30].copy_from_slice(&stat.mode.to_ne_bytes());
    bytes[32..40].copy_from_slice(&stat.ino.to_ne_bytes());
    bytes[40..48].copy_from_slice(&stat.size.to_ne_bytes());
    bytes[48..56].copy_from_slice(&stat.blocks.to_ne_bytes());
    // KStat timestamps are seconds. statx timestamps carry seconds+nanos.
    for (offset, value) in [(64, stat.atime), (96, stat.ctime), (112, stat.mtime)] {
        bytes[offset..offset + 8].copy_from_slice(&value.to_ne_bytes());
    }
    if copy_to_user(out, &bytes).is_err() { return efault(); }
    0
}

pub(super) fn sys_mount_impl(source: usize, target: usize, kind: usize, flags: u64, data: usize) -> isize {
    let source = if source == 0 { String::new() } else { match read_user_str(source, 4096) { Ok(s) => s, Err(_) => return efault() } };
    let target = match resolve_at_path_for_stubs(-100, target) { Ok(s) => s, Err(e) => return e };
    let kind = if kind == 0 { String::new() } else { match read_user_str(kind, 64) { Ok(s) => s, Err(_) => return efault() } };
    let data = if data == 0 { String::new() } else { match read_user_str(data, 4096) { Ok(s) => s, Err(_) => return efault() } };
    crate::fs::mount::sys_mount(&source, &target, &kind, flags, &data)
}
pub(super) fn sys_umount2_impl(target: usize, flags: i32) -> isize {
    let target = match resolve_at_path_for_stubs(-100, target) { Ok(s) => s, Err(e) => return e };
    crate::fs::mount::sys_umount2(&target, flags as u32)
}

pub(super) fn sys_execveat_impl(dirfd: i32, path_va: usize, argv: usize, envp: usize, flags: i32) -> isize {
    if flags & !(0x100 | 0x1000) != 0 { return einval(); }
    let path = match crate::proc::exec::read_cstr_safe(path_va) {
        Some(p) if p.is_empty() && flags & 0x1000 != 0 => {
            match crate::fs::process_fd::proc_fd_path(scheduler::current_pid(), dirfd as usize) {
                Some(p) => p, None => return ebadf(),
            }
        },
        Some(p) if p.is_empty() => return -2,
        Some(_) => match resolve_at_path_for_stubs(dirfd, path_va) { Ok(p) => p, Err(e) => return e },
        None => return efault(),
    };
    if flags & 0x100 != 0 {
        match crate::fs::vfs_ops::lstat(&path) {
            Ok(s) if s.mode & 0o170000 == 0o120000 => return -40,
            Ok(_) => {}, Err(e) => return e,
        }
    }
    crate::proc::exec::sys_execve_path(path, argv, envp)
}

fn vectored_at(fd: usize, iov: usize, count: usize, low: usize, high: usize, flags: i32, write: bool) -> isize {
    if flags != 0 { return enotsup(); }
    if count > 1024 { return einval(); }
    let mut offset = ((low as u64) | ((high as u64) << 32)) as i64;
    if offset == -1 {
        return if write { crate::fs::io_syscalls::sys_writev(fd, iov, count) } else { crate::fs::io_syscalls::sys_readv(fd, iov, count) };
    }
    if offset < 0 { return einval(); }
    let mut total = 0isize;
    for i in 0..count {
        let Some(ptr) = iov.checked_add(i * 16) else { return if total > 0 { total } else { efault() }; };
        let mut bytes = [0u8; 16];
        if copy_from_user(&mut bytes, ptr).is_err() { return if total > 0 { total } else { efault() }; }
        let base = usize::from_ne_bytes(bytes[..8].try_into().unwrap());
        let len = usize::from_ne_bytes(bytes[8..].try_into().unwrap());
        if len > isize::MAX as usize - total as usize { return if total > 0 { total } else { einval() }; }
        let n = if write { crate::fs::io_syscalls::sys_pwrite64(fd, base, len, offset) } else { crate::fs::io_syscalls::sys_pread64(fd, base, len, offset) };
        if n < 0 { return if total > 0 { total } else { n }; }
        total += n;
        if n as usize != len { break; }
        let Some(next) = offset.checked_add(n as i64) else { break; };
        offset = next;
    }
    total
}
pub(super) fn sys_preadv2_impl(fd: usize, iov: usize, count: usize, low: usize, high: usize, flags: i32) -> isize {
    vectored_at(fd, iov, count, low, high, flags, false)
}
pub(super) fn sys_pwritev2_impl(fd: usize, iov: usize, count: usize, low: usize, high: usize, flags: i32) -> isize {
    vectored_at(fd, iov, count, low, high, flags, true)
}
pub(super) fn sys_sendfile_impl(out: usize, input: usize, offset: usize, count: usize) -> isize {
    crate::fs::io_syscalls::sys_copy_file_range(input, offset, out, 0, count.min(0x7ffff000))
}

fn message_batch(fd: usize, msgs: usize, count: u32, flags: u32, receive: bool) -> isize {
    if count == 0 { return 0; }
    if count > 1024 { return einval(); }
    for i in 0..count as usize {
        let Some(ptr) = msgs.checked_add(i * 64) else { return if i > 0 { i as isize } else { efault() }; };
        let n = if receive { crate::net::socket::sys_recvmsg(fd, ptr, flags) } else { crate::net::socket::sys_sendmsg(fd, ptr, flags) };
        if n < 0 { return if i > 0 { i as isize } else { n }; }
        if copy_to_user(ptr + 56, &(n as u32).to_ne_bytes()).is_err() {
            return if i > 0 { i as isize } else { efault() };
        }
    }
    count as isize
}
pub(super) fn sys_sendmmsg_impl(fd: usize, msgs: usize, count: u32, flags: u32) -> isize {
    message_batch(fd, msgs, count, flags, false)
}
pub(super) fn sys_recvmmsg_impl(fd: usize, msgs: usize, count: u32, flags: u32, timeout: usize) -> isize {
    if timeout != 0 { return enotsup(); }
    message_batch(fd, msgs, count, flags, true)
}

pub(super) fn sys_time_impl(out: usize) -> isize {
    let sec = crate::time::clock::time_secs();
    if out != 0 && copy_to_user(out, &sec.to_ne_bytes()).is_err() { return efault(); }
    sec as isize
}
pub(super) fn sys_gettimeofday_impl(tv: usize, tz: usize) -> isize {
    if tv != 0 {
        let now = crate::time::clock::gettimeofday();
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&now.tv_sec.to_ne_bytes());
        bytes[8..].copy_from_slice(&(now.tv_usec as i64).to_ne_bytes());
        if copy_to_user(tv, &bytes).is_err() { return efault(); }
    }
    if tz != 0 && copy_to_user(tz, &[0u8; 8]).is_err() { return efault(); }
    0
}
fn can_set_time() -> bool {
    scheduler::with_proc(scheduler::current_pid(), |p| p.caps.has(crate::security::cap::SYS_TIME)).unwrap_or(false)
}
pub(super) fn sys_clock_settime_impl(clock: u32, ptr: usize) -> isize {
    if !can_set_time() { return -1; }
    let mut bytes = [0u8; 16];
    if copy_from_user(&mut bytes, ptr).is_err() { return efault(); }
    let sec = i64::from_ne_bytes(bytes[..8].try_into().unwrap());
    let nsec = i64::from_ne_bytes(bytes[8..].try_into().unwrap());
    let ns = match timestamp_ns(sec, nsec, 1) { Ok(n) if n <= i64::MAX as u64 => n, _ => return einval() };
    crate::time::clock::clock_settime(clock as i32, crate::time::Timespec::from_ns(ns)).map_or_else(|e| e, |_| 0)
}
pub(super) fn sys_settimeofday_impl(tv: usize, tz: usize) -> isize {
    if !can_set_time() { return -1; }
    if tz != 0 { return enotsup(); }
    if tv == 0 { return 0; }
    let mut bytes = [0u8; 16];
    if copy_from_user(&mut bytes, tv).is_err() { return efault(); }
    let sec = i64::from_ne_bytes(bytes[..8].try_into().unwrap());
    let usec = i64::from_ne_bytes(bytes[8..].try_into().unwrap());
    let ns = match timestamp_ns(sec, usec, 1000) { Ok(n) if n <= i64::MAX as u64 => n, _ => return einval() };
    crate::time::clock::clock_settime(0, crate::time::Timespec::from_ns(ns)).map_or_else(|e| e, |_| 0)
}

pub(super) fn sys_getrandom_impl(ptr: usize, len: usize, flags: u32) -> isize {
    if flags & !7 != 0 || flags & 6 == 6 { return einval(); }
    let count = len.min(0x7ffff000);
    if count == 0 { return 0; }
    #[cfg(target_arch = "x86_64")]
    {
        if unsafe { core::arch::x86_64::__cpuid(1).ecx } & (1 << 30) == 0 { return -11; }
        let mut done = 0usize;
        while done < count {
            let mut value = 0u64;
            let mut ready = false;
            for _ in 0..10 {
                if unsafe { core::arch::x86_64::_rdrand64_step(&mut value) } == 1 { ready = true; break; }
            }
            if !ready { return if done > 0 { done as isize } else { -11 }; }
            let n = (count - done).min(8);
            let Some(dst) = ptr.checked_add(done) else { return if done > 0 { done as isize } else { efault() }; };
            if copy_to_user(dst, &value.to_ne_bytes()[..n]).is_err() { return if done > 0 { done as isize } else { efault() }; }
            done += n;
        }
        done as isize
    }
    #[cfg(not(target_arch = "x86_64"))]
    { let _ = ptr; -11 }
}

pub(super) fn sys_reboot_impl(magic: i32, magic2: i32, cmd: u32, _arg: usize) -> isize {
    if magic as u32 != 0xfee1dead || ![672274793u32, 85072278, 369367448, 537993216].contains(&(magic2 as u32)) { return einval(); }
    if !scheduler::with_proc(scheduler::current_pid(), |p| p.caps.has(crate::security::cap::SYS_BOOT)).unwrap_or(false) { return -1; }
    match cmd {
        0x01234567 => crate::firmware::acpi::power::reboot(),
        0x4321fedc => crate::firmware::acpi::power::shutdown(),
        _ => return enotsup(),
    }
    // Firmware returning means the requested reset did not occur.
    -5
}

// These operations lack the required backing subsystem; explicit errors allow
// callers to fall back without claiming that privileged state was changed.
pub(super) fn sys_ustat_impl(_dev: u64, _out: usize) -> isize { enotsup() }
pub(super) fn sys_acct_impl(_path: usize) -> isize { enotsup() }
pub(super) fn sys_swapon_impl(_path: usize, _flags: i32) -> isize { enotsup() }
pub(super) fn sys_pivot_root_impl(_new: usize, _old: usize) -> isize { enotsup() }
pub(super) fn sys_syslog_impl(_action: i32, _buf: usize, _len: i32) -> isize { enotsup() }
pub(super) fn sys_memfd_create_impl(_name: usize, _flags: u32) -> isize { enotsup() }
pub(super) fn sys_futex_waitv_impl(_waiters: usize, count: u32, flags: u32, _timeout: usize, clock: u64) -> isize {
    if count == 0 || count > 128 || flags != 0 || clock > 1 { return einval(); }
    enotsup()
}
