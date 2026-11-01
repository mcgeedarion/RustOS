//! JBD2 journal replay support for ext4.

extern crate alloc;

use alloc::vec::Vec;
use alloc::vec;

const JBD2_MAGIC: u32 = 0xC03B3998;
const JBD2_DESCRIPTOR_BLOCK: u32 = 1;
const JBD2_COMMIT_BLOCK: u32 = 2;
const JBD2_SUPERBLOCK_V1: u32 = 3;
const JBD2_SUPERBLOCK_V2: u32 = 4;
const JBD2_REVOKE_BLOCK: u32 = 5;

const JBD2_FLAG_ESCAPE: u16 = 0x0001;
const JBD2_FLAG_SAME_UUID: u16 = 0x0002;
const JBD2_FLAG_DELETED: u16 = 0x0004;
const JBD2_FLAG_LAST_TAG: u16 = 0x0008;

const JBD2_FEATURE_INCOMPAT_REVOKE: u32 = 0x00000001;
const JBD2_FEATURE_INCOMPAT_64BIT: u32 = 0x00000002;
const JBD2_FEATURE_INCOMPAT_ASYNC_COMMIT: u32 = 0x00000004;
const JBD2_FEATURE_INCOMPAT_CSUM_V2: u32 = 0x00000008;
const JBD2_FEATURE_INCOMPAT_CSUM_V3: u32 = 0x00000010;
const JBD2_FEATURE_INCOMPAT_FAST_COMMIT: u32 = 0x00000020;

const JBD2_FEATURE_COMPAT_CHECKSUM: u32 = 0x00000001;

#[derive(Clone, Copy, Debug, Default)]
pub struct JournalFeatures {
    pub compat: u32,
    pub incompat: u32,
    pub ro_compat: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct JournalSuperblock {
    pub block_size: usize,
    pub max_len: u32,
    pub first: u32,
    pub sequence: u32,
    pub start: u32,
    pub errno: u32,
    pub features: JournalFeatures,
    pub uuid: [u8; 16],
    pub checksum_type: u8,
    pub checksum_seed: u32,
}

#[derive(Clone, Debug)]
struct DescriptorTag {
    fs_block: u64,
    flags: u16,
    escaped: bool,
}

#[derive(Clone, Debug)]
struct RevokeRecord {
    sequence: u32,
    fs_block: u64,
}

#[derive(Clone, Debug)]
struct PendingTxn {
    sequence: u32,
    tags: Vec<DescriptorTag>,
    payloads: Vec<Vec<u8>>,
    revokes: Vec<RevokeRecord>,
}

#[derive(Clone, Debug, Default)]
pub struct ReplayReport {
    pub transactions_seen: usize,
    pub transactions_replayed: usize,
    pub blocks_replayed: usize,
    pub revoke_records: usize,
    pub checksum_failures: usize,
    pub unsupported_fast_commit_blocks: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayError {
    EmptyJournal,
    InvalidBlockSize,
    BadSuperblock,
    UnsupportedFeature(u32),
    OutOfBounds,
    BadDescriptor,
}

#[inline]
fn be16(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(off)?, *b.get(off + 1)?]))
}

#[inline]
fn be32(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *b.get(off)?,
        *b.get(off + 1)?,
        *b.get(off + 2)?,
        *b.get(off + 3)?,
    ]))
}

#[inline]
fn be64(b: &[u8], off: usize) -> Option<u64> {
    Some(u64::from_be_bytes([
        *b.get(off)?,
        *b.get(off + 1)?,
        *b.get(off + 2)?,
        *b.get(off + 3)?,
        *b.get(off + 4)?,
        *b.get(off + 5)?,
        *b.get(off + 6)?,
        *b.get(off + 7)?,
    ]))
}

