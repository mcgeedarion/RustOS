//! Minimal Landlock syscall support.
//!
//! This is an ABI-compatible ruleset lifecycle implementation: callers can
//! query the supported ABI version, create ruleset fds, attach path-beneath
//! rules to those fds, and restrict the current task.  The enforcement layer is
//! intentionally conservative while path-scoped VFS objects are still being
//! plumbed through LSM contexts: once restricted, a task may only perform access
//! classes allowed by its installed ruleset.  Invalid pointers, sizes, flags,
//! rule types, and fds are rejected with Linux-style errno values.

use crate::core::fast_hash::KernelFastMap;
use crate::security::lsm::{LsmCtx, LsmHooks, LsmVerdict};
use spin::Mutex;

const E2BIG: isize = -7;
const EBADF: isize = -9;
const EFAULT: isize = -14;
const EINVAL: isize = -22;
const EMFILE: isize = -24;
const ENOMSG: isize = -42;

const LANDLOCK_ABI_VERSION: isize = 6;
const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1 << 0;
const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;

const LANDLOCK_ACCESS_FS_EXECUTE: u64 = 1 << 0;
const LANDLOCK_ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
const LANDLOCK_ACCESS_FS_READ_FILE: u64 = 1 << 2;
const LANDLOCK_ACCESS_FS_READ_DIR: u64 = 1 << 3;
const LANDLOCK_ACCESS_FS_REMOVE_DIR: u64 = 1 << 4;
const LANDLOCK_ACCESS_FS_REMOVE_FILE: u64 = 1 << 5;
const LANDLOCK_ACCESS_FS_MAKE_REG: u64 = 1 << 8;
const LANDLOCK_ACCESS_FS_TRUNCATE: u64 = 1 << 14;
const ACCESS_FS_MASK: u64 = (1u64 << 16) - 1;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RulesetAttr {
    handled_access_fs: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

#[derive(Clone, Copy, Default)]
struct Ruleset {
    handled_access_fs: u64,
    allowed_access_fs: u64,
    rule_count: usize,
    restricted: bool,
}

#[derive(Clone, Copy, Default)]
struct Domain {
    handled_access_fs: u64,
    allowed_access_fs: u64,
}

static RULESETS: Mutex<KernelFastMap<usize, Ruleset>> = Mutex::new(KernelFastMap::new());
static DOMAINS: Mutex<KernelFastMap<usize, Domain>> = Mutex::new(KernelFastMap::new());

pub struct LandlockLsm;

pub static LANDLOCK_LSM: LandlockLsm = LandlockLsm;

fn enforce(pid: usize, requested: u64) -> LsmVerdict {
    if requested == 0 {
        return LsmVerdict::Allow;
    }
    let domains = DOMAINS.lock();
    let domain = match domains.get(&pid) {
        Some(domain) => *domain,
        None => return LsmVerdict::Allow,
    };
    let relevant = requested & domain.handled_access_fs;
    if relevant == 0 || (relevant & !domain.allowed_access_fs) == 0 {
        LsmVerdict::Allow
    } else {
        LsmVerdict::Deny(13)
    }
}

impl LsmHooks for LandlockLsm {
    fn name(&self) -> &'static str {
        "landlock"
    }

    fn file_read(&self, ctx: &LsmCtx) -> LsmVerdict {
        enforce(
            ctx.pid,
            LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_READ_DIR,
        )
    }

    fn file_write(&self, ctx: &LsmCtx) -> LsmVerdict {
        enforce(
            ctx.pid,
            LANDLOCK_ACCESS_FS_WRITE_FILE | LANDLOCK_ACCESS_FS_TRUNCATE,
        )
    }

    fn file_exec(&self, ctx: &LsmCtx) -> LsmVerdict {
        enforce(ctx.pid, LANDLOCK_ACCESS_FS_EXECUTE)
    }

    fn inode_create(&self, ctx: &LsmCtx) -> LsmVerdict {
        enforce(ctx.pid, LANDLOCK_ACCESS_FS_MAKE_REG)
    }

    fn inode_unlink(&self, ctx: &LsmCtx) -> LsmVerdict {
        enforce(
            ctx.pid,
            LANDLOCK_ACCESS_FS_REMOVE_FILE | LANDLOCK_ACCESS_FS_REMOVE_DIR,
        )
    }
}

