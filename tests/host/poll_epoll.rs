//! Production poll/select/epoll implementation with deterministic platform
//! mocks. Exercises ABI/state/lifetime behavior, not hardware scheduling/SMP.
extern crate alloc;
extern crate self as spin;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

pub struct Mutex<T>(std::sync::Mutex<T>);
impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self { Self(std::sync::Mutex::new(value)) }
    pub fn lock(&self) -> std::sync::MutexGuard<'_, T> { self.0.lock().unwrap() }
}

static PID: AtomicUsize = AtomicUsize::new(1);
static NOW: AtomicU64 = AtomicU64::new(0);
static SLEEPS: AtomicUsize = AtomicUsize::new(0);
static COPY_FAIL: AtomicBool = AtomicBool::new(false);
static INSTALL_FAIL: AtomicBool = AtomicBool::new(false);
static STDIN: AtomicUsize = AtomicUsize::new(0);
static TOKEN: Mutex<Option<Arc<sync::wait_queue::CancellationToken>>> = Mutex::new(None);
static SLEEP_ACTION: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);
static REGISTER_ACTION: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);
static FDS: Mutex<BTreeMap<(usize, usize), fs::process_fd::FdEntry>> = Mutex::new(BTreeMap::new());
static SOURCES: Mutex<BTreeMap<usize, Arc<Source>>> = Mutex::new(BTreeMap::new());
static SCHEMES: Mutex<BTreeMap<usize, Option<Arc<Source>>>> = Mutex::new(BTreeMap::new());
static CLOSED: Mutex<Vec<usize>> = Mutex::new(Vec::new());

#[path = "../../src/fs/poll.rs"]
pub mod real_poll;
#[path = "../../src/sync/poll_source.rs"]
pub mod real_poll_source;

pub mod time { pub fn read_monotonic_ns() -> u64 { crate::NOW.load(crate::Ordering::SeqCst) } }
pub mod tty { pub mod serial {
    pub fn bytes_available() -> usize { crate::STDIN.load(crate::Ordering::SeqCst) }
} }
pub mod proc { pub mod scheduler {
    pub fn current_pid() -> usize { crate::PID.load(crate::Ordering::SeqCst) }
    pub fn task_cancel_token(_: usize) -> Option<std::sync::Arc<crate::sync::wait_queue::CancellationToken>> {
        crate::TOKEN.lock().clone()
    }
} }
pub mod sync {
    pub use crate::real_poll_source as poll_source;
    pub mod wait_queue {
        use crate::*;
        pub type ReadyMask = u32;
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum WakeReason { Ready, Timeout, Cancelled }
        pub struct CancellationToken(AtomicBool);
        impl CancellationToken {
            pub fn new() -> Self { Self(AtomicBool::new(false)) }
            pub fn is_cancelled(&self) -> bool { self.0.load(Ordering::SeqCst) }
            pub fn signal(&self) { self.0.store(true, Ordering::SeqCst); }
        }
        pub struct WaitQueue {
            ready: AtomicU32,
            forwarders: Mutex<Vec<(usize, u32)>>,
        }
        impl WaitQueue {
            pub const fn new() -> Self {
                Self { ready: AtomicU32::new(0), forwarders: Mutex::new(Vec::new()) }
            }
            pub fn wake(&self, bits: u32) {
                self.ready.fetch_or(bits, Ordering::SeqCst);
                for (target, interest) in self.forwarders.lock().iter() {
                    if bits & interest != 0 {
                        unsafe { &*(*target as *const WaitQueue) }.ready.fetch_or(bits, Ordering::SeqCst);
                    }
                }
            }
            pub fn clear(&self, bits: u32) { self.ready.fetch_and(!bits, Ordering::SeqCst); }
            pub fn poll(&self, bits: u32) -> u32 { self.ready.load(Ordering::SeqCst) & bits }
            pub fn register_forwarder(&self, target: &Self, interest: u32) {
                let ptr = target as *const Self as usize;
                {
                    let mut forwarders = self.forwarders.lock();
                    if let Some((_, old)) = forwarders.iter_mut().find(|(p, _)| *p == ptr) {
                        *old |= interest;
                    } else { forwarders.push((ptr, interest)); }
                }
                let action = REGISTER_ACTION.lock().take();
                if let Some(action) = action { action(); }
            }
            pub fn remove_forwarder(&self, target: &Self) {
                let ptr = target as *const Self as usize;
                self.forwarders.lock().retain(|(p, _)| *p != ptr);
            }
            pub fn wait(&self, interest: u32, cancel: Option<&CancellationToken>, deadline: Option<u64>) -> WakeReason {
                if self.poll(interest) != 0 { return WakeReason::Ready; }
                if cancel.map_or(false, |c| c.is_cancelled()) { return WakeReason::Cancelled; }
                SLEEPS.fetch_add(1, Ordering::SeqCst);
                if let Some(deadline) = deadline {
                    assert!(deadline >= NOW.load(Ordering::SeqCst));
                    NOW.store(deadline, Ordering::SeqCst);
                }
                let action = SLEEP_ACTION.lock().take();
                if let Some(action) = action { action(); }
                if cancel.map_or(false, |c| c.is_cancelled()) { return WakeReason::Cancelled; }
                assert!(deadline.is_some(), "unbounded mock wait without cancellation");
                WakeReason::Timeout
            }
        }
    }
}