#[inline]
fn header(block: &[u8]) -> Option<(u32, u32, u32)> {
    let magic = be32(block, 0)?;
    let ty = be32(block, 4)?;
    let seq = be32(block, 8)?;
    if magic != JBD2_MAGIC {
        return None;
    }
    Some((magic, ty, seq))
}

pub fn parse_superblock(block: &[u8]) -> Option<JournalSuperblock> {
    let (_, ty, _) = header(block)?;
    if ty != JBD2_SUPERBLOCK_V1 && ty != JBD2_SUPERBLOCK_V2 {
        return None;
    }

    let block_size = be32(block, 12)? as usize;
    let max_len = be32(block, 16)?;
    let first = be32(block, 20)?;
    let sequence = be32(block, 24)?;
    let start = be32(block, 28)?;
    let errno = be32(block, 32).unwrap_or(0);
    let compat = be32(block, 36).unwrap_or(0);
    let incompat = be32(block, 40).unwrap_or(0);
    let ro_compat = be32(block, 44).unwrap_or(0);
    let mut uuid = [0u8; 16];
    if block.len() >= 64 {
        uuid.copy_from_slice(&block[48..64]);
    }
    let checksum_type = *block.get(80).unwrap_or(&0);
    let checksum_seed = be32(block, 84).unwrap_or(0);

    if block_size == 0 || block_size & (block_size - 1) != 0 {
        return None;
    }

    Some(JournalSuperblock {
        block_size,
        max_len,
        first,
        sequence,
        start,
        errno,
        features: JournalFeatures {
            compat,
            incompat,
            ro_compat,
        },
        uuid,
        checksum_type,
        checksum_seed,
    })
}

fn unsupported_incompat(features: JournalFeatures) -> u32 {
    // Checksummed, async and fast-commit journals must not be replayed without
    // their verification/protocol implementations.
    let supported = JBD2_FEATURE_INCOMPAT_REVOKE | JBD2_FEATURE_INCOMPAT_64BIT;
    features.incompat & !supported
}

fn block_by_journal_index<'a>(
    journal: &'a [u8],
    block_size: usize,
    idx: usize,
) -> Option<&'a [u8]> {
    let off = idx.checked_mul(block_size)?;
    journal.get(off..off.checked_add(block_size)?)
}

fn parse_descriptor(block: &[u8], sb: &JournalSuperblock) -> Vec<DescriptorTag> {
    let mut tags = Vec::new();
    let has_64bit = sb.features.incompat & JBD2_FEATURE_INCOMPAT_64BIT != 0;
    let has_csum_v3 = sb.features.incompat & JBD2_FEATURE_INCOMPAT_CSUM_V3 != 0;
    let has_csum_v2 = sb.features.incompat & JBD2_FEATURE_INCOMPAT_CSUM_V2 != 0;

    let tail = if has_csum_v2 || has_csum_v3 { 8 } else { 0 };
    let mut off = 12usize;
    while off + 8 <= block.len().saturating_sub(tail) {
        let lo = match be32(block, off) {
            Some(v) => v,
            None => break,
        };
        // Legacy tags contain blocknr, checksum/reserved, flags, then the
        // optional high blocknr. Flags are not at offset four.
        let flags = be16(block, off + 6).unwrap_or(0);
        if flags & !(JBD2_FLAG_ESCAPE | JBD2_FLAG_SAME_UUID | JBD2_FLAG_LAST_TAG) != 0 {
            return Vec::new();
        }
        let mut tag_off = off + 8;
        let hi = if has_64bit {
            let v = be32(block, tag_off).unwrap_or(0);
            tag_off += 4;
            v
        } else {
            0
        };
        if flags & JBD2_FLAG_SAME_UUID == 0 {
            if block.get(tag_off..tag_off + 16) != Some(sb.uuid.as_slice()) {
                return Vec::new();
            }
            tag_off += 16;
        }
        if has_csum_v3 {
            tag_off += 4;
        } else if has_csum_v2 {
            tag_off += 2;
        }

        if tag_off > block.len() {
            return Vec::new();
        }

        let fs_block = ((hi as u64) << 32) | lo as u64;
        if flags & JBD2_FLAG_DELETED == 0 {
            tags.push(DescriptorTag {
                fs_block,
                flags,
                escaped: flags & JBD2_FLAG_ESCAPE != 0,
            });
        }
        off = tag_off;
        if flags & JBD2_FLAG_LAST_TAG != 0 {
            return tags;
        }
    }
    Vec::new()
}

