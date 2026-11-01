//! Descriptor readiness and Linux poll/select/epoll entry points.
//!
//! User descriptors are always resolved through the current process table,
//! including standard input/output. Scheme handlers supply their own readiness
//! source: opaque scheme-local IDs must never be guessed to be another type.
//! Legacy snapshot-only sources are checked after bounded scheduler sleeps;
//! they are not advertised as always ready.

extern crate alloc;
use crate::sync::poll_source::{AlwaysReady, PollSource};
use crate::sync::wait_queue::{ReadyMask, WaitQueue};
use alloc::sync::Arc;

#[path = "poll_syscalls.rs"]
mod syscalls;
pub use syscalls::*;

pub const POLLIN: u32 = 0x0001;
pub const POLLPRI: u32 = 0x0002;
pub const POLLOUT: u32 = 0x0004;
pub const POLLERR: u32 = 0x0008;
pub const POLLHUP: u32 = 0x0010;
pub const POLLNVAL: u32 = 0x0020;
pub const POLLRDNORM: u32 = 0x0040;
pub const POLLWRNORM: u32 = 0x0100;
pub const POLLRDHUP: u32 = 0x2000;
pub const EPOLLONESHOT: u32 = 0x4000_0000;

fn user_fd_to_bfd(user_fd: usize) -> Option<usize> {
    let bfd = crate::fs::process_fd::proc_fd_backing(
        crate::proc::scheduler::current_pid(), user_fd,
    );
    (bfd >= 0).then_some(bfd as usize)
}

/// Normalize legacy POLLIN/POLLOUT snapshots without returning unrequested
/// normal-read/write alias bits. Error/hangup bits are always observable.
fn ready_subset(bits: u32, interest: u32) -> u32 {
    let mut bits = bits;
    if bits & POLLIN != 0 { bits |= POLLRDNORM; }
    if bits & POLLOUT != 0 { bits |= POLLWRNORM; }
    bits & (interest | POLLERR | POLLHUP | POLLNVAL)
}

enum SnapshotKind {
    Stdin,
    Socket(usize),
    Event(usize),
    Timer(usize),
    Signal(usize),
    Inotify(usize),
    Fanotify(usize),
    Pid(usize),
}

struct SnapshotSource {
    kind: SnapshotKind,
    wq: WaitQueue,
}

impl PollSource for SnapshotSource {
    fn poll(&self, interest: ReadyMask) -> ReadyMask {
        let bits = match self.kind {
            SnapshotKind::Stdin => {
                if crate::tty::serial::bytes_available() > 0 { POLLIN } else { 0 }
            },
            SnapshotKind::Socket(fd) => crate::net::socket::socket_poll(fd),
            SnapshotKind::Event(fd) => crate::fs::eventfd::eventfd_poll(fd),
            SnapshotKind::Timer(fd) => crate::fs::timerfd::timerfd_poll(fd, interest | POLLIN),
            SnapshotKind::Signal(fd) => crate::fs::signalfd::signalfd_poll(fd),
            SnapshotKind::Inotify(fd) => crate::fs::inotify::inotify_poll(fd, interest | POLLIN),
            SnapshotKind::Fanotify(fd) => crate::fs::fanotify::fanotify_poll(fd, interest | POLLIN),
            SnapshotKind::Pid(fd) => crate::fs::pidfd::pidfd_poll(fd, interest | POLLIN),
        };
        ready_subset(bits, interest)
    }

    fn wait_queue(&self) -> &WaitQueue { &self.wq }
}

fn snapshot(kind: SnapshotKind) -> Arc<dyn PollSource> {
    Arc::new(SnapshotSource { kind, wq: WaitQueue::new() })
}

/// Snapshot adapters for schemes that do not yet publish queue notifications.
pub fn timer_source(raw: usize) -> Arc<dyn PollSource> { snapshot(SnapshotKind::Timer(raw)) }
pub fn inotify_source(raw: usize) -> Arc<dyn PollSource> { snapshot(SnapshotKind::Inotify(raw)) }
pub fn pidfd_source(raw: usize) -> Arc<dyn PollSource> { snapshot(SnapshotKind::Pid(raw)) }

/// Resolve a kernel backing descriptor without interpreting opaque scheme IDs.
fn backing_poll_source(bfd: usize) -> Option<Arc<dyn PollSource>> {
    if let Some((scheme, fid)) = crate::fs::scheme_fd::scheme_fd_get_fid(bfd) {
        return scheme.poll_source(fid);
    }
    if bfd == 0 { return Some(snapshot(SnapshotKind::Stdin)); }
    if bfd == 1 || bfd == 2 { return Some(Arc::new(AlwaysReady::new())); }
    if let Some(source) = syscalls::epoll_poll_source(bfd) { return Some(source); }
    if crate::fs::pipe::is_pipe(bfd) {
        return crate::fs::pipe::pipe_poll_source(bfd);
    }
    if crate::net::socket::is_socket_fd(bfd) {
        return Some(snapshot(SnapshotKind::Socket(bfd)));
    }
    if crate::fs::eventfd::is_eventfd(bfd) {
        return Some(snapshot(SnapshotKind::Event(bfd)));
    }
    if crate::fs::timerfd::is_timerfd(bfd) {
        return Some(snapshot(SnapshotKind::Timer(bfd)));
    }
    if crate::fs::signalfd::is_signalfd(bfd) {
        return Some(snapshot(SnapshotKind::Signal(bfd)));
    }
    if crate::fs::inotify::is_inotify_fd(bfd) {
        return Some(snapshot(SnapshotKind::Inotify(bfd)));
    }
    if crate::fs::fanotify::is_fanotify_fd(bfd) {
        return Some(snapshot(SnapshotKind::Fanotify(bfd)));
    }
    if crate::fs::vfs::is_raw_fd(bfd)
        || crate::fs::devfs::get_dev_fd(bfd).is_some()
        || crate::fs::procfs::is_procfs_fd(bfd)
        || crate::fs::sysfs::is_sysfs_fd(bfd)
        || crate::fs::cgroupfs::is_cgroupfs_fd(bfd)
    {
        return Some(Arc::new(AlwaysReady::new()));
    }
    None
}

/// Source for an open process-local descriptor, or None for invalid/unsupported.
pub fn fd_poll_source(fd: usize) -> Option<Arc<dyn PollSource>> {
    resolved_source(fd).ok()
}

struct PinnedSource {
    source: Arc<dyn PollSource>,
    // A cloned FdEntry pins the underlying open-file description. Dropping it
    // after the syscall is safe even if another thread closes the user fd.
    _entry: crate::fs::process_fd::FdEntry,
}

impl PollSource for PinnedSource {
    fn poll(&self, interest: ReadyMask) -> ReadyMask { self.source.poll(interest) }
    fn wait_queue(&self) -> &WaitQueue { self.source.wait_queue() }
}

fn resolved_source(fd: usize) -> Result<Arc<dyn PollSource>, isize> {
    let entry = crate::fs::process_fd::proc_fd_get(
        crate::proc::scheduler::current_pid(), fd,
    ).ok_or(-9isize)?;
    let source = backing_poll_source(entry.backing_fd).ok_or(-95isize)?;
    Ok(Arc::new(PinnedSource { source, _entry: entry }))
}

/// Synchronous compatibility entry point used by io_uring.
pub fn fd_ready(fd: usize, events: u32) -> u32 {
    match resolved_source(fd) {
        Ok(source) => ready_subset(source.poll(events | POLLERR | POLLHUP), events),
        Err(-9) => POLLNVAL,
        Err(_) => POLLERR,
    }
}