pub struct Source {
    ready: AtomicU32,
    queue: sync::wait_queue::WaitQueue,
}
impl Source {
    fn new(ready: u32) -> Arc<Self> {
        Arc::new(Self { ready: AtomicU32::new(ready), queue: sync::wait_queue::WaitQueue::new() })
    }
    fn set(&self, bits: u32) {
        self.ready.store(bits, Ordering::SeqCst);
        self.queue.wake(bits);
    }
}
impl sync::poll_source::PollSource for Source {
    fn poll(&self, interest: u32) -> u32 { self.ready.load(Ordering::SeqCst) & interest }
    fn wait_queue(&self) -> &sync::wait_queue::WaitQueue { &self.queue }
}

pub mod fs {
    pub use crate::real_poll as poll;
    pub mod process_fd {
        use crate::*;
        #[derive(Clone)]
        pub struct FdEntry {
            pub backing_fd: usize,
            pub cloexec: bool,
            lifetime: Arc<Lifetime>,
        }
        struct Lifetime(usize);
        impl Drop for Lifetime {
            fn drop(&mut self) {
                fs::poll::on_backing_close(self.0);
                fs::poll::epoll_close(self.0);
                SOURCES.lock().remove(&self.0);
                SCHEMES.lock().remove(&self.0);
                CLOSED.lock().push(self.0);
            }
        }
        pub fn proc_fd_get(pid: usize, fd: usize) -> Option<FdEntry> { FDS.lock().get(&(pid, fd)).cloned() }
        pub fn proc_fd_backing(pid: usize, fd: usize) -> isize {
            proc_fd_get(pid, fd).map_or(-9, |e| e.backing_fd as isize)
        }
        pub fn proc_fd_install_checked(
            pid: usize, bfd: usize, _: Option<String>, flags: u32, preferred: Option<usize>,
        ) -> Result<usize, isize> {
            let entry = FdEntry { backing_fd: bfd, cloexec: flags & 0x80000 != 0,
                lifetime: Arc::new(Lifetime(bfd)) };
            if INSTALL_FAIL.load(Ordering::SeqCst) { return Err(-24); }
            let mut fds = FDS.lock();
            let fd = preferred.unwrap_or_else(|| (0..4096).find(|fd| !fds.contains_key(&(pid, *fd))).unwrap());
            assert!(!fds.contains_key(&(pid, fd)));
            fds.insert((pid, fd), entry);
            Ok(fd)
        }
        pub fn close(pid: usize, fd: usize) {
            let old = FDS.lock().remove(&(pid, fd));
            drop(old);
        }
        pub fn dup_to(from_pid: usize, from_fd: usize, pid: usize, fd: usize) {
            let entry = proc_fd_get(from_pid, from_fd).unwrap();
            assert!(!FDS.lock().contains_key(&(pid, fd)));
            FDS.lock().insert((pid, fd), entry);
        }
    }
    pub mod scheme_fd {
        use crate::*;
        pub struct Scheme(Option<Arc<Source>>);
        impl Scheme {
            pub fn poll_source(&self, _: usize) -> Option<Arc<dyn sync::poll_source::PollSource>> {
                self.0.clone().map(|s| s as Arc<dyn sync::poll_source::PollSource>)
            }
        }
        pub fn scheme_fd_get_fid(bfd: usize) -> Option<(Arc<Scheme>, usize)> {
            SCHEMES.lock().get(&bfd).cloned().map(|s| (Arc::new(Scheme(s)), 0))
        }
    }
    pub mod pipe {
        use crate::*;
        pub fn is_pipe(bfd: usize) -> bool { SOURCES.lock().contains_key(&bfd) }
        pub fn pipe_poll_source(bfd: usize) -> Option<Arc<dyn sync::poll_source::PollSource>> {
            SOURCES.lock().get(&bfd).cloned().map(|s| s as Arc<dyn sync::poll_source::PollSource>)
        }
    }
    pub mod vfs { pub fn is_raw_fd(bfd: usize) -> bool { bfd == 10000 } }
    pub mod devfs { pub fn get_dev_fd(_: usize) -> Option<()> { None } }
    pub mod procfs { pub fn is_procfs_fd(_: usize) -> bool { false } }
    pub mod sysfs { pub fn is_sysfs_fd(_: usize) -> bool { false } }
    pub mod cgroupfs { pub fn is_cgroupfs_fd(_: usize) -> bool { false } }
    pub mod eventfd {
        pub fn is_eventfd(_: usize) -> bool { false }
        pub fn eventfd_poll(_: usize) -> u32 { 0 }
    }
    pub mod timerfd {
        pub fn is_timerfd(_: usize) -> bool { false }
        pub fn timerfd_poll(_: usize, _: u32) -> u32 { 0 }
    }
    pub mod signalfd {
        pub fn is_signalfd(_: usize) -> bool { false }
        pub fn signalfd_poll(_: usize) -> u32 { 0 }
    }
    pub mod inotify {
        pub fn is_inotify_fd(_: usize) -> bool { false }
        pub fn inotify_poll(_: usize, _: u32) -> u32 { 0 }
    }
    pub mod fanotify {
        pub fn is_fanotify_fd(_: usize) -> bool { false }
        pub fn fanotify_poll(_: usize, _: u32) -> u32 { 0 }
    }
    pub mod pidfd { pub fn pidfd_poll(_: usize, _: u32) -> u32 { 0 } }
}
pub mod net { pub mod socket {
    pub fn is_socket_fd(_: usize) -> bool { false }
    pub fn socket_poll(_: usize) -> u32 { 0 }
} }
pub mod uaccess {
    pub fn validate_user_ptr(ptr: usize, len: usize) -> bool {
        len == 0 || (ptr >= 4096 && ptr.checked_add(len).map_or(false, |end| end < usize::MAX / 2))
    }
    pub fn copy_from_user(dst: &mut [u8], ptr: usize) -> Result<(), ()> {
        if dst.is_empty() { return Ok(()); }
        if !validate_user_ptr(ptr, dst.len()) { return Err(()); }
        unsafe { core::ptr::copy_nonoverlapping(ptr as *const u8, dst.as_mut_ptr(), dst.len()) };
        Ok(())
    }
    pub fn copy_to_user(ptr: usize, src: &[u8]) -> Result<(), ()> {
        if src.is_empty() { return Ok(()); }
        if crate::COPY_FAIL.load(crate::Ordering::SeqCst) || !validate_user_ptr(ptr, src.len()) { return Err(()); }
        unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), ptr as *mut u8, src.len()) };
        Ok(())
    }
}