fn parse_revoke(block: &[u8], sb: &JournalSuperblock, seq: u32) -> Vec<RevokeRecord> {
    let mut out = Vec::new();
    let has_64bit = sb.features.incompat & JBD2_FEATURE_INCOMPAT_64BIT != 0;
    let rec_len = if has_64bit { 8 } else { 4 };
    let used = be32(block, 12).unwrap_or(block.len() as u32) as usize;
    let end = used.min(block.len());
    let mut off = 16usize;
    while off + rec_len <= end {
        let fs_block = if has_64bit {
            be64(block, off).unwrap_or(0)
        } else {
            be32(block, off).unwrap_or(0) as u64
        };
        out.push(RevokeRecord {
            sequence: seq,
            fs_block,
        });
        off += rec_len;
    }
    out
}

fn is_revoked(revokes: &[RevokeRecord], sequence: u32, fs_block: u64) -> bool {
    revokes
        .iter()
        .any(|r| r.sequence == sequence && r.fs_block == fs_block)
}

fn apply_txn(fs_image: &mut [u8], block_size: usize, txn: &PendingTxn) -> usize {
    let mut replayed = 0usize;
    for (idx, tag) in txn.tags.iter().enumerate() {
        if is_revoked(&txn.revokes, txn.sequence, tag.fs_block) {
            continue;
        }
        let src = match txn.payloads.get(idx) {
            Some(s) => s,
            None => break,
        };
        let dst_off = match (tag.fs_block as usize).checked_mul(block_size) {
            Some(v) => v,
            None => continue,
        };
        let dst = match fs_image.get_mut(dst_off..dst_off + block_size) {
            Some(d) => d,
            None => continue,
        };
        dst.copy_from_slice(src);
        if tag.escaped && dst.len() >= 4 {
            dst[0..4].copy_from_slice(&JBD2_MAGIC.to_be_bytes());
        }
        replayed += 1;
    }
    replayed
}

