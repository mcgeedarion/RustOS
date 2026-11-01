//! Bounded user ABI decoding and lifetime-managed readiness syscalls.
//!
//! The process descriptor table owns backing lifetimes. The final-close hook
//! removes watches, so fd reuse cannot retarget an existing watch. Epoll aliases
//! and forked descriptors share the same instance through that table.

use super::*;
use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::sync::poll_source::wait_any;
use crate::sync::wait_queue::{CancellationToken, WakeReason};
use crate::uaccess::{copy_from_user, copy_to_user, validate_user_ptr};
use spin::Mutex;

const MAX_POLL_FDS: usize = 4096;
const MAX_EPOLLS: usize = 256;
const EPOLL_BASE: usize = 0x7500_0000;
const EPOLL_CLOEXEC: u32 = 0x0008_0000;
const ALWAYS: u32 = POLLERR | POLLHUP | POLLNVAL;
const EPOLL_EVENTS: u32 = POLLIN | POLLPRI | POLLOUT | POLLERR | POLLHUP
    | POLLRDNORM | POLLWRNORM | POLLRDHUP | EPOLLONESHOT;
// Not all legacy sources publish notifications yet. Sleep, rather than spin,
// between snapshots, while honoring an earlier user deadline.
const RESCAN_NS: u64 = 1_000_000;

fn now() -> u64 { crate::time::read_monotonic_ns() }

fn millis_deadline(timeout: i32) -> Option<u64> {
    (timeout >= 0).then(|| now().saturating_add(timeout as u64 * 1_000_000))
}

fn timeout_deadline(ptr: usize, unit: u64) -> Result<Option<u64>, isize> {
    if ptr == 0 { return Ok(None); }
    let mut bytes = [0u8; 16];
    copy_from_user(&mut bytes, ptr).map_err(|_| -14isize)?;
    let sec = i64::from_ne_bytes(bytes[..8].try_into().unwrap());
    let frac = i64::from_ne_bytes(bytes[8..].try_into().unwrap());
    if sec < 0 || frac < 0 || frac as u64 >= 1_000_000_000 / unit {
        return Err(-22);
    }
    let ns = (sec as u64).saturating_mul(1_000_000_000)
        .saturating_add(frac as u64 * unit);
    Ok(Some(now().saturating_add(ns)))
}

fn remaining(ptr: usize, deadline: Option<u64>, unit: u64) -> Result<(), isize> {
    if ptr == 0 { return Ok(()); }
    let ns = deadline.unwrap_or_else(now).saturating_sub(now());
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&(ns / 1_000_000_000).to_ne_bytes());
    bytes[8..].copy_from_slice(&((ns % 1_000_000_000) / unit).to_ne_bytes());
    copy_to_user(ptr, &bytes).map_err(|_| -14isize)
}

/// Signal-mask replacement requires atomic signal-delivery integration.
/// Do not silently ignore a requested mask, including an all-zero mask.
fn check_sigmask(ptr: usize, len: usize) -> Result<(), isize> {
    if ptr == 0 { return Ok(()); }
    if len != 8 { return Err(-22); }
    let mut mask = [0; 8];
    copy_from_user(&mut mask, ptr).map_err(|_| -14isize)?;
    Err(-95)
}

fn sleep_sources(
    sources: &[(Arc<dyn PollSource>, u32)],
    cancel: Option<&CancellationToken>,
    deadline: Option<u64>,
) -> Result<(), isize> {
    if cancel.map_or(false, CancellationToken::is_cancelled) { return Err(-4); }
    let slice = now().saturating_add(RESCAN_NS);
    let wake_at = if sources.is_empty() { deadline } else {
        Some(deadline.map_or(slice, |d| d.min(slice)))
    };
    match wait_any(sources, cancel, wake_at) {
        WakeReason::Cancelled => Err(-4),
        WakeReason::Ready | WakeReason::Timeout => Ok(()),
    }
}

struct PollFd {
    events: u32,
    source: Option<Arc<dyn PollSource>>,
    invalid: bool,
}

