//! Cutting a component that turned out to be two characters.
//!
//! [#106](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/106). Connected-component
//! labelling is 8-connected, and [`crate::ccl`] pins characters that touch at a corner fusing into
//! one component as **accepted behaviour** — an `r`'s arm reaches over the letter after it, and
//! nothing in the tree has ever separated them again.
//!
//! `docs/error-census.md` measured what that costs on a real Blu-ray: of 105 components the matcher
//! could not read, six were full stops and roughly ninety-four were fusions — `rt`, `ry`, `tw`,
//! `yw`, `wf` — each costing **two** characters, a placeholder where the pair belonged and the
//! second letter missing entirely. **193 of 687 remaining errors, 28% of them.**
//!
//! ## Why this is allowed to be loose
//!
//! Every rule here is a *proposal* that the caller checks. This module finds candidate cuts; it
//! never decides that a cut was right. [`crate::matcher`] does that, and the acceptance rule is the
//! whole safety argument:
//!
//! - it runs **only** on components the matcher already returned `unmatched` for, so nothing it
//!   does can turn a match into a wrong answer;
//! - a cut is accepted **only** if *every* part matches within the ceiling.
//!
//! The failure mode is therefore bounded to unmatched → matched, which is the direction the
//! accuracy gate measures anyway. That is what lets the trigger below be as permissive as it is.
//!
//! ## Why the trigger is not a width test
//!
//! #97 proposed cutting "a component whose width against its line's cap height exceeds any single
//! reference character's". A fused `rt` at 1080p is **31x42 — narrower than it is tall** — because
//! `r` and `t` are both narrow, and `docs/error-census.md` records that the disc's fusions run from
//! 73% to 200% of their line's cap height. No width threshold separates them from single
//! characters, so none is used.

use subtrackt_core::Rect;

use crate::binarize::BinaryMask;

/// How a component is offered up for cutting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitRules {
    /// Narrowest part a cut may leave, as a percentage of the component's width.
    ///
    /// Stops a cut slicing a sliver off an edge, which would produce a part that matches something
    /// narrow by accident. A fusion of two characters divides somewhere near the middle; anything
    /// that leaves one side under this is not the cut being looked for.
    pub min_part_percent: u32,
    /// How light a column must be to be a candidate cut, as a percentage of the component's
    /// *median* ink column.
    ///
    /// The median rather than the mean, because a fused pair has a long tail of heavy columns
    /// through the two stems and a mean would be dragged up by them. Characters that touch at a
    /// corner leave a narrow bridge — a column with some ink but far less than a stroke — so this
    /// is well above zero on purpose.
    pub max_bridge_percent: u32,
    /// How many cuts may be made in one component, so a three-character fusion is reachable.
    ///
    /// `docs/error-census.md` found one on the disc: `ryw` in `everywhere`, 84x44. Two is enough
    /// for everything measured and bounds the work at a handful of scans per unread glyph.
    pub max_cuts: usize,
}

impl Default for SplitRules {
    fn default() -> Self {
        Self { min_part_percent: 20, max_bridge_percent: 40, max_cuts: 2 }
    }
}

/// Columns inside `bounds` worth trying as a cut, lightest first.
///
/// Returned in order of how little ink they carry, because the lightest bridge is the likeliest
/// join and the caller stops at the first cut that produces parts which all match. Coordinates are
/// absolute, in the same space as `bounds`.
#[must_use]
pub fn cut_columns(mask: &BinaryMask, bounds: Rect, rules: SplitRules) -> Vec<u32> {
    if bounds.width < 4 {
        return Vec::new();
    }
    let profile: Vec<u32> = (bounds.x..bounds.right())
        .map(|x| {
            (bounds.y..bounds.bottom())
                .filter(|&y| mask.get(x, y))
                .count()
                .try_into()
                .unwrap_or(u32::MAX)
        })
        .collect();

    let mut sorted = profile.clone();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    let ceiling = median * rules.max_bridge_percent / 100;

    // At least two pixels, whatever the percentage works out to. A one-pixel part is not a
    // character under any rendering, and on a narrow box the percentage alone would allow one.
    let margin = (bounds.width * rules.min_part_percent / 100).max(2) as usize;
    if profile.len() <= margin * 2 {
        return Vec::new();
    }

    let mut candidates: Vec<(u32, u32)> = Vec::new();
    for index in margin..profile.len() - margin {
        let ink = profile[index];
        if ink > ceiling {
            continue;
        }
        // A local minimum, so a wide light region contributes its middle rather than every column
        // in it. Ties lean left, which is arbitrary and only decides which of two identical columns
        // is tried first.
        if ink > profile[index - 1] || ink > profile[index + 1] {
            continue;
        }
        candidates.push((ink, bounds.x + u32::try_from(index).unwrap_or(0)));
    }
    candidates.sort_unstable();
    candidates.into_iter().map(|(_, x)| x).collect()
}