fn setup() {
    let old = core::mem::take(&mut *FDS.lock());
    drop(old);
    SOURCES.lock().clear();
    SCHEMES.lock().clear();
    CLOSED.lock().clear();
    PID.store(1, Ordering::SeqCst);
    NOW.store(0, Ordering::SeqCst);
    SLEEPS.store(0, Ordering::SeqCst);
    COPY_FAIL.store(false, Ordering::SeqCst);
    INSTALL_FAIL.store(false, Ordering::SeqCst);
    STDIN.store(0, Ordering::SeqCst);
    *TOKEN.lock() = Some(Arc::new(sync::wait_queue::CancellationToken::new()));
    *SLEEP_ACTION.lock() = None;
    *REGISTER_ACTION.lock() = None;
}
fn install(fd: usize, bfd: usize) {
    fs::process_fd::proc_fd_install_checked(1, bfd, None, 0, Some(fd)).unwrap();
}
fn source_fd(fd: usize, bfd: usize, ready: u32) -> Arc<Source> {
    let source = Source::new(ready);
    SOURCES.lock().insert(bfd, Arc::clone(&source));
    install(fd, bfd);
    source
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PollFd { fd: i32, events: u16, revents: u16 }
fn poll(fds: &mut [PollFd], timeout: i32) -> isize {
    real_poll::sys_poll(fds.as_mut_ptr() as usize, fds.len(), timeout)
}
fn event(events: u32, data: u64) -> [u8; 12] {
    let mut bytes = [0; 12];
    bytes[..4].copy_from_slice(&events.to_ne_bytes());
    bytes[4..].copy_from_slice(&data.to_ne_bytes());
    bytes
}
fn ctl(ep: usize, op: i32, fd: i32, flags: u32, data: u64) -> isize {
    real_poll::sys_epoll_ctl(ep, op, fd, event(flags, data).as_ptr() as usize)
}
fn wait(ep: usize, bytes: &mut [u8], max: i32, timeout: i32) -> isize {
    real_poll::sys_epoll_wait(ep, bytes.as_mut_ptr() as usize, max, timeout)
}

#[test]
fn poll_zero_bad_fds_masks_and_checked_copy() {
    setup();
    assert_eq!(real_poll::sys_poll(0, 0, 0), 0);
    assert_eq!(real_poll::sys_poll(0, 4097, 0), -22);
    assert_eq!(real_poll::sys_poll(0, 1, 0), -14);
    assert_eq!(real_poll::sys_poll(usize::MAX - 3, 2, 0), -14);
    source_fd(4, 100, real_poll::POLLIN | real_poll::POLLOUT);
    source_fd(5, 101, real_poll::POLLHUP);
    let mut fds = [
        PollFd { fd: -1, events: 0xffff, revents: 0xffff },
        PollFd { fd: 90, events: 0, revents: 0 },
        PollFd { fd: 4, events: 1, revents: 0 },
        PollFd { fd: 5, events: 0, revents: 0 },
        PollFd { fd: 4, events: 4, revents: 0 },
    ];
    assert_eq!(poll(&mut fds, 0), 4);
    assert_eq!(fds.map(|f| f.revents), [0, 0x20, 1, 0x10, 4]);
    COPY_FAIL.store(true, Ordering::SeqCst);
    assert_eq!(poll(&mut fds, 0), -14);
    COPY_FAIL.store(false, Ordering::SeqCst);
    assert_eq!(real_poll::fd_ready(0, 1), 0x20, "closed stdin is not implicitly valid");
    fs::process_fd::dup_to(1, 4, 1, 0);
    assert_eq!(real_poll::fd_ready(0, 1), 1, "redirected stdin follows its backing");
    install(6, 10000);
    assert_eq!(real_poll::fd_ready(6, 1), 1, "unrequested RDNORM alias is masked");
}

#[test]
fn timeout_progress_readiness_and_cancellation() {
    setup();
    assert_eq!(real_poll::sys_poll(0, 0, 7), 0);
    assert_eq!(NOW.load(Ordering::SeqCst), 7_000_000);
    assert_eq!(SLEEPS.load(Ordering::SeqCst), 1);
    let source = source_fd(4, 100, 0);
    let mut fds = [PollFd { fd: 4, events: 1, revents: 0xffff }];
    assert_eq!(poll(&mut fds, 3), 0);
    assert_eq!(fds[0].revents, 0);
    assert_eq!(NOW.load(Ordering::SeqCst), 10_000_000);
    assert_eq!(SLEEPS.load(Ordering::SeqCst), 4);
    *SLEEP_ACTION.lock() = Some(Box::new(move || source.set(1)));
    assert_eq!(poll(&mut fds, 5), 1);
    assert_eq!(NOW.load(Ordering::SeqCst), 11_000_000);
    let token = TOKEN.lock().clone().unwrap();
    *SLEEP_ACTION.lock() = Some(Box::new(move || token.signal()));
    assert_eq!(real_poll::sys_poll(0, 0, -1), -4);
}

#[test]
fn source_change_during_registration_is_not_lost() {
    setup();
    let source = source_fd(4, 100, 0);
    *REGISTER_ACTION.lock() = Some(Box::new(move || source.set(1)));
    let mut fds = [PollFd { fd: 4, events: 1, revents: 0 }];
    assert_eq!(poll(&mut fds, 10), 1);
    assert_eq!(SLEEPS.load(Ordering::SeqCst), 0);
    assert_eq!(NOW.load(Ordering::SeqCst), 0);
}

#[test]
fn select_bit_counts_timeout_validation_and_mask_rejection() {
    setup();
    source_fd(4, 100, 5);
    let mut read = 1u64 << 4;
    let mut write = read;
    let mut except = read;
    let mut timeval = [0i64, 0];
    assert_eq!(real_poll::sys_select(5, &mut read as *mut _ as usize,
        &mut write as *mut _ as usize, &mut except as *mut _ as usize,
        timeval.as_mut_ptr() as usize), 2);
    assert_eq!((read, write, except), (16, 16, 0));
    read = 1 << 8;
    assert_eq!(real_poll::sys_select(9, &mut read as *mut _ as usize, 0, 0,
        timeval.as_mut_ptr() as usize), -9);
    assert_eq!(read, 1 << 8, "EBADF does not silently clear input");
    timeval = [0, 1_000_000];
    assert_eq!(real_poll::sys_select(0, 0, 0, 0, timeval.as_mut_ptr() as usize), -22);
    timeval = [-1, 0];
    assert_eq!(real_poll::sys_select(0, 0, 0, 0, timeval.as_mut_ptr() as usize), -22);
    let mut timespec = [0i64, 2_000_000];
    assert_eq!(real_poll::sys_ppoll(0, 0, timespec.as_mut_ptr() as usize, 0, 0), 0);
    assert_eq!(timespec, [0, 0]);
    let mask = 0u64;
    assert_eq!(real_poll::sys_ppoll(0, 0, 0, &mask as *const _ as usize, 8), -95);
    assert_eq!(real_poll::sys_ppoll(0, 0, 0, &mask as *const _ as usize, 1), -22);
    assert_eq!(real_poll::sys_pselect6(0, 0, 0, 0, 0, 1), -14);
    let args = [&mask as *const _ as usize, 8usize];
    assert_eq!(real_poll::sys_pselect6(0, 0, 0, 0, 0, args.as_ptr() as usize), -95);
    let null_args = [0usize, 123];
    timespec = [0, 0];
    assert_eq!(real_poll::sys_pselect6(0, 0, 0, 0, timespec.as_mut_ptr() as usize,
        null_args.as_ptr() as usize), 0);
}

#[test]
fn epoll_add_modify_delete_oneshot_and_efault() {
    setup();
    source_fd(4, 100, 5);
    assert_eq!(real_poll::sys_epoll_create(0), -22);
    assert_eq!(real_poll::sys_epoll_create1(1), -22);
    let ep = real_poll::sys_epoll_create1(0x80000) as usize;
    assert!(fs::process_fd::proc_fd_get(1, ep).unwrap().cloexec);
    let mut output = [0u8; 24];
    assert_eq!(wait(ep, &mut output, 2, 0), 0);
    assert_eq!(real_poll::sys_epoll_wait(ep, 0, 1, 0), -14);
    assert_eq!(wait(ep, &mut output, 0, 0), -22);
    assert_eq!(ctl(ep, 1, 4, 0x80000001, 7), -95);
    assert_eq!(ctl(ep, 1, 4, 0x80, 7), -22);
    assert_eq!(ctl(ep, 1, ep as i32, 1, 7), -22);
    assert_eq!(ctl(ep, 1, 4, 1 | real_poll::EPOLLONESHOT, u64::MAX), 0);
    assert_eq!(ctl(ep, 1, 4, 1, 0), -17);
    COPY_FAIL.store(true, Ordering::SeqCst);
    assert_eq!(wait(ep, &mut output, 2, 0), -14);
    COPY_FAIL.store(false, Ordering::SeqCst);
    assert_eq!(wait(ep, &mut output, 2, 0), 1, "failed copyout does not consume one-shot");
    assert_eq!(&output[..12], &event(1, u64::MAX));
    assert_eq!(wait(ep, &mut output, 2, 0), 0);
    assert_eq!(ctl(ep, 3, 4, 4 | real_poll::EPOLLONESHOT, 99), 0);
    assert_eq!(wait(ep, &mut output, 2, 0), 1);
    assert_eq!(&output[..12], &event(4, 99));
    assert_eq!(real_poll::sys_epoll_ctl(ep, 2, 4, 0), 0, "DEL ignores event pointer");
    assert_eq!(real_poll::sys_epoll_ctl(ep, 2, 4, 0), -2);
    assert_eq!(wait(ep, &mut output, 2, 3), 0);
    assert_eq!(NOW.load(Ordering::SeqCst), 3_000_000);
    let token = TOKEN.lock().clone().unwrap();
    *SLEEP_ACTION.lock() = Some(Box::new(move || token.signal()));
    assert_eq!(wait(ep, &mut output, 2, -1), -4);
}

#[test]
fn epoll_shared_lifetime_ownership_final_close_and_fd_reuse() {
    setup();
    source_fd(4, 100, 1);
    let ep = real_poll::sys_epoll_create(1) as usize;
    let bfd = fs::process_fd::proc_fd_backing(1, ep) as usize;
    assert_eq!(ctl(ep, 1, 4, 1, 42), 0);
    PID.store(2, Ordering::SeqCst);
    let mut output = [0u8; 12];
    assert_eq!(wait(ep, &mut output, 1, 0), -9, "another process cannot use a global epoll number");
    PID.store(1, Ordering::SeqCst);
    fs::process_fd::dup_to(1, ep, 1, 6);
    fs::process_fd::dup_to(1, 4, 1, 5);
    fs::process_fd::close(1, ep);
    assert!(real_poll::is_epoll_fd(bfd));
    fs::process_fd::close(1, 4);
    assert_eq!(wait(6, &mut output, 1, 0), 1, "a live alias preserves the open-file watch");
    source_fd(4, 101, 1);
    assert_eq!(real_poll::sys_epoll_ctl(6, 2, 4, 0), -2, "reused fd is a different open file");
    fs::process_fd::close(1, 5);
    assert_eq!(wait(6, &mut output, 1, 0), 0, "last backing reference removes the old watch");
    assert_eq!(ctl(6, 1, 4, 1, 77), 0);
    fs::process_fd::dup_to(1, 6, 2, 8);
    fs::process_fd::dup_to(1, 4, 2, 4);
    PID.store(2, Ordering::SeqCst);
    assert_eq!(wait(8, &mut output, 1, 0), 1, "fork-style inherited descriptor shares instance");
    assert_eq!(&output, &event(1, 77));
    fs::process_fd::close(1, 6);
    assert!(real_poll::is_epoll_fd(bfd));
    fs::process_fd::close(2, 8);
    assert!(!real_poll::is_epoll_fd(bfd));
    assert_eq!(CLOSED.lock().iter().filter(|fd| **fd == bfd).count(), 1);
}

#[test]
fn epoll_round_robin_limits_and_allocation_rollback() {
    setup();
    source_fd(4, 100, 1);
    fs::process_fd::dup_to(1, 4, 1, 5);
    let ep = real_poll::sys_epoll_create(1) as usize;
    assert_eq!(ctl(ep, 1, 4, 1, 4), 0);
    assert_eq!(ctl(ep, 1, 5, 1, 5), 0, "distinct fd aliases can have distinct watches");
    let mut output = [0u8; 12];
    assert_eq!(wait(ep, &mut output, 1, 0), 1);
    assert_eq!(output, event(1, 4));
    assert_eq!(wait(ep, &mut output, 1, 0), 1);
    assert_eq!(output, event(1, 5));
    install(7, 10000);
    assert_eq!(ctl(ep, 1, 7, 1, 0), -1, "regular files cannot be registered");
    let other = real_poll::sys_epoll_create(1) as usize;
    assert_eq!(ctl(ep, 1, other as i32, 1, 0), -95, "nested epoll is explicitly unsupported");
    INSTALL_FAIL.store(true, Ordering::SeqCst);
    let before = CLOSED.lock().len();
    assert_eq!(real_poll::sys_epoll_create1(0), -24);
    assert_eq!(CLOSED.lock().len(), before + 1);
    let failed_bfd = *CLOSED.lock().last().unwrap();
    assert!(!real_poll::is_epoll_fd(failed_bfd), "failed fd allocation removes the epoll instance");
}

#[test]
fn scheme_identity_precedes_raw_namespace_and_poll_pins_lifetime() {
    setup();
    let bfd = 0x80000000;
    SCHEMES.lock().insert(bfd, Some(Source::new(1)));
    SOURCES.lock().insert(bfd, Source::new(4));
    install(4, bfd);
    assert_eq!(real_poll::fd_ready(4, 5), 1, "scheme-local identity beats overlapping raw IDs");
    let unsupported = 0x80000001;
    SCHEMES.lock().insert(unsupported, None);
    install(5, unsupported);
    let mut bad = [PollFd { fd: 5, events: 1, revents: 0 }];
    assert_eq!(poll(&mut bad, 0), -95, "unknown scheme readiness is not fabricated");
    let source = source_fd(6, 100, 0);
    *SLEEP_ACTION.lock() = Some(Box::new(move || {
        fs::process_fd::close(1, 6);
        assert!(!CLOSED.lock().contains(&100), "active poll holds the backing lifetime");
        source.set(1);
    }));
    let mut fds = [PollFd { fd: 6, events: 1, revents: 0 }];
    assert_eq!(poll(&mut fds, 3), 1);
    assert!(CLOSED.lock().contains(&100), "pin is released when poll completes");
}
