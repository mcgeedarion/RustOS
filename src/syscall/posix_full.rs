// POSIX process and descriptor adapters, included in syscall/mod.rs.

fn copy_bytes_to_user(dst: usize, bytes: &[u8]) -> bool {
    crate::uaccess::copy_to_user(dst, bytes).is_ok()
}

fn read_user_str(addr: usize, max_len: usize) -> Result<String, ()> {
    let mut buf = alloc::vec![0u8; max_len.saturating_add(1)];
    let len = crate::uaccess::strncpy_from_user(&mut buf, addr).map_err(|_| ())?;
    core::str::from_utf8(&buf[..len]).map(String::from).map_err(|_| ())
}

pub(super) fn sys_getrlimit_impl(resource: u32, out: usize) -> isize {
    sys_prlimit64_impl(0, resource, 0, out)
}

pub(super) fn sys_setrlimit_impl(resource: u32, new: usize) -> isize {
    if new == 0 { return efault(); }
    sys_prlimit64_impl(0, resource, new, 0)
}

pub(super) fn sys_prlimit64_impl(pid: u32, resource: u32, new: usize, out: usize) -> isize {
    if resource as usize >= crate::proc::rlimit::RLIMIT_NLIMITS { return einval(); }
    let caller = scheduler::current_pid();
    let target = if pid == 0 { caller } else { pid as usize };
    let Some((uid, privileged)) = scheduler::with_proc(caller, |p|
        (p.uid, p.caps.has(crate::security::cap::SYS_RESOURCE))) else { return esrch(); };
    let Some((old, allowed)) = scheduler::with_proc(target, |p|
        (p.rlimits.get(resource as usize), target == caller || privileged ||
            (uid == p.uid && uid == p.euid && uid == p.suid))) else { return esrch(); };
    if !allowed { return -1; }
    let proposed = if new != 0 {
        let mut bytes = [0u8; 16];
        if copy_from_user(&mut bytes, new).is_err() { return efault(); }
        let cur = u64::from_ne_bytes(bytes[..8].try_into().unwrap());
        let max = u64::from_ne_bytes(bytes[8..].try_into().unwrap());
        if cur > max { return einval(); }
        if max > old.1 && !privileged { return -1; }
        Some((cur, max))
    } else { None };
    if out != 0 {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&old.0.to_ne_bytes());
        bytes[8..].copy_from_slice(&old.1.to_ne_bytes());
        if copy_to_user(out, &bytes).is_err() { return efault(); }
    }
    if let Some((cur, max)) = proposed {
        return scheduler::with_proc_mut(target, |p, _| {
            if max > p.rlimits.get(resource as usize).1 && !privileged { return -1; }
            p.rlimits.set(resource as usize, cur, max)
        }).unwrap_or_else(esrch);
    }
    0
}

pub(super) fn sys_getrusage_impl(who: i32, out: usize) -> isize {
    crate::proc::rusage::sys_getrusage(who, out)
}

pub(super) fn sys_sysinfo_impl(out: usize) -> isize {
    let mut bytes = [0u8; 112];
    bytes[..8].copy_from_slice(&(crate::time::monotonic_ns() / 1_000_000_000).to_ne_bytes());
    bytes[32..40].copy_from_slice(&(crate::mm::pmm::total_pages() as u64 * 4096).to_ne_bytes());
    bytes[40..48].copy_from_slice(&(crate::mm::pmm::free_pages() as u64 * 4096).to_ne_bytes());
    let count = scheduler::with_procs_ro(|p| p.len().min(u16::MAX as usize) as u16);
    bytes[80..82].copy_from_slice(&count.to_ne_bytes());
    bytes[104..108].copy_from_slice(&1u32.to_ne_bytes());
    if copy_to_user(out, &bytes).is_err() { return efault(); }
    0
}

pub(super) fn sys_times_impl(out: usize) -> isize {
    if out != 0 {
        let Some((utime, stime)) = scheduler::with_proc(scheduler::current_pid(), |p|
            (p.utime_ns / 10_000_000, p.stime_ns / 10_000_000)) else { return esrch(); };
        let mut bytes = [0u8; 32];
        bytes[..8].copy_from_slice(&utime.to_ne_bytes());
        bytes[8..16].copy_from_slice(&stime.to_ne_bytes());
        if copy_to_user(out, &bytes).is_err() { return efault(); }
    }
    (crate::time::monotonic_ns() / 10_000_000) as isize
}