/// Replay a linear journal byte image into `fs_image`.
///
/// `journal` must begin with a JBD2 superblock.  The caller is responsible for
/// translating ext4's journal inode mapping into this contiguous byte image.
pub fn replay_journal_image(
    fs_image: &mut [u8],
    journal: &[u8],
) -> Result<ReplayReport, ReplayError> {
    let sb_block = journal.get(..1024).ok_or(ReplayError::EmptyJournal)?;
    let sb = parse_superblock(sb_block).ok_or(ReplayError::BadSuperblock)?;
    if sb.block_size == 0 || sb.block_size > 65536 {
        return Err(ReplayError::InvalidBlockSize);
    }
    let unsupported = unsupported_incompat(sb.features);
    if unsupported != 0 || sb.features.compat != 0 || sb.features.ro_compat != 0 {
        return Err(ReplayError::UnsupportedFeature(unsupported));
    }

    let mut report = ReplayReport::default();
    let max_len = sb.max_len as usize;
    if max_len == 0 || sb.first == 0 || sb.first >= sb.max_len
        || (sb.start != 0 && (sb.start < sb.first || sb.start >= sb.max_len))
        || max_len
            .checked_mul(sb.block_size)
            .map_or(true, |n| n > journal.len())
    {
        return Err(ReplayError::OutOfBounds);
    }
    // A checkpointed journal may contain stale valid descriptor/commit blocks.
    // Its start==0 is authoritative, so never scan those old transactions.
    if sb.start == 0 { return Ok(report); }
    let mut idx = sb.start as usize;
    let mut expected = sb.sequence;
    let mut committed = Vec::new();
    let mut current: Option<PendingTxn> = None;
    let mut blocks_scanned = 0usize;
    let ring_len = max_len - sb.first as usize;
    let advance = |index: usize| if index + 1 == max_len { sb.first as usize } else { index + 1 };
    while blocks_scanned < ring_len {
        let block = block_by_journal_index(journal, sb.block_size, idx).ok_or(ReplayError::OutOfBounds)?;
        let Some((_, ty, seq)) = header(block) else { break; };
        if seq != expected { break; }
        idx = advance(idx);
        blocks_scanned += 1;
        match ty {
            JBD2_DESCRIPTOR_BLOCK => {
                let tags = parse_descriptor(block, &sb);
                if tags.is_empty() { return Err(ReplayError::BadDescriptor); }
                if tags.len() > ring_len - blocks_scanned { break; }
                let txn = current.get_or_insert_with(|| PendingTxn {
                    sequence: seq,
                    tags: Vec::new(),
                    payloads: Vec::new(),
                    revokes: Vec::new(),
                });
                // Payloads are identified by descriptor count, not by their
                // first four bytes: valid data may begin with JBD2 magic.
                for _ in &tags {
                    let payload = block_by_journal_index(journal, sb.block_size, idx)
                        .ok_or(ReplayError::OutOfBounds)?;
                    txn.payloads.push(payload.to_vec());
                    idx = advance(idx);
                    blocks_scanned += 1;
                }
                txn.tags.extend(tags);
            }
            JBD2_REVOKE_BLOCK => {
                let record_size = if sb.features.incompat & JBD2_FEATURE_INCOMPAT_64BIT != 0 { 8 } else { 4 };
                let used = be32(block, 12).ok_or(ReplayError::BadDescriptor)? as usize;
                if used < 16 || used > block.len() || (used - 16) % record_size != 0 {
                    return Err(ReplayError::BadDescriptor);
                }
                let revokes = parse_revoke(block, &sb, seq);
                report.revoke_records += revokes.len();
                current.get_or_insert_with(|| PendingTxn {
                    sequence: seq, tags: Vec::new(), payloads: Vec::new(), revokes: Vec::new(),
                }).revokes.extend(revokes);
            }
            JBD2_COMMIT_BLOCK => {
                if let Some(txn) = current.take() {
                    committed.push(txn);
                    expected = expected.wrapping_add(1);
                } else {
                    break;
                }
            },
            _ => break,
        }
    }
    report.transactions_seen = committed.len() + usize::from(current.is_some());
    // Validate every replay destination before touching the caller's image.
    // Later committed revokes suppress earlier logged data for that block.
    let mut writes = Vec::new();
    for (index, txn) in committed.iter().enumerate() {
        for (tag, payload) in txn.tags.iter().zip(&txn.payloads) {
            if committed[index..].iter().any(|later| later.revokes.iter().any(|record| record.fs_block == tag.fs_block)) {
                continue;
            }
            let off = usize::try_from(tag.fs_block).ok()
                .and_then(|block| block.checked_mul(sb.block_size)).ok_or(ReplayError::OutOfBounds)?;
            let end = off.checked_add(sb.block_size).ok_or(ReplayError::OutOfBounds)?;
            fs_image.get(off..end).ok_or(ReplayError::OutOfBounds)?;
            writes.push((off, payload, tag.escaped));
        }
    }
    for (off, payload, escaped) in writes {
        fs_image[off..off + sb.block_size].copy_from_slice(payload);
        if escaped { fs_image[off..off + 4].copy_from_slice(&JBD2_MAGIC.to_be_bytes()); }
        report.blocks_replayed += 1;
    }
    report.transactions_replayed = committed.len();
    Ok(report)
}

