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

/// Which way a glyph's diacritic leans.
///
/// The second thing the shape vector cannot express, and for the same reason as [`LineMetrics`].
/// Letterboxing scales the merged bounding box — base plus mark — to fill the grid, so a mark
/// occupying the top sixth of the glyph lands in one or two rows of cells while everything below it
/// is identical between `à` and `á`. The distance is then dominated by the part carrying no
/// information, and #46 found sixteen such pairs among the 21 the matcher calls ambiguous — more
/// than four times what `l`/`I` accounts for.
///
/// One signed number, because #48 measured the alternatives: the mark's own feature vector
/// separates the same sixteen pairs but clears its own rendering noise by a factor of 1.6, where
/// this clears it by ten to twenty. See `docs/glyph-stability.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MarkSlope {
    /// The normalised second moment cross term of the mark's ink, as a percentage.
    ///
    /// Positive where the ink falls left to right in image coordinates — a grave, around +67 —
    /// and negative where it rises — an acute, around −65. A mark symmetric about its vertical
    /// axis has no direction and reads 0, which puts a circumflex *between* the two rather than
    /// beside one. That is what lets a single number separate three marks.
    pub percent: i32,
    /// Whether this glyph has a mark whose direction could be measured at all.
    ///
    /// False for the great majority of characters, which carry no mark, and false for a mark too
    /// small to have a direction — fewer than three ink pixels lie on a line by construction, so a
    /// slope read off them reports the pixel count rather than the letterform.
    ///
    /// It is also false where `group` failed to attach a mark that exists. That case is real: an
    /// accent over a *capital* sits above every letterform the charset can spell, so the row under
    /// it is blank across the line and it bands as a line of its own. Treating all three the same
    /// way is deliberate — see [`Self::difference`].
    pub known: bool,
}

impl MarkSlope {
    /// No mark, or none that could be measured.
    pub const NONE: Self = Self { percent: 0, known: false };

    /// A measured slope.
    #[must_use]
    pub const fn new(percent: i32) -> Self {
        Self { percent, known: true }
    }

    /// How far apart two marks lean, in percentage points.
    ///
    /// `None` unless *both* sides carry a measured mark, which is the same contract
    /// [`LineMetrics::difference`] keeps and it matters more here. A glyph whose mark failed to
    /// group is indistinguishable from one that never had a mark, so charging it for the
    /// difference would penalise a segmentation failure by rejecting the character it actually
    /// resembles. Falling back to shape alone is worse than the full comparison and much better
    /// than scoring against a mark that was never delivered.
    #[must_use]
    pub const fn difference(self, other: Self) -> Option<u32> {
        if !self.known || !other.known {
            return None;
        }
        Some(self.percent.abs_diff(other.percent))
    }
}

/// How wide a glyph's ink stands, relative to its own height.
///
/// The third thing the shape vector cannot express, and the one that took longest to find because
/// it looks as though the vector already has it. Letterboxing preserves exactly this ratio — that is
/// the whole reason normalisation letterboxes rather than stretching — but it preserves it *onto
/// sixteen cells*. An `I` is 7% wider than an `l` in Arial's outlines, which at a cap height of 42
/// pixels is four tenths of one pixel and a fifth of one grid cell. The information is in the ink
/// and the quantisation is what loses it, so carrying the ratio as its own number is not a new
/// feature so much as the same one at a resolution that keeps it. `docs/error-census.md` has the
/// measurement: on a real disc it separates `l` from `I` at two glyphs in 867.
///
/// **Against the glyph's own height, not against the line's cap height**, and that is a measured
/// decision rather than an obvious one. Cap-relative width was tried first and is strictly more
/// informative — it says how big the character is as well as how wide — but it inherits every error
/// in the line metrics, and a line whose cap height is found at the x-height instead measures every
/// glyph on it a third too wide. `docs/glyph-stability.md` records what that cost: 32 `o` read as
/// `C` and 20 `s` as `S`, on the lines where the cap height was already wrong. This ratio is a
/// property of one component's own bounding box, so it is right on a line nothing else could
/// measure — and #37's height term supplies the size the two of them together would have carried
/// anyway.
///
/// **Tenths of a percent, not percent.** The gap between an `l` and an `I` is eight tenths of one
/// percent; in whole percent it is a difference of zero or one, and one point priced at #37's
/// weight rounds to nothing at all. The unit is the finding as much as the number is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InkAspect {
    /// Ink width as tenths of a percent of ink height.
    ///
    /// About 123 for Arial's `l` and 131 for its `I`; about 960 for an `o`, which is very nearly
    /// round; over 2000 for a `w`, which is twice as wide as it is tall.
    pub permille: u32,
    /// Whether it could be measured at all.
    ///
    /// False only for a glyph with no height, which a component filter should never let through,
    /// and for a character its face draws no outline for. Unlike [`LineMetrics`] this does not
    /// depend on the line being measurable, which is the point of it.
    pub known: bool,
}

