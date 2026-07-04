//! Subsystem dispatch routers.

use crate::syscall::dispatcher_context::SyscallContext;
use crate::syscall::errno::{efault, einval, emsgsize, enosys};
use crate::syscall::nr::*;

// Covers: basic I/O, stat/path ops, directory ops, *at variants, timerfd,
// inotify, fanotify, epoll/poll/select, io_uring, pipes, sendfile,
// eventfd, getdents, fcntl, ioctl, fallocate, copy_file_range, statx,
// sockets (NR 41-55, 288), pidfd.
pub fn dispatch_filesystem(ctx: &SyscallContext) -> Option<isize> {
    let (a, b, c, d, e, f) = (ctx.a0(), ctx.a1(), ctx.a2(), ctx.a3(), ctx.a4(), ctx.a5());
    match ctx.nr {
        SYS_READ => Some(crate::fs::io_syscalls::sys_read(a, b, c)),
        SYS_WRITE => Some(crate::fs::io_syscalls::sys_write(a, b, c)),
        SYS_OPEN => Some(crate::fs::io_syscalls::sys_open(a, b as u32, c as u32)),
        SYS_CLOSE => Some(crate::fs::io_syscalls::sys_close(a)),
        SYS_PREAD64 => Some(crate::fs::io_syscalls::sys_pread64(a, b, c, d as i64)),
        SYS_PWRITE64 => Some(crate::syscall::sys_pwrite64_impl(a, b, c, d as i64)),
        SYS_READV => Some(crate::syscall::sys_readv_impl(a, b, c)),
        SYS_WRITEV => Some(crate::fs::io_syscalls::sys_writev(a, b, c)),
        SYS_SENDFILE => Some(crate::syscall::sys_sendfile_impl(a, b, c, d)),
        SYS_STAT => Some(crate::fs::stat_syscalls::sys_stat(a, b)),
        SYS_FSTAT => Some(crate::fs::stat_syscalls::sys_fstat(a, b)),
        SYS_LSTAT => Some(crate::fs::stat_syscalls::sys_lstat(a, b)),
        SYS_LSEEK => Some(crate::fs::stat_syscalls::sys_lseek(a, b as i64, c as i32)),
        SYS_ACCESS => Some(crate::fs::stat_syscalls::sys_access(a, b as u32)),
        SYS_GETCWD => Some(crate::fs::stat_syscalls::sys_getcwd(a, b)),
        SYS_CHDIR => Some(crate::fs::stat_syscalls::sys_chdir(a)),
        SYS_FCHDIR => Some(crate::syscall::sys_fchdir_impl(a)),
        SYS_MKDIR => Some(crate::fs::stat_syscalls::sys_mkdir(a, b as u32)),
        SYS_RMDIR => Some(crate::syscall::sys_rmdir_impl(a)),
        SYS_UNLINK => Some(crate::fs::stat_syscalls::sys_unlink(a)),
        SYS_RENAME => Some(crate::fs::stat_syscalls::sys_rename(a, b)),
        SYS_LINK => Some(crate::syscall::sys_link_impl(a, b)),
        SYS_SYMLINK => Some(crate::syscall::sys_symlink_impl(a, b)),
        SYS_READLINK => Some(crate::syscall::sys_readlink_impl(a, b, c)),
        SYS_CREAT => Some(crate::syscall::sys_creat_impl(a, b as u32)),
        SYS_UMASK => Some(crate::syscall::sys_umask_impl(a as u32)),
        SYS_SYNC => Some(crate::syscall::sys_sync_impl()),
        SYS_UTIME => Some(crate::syscall::sys_utime_impl(a, b)),
        SYS_UTIMES => Some(crate::syscall::sys_utimes_impl(a, b)),
        SYS_MKNOD => Some(crate::syscall::sys_mknod_impl(a, b as u32, c as u64)),
        SYS_CHMOD => Some(crate::syscall::sys_chmod_impl(a, b as u32)),
        SYS_FCHMOD => Some(crate::syscall::sys_fchmod_impl(a, b as u32)),
        SYS_CHOWN => Some(crate::syscall::sys_chown_impl(a, b as u32, c as u32)),
        SYS_LCHOWN => Some(crate::syscall::sys_lchown_impl(a, b as u32, c as u32)),
        SYS_FCHOWN => Some(crate::syscall::sys_fchown_impl(a, b as u32, c as u32)),
        SYS_STATFS => Some(crate::syscall::sys_statfs_impl(a, b)),
        SYS_FSTATFS => Some(crate::syscall::sys_fstatfs_impl(a, b)),
        SYS_USTAT => Some(crate::syscall::sys_ustat_impl(a as u64, b)),
        SYS_ACCT => Some(crate::syscall::sys_acct_impl(a)),
        SYS_MOUNT => Some(crate::syscall::sys_mount_impl(a, b, c, d as u64, e)),
        SYS_UMOUNT2 => Some(crate::syscall::sys_umount2_impl(a, b as i32)),
        SYS_SWAPON => Some(crate::syscall::sys_swapon_impl(a, b as i32)),
        SYS_SYNCFS => Some(crate::syscall::sys_syncfs_impl(a)),
        SYS_FSYNC => Some(crate::syscall::sys_fsync_impl(a)),
        SYS_FDATASYNC => Some(crate::syscall::sys_fdatasync_impl(a)),
        SYS_TRUNCATE => Some(crate::syscall::sys_truncate_impl(a, b as i64)),
        SYS_FTRUNCATE => Some(crate::syscall::sys_ftruncate_impl(a, b as i64)),
        SYS_FALLOCATE => Some(crate::syscall::sys_fallocate_impl(
            a, b as i32, c as i64, d as i64,
        )),
        SYS_DUP => Some(crate::fs::vfs::dup(a)),
        SYS_DUP2 => Some(crate::fs::fcntl::sys_dup2(a, b)),
        SYS_DUP3 => Some(crate::fs::fcntl::sys_dup3(a, b, c as i32)),
        SYS_FCNTL => Some(crate::fs::fcntl::sys_fcntl(a, b as i32, c)),
        SYS_IOCTL => Some(crate::fs::ioctl::sys_ioctl(a, b, c)),
        SYS_FLOCK => Some(crate::fs::vfs_extras::sys_flock(a, b as i32)),
        SYS_PIPE => Some(crate::fs::pipe::sys_pipe(a)),
        SYS_PIPE2 => Some(crate::fs::pipe::sys_pipe2(a, b as u32)),
        SYS_GETDENTS => Some(crate::fs::getdents::sys_getdents(a, b, c)),
        SYS_GETDENTS64 => Some(crate::fs::getdents::sys_getdents64(a, b, c)),
        SYS_OPENAT => Some(crate::syscall::sys_openat_impl(
            a as i32, b, c as i32, d as u32,
        )),
        SYS_MKDIRAT => Some(crate::syscall::sys_mkdirat_impl(a as i32, b, c as u32)),
        SYS_MKNODAT => Some(crate::syscall::sys_mknodat_impl(
            a as i32, b, c as u32, d as u64,
        )),
        SYS_FCHOWNAT => Some(crate::syscall::sys_fchownat_impl(
            a as i32, b, c as u32, d as u32, e as i32,
        )),
        SYS_FCHMODAT => Some(crate::syscall::sys_fchmodat_impl(a as i32, b, c as u32)),
        SYS_FUTIMESAT => Some(crate::syscall::sys_futimesat_impl(a as i32, b, c)),
        SYS_NEWFSTATAT => Some(crate::syscall::sys_newfstatat_impl(
            a as i32, b, c, d as u32,
        )),
        SYS_UNLINKAT => Some(crate::syscall::sys_unlinkat_impl(a as i32, b, c as u32)),
        SYS_RENAMEAT => Some(crate::syscall::sys_renameat_impl(a as i32, b, c as i32, d)),
        SYS_LINKAT => Some(crate::syscall::sys_linkat_impl(
            a as i32, b, c as i32, d, e as i32,
        )),
        SYS_SYMLINKAT => Some(crate::syscall::sys_symlinkat_impl(a, b as i32, c)),
        SYS_READLINKAT => Some(crate::syscall::sys_readlinkat_impl(a as i32, b, c, d)),
        SYS_FACCESSAT => Some(crate::fs::stat_syscalls::sys_faccessat(
            a as i32, b, c as u32,
        )),
        SYS_UTIMENSAT => Some(crate::syscall::sys_utimensat_impl(a as i32, b, c, d as i32)),
        SYS_EXECVEAT => Some(crate::syscall::sys_execveat_impl(
            a as i32, b, c, d, e as i32,
        )),
        SYS_OPENAT2 => Some(crate::syscall::sys_openat2_impl(a as i32, b, c, d)),
        SYS_COPY_FILE_RANGE => Some(crate::syscall::sys_copy_file_range_impl(
            a, b, c, d, e, f as u32,
        )),
        SYS_PREADV2 => Some(crate::syscall::sys_preadv2_impl(a, b, c, d, e, f as i32)),
        SYS_PWRITEV2 => Some(crate::syscall::sys_pwritev2_impl(a, b, c, d, e, f as i32)),
        SYS_STATX => Some(crate::syscall::sys_statx_impl(
            a as i32, b, c as u32, d as u32, e,
        )),
        SYS_CLOSE_RANGE => match (super::arg_u32(a), super::arg_u32(b), super::arg_u32(c)) {
            (Some(first), Some(last), Some(flags)) => {
                Some(crate::fs::close_range::sys_close_range(first, last, flags))
            },
            _ => Some(einval()),
        },
        SYS_MEMFD_CREATE => Some(crate::syscall::sys_memfd_create_impl(a, b as u32)),
        SYS_REMAP_FILE_PAGES => Some(crate::syscall::sys_remap_file_pages_impl()),
        SYS_POSIX_FADVISE => Some(crate::fs::vfs_extras::sys_posix_fadvise(
            a, b as i64, c as i64, d as i32,
        )),
        SYS_POLL => Some(crate::fs::poll::sys_poll(a, b, c as i32)),
        SYS_SELECT => Some(crate::fs::poll::sys_select(a, b, c, d, e)),
        SYS_PPOLL => Some(crate::fs::poll::sys_ppoll(a, b, c, d, e)),
        SYS_PSELECT6 => Some(crate::fs::poll::sys_pselect6(a, b, c, d, e, f)),
        SYS_EPOLL_CREATE => Some(crate::fs::poll::sys_epoll_create(a as i32)),
        SYS_EPOLL_CREATE1 => Some(crate::syscall::sys_epoll_create1(a as u32)),
        SYS_EPOLL_CTL => Some(crate::fs::poll::sys_epoll_ctl(a, b as i32, c as i32, d)),
        SYS_EPOLL_WAIT => Some(crate::fs::poll::sys_epoll_wait(a, b, c as i32, d as i32)),
        SYS_EPOLL_PWAIT => Some(crate::fs::poll::sys_epoll_pwait(
            a, b, c as i32, d as i32, e, f,
        )),
        SYS_EVENTFD => Some(crate::fs::eventfd::sys_eventfd(a as u32)),
        SYS_EVENTFD2 => Some(crate::fs::eventfd::sys_eventfd2(a as u32, b as u32)),
        SYS_SIGNALFD => Some(crate::fs::signalfd::sys_signalfd(a as isize, b, c)),
        SYS_SIGNALFD4 => Some(crate::fs::signalfd::sys_signalfd4(
            a as isize, b, c, d as u32,
        )),
        SYS_TIMERFD_CREATE => Some(crate::fs::timerfd::sys_timerfd_create(a as u32, b as u32)),
        SYS_TIMERFD_SETTIME => Some(crate::fs::timerfd::sys_timerfd_settime(a, b as i32, c, d)),
        SYS_TIMERFD_GETTIME => Some(crate::fs::timerfd::sys_timerfd_gettime(a, b)),
        SYS_INOTIFY_INIT => Some(crate::fs::inotify::sys_inotify_init1(0)),
        SYS_INOTIFY_ADD_WATCH => Some(crate::fs::inotify::sys_inotify_add_watch(a, b, c as u32)),
        SYS_INOTIFY_RM_WATCH => Some(crate::fs::inotify::sys_inotify_rm_watch(a, b as i32)),
        SYS_INOTIFY_INIT1 => Some(crate::fs::inotify::sys_inotify_init1(a as u32)),
        SYS_FANOTIFY_INIT => Some(crate::fs::fanotify::sys_fanotify_init(a as u32, b as u32)),
        SYS_FANOTIFY_MARK => Some(crate::fs::fanotify::sys_fanotify_mark(
            a, b as u32, c as u64, d as i32, e,
        )),
        SYS_IO_URING_SETUP => Some(crate::io_uring::syscall::sys_io_uring_setup(a as u32, b)),
        SYS_IO_URING_ENTER => Some(crate::io_uring::syscall::sys_io_uring_enter(
            a, b as u32, c as u32, d as u32, e, f,
        )),
        SYS_IO_URING_REGISTER => Some(crate::io_uring::syscall::sys_io_uring_register(
            a, b as u32, c, d as u32,
        )),
        SYS_SOCKET => Some(crate::net::socket::sys_socket(a as i32, b as i32, c as i32)),
        SYS_CONNECT => Some(crate::net::socket::sys_connect(a, b, c as u32)),
        SYS_ACCEPT => Some(crate::net::socket::sys_accept(a, b, c)),
        SYS_SENDTO => Some(crate::net::socket::sys_sendto(
            a, b, c, d as i32, e, f as u32,
        )),
        SYS_RECVFROM => Some(crate::net::socket::sys_recvfrom(a, b, c, d as i32, e, f)),
        SYS_SENDMSG => Some(crate::net::socket::sys_sendmsg(a, b, c as i32)),
        SYS_RECVMSG => Some(crate::net::socket::sys_recvmsg(a, b, c as i32)),
        SYS_SHUTDOWN => Some(crate::net::socket::sys_shutdown(a, b as i32)),
        SYS_BIND => Some(crate::net::socket::sys_bind(a, b, c as u32)),
        SYS_LISTEN => Some(crate::net::socket::sys_listen(a, b as i32)),
        SYS_GETSOCKNAME => Some(crate::net::socket::sys_getsockname(a, b, c)),
        SYS_GETPEERNAME => Some(crate::net::socket::sys_getpeername(a, b, c)),
        SYS_SOCKETPAIR => Some(crate::net::socket::sys_socketpair(
            a as i32, b as i32, c as i32, d,
        )),
        SYS_SETSOCKOPT => Some(crate::net::socket::sys_setsockopt(
            a, b as i32, c as i32, d, e as u32,
        )),
        SYS_GETSOCKOPT => Some(crate::net::socket::sys_getsockopt(
            a, b as i32, c as i32, d, e,
        )),
        SYS_ACCEPT4 => {
            let fd = crate::net::socket::sys_accept(a, b, c);
            if fd >= 0 {
                const SOCK_NONBLOCK: usize = 0x800;
                const SOCK_CLOEXEC: usize = 0x80000;
                if d & SOCK_NONBLOCK != 0 {
                    let mut t = crate::net::socket::TCP_SOCKETS.lock();
                    if let Some(Some(s)) = t.get_mut(fd as usize) {
                        s.nonblocking = true;
                    }
                }
                if d & SOCK_CLOEXEC != 0 {
                    crate::fs::fcntl::set_cloexec(fd as usize, true);
                }
            }
            Some(fd)
        },
        SYS_SENDMMSG => Some(crate::syscall::sys_sendmmsg_impl(a, b, c as u32, d as u32)),
        SYS_RECVMMSG => Some(crate::syscall::sys_recvmmsg_impl(
            a, b, c as u32, d as u32, e,
        )),
        SYS_PIDFD_SEND_SIGNAL => Some(crate::fs::pidfd::sys_pidfd_send_signal(
            a, b as u32, c, d as u32,
        )),
        SYS_PIDFD_OPEN => Some(crate::fs::pidfd::sys_pidfd_open(a, b as u32)),
        SYS_PIDFD_GETFD => Some(crate::fs::pidfd::sys_pidfd_getfd(a, b, c as u32)),
        _ => None,
    }
}

