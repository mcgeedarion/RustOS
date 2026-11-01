//! System V shared memory.
//!
//! ## API
//!
//! | Function                          | Returns      |
//! |-----------------------------------|--------------|
//! | `shmget(key, size, shmflg)`       | `shmid`      |
//! | `shmat(shmid, shmaddr, shmflg)`   | `*void` (VA) |
//! | `shmdt(shmaddr)`                  | `0`          |
//! | `shmctl(shmid, cmd, buf)`         | varies       |
//!
//! ## Implementation
//!
//! Physical pages are allocated via `pmm::alloc_frames(n)` when the segment
//! is created.  `shmat` maps the frames into the calling process's address
//! space using `vmm::map_range`.  `shmdt` removes the mapping but does not
//! free the frames (the last detach is tracked by `nattch`).
//!
//! The segment is freed when `IPC_RMID` has been issued AND `nattch == 0`.
//!
//! ## Address hint
//!
//! If `shmaddr == NULL`, the kernel chooses a free region.  The current
//! implementation uses a bump allocator in the process's mmap region
//! (integration point: replace with `vmm::find_free_region`).

extern crate alloc;
use crate::ipc::{
    check_perm, IpcPerm, IPC_CREAT, IPC_EXCL, IPC_PRIVATE, IPC_RMID, IPC_SET, IPC_STAT,
};
use alloc::{collections::BTreeMap, vec::Vec};
use spin::Mutex;

pub const SHM_RDONLY: i32 = 0o10000;
pub const SHM_RND: i32 = 0o20000;
pub const SHM_EXEC: i32 = 0o100000;

const PAGE_SIZE: usize = 4096;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct ShmidDs {
    pub shm_perm: IpcPerm,
    pub shm_segsz: usize,
    pub shm_atime: i64,
    pub shm_dtime: i64,
    pub shm_ctime: i64,
    pub shm_cpid: i32,
    pub shm_lpid: i32,
    pub shm_nattch: u64,
    pub _reserved: [u64; 2],
}

const _: () = assert!(core::mem::size_of::<ShmidDs>() == 112);

/// Kernel-side shared memory segment.
struct ShmSegment {
    perm: IpcPerm,
    size: usize,
    frames: Arc<crate::mm::vmm::SharedFrames>,
    nattch: usize,      // number of current attaches
    cpid: u32,
    lpid: u32,
    removed: bool, // IPC_RMID was issued; free on last detach
    atime: i64,
    dtime: i64,
    ctime: i64,
}

use alloc::sync::Arc;
static SHM_TABLE: Mutex<BTreeMap<i32, Arc<Mutex<ShmSegment>>>> = Mutex::new(BTreeMap::new());
static NEXT_ID: Mutex<i32> = Mutex::new(1);

/// Per-process attach table: `(shmid, va)` pairs.
static ATTACH_TABLE: Mutex<BTreeMap<(usize, usize), i32>> = Mutex::new(BTreeMap::new());

#[inline]
fn alloc_id() -> i32 {
    let mut id = NEXT_ID.lock();
    let v = *id;
    *id += 1;
    v
}

