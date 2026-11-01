//! System V message queues.
//!
//! ## API
//!
//! | Function                                      | Returns   |
//! |-----------------------------------------------|------------|
//! | `msgget(key, msgflg)`                         | `msqid`   |
//! | `msgsnd(msqid, msgp, msgsz, msgflg)`          | `0`       |
//! | `msgrcv(msqid, msgp, msgsz, msgtyp, msgflg)` | `ssize_t` |
//! | `msgctl(msqid, cmd, buf)`                     | varies    |
//!
//! ## `struct msgbuf` layout (matches Linux UAPI)
//!
//! ```c
//! long  mtype;   // message type, must be > 0
//! char  mtext[]; // message data
//! ```
//!
//! ## Key semantics
//!
//! - `IPC_PRIVATE` always creates a new private queue.
//! - `IPC_CREAT | IPC_EXCL` fails with EEXIST if key already exists.
//! - `IPC_CREAT` alone creates-or-opens.
//!
//! ## `msgrcv` type matching
//!
//! | `msgtyp` | Behaviour |
//! |----------|-----------|
//! | 0        | First message in queue |
//! | > 0      | First message with `mtype == msgtyp` |
//! | < 0      | First message with `mtype <= abs(msgtyp)` |

extern crate alloc;
use crate::ipc::{
    check_perm, IpcPerm, IPC_CREAT, IPC_EXCL, IPC_NOWAIT, IPC_PRIVATE, IPC_RMID, IPC_SET, IPC_STAT,
};
use alloc::{collections::VecDeque, string::String, sync::Arc, vec::Vec};
use spin::Mutex;

pub const MSGMAX: usize = 8192;
pub const MSGMNB: usize = 16384; // default max queue size in bytes
pub const MSGMNI: i32 = 32000; // system-wide queue limit
pub const MSG_NOERROR: i32 = 0o10000;
pub const MSG_EXCEPT: i32 = 0o20000;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct MsqidDs {
    pub msg_perm: IpcPerm,
    pub msg_stime: i64,
    pub msg_rtime: i64,
    pub msg_ctime: i64,
    pub msg_cbytes: u64,
    pub msg_qnum: u64,
    pub msg_qbytes: u64,
    pub msg_lspid: i32,
    pub msg_lrpid: i32,
    pub _reserved: [u64; 2],
}

const _: () = assert!(core::mem::size_of::<MsqidDs>() == 120);

/// One queued message.
struct Message {
    mtype: i64,
    data: Vec<u8>,
}

/// Kernel-side message queue.
struct MsgQueue {
    perm: IpcPerm,
    messages: VecDeque<Message>,
    msg_cbytes: usize, // current bytes in queue
    msg_qnum: usize,
    msg_qbytes: usize, // max bytes allowed
    msg_lspid: u32,
    msg_lrpid: u32,
    msg_stime: u64,
    msg_rtime: u64,
    msg_ctime: u64,
}

use alloc::collections::BTreeMap;
static MSG_TABLE: Mutex<BTreeMap<i32, Arc<Mutex<MsgQueue>>>> = Mutex::new(BTreeMap::new());
static NEXT_ID: spin::Mutex<i32> = spin::Mutex::new(1);

fn alloc_id() -> i32 {
    let mut id = NEXT_ID.lock();
    let v = *id;
    *id += 1;
    v
}

pub fn msgget(key: i32, msgflg: i32) -> Result<i32, isize> {
    let mut tbl = MSG_TABLE.lock();
    let (uid, gid) = super::current_ids();

    // IPC_PRIVATE — always create a new private queue.
    if key == IPC_PRIVATE {
        let id = alloc_id();
        let q = MsgQueue {
            perm: IpcPerm::new(IPC_PRIVATE, uid, gid, (msgflg & 0o777) as u16),
            messages: VecDeque::new(),
            msg_cbytes: 0,
            msg_qnum: 0,
            msg_qbytes: MSGMNB,
            msg_lspid: 0,
            msg_lrpid: 0,
            msg_stime: 0,
            msg_rtime: 0,
            msg_ctime: super::now_secs() as u64,
        };
        tbl.insert(id, Arc::new(Mutex::new(q)));
        return Ok(id);
    }

    // Look for an existing queue with matching key.
    if let Some((&id, arc)) = tbl.iter().find(|(_, q)| q.lock().perm.key == key) {
        if msgflg & IPC_CREAT != 0 && msgflg & IPC_EXCL != 0 {
            return Err(-17); // EEXIST
        }
        if !check_perm(&arc.lock().perm, uid, gid, (msgflg as u16 >> 6) & 6) {
            return Err(-13);
        }
        return Ok(id);
    }

    if msgflg & IPC_CREAT == 0 {
        return Err(-2);
    } // ENOENT

    let id = alloc_id();
    let q = MsgQueue {
        perm: IpcPerm::new(key, uid, gid, (msgflg & 0o777) as u16),
        messages: VecDeque::new(),
        msg_cbytes: 0,
        msg_qnum: 0,
        msg_qbytes: MSGMNB,
        msg_lspid: 0,
        msg_lrpid: 0,
        msg_stime: 0,
        msg_rtime: 0,
        msg_ctime: super::now_secs() as u64,
    };
    tbl.insert(id, Arc::new(Mutex::new(q)));
    Ok(id)
}

