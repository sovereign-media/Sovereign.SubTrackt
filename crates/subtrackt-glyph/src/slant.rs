//! How far a line of text leans, and where its ink would stand if it did not.
//!
//! [#121](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/121), from the measurement
//! in `docs/italic-slant.md`. A bounding box is the wrong shape for slanted ink: an italic
//! ascender's box is mostly slant, so it overhangs the box of the letter after it and the gap
//! between them — which is what `subtrackt-text`'s spacing rule ranks — arrives saturated at zero.
//! On a real Blu-ray that is **27% of an italic line's gaps against 0.7% of an upright line's**,
//! and the rule then declines to place any space at all on 16% of italic lines against 7.5% of
//! upright ones.
//!
//! What this produces is not a deskewed *bitmap*. Nothing here shears, resamples or moves a pixel:
//! [`upright_span`] asks where a glyph's existing ink would begin and end once its line stood
//! upright, and reports that. The distinction matters because #99, #110 and #113 were each one side
//! of this pipeline putting a measurement through a quantisation the other side never saw, and a
//! resampled mask is exactly that shape of change. Shearing the *sampling* the feature vector does
//! is a separate question with its own issue and its own bench.
//!
//! ## The estimator
//!
//! `k = Cxy / Cyy` over the line's ink: the shear that makes the covariance cross term vanish,
//! which is what "the stems now stand vertical" means as an equation. [`crate::mark::slope`] reads a
//! diacritic's direction from the same second moment — #48 measured one signed number separating
//! all sixteen accent pairs at ten to twenty times its noise, across three typefaces and six sizes
//! — so this is a second reading of machinery that was already measured rather than new machinery.
//!
//! Two properties it is written for, and both are pinned below:
//!
//! - **Each glyph contributes its covariance about its own centroid.** Pooling the raw pixels
//!   instead would measure the line's *layout*: a row of letters is far wider than it is tall, so
//!   its cross term would be dominated by where the words sit and by a baseline that any descender
//!   pulls off level.
//! - **The estimate is per line, not per glyph.** `A`, `V`, `w` and `y` have diagonal ink that is
//!   not slant, and #14 found slant to be one of the axes that is constant within a stream. A line
//!   is the unit that has it.
//!
//! It is a slope, so it is dimensionless and survives the resolution change `CLAUDE.md` requires
//! every threshold here to survive. Its sign follows the plane's: y grows downward, so an italic
//! leaning right at the top reports a **negative** shear. On two real discs it reads -0.155 and
//! -0.160 against Arial Italic's own -0.173.

use subtrackt_core::{SPACING_BANDS, SPAN_TENTHS, UprightBands, UprightSpan};

use crate::ccl::LabelMap;
use crate::group::GroupedGlyph;

/// Ink pixels a line needs before its slant is worth reading.
///
/// A shear estimate is a ratio of two second moments, and a second moment over a handful of pixels
/// is noise. `mark.rs` guards its own moment the same way and for the same reason. Below this the
/// line reports **unknown** and every glyph on it falls back to its bounding box — never to a
/// fabricated shear of zero, which is the boundary `CLAUDE.md` requires and the choice
/// `MarkSlope::NONE` and `LineMetrics::known` already make.
///
/// Two hundred is about one capital at 1080p and about six at the 21 px the library survey found at
/// the bottom of the range, so this is a floor on *evidence* rather than on resolution.
const MIN_INK: u64 = 200;

/// Glyphs a line needs before its slant is worth reading.
///
/// Separate from the ink floor because the two fail differently: one full stop clears neither, but
/// a single large `O` clears the ink floor while carrying no stem to lean. Both are required.
const MIN_GLYPHS: usize = 4;

