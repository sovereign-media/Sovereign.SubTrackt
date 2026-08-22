//! Segmented glyphs and the fixed-length feature vectors they are matched by.
//!
//! The matcher is deliberately not a general OCR engine. A glyph is normalised onto a fixed grid,
//! flattened to a bit vector, and compared against reference vectors by Hamming distance. The
//! consequence that matters is that a glyph the reference set does not contain fails *loudly* —
//! see [`crate::Error::UnmatchedGlyph`].

use crate::bitmap::Rect;

/// Edge length of the normalisation grid a glyph is resampled onto.
///
/// 16 gives a 256-bit vector, which is four `u64` words and fits comfortably in registers. The
/// architecture document leaves 16 vs 32 open; benchmarking that trade-off is tracked separately.
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

/// One connected component (or diacritic group) lifted out of a subtitle image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    /// Where the glyph sat in the subtitle image, in plane coordinates.
    pub bounds: Rect,
    /// Index of the text line the glyph was assigned to, top to bottom.
    pub line: usize,
    /// The normalised feature vector.
    pub features: FeatureVector,
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
        assert_eq!(FEATURE_BITS, 256);
        assert_eq!(FeatureVector::EMPTY.words().len(), 4);
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
