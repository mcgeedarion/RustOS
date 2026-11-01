//! System V semaphores.
//!
//! Implements `semget`, `semop`, and the currently wired `semctl` commands.
//! `semop` applies operation arrays atomically. If an operation would block,
//! the caller is recorded in the semaphore wait counters and the scheduler is
//! yielded before retrying.

extern crate alloc;

use crate::ipc::{
    check_perm, IpcPerm, IPC_CREAT, IPC_EXCL, IPC_PRIVATE, IPC_RMID, IPC_SET, IPC_STAT,
};
use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};
use spin::Mutex;

pub const GETVAL: i32 = 12;
pub const SETVAL: i32 = 16;
pub const GETALL: i32 = 13;
pub const SETALL: i32 = 17;
pub const GETPID: i32 = 11;
pub const GETNCNT: i32 = 14;
pub const GETZCNT: i32 = 15;
pub const SEMOPM: usize = 500;
pub const SEMMSL: usize = 32000;
pub const SEMVMX: i32 = 32767;

const IPC_NOWAIT: i16 = 0x0800;
const SEM_UNDO: i16 = 0x1000;

/// One semaphore within a set.
#[derive(Clone)]
struct Sem {
    val: i16,
    pid: u32,
    ncnt: u32,
    zcnt: u32,
}

/// Kernel-side semaphore set.
struct SemSet {
    perm: IpcPerm,
    sems: Vec<Sem>,
    otime: i64,
    ctime: i64,
}

static SEM_TABLE: Mutex<BTreeMap<i32, Arc<Mutex<SemSet>>>> = Mutex::new(BTreeMap::new());
static NEXT_ID: Mutex<i32> = Mutex::new(1);
static UNDO: Mutex<BTreeMap<(usize, i32, usize), i32>> = Mutex::new(BTreeMap::new());

#[derive(Clone, Copy)]
enum WaitKind {
    NonZero,
    Zero,
}

#[inline]
fn alloc_id() -> i32 {
    let mut id = NEXT_ID.lock();
    let v = *id;
    *id += 1;
    v
}

fn blank_sem() -> Sem {
    Sem {
        val: 0,
        pid: 0,
        ncnt: 0,
        zcnt: 0,
    }
}

pub fn semget(key: i32, nsems: i32, semflg: i32) -> Result<i32, isize> {
    if nsems < 0 || nsems as usize > SEMMSL {
        return Err(-22);
    }

    let mut tbl = SEM_TABLE.lock();
    let (uid, gid) = super::current_ids();

    if key == IPC_PRIVATE {
        if nsems == 0 { return Err(-22); }
        let id = alloc_id();

        tbl.insert(
            id,
            Arc::new(Mutex::new(SemSet {
                perm: IpcPerm::new(IPC_PRIVATE, uid, gid, (semflg & 0o777) as u16),
                sems: (0..nsems as usize).map(|_| blank_sem()).collect(),
                otime: 0,
                ctime: super::now_secs(),
            })),
        );

        return Ok(id);
    }

    if let Some((&id, arc)) = tbl.iter().find(|(_, s)| s.lock().perm.key == key) {
        if semflg & IPC_CREAT != 0 && semflg & IPC_EXCL != 0 {
            return Err(-17);
        }

        let set = arc.lock();
        if nsems as usize > set.sems.len() { return Err(-22); }
        if !check_perm(&set.perm, uid, gid, (semflg as u16 >> 6) & 6) {
            return Err(-13);
        }
        return Ok(id);
    }

    if semflg & IPC_CREAT == 0 {
        return Err(-2);
    }
    if nsems == 0 { return Err(-22); }

    let id = alloc_id();

    tbl.insert(
        id,
        Arc::new(Mutex::new(SemSet {
            perm: IpcPerm::new(key, uid, gid, (semflg & 0o777) as u16),
            sems: (0..nsems as usize).map(|_| blank_sem()).collect(),
            otime: 0,
            ctime: super::now_secs(),
        })),
    );

    Ok(id)
}

/// One element of the `sembuf` array passed to `semop(2)`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SemBuf {
    pub sem_num: u16,
    pub sem_op: i16,
    pub sem_flg: i16,
}
pub type Sembuf = SemBuf;
const _: () = assert!(core::mem::size_of::<SemBuf>() == 6);