/// Parse a `struct sigaction` from userspace.
/// Layout (x86_64 / musl):
///   [0..8]   sa_handler or sa_sigaction
///   [8..16]  sa_flags  (only low 32 bits used)
///   [16..24] sa_restorer
///   [24..32] sa_mask (low 64 bits of an rt_sigset_t[2])
fn read_sigaction_from_user(va: usize) -> Option<crate::proc::signal::SigAction> {
    if va == 0 {
        return None;
    }
    let mut buf = [0u8; 32];
    crate::uaccess::copy_from_user(&mut buf, va).ok()?;
    Some(crate::proc::signal::SigAction {
        handler: usize::from_ne_bytes(buf[0..8].try_into().ok()?) as usize,
        flags: u32::from_ne_bytes(buf[8..12].try_into().ok()?),
        restorer: usize::from_ne_bytes(buf[16..24].try_into().ok()?) as usize,
        mask: u64::from_ne_bytes(buf[24..32].try_into().ok()?),
    })
}

fn write_sigaction_to_user(va: usize, sa: &crate::proc::signal::SigAction) -> bool {
    if va == 0 {
        return true;
    }
    let mut buf = [0u8; 32];
    buf[0..8].copy_from_slice(&(sa.handler as u64).to_ne_bytes());
    buf[8..12].copy_from_slice(&sa.flags.to_ne_bytes());
    buf[16..24].copy_from_slice(&(sa.restorer as u64).to_ne_bytes());
    buf[24..32].copy_from_slice(&sa.mask.to_ne_bytes());
    crate::uaccess::copy_to_user(va, &buf).is_ok()
}