/// Shear a line must reach before it is called leaning at all.
///
/// **Not a tuning constant — it is the spread this estimator has on upright material**, and reading
/// a line inside it as leaning is reporting a measurement that was not made. #115 named the
/// mechanism before any of this was built: the moment is sensitive to which letters are present,
/// and a line of `A`, `V`, `w` and `y` has diagonal ink that is not slant. `docs/italic-slant.md`
/// measured what that is worth, over 6,200 lines of three discs:
///
/// | | upright p10 | upright p90 | italic p75 |
/// | :--- | ---: | ---: | ---: |
/// | 10 Cloverfield Lane | -0.027 | +0.035 | -0.123 |
/// | A Fish Called Wanda | -0.026 | +0.043 | -0.106 |
///
/// The two populations do not come close to touching, and 0.06 sits in the empty band between them
/// — the same "cut where nothing is" that `SpacingRule::WidestSplit` is built on.
///
/// It has to be here rather than left to a caller because the alternative is not free. Every
/// non-zero shear widens an upright glyph's span, since deskewing ink that was never skewed leans
/// it — about `|k|` times the glyph's height, which at 0.03 over a 40-pixel capital is more than a
/// pixel. That pixel comes off *every* gap on the line, and a word gap on a real disc clears the
/// decisiveness test by two or three. Applying the estimator ungated cost 174 cues on Gone Girl
/// before this existed, all of them upright lines losing spaces they had.
///
/// Two-sided, because a shear is signed and nothing here should assume which way a face leans.
const MIN_SHEAR: f64 = 0.06;

/// The shear that would stand a line's ink upright: `x' = x - k·y`.
///
/// `None` where the line carries too little ink or too few glyphs to say — see the ink and glyph floors below — and
/// `None` again where what it found is inside the estimator's own spread on upright material, see
/// the shear floor. Both are facts rather than zeros, and every caller must treat them as such.
///
/// The two unknowns are deliberately not distinguished. A caller can do exactly one thing with
/// either: measure the glyph's box, which is what it would have measured anyway.
///
/// Computed in `f64` over a line a few thousand pixels wide, so the sums stay far inside what the
/// type represents exactly.
#[must_use]
pub fn line_shear(map: &LabelMap, glyphs: &[&GroupedGlyph]) -> Option<f64> {
    if glyphs.len() < MIN_GLYPHS {
        return None;
    }
    let (mut cyy, mut cxy, mut ink) = (0f64, 0f64, 0u64);
    for glyph in glyphs {
        for part in &glyph.parts {
            let (count, mean_x, mean_y) = centroid(map, part);
            if count == 0.0 {
                continue;
            }
            map.for_each(part.label, part.bounds, |x, y| {
                let (dx, dy) = (f64::from(x) - mean_x, f64::from(y) - mean_y);
                cyy += dy * dy;
                cxy += dx * dy;
            });
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                ink += count as u64;
            }
        }
    }
    if ink < MIN_INK || cyy <= 0.0 {
        return None;
    }
    let shear = cxy / cyy;
    (shear.abs() >= MIN_SHEAR).then_some(shear)
}

/// Where a glyph's ink would begin and end if its line stood upright.
///
/// The image of the ink under `x' = x - k·y`, taken over each pixel's **square** rather than its
/// top-left corner: a pixel at row `y` occupies rows `y..y+1`, and under a shear those two rows do
/// not map to the same column. Reading the corner alone would lose most of a stem's width at the
/// extremes, which is the whole quantity being measured.
///
/// Read through the label map rather than off the mask, because a slanted letter's box contains its
/// neighbour's ink — see [`Component::label`](crate::ccl::Component::label). Taking the box would
/// pull the next letter's foot into this letter's span and close the gap the shear had just opened.
///
/// `pivot` is subtracted from `y` before the shear so the result sits near the glyph rather than
/// hundreds of pixels away. It is a translation, so it cancels in [`UprightSpan::gap_to`] and
/// cannot change any answer; it exists so a span is readable beside the box it came from.
#[must_use]
pub fn upright_span(map: &LabelMap, glyph: &GroupedGlyph, shear: f64, pivot: u32) -> UprightSpan {
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for part in &glyph.parts {
        map.for_each(part.label, part.bounds, |x, y| {
            let (x, y) = (f64::from(x), f64::from(y));
            for (cx, cy) in [(x, y), (x + 1.0, y), (x, y + 1.0), (x + 1.0, y + 1.0)] {
                let sheared = cx - shear * (cy - f64::from(pivot));
                lo = lo.min(sheared);
                hi = hi.max(sheared);
            }
        });
    }
    if hi <= lo {
        return UprightSpan::UNKNOWN;
    }
    // Rounded outward, so a span never claims to be narrower than the ink it describes. A tenth of
    // a pixel either way is nothing beside a word gap; a span that ate a column would be the same
    // class of error this whole type exists to remove.
    UprightSpan::new(round_to_tenths(lo, f64::floor), round_to_tenths(hi, f64::ceil))
}

