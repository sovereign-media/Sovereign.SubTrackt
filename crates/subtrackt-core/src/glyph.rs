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

/// Where a glyph's ink stands once its line's slant has been divided out, in tenths of a pixel.
///
/// The fourth thing a bounding box gets wrong, and the one that fails *two stages before the
/// matcher*. A slanted ascender's box is mostly slant — Arial Italic draws `l` across 33% of cap
/// height where the ink itself is a 12.75% stem — so an italic letter's box overhangs the box of
/// the letter after it. `subtrackt-text`'s spacing rule measures the space between two glyphs as
/// `next.x - this.right()`, saturating at zero, and on a real Blu-ray **27% of an italic line's
/// gaps arrive already saturated against 0.7% of an upright line's**, half of them from boxes that
/// genuinely overlap. #40's rule needs the line's gaps to separate into two classes; a run of
/// clamped zeros collapses the letter-gap mode onto the floor and takes the band with it.
///
/// This is the same ink measured along the line's own slant instead. `docs/italic-slant.md` has the
/// measurement and #121 the change.
///
/// **Tenths of a pixel, not pixels.** The unit is the finding here as much as it is for
/// [`InkAspect`]. A word gap on a 1080p disc is five or six pixels and a kerning gap is one or two,
/// so rounding each edge to a whole pixel puts a whole pixel of error on a quantity whose two
/// populations sit three pixels apart. #99, #110 and #113 were each one side of this pipeline
/// quantising away a difference the other side needed.
///
/// **Comparable only within one line.** The shear is applied about the line's own pivot, so two
/// spans from different lines are offset by different constants. That offset cancels in a
/// difference between neighbours, which is the only thing anything asks of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UprightSpan {
    /// Left edge of the deskewed ink, in tenths of a pixel.
    pub left: i32,
    /// One past the right edge of the deskewed ink, in tenths of a pixel.
    pub right: i32,
    /// Whether the line's slant could be measured at all.
    ///
    /// False for a line carrying too little ink or too few glyphs to estimate a shear from. A line
    /// that cannot be measured reports **unknown** and is *not* reported as upright: a caller then
    /// falls back to the bounding box, which is what it would have used anyway. Defaulting to zero
    /// shear would be a fabricated measurement, and `CLAUDE.md` has the rule.
    pub known: bool,
}

/// Tenths of a pixel in one pixel, the unit [`UprightSpan`] carries.
pub const SPAN_TENTHS: i32 = 10;

impl UprightSpan {
    /// A span whose line's slant could not be measured.
    pub const UNKNOWN: Self = Self { left: 0, right: 0, known: false };

    /// A measured span, in tenths of a pixel.
    #[must_use]
    pub const fn new(left: i32, right: i32) -> Self {
        Self { left, right, known: true }
    }

    /// The span a glyph's own bounding box implies, which is what a zero shear produces.
    ///
    /// Not a fallback dressed as a measurement: it is marked `known` because it *is* the answer
    /// when there is no slant to take out, and `a_zero_shear_span_is_the_glyph_box` pins the two
    /// together. Callers wanting the honest unknown want [`Self::UNKNOWN`].
    ///
    /// This is also what a glyph on a line whose slant could not be measured reports — its own
    /// bounding box, which is what the spacing rule used before #121 existed.
    /// [`line_shear`](../../subtrackt_glyph/slant/fn.line_shear.html) returning `None` is where
    /// that unknown lives; it is not this function's to express.
    #[must_use]
    pub const fn of_box(bounds: Rect) -> Self {
        // A subtitle plane is a few thousand pixels across, so a coordinate in tenths is nowhere
        // near `i32`'s range and the saturation below never fires. It is written rather than
        // asserted because a plane wide enough to reach it would be a decoder bug, and clamping a
        // span is a smaller wrong answer than wrapping one.
        Self::new(
            bounds
                .x
                .saturating_mul(SPAN_TENTHS.unsigned_abs())
                .cast_signed(),
            bounds
                .right()
                .saturating_mul(SPAN_TENTHS.unsigned_abs())
                .cast_signed(),
        )
    }