pub(super) fn sys_uname_impl(out: usize) -> isize {
    #[cfg(target_arch = "x86_64")] let machine = "x86_64";
    #[cfg(target_arch = "aarch64")] let machine = "aarch64";
    #[cfg(target_arch = "riscv64")] let machine = "riscv64";
    let mut bytes = [0u8; 390];
    for (i, name) in ["Linux", "rustos", "6.1.0-rustos", "#1 SMP", machine, "(none)"].iter().enumerate() {
        bytes[i * 65..i * 65 + name.len()].copy_from_slice(name.as_bytes());
    }
    if copy_to_user(out, &bytes).is_err() { return efault(); }
    0
}

pub(super) fn sys_prctl_impl(op: i32, a2: usize, a3: usize, a4: usize, a5: usize) -> isize {
    let pid = scheduler::current_pid();
    match op {
        15 => {
            let mut bytes = [0u8; 16];
            if copy_from_user(&mut bytes, a2).is_err() { return efault(); }
            let n = bytes.iter().position(|b| *b == 0).unwrap_or(15).min(15);
            let name = String::from_utf8_lossy(&bytes[..n]).into_owned();
            scheduler::with_proc_mut(pid, |p, _| p.name = name).map_or_else(esrch, |_| 0)
        },
        16 => {
            let Some(name) = scheduler::with_proc(pid, |p| p.name.clone()) else { return esrch(); };
            let mut bytes = [0u8; 16];
            let n = name.len().min(15);
            bytes[..n].copy_from_slice(&name.as_bytes()[..n]);
            if copy_to_user(a2, &bytes).is_err() { return efault(); }
            0
        },
        38 => {
            if a2 != 1 || a3 != 0 || a4 != 0 || a5 != 0 { return einval(); }
            scheduler::with_proc_mut(pid, |p, _| p.no_new_privs = true).map_or_else(esrch, |_| 0)
        },
        39 => {
            if a2 != 0 || a3 != 0 || a4 != 0 || a5 != 0 { return einval(); }
            scheduler::with_proc(pid, |p| p.no_new_privs as isize).unwrap_or_else(esrch)
        },
        // Dumpability, parent-death signals and ambient capabilities have no
        // state or enforcement in this kernel; never acknowledge them falsely.
        1..=4 | 21 | 22 | 47 => enotsup(),
        _ => einval(),
    }
}

pub(super) fn sys_arch_prctl_impl(code: i32, addr: usize) -> isize {
    if code == 0x1002 && addr >= 0x0000_8000_0000_0000 { return -1; }
    crate::proc::thread::sys_arch_prctl(code as usize, addr)
}

pub(super) fn sys_ioctl_impl(fd: usize, request: u64, arg: usize) -> isize {
    crate::fs::ioctl::sys_ioctl(fd, request as usize, arg)
}

pub(super) fn sys_iopl_impl(_level: i32) -> isize { -1 }
pub(super) fn sys_ioperm_impl(_from: usize, _num: usize, _on: i32) -> isize { -1 }
pub(super) fn sys_init_module_impl(_module: usize, _len: usize, _opts: usize) -> isize { enosys() }
pub(super) fn sys_delete_module_impl(_name: usize, _flags: u32) -> isize { enosys() }

pub(super) fn sys_fallocate_impl(fd: usize, mode: i32, offset: i64, len: i64) -> isize {
    if offset < 0 || len <= 0 || offset.checked_add(len).is_none() { return einval(); }
    // Extending length is not allocation: do not pretend truncate reserves
    // blocks or implements KEEP_SIZE/PUNCH_HOLE.
    if crate::fs::process_fd::proc_fd_get(scheduler::current_pid(), fd).is_none() { return ebadf(); }
    if mode & !0x7f != 0 { return einval(); }
    enotsup()
}

pub(super) fn sys_copy_file_range_impl(a: usize, b: usize, c: usize, d: usize, e: usize, flags: u32) -> isize {
    if flags != 0 { return einval(); }
    crate::fs::io_syscalls::sys_copy_file_range(a, b, c, d, e)
}