fn poll_impl(ptr: usize, nfds: usize, deadline: Option<u64>) -> isize {
    if nfds > MAX_POLL_FDS { return -22; }
    let bytes_len = nfds * 8;
    if bytes_len != 0 && !validate_user_ptr(ptr, bytes_len) { return -14; }
    let mut raw = vec![0u8; bytes_len];
    if copy_from_user(&mut raw, ptr).is_err() { return -14; }
    let mut fds = Vec::with_capacity(nfds);
    let mut sources = Vec::with_capacity(nfds);
    for bytes in raw.chunks_exact(8) {
        let fd = i32::from_ne_bytes(bytes[..4].try_into().unwrap());
        let events = u16::from_ne_bytes(bytes[4..6].try_into().unwrap()) as u32;
        let (source, invalid) = if fd < 0 { (None, false) } else {
            match resolved_source(fd as usize) {
                Ok(source) => (Some(source), false),
                Err(-9) => (None, true),
                Err(e) => return e,
            }
        };
        if let Some(source) = &source {
            sources.push((Arc::clone(source), events | ALWAYS));
        }
        fds.push(PollFd { events, source, invalid });
    }
    let cancel = crate::proc::scheduler::task_cancel_token(crate::proc::scheduler::current_pid());
    loop {
        let mut count = 0;
        for (i, fd) in fds.iter().enumerate() {
            let ready = if fd.invalid { POLLNVAL } else {
                fd.source.as_ref().map_or(0, |s| {
                    ready_subset(s.poll(fd.events | ALWAYS), fd.events)
                })
            };
            raw[i * 8 + 6..i * 8 + 8].copy_from_slice(&(ready as u16).to_ne_bytes());
            count += (ready != 0) as isize;
        }
        if count != 0 || deadline.map_or(false, |d| now() >= d) {
            // Only revents belongs to the kernel; preserve userspace fd/events.
            for i in 0..nfds {
                if copy_to_user(ptr + i * 8 + 6, &raw[i * 8 + 6..i * 8 + 8]).is_err() {
                    return -14;
                }
            }
            return count;
        }
        if let Err(error) = sleep_sources(&sources, cancel.as_deref(), deadline) {
            return error;
        }
    }
}

pub fn sys_poll(ptr: usize, nfds: usize, timeout: i32) -> isize {
    poll_impl(ptr, nfds, millis_deadline(timeout))
}

pub fn sys_ppoll(ptr: usize, nfds: usize, timeout: usize, mask: usize, mask_len: usize) -> isize {
    if let Err(error) = check_sigmask(mask, mask_len) { return error; }
    let deadline = match timeout_deadline(timeout, 1) { Ok(d) => d, Err(e) => return e };
    let result = poll_impl(ptr, nfds, deadline);
    if result >= 0 || result == -4 {
        if let Err(e) = remaining(timeout, deadline, 1) { return e; }
    }
    result
}

fn bit(bytes: &[u8], fd: usize) -> bool {
    bytes.get(fd / 8).map_or(false, |b| b & (1 << (fd % 8)) != 0)
}

fn select_impl(nfds: usize, ptrs: [usize; 3], deadline: Option<u64>) -> isize {
    if nfds > MAX_POLL_FDS { return -22; }
    // fd_set is a long array, not a byte-rounded bit array.
    let len = (nfds + 63) / 64 * 8;
    let mut sets: [Vec<u8>; 3] = core::array::from_fn(|_| vec![0; len]);
    for i in 0..3 {
        if ptrs[i] != 0 && copy_from_user(&mut sets[i], ptrs[i]).is_err() { return -14; }
    }
    let masks = [POLLIN | POLLRDNORM | POLLHUP | POLLERR,
        POLLOUT | POLLWRNORM | POLLERR, POLLPRI];
    let mut watches = Vec::new();
    let mut sources = Vec::new();
    for fd in 0..nfds {
        let mut interest = 0;
        for i in 0..3 { if bit(&sets[i], fd) { interest |= masks[i]; } }
        if interest == 0 { continue; }
        let source = match resolved_source(fd) { Ok(s) => s, Err(e) => return e };
        sources.push((Arc::clone(&source), interest | ALWAYS));
        watches.push((fd, source, interest));
    }
    let cancel = crate::proc::scheduler::task_cancel_token(crate::proc::scheduler::current_pid());
    let mut out: [Vec<u8>; 3] = core::array::from_fn(|_| vec![0; len]);
    loop {
        for set in &mut out { set.fill(0); }
        let mut count = 0;
        for (fd, source, interest) in &watches {
            let ready = source.poll(*interest | ALWAYS);
            if ready & POLLNVAL != 0 { return -9; }
            for i in 0..3 {
                if bit(&sets[i], *fd) && ready & masks[i] != 0 {
                    out[i][fd / 8] |= 1 << (fd % 8);
                    count += 1;
                }
            }
        }
        if count != 0 || deadline.map_or(false, |d| now() >= d) {
            for i in 0..3 {
                if ptrs[i] != 0 && copy_to_user(ptrs[i], &out[i]).is_err() { return -14; }
            }
            return count;
        }
        if let Err(error) = sleep_sources(&sources, cancel.as_deref(), deadline) { return error; }
    }
}

pub fn sys_select(nfds: usize, read: usize, write: usize, except: usize, timeout: usize) -> isize {
    let deadline = match timeout_deadline(timeout, 1000) { Ok(d) => d, Err(e) => return e };
    let result = select_impl(nfds, [read, write, except], deadline);
    if result >= 0 || result == -4 {
        if let Err(e) = remaining(timeout, deadline, 1000) { return e; }
    }
    result
}

