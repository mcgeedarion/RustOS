//! Minimal devfs implementation.
//!
//! Provides:
//! - A static major/minor dispatch table (`DEVFS_TABLE`) mapping `(major, minor)` to `Arc<dyn
//!   FileOps>`.
//! - `register_char_device(major, minor, ops)` — called by subsystems to publish their devices.
//! - `devfs_open(path)` — resolves a `/dev/…` path to a `FileOps`.
//! - `init_devfs()` — creates `/dev/input/` VFS entries and registers `EventNode`s for every device
//!   in `InputDeviceRegistry`.
//!
//! # Major numbers used
//!
//! | Major | Subsystem        | Nodes                     |
//! |------:|:-----------------|:--------------------------|
//! |    13 | input (evdev)    | `/dev/input/event0` …     |
//!
//! Others (DRM = 226, tty = 4, …) follow the same pattern and can be wired
//! in their own subsystem init functions.

#![allow(dead_code)]

use crate::fs::vfs_ops::FileOps;
#[cfg(feature = "input_events")]
use crate::input::{device_count, EventNode};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use spin::Mutex;

const MAX_MAJOR: usize = 256;
const MAX_MINOR: usize = 256;

/// A single cell in the dispatch table.  `None` until a device is registered.
type DevCell = Option<Arc<dyn FileOps + Send + Sync>>;

/// Two-level table: DEVFS_TABLE[major][minor].
struct DevfsTable {
    majors: [Option<alloc::boxed::Box<[DevCell; MAX_MINOR]>>; MAX_MAJOR],
}

impl DevfsTable {
    const fn empty_majors() -> [Option<alloc::boxed::Box<[DevCell; MAX_MINOR]>>; MAX_MAJOR] {
        // Can't const-init Box<[…; 256]> easily; we initialise lazily in
        // register_char_device instead.  This just provides the None array.
        [const { None }; MAX_MAJOR]
    }

    fn get(&self, major: usize, minor: usize) -> Option<Arc<dyn FileOps + Send + Sync>> {
        self.majors.get(major)?.as_ref()?.get(minor)?.clone()
    }

    fn set(&mut self, major: usize, minor: usize, ops: Arc<dyn FileOps + Send + Sync>) {
        if self.majors[major].is_none() {
            let boxed: alloc::boxed::Box<[DevCell; MAX_MINOR]> =
                alloc::boxed::Box::new([const { None }; MAX_MINOR]);
            self.majors[major] = Some(boxed);
        }
        if let Some(ref mut row) = self.majors[major] {
            row[minor] = Some(ops);
        }
    }
}

// SAFETY: DEVFS_TABLE is mutated only during single-threaded init.
static mut DEVFS_TABLE: DevfsTable = DevfsTable {
    majors: [const { None }; MAX_MAJOR],
};

/// Register a character device at (major, minor).
pub fn register_char_device(major: usize, minor: usize, ops: Arc<dyn FileOps + Send + Sync>) {
    unsafe { DEVFS_TABLE.set(major, minor, ops) }
}

/// Resolve a `/dev/input/eventN` path to its `FileOps`.
pub fn devfs_open(path: &str) -> Option<Arc<dyn FileOps + Send + Sync>> {
    let rel = path
        .strip_prefix("/dev/input/event")
        .or_else(|| path.strip_prefix("input/event"))
        .or_else(|| path.strip_prefix("event"))?;

    let minor: usize = rel.parse().ok()?;
    unsafe { DEVFS_TABLE.get(INPUT_MAJOR, minor) }
}

/// Input subsystem major number (matches Linux).
pub const INPUT_MAJOR: usize = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyntheticDev {
    Null,
    Zero,
}

static SYNTH_DEV_FDS: Mutex<BTreeMap<usize, SyntheticDev>> = Mutex::new(BTreeMap::new());
static NEXT_SYNTH_DEV_FD: Mutex<usize> = Mutex::new(0xD000_0000);

