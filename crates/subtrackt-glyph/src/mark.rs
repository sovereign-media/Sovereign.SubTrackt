//! Which way a glyph's diacritic leans.
//!
//! [`group`](crate::group) has already decided which components are marks and which is the body;
//! this reads a direction off the mark before [`feature`](crate::feature) merges the boxes and
//! normalises that direction away. The measurement behind it is #48, written up in
//! `docs/glyph-stability.md`: of the 21 pairs the shipped matcher calls ambiguous, sixteen are one
//! base letter differing only in which way its accent leans, and accented lowercase is ordinary
//! Spanish, French and Italian subtitle text.
//!
//! Three candidates were measured before any of this was built. The mark's box placement separates
//! 11 of the 16. The mark's own feature vector separates all 16, but clears its own rendering noise
//! by a factor of 1.6 — and #14 is the record of what a ratio that thin buys. What ships is the
//! third: one signed number, separating all 16 at ten to twenty times its noise, holding its sign
//! in every rendering across six sizes and three ink thresholds, and not reversing between Arial,
//! Verdana and Tahoma.

use subtrackt_core::{MarkSlope, Rect};

use crate::binarize::BinaryMask;
use crate::group::GroupedGlyph;

/// Fewest ink pixels a mark needs before its direction means anything.
///
/// Two pixels lie on a line by construction, so a slope read off them reports the pixel count and
/// not the letterform. Reporting [`MarkSlope::NONE`] is the honest answer; inventing a direction is
/// the thing this project exists not to do.
const MIN_INK: u32 = 3;

/// Smallest spread of ink, in pixels squared, that a direction can be read from.
///
/// A macron rasterised to a single row has zero vertical variance, so the normalising divisor
/// vanishes. That is undefined rather than vertical, and dividing by it would report a direction
/// with no basis — as well as producing a non-finite number.
const MIN_SPREAD: f64 = 1.0;

/// The box enclosing every part of a grouped glyph except its body.
///
/// The body is the tallest part, held as an index rather than compared by value: a diaeresis
/// arrives as two parts of identical bounds, and comparing boxes would drop the wrong one.
fn mark_box(glyph: &GroupedGlyph) -> Option<Rect> {
    if glyph.parts.len() < 2 {
        return None;
    }
    let body_at = glyph
        .parts
        .iter()
        .enumerate()
        .max_by_key(|(_, part)| part.bounds.height)
        .map(|(index, _)| index)?;
    glyph
        .parts
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != body_at)
        .map(|(_, part)| part.bounds)
        .reduce(Rect::union)
}

/// The normalised second moment cross term of the ink inside `area`, as a percentage.
///
/// `Cxy / sqrt(Cxx · Cyy)`. Normalised by the two variances so it reports the *direction* of the
/// ink and not its extent: a long accent and a short one that lean the same way have to agree, or
/// the feature is reporting rendering size again — the axis `docs/glyph-stability.md` measured as
/// costing 11 cells on its own.
///
/// Computed in `f64` over a box a few dozen pixels on a side, so the sums stay far inside what the
/// type represents exactly.
#[allow(clippy::cast_possible_truncation)]
fn slope_of(mask: &BinaryMask, area: Rect) -> Option<i32> {
    let mut count = 0f64;
    let (mut sum_x, mut sum_y) = (0f64, 0f64);
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if mask.get(x, y) {
                count += 1.0;
                sum_x += f64::from(x);
                sum_y += f64::from(y);
            }
        }
    }
    if count < f64::from(MIN_INK) {
        return None;
    }

    let (mean_x, mean_y) = (sum_x / count, sum_y / count);
    let (mut cxx, mut cyy, mut cxy) = (0f64, 0f64, 0f64);
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if mask.get(x, y) {
                let (dx, dy) = (f64::from(x) - mean_x, f64::from(y) - mean_y);
                cxx += dx * dx;
                cyy += dy * dy;
                cxy += dx * dy;
            }
        }
    }

    let spread = (cxx * cyy).sqrt();
    if spread < MIN_SPREAD {
        return None;
    }
    Some((cxy / spread * 100.0).round() as i32)
}

