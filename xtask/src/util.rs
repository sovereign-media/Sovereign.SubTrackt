//! Helpers more than one command needs.
//!
//! Small things, and they are here because they were written twice. #144 found `median` in two
//! files, `nearest` in two more with the same body and the same doc comment, `stem` in another two,
//! and twelve copies of loading a reference set from disk. None of that was a decision; it is what
//! a directory of thirty single-purpose commands accumulates.
//!
//! **Deliberately not the percentile family.** There are four spellings of that across `fontid`,
//! `slant`, `refmatch` and `stability`, and they use *three different index formulas* — so merging
//! them is not deduplication, it is choosing which published figure to change. That is its own
//! question and it needs its own measurement.

use std::path::Path;

use anyhow::Context as _;
use subtrackt_glyph::ReferenceSet;

/// Load a `.subtref` from disk.
///
/// Twelve call sites wrote this out, ten of them wrapping the parse in
/// `map_err(|e| anyhow::anyhow!("{e}"))`. That adapter was never needed — `subtrackt_core::Error`
/// converts through `?` on its own — and it was actively harmful: it flattens a structured error
/// into a string and drops its `source()`, so an `Error::Io` arrived without the OS error under it.
pub fn load_reference(path: &Path) -> anyhow::Result<ReferenceSet> {
    let bytes =
        std::fs::read(path).with_context(|| format!("reading reference set {}", path.display()))?;
    ReferenceSet::decode(&bytes)
        .with_context(|| format!("parsing reference set {}", path.display()))
}

/// The median of a list, taking the upper of the two middles on an even count.
pub fn median(values: &mut [u32]) -> u32 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

/// A path's file stem, or `unnamed` for a path that has none.
pub fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "unnamed".to_owned(), |s| s.to_string_lossy().into_owned())
}

/// The value at the `percent`-th percentile of an already-sorted slice.
///
/// `None` for an empty slice, because a percentile of nothing is not zero — a character that
/// rasterises to no ink at one size has to drop out of the distribution rather than enter it as a
/// zero and flatter the result.
///
/// **The index is `round((len - 1) * percent / 100)`**, and #165 is the argument for that one out of
/// the three this tree had. Two things decide it, and a third that looked decisive does not.
///
/// - `(len - 1) * percent / 100` is the **standard position** — R's type 7, and what `numpy` and
///   `pandas` default to — so a figure in `docs/` can be reproduced by anyone who puts the same
///   numbers in a spreadsheet.
/// - It is the **least biased** of the three against that position. Swept over every sample size
///   from 2 to 400 and every whole percentage, its mean signed error is **+0.013** of an index,
///   against `floor(len * percent / 100)`'s +0.020 and `floor((len - 1) * percent / 100)`'s
///   **−0.470** — the last sits half an index low everywhere, which is what a floor does.
///
/// **It is not symmetric, and neither is either of the others.** Symmetry was the argument this was
/// first chosen on and it does not survive: where the position falls exactly between two samples,
/// any rule that returns a *sample* has to pick one, and the mirror breaks. Only an interpolating
/// percentile is symmetric. What is true is that this one is the closest — 1,039 asymmetric cases
/// over that sweep against 1,581 and 37,821 — and is never off a mirror by more than one index,
/// which `a_percentile_is_never_more_than_one_index_from_its_complement` pins.
///
/// Rounded in integers as `((len - 1) * percent + 50) / 100`, which is exactly
/// `round((len - 1) * percent / 100)` for whole percentages and keeps an integer path integer.
///
/// It does **not** interpolate. Every caller here reports an observed quantity — a cell count, a
/// pixel width, a measured shear — and a percentile that returned a value no glyph exhibited would
/// be a number with nothing behind it.
pub fn percentile<T: Copy>(sorted: &[T], percent: u32) -> Option<T> {
    sorted
        .get(percentile_index(sorted.len(), percent)?)
        .copied()
}

/// Which element of a sorted sample of `len` the `percent`-th percentile is.
///
/// [`percentile`] reads the value; this is for the callers that want the position — indexing a
/// parallel slice, or slicing at it. One formula reached two ways, because two spellings of an index
/// is how #165 happened in the first place.
///
/// `None` for an empty sample.
#[must_use]
pub fn percentile_index(len: usize, percent: u32) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(((len - 1) * percent.min(100) as usize + 50) / 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_percentile_is_never_more_than_one_index_from_its_complement() {
        // Not *equal* to it. Where the position falls exactly between two samples, a rule that
        // returns a sample has to pick one and the mirror breaks — so exact symmetry is available
        // only to an interpolating percentile, and none of these is one. That was the property this
        // index was first chosen on and it does not hold; what does, and what still chooses it over
        // the other two #165 found, is that it never misses by more than one and does not lean.
        for len in 2usize..400 {
            for percent in 0u32..=100 {
                let low = percentile_index(len, percent).unwrap();
                let mirrored = len - 1 - percentile_index(len, 100 - percent).unwrap();
                assert!(
                    low.abs_diff(mirrored) <= 1,
                    "{len} samples at p{percent}: {low} against {mirrored}"
                );
            }
        }
    }

    #[test]
    fn the_index_is_the_standard_position_rounded_to_a_sample() {
        // R's type 7 and `numpy`'s default put the p-th percentile at `(len - 1) * p / 100`; this
        // rounds that to the nearest actual sample rather than interpolating between two. Spelled
        // out here so the integer arithmetic in `percentile_index` can be checked against it.
        for len in [1usize, 2, 7, 64, 100, 999] {
            for percent in [0u32, 1, 25, 50, 75, 99, 100] {
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                #[allow(clippy::cast_sign_loss)]
                let expected = (((len - 1) as f64 * f64::from(percent) / 100.0).round()) as usize;
                assert_eq!(
                    percentile_index(len, percent).unwrap(),
                    expected,
                    "{len} samples at p{percent}"
                );
            }
        }
    }

    #[test]
    fn a_percentile_of_nothing_is_nothing_rather_than_zero() {
        // A character that rasterises to no ink at one size drops out of the distribution; entering
        // it as a zero would flatter every figure computed over it.
        assert_eq!(percentile::<u32>(&[], 50), None);
        assert_eq!(percentile_index(0, 50), None);
        assert_eq!(percentile(&[7u32], 95), Some(7));
    }

    #[test]
    fn the_ends_are_the_ends() {
        let sorted: Vec<u32> = (0..=100).collect();
        assert_eq!(percentile(&sorted, 0), Some(0));
        assert_eq!(percentile(&sorted, 100), Some(100));
        assert_eq!(percentile(&sorted, 50), Some(50));
    }
}