// Covers: process/thread lifecycle, credentials, signals, futex, prctl.
pub fn dispatch_process(ctx: &SyscallContext) -> Option<isize> {
    let (a, b, c, d, e, f) = (ctx.a0(), ctx.a1(), ctx.a2(), ctx.a3(), ctx.a4(), ctx.a5());
    match ctx.nr {
        // ---------------------------------------------------------------
        // Process lifecycle
        // ---------------------------------------------------------------
        SYS_FORK => Some(crate::proc::fork_syscall::sys_fork()),
        SYS_VFORK => Some(crate::proc::clone::sys_vfork()),
        SYS_CLONE => Some(crate::proc::clone::sys_clone(a, b, c, d, e)),
        SYS_CLONE3 => Some(crate::proc::clone::sys_clone3(a, b)),
        SYS_EXECVE => Some(crate::proc::exec::sys_execve(a, b, c)),
        SYS_EXECVEAT => Some(crate::syscall::sys_execveat_impl(
            a as i32, b, c, d, e as i32,
        )),
        SYS_EXIT => {
            crate::proc::exit::sys_exit(a as i32);
            Some(0)
        },
        SYS_EXIT_GROUP => {
            crate::proc::exit::sys_exit_group(a as i32);
            Some(0)
        },

        // ---------------------------------------------------------------
        // wait
        // ---------------------------------------------------------------
        SYS_WAIT4 => Some(crate::proc::wait::sys_wait4(a as isize, b, c as i32, d)),
        SYS_WAITPID => Some(crate::proc::wait::sys_waitpid(a as isize, b, c as i32)),
        SYS_WAITID => {
            // waitid(idtype, id, siginfo_ptr, options) — simplified
            // forward as wait4(-1, wstatus=0, opts, 0) for any-child variant
            let id = b as isize;
            let opt = d as i32;
            Some(crate::proc::wait::sys_wait4(id, 0, opt, 0))
        },

        // ---------------------------------------------------------------
        // Signal delivery
        // ---------------------------------------------------------------
        SYS_KILL => Some(crate::proc::signal::sys_kill(a as isize, b as i32)),
        SYS_TGKILL => Some(crate::proc::signal::sys_tgkill(a, b, c as i32)),
        SYS_TKILL => Some(crate::proc::signal::sys_tkill(a, b as i32)),

        // rt_sigaction: sig, new_act_ptr, old_act_ptr, sigsetsize
        SYS_RT_SIGACTION => {
            let sig = b as u32;
            let pid = crate::proc::scheduler::current_pid();
            let new_sa = read_sigaction_from_user(b);
            let mut old_sa = crate::proc::signal::SigAction::default();
            let had_old = c != 0;
            let ret = crate::proc::signal::sys_rt_sigaction(
                pid,
                sig,
                if a != 0 {
                    read_sigaction_from_user(a)
                } else {
                    None
                },
                if had_old { Some(&mut old_sa) } else { None },
            );
            if ret == 0 && had_old {
                write_sigaction_to_user(c, &old_sa);
            }
            let _ = new_sa;
            Some(ret)
        },

        // rt_sigprocmask: how, new_set_ptr, old_set_ptr, sigsetsize
        SYS_RT_SIGPROCMASK => {
            let how = a as u32;
            let pid = crate::proc::scheduler::current_pid();
            let new_set: Option<u64> = if b != 0 {
                let mut s = [0u8; 8];
                crate::uaccess::copy_from_user(&mut s, b)
                    .ok()
                    .map(|_| u64::from_ne_bytes(s))
            } else {
                None
            };
            let mut old_val = 0u64;
            let has_old = c != 0;
            let ret = crate::proc::signal::sys_rt_sigprocmask(
                how,
                new_set,
                if has_old { Some(&mut old_val) } else { None },
                pid,
            );
            if ret == 0 && has_old {
                let _ = crate::uaccess::copy_to_user(c, &old_val.to_ne_bytes());
            }
            Some(ret)
        },

        // rt_sigpending: set_ptr, sigsetsize
        SYS_RT_SIGPENDING => {
            let pid = crate::proc::scheduler::current_pid();
            let pending = crate::proc::signal::sys_rt_sigpending(pid);
            if a != 0 {
                let _ = crate::uaccess::copy_to_user(a, &pending.to_ne_bytes());
            }
            Some(0)
        },

        // rt_sigsuspend: new_mask_ptr, sigsetsize
        // Atomically replace signal mask and sleep until a signal arrives.
        SYS_RT_SIGSUSPEND => {
            let pid = crate::proc::scheduler::current_pid();
            if a != 0 {
                let mut s = [0u8; 8];
                if crate::uaccess::copy_from_user(&mut s, a).is_ok() {
                    let mask = u64::from_ne_bytes(s);
                    crate::proc::signal::sys_rt_sigprocmask(
                        2, // SIG_SETMASK
                        Some(mask),
                        None,
                        pid,
                    );
                }
            }
            crate::proc::scheduler::sleep_current();
            Some(-4) // EINTR
        },

        // rt_sigreturn (NR 15): restore context after signal handler.
        // The frame pointer is taken directly from the arch syscall entry.
        SYS_RT_SIGRETURN => {
            // The calling convention passes the SyscallFrame pointer in the
            // saved-rip field used by the x86_64 entry stub.  We retrieve it
            // via the context's saved_rip field, which holds the user RSP
            // at entry (set by arch/x86_64/syscall.rs).
            // Real work is done in signal::sys_rt_sigreturn, called directly
            // by the x86_64 entry assembly.  This arm is here as a fallback
            // in case dispatch() is called from a context that missed the
            // early intercept.
            Some(-38) // ENOSYS fallback — should be intercepted before here
        },

        // ---------------------------------------------------------------
        // Credentials
        // ---------------------------------------------------------------
        SYS_GETPID => Some(crate::proc::scheduler::current_pid() as isize),
        SYS_GETTID => Some(crate::proc::scheduler::current_pid() as isize),
        SYS_GETPPID => {
            let pid = crate::proc::scheduler::current_pid();
            Some(crate::proc::scheduler::with_proc(pid, |p| p.ppid as isize).unwrap_or(1))
        },
        SYS_GETPGRP => {
            let pid = crate::proc::scheduler::current_pid();
            Some(crate::proc::scheduler::with_proc(pid, |p| p.pgid as isize).unwrap_or(1))
        },
        SYS_GETPGID => Some(crate::proc::session::get_pgid(if a == 0 {
            crate::proc::scheduler::current_pid()
        } else {
            a
        })),
        SYS_SETPGID => Some(crate::proc::session::set_pgid(a, b)),
        SYS_GETSID => Some(crate::proc::session::get_sid(a)),
        SYS_SETSID => Some(crate::proc::session::setsid()),
        SYS_GETUID => {
            let pid = crate::proc::scheduler::current_pid();
            Some(crate::proc::scheduler::with_proc(pid, |p| p.uid as isize).unwrap_or(0))
        },
        SYS_GETGID => {
            let pid = crate::proc::scheduler::current_pid();
            Some(crate::proc::scheduler::with_proc(pid, |p| p.gid as isize).unwrap_or(0))
        },
        SYS_GETEUID => {
            let pid = crate::proc::scheduler::current_pid();
            Some(crate::proc::scheduler::with_proc(pid, |p| p.euid as isize).unwrap_or(0))
        },
        SYS_GETEGID => {
            let pid = crate::proc::scheduler::current_pid();
            Some(crate::proc::scheduler::with_proc(pid, |p| p.egid as isize).unwrap_or(0))
        },
        SYS_SETUID => Some(crate::proc::creds::sys_setuid(a as u32)),
        SYS_SETGID => Some(crate::proc::creds::sys_setgid(a as u32)),
        SYS_SETEUID => Some(crate::proc::creds::sys_seteuid(a as u32)),
        SYS_SETEGID => Some(crate::proc::creds::sys_setegid(a as u32)),
        SYS_SETREUID => Some(crate::proc::creds::sys_setreuid(a as u32, b as u32)),
        SYS_SETREGID => Some(crate::proc::creds::sys_setregid(a as u32, b as u32)),
        SYS_SETRESUID => Some(crate::proc::creds::sys_setresuid(
            a as u32, b as u32, c as u32,
        )),
        SYS_SETRESGID => Some(crate::proc::creds::sys_setresgid(
            a as u32, b as u32, c as u32,
        )),
        SYS_GETRESUID => Some(crate::syscall::copy_uid_to_user(a, b, c)),
        SYS_GETRESGID => Some(crate::syscall::copy_gid_to_user(a, b, c)),
        SYS_SETGROUPS => Some(crate::proc::creds::sys_setgroups(a, b)),
        SYS_GETGROUPS => Some(crate::proc::creds::sys_getgroups(a, b)),
        SYS_CAPGET => Some(crate::proc::creds::sys_capget(a, b)),
        SYS_CAPSET => Some(crate::proc::creds::sys_capset(a, b)),

        // ---------------------------------------------------------------
        // prctl / rlimit
        // ---------------------------------------------------------------
        SYS_PRCTL => Some(crate::proc::creds::sys_prctl(a as i32, b, c, d, e)),
        SYS_ARCH_PRCTL => Some(crate::proc::creds::sys_arch_prctl(a as i32, b)),
        SYS_GETRLIMIT => Some(crate::proc::rlimit::sys_getrlimit(a as u32, b)),
        SYS_SETRLIMIT => Some(crate::proc::rlimit::sys_setrlimit(a as u32, b)),
        SYS_PRLIMIT64 => Some(crate::proc::rlimit::sys_prlimit64(a, b as u32, c, d)),
        SYS_GETRUSAGE => Some(crate::proc::rusage::sys_getrusage(a as i32, b)),

        // ---------------------------------------------------------------
        // Thread-related
        // ---------------------------------------------------------------
        SYS_SET_TID_ADDRESS => Some(crate::proc::thread::sys_set_tid_address(a)),
        SYS_SET_ROBUST_LIST => Some(crate::proc::thread::sys_set_robust_list(a, b)),
        SYS_GET_ROBUST_LIST => Some(crate::proc::thread::sys_get_robust_list(a, b, c)),
        SYS_PTRACE => Some(crate::proc::ptrace::sys_ptrace(a as i32, b, c, d)),

        // ---------------------------------------------------------------
        // Futex
        // ---------------------------------------------------------------
        SYS_FUTEX => Some(crate::proc::futex::sys_futex(
            a, b as i32, c as u32, d, e, f as u32,
        )),
        SYS_FUTEX_WAITV => Some(crate::proc::futex::sys_futex_waitv(
            a, b as u32, c as u32, d, e as u64,
        )),

        _ => None,
    }
}