pub fn shmget(key: i32, size: usize, shmflg: i32) -> Result<i32, isize> {
    let pages = size.checked_add(PAGE_SIZE - 1).ok_or(-22isize)? / PAGE_SIZE;
    let mut tbl = SHM_TABLE.lock();
    let (uid, gid) = super::current_ids();

    if key == IPC_PRIVATE {
        if size == 0 { return Err(-22); }
        let frames = alloc_frames(pages)?;
        let id = alloc_id();
        tbl.insert(
            id,
            Arc::new(Mutex::new(ShmSegment {
                perm: IpcPerm::new(IPC_PRIVATE, uid, gid, (shmflg & 0o777) as u16),
                size,
                frames,
                nattch: 0,
                cpid: crate::proc::scheduler::current_pid() as u32,
                lpid: 0,
                removed: false,
                atime: 0,
                dtime: 0,
                ctime: super::now_secs(),
            })),
        );
        return Ok(id);
    }

    if let Some((&id, arc)) = tbl.iter().find(|(_, s)| {
        let s = s.lock();
        !s.removed && s.perm.key == key
    }) {
        if shmflg & IPC_CREAT != 0 && shmflg & IPC_EXCL != 0 {
            return Err(-17);
        } // EEXIST
        let seg = arc.lock();
        if size > seg.size { return Err(-22); }
        if !check_perm(&seg.perm, uid, gid, (shmflg as u16 >> 6) & 6) {
            return Err(-13);
        }
        return Ok(id);
    }
    if shmflg & IPC_CREAT == 0 {
        return Err(-2);
    } // ENOENT

    if size == 0 { return Err(-22); }
    let frames = alloc_frames(pages)?;
    let id = alloc_id();
    tbl.insert(
        id,
        Arc::new(Mutex::new(ShmSegment {
            perm: IpcPerm::new(key, uid, gid, (shmflg & 0o777) as u16),
            size,
            frames,
            nattch: 0,
            cpid: crate::proc::scheduler::current_pid() as u32,
            lpid: 0,
            removed: false,
            atime: 0,
            dtime: 0,
            ctime: super::now_secs(),
        })),
    );
    Ok(id)
}

pub fn shmat(shmid: i32, shmaddr: usize, shmflg: i32) -> Result<usize, isize> {
    let arc = {
        let tbl = SHM_TABLE.lock();
        tbl.get(&shmid).cloned().ok_or(-22isize)? // EINVAL
    };
    let mut seg = arc.lock();
    let (uid, gid) = super::current_ids();

    if !check_perm(
        &seg.perm,
        uid,
        gid,
        if shmflg & SHM_RDONLY != 0 { 0o4 } else { 0o6 },
    ) {
        return Err(-13); // EACCES
    }

    let mut prot = if shmflg & SHM_RDONLY != 0 {
        crate::mm::mmap::PROT_READ
    } else {
        crate::mm::mmap::PROT_READ | crate::mm::mmap::PROT_WRITE
    };
    if shmflg & SHM_EXEC != 0 {
        if !check_perm(&seg.perm, uid, gid, 1) { return Err(-13); }
        prot |= crate::mm::mmap::PROT_EXEC;
    }

    let hint = if shmaddr == 0 {
        0
    } else {
        if shmflg & SHM_RND != 0 {
            shmaddr & !(65536 - 1)
        } else {
            shmaddr
        }
    };

    let va = crate::mm::vmm::map_shared(
        hint, seg.frames.frames().len() * PAGE_SIZE, prot, Arc::clone(&seg.frames),
    )?;

    seg.nattch += 1;
    seg.lpid = crate::proc::scheduler::current_pid() as u32;
    seg.atime = super::now_secs();

    let pid = crate::proc::scheduler::current_pid();
    ATTACH_TABLE.lock().insert((pid, va), shmid);
    Ok(va)
}

pub fn shmdt(shmaddr: usize) -> Result<(), isize> {
    let pid = crate::proc::scheduler::current_pid();
    let shmid = *ATTACH_TABLE.lock().get(&(pid, shmaddr)).ok_or(-22isize)?;
    crate::mm::vmm::unmap_range(shmaddr)?;
    ATTACH_TABLE.lock().remove(&(pid, shmaddr));

    let should_free = {
        let tbl = SHM_TABLE.lock();
        let arc = tbl.get(&shmid).ok_or(-22isize)?;
        let mut seg = arc.lock();
        seg.nattch = seg.nattch.saturating_sub(1);
        seg.lpid = pid as u32;
        seg.dtime = super::now_secs();
        seg.removed && seg.nattch == 0
    };

    if should_free {
        SHM_TABLE.lock().remove(&shmid);
    }
    Ok(())
}