pub(super) fn sys_fsync_impl(fd: usize) -> isize {
    let bfd = crate::fs::process_fd::proc_fd_backing(scheduler::current_pid(), fd);
    if bfd < 0 { return bfd; }
    crate::fs::vfs::flush_fd(bfd as usize, true)
}
pub(super) fn sys_fdatasync_impl(fd: usize) -> isize {
    let bfd = crate::fs::process_fd::proc_fd_backing(scheduler::current_pid(), fd);
    if bfd < 0 { return bfd; }
    crate::fs::vfs::flush_fd(bfd as usize, false)
}
pub(super) fn sys_syncfs_impl(fd: usize) -> isize {
    if crate::fs::process_fd::proc_fd_get(scheduler::current_pid(), fd).is_none() { return ebadf(); }
    crate::fs::vfs::flush_all_dirty();
    0
}
pub(super) fn sys_sync_impl() -> isize {
    crate::fs::vfs::flush_all_dirty();
    0
}
pub(super) fn sys_personality_impl(persona: u32) -> isize {
    scheduler::with_proc_mut(scheduler::current_pid(), |p, _| {
        let old = p.personality;
        if persona == u32::MAX { return old as isize; }
        if persona != 0 { return einval(); }
        p.personality = persona;
        old as isize
    }).unwrap_or_else(esrch)
}

pub(super) fn sys_setpgid_impl(pid: usize, pgid: usize) -> isize { crate::proc::session::set_pgid(pid, pgid) }
pub(super) fn sys_getpgid_impl(pid: usize) -> isize { crate::proc::session::get_pgid(pid) }
pub(super) fn sys_setsid_impl() -> isize { crate::proc::session::setsid() }
pub(super) fn sys_getsid_impl(pid: usize) -> isize { crate::proc::session::get_sid(pid) }

fn capability_header(header: usize) -> Result<usize, isize> {
    let mut bytes = [0u8; 8];
    copy_from_user(&mut bytes, header).map_err(|_| efault())?;
    if u32::from_ne_bytes(bytes[..4].try_into().unwrap()) != 0x20080522 {
        copy_to_user(header, &0x20080522u32.to_ne_bytes()).map_err(|_| efault())?;
        return Err(einval());
    }
    let pid = i32::from_ne_bytes(bytes[4..].try_into().unwrap());
    if pid < 0 { return Err(einval()); }
    Ok(if pid == 0 { scheduler::current_pid() } else { pid as usize })
}

pub(super) fn sys_capget_impl(header: usize, data: usize) -> isize {
    let pid = match capability_header(header) { Ok(p) => p, Err(e) => return e };
    let Some(caps) = scheduler::with_proc(pid, |p| p.caps) else { return esrch(); };
    let mut bytes = [0u8; 24];
    for i in 0..2 {
        for (j, value) in [caps.effective, caps.permitted, caps.inheritable].iter().enumerate() {
            bytes[i * 12 + j * 4..i * 12 + j * 4 + 4].copy_from_slice(&((*value >> (i * 32)) as u32).to_ne_bytes());
        }
    }
    if copy_to_user(data, &bytes).is_err() { return efault(); }
    0
}

pub(super) fn sys_capset_impl(header: usize, data: usize) -> isize {
    let pid = match capability_header(header) { Ok(p) => p, Err(e) => return e };
    if pid != scheduler::current_pid() { return -1; }
    let mut bytes = [0u8; 24];
    if copy_from_user(&mut bytes, data).is_err() { return efault(); }
    let word = |i| u32::from_ne_bytes(bytes[i..i + 4].try_into().unwrap()) as u64;
    let caps = crate::security::CapSet {
        effective: word(0) | word(12) << 32,
        permitted: word(4) | word(16) << 32,
        inheritable: word(8) | word(20) << 32,
    };
    scheduler::with_proc_mut(pid, |p, _| {
        if caps.effective & !caps.permitted != 0 || caps.permitted & !p.caps.permitted != 0 ||
            caps.inheritable & !(p.caps.inheritable | p.caps.permitted) != 0 { return -1; }
        p.caps = caps;
        0
    }).unwrap_or_else(esrch)
}