fn blocking_ops(set: &SemSet, sops: &[SemBuf]) -> Result<Vec<(usize, WaitKind)>, isize> {
    let mut waiters = Vec::new();
    let mut values: Vec<i32> = set.sems.iter().map(|s| s.val as i32).collect();

    for sop in sops {
        let idx = sop.sem_num as usize;

        if idx >= set.sems.len() {
            return Err(-22);
        }

        if sop.sem_flg & !(IPC_NOWAIT | SEM_UNDO) != 0 { return Err(-22); }
        let val = values[idx];

        if sop.sem_op < 0 && (val + sop.sem_op as i32) < 0 {
            if sop.sem_flg & IPC_NOWAIT != 0 { return Err(-11); }
            waiters.push((idx, WaitKind::NonZero));
            break;
        } else if sop.sem_op == 0 && val != 0 {
            if sop.sem_flg & IPC_NOWAIT != 0 { return Err(-11); }
            waiters.push((idx, WaitKind::Zero));
            break;
        }
        values[idx] += sop.sem_op as i32;
        if values[idx] > SEMVMX { return Err(-34); }
    }

    Ok(waiters)
}

fn adjust_wait_counts(set: &mut SemSet, waiters: &[(usize, WaitKind)], add: bool) {
    for &(idx, kind) in waiters {
        let Some(sem) = set.sems.get_mut(idx) else {
            continue;
        };

        match (kind, add) {
            (WaitKind::NonZero, true) => sem.ncnt = sem.ncnt.saturating_add(1),
            (WaitKind::NonZero, false) => sem.ncnt = sem.ncnt.saturating_sub(1),
            (WaitKind::Zero, true) => sem.zcnt = sem.zcnt.saturating_add(1),
            (WaitKind::Zero, false) => sem.zcnt = sem.zcnt.saturating_sub(1),
        }
    }
}

fn yield_to_scheduler() {
    crate::proc::scheduler::schedule();
}

pub fn semop(semid: i32, sops: &[SemBuf]) -> Result<(), isize> {
    if sops.is_empty() { return Err(-22); }
    if sops.len() > SEMOPM { return Err(-7); }
    loop {
        let arc = {
            let tbl = SEM_TABLE.lock();
            tbl.get(&semid).cloned().ok_or(-43isize)?
        };

        let mut set = arc.lock();

        let (uid, gid) = super::current_ids();
        let need = if sops.iter().all(|s| s.sem_op == 0) { 4 } else { 2 };
        if !check_perm(&set.perm, uid, gid, need) {
            return Err(-13);
        }

        let waiters = blocking_ops(&set, sops)?;

        if !waiters.is_empty() {
            adjust_wait_counts(&mut set, &waiters, true);
            drop(set);

            yield_to_scheduler();

            let mut set = arc.lock();
            adjust_wait_counts(&mut set, &waiters, false);
            continue;
        }

        let pid = crate::proc::scheduler::current_pid() as u32;
        let mut undo = UNDO.lock();
        let mut deltas = BTreeMap::<usize, i32>::new();
        for sop in sops {
            if sop.sem_flg & SEM_UNDO != 0 {
                *deltas.entry(sop.sem_num as usize).or_insert(0) -= sop.sem_op as i32;
            }
        }
        for (&idx, &delta) in &deltas {
            let old = undo.get(&(pid as usize, semid, idx)).copied().unwrap_or(0);
            if (old as i64 + delta as i64).unsigned_abs() > SEMVMX as u64 {
                return Err(-34);
            }
        }
        for (idx, delta) in deltas {
            *undo.entry((pid as usize, semid, idx)).or_insert(0) += delta;
        }

        for sop in sops {
            let s = &mut set.sems[sop.sem_num as usize];
            s.val = (s.val as i32 + sop.sem_op as i32) as i16;
            s.pid = pid;

        }
        set.otime = super::now_secs();

        return Ok(());
    }
}