pub fn shmctl(shmid: i32, cmd: i32) -> Result<ShmidDs, isize> {
    let arc = {
        let tbl = SHM_TABLE.lock();
        tbl.get(&shmid).cloned().ok_or(-22isize)?
    };
    match cmd {
        c if c == IPC_RMID => {
            let mut seg = arc.lock();
            if !super::owner_perm(&seg.perm) { return Err(-1); }
            seg.removed = true;
            if seg.nattch == 0 {
                drop(seg);
                SHM_TABLE.lock().remove(&shmid);
            }
            Ok(ShmidDs::default())
        },
        c if c == IPC_STAT => {
            let seg = arc.lock();
            let (uid, gid) = super::current_ids();
            if !check_perm(&seg.perm, uid, gid, 4) { return Err(-13); }
            let mut perm = seg.perm;
            if seg.removed { perm.mode |= 0o1000; }
            Ok(ShmidDs {
                shm_perm: perm,
                shm_segsz: seg.size,
                shm_atime: seg.atime,
                shm_dtime: seg.dtime,
                shm_ctime: seg.ctime,
                shm_cpid: seg.cpid as i32,
                shm_lpid: seg.lpid as i32,
                shm_nattch: seg.nattch as u64,
                _reserved: [0; 2],
            })
        },
        _ => Err(-22),
    }
}

pub fn shmctl_set(shmid: i32, ds: ShmidDs) -> Result<(), isize> {
    let arc = SHM_TABLE.lock().get(&shmid).cloned().ok_or(-22isize)?;
    let mut seg = arc.lock();
    if !super::owner_perm(&seg.perm) { return Err(-1); }
    seg.perm.uid = ds.shm_perm.uid;
    seg.perm.gid = ds.shm_perm.gid;
    seg.perm.mode = (seg.perm.mode & !0o777) | (ds.shm_perm.mode & 0o777);
    seg.ctime = super::now_secs();
    Ok(())
}

fn alloc_frames(n: usize) -> Result<Arc<crate::mm::vmm::SharedFrames>, isize> {
    let mut frames = Vec::with_capacity(n);
    for _ in 0..n {
        let Some(pa) = crate::mm::pmm::alloc_page() else {
            for pa in frames { crate::mm::pmm::free_page(pa); }
            return Err(-12);
        };
        unsafe { core::ptr::write_bytes(pa as *mut u8, 0, PAGE_SIZE); }
        frames.push(pa);
    }
    Ok(Arc::new(crate::mm::vmm::SharedFrames::from_owned(frames)))
}

/// Copy attachment accounting after the child's shared VMAs have been cloned.
pub fn fork_attach(parent: usize, child: usize) {
    let attachments: Vec<_> = ATTACH_TABLE.lock().iter()
        .filter(|((pid, _), _)| *pid == parent)
        .map(|(&(_, va), &id)| (va, id)).collect();
    for (va, id) in attachments {
        if let Some(arc) = SHM_TABLE.lock().get(&id).cloned() {
            arc.lock().nattch += 1;
            ATTACH_TABLE.lock().insert((child, va), id);
        }
    }
}

/// Exit releases VMAs separately; only SysV attachment accounting is removed here.
pub fn cleanup_pid(pid: usize) {
    let attachments: Vec<_> = {
        let mut table = ATTACH_TABLE.lock();
        let ids = table.iter().filter(|((owner, _), _)| *owner == pid)
            .map(|(_, &id)| id).collect();
        table.retain(|(owner, _), _| *owner != pid);
        ids
    };
    for id in attachments {
        let arc = SHM_TABLE.lock().get(&id).cloned();
        if let Some(arc) = arc {
            let remove = {
                let mut seg = arc.lock();
                seg.nattch = seg.nattch.saturating_sub(1);
                seg.lpid = pid as u32;
                seg.dtime = super::now_secs();
                seg.removed && seg.nattch == 0
            };
            if remove { SHM_TABLE.lock().remove(&id); }
        }
    }
}