/// The two boxes a cut at `column` produces, each cropped back to its own ink.
///
/// Cropping matters: the left part of a fused `rt` ends where the `r` ends, not where the cut fell,
/// and a box carrying a column of background would letterbox differently from the same character
/// segmented on its own. Returns `None` if either side has no ink at all.
#[must_use]
pub fn parts_at(mask: &BinaryMask, bounds: Rect, column: u32) -> Option<(Rect, Rect)> {
    let left = Rect::new(bounds.x, bounds.y, column.saturating_sub(bounds.x), bounds.height);
    let right = Rect::new(column, bounds.y, bounds.right().saturating_sub(column), bounds.height);
    Some((ink_bounds(mask, left)?, ink_bounds(mask, right)?))
}

/// The bounding box of the foreground inside `area`, or `None` if there is none.
fn ink_bounds(mask: &BinaryMask, area: Rect) -> Option<Rect> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (u32::MAX, u32::MAX, 0u32, 0u32);
    let mut any = false;
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if mask.get(x, y) {
                any = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    any.then(|| Rect::new(min_x, min_y, max_x - min_x + 1, max_y - min_y + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(rows: &[&str]) -> BinaryMask {
        let height = u32::try_from(rows.len()).unwrap();
        let width = u32::try_from(rows[0].len()).unwrap();
        let bits = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c == '#'))
            .collect();
        BinaryMask::from_bits(width, height, bits).unwrap()
    }

    /// Two blocks joined by a single-pixel bridge: the shape a corner touch makes.
    ///
    /// Deliberately with **no empty column** anywhere. That is what a fusion looks like — if there
    /// were a gap the components would never have been labelled as one — and a test mask with a
    /// wide gap would be testing something the pass never sees.
    fn fused() -> BinaryMask {
        mask(&[
            "####.####",
            "####.####",
            "#########",
            "####.####",
            "####.####",
        ])
    }

    #[test]
    fn the_bridge_between_two_characters_is_the_first_cut_offered() {
        // The property the whole pass rests on: the join is where the ink is thinnest, so trying
        // columns in order of ink means the first cut tried is the likeliest one.
        let m = fused();
        let columns = cut_columns(&m, Rect::new(0, 0, 9, 5), SplitRules::default());
        assert_eq!(columns, vec![4], "the bridge and nothing else");
    }

    #[test]
    fn a_solid_block_offers_no_cut_at_all() {
        // The control. A single character has no light column, so nothing is proposed and the
        // caller falls straight back to reporting it unread.
        let m = mask(&["#####", "#####", "#####", "#####"]);
        assert!(cut_columns(&m, Rect::new(0, 0, 5, 4), SplitRules::default()).is_empty());
    }

    #[test]
    fn a_cut_may_not_slice_a_sliver_off_the_edge() {
        // A light column one pixel in from the edge is not the join between two characters; it is
        // the inside of the first one. Accepting it would produce a part that matches something
        // narrow by accident, which is the one way this pass could invent an answer. The mask
        // carries a real bridge too, so the test cannot pass by finding nothing.
        let m = mask(&["#.##.####", "#.##.####", "#####.###"]);
        let columns = cut_columns(&m, Rect::new(0, 0, 9, 3), SplitRules::default());
        assert!(!columns.is_empty(), "the middle bridge is still a candidate");
        assert!(columns.iter().all(|x| *x > 1), "cut at the edge: {columns:?}");
    }

    #[test]
    fn each_part_is_cropped_to_its_own_ink_rather_than_to_the_cut() {
        // The left part of a fused `rt` ends where the `r` ends, not where the cut fell. A box
        // carrying background would letterbox differently from the same character segmented alone,
        // and the whole point is to hand the matcher what it would have seen without the fusion.
        let m = fused();
        let (left, right) = parts_at(&m, Rect::new(0, 0, 9, 5), 4).unwrap();
        assert_eq!(left, Rect::new(0, 0, 4, 5), "the left block, without the bridge column");
        assert_eq!(
            right,
            Rect::new(4, 0, 5, 5),
            "the right block, carrying the bridge pixel"
        );
    }

    #[test]
    fn a_cut_that_leaves_one_side_empty_produces_no_parts() {
        let m = mask(&["###..", "###..", "###.."]);
        assert_eq!(parts_at(&m, Rect::new(0, 0, 5, 3), 4), None);
    }

    #[test]
    fn a_component_too_narrow_to_hold_two_characters_is_left_alone() {
        let m = mask(&["#.#", "#.#"]);
        assert!(cut_columns(&m, Rect::new(0, 0, 3, 2), SplitRules::default()).is_empty());
    }
}
