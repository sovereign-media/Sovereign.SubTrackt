//! Segmented glyphs and the fixed-length feature vectors they are matched by.
//!
//! The matcher is deliberately not a general OCR engine. A glyph is normalised onto a fixed grid,
//! flattened to a bit vector, and compared against reference vectors by Hamming distance. The
//! consequence that matters is that a glyph the reference set does not contain fails *loudly* —
//! see [`crate::Error::UnmatchedGlyph`].

use crate::bitmap::Rect;

/// Edge length of the normalisation grid a glyph is resampled onto.
///
/// 16 gives a 256-bit vector, which is four `u64` words and fits comfortably in registers.
///
/// 32 was measured against it and **is not better**: one harness makes it several points of CER
/// better on a mismatched typeface, another makes it a wash across four of them with match coverage
/// down on every one, and the shape vector's own separation statistic slightly worsens. See
/// `docs/glyph-stability.md`. Everything downstream is a fraction of [`FEATURE_BITS`], so changing
/// this constant is a one-line experiment. It was not always: `MatchThresholds::metric_weight` held
/// a cell count until #45, and doubling the grid quietly un-tuned the matcher.
pub const FEATURE_GRID: usize = 16;

/// Number of bits in a [`FeatureVector`].
pub const FEATURE_BITS: usize = FEATURE_GRID * FEATURE_GRID;

/// Number of 64-bit words a [`FeatureVector`] occupies.
pub const FEATURE_WORDS: usize = FEATURE_BITS / 64;

/// A glyph normalised to a fixed-length bit vector.
///
/// Bit `i` is set when cell `i` of the row-major normalisation grid is foreground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FeatureVector {
    words: [u64; FEATURE_WORDS],
}

impl FeatureVector {
    /// The all-background vector.
    pub const EMPTY: Self = Self { words: [0; FEATURE_WORDS] };

    /// Build a vector from its raw words.
    #[must_use]
    pub const fn from_words(words: [u64; FEATURE_WORDS]) -> Self {
        Self { words }
    }

    /// The raw words, for serialising reference data.
    #[must_use]
    pub const fn words(&self) -> &[u64; FEATURE_WORDS] {
        &self.words
    }

    /// Set the bit for a grid cell. Indices past the end of the grid are ignored.
    pub fn set(&mut self, index: usize) {
        if index < FEATURE_BITS {
            self.words[index / 64] |= 1 << (index % 64);
        }
    }

    /// Whether the bit for a grid cell is set.
    #[must_use]
    pub const fn get(&self, index: usize) -> bool {
        index < FEATURE_BITS && (self.words[index / 64] >> (index % 64)) & 1 == 1
    }

    /// Number of foreground cells.
    #[must_use]
    pub fn popcount(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }

    /// Hamming distance to another vector: the number of cells that disagree.
    ///
    /// This compiles to four `xor` + `popcnt` pairs, which is the whole reason the vector is a
    /// fixed-size array rather than a `Vec`.
    #[must_use]
    pub fn distance(&self, other: &Self) -> u32 {
        self.words
            .iter()
            .zip(other.words.iter())
            .map(|(a, b)| (a ^ b).count_ones())
            .sum()
    }

    /// A stable 64-bit key for the session cache described in the architecture document.
    #[must_use]
    pub fn cache_key(&self) -> u64 {
        // FNV-1a over the raw words: cheap, and collisions only cost a re-match.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for word in self.words {
            for byte in word.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash
    }
}

/// Where a glyph sits within its text line, relative to that line's own metrics.
///
/// The feature vector cannot express this and deliberately so: normalisation letterboxes every
/// glyph to fill the grid, which is what makes a 480p and a 1080p render of one character agree.
/// The cost is that it also makes an `o` and an `O` agree — #10 measured `I`, `l` and `|` at
/// distance *zero* from one another — because within a shape vector they differ in nothing but
/// size, and size is exactly what was normalised away.
///
/// Both figures are percentages of the line's cap height rather than pixel counts, so they survive
/// a resolution change the way the rest of the pipeline's thresholds do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LineMetrics {
    /// Glyph height as a percentage of the line's cap height.
    ///
    /// Around 100 for a capital, 75 for a lowercase x-height letter in a typical sans face, more
    /// than 100 for a round capital's overshoot or an accented one.
    pub height_percent: u32,
    /// How far the glyph's bottom sits below the line's baseline, as a percentage of cap height.
    ///
    /// Zero for most characters, positive for a descender, and *negative* for something that floats
    /// clear of the baseline — a hyphen, an apostrophe, a quotation mark. Which is why it is
    /// signed: treating a floating mark as sitting on the baseline would merge `-` with `_`.
    pub descent_percent: i32,
    /// Whether these figures were measurable at all.
    ///
    /// A line with too few glyphs to locate a baseline has no metrics, and saying so is the whole
    /// point: a fabricated 100% would be indistinguishable from a real one and would quietly bias
    /// every match on that line. An unknown metric contributes nothing to a distance instead.
    pub known: bool,
}