// Covers: mmap, mprotect, munmap, mremap, brk, madvise, mlock/munlock,
// msync, mmap2, pkey_*, mbarrier.
pub fn dispatch_memory(ctx: &SyscallContext) -> Option<isize> {
    let (a, b, c, d, e, f) = (ctx.a0(), ctx.a1(), ctx.a2(), ctx.a3(), ctx.a4(), ctx.a5());
    match ctx.nr {
        SYS_MMAP => Some(crate::mm::mmap::sys_mmap(
            a, b, c as i32, d as i32, e, f as i64,
        )),
        SYS_MPROTECT => Some(crate::mm::mmap::sys_mprotect(a, b, c as i32)),
        SYS_MUNMAP => Some(crate::mm::mmap::sys_munmap(a, b)),
        SYS_MREMAP => Some(crate::mm::mmap::sys_mremap(a, b, c, d as i32, e)),
        SYS_BRK => Some(crate::mm::mmap::sys_brk(a)),
        SYS_MADVISE => Some(crate::mm::mmap::sys_madvise(a, b, c as i32)),
        SYS_MLOCK => Some(crate::mm::mmap::sys_mlock(a, b)),
        SYS_MUNLOCK => Some(crate::mm::mmap::sys_munlock(a, b)),
        SYS_MLOCKALL => Some(crate::mm::mmap::sys_mlockall(a as i32)),
        SYS_MUNLOCKALL => Some(crate::mm::mmap::sys_munlockall()),
        SYS_MSYNC => Some(crate::mm::mmap::sys_msync(a, b, c as i32)),
        SYS_MMAP2 => Some(crate::mm::mmap::sys_mmap(
            a,
            b,
            c as i32,
            d as i32,
            e,
            (f as i64) * 4096,
        )),
        SYS_PKEY_ALLOC => Some(crate::mm::pkeys::sys_pkey_alloc(a as u32, b as u32)),
        SYS_PKEY_FREE => Some(crate::mm::pkeys::sys_pkey_free(a as u32)),
        SYS_PKEY_MPROTECT => Some(crate::mm::pkeys::sys_pkey_mprotect(
            a, b, c as i32, d as i32,
        )),
        SYS_PROCESS_VM_READV => Some(crate::mm::vm_rw::sys_process_vm_readv(
            a, b, c, d, e, f as u32,
        )),
        SYS_PROCESS_VM_WRITEV => Some(crate::mm::vm_rw::sys_process_vm_writev(
            a, b, c, d, e, f as u32,
        )),
        _ => None,
    }
}

