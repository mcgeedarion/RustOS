//! Allocation safety boundary.
//!
//! Mapped chunk space is NOT free space. The old bump allocator could overwrite
//! arbitrary live extents. Bounded inline mutations require no new blocks; a
//! future allocator must update extent references, block groups and free-space
//! trees transactionally before allocating anything.
pub use super::superblock::BtrfsFs;

impl BtrfsFs {
    pub(crate) fn try_alloc_logical_block(&mut self, _size: usize) -> Result<u64, isize> {
        Err(-95)
    }

    pub(crate) fn logical_range_is_mapped(&self, logical: u64, len: u64) -> bool {
        logical.checked_add(len).is_some_and(|end| self.chunk_map.iter()
            .any(|(start, chunk_end, _)| logical >= *start && end <= *chunk_end))
    }
}