impl LineMetrics {
    /// Metrics that could not be measured.
    pub const UNKNOWN: Self = Self { height_percent: 0, descent_percent: 0, known: false };

    /// Measured metrics.
    #[must_use]
    pub const fn new(height_percent: u32, descent_percent: i32) -> Self {
        Self { height_percent, descent_percent, known: true }
    }

    /// How far apart two glyphs sit in metric terms, in percentage points.
    ///
    /// Returns `None` when either side is unknown, so a caller adds nothing rather than guessing.
    /// A glyph whose line was unmeasurable must fall back to shape alone, not be penalised for it.
    #[must_use]
    pub fn difference(self, other: Self) -> Option<u32> {
        if !self.known || !other.known {
            return None;
        }
        Some(
            self.height_percent.abs_diff(other.height_percent)
                + self.descent_percent.abs_diff(other.descent_percent),
        )
    }
}

/// One connected component (or diacritic group) lifted out of a subtitle image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    /// Where the glyph sat in the subtitle image, in plane coordinates.
    pub bounds: Rect,
    /// Index of the text line the glyph was assigned to, top to bottom.
    pub line: usize,
    /// The normalised feature vector.
    pub features: FeatureVector,
    /// Where the glyph sits in its line, which the feature vector cannot say.
    pub metrics: LineMetrics,
}

/// The result of matching one [`Glyph`] against the reference set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphMatch {
    /// The character the matcher settled on, or `None` when nothing was within threshold.
    pub character: Option<char>,
    /// Hamming distance to the winning reference vector.
    pub distance: u32,
    /// Distance to the runner-up. A close second is the signal that a read is ambiguous
    /// (`0` vs `O`, `1` vs `l`) and should be handed to post-correction rather than trusted.
    pub runner_up_distance: u32,
}

impl GlyphMatch {
    /// An unmatched glyph, carrying the best distance seen for diagnostics.
    #[must_use]
    pub const fn unmatched(best_distance: u32) -> Self {
        Self {
            character: None,
            distance: best_distance,
            runner_up_distance: u32::MAX,
        }
    }

    /// Whether the winner beat the runner-up by at least `margin` cells.
    ///
    /// Post-correction only needs to look at glyphs where this is false.
    #[must_use]
    pub const fn is_unambiguous(&self, margin: u32) -> bool {
        self.character.is_some() && self.runner_up_distance.saturating_sub(self.distance) >= margin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vector_is_four_words_wide() {
        assert_eq!(FEATURE_BITS, FEATURE_GRID * FEATURE_GRID);
        assert_eq!(FeatureVector::EMPTY.words().len(), FEATURE_BITS / 64);
    }

    #[test]
    fn distance_counts_disagreeing_cells() {
        let mut a = FeatureVector::EMPTY;
        let mut b = FeatureVector::EMPTY;
        a.set(0);
        a.set(200);
        b.set(0);
        assert_eq!(a.distance(&b), 1);
        assert_eq!(a.distance(&a), 0);
        assert_eq!(a.popcount(), 2);
    }

    #[test]
    fn set_ignores_indices_past_the_grid() {
        let mut v = FeatureVector::EMPTY;
        v.set(FEATURE_BITS);
        assert_eq!(v.popcount(), 0);
        assert!(!v.get(FEATURE_BITS));
    }

    #[test]
    fn distinct_vectors_get_distinct_cache_keys() {
        let mut a = FeatureVector::EMPTY;
        a.set(7);
        assert_ne!(a.cache_key(), FeatureVector::EMPTY.cache_key());
    }

    #[test]
    fn a_near_tie_is_reported_as_ambiguous() {
        let close = GlyphMatch { character: Some('0'), distance: 8, runner_up_distance: 9 };
        let clear = GlyphMatch { character: Some('A'), distance: 2, runner_up_distance: 40 };
        assert!(!close.is_unambiguous(6));
        assert!(clear.is_unambiguous(6));
        assert!(!GlyphMatch::unmatched(90).is_unambiguous(0));
    }
}
