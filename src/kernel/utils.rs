//! Miscellaneous kernel utilities — canonical location: src/kernel/utils.rs
//!
//! Provides alignment, bit manipulation, and power-of-two utilities
//! with overflow protection and compile-time validation.

/// Aligns `val` up to the next multiple of `align`.
///
/// # Panics
/// Panics in debug mode if `align` is not a power of two.
/// Uses wrapping arithmetic to prevent overflow panics in release mode.
#[inline]
pub const fn align_up(val: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two(), "align must be power of 2");
    // Use wrapping arithmetic to prevent overflow panics in release mode
    val.wrapping_add(align.wrapping_sub(1)) & !align.wrapping_sub(1)
}

/// Aligns `val` down to the previous multiple of `align`.
///
/// # Panics
/// Panics in debug mode if `align` is not a power of two.
#[inline]
pub const fn align_down(val: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two(), "align must be power of 2");
    val & !align.wrapping_sub(1)
}

/// Returns `true` if `val` is aligned to `align`.
///
/// # Panics
/// Panics in debug mode if `align` is not a power of two.
#[inline]
pub const fn is_aligned(val: usize, align: usize) -> bool {
    debug_assert!(align.is_power_of_two(), "align must be power of 2");
    val & align.wrapping_sub(1) == 0
}

/// Rounds `val` down to the nearest power of two.
/// Returns 0 for input 0.
#[inline]
pub const fn round_down_pow2(mut val: usize) -> usize {
    if val == 0 {
        return 0;
    }
    // Set all bits below the highest set bit, then isolate the highest bit
    val |= val >> 1;
    val |= val >> 2;
    val |= val >> 4;
    val |= val >> 8;
    val |= val >> 16;
    #[cfg(target_pointer_width = "64")]
    {
        val |= val >> 32;
    }
    val - (val >> 1)
}

/// Rounds `val` up to the nearest power of two.
/// Returns 1 for input 0.
#[inline]
pub const fn round_up_pow2(mut val: usize) -> usize {
    if val == 0 {
        return 1;
    }
    val = val.wrapping_sub(1);
    val |= val >> 1;
    val |= val >> 2;
    val |= val >> 4;
    val |= val >> 8;
    val |= val >> 16;
    #[cfg(target_pointer_width = "64")]
    {
        val |= val >> 32;
    }
    val.wrapping_add(1)
}

/// Returns a mask with the lowest `bits` bits set to 1.
/// Returns all 1s if `bits >= usize::BITS`.
#[inline]
pub const fn bit_mask(bits: usize) -> usize {
    if bits >= usize::BITS as usize {
        usize::MAX
    } else {
        (1usize << bits).wrapping_sub(1)
    }
}

/// Extracts bits from position `start` (inclusive) to `end` (exclusive).
/// Equivalent to `(val >> start) & ((1 << (end - start)) - 1)`.
#[inline]
pub const fn extract_bits(val: usize, start: usize, end: usize) -> usize {
    (val >> start) & bit_mask(end.wrapping_sub(start))
}

/// Inserts `field` into `val` at positions `start` to `end`.
/// Bits outside the field range are preserved.
#[inline]
pub const fn insert_bits(val: usize, field: usize, start: usize, end: usize) -> usize {
    let mask = bit_mask(end.wrapping_sub(start)) << start;
    (val & !mask) | ((field & bit_mask(end.wrapping_sub(start))) << start)
}

/// Returns the number of leading zeros in `val`.
#[inline]
pub const fn leading_zeros(val: usize) -> u32 {
    val.leading_zeros()
}

/// Returns the number of trailing zeros in `val`.
#[inline]
pub const fn trailing_zeros(val: usize) -> u32 {
    val.trailing_zeros()
}

/// Returns `true` if `val` is a power of two.
#[inline]
pub const fn is_power_of_two(val: usize) -> bool {
    val != 0 && (val & (val - 1)) == 0
}

/// Returns the next power of two greater than or equal to `val`.
/// Same as `round_up_pow2` but with a more descriptive name.
#[inline]
pub const fn next_power_of_two(val: usize) -> usize {
    round_up_pow2(val)
}