// Covers: SysV shm/sem/msg, POSIX mq.
pub fn dispatch_ipc(ctx: &SyscallContext) -> Option<isize> {
    let (a, b, c, d, e, _f) = (ctx.a0(), ctx.a1(), ctx.a2(), ctx.a3(), ctx.a4(), ctx.a5());
    match ctx.nr {
        SYS_SHMGET => Some(match crate::ipc::shm::shmget(a as i32, b, c as i32) {
            Ok(id) => id as isize,
            Err(e) => e,
        }),
        SYS_SHMAT => Some(match crate::ipc::shm::shmat(a as i32, b, c as i32) {
            Ok(va) => va as isize,
            Err(e) => e,
        }),
        SYS_SHMDT => Some(match crate::ipc::shm::shmdt(a) {
            Ok(()) => 0,
            Err(e) => e,
        }),
        SYS_SHMCTL => Some(crate::syscall::shmctl_dispatch(a as i32, b as i32, c)),
        SYS_SEMGET => Some(
            match crate::ipc::sem::semget(a as i32, b as i32, c as i32) {
                Ok(id) => id as isize,
                Err(e) => e,
            },
        ),
        SYS_SEMOP => Some(crate::syscall::semop_dispatch(a as i32, b, c)),
        SYS_SEMCTL => Some(crate::syscall::semctl_dispatch(
            a as i32, b as i32, c as i32, d,
        )),
        SYS_MSGGET => Some(match crate::ipc::msg::msgget(a as i32, b as i32) {
            Ok(id) => id as isize,
            Err(e) => e,
        }),
        SYS_MSGSND => Some(crate::syscall::msgsnd_dispatch(a as i32, b, c, d as i32)),
        SYS_MSGRCV => Some(crate::syscall::msgrcv_dispatch(
            a as i32, b, c, d as i64, e as i32,
        )),
        SYS_MSGCTL => Some(crate::syscall::msgctl_dispatch(a as i32, b as i32, c)),
        SYS_MQ_OPEN => Some(crate::syscall::mq_open_dispatch(a, b as i32, c as u32, d)),
        SYS_MQ_UNLINK => Some(crate::syscall::mq_unlink_dispatch(a)),
        SYS_MQ_TIMEDSEND => Some(crate::syscall::mq_timedsend_dispatch(
            a as u64, b, c, d as u32,
        )),
        SYS_MQ_TIMEDRECEIVE => Some(crate::syscall::mq_timedreceive_dispatch(a as u64, b, c, d)),
        SYS_MQ_NOTIFY => Some(crate::syscall::mq_notify_dispatch(a as u64, b)),
        SYS_MQ_GETSETATTR => Some(crate::syscall::mq_getsetattr_dispatch(a as u64, b, c)),
        _ => None,
    }
}