pub fn sys_pselect6(
    nfds: usize, read: usize, write: usize, except: usize, timeout: usize, sigarg: usize,
) -> isize {
    if sigarg != 0 {
        let mut args = [0u8; 16];
        if copy_from_user(&mut args, sigarg).is_err() { return -14; }
        let mask = u64::from_ne_bytes(args[..8].try_into().unwrap()) as usize;
        let len = u64::from_ne_bytes(args[8..].try_into().unwrap()) as usize;
        if let Err(e) = check_sigmask(mask, len) { return e; }
    }
    let deadline = match timeout_deadline(timeout, 1) { Ok(d) => d, Err(e) => return e };
    let result = select_impl(nfds, [read, write, except], deadline);
    if result >= 0 || result == -4 {
        if let Err(e) = remaining(timeout, deadline, 1) { return e; }
    }
    result
}

struct Watch {
    fd: i32,
    backing: usize,
    source: Arc<dyn PollSource>,
    events: u32,
    data: u64,
    armed: bool,
}

struct EpollState {
    watches: Vec<Watch>,
    cursor: usize,
    closed: bool,
}

struct Epoll {
    state: Mutex<EpollState>,
    changed: WaitQueue,
}

impl PollSource for Epoll {
    fn poll(&self, interest: ReadyMask) -> ReadyMask {
        let state = self.state.lock();
        if state.closed { return POLLHUP; }
        if interest & (POLLIN | POLLRDNORM) != 0 && state.watches.iter().any(|w| {
            w.armed && ready_subset(w.source.poll(w.events | ALWAYS), w.events) != 0
        }) {
            interest & (POLLIN | POLLRDNORM)
        } else { 0 }
    }
    fn wait_queue(&self) -> &WaitQueue { &self.changed }
}

static EPOLLS: Mutex<BTreeMap<usize, Arc<Epoll>>> = Mutex::new(BTreeMap::new());
static NEXT_EPOLL: AtomicUsize = AtomicUsize::new(EPOLL_BASE);

pub fn is_epoll_fd(bfd: usize) -> bool { EPOLLS.lock().contains_key(&bfd) }

pub(super) fn epoll_poll_source(bfd: usize) -> Option<Arc<dyn PollSource>> {
    EPOLLS.lock().get(&bfd).map(|ep| Arc::clone(ep) as Arc<dyn PollSource>)
}

fn instance(fd: usize) -> Result<Arc<Epoll>, isize> {
    let bfd = user_fd_to_bfd(fd).ok_or(-9isize)?;
    EPOLLS.lock().get(&bfd).cloned().ok_or(-22)
}

pub fn sys_epoll_create(size: i32) -> isize {
    if size <= 0 { return -22; }
    sys_epoll_create1(0)
}

pub fn sys_epoll_create1(flags: u32) -> isize {
    if flags & !EPOLL_CLOEXEC != 0 { return -22; }
    let ep = Arc::new(Epoll {
        state: Mutex::new(EpollState { watches: Vec::new(), cursor: 0, closed: false }),
        changed: WaitQueue::new(),
    });
    let bfd;
    {
        let mut table = EPOLLS.lock();
        if table.len() >= MAX_EPOLLS { return -23; }
        bfd = match NEXT_EPOLL.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            n.checked_add(1).filter(|v| *v <= isize::MAX as usize)
        }) { Ok(n) => n, Err(_) => return -23 };
        table.insert(bfd, ep);
    }
    match crate::fs::process_fd::proc_fd_install_checked(
        crate::proc::scheduler::current_pid(), bfd, None, flags, None,
    ) { Ok(fd) => fd as isize, Err(e) => e }
}

/// Final backing release only; process_fd owns dup/fork reference accounting.
pub fn epoll_close(bfd: usize) {
    let ep = EPOLLS.lock().remove(&bfd);
    if let Some(ep) = ep {
        {
            let mut state = ep.state.lock();
            state.closed = true;
            state.watches.clear();
        }
        ep.changed.wake(POLLHUP);
    }
}

/// Called before a backing ID is freed or reused, on its final release.
pub fn on_backing_close(bfd: usize) {
    let instances: Vec<_> = EPOLLS.lock().values().cloned().collect();
    for ep in instances {
        let removed = {
            let mut state = ep.state.lock();
            let before = state.watches.len();
            state.watches.retain(|watch| watch.backing != bfd);
            before != state.watches.len()
        };
        if removed { ep.changed.wake(POLLIN); }
    }
}

/// Compatibility for raw-backing close dispatchers.
pub fn on_close(bfd: usize) { on_backing_close(bfd); }