/// Read the direction of a glyph's mark, if it has one.
///
/// Reads the binary mask rather than the coverage plane even when the feature vector is built from
/// coverage. What the vector gains from the ramp is a smoother per-cell decision; what a moment
/// over three pixels would gain is unmeasured, and #48's stability figures were taken on the binary
/// mask. Weighting by coverage is a change that should be measured before it is made.
#[must_use]
pub fn slope(mask: &BinaryMask, glyph: &GroupedGlyph) -> MarkSlope {
    mark_box(glyph)
        .and_then(|mark| slope_of(mask, mark))
        .map_or(MarkSlope::NONE, MarkSlope::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ccl::Component;

    /// Build a mask from an ASCII picture, `#` for ink.
    fn mask_of(rows: &[&str]) -> BinaryMask {
        let height = u32::try_from(rows.len()).unwrap();
        let width = u32::try_from(rows[0].len()).unwrap();
        let bits: Vec<bool> = rows
            .iter()
            .flat_map(|row| row.chars().map(|c| c == '#'))
            .collect();
        BinaryMask::from_bits(width, height, &bits).unwrap()
    }

    fn part(x: u32, y: u32, width: u32, height: u32) -> Component {
        Component {
            bounds: Rect::new(x, y, width, height),
            pixels: u64::from(width) * u64::from(height),
            label: 0,
        }
    }

    fn glyph(parts: Vec<Component>) -> GroupedGlyph {
        GroupedGlyph { parts, line: 0 }
    }

    #[test]
    fn a_grave_leans_the_opposite_way_to_an_acute() {
        // In image coordinates y grows downwards, so a grave — drawn from upper left to lower
        // right — has ink whose x and y rise together. Getting this backwards breaks nothing that
        // any other test can see; it silently swaps which letter is which.
        let grave = mask_of(&["##..", ".##.", "..##"]);
        let acute = mask_of(&["..##", ".##.", "##.."]);
        assert!(slope_of(&grave, Rect::new(0, 0, 4, 3)).unwrap() > 0);
        assert!(slope_of(&acute, Rect::new(0, 0, 4, 3)).unwrap() < 0);
    }

    #[test]
    fn a_symmetric_mark_lands_between_the_two_it_has_to_separate() {
        // Why one number separates three marks: a circumflex is symmetric about its vertical axis,
        // so its cross term cancels and it sits between an acute and a grave rather than beside
        // either.
        let circumflex = mask_of(&["..##..", ".####.", "##..##"]);
        let grave = mask_of(&["##..", ".##.", "..##"]);
        let symmetric = slope_of(&circumflex, Rect::new(0, 0, 6, 3)).unwrap();
        let leaning = slope_of(&grave, Rect::new(0, 0, 4, 3)).unwrap();
        assert!(symmetric.unsigned_abs() * 2 < leaning.unsigned_abs());
    }

    #[test]
    fn the_same_stroke_at_two_sizes_reports_the_same_direction() {
        // The feature has to report direction and not size, or it reports rendering resolution
        // again — which is what the normalisation exists to absorb.
        let small = mask_of(&["##..", ".##.", "..##"]);
        let large = mask_of(&[
            "####....", "####....", "..####..", "..####..", "....####", "....####",
        ]);
        let a = slope_of(&small, Rect::new(0, 0, 4, 3)).unwrap();
        let b = slope_of(&large, Rect::new(0, 0, 8, 6)).unwrap();
        assert!(a.abs_diff(b) < 20, "measured {a} and {b}");
    }

    #[test]
    fn too_little_ink_reports_no_direction_rather_than_a_guess() {
        let two = mask_of(&["#.", ".#"]);
        assert_eq!(slope_of(&two, Rect::new(0, 0, 2, 2)), None);
        let empty = mask_of(&["..", ".."]);
        assert_eq!(slope_of(&empty, Rect::new(0, 0, 2, 2)), None);
    }

    #[test]
    fn a_mark_one_row_tall_has_no_direction_to_report() {
        // A macron. Zero vertical variance means the normalising divisor vanishes, and the answer
        // is undefined rather than horizontal.
        let flat = mask_of(&["####"]);
        assert_eq!(slope_of(&flat, Rect::new(0, 0, 4, 1)), None);
    }

    #[test]
    fn a_glyph_with_one_part_has_no_mark() {
        let mask = mask_of(&["####", "####"]);
        assert_eq!(slope(&mask, &glyph(vec![part(0, 0, 4, 2)])), MarkSlope::NONE);
    }

    #[test]
    fn the_tallest_part_is_the_body_and_everything_else_is_the_mark() {
        // An accented lowercase letter: a two-row mark above a four-row body. The slope must come
        // from the mark alone — including the body would average the direction away, which is
        // exactly the failure this feature exists to avoid.
        let mask = mask_of(&["##..", ".##.", "####", "####", "####", "####"]);
        let parts = vec![part(0, 0, 4, 2), part(0, 2, 4, 4)];
        let measured = slope(&mask, &glyph(parts));
        assert!(measured.known);
        assert!(measured.percent > 0, "a grave over a body measured {measured:?}");
    }

    #[test]
    fn a_diaeresis_is_two_parts_of_the_same_size_and_still_finds_its_body() {
        // Both dots have identical bounds, so picking the body by comparing boxes rather than by
        // index would drop one of them into the body slot.
        let mask = mask_of(&["#..#", "....", "####", "####", "####"]);
        let parts = vec![part(0, 0, 1, 1), part(3, 0, 1, 1), part(0, 2, 4, 3)];
        let measured = slope(&mask, &glyph(parts));
        // Two dots side by side have no vertical spread, so there is no direction to read. What
        // matters is that the body was not mistaken for one of them.
        assert_eq!(measured, MarkSlope::NONE);
    }
}