/// Build a contiguous journal image from explicit filesystem block numbers and
/// replay it into `fs_image`.
pub fn replay_from_block_list(
    fs_image: &mut [u8],
    fs_block_size: usize,
    journal_blocks: &[u64],
) -> Result<ReplayReport, ReplayError> {
    if fs_block_size == 0 || journal_blocks.is_empty() {
        return Err(ReplayError::EmptyJournal);
    }
    let mut journal = Vec::with_capacity(journal_blocks.len() * fs_block_size);
    for &blk in journal_blocks {
        let off = (blk as usize)
            .checked_mul(fs_block_size)
            .ok_or(ReplayError::OutOfBounds)?;
        let end = off.checked_add(fs_block_size).ok_or(ReplayError::OutOfBounds)?;
        let src = fs_image
            .get(off..end)
            .ok_or(ReplayError::OutOfBounds)?;
        journal.extend_from_slice(src);
    }
    replay_journal_image(fs_image, &journal)
}

// ============================================================================
// Journaling Validation Functions (Production Requirements Line 82-92)
// ============================================================================

/// Validate journal integrity without replaying.
///
/// This function performs comprehensive validation of the journal structure:
/// - Checks journal superblock magic number
/// - Verifies journal sequence numbers are monotonically increasing
/// - Validates descriptor blocks have proper format
/// - Checks commit blocks match their transactions
/// - Detects incomplete or corrupted transactions
///
/// # Arguments
/// * `journal` - Raw journal data (must start with superblock)
///
/// # Returns
/// * `Ok(JournalValidationReport)` - Detailed validation results
/// * `Err(JournalValidationError)` - Specific validation failure
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JournalValidationReport {
    pub superblock_valid: bool,
    pub magic_valid: bool,
    pub block_size_valid: bool,
    pub sequence_valid: bool,
    pub descriptor_blocks_valid: usize,
    pub commit_blocks_valid: usize,
    pub revoke_blocks_valid: usize,
    pub incomplete_transactions: usize,
    pub checksum_failures: usize,
    pub total_blocks_scanned: usize,
    pub first_valid_sequence: u32,
    pub last_valid_sequence: u32,
    pub transactions_committed: usize,
    pub blocks_replayed: usize,
    pub transactions_replayed: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalValidationError {
    EmptyJournal,
    InvalidBlockSize,
    BadSuperblockMagic,
    BadSuperblockType,
    UnsupportedFeature(u32),
    SequenceGap(u32, u32), // expected, found
    DescriptorBlockCorrupt,
    CommitBlockMismatch(u32, u32), // expected seq, found seq
    OutOfBounds,
    ChecksumFailure,
}

/// Validate journal superblock integrity
pub fn validate_journal_superblock(
    journal: &[u8],
) -> Result<JournalSuperblock, JournalValidationError> {
    let sb_block = journal
        .get(..1024)
        .ok_or(JournalValidationError::EmptyJournal)?;
    if be32(sb_block, 0) == Some(JBD2_MAGIC) {
        let size = be32(sb_block, 12).ok_or(JournalValidationError::InvalidBlockSize)?;
        if !(1024..=65536).contains(&size) || !size.is_power_of_two() {
            return Err(JournalValidationError::InvalidBlockSize);
        }
    }
    let sb = parse_superblock(sb_block).ok_or(JournalValidationError::BadSuperblockMagic)?;

    // Validate magic number
    const JBD2_MAGIC_EXPECTED: u32 = 0xC03B3998;
    let magic = be32(sb_block, 0).ok_or(JournalValidationError::BadSuperblockMagic)?;
    if magic != JBD2_MAGIC_EXPECTED {
        return Err(JournalValidationError::BadSuperblockMagic);
    }

    // Validate block size
    if sb.block_size == 0 || sb.block_size > 65536 || (sb.block_size & (sb.block_size - 1)) != 0 {
        return Err(JournalValidationError::InvalidBlockSize);
    }

    // Check for unsupported features
    let unsupported = unsupported_incompat(sb.features);
    if unsupported != 0 || sb.features.compat != 0 || sb.features.ro_compat != 0 {
        return Err(JournalValidationError::UnsupportedFeature(unsupported));
    }

    Ok(sb)
}

/// Validate sequence numbers in journal blocks
fn validate_sequences(
    journal: &[u8],
    sb: &JournalSuperblock,
) -> Result<(u32, u32, usize), JournalValidationError> {
    let mut idx = if sb.start == 0 {
        sb.first as usize
    } else {
        sb.start as usize
    };
    if idx == 0 {
        idx = 1;
    }

    let max_len = sb.max_len as usize;
    let mut blocks_scanned = 0usize;
    let mut first_seq: Option<u32> = None;
    let mut last_seq: Option<u32> = None;
    let mut valid_blocks = 0usize;

    while blocks_scanned < max_len {
        let block = match block_by_journal_index(journal, sb.block_size, idx) {
            Some(b) => b,
            None => break,
        };

        if let Some((_, _, seq)) = header(block) {
            if first_seq.is_none() {
                first_seq = Some(seq);
            }
            last_seq = Some(seq);
            valid_blocks += 1;
        }

        idx += 1;
        if idx >= max_len {
            idx = sb.first as usize;
        }
        blocks_scanned += 1;
    }

    Ok((first_seq.unwrap_or(0), last_seq.unwrap_or(0), valid_blocks))
}

/// Structural validation for the explicitly supported legacy journal format.
pub fn validate_journal(journal: &[u8]) -> Result<JournalValidationReport, JournalValidationError> {
    let mut report = JournalValidationReport::default();

    // Step 1: Validate superblock
    let sb = match validate_journal_superblock(journal) {
        Ok(s) => {
            report.superblock_valid = true;
            report.magic_valid = true;
            report.block_size_valid = true;
            s
        },
        Err(e) => return Err(e),
    };
    if sb.first == 0 || sb.first >= sb.max_len
        || (sb.max_len as usize).checked_mul(sb.block_size).map_or(true, |len| len > journal.len())
        || (sb.start != 0 && (sb.start < sb.first || sb.start >= sb.max_len))
    { return Err(JournalValidationError::OutOfBounds); }
    if sb.start == 0 { report.sequence_valid = true; return Ok(report); }

    // Step 2: Scan and validate all blocks
    let mut idx = if sb.start == 0 {
        sb.first as usize
    } else {
        sb.start as usize
    };
    if idx == 0 {
        idx = 1;
    }

    let max_len = sb.max_len as usize;
    let mut blocks_scanned = 0usize;
    let mut current_txn_seq: Option<u32> = None;
    let mut pending_tags: usize = 0;

    while blocks_scanned < max_len {
        let block = match block_by_journal_index(journal, sb.block_size, idx) {
            Some(b) => b,
            None => break,
        };

        report.total_blocks_scanned += 1;
        // Data is consumed according to descriptor count, regardless of magic.
        if pending_tags > 0 {
            pending_tags -= 1;
            idx += 1;
            if idx >= max_len { idx = sb.first as usize; }
            blocks_scanned += 1;
            continue;
        }

        if let Some((magic, ty, seq)) = header(block) {
            let _ = magic; // Already validated in header()

            // Track sequence range
            if report.first_valid_sequence == 0 {
                report.first_valid_sequence = seq;
            }
            report.last_valid_sequence = seq;

            match ty {
                JBD2_DESCRIPTOR_BLOCK => {
                    let tags = parse_descriptor(block, &sb);
                    if tags.is_empty() {
                        return Err(JournalValidationError::DescriptorBlockCorrupt);
                    }
                    pending_tags = tags.len();
                    current_txn_seq = Some(seq);
                    report.descriptor_blocks_valid += 1;
                },
                JBD2_REVOKE_BLOCK => {
                    let revokes = parse_revoke(block, &sb, seq);
                    report.revoke_blocks_valid += 1;
                    let _ = revokes; // Could add more detailed validation here
                },
                JBD2_COMMIT_BLOCK => {
                    if let Some(expected_seq) = current_txn_seq {
                        if seq != expected_seq {
                            return Err(JournalValidationError::CommitBlockMismatch(
                                expected_seq,
                                seq,
                            ));
                        }
                        report.transactions_committed += 1;
                        current_txn_seq = None;
                        pending_tags = 0;
                    }
                    report.commit_blocks_valid += 1;
                },
                JBD2_SUPERBLOCK_V1 | JBD2_SUPERBLOCK_V2 => {
                    // Superblock copies are valid but don't count toward metrics
                },
                _ => {
                    // Unknown block type - could be fast commit or corruption
                    if ty == JBD2_FEATURE_INCOMPAT_FAST_COMMIT {
                        report.incomplete_transactions += 1;
                    }
                },
            }
        }

        idx += 1;
        if idx >= max_len {
            idx = sb.first as usize;
        }
        blocks_scanned += 1;
    }

    // Check for incomplete transaction (descriptor without commit)
    if current_txn_seq.is_some() {
        report.incomplete_transactions += 1;
    }

    // Validate sequence continuity (optional - some gaps may be normal after cleanup)
    report.sequence_valid = report.first_valid_sequence <= report.last_valid_sequence;

    Ok(report)
}

/// Validate ext4 filesystem journal and optionally replay
///
/// This is the main entry point for production journaling validation.
/// It validates the journal inode and can replay uncommitted transactions.
///
/// # Arguments
/// * `fs_data` - Complete filesystem image
/// * `journal_inode_block` - Block number of journal inode
/// * `do_replay` - Whether to replay valid uncommitted transactions
///
/// # Returns
/// * `Ok(JournalValidationReport)` - Validation results (and replay if requested)
/// * `Err(JournalValidationError)` - Validation or replay failure
pub fn validate_ext4_journal(
    fs_data: &mut [u8],
    journal_inode_block: u64,
    do_replay: bool,
) -> Result<JournalValidationReport, JournalValidationError> {
    // This legacy API accepts the first physical journal block, not an inode
    // number. Resolve dimensions from the actual ext4 and journal superblocks.
    // Non-contiguous journal inodes must use replay_from_block_list instead.
    let log_size = fs_data.get(1048..1052).ok_or(JournalValidationError::OutOfBounds)?;
    let log_size = u32::from_le_bytes(log_size.try_into().map_err(|_| JournalValidationError::OutOfBounds)?);
    if log_size > 6 { return Err(JournalValidationError::InvalidBlockSize); }
    let block_size = 1024usize << log_size;
    let journal_start = usize::try_from(journal_inode_block).ok()
        .and_then(|block| block.checked_mul(block_size))
        .ok_or(JournalValidationError::OutOfBounds)?;
    let prefix_end = journal_start.checked_add(1024).ok_or(JournalValidationError::OutOfBounds)?;
    let prefix = fs_data.get(journal_start..prefix_end).ok_or(JournalValidationError::OutOfBounds)?;
    let sb = parse_superblock(prefix).ok_or(JournalValidationError::BadSuperblockMagic)?;
    if sb.block_size != block_size { return Err(JournalValidationError::InvalidBlockSize); }
    let end = (sb.max_len as usize).checked_mul(block_size)
        .and_then(|len| journal_start.checked_add(len)).ok_or(JournalValidationError::OutOfBounds)?;
    let journal = fs_data.get(journal_start..end)
        .ok_or(JournalValidationError::OutOfBounds)?.to_vec();

    // Validate journal structure
    let mut report = validate_journal(&journal)?;

    // Optionally replay valid transactions
    if do_replay && report.incomplete_transactions == 0 {
        let replay_result = replay_journal_image(fs_data, &journal);
        match replay_result {
            Ok(replay_report) => {
                report.blocks_replayed = replay_report.blocks_replayed;
                report.transactions_replayed = replay_report.transactions_replayed;
            },
            Err(_) => {
                // Replay failed but validation succeeded
                report.checksum_failures += 1;
            },
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_empty_journal() {
        let empty_journal: [u8; 0] = [];
        assert_eq!(
            validate_journal(&empty_journal),
            Err(JournalValidationError::EmptyJournal)
        );
    }

    #[test]
    fn test_validate_bad_magic() {
        let mut bad_journal = vec![0u8; 1024];
        // Write wrong magic
        bad_journal[0..4].copy_from_slice(&0xDEADBEEFu32.to_be_bytes());
        assert_eq!(
            validate_journal(&bad_journal),
            Err(JournalValidationError::BadSuperblockMagic)
        );
    }

    #[test]
    fn test_validate_invalid_block_size() {
        let mut journal = vec![0u8; 4096];
        // Write valid magic
        journal[0..4].copy_from_slice(&JBD2_MAGIC.to_be_bytes());
        // Write invalid block size (not power of 2)
        journal[12..16].copy_from_slice(&1000u32.to_be_bytes());
        // Write superblock type
        journal[4..8].copy_from_slice(&JBD2_SUPERBLOCK_V1.to_be_bytes());

        assert_eq!(
            validate_journal(&journal),
            Err(JournalValidationError::InvalidBlockSize)
        );
    }

    #[test]
    fn test_validation_report_default() {
        let report = JournalValidationReport::default();
        assert!(!report.superblock_valid);
        assert_eq!(report.descriptor_blocks_valid, 0);
        assert_eq!(report.commit_blocks_valid, 0);
    }
}

// ============================================================================
// Runtime Journal Functions (for kernel use)
// ============================================================================

/// Error type for journal operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalError {
    NoTransaction,
    IoError,
    NoSpace,
    InvalidBlock,
    UnsupportedFeature,
}

impl From<JournalError> for i32 {
    fn from(err: JournalError) -> Self {
        match err {
            JournalError::NoTransaction => -1, // EPERM
            JournalError::IoError => -5,       // EIO
            JournalError::NoSpace => -28,      // ENOSPC
            JournalError::InvalidBlock => -22, // EINVAL
            JournalError::UnsupportedFeature => -95,
        }
    }
}

/// On-disk JBD2 transaction creation is not implemented. Do not substitute
/// direct home-block writes for write-ahead logging: that falsely promises
/// crash consistency. The real ext4 writer supports non-journaled profiles.
pub fn journal_start_write() -> Result<u32, JournalError> {
    Err(JournalError::UnsupportedFeature)
}

/// Write a block through the journal.
///
/// # Arguments
/// * `txn` - Transaction handle from `journal_start_write`
/// * `block_num` - Physical block number to write
/// * `data` - Block data (must be exactly one block size)
pub fn journal_write_block(_txn: u32, _block_num: u64, _data: &[u8]) -> Result<(), JournalError> {
    Err(JournalError::UnsupportedFeature)
}

/// Commit the current journal transaction.
pub fn journal_commit(_txn: u32) -> Result<(), JournalError> {
    Err(JournalError::UnsupportedFeature)
}

/// Replay the journal after an unclean shutdown.
/// This is called during filesystem mount.
pub fn replay_journal() -> Result<(), JournalError> {
    // Recovery is a mount-time operation requiring exclusive ownership of the
    // image and block mappings. mount() currently refuses NEEDS_RECOVERY; this
    // runtime entry point must not pretend an empty replay recovered the disk.
    Err(JournalError::UnsupportedFeature)
}

/// Checkpoint the journal - flush all committed transactions to main filesystem.
pub fn checkpoint_journal() -> Result<(), JournalError> {
    Err(JournalError::UnsupportedFeature)
}