fn copy_from_user<T: Copy + Default>(addr: usize) -> Result<T, isize> {
    if addr == 0 {
        return Err(EFAULT);
    }
    let mut value = T::default();
    crate::uaccess::copy_from_user(
        &mut value as *mut T as *mut u8,
        addr,
        core::mem::size_of::<T>(),
    )
    .map_err(|_| EFAULT)?;
    Ok(value)
}

pub fn is_landlock_fd(fd: usize) -> bool {
    RULESETS.lock().contains_key(&fd)
}

pub fn close_landlock_fd(fd: usize) {
    RULESETS.lock().remove(&fd);
    let _ = crate::fs::vfs::close(fd);
}

pub fn dup_landlock_fd(fd: usize) -> usize {
    let ruleset = match RULESETS.lock().get(&fd).copied() {
        Some(ruleset) => ruleset,
        None => return fd,
    };
    let new_fd = match crate::fs::vfs::open_anon(crate::fs::vfs::O_CLOEXEC) {
        Ok(new_fd) => new_fd,
        Err(_) => return fd,
    };
    RULESETS.lock().insert(new_fd, ruleset);
    new_fd
}

pub fn sys_landlock_create_ruleset(attr: usize, size: usize, flags: u32) -> isize {
    if flags == LANDLOCK_CREATE_RULESET_VERSION {
        return if attr == 0 && size == 0 {
            LANDLOCK_ABI_VERSION
        } else {
            EINVAL
        };
    }
    if flags != 0 {
        return EINVAL;
    }
    if size < core::mem::size_of::<RulesetAttr>() {
        return EINVAL;
    }
    if size > core::mem::size_of::<RulesetAttr>() {
        return E2BIG;
    }

    let attr = match copy_from_user::<RulesetAttr>(attr) {
        Ok(attr) => attr,
        Err(e) => return e,
    };
    if attr.handled_access_fs == 0 || (attr.handled_access_fs & !ACCESS_FS_MASK) != 0 {
        return ENOMSG;
    }

    let fd = match crate::fs::vfs::open_anon(crate::fs::vfs::O_CLOEXEC) {
        Ok(fd) => fd,
        Err(_) => return EMFILE,
    };
    RULESETS.lock().insert(
        fd,
        Ruleset {
            handled_access_fs: attr.handled_access_fs,
            allowed_access_fs: 0,
            rule_count: 0,
            restricted: false,
        },
    );
    fd as isize
}

pub fn sys_landlock_add_rule(
    ruleset_fd: usize,
    rule_type: u32,
    rule_attr: usize,
    flags: u32,
) -> isize {
    if flags != 0 || rule_type != LANDLOCK_RULE_PATH_BENEATH {
        return EINVAL;
    }
    let attr = match copy_from_user::<PathBeneathAttr>(rule_attr) {
        Ok(attr) => attr,
        Err(e) => return e,
    };
    if attr.parent_fd < 0 {
        return EBADF;
    }

    let mut rulesets = RULESETS.lock();
    let ruleset = match rulesets.get_mut(&ruleset_fd) {
        Some(ruleset) => ruleset,
        None => return EBADF,
    };
    if ruleset.restricted || attr.allowed_access == 0 {
        return EINVAL;
    }
    if (attr.allowed_access & !ruleset.handled_access_fs) != 0 {
        return EINVAL;
    }
    ruleset.allowed_access_fs |= attr.allowed_access;
    ruleset.rule_count = ruleset.rule_count.saturating_add(1);
    0
}

pub fn sys_landlock_restrict_self(ruleset_fd: usize, flags: u32) -> isize {
    if flags != 0 {
        return EINVAL;
    }
    let mut rulesets = RULESETS.lock();
    let ruleset = match rulesets.get_mut(&ruleset_fd) {
        Some(ruleset) => ruleset,
        None => return EBADF,
    };
    ruleset.restricted = true;
    let domain = Domain {
        handled_access_fs: ruleset.handled_access_fs,
        allowed_access_fs: ruleset.allowed_access_fs,
    };
    let pid = crate::proc::scheduler::current_pid() as usize;
    drop(rulesets);
    DOMAINS.lock().insert(pid, domain);
    0
}