/// As [`upright_span`], measured separately in each of the line's four spacing bands.
///
/// #219. The span above is a box, and a box gap understates the space between two letters whenever
/// one of them is widened by ink at a height the other does not occupy -- by 29 points in front of
/// a `j`, whose descender hook reaches left below the baseline, and by 46 in front of a `T`, whose
/// crossbar reaches right at cap height. Measuring per band and taking the narrowest answer over
/// the bands both glyphs reach is what removes that.
///
/// `cap_top` and `baseline` are the line's own anchors, so the bands are a fraction of a *measured*
/// cap height. A shear of zero is the honest answer for a line that does not lean, and it is what
/// [`UprightSpan::of_box`] already does for the same case -- this is not a fabricated slant.
#[must_use]
pub fn upright_bands(
    map: &LabelMap,
    glyph: &GroupedGlyph,
    shear: f64,
    pivot: u32,
    cap_top: u32,
    baseline: u32,
) -> UprightBands {
    let mut lo = [f64::MAX; SPACING_BANDS];
    let mut hi = [f64::MIN; SPACING_BANDS];
    for part in &glyph.parts {
        map.for_each(part.label, part.bounds, |x, y| {
            // The band the pixel's own row falls in. Its corners are expanded below the way
            // `upright_span` expands them, and a corner that strays a fraction of a row into the
            // next band widens this one instead -- which errs towards a *narrower* gap, the same
            // direction the outward rounding errs in.
            let band = UprightBands::band_of(y, cap_top, baseline);
            let (x, y) = (f64::from(x), f64::from(y));
            for (cx, cy) in [(x, y), (x + 1.0, y), (x, y + 1.0), (x + 1.0, y + 1.0)] {
                let sheared = cx - shear * (cy - f64::from(pivot));
                lo[band] = lo[band].min(sheared);
                hi[band] = hi[band].max(sheared);
            }
        });
    }
    UprightBands::new(std::array::from_fn(|at| {
        if hi[at] <= lo[at] {
            UprightBands::EMPTY_BAND
        } else {
            (round_to_tenths(lo[at], f64::floor), round_to_tenths(hi[at], f64::ceil))
        }
    }))
}