impl InkAspect {
    /// An aspect ratio that could not be measured.
    pub const UNKNOWN: Self = Self { permille: 0, known: false };

    /// A measured ratio, in tenths of a percent.
    #[must_use]
    pub const fn new(permille: u32) -> Self {
        Self { permille, known: true }
    }

    /// Measure a box of ink.
    ///
    /// Returns [`Self::UNKNOWN`] for a box with no height rather than dividing by zero.
    #[must_use]
    pub const fn measure(width: u32, height: u32) -> Self {
        if height == 0 {
            return Self::UNKNOWN;
        }
        Self::new(width * 1000 / height)
    }

    /// How far apart two ratios are, in tenths of a percent.
    ///
    /// `None` unless both sides carry a measurement, the contract [`LineMetrics::difference`] and
    /// [`MarkSlope::difference`] both keep: a reference set generated before this field existed
    /// contributes nothing rather than being scored against a ratio it never carried.
    #[must_use]
    pub const fn difference(self, other: Self) -> Option<u32> {
        if !self.known || !other.known {
            return None;
        }
        Some(self.permille.abs_diff(other.permille))
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
    /// Which way its diacritic leans, which the feature vector cannot say either.
    pub mark: MarkSlope,
    /// How wide its ink stands against its own height, which the vector says only to within a
    /// grid cell.
    pub aspect: InkAspect,
}

/// The result of matching one [`Glyph`] against the reference set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphMatch {
    /// The character the matcher settled on, or `None` when nothing was within threshold.
    pub character: Option<char>,
    /// Hamming distance to the winning reference vector.
    pub distance: u32,
    /// Distance to the nearest reference for a **different character**. A close second is the
    /// signal that a read is ambiguous (`0` vs `O`, `1` vs `l`) and should be handed to
    /// post-correction rather than trusted.
    ///
    /// "Different character" rather than "next nearest entry", and the distinction is the whole
    /// meaning of the field. A reference set may hold several entries for one character — one per
    /// [`Style`](crate::glyph) once anything populates that byte — and the second-nearest entry is
    /// then *the same letter in another weight*. Reporting that as a runner-up would say the
    /// matcher could not decide between `a` and `a`, and every glyph in a track carrying style
    /// variants would come back ambiguous.
    ///
    /// The ambiguity margin exists to flag a glyph the matcher could not call between two
    /// **characters**. Nothing has ever generated a set with duplicates, so this cost nothing until
    /// something did.
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
    fn an_aspect_ratio_is_measured_against_the_glyph_own_height() {
        // #109, and the property the whole feature turns on: nothing about the *line* enters it.
        // A line whose cap height was found at the x-height measures every glyph on it a third too
        // tall, and this ratio is right anyway.
        assert_eq!(InkAspect::measure(5, 42).permille, 119);
        assert_eq!(InkAspect::measure(6, 42).permille, 142);
        assert_eq!(InkAspect::measure(28, 31).permille, 903, "an `o` is very nearly round");
    }

    #[test]
    fn whole_percent_would_lose_the_decision_this_unit_exists_to_make() {
        // The unit is the finding as much as the number is, and the case that needs it is not the
        // pair the field was built for. A real disc draws `s` at 75.8% of its own height, and a set
        // carries `s` at 77.4 and `S` at 73.8 — so the observation is 1.6 from one and 2.0 from the
        // other, and the nearer one is right. Rounded to whole percent both distances are 2 and the
        // decision is a coin toss.
        let observed = InkAspect::new(758);
        let (lower, upper) = (InkAspect::new(738), InkAspect::new(774));
        assert!(observed.difference(upper) < observed.difference(lower));
        assert_eq!(
            (observed.permille / 10).abs_diff(upper.permille / 10),
            (observed.permille / 10).abs_diff(lower.permille / 10),
            "in whole percent the two are equidistant and nothing decides it"
        );
    }

    #[test]
    fn a_glyph_with_no_height_has_no_aspect_rather_than_a_division_by_zero() {
        assert_eq!(InkAspect::measure(5, 0), InkAspect::UNKNOWN);
    }

    #[test]
    fn an_unmeasured_aspect_contributes_no_distance_rather_than_a_penalty() {
        // The contract `LineMetrics` and `MarkSlope` both keep, and here it is what lets a
        // reference set generated before this field existed keep working: its entries carry no
        // ratio, so the term is not applied to them at all.
        let known = InkAspect::new(123);
        assert_eq!(known.difference(InkAspect::UNKNOWN), None);
        assert_eq!(InkAspect::UNKNOWN.difference(known), None);
        assert_eq!(known.difference(known), Some(0));
    }

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
