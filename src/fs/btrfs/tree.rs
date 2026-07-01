//! B-tree + block I/O compatibility surface.
//!
//! The parsed Btrfs key structures currently live in `superblock.rs`; older
//! B-tree/directory code imports them through `tree`, so re-export the key
//! types here while the B-tree implementation is rebuilt.

pub use super::superblock::{BtrfsKey, BtrfsKeyPtr};
