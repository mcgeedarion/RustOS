//! Spinning mutex compatibility surface, not a sleeping scheduler mutex.
//! Guards must be released before blocking; interrupt masking is separate.
pub use spin::{Mutex, MutexGuard};