/// A sheared coordinate in tenths of a pixel, rounded the given way.
#[allow(clippy::cast_possible_truncation)]
fn round_to_tenths(value: f64, how: fn(f64) -> f64) -> i32 {
    let tenths = how(value * f64::from(SPAN_TENTHS));
    tenths.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

/// Ink count and centroid of one component, in plane coordinates.
fn centroid(map: &LabelMap, part: &crate::ccl::Component) -> (f64, f64, f64) {
    let (mut count, mut sum_x, mut sum_y) = (0f64, 0f64, 0f64);
    map.for_each(part.label, part.bounds, |x, y| {
        count += 1.0;
        sum_x += f64::from(x);
        sum_y += f64::from(y);
    });
    if count == 0.0 {
        return (0.0, 0.0, 0.0);
    }
    (count, sum_x / count, sum_y / count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binarize::BinaryMask;
    use crate::ccl::{self, Component, ComponentFilter};
    use subtrackt_core::Rect;

    /// A canvas of `count` stems, each `width` wide and `height` tall, whose top sits `lean`
    /// columns to the right of its foot — an italic stem's geometry, and nothing else.
    fn stems(count: u32, at: u32, pitch: u32, width: u32, height: u32, lean: u32) -> BinaryMask {
        let mut mask = BinaryMask::blank(at + pitch * count + width + lean + 2, height);
        for index in 0..count {
            let base = at + index * pitch;
            for y in 0..height {
                let shift = lean * (height - 1 - y) / (height - 1).max(1);
                for x in 0..width {
                    mask.set(base + x + shift, y, true);
                }
            }
        }
        mask
    }

    /// Label a canvas and hand back one glyph per component, in reading order.
    fn labelled(mask: &BinaryMask) -> (LabelMap, Vec<GroupedGlyph>) {
        let (components, map) =
            ccl::label_with_map(mask, ComponentFilter::permissive()).expect("labels");
        let mut components = components;
        components.sort_by_key(|c| c.bounds.x);
        let glyphs = components
            .into_iter()
            .map(|part| GroupedGlyph { parts: vec![part], line: 0 })
            .collect();
        (map, glyphs)
    }

    fn refs(glyphs: &[GroupedGlyph]) -> Vec<&GroupedGlyph> {
        glyphs.iter().collect()
    }

    #[test]
    fn an_upright_line_reports_no_shear_at_all() {
        // Not "a small shear": nothing. A line that has not been shown to lean must produce the
        // same layout it produced before #121, and the only way to guarantee that is to give the
        // caller nothing to deskew with.
        let mask = stems(5, 1, 8, 3, 40, 0);
        let (map, glyphs) = labelled(&mask);
        assert_eq!(line_shear(&map, &refs(&glyphs)), None);
    }

    #[test]
    fn a_line_leaning_less_than_the_estimator_can_resolve_reports_unknown() {
        // One column of lean over forty rows is a shear of 0.025, inside the spread the estimator
        // shows on upright material. Reading it as slant would put a pixel of error on every gap
        // the line has, which is what it costs to be wrong here.
        let mask = stems(6, 1, 20, 3, 40, 1);
        let (map, glyphs) = labelled(&mask);
        assert_eq!(line_shear(&map, &refs(&glyphs)), None);
    }

    #[test]
    fn a_line_leaning_right_reports_a_negative_shear() {
        // The plane's y grows downward, so ink standing further right at the top has a negative
        // covariance cross term. Reversing this sign would deskew an italic into a worse italic,
        // and no gap figure downstream would say which had happened.
        let mask = stems(5, 1, 20, 3, 40, 8);
        let (map, glyphs) = labelled(&mask);
        let shear = line_shear(&map, &refs(&glyphs)).expect("enough ink and glyphs");
        assert!(shear < -0.1, "a right-leaning line reported {shear}");
    }

    #[test]
    fn the_shear_does_not_move_when_the_glyphs_are_laid_out_differently() {
        // Each glyph contributes its covariance about its own centroid, so where the letters sit
        // along the line cannot reach the answer. Pooling the raw pixels would fail this, and it is
        // the whole reason the estimator is written the way it is.
        let (tight, loose) = (stems(5, 1, 14, 3, 40, 8), stems(5, 1, 90, 3, 40, 8));
        let (map_a, a) = labelled(&tight);
        let (map_b, b) = labelled(&loose);
        let one = line_shear(&map_a, &refs(&a)).expect("enough ink and glyphs");
        let other = line_shear(&map_b, &refs(&b)).expect("enough ink and glyphs");
        assert!((one - other).abs() < 1e-9, "{one} against {other}");
    }

    #[test]
    fn a_line_with_too_few_glyphs_reports_unknown_rather_than_upright() {
        // The boundary `CLAUDE.md` requires. A line that could not be measured and a line measured
        // as upright are different facts, and only the second may produce a deskewed span.
        let mask = stems(u32::try_from(MIN_GLYPHS).unwrap() - 1, 1, 20, 3, 60, 8);
        let (map, glyphs) = labelled(&mask);
        assert_eq!(line_shear(&map, &refs(&glyphs)), None);
    }

    #[test]
    fn a_line_with_too_little_ink_reports_unknown_rather_than_upright() {
        let mask = stems(5, 1, 4, 1, 3, 0);
        let (map, glyphs) = labelled(&mask);
        assert_eq!(line_shear(&map, &refs(&glyphs)), None);
    }

    #[test]
    fn a_zero_shear_span_is_the_glyph_box() {
        // The property that makes this safe to apply to every line rather than only to leaning
        // ones: with no slant to take out, the span *is* the box the spacing rule already used, to
        // the tenth. Upright material therefore cannot move because the measurement changed, only
        // because of the shear the estimator reported — and those two are separable only if this
        // holds.
        let mask = stems(1, 3, 20, 4, 20, 0);
        let (map, glyphs) = labelled(&mask);
        assert_eq!(
            upright_span(&map, &glyphs[0], 0.0, 0),
            UprightSpan::of_box(glyphs[0].bounds())
        );
    }

    #[test]
    fn deskewing_recovers_a_gap_the_bounding_boxes_had_swallowed() {
        // The mechanism, in miniature: two leaning stems set far enough apart to be separate words.
        // Their boxes overlap, so the runtime's saturating subtraction reports zero, while the ink
        // is nowhere near touching.
        let mask = stems(2, 1, 11, 3, 40, 10);
        let (map, glyphs) = labelled(&mask);
        assert_eq!(glyphs.len(), 2, "the stems were meant to stay separate components");

        let boxed = i64::from(glyphs[1].bounds().x) - i64::from(glyphs[0].bounds().right());
        assert!(boxed <= 0, "the boxes were meant to overlap; they gapped by {boxed}");

        let shear = line_shear(&map, &refs(&glyphs)).unwrap_or(-0.25);
        let gap = upright_span(&map, &glyphs[0], shear, 0)
            .gap_to(upright_span(&map, &glyphs[1], shear, 0))
            .expect("both measured");
        assert!(gap > 0, "the deskewed gap was {gap} tenths");
    }

    #[test]
    fn a_span_reads_the_component_and_not_everything_inside_its_box() {
        // The reason the label map is threaded through at all. The two stems above overlap in
        // columns, so the first one's box contains part of the second one's foot — and a span taken
        // off the mask would stretch to cover it and close the gap the shear had just opened.
        let mask = stems(2, 1, 11, 3, 40, 10);
        let (map, glyphs) = labelled(&mask);
        let shear = -0.25;
        let honest = upright_span(&map, &glyphs[0], shear, 0);

        let both = GroupedGlyph {
            parts: glyphs.iter().flat_map(|g| g.parts.clone()).collect(),
            line: 0,
        };
        let contaminated = upright_span(&map, &both, shear, 0);
        assert!(
            contaminated.right > honest.right,
            "the two-component span should reach further right: {contaminated:?} {honest:?}"
        );
    }

    #[test]
    fn a_span_is_unknown_when_the_glyph_has_no_ink() {
        let mask = BinaryMask::blank(8, 8);
        let (_, map) = ccl::label_with_map(&mask, ComponentFilter::permissive()).expect("labels");
        let glyph = GroupedGlyph {
            parts: vec![Component {
                bounds: Rect::new(0, 0, 8, 8),
                pixels: 0,
                label: ccl::NO_LABEL,
            }],
            line: 0,
        };
        assert_eq!(upright_span(&map, &glyph, -0.2, 0), UprightSpan::UNKNOWN);
    }

    #[test]
    fn the_pivot_shifts_a_span_without_changing_a_gap() {
        // A pivot is a translation. It exists so a span reads near the box it came from, and it may
        // not reach any answer — the gap is the only thing anything asks for.
        let mask = stems(2, 1, 20, 3, 40, 8);
        let (map, glyphs) = labelled(&mask);
        let gap_at = |pivot| {
            upright_span(&map, &glyphs[0], -0.2, pivot)
                .gap_to(upright_span(&map, &glyphs[1], -0.2, pivot))
                .expect("both measured")
        };
        assert_eq!(gap_at(0), gap_at(37));
    }
}
