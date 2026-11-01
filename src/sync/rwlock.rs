//! Spinning reader/writer locks. Do not hold guards across scheduler handoff.
pub use spin::{RwLock, RwLockReadGuard, RwLockWriteGuard};