// Covers: clock_*, nanosleep, getitimer, setitimer, times, alarm.
pub fn dispatch_time(ctx: &SyscallContext) -> Option<isize> {
    let (a, b, c, d, _e, _f) = (ctx.a0(), ctx.a1(), ctx.a2(), ctx.a3(), ctx.a4(), ctx.a5());
    match ctx.nr {
        SYS_NANOSLEEP => Some(crate::proc::nanosleep::sys_nanosleep(a, b)),
        SYS_CLOCK_NANOSLEEP => Some(crate::proc::nanosleep::sys_clock_nanosleep(
            a as u32, b as i32, c, d,
        )),
        SYS_CLOCK_GETTIME => Some(crate::proc::nanosleep::sys_clock_gettime(a as u32, b)),
        SYS_CLOCK_SETTIME => Some(crate::proc::nanosleep::sys_clock_settime(a as u32, b)),
        SYS_CLOCK_GETRES => Some(crate::proc::nanosleep::sys_clock_getres(a as u32, b)),
        SYS_GETITIMER => Some(crate::proc::itimer::sys_getitimer(a as i32, b)),
        SYS_SETITIMER => Some(crate::proc::itimer::sys_setitimer(a as i32, b, c)),
        SYS_ALARM => Some(crate::proc::itimer::sys_alarm(a as u32)),
        SYS_TIMES => Some(crate::proc::rusage::sys_times(a)),
        SYS_TIME => Some(crate::proc::nanosleep::sys_time(a)),
        SYS_GETTIMEOFDAY => Some(crate::proc::nanosleep::sys_gettimeofday(a, b)),
        SYS_SETTIMEOFDAY => Some(crate::proc::nanosleep::sys_settimeofday(a, b)),
        _ => None,
    }
}