pub fn sys_epoll_ctl(epfd: usize, operation: i32, fd: i32, event: usize) -> isize {
    if !(1..=3).contains(&operation) { return -22; }
    let ep = match instance(epfd) { Ok(ep) => ep, Err(e) => return e };
    if fd < 0 { return -9; }
    let Some(target) = crate::fs::process_fd::proc_fd_get(
        crate::proc::scheduler::current_pid(), fd as usize,
    ) else { return -9; };
    let backing = target.backing_fd;
    if user_fd_to_bfd(epfd) == Some(backing) { return -22; }
    // Nested epoll needs cycle/depth tracking and recursive wake forwarding.
    if is_epoll_fd(backing) { return -95; }
    let (events, data) = if operation == 2 { (0, 0) } else {
        // x86-64 Linux epoll_event is packed: u32 events + u64 data, 12 bytes.
        let mut bytes = [0u8; 12];
        if copy_from_user(&mut bytes, event).is_err() { return -14; }
        let events = u32::from_ne_bytes(bytes[..4].try_into().unwrap());
        if events & (0x8000_0000 | 0x1000_0000 | 0x2000_0000) != 0 { return -95; }
        if events & !EPOLL_EVENTS != 0 { return -22; }
        (events, u64::from_ne_bytes(bytes[4..].try_into().unwrap()))
    };
    let mut state = ep.state.lock();
    if state.closed { return -9; }
    let found = state.watches.iter().position(|w| w.fd == fd && w.backing == backing);
    match (operation, found) {
        (1, Some(_)) => return -17,
        (2 | 3, None) => return -2,
        (1, None) => {
            if state.watches.len() >= MAX_POLL_FDS { return -28; }
            // Regular files cannot be registered in Linux epoll.
            if crate::fs::vfs::is_raw_fd(backing) && !crate::fs::eventfd::is_eventfd(backing) {
                return -1;
            }
            let Some(source) = backing_poll_source(backing) else { return -95; };
            state.watches.push(Watch { fd, backing, source, events, data, armed: true });
        },
        (2, Some(i)) => { state.watches.remove(i); },
        (3, Some(i)) => {
            let watch = &mut state.watches[i];
            watch.events = events;
            watch.data = data;
            watch.armed = true;
        },
        _ => return -22,
    }
    drop(state);
    ep.changed.wake(POLLIN);
    0
}

pub fn sys_epoll_wait(epfd: usize, out: usize, maxevents: i32, timeout: i32) -> isize {
    if maxevents <= 0 || maxevents as usize > MAX_POLL_FDS { return -22; }
    let ep = match instance(epfd) { Ok(ep) => ep, Err(e) => return e };
    if !validate_user_ptr(out, maxevents as usize * 12) { return -14; }
    let deadline = millis_deadline(timeout);
    let cancel = crate::proc::scheduler::task_cancel_token(crate::proc::scheduler::current_pid());
    loop {
        let mut sources = Vec::new();
        {
            let mut state = ep.state.lock();
            if state.closed { return -9; }
            ep.changed.clear(POLLIN);
            let len = state.watches.len();
            let mut bytes = Vec::new();
            let mut fired = Vec::new();
            for offset in 0..len {
                let i = (state.cursor + offset) % len;
                let w = &state.watches[i];
                if !w.armed { continue; }
                sources.push((Arc::clone(&w.source), (w.events & !EPOLLONESHOT) | ALWAYS));
                let ready = ready_subset(w.source.poll((w.events & !EPOLLONESHOT) | ALWAYS), w.events);
                if ready != 0 && fired.len() < maxevents as usize {
                    bytes.extend_from_slice(&ready.to_ne_bytes());
                    bytes.extend_from_slice(&w.data.to_ne_bytes());
                    fired.push(i);
                }
            }
            if !fired.is_empty() {
                if copy_to_user(out, &bytes).is_err() { return -14; }
                // EFAULT must not consume one-shot arming or advance fairness.
                for i in &fired {
                    if state.watches[*i].events & EPOLLONESHOT != 0 { state.watches[*i].armed = false; }
                }
                state.cursor = (fired[fired.len() - 1] + 1) % len;
                return fired.len() as isize;
            }
        }
        if deadline.map_or(false, |d| now() >= d) { return 0; }
        // Changes are also observed by the bounded rescan, including ADD while
        // there are no watched sources. No unbounded empty-instance sleep.
        let change_source: Arc<dyn PollSource> = Arc::clone(&ep) as Arc<dyn PollSource>;
        sources.push((change_source, POLLIN | POLLHUP));
        if let Err(e) = sleep_sources(&sources, cancel.as_deref(), deadline) { return e; }
    }
}

pub fn sys_epoll_pwait(
    epfd: usize, out: usize, maxevents: i32, timeout: i32, mask: usize, mask_len: usize,
) -> isize {
    if let Err(e) = check_sigmask(mask, mask_len) { return e; }
    sys_epoll_wait(epfd, out, maxevents, timeout)
}