pub fn msgsnd(msqid: i32, mtype: i64, data: Vec<u8>, msgflg: i32) -> Result<(), isize> {
    if mtype <= 0 {
        return Err(-22);
    } // EINVAL — mtype must be positive
    if data.len() > MSGMAX {
        return Err(-90);
    } // EMSGSIZE

    loop {
        let arc = {
            let tbl = MSG_TABLE.lock();
            tbl.get(&msqid).cloned().ok_or(-43isize)? // EIDRM
        };
        let mut q = arc.lock();

        let (uid, gid) = super::current_ids();
        if !check_perm(&q.perm, uid, gid, 0o2) {
            return Err(-13);
        } // EACCES

        let needed = data.len();
        if q.msg_cbytes.saturating_add(needed) > q.msg_qbytes || q.msg_qnum >= q.msg_qbytes {
            if msgflg & IPC_NOWAIT != 0 {
                return Err(-11);
            } // EAGAIN
            drop(q);
            crate::proc::scheduler::schedule();
            continue;
        }
        q.msg_cbytes += needed;
        q.msg_qnum += 1;
        q.msg_lspid = crate::proc::scheduler::current_pid() as u32;
        q.msg_stime = super::now_secs() as u64;
        q.messages.push_back(Message { mtype, data });
        return Ok(());
    }
}

pub fn msgrcv(msqid: i32, msgsz: usize, msgtyp: i64, msgflg: i32) -> Result<(i64, Vec<u8>), isize> {
    loop {
        let arc = {
            let tbl = MSG_TABLE.lock();
            tbl.get(&msqid).cloned().ok_or(-43isize)?
        };
        let mut q = arc.lock();

        let (uid, gid) = super::current_ids();
        if !check_perm(&q.perm, uid, gid, 0o4) {
            return Err(-13);
        } // EACCES

        // Find the target message index.
        let idx = if msgtyp == 0 {
            if q.messages.is_empty() {
                None
            } else {
                Some(0)
            }
        } else if msgtyp > 0 {
            q.messages.iter().position(|m| {
                if msgflg & MSG_EXCEPT != 0 { m.mtype != msgtyp } else { m.mtype == msgtyp }
            })
        } else {
            let abs_typ = msgtyp.unsigned_abs();
            q.messages
                .iter()
                .enumerate()
                .filter(|(_, m)| m.mtype as u64 <= abs_typ)
                .min_by_key(|(_, m)| m.mtype)
                .map(|(i, _)| i)
        };

        if let Some(i) = idx {
            if q.messages[i].data.len() > msgsz && msgflg & MSG_NOERROR == 0 {
                return Err(-7); // E2BIG leaves the message queued.
            }
            let mut msg = q.messages.remove(i).ok_or(-42isize)?;
            q.msg_cbytes = q.msg_cbytes.saturating_sub(msg.data.len());
            q.msg_qnum = q.msg_qnum.saturating_sub(1);
            q.msg_lrpid = crate::proc::scheduler::current_pid() as u32;
            q.msg_rtime = super::now_secs() as u64;
            msg.data.truncate(msgsz);
            return Ok((msg.mtype, msg.data));
        }

        if msgflg & IPC_NOWAIT != 0 {
            return Err(-42);
        } // ENOMSG
        drop(q);
        crate::proc::scheduler::schedule();
    }
}

pub fn msgctl(msqid: i32, cmd: i32) -> Result<MsqidDs, isize> {
    let arc = MSG_TABLE.lock().get(&msqid).cloned().ok_or(-22isize)?;
    match cmd {
        c if c == IPC_RMID => {
            if !super::owner_perm(&arc.lock().perm) { return Err(-1); }
            MSG_TABLE.lock().remove(&msqid).ok_or(-43isize)?;
            Ok(MsqidDs::default())
        },
        c if c == IPC_STAT => {
            let q = arc.lock();
            let (uid, gid) = super::current_ids();
            if !check_perm(&q.perm, uid, gid, 4) { return Err(-13); }
            Ok(MsqidDs {
                msg_perm: q.perm,
                msg_stime: q.msg_stime as i64,
                msg_rtime: q.msg_rtime as i64,
                msg_ctime: q.msg_ctime as i64,
                msg_cbytes: q.msg_cbytes as u64,
                msg_qnum: q.msg_qnum as u64,
                msg_qbytes: q.msg_qbytes as u64,
                msg_lspid: q.msg_lspid as i32,
                msg_lrpid: q.msg_lrpid as i32,
                _reserved: [0; 2],
            })
        },
        _ => Err(-22),
    }
}

pub fn msgctl_set(msqid: i32, ds: MsqidDs) -> Result<(), isize> {
    let arc = MSG_TABLE.lock().get(&msqid).cloned().ok_or(-22isize)?;
    let mut q = arc.lock();
    if !super::owner_perm(&q.perm) { return Err(-1); }
    let (uid, _) = super::current_ids();
    if ds.msg_qbytes > MSGMNB as u64 && uid != 0 { return Err(-1); }
    q.perm.uid = ds.msg_perm.uid;
    q.perm.gid = ds.msg_perm.gid;
    q.perm.mode = (q.perm.mode & !0o777) | (ds.msg_perm.mode & 0o777);
    q.msg_qbytes = usize::try_from(ds.msg_qbytes).map_err(|_| -22isize)?;
    q.msg_ctime = super::now_secs() as u64;
    Ok(())
}