// Covers: hybrid/misc: getrandom, uname, umask, personality, sysinfo,
// syslog, reboot, pivot_root, acpi, seccomp, bpf.
pub fn dispatch_hybrid_services(ctx: &SyscallContext) -> Option<isize> {
    let (a, b, c, _d, _e, _f) = (ctx.a0(), ctx.a1(), ctx.a2(), ctx.a3(), ctx.a4(), ctx.a5());
    match ctx.nr {
        SYS_GETRANDOM => Some(crate::syscall::sys_getrandom_impl(a, b, c as u32)),
        SYS_UNAME => Some(crate::syscall::sys_uname_impl(a)),
        SYS_PERSONALITY => Some(0isize),
        SYS_SYSINFO => Some(crate::syscall::sys_sysinfo_impl(a)),
        SYS_SYSLOG => Some(crate::syscall::sys_syslog_impl(a as i32, b, c as i32)),
        SYS_REBOOT => Some(crate::syscall::sys_reboot_impl(
            a as i32, b as i32, c as u32, 0,
        )),
        SYS_PIVOT_ROOT => Some(crate::syscall::sys_pivot_root_impl(a, b)),
        SYS_SECCOMP => Some(crate::security::seccomp::sys_seccomp(a as u32, b as u32, c)),
        SYS_BPF => Some(crate::security::bpf::sys_bpf(a as i32, b, c as u32)),
        SYS_LANDLOCK_CREATE_RULESET => Some(
            crate::security::landlock::sys_landlock_create_ruleset(a, b, c as u32),
        ),
        SYS_LANDLOCK_ADD_RULE => Some(crate::security::landlock::sys_landlock_add_rule(
            a, b as u32, c, d as u32,
        )),
        SYS_LANDLOCK_RESTRICT_SELF => Some(crate::security::landlock::sys_landlock_restrict_self(
            a, b as u32,
        )),
        _ => None,
    }
}

#[cfg(feature = "kmtest")]
pub fn dispatch_kmtest(ctx: &SyscallContext) -> Option<isize> {
    match ctx.nr {
        crate::syscall::nr::SYS_KMTEST_LIST => {
            Some(crate::syscall::kmtest::sys_kmtest_list(ctx.a0(), ctx.a1()))
        },
        crate::syscall::nr::SYS_KMTEST_RUN => Some(crate::syscall::kmtest::sys_kmtest_run(
            ctx.a0(),
            ctx.a1(),
            ctx.a2(),
        )),
        _ => None,
    }
}

#[cfg(not(feature = "kmtest"))]
pub fn dispatch_kmtest(_ctx: &SyscallContext) -> Option<isize> {
    None
}
