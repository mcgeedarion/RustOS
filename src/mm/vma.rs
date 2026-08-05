//! Compatibility exports for older VMA callers.
//!
//! The active VMA implementation lives under [`crate::mm::mmap`].  Some
//! full-kernel paths still import `crate::mm::vma::{MappingFlags, VmaKind}`;
//! keep that API shape here while those callers are migrated.

pub use crate::mm::mmap::{Vma, VmaKind};

/// Lightweight mapping-permission flags used by legacy loader code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MappingFlags(u32);

impl MappingFlags {
    pub const READ: Self = Self(1 << 0);
    pub const WRITE: Self = Self(1 << 1);
    pub const EXEC: Self = Self(1 << 2);

    #[inline]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[inline]
    pub const fn bits(self) -> u32 {
        self.0
    }

    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl core::ops::BitOr for MappingFlags {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitOrAssign for MappingFlags {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}
