//! Production SysV IPC/ring logic with mocked process, user-copy, and VM services.
//! This exercises state/ABI/lifetime contracts, not hardware paging or scheduling.
extern crate alloc;
extern crate self as spin;

use std::sync::atomic::{AtomicUsize, Ordering};
pub struct Mutex<T>(std::sync::Mutex<T>);
impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self { Self(std::sync::Mutex::new(value)) }
    pub fn lock(&self) -> std::sync::MutexGuard<'_, T> { self.0.lock().unwrap() }
}
static CURRENT: AtomicUsize = AtomicUsize::new(1);
static UID: AtomicUsize = AtomicUsize::new(1000);
static GID: AtomicUsize = AtomicUsize::new(1000);
pub mod proc {
    pub struct Creds { pub euid: u32, pub egid: u32 }
    pub fn current_creds() -> Creds {
        Creds { euid: crate::UID.load(crate::Ordering::SeqCst) as u32,
                egid: crate::GID.load(crate::Ordering::SeqCst) as u32 }
    }
    pub mod scheduler {
        pub fn current_pid() -> usize { crate::CURRENT.load(crate::Ordering::SeqCst) }
        pub fn schedule() { panic!("unexpected blocking operation in host regression"); }
    }
}
pub mod time {
    pub fn monotonic_ns() -> u64 { 10_000_000_000 }
    pub fn realtime_offset_ns() -> i64 { 1_700_000_000_000_000_000 }
}
pub mod uaccess {
    pub fn copy_to_user(va: usize, bytes: &[u8]) -> Result<(), ()> {
        if va < 4096 { return Err(()); }
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), va as *mut u8, bytes.len()); }
        Ok(())
    }
    pub fn copy_from_user(bytes: &mut [u8], va: usize) -> Result<(), ()> {
        if va < 4096 { return Err(()); }
        unsafe { std::ptr::copy_nonoverlapping(va as *const u8, bytes.as_mut_ptr(), bytes.len()); }
        Ok(())
    }
}
pub mod mm {
    pub mod pmm {
        use std::collections::BTreeMap;
        use std::alloc::{alloc_zeroed, dealloc, Layout};
        static BLOCKS: crate::Mutex<BTreeMap<usize, (usize, usize)>> = crate::Mutex::new(BTreeMap::new());
        pub fn alloc_pages_contig(n: usize) -> Option<usize> {
            let layout = Layout::from_size_align(n * 4096, 4096).ok()?;
            let pa = unsafe { alloc_zeroed(layout) } as usize;
            if pa == 0 { return None; }
            BLOCKS.lock().insert(pa, (n, n));
            Some(pa)
        }
        pub fn alloc_page() -> Option<usize> { alloc_pages_contig(1) }
        pub fn free_page(pa: usize) {
            let mut blocks = BLOCKS.lock();
            let (&base, &(pages, refs)) = blocks.iter()
                .find(|(&base, &(n, _))| pa >= base && pa < base + n * 4096).unwrap();
            if refs == 1 {
                blocks.remove(&base);
                unsafe { dealloc(base as *mut u8, Layout::from_size_align(pages * 4096, 4096).unwrap()); }
            } else { blocks.get_mut(&base).unwrap().1 -= 1; }
        }
        pub fn live_pages() -> usize { BLOCKS.lock().values().map(|(_, refs)| refs).sum() }
    }
    pub mod mmap {
        pub const PROT_READ: u32 = 1;
        pub const PROT_WRITE: u32 = 2;
        pub const PROT_EXEC: u32 = 4;
        pub const MAP_SHARED: u32 = 1;
    }
    pub mod vmm {
        use std::sync::Arc;
        use std::collections::BTreeMap;
        #[derive(Debug)]
        pub struct SharedFrames { frames: Vec<usize> }
        impl SharedFrames {
            pub fn from_owned(frames: Vec<usize>) -> Self { Self { frames } }
            pub fn frames(&self) -> &[usize] { &self.frames }
        }
        impl Drop for SharedFrames {
            fn drop(&mut self) { for &pa in &self.frames { super::pmm::free_page(pa); } }
        }
        static MAPS: crate::Mutex<BTreeMap<(usize, usize), Arc<SharedFrames>>> =
            crate::Mutex::new(BTreeMap::new());
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x10000);
        pub fn map_shared(hint: usize, size: usize, _: u32, frames: Arc<SharedFrames>) -> Result<usize, isize> {
            let va = if hint != 0 { hint } else { NEXT.fetch_add(size + 4096, crate::Ordering::SeqCst) };
            MAPS.lock().insert((crate::proc::scheduler::current_pid(), va), frames);
            Ok(va)
        }
        pub fn unmap_range(va: usize) -> Result<(), isize> {
            MAPS.lock().remove(&(crate::proc::scheduler::current_pid(), va)).ok_or(-22)?;
            Ok(())
        }
    }
}
pub mod sync {
    pub mod wait_queue {
        pub type ReadyMask = u32;
        pub struct WaitQueue(std::sync::atomic::AtomicU32);
        impl WaitQueue {
            pub const fn new() -> Self { Self(std::sync::atomic::AtomicU32::new(0)) }
            pub fn wake(&self, bits: u32) { self.0.fetch_or(bits, crate::Ordering::SeqCst); }
        }
    }
}
pub mod fs {
    pub mod process_fd {
        pub fn proc_fd_backing(_: usize, _: usize) -> isize { -9 }
    }
    pub mod vfs_uring {
        pub fn uring_fd_to_ring(_: usize) -> Option<usize> { None }
    }
}
#[path = "../../src/ipc/abi.rs"] pub mod ipc_abi;
#[path = "../../src/ipc/sem.rs"] pub mod real_sem;
#[path = "../../src/ipc/msg.rs"] pub mod real_msg;
#[path = "../../src/ipc/shm.rs"] pub mod real_shm;
#[path = "../../src/io_uring/sqe.rs"] pub mod real_sqe;
#[path = "../../src/io_uring/ring.rs"] pub mod real_ring;
pub mod ipc {
    pub use crate::ipc_abi::*;
    pub use crate::{real_sem as sem, real_msg as msg, real_shm as shm};
}
pub use ipc_abi::{current_ids, owner_perm, now_secs};
pub mod io_uring { pub use crate::{real_sqe as sqe, real_ring as ring}; }