    /// The space between this glyph and the next one along the line, in tenths of a pixel.
    ///
    /// **Signed**, unlike the saturating subtraction it replaces. A negative gap is two glyphs whose
    /// ink still overlaps after the slant is out, which is a real and different fact from a gap of
    /// zero — and the one the runtime has never been able to see.
    ///
    /// `None` unless both sides carry a measurement, the contract [`LineMetrics::difference`],
    /// [`MarkSlope::difference`] and [`InkAspect::difference`] all keep.
    #[must_use]
    pub const fn gap_to(self, next: Self) -> Option<i32> {
        if !self.known || !next.known {
            return None;
        }
        Some(next.left - self.right)
    }
}

/// How far the text line a glyph stands on leans, in tenths of a percent of a slope.
///
/// The fifth thing a glyph carries that its own shape cannot say, and the only one that is a
/// property of the *line* rather than of the character. `x' = x - k·y` with `k = Cxy / Cyy` over
/// the line's ink: the shear that makes the covariance cross term vanish, which is what "the stems
/// now stand vertical" means as an equation.
///
/// A slope is already dimensionless, so unlike [`LineMetrics`] this needs no cap height to divide
/// by and unlike [`InkAspect`] it is not a ratio of two measurements of one glyph. It survives a
/// resolution change because it never had a unit to lose.
///
/// **Its sign follows the plane's.** y grows downward, so an italic leaning right at the top has a
/// *negative* shear. Two real Blu-rays read -155 and -160 against Arial Italic's own -173.
///
/// **`known` means "shown to lean", not "measured".** A line carrying too little ink to estimate a
/// shear and a line whose estimate sits inside what the estimator shows on upright material are the
/// same answer to the only question anything asks — *has this line been shown to lean?* — and both
/// report `false`. That collapse is deliberate: `docs/italic-slant.md` measured the two populations
/// as not coming close to touching, so a value inside the band is not a small lean but an absence
/// of evidence, and reporting it as a lean would be inventing a measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Slant {
    /// The shear, in tenths of a percent. About -160 for Arial Italic on a real disc.
    pub permille: i32,
    /// Whether the line was shown to lean at all.
    pub known: bool,
}

impl Slant {
    /// A line that was not shown to lean.
    pub const UPRIGHT: Self = Self { permille: 0, known: false };

    /// A measured lean, in tenths of a percent.
    #[must_use]
    pub const fn new(permille: i32) -> Self {
        Self { permille, known: true }
    }

    /// Whether this line leans **the way an italic leans** — which is what an `<i>` is written from.
    ///
    /// Directional, and that is not the same question the shear floor answers. Deskewing is
    /// geometry and a line that leans either way is worth standing upright; an italic is
    /// *typography*, and no Latin emphasis face leans left. A back-slanted estimate is therefore a
    /// line whose letters happen to carry diagonal ink — `A`, `V`, `w`, `y` — rather than a face
    /// choice, and tagging it would be reporting the alphabet.
    ///
    /// Measured: on A Fish Called Wanda, treating both signs as italic tagged 104 upright cues of
    /// 1,279; requiring the italic direction takes that to a fraction of it. `docs/italic-slant.md`
    /// has the figure.
    #[must_use]
    pub const fn leans(self) -> bool {
        self.known && self.permille < 0
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
    /// Where its ink stands once its line's slant is divided out, which its box gets wrong.
    pub upright: UprightSpan,
    /// How far the line it stands on leans, which is the only thing here that is not about it.
    pub slant: Slant,
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
    fn a_line_that_was_not_shown_to_lean_does_not_lean() {
        assert!(!Slant::UPRIGHT.leans());
        assert!(!Slant::default().leans());
    }

    #[test]
    fn only_a_line_leaning_the_way_an_italic_leans_is_italic() {
        // Directional on purpose, and it is worth 14 false tags on one disc and 74 on another.
        // The plane's y grows downward, so an italic reads negative; a positive estimate is a line
        // whose letters carry diagonal ink -- `A`, `V`, `w`, `y` -- and no Latin emphasis face
        // leans that way.
        assert!(Slant::new(-160).leans());
        assert!(!Slant::new(160).leans());
    }

    #[test]
    fn a_measured_lean_keeps_the_number_that_was_measured() {
        // The flag is what the output reads; the figure is what a bench does. Collapsing the two
        // would leave nothing to score a threshold against later.
        assert_eq!(Slant::new(-173).permille, -173);
        assert!(Slant::new(-173).known);
    }
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