fn alloc_synth_fd(kind: SyntheticDev) -> usize {
    let mut next = NEXT_SYNTH_DEV_FD.lock();
    let fd = *next;
    *next = next.saturating_add(1);
    SYNTH_DEV_FDS.lock().insert(fd, kind);
    fd
}

/// Initialise the devfs layer.
pub fn init() {
    #[cfg(feature = "input_events")]
    {
        let count = device_count();
        for minor in 0..count {
            let node = Arc::new(EventNode::new(minor)) as Arc<dyn FileOps + Send + Sync>;
            register_char_device(INPUT_MAJOR, minor, node);
            log::info!("devfs: registered /dev/input/event{}", minor);
        }

        crate::fs::vfs::ensure_dir("/dev/input");
    }
}

/// Open the built-in minimal device nodes that are required before the full
/// devfs/VFS stack is complete.
pub fn try_open(path: &str, _flags: u32) -> Option<usize> {
    match path {
        "/dev/null" | "dev/null" => Some(alloc_synth_fd(SyntheticDev::Null)),
        "/dev/zero" | "dev/zero" => Some(alloc_synth_fd(SyntheticDev::Zero)),
        _ => None,
    }
}

/// fd -> device lookup used by the syscall dispatch path.
pub fn get_dev_fd(fd: usize) -> Option<()> {
    if SYNTH_DEV_FDS.lock().contains_key(&fd) {
        Some(())
    } else {
        None
    }
}

pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    match SYNTH_DEV_FDS.lock().get(&fd).copied() {
        Some(SyntheticDev::Null) => 0,
        Some(SyntheticDev::Zero) => {
            buf.fill(0);
            buf.len() as isize
        },
        None => -9,
    }
}

pub fn write(fd: usize, buf: &[u8]) -> isize {
    match SYNTH_DEV_FDS.lock().get(&fd).copied() {
        Some(SyntheticDev::Null) | Some(SyntheticDev::Zero) => buf.len() as isize,
        None => -9,
    }
}

pub fn close(fd: usize) -> isize {
    if SYNTH_DEV_FDS.lock().remove(&fd).is_some() {
        0
    } else {
        -9
    }
}

// ===== GUESS: stat alias for devfs entries =====
pub fn stat(_path: &str) -> Result<crate::fs::vfs_ops::KStat, isize> {
    // GUESS: cannot resolve without a devfs path map. Surface ENOENT
    // so VFS dispatchers fall through to the next FS.
    Err(-2)
}

/// Read all bytes from a devfs path. Character devices do not expose
/// bulk reads this way; return empty to satisfy the VFS dispatch table
/// without error so callers get a valid (empty) response.
pub fn read_all(_path: &str) -> Result<alloc::vec::Vec<u8>, isize> {
    Ok(alloc::vec::Vec::new())
}

/// List directory entries under a devfs path.
/// Returns synthesised entries for the known input device nodes.
pub fn readdir(path: &str) -> Result<alloc::vec::Vec<crate::fs::vfs::ops::DirEntry>, isize> {
    #[cfg(feature = "input_events")]
    {
        if path == "/dev/input" || path == "input" || path.is_empty() {
            let count = crate::input::device_count();
            let entries = (0..count)
                .map(|i| crate::fs::vfs::ops::DirEntry {
                    name: alloc::format!("event{}", i),
                    ino: (INPUT_MAJOR * MAX_MINOR + i) as u64,
                    is_dir: false,
                    mode: 0o020600,
                    size: 0,
                })
                .collect();
            return Ok(entries);
        }
    }

    let _ = path;
    Err(-2)
}

/// Concrete `dev:` scheme adapter.
///
/// The implementation lives in `url_dispatch` so all filesystem URL handlers
/// share the same fd-table and flag-handling helpers.
pub use crate::fs::url_dispatch::DevFs;