#[test]
fn ipc_and_ring_regressions() {
    use ipc::{sem, msg, shm, *};
    use io_uring::{ring, sqe::Sqe};
    assert_eq!(std::mem::size_of::<IpcPerm>(), 48);
    assert_eq!(std::mem::offset_of!(IpcPerm, seq), 24);
    assert_eq!(std::mem::size_of::<shm::ShmidDs>(), 112);
    assert_eq!(std::mem::size_of::<msg::MsqidDs>(), 120);
    assert_eq!(std::mem::size_of::<sem::SemBuf>(), 6);
    assert_eq!(std::mem::size_of::<ring::IoUringSqe>(), 64);
    assert_eq!(std::mem::size_of::<ring::IoUringCqe>(), 16);

    let semid = sem::semget(IPC_PRIVATE, 2, 0o600).unwrap();
    assert_eq!(sem::semctl(semid, 0, sem::SETVAL, 1), Ok(0));
    let twice = [
        sem::SemBuf { sem_num: 0, sem_op: -1, sem_flg: 0 },
        sem::SemBuf { sem_num: 0, sem_op: -1, sem_flg: IPC_NOWAIT as i16 },
    ];
    assert_eq!(sem::semop(semid, &twice), Err(-11));
    assert_eq!(sem::semctl(semid, 0, sem::GETVAL, 0), Ok(1));
    let sequence = [
        sem::SemBuf { sem_num: 0, sem_op: 1, sem_flg: 0 },
        sem::SemBuf { sem_num: 0, sem_op: -2, sem_flg: IPC_NOWAIT as i16 },
    ];
    assert_eq!(sem::semop(semid, &sequence), Ok(()));
    assert_eq!(sem::semctl(semid, 0, sem::GETVAL, 0), Ok(0));
    assert_eq!(sem::semctl(semid, 0, sem::SETVAL, 32768), Err(-34));
    let initial: [u16; 2] = [7, 9];
    assert_eq!(sem::semctl(semid, 0, sem::SETALL, initial.as_ptr() as u64), Ok(0));
    let mut returned = [0u16; 2];
    assert_eq!(sem::semctl(semid, 0, sem::GETALL, returned.as_mut_ptr() as u64), Ok(0));
    assert_eq!(returned, initial);
    assert_eq!(sem::semctl(semid, 0, sem::GETALL, 0), Err(-14));
    let undo = [sem::SemBuf { sem_num: 0, sem_op: -3, sem_flg: 0x1000 }];
    assert_eq!(sem::semop(semid, &undo), Ok(()));
    sem::undo_on_exit(1);
    assert_eq!(sem::semctl(semid, 0, sem::GETVAL, 0), Ok(7));
    sem::undo_on_exit(1);
    assert_eq!(sem::semctl(semid, 0, sem::GETVAL, 0), Ok(7));
    assert_eq!(sem::semctl(semid, 0, IPC_RMID, 0), Ok(0));

    let queue = msg::msgget(IPC_PRIVATE, 0o600).unwrap();
    msg::msgsnd(queue, 9, vec![1, 2, 3], 0).unwrap();
    msg::msgsnd(queue, 2, vec![4, 5], 0).unwrap();
    let mut stat = msg::msgctl(queue, IPC_STAT).unwrap();
    assert_eq!((stat.msg_qnum, stat.msg_cbytes), (2, 5));
    assert_eq!((stat.msg_perm.uid, stat.msg_perm.gid), (1000, 1000));
    assert_eq!(msg::msgrcv(queue, 1, 9, IPC_NOWAIT), Err(-7));
    assert_eq!(msg::msgctl(queue, IPC_STAT).unwrap().msg_qnum, 2);
    assert_eq!(msg::msgrcv(queue, 8, i64::MIN, 0), Ok((2, vec![4, 5])));
    assert_eq!(msg::msgrcv(queue, 1, 9, msg::MSG_NOERROR), Ok((9, vec![1])));
    assert_eq!(msg::msgrcv(queue, 8, 0, IPC_NOWAIT), Err(-42));
    stat.msg_qbytes = 2;
    stat.msg_perm.mode = 0o640;
    msg::msgctl_set(queue, stat).unwrap();
    assert_eq!(msg::msgctl(queue, IPC_STAT).unwrap().msg_qbytes, 2);
    assert_eq!(msg::msgsnd(queue, 1, vec![1, 2, 3], IPC_NOWAIT), Err(-11));
    UID.store(2000, Ordering::SeqCst);
    assert!(matches!(msg::msgctl(queue, IPC_STAT), Err(-13)));
    assert!(matches!(msg::msgctl(queue, IPC_RMID), Err(-1)));
    UID.store(1000, Ordering::SeqCst);
    msg::msgctl(queue, IPC_RMID).unwrap();

    let baseline = mm::pmm::live_pages();
    let segment = shm::shmget(IPC_PRIVATE, 4097, 0o600).unwrap();
    assert_eq!(mm::pmm::live_pages(), baseline + 2);
    assert_eq!(shm::shmctl(segment, IPC_STAT).unwrap().shm_segsz, 4097);
    let va = shm::shmat(segment, 0, 0).unwrap();
    assert_eq!(shm::shmctl(segment, IPC_STAT).unwrap().shm_nattch, 1);
    shm::shmctl(segment, IPC_RMID).unwrap();
    assert_eq!(mm::pmm::live_pages(), baseline + 2);
    shm::shmdt(va).unwrap();
    assert_eq!(mm::pmm::live_pages(), baseline);
    assert!(matches!(shm::shmctl(segment, IPC_STAT), Err(-22)));
    let unattached = shm::shmget(IPC_PRIVATE, 1, 0o600).unwrap();
    shm::shmctl(unattached, IPC_RMID).unwrap();
    assert_eq!(mm::pmm::live_pages(), baseline);

    let ringid = ring::alloc_ring(1, 3).unwrap();
    let pinned = ring::with_ring(ringid, |r| r.sq_frames.clone()).unwrap();
    ring::with_ring(ringid, |r| {
        assert_eq!(r.entries, 4);
        assert_eq!(r.build_params().sq_off.array, 64);
        assert_eq!(r.build_params().cq_off.cqes, 64);
        assert_eq!(r.build_params().features & ring::IORING_FEAT_NODROP, 0);
        for token in 0..4 { assert!(r.push_sqe(Sqe::nop(token).into())); }
        assert!(!r.push_sqe(Sqe::nop(4).into()));
        assert_eq!(r.drain_sq_limit(0).len(), 0);
        let first = r.drain_sq_limit(2);
        assert_eq!(first.iter().map(|s| s.user_data).collect::<Vec<_>>(), vec![0, 1]);
        let rest = r.drain_sq();
        assert_eq!(rest.iter().map(|s| s.user_data).collect::<Vec<_>>(), vec![2, 3]);
        for token in 0..4 { assert!(r.post_cqe(token, token as i32, 0)); }
        assert!(!r.post_cqe(5, 5, 0));
        assert_eq!(r.cq_available(), 4);
        let head = unsafe { &*(r.cq_pa as *const std::sync::atomic::AtomicU32) };
        head.store(2, Ordering::Release);
        assert!(r.post_cqe(6, 6, 0));
        assert_eq!(r.cq_available(), 3);
    }).unwrap();
    ring::free_ring(ringid);
    assert_eq!(mm::pmm::live_pages(), baseline + 1);
    drop(pinned);
    assert_eq!(mm::pmm::live_pages(), baseline);
    let big = ring::alloc_ring(1, ring::MAX_ENTRIES).unwrap();
    ring::with_ring(big, |r| {
        assert_eq!(r.sqe_frames.frames().len(), 64);
        assert!(r.push_sqe(Sqe::nop(42).into()));
        assert_eq!(r.drain_sq()[0].user_data, 42);
    }).unwrap();
    ring::free_ring(big);
    assert_eq!(mm::pmm::live_pages(), baseline);
    let accept = Sqe::accept(3, 0x2000, 0x3000, 0, 7);
    let decoded = Sqe::from(ring::IoUringSqe::from(accept));
    assert_eq!((decoded.off, decoded.addr, decoded.user_data), (0x3000, 0x2000, 7));
    assert_eq!(Sqe::connect(3, 0x2000, 16, 8).off, 16);
}