pub fn semctl(semid: i32, semnum: i32, cmd: i32, arg: u64) -> Result<i32, isize> {
    let arc = {
        let tbl = SEM_TABLE.lock();
        tbl.get(&semid).cloned().ok_or(-43isize)?
    };

    let mut set = arc.lock();
    let n = semnum as usize;
    let (uid, gid) = super::current_ids();
    if cmd == IPC_RMID || cmd == IPC_SET {
        if !super::owner_perm(&set.perm) { return Err(-1); }
    } else {
        let need = if cmd == SETVAL || cmd == SETALL { 2 } else { 4 };
        if !check_perm(&set.perm, uid, gid, need) { return Err(-13); }
    }

    match cmd {
        c if c == GETVAL => {
            if n >= set.sems.len() {
                return Err(-22);
            }

            Ok(set.sems[n].val as i32)
        },
        c if c == SETVAL => {
            if n >= set.sems.len() {
                return Err(-22);
            }

            if arg > SEMVMX as u64 { return Err(-34); }
            set.sems[n].val = arg as i16;
            set.sems[n].pid = crate::proc::scheduler::current_pid() as u32;
            set.ctime = super::now_secs();
            UNDO.lock().retain(|&(_, id, idx), _| id != semid || idx != n);
            Ok(0)
        },
        c if c == GETPID => {
            if n >= set.sems.len() {
                return Err(-22);
            }

            Ok(set.sems[n].pid as i32)
        },
        c if c == GETNCNT => {
            if n >= set.sems.len() {
                return Err(-22);
            }

            Ok(set.sems[n].ncnt as i32)
        },
        c if c == GETZCNT => {
            if n >= set.sems.len() {
                return Err(-22);
            }

            Ok(set.sems[n].zcnt as i32)
        },
        c if c == GETALL => {
            let mut bytes = Vec::with_capacity(set.sems.len() * 2);
            for sem in &set.sems { bytes.extend_from_slice(&(sem.val as u16).to_ne_bytes()); }
            crate::uaccess::copy_to_user(arg as usize, &bytes).map_err(|_| -14isize)?;
            Ok(0)
        },
        c if c == SETALL => {
            let mut bytes = alloc::vec![0u8; set.sems.len() * 2];
            crate::uaccess::copy_from_user(&mut bytes, arg as usize).map_err(|_| -14isize)?;
            let values: Vec<u16> = bytes.chunks_exact(2)
                .map(|b| u16::from_ne_bytes([b[0], b[1]])).collect();
            if values.iter().any(|&v| v > SEMVMX as u16) { return Err(-34); }
            let pid = crate::proc::scheduler::current_pid() as u32;
            for (sem, value) in set.sems.iter_mut().zip(values) {
                sem.val = value as i16;
                sem.pid = pid;
            }
            set.ctime = super::now_secs();
            UNDO.lock().retain(|&(_, id, _), _| id != semid);
            Ok(0)
        },
        c if c == IPC_RMID => {
            drop(set);
            SEM_TABLE.lock().remove(&semid);
            UNDO.lock().retain(|&(_, id, _), _| id != semid);
            Ok(0)
        },
        c if c == IPC_STAT => {
            let mut bytes = [0u8; 104];
            let p = &set.perm;
            bytes[0..4].copy_from_slice(&p.key.to_ne_bytes());
            bytes[4..8].copy_from_slice(&p.uid.to_ne_bytes());
            bytes[8..12].copy_from_slice(&p.gid.to_ne_bytes());
            bytes[12..16].copy_from_slice(&p.cuid.to_ne_bytes());
            bytes[16..20].copy_from_slice(&p.cgid.to_ne_bytes());
            bytes[20..22].copy_from_slice(&p.mode.to_ne_bytes());
            bytes[24..26].copy_from_slice(&p.seq.to_ne_bytes());
            bytes[48..56].copy_from_slice(&set.otime.to_ne_bytes());
            bytes[64..72].copy_from_slice(&set.ctime.to_ne_bytes());
            bytes[80..88].copy_from_slice(&(set.sems.len() as u64).to_ne_bytes());
            crate::uaccess::copy_to_user(arg as usize, &bytes).map_err(|_| -14isize)?;
            Ok(0)
        },
        c if c == IPC_SET => {
            let mut bytes = [0u8; 104];
            crate::uaccess::copy_from_user(&mut bytes, arg as usize).map_err(|_| -14isize)?;
            set.perm.uid = u32::from_ne_bytes(bytes[4..8].try_into().unwrap());
            set.perm.gid = u32::from_ne_bytes(bytes[8..12].try_into().unwrap());
            set.perm.mode = (set.perm.mode & !0o777)
                | (u16::from_ne_bytes(bytes[20..22].try_into().unwrap()) & 0o777);
            set.ctime = super::now_secs();
            Ok(0)
        },
        _ => Err(-22),
    }
}

/// Apply the process's accumulated SEM_UNDO adjustments, once, during exit.
pub fn undo_on_exit(pid: usize) {
    let entries: Vec<_> = {
        let mut undo = UNDO.lock();
        let entries = undo.iter().filter(|(k, _)| k.0 == pid)
            .map(|(&key, &value)| (key, value)).collect();
        undo.retain(|key, _| key.0 != pid);
        entries
    };
    for ((_, semid, idx), delta) in entries {
        let arc = SEM_TABLE.lock().get(&semid).cloned();
        if let Some(arc) = arc {
            let mut set = arc.lock();
            if let Some(sem) = set.sems.get_mut(idx) {
                sem.val = (sem.val as i32 + delta).clamp(0, SEMVMX) as i16;
                sem.pid = pid as u32;
                set.otime = super::now_secs();
            }
        }
    }
}
