//! What the non-shape terms of a glyph distance cost.
//!
//! Three features exist because the feature vector cannot express them — how tall a glyph stands in
//! its line (#37), which way its diacritic leans (#48), and how wide its ink is against its own
//! height (#109) — and each needs an exchange rate against Hamming distance to be weighed at all.
//!
//! Those rates were written twice: once on `MatchThresholds`, for deciding which reference entry a
//! glyph is, and once on `ClusterRules`, for deciding which glyphs get one answer. So was the
//! weighted sum that spends them. The doc comments on both said, five times over, that the two
//! *must not* disagree — grouping is what decides which glyphs get one answer, so a term the
//! matcher weighs and the clusterer does not would merge an `l` with an `I` before the matcher ever
//! saw them apart, and those two are at Hamming distance zero.
//!
//! Five statements that two things must not drift is an argument for one implementation rather than
//! for two promises. This is it; both types now hold a `Weights` and neither does the arithmetic.

use subtrackt_core::{FEATURE_BITS, FeatureVector, InkAspect, LineMetrics, MarkSlope};

/// A glyph as everything that compares glyphs sees it.
///
/// Shape, where it sits in its line, which way its mark leans, how wide its ink stands.
pub type Shape = (FeatureVector, LineMetrics, MarkSlope, InkAspect);

/// The exchange rates between the three measured features and Hamming distance.
///
/// Every field is **tenths of a percent of [`FEATURE_BITS`]** rather than a cell count, and that is
/// the whole reason the type exists rather than three loose numbers. Holding a cell count is what
/// #45 did: the gap it priced between an `o` and an `O` was 5.5% of a 256-bit vector and 1.4% of a
/// 1024-bit one, so doubling the grid quietly stopped weighting the feature #37 added to separate
/// exactly that pair — measured at up to 12.8 points of CER, with nothing erroring and no counter
/// moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weights {
    /// What a full cap height of difference in line metrics costs.
    pub metric_permille: u32,
    /// What a full 100 points of difference in mark direction costs.
    pub mark_permille: u32,
    /// What a full cap height of difference in ink width costs.
    pub width_permille: u32,
}

impl Weights {
    /// A rate in cells, against an arbitrary vector length.
    ///
    /// Split out so a test can check the scale-free property across grid sizes. [`FEATURE_BITS`] is
    /// a compile-time constant, so a promise about what happens when it changes is otherwise
    /// unenforceable from inside one build — which is exactly how #45 survived unnoticed.
    #[must_use]
    pub const fn cells_at(permille: u32, bits: u32) -> u32 {
        bits * permille / 1000
    }

    /// What a full cap-height metric difference costs, in cells.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn metric(self) -> u32 {
        Self::cells_at(self.metric_permille, FEATURE_BITS as u32)
    }

    /// What a full 100 points of mark-direction difference costs, in cells.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn mark(self) -> u32 {
        Self::cells_at(self.mark_permille, FEATURE_BITS as u32)
    }

    /// What a full cap height of ink-width difference costs, in cells.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn width(self) -> u32 {
        Self::cells_at(self.width_permille, FEATURE_BITS as u32)
    }

    /// Distance between two shapes: Hamming on the vector, plus each measured term it can price.
    ///
    /// Each term is **omitted rather than defaulted** when either side lacks it. A glyph on a line
    /// too short to locate a baseline, or one whose accent never reached its body, is compared on
    /// what is left — which is worse than the full comparison and much better than being scored
    /// against a height or a direction that was never measured. `Measured` is where that contract
    /// lives.
    ///
    /// The metric and mark terms divide by 100 because their differences are counted in whole
    /// percentage points; the width term divides by 1000 because #109 measured in tenths, and the
    /// gap between an `l` and an `I` is eight tenths of one percent — a difference of zero or one
    /// in whole percent, which at these weights rounds to nothing at all.
    #[must_use]
    pub fn distance(self, a: &Shape, b: &Shape) -> u32 {
        let mut total = a.0.distance(&b.0);
        if let Some(points) = a.1.difference(b.1) {
            total += points * self.metric() / 100;
        }
        if let Some(points) = a.2.difference(b.2) {
            total += points * self.mark() / 100;
        }
        if let Some(points) = a.3.difference(b.3) {
            total += points * self.width() / 1000;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_is_the_same_fraction_of_the_vector_at_any_grid_size() {
        // #45, as arithmetic, and stated as the property that issue actually needed: doubling the
        // grid has to double what a term costs. A cell count held fixed instead is what silently
        // un-tuned the matcher when 16x16 was compared against 32x32 -- the same weight became a
        // quarter of the vector it had been.
        //
        // Not an exact ratio, because the conversion truncates: 256 x 196 / 1000 is 50 rather than
        // 50.176, which reads back as 195 tenths of a percent. One cell of slack is the arithmetic,
        // not the tuning.
        for bits in [256u32, 1024, 4096] {
            let single = Weights::cells_at(196, bits);
            let double = Weights::cells_at(196, bits * 2);
            assert!(double.abs_diff(single * 2) <= 1, "{bits} bits: {single} then {double}");
        }
    }

    #[test]
    fn an_unmeasured_term_is_omitted_rather_than_charged() {
        let weights = Weights { metric_permille: 196, mark_permille: 0, width_permille: 190 };
        let shape = |m: LineMetrics| (FeatureVector::EMPTY, m, MarkSlope::NONE, InkAspect::UNKNOWN);

        let known = shape(LineMetrics::new(100, 0));
        let unknown = shape(LineMetrics::UNKNOWN);
        assert_eq!(
            weights.distance(&known, &unknown),
            0,
            "nothing to compare, nothing charged"
        );
        assert!(weights.distance(&known, &shape(LineMetrics::new(72, 0))) > 0);
    }
}
