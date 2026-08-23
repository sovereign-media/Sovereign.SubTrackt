//! Line assignment and diacritic grouping.
//!
//! Connected components are not characters. The dot of an `i`, the accent of an `é` and the two
//! dots of a diaeresis each arrive as their own component and belong to the glyph they sit above;
//! a cedilla belongs to the one above it.
//!
//! The catch that shapes the whole design is punctuation. A colon is two small components stacked
//! vertically — geometrically identical to a diaeresis — and it must survive as one character
//! rather than being welded onto the letter beside it. What separates them is not the marks but
//! what sits under them: a diaeresis has a full-height letter body below it, and a colon has
//! nothing but another dot. So components are classified by height relative to the measured line,
//! and a mark only attaches to a *body*.
//!
//! Known limitation: a double quote is two marks side by side rather than stacked, so it stays two
//! glyphs and reads as two single quotes. That degrades to something close rather than to garbage,
//! and fixing it needs the horizontal-neighbour case that #11 is better placed to handle.

use subtrackt_core::{Error, Rect, Result};

use crate::binarize::BinaryMask;
use crate::ccl::Component;

/// Thresholds for merging a component into its neighbour.
///
/// Everything here is a fraction of the measured line height rather than a pixel count: the same
/// title ships at several resolutions, and an absolute threshold that works at 1080p will merge
/// half a line at 480p.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupingRules {
    /// A component shorter than this fraction of line height, in percent, may be a diacritic.
    pub diacritic_max_height_percent: u32,
    /// Maximum vertical gap between a diacritic and its base, in percent of line height.
    pub max_gap_percent: u32,
    /// Minimum horizontal overlap between a diacritic and its base, in percent of **the narrower
    /// of the two**.
    ///
    /// The narrower rather than the diacritic, and #58 is what the difference cost. Measured against
    /// the diacritic alone, a mark wider than the letter under it can never reach any threshold
    /// however well centred: the circumflex of an `î` is 28px over a 9px stem, which is 32% against
    /// a floor of 50, so `Î Ï î ï` never grouped at all. `ì` passed at 53%, which is a coincidence
    /// rather than a margin.
    ///
    /// The question the rule exists to ask is whether a mark sits over *this* letter rather than a
    /// different one, and for a body narrower than the mark that cannot be expressed as a fraction
    /// of the mark. A mark genuinely straddling two letters still fails, because the overlap with
    /// either one is then a small part of that letter too.
    pub min_overlap_percent: u32,
    /// Maximum horizontal gap between two side-by-side marks that may be one mark, in percent of
    /// the wider one's width.
    ///
    /// A diaeresis is two dots straddling the stem they belong to — at x84 and x102 over a stem at
    /// x93 in Arial at 96px — so **neither dot overlaps the stem and neither centre falls on it**.
    /// Only their union does, and marks are matched to bodies one at a time. That is why `Ï` and
    /// `ï` never grouped, which is the second half of #58.
    ///
    /// This is a guard rather than the discriminator. What separates a diaeresis from the two dots
    /// of `ii` — also two marks at the same height — is not distance but that each `i` dot finds
    /// its own stem on its own and never becomes an orphan. Only orphans are merged, so the `ii`
    /// case cannot arise however the threshold is set.
    pub mark_cluster_gap_percent: u32,
    /// Maximum vertical gap between two stacked punctuation marks, in percent of the taller
    /// mark's height.
    ///
    /// Measured against the marks rather than the line for two reasons. A colon sits far further
    /// apart than a diacritic does from its base, so it needs its own threshold; and a line
    /// holding nothing but punctuation has no meaningful line height to scale against.
    pub punctuation_gap_percent: u32,
}

impl Default for GroupingRules {
    fn default() -> Self {
        Self {
            diacritic_max_height_percent: 40,
            max_gap_percent: 25,
            min_overlap_percent: 50,
            mark_cluster_gap_percent: 150,
            punctuation_gap_percent: 200,
        }
    }
}

/// Components merged into one character, in reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupedGlyph {
    /// The components that make up this character.
    pub parts: Vec<Component>,
    /// Which text line it belongs to, counting from the top.
    pub line: usize,
}

impl GroupedGlyph {
    /// The box enclosing every part.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        self.parts
            .iter()
            .map(|p| p.bounds)
            .reduce(Rect::union)
            .unwrap_or_default()
    }
}

/// A vertical band of the image containing one line of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineBand {
    /// First row of the band.
    pub top: u32,
    /// One past the last row of the band.
    pub bottom: u32,
}

impl LineBand {
    /// Height of the band, which is the measured line height every threshold scales against.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.bottom - self.top
    }

    /// Whether a row falls inside the band.
    #[must_use]
    pub const fn contains(self, y: u32) -> bool {
        y >= self.top && y < self.bottom
    }
}

/// Find the bands of rows that hold text, from the mask's row projection.
///
/// A band is a maximal run of rows with at least one foreground pixel. Taking this from the image
/// rather than from component geometry matters: a line break is a property of the pixels, and
/// clustering component boxes instead would invent one wherever a line happened to be sparse.
#[must_use]
pub fn line_bands(mask: &BinaryMask) -> Vec<LineBand> {
    let mut bands = Vec::new();
    let mut start: Option<u32> = None;

    for (row, count) in mask.row_projection().iter().enumerate() {
        let row = u32::try_from(row).unwrap_or(u32::MAX);
        match (start, *count > 0) {
            (None, true) => start = Some(row),
            (Some(top), false) => {
                bands.push(LineBand { top, bottom: row });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(top) = start {
        bands.push(LineBand { top, bottom: mask.height() });
    }
    bands
}

/// The bands that are actually text lines.
///
/// [`line_bands`] cuts at any row carrying no ink across the whole width, which is the right rule
/// for a line break and the wrong one for an accent. The accent on a *capital* sits above every
/// letterform this charset can spell, so in a line of nothing but letters the row beneath it is
/// blank and it bands on its own — `À` then segments as a bare `A` and a floating grave, which is
/// #57.
///
/// **A band holding nothing but mark-height components is not a line.** That is the test, and it is
/// about what is *in* the band rather than how far it sits from its neighbour — which is why it
/// cannot merge two lines of text however tightly they are set. A text line always carries at least
/// one full-height letterform.
///
/// The gap is a second condition rather than the first. A cue whose opening line is genuinely all
/// marks — an ellipsis, a row of dashes — is mark-only too, and only the distance keeps it from
/// being swallowed by the paragraph under it. Line leading is wider than the space above an accent,
/// so [`GroupingRules::max_gap_percent`] separates them.
///
/// Both thresholds are the ones grouping already uses, applied at band scale rather than at
/// component scale.
#[must_use]
pub fn text_lines(
    mask: &BinaryMask,
    components: &[Component],
    rules: GroupingRules,
) -> Vec<LineBand> {
    let centre = |c: &Component| c.bounds.y + c.bounds.height / 2;
    let mut merged: Vec<LineBand> = Vec::new();

    for band in line_bands(mask) {
        let Some(previous) = merged.last().copied() else {
            merged.push(band);
            continue;
        };

        // A mark is short relative to the line it belongs to, so the *lower* band is what its
        // height is a fraction of.
        let tall = band.height().max(1);
        let mut above = components
            .iter()
            .filter(|c| previous.contains(centre(c)))
            .peekable();
        let holds_anything = above.peek().is_some();
        let mark_only =
            above.all(|c| c.bounds.height * 100 <= tall * rules.diacritic_max_height_percent);
        let gap = band.top.saturating_sub(previous.bottom);

        if holds_anything && mark_only && gap * 100 <= tall * rules.max_gap_percent {
            merged.pop();
            merged.push(LineBand { top: previous.top, bottom: band.bottom });
        } else {
            merged.push(band);
        }
    }
    merged
}

/// Assign each component to a text line, counting from the top.
///
/// The returned vector is index-aligned with `components`.
///
/// # Errors
/// Returns [`Error::Config`] if the mask holds no text at all while components were supplied,
/// which means the two arguments came from different images.
pub fn assign_lines(mask: &BinaryMask, components: &[Component]) -> Result<Vec<usize>> {
    assign_to(&text_lines(mask, components, GroupingRules::default()), components)
}

/// Assign components to bands that have already been worked out.
///
/// Split out so a caller measuring line metrics can use the *same* bands the assignment used. Two
/// calls to [`text_lines`] would agree today, but a caller that banded separately and then indexed
/// glyphs by line would be one refactor away from indexing into a different set.
///
/// # Errors
/// Returns [`Error::Config`] if there are no bands while components were supplied, which means the
/// two arguments came from different images.
pub fn assign_to(bands: &[LineBand], components: &[Component]) -> Result<Vec<usize>> {
    if components.is_empty() {
        return Ok(Vec::new());
    }

    if bands.is_empty() {
        return Err(Error::Config(
            "components were supplied but the mask has no foreground rows; they cannot be from \
             the same image"
                .into(),
        ));
    }

    Ok(components
        .iter()
        .map(|component| {
            // The vertical centre decides the line. A tall component spanning two bands is rare
            // and belongs to whichever it sits mostly in.
            let centre = component.bounds.y + component.bounds.height / 2;
            bands
                .iter()
                .position(|b| b.contains(centre))
                .unwrap_or_else(|| nearest_band(bands, centre))
        })
        .collect())
}

/// Index of the band whose edge is closest to `y`, for a component the projection missed.
fn nearest_band(bands: &[LineBand], y: u32) -> usize {
    bands
        .iter()
        .enumerate()
        .min_by_key(|(_, band)| {
            let above = band.top.saturating_sub(y);
            let below = y.saturating_sub(band.bottom);
            above.max(below)
        })
        .map_or(0, |(index, _)| index)
}

/// Merge diacritics onto their base glyphs.
///
/// `components` and `lines` are index-aligned, as produced by [`assign_lines`]. Glyphs come back
/// in reading order: line by line, left to right within a line.
///
/// # Errors
/// Returns [`Error::Config`] if the two slices are different lengths.
pub fn group(
    components: &[Component],
    lines: &[usize],
    rules: GroupingRules,
) -> Result<Vec<GroupedGlyph>> {
    if components.len() != lines.len() {
        return Err(Error::Config(format!(
            "group got {} components and {} line assignments; they must be index-aligned",
            components.len(),
            lines.len()
        )));
    }

    let line_count = lines.iter().copied().max().map_or(0, |m| m + 1);
    let mut glyphs = Vec::new();

    for line in 0..line_count {
        let members: Vec<Component> = components
            .iter()
            .zip(lines)
            .filter(|(_, l)| **l == line)
            .map(|(c, _)| *c)
            .collect();

        if !members.is_empty() {
            glyphs.extend(group_one_line(&members, line, rules));
        }
    }

    Ok(glyphs)
}

/// Group the components of a single text line.
fn group_one_line(members: &[Component], line: usize, rules: GroupingRules) -> Vec<GroupedGlyph> {
    let top = members.iter().map(|c| c.bounds.y).min().unwrap_or(0);
    let bottom = members.iter().map(|c| c.bounds.bottom()).max().unwrap_or(0);
    let line_height = bottom.saturating_sub(top).max(1);

    let mark_max_height = line_height * rules.diacritic_max_height_percent / 100;
    let max_gap = line_height * rules.max_gap_percent / 100;

    // A body is a full-height letter form; a mark is small enough to be a diacritic or a piece of
    // punctuation. This split is what keeps a colon away from the letter beside it.
    let (marks, bodies): (Vec<usize>, Vec<usize>) =
        (0..members.len()).partition(|i| members[*i].bounds.height <= mark_max_height);

    let mut parts: Vec<Vec<Component>> = bodies.iter().map(|i| vec![members[*i]]).collect();
    let mut orphans = Vec::new();

    for mark in marks {
        match best_body(members, &bodies, mark, max_gap, rules) {
            Some(slot) => parts[slot].push(members[mark]),
            None => orphans.push(mark),
        }
    }

    // A mark that found no body may be half of one. A diaeresis straddles its stem, so neither dot
    // overlaps it and each fails alone where their union would succeed — #58. Retrying the merged
    // pair is safe precisely because it only ever sees marks that already failed: the dots of `ii`
    // each match their own stem individually and never reach here.
    let (rejoined, orphans) = rejoin_split_marks(members, &orphans, &bodies, max_gap, rules);
    for (slot, pair) in rejoined {
        parts[slot].extend(pair);
    }

    // Marks with no body under them are punctuation in their own right. Stacked ones are a single
    // character — a colon, a semicolon — so they cluster together rather than becoming two glyphs.
    for cluster in cluster_orphans(members, &orphans, rules) {
        parts.push(cluster);
    }

    let mut glyphs: Vec<GroupedGlyph> = parts
        .into_iter()
        .map(|parts| GroupedGlyph { parts, line })
        .collect();
    glyphs.sort_by_key(|g| g.bounds().x);
    glyphs
}

/// Index into `bodies` of the closest body a mark belongs to, if any.
fn best_body(
    members: &[Component],
    bodies: &[usize],
    mark: usize,
    max_gap: u32,
    rules: GroupingRules,
) -> Option<usize> {
    bodies
        .iter()
        .enumerate()
        .filter(|(_, body)| {
            overlaps_enough(members[mark].bounds, members[**body].bounds, rules)
                && vertical_gap(members[mark].bounds, members[**body].bounds) <= max_gap
        })
        .min_by_key(|(_, body)| vertical_gap(members[mark].bounds, members[**body].bounds))
        .map(|(slot, _)| slot)
}

/// Cluster leftover marks that stack vertically, so `:` and `;` stay one character.
/// Retry orphaned marks in horizontally adjacent pairs.
///
/// The second half of #58. A mark is matched to a body on its own, which is right for every mark
/// that sits over the letter it belongs to and wrong for the one that straddles it: a diaeresis is
/// two dots either side of a narrow stem, so neither overlaps it and only their union does.
///
/// Only orphans are considered, and that is the whole safety argument. The dots of `ii` are two
/// marks at the same height too, and merging *those* would attach one mark to one stem — but each
/// of them finds its own stem individually and never becomes an orphan, so the pair never reaches
/// this pass however the gap threshold is set.
///
/// Returns the pairs that found a body, keyed by the body's slot, and the orphans that are still
/// orphans.
fn rejoin_split_marks(
    members: &[Component],
    orphans: &[usize],
    bodies: &[usize],
    max_gap: u32,
    rules: GroupingRules,
) -> (Vec<(usize, Vec<Component>)>, Vec<usize>) {
    let mut rejoined = Vec::new();
    let mut used = vec![false; orphans.len()];

    for (first, left) in orphans.iter().enumerate() {
        if used[first] {
            continue;
        }
        for (second, right) in orphans.iter().enumerate().skip(first + 1) {
            if used[second] || !side_by_side(members[*left].bounds, members[*right].bounds, rules) {
                continue;
            }

            // The pair as one mark, appended so the body indices still address the same
            // components. If it now finds a body, the two were one character all along.
            let mut extended = members.to_vec();
            extended.push(Component {
                bounds: members[*left].bounds.union(members[*right].bounds),
                pixels: members[*left].pixels + members[*right].pixels,
                // Nothing labelled this: it is a box drawn round two components to ask one
                // question. It never reaches a `GroupedGlyph` — the pair is pushed below as its
                // two original members, which carry their own labels and therefore their own ink.
                label: crate::ccl::NO_LABEL,
            });
            let merged = extended.len() - 1;
            let Some(slot) = best_body(&extended, bodies, merged, max_gap, rules) else {
                continue;
            };

            rejoined.push((slot, vec![members[*left], members[*right]]));
            used[first] = true;
            used[second] = true;
            break;
        }
    }

    let still_orphaned = orphans
        .iter()
        .enumerate()
        .filter(|(index, _)| !used[*index])
        .map(|(_, orphan)| *orphan)
        .collect();
    (rejoined, still_orphaned)
}

/// Are two marks side by side at the same height, close enough to be one mark?
///
/// Vertical agreement first: two dots of a diaeresis share a top, and a mark beside an unrelated
/// one — an apostrophe next to a hyphen — does not. Then the horizontal gap, as a fraction of the
/// wider mark, so the same pair merges at every resolution.
fn side_by_side(left: Rect, right: Rect, rules: GroupingRules) -> bool {
    let (left, right) = if left.x <= right.x {
        (left, right)
    } else {
        (right, left)
    };
    let widest = left.width.max(right.width).max(1);
    let tallest = left.height.max(right.height).max(1);

    // Tops within a fraction of their own height, which is what "the same height" means for two
    // parts of one mark.
    if left.y.abs_diff(right.y) * 100 > tallest * rules.diacritic_max_height_percent {
        return false;
    }
    let gap = right.x.saturating_sub(left.right());
    gap * 100 <= widest * rules.mark_cluster_gap_percent
}

fn cluster_orphans(
    members: &[Component],
    orphans: &[usize],
    rules: GroupingRules,
) -> Vec<Vec<Component>> {
    let mut clusters: Vec<Vec<Component>> = Vec::new();

    for orphan in orphans {
        let mark = members[*orphan];
        let joined = clusters.iter_mut().find(|cluster| {
            cluster.iter().any(|other| {
                let tallest = mark.bounds.height.max(other.bounds.height).max(1);
                overlaps_enough(mark.bounds, other.bounds, rules)
                    && vertical_gap(mark.bounds, other.bounds) * 100
                        <= tallest * rules.punctuation_gap_percent
            })
        });
        match joined {
            Some(cluster) => cluster.push(mark),
            None => clusters.push(vec![mark]),
        }
    }
    clusters
}

/// Whether `mark` sits far enough over `base` horizontally to belong to it.
fn overlaps_enough(mark: Rect, base: Rect, rules: GroupingRules) -> bool {
    let left = mark.x.max(base.x);
    let right = mark.right().min(base.right());
    if right <= left {
        return false;
    }
    // The mark's centre has to fall within the letter. Denominating the overlap by the narrower of
    // the two boxes is what lets a wide mark attach to a narrow stem, and on its own it would also
    // let a mark *straddling* two narrow letters claim whichever it covers half of. A centre test
    // costs nothing and rules that out: a mark the typeface placed over a letter is centred on it,
    // and a mark between two is centred between them.
    let centre = mark.x + mark.width / 2;
    if centre < base.x || centre >= base.right() {
        return false;
    }

    let overlap = right - left;
    // The narrower of the two. See `GroupingRules::min_overlap_percent` for why this is not the
    // mark's own width.
    let narrower = mark.width.min(base.width).max(1);
    overlap * 100 >= narrower * rules.min_overlap_percent
}

/// Rows between two boxes, or zero when they overlap vertically.
const fn vertical_gap(a: Rect, b: Rect) -> u32 {
    if a.bottom() <= b.y {
        b.y - a.bottom()
    } else if b.bottom() <= a.y {
        a.y - b.bottom()
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component(x: u32, y: u32, w: u32, h: u32) -> Component {
        Component {
            bounds: Rect::new(x, y, w, h),
            pixels: u64::from(w) * u64::from(h),
            label: 0,
        }
    }

    /// Group a line of components directly, skipping the mask.
    fn group_line(components: &[Component]) -> Vec<GroupedGlyph> {
        let lines = vec![0; components.len()];
        group(components, &lines, GroupingRules::default()).unwrap()
    }

    /// A mask with ink in the given row ranges, full width.
    fn banded(height: u32, runs: &[(u32, u32)]) -> BinaryMask {
        let mut mask = BinaryMask::blank(40, height);
        for (top, bottom) in runs {
            for y in *top..*bottom {
                for x in 0..40 {
                    mask.set(x, y, true);
                }
            }
        }
        mask
    }

    #[test]
    fn two_lines_of_text_are_never_merged_however_close_they_are_set() {
        // The case that decides whether merging mark-only bands is safe, and the reason the test is
        // about band *contents* rather than the gap: a text line always carries a full-height
        // letterform, so it is never mark-only however tight the leading.
        let rules = GroupingRules::default();
        for leading in [2u32, 4, 8, 16] {
            let mask = banded(200, &[(10, 50), (50 + leading, 90 + leading)]);
            let components = [component(0, 10, 40, 40), component(0, 50 + leading, 40, 40)];
            assert_eq!(
                text_lines(&mask, &components, rules).len(),
                2,
                "two full-height lines {leading}px apart must stay two lines"
            );
        }
    }

    #[test]
    fn a_band_of_nothing_but_accents_joins_the_line_beneath_it() {
        // #57. In a line of nothing but letters the row under a capital's accent is blank across the
        // whole width, so it bands alone and `À` segments as a bare `A` plus a floating grave.
        let rules = GroupingRules::default();
        let mask = banded(200, &[(10, 18), (22, 90)]);
        let components = [component(0, 10, 8, 8), component(0, 22, 40, 68)];

        let lines = text_lines(&mask, &components, rules);
        assert_eq!(lines.len(), 1, "the accent row is not a line of its own");
        assert_eq!(lines[0], LineBand { top: 10, bottom: 90 });
    }

    #[test]
    fn a_line_of_genuine_punctuation_is_not_swallowed_by_the_paragraph_under_it() {
        // The risk the gap condition exists for. An ellipsis is mark-height throughout, so the
        // contents test alone would merge it into the line below; ordinary line leading is what
        // keeps them apart.
        let rules = GroupingRules::default();
        let mask = banded(200, &[(10, 18), (50, 118)]);
        let components = [component(0, 10, 8, 8), component(0, 50, 40, 68)];

        assert_eq!(
            text_lines(&mask, &components, rules).len(),
            2,
            "32px of leading over a 68px line is a line break, not an accent"
        );
    }

    #[test]
    fn an_empty_band_is_left_alone_rather_than_treated_as_all_marks() {
        // A band the components do not reach — the projection saw ink the filter rejected — is not
        // a band of marks. Treating "no components" as "all of them are short" would merge on the
        // strength of an empty set being vacuously true.
        let rules = GroupingRules::default();
        let mask = banded(200, &[(10, 18), (22, 90)]);
        let components = [component(0, 22, 40, 68)];

        assert_eq!(
            text_lines(&mask, &components, rules).len(),
            2,
            "the upper band holds nothing, so nothing says it is an accent row"
        );
    }

    #[test]
    fn two_dots_straddling_a_narrow_stem_are_one_mark() {
        // #58's second half. A diaeresis sits either side of the stem it belongs to, so neither dot
        // overlaps it and each fails alone where their union succeeds.
        let left = component(0, 0, 9, 9);
        let right = component(18, 0, 9, 9);
        let stem = component(9, 20, 9, 50);

        let grouped = group_line(&[left, right, stem]);
        assert_eq!(grouped.len(), 1, "one character, not three glyphs");
        assert_eq!(grouped[0].parts.len(), 3, "both dots joined the stem");
    }

    #[test]
    fn the_dots_of_two_adjacent_letters_are_not_merged_with_each_other() {
        // The hazard this issue named, and the reason only *orphans* are retried in pairs. Two `i`
        // dots are two marks at the same height, and merging them would attach one mark to one
        // stem — but each finds its own stem individually and never reaches the merge pass.
        let first_dot = component(0, 0, 9, 9);
        let first_stem = component(0, 20, 9, 50);
        let second_dot = component(27, 0, 9, 9);
        let second_stem = component(27, 20, 9, 50);

        let grouped = group_line(&[first_dot, first_stem, second_dot, second_stem]);
        assert_eq!(grouped.len(), 2, "two letters");
        assert!(
            grouped.iter().all(|g| g.parts.len() == 2),
            "each dot went to its own stem"
        );
    }

    #[test]
    fn a_colon_is_untouched_by_the_pair_retry() {
        // A colon's dots are stacked vertically, so the side-by-side test never sees them and
        // `cluster_orphans` handles them exactly as before. This is the pair `group` is built
        // around.
        let grouped = group_line(&colon(0));
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].parts.len(), 2, "still one colon of two dots");
    }

    #[test]
    fn two_marks_at_different_heights_are_not_one_mark() {
        // An apostrophe beside a hyphen: both are bodiless marks side by side, and merging them
        // would make one glyph that matches nothing — a worse failure than the two they replace.
        // Their tops disagree, which is what says they are not two halves of one mark.
        let rules = GroupingRules::default();
        let apostrophe = Rect::new(0, 0, 6, 10);
        let hyphen = Rect::new(10, 30, 12, 4);
        assert!(!side_by_side(apostrophe, hyphen, rules));
    }

    #[test]
    fn a_mark_wider_than_the_letter_under_it_still_attaches() {
        // #58. The circumflex of an `î` is three times the width of the stem it sits on — 28px over
        // 9px in Arial at 96px — so an overlap measured as a fraction of the *mark* tops out at 32%
        // against a floor of 50 and the mark never attaches. Measured against the narrower of the
        // two it is 100%, which is the question the rule was trying to ask.
        let mark = component(0, 0, 28, 13);
        let body = component(10, 20, 9, 50);
        assert!(overlaps_enough(mark.bounds, body.bounds, GroupingRules::default()));

        let grouped = group_line(&[mark, body]);
        assert_eq!(grouped.len(), 1, "one character, not two glyphs");
        assert_eq!(grouped[0].parts.len(), 2);
    }

    #[test]
    fn a_mark_straddling_two_narrow_letters_still_belongs_to_neither() {
        // The failure the change has to avoid: denominating by the narrower box could let a mark
        // that sits between two letters claim both. It cannot, because the overlap with either one
        // is then a small part of that letter too.
        let rules = GroupingRules::default();
        let left = component(0, 20, 8, 40);
        let right = component(36, 20, 8, 40);
        // A wide mark spanning the gap, covering half of each letter. Its centre lands between
        // them, in neither.
        let straddling = component(4, 0, 36, 10);
        assert!(!overlaps_enough(straddling.bounds, left.bounds, rules));
        assert!(!overlaps_enough(straddling.bounds, right.bounds, rules));

        // And the same mark shifted onto the right letter does attach, so the test is about where
        // the mark sits rather than about its width.
        let over_right = component(22, 0, 36, 10);
        assert!(overlaps_enough(over_right.bounds, right.bounds, rules));
    }

    #[test]
    fn an_accent_that_clears_every_letter_on_the_line_bands_as_a_line_of_its_own() {
        // Pinned because it is surprising, it is not what the grouping rules say, and it is a real
        // limitation rather than a hypothetical one. A mark attaches to a body only within a text
        // line, and `line_bands` cuts a line at any row carrying no ink *across its whole width*.
        // The accent on a capital sits above every letterform Arial draws, so in a line of nothing
        // but letters the row beneath it is blank and it never reaches the letter it belongs to.
        // `À` then segments as a bare `A` and a floating grave.
        //
        // Found while building #48's bench; see `docs/glyph-stability.md`, which measures it at 25
        // of 51 marks reaching their body in a letters-only line against 44 with a `$` on it. The
        // fix is #57. Until then, this test is what makes the behaviour a decision.
        let mut mask = BinaryMask::blank(8, 5);
        for x in 0..2 {
            mask.set(x, 0, true); // the mark, alone on its rows
        }
        for y in 2..5 {
            for x in [0, 1, 2, 5, 6, 7] {
                mask.set(x, y, true); // two letters, neither reaching above the mark
            }
        }

        assert_eq!(
            line_bands(&mask).len(),
            2,
            "the blank row between the mark and the letters splits the line"
        );

        let components = [
            component(0, 0, 2, 1),
            component(0, 2, 3, 3),
            component(5, 2, 3, 3),
        ];
        let lines = assign_lines(&mask, &components).unwrap();
        let glyphs = group(&components, &lines, GroupingRules::default()).unwrap();

        assert_eq!(glyphs.len(), 3, "the mark stayed a glyph instead of joining one");
        assert!(
            glyphs.iter().all(|g| g.parts.len() == 1),
            "nothing was grouped, because the mark is not on the letters' line"
        );
    }

    /// Scale a layout so the same fixture can be checked at two resolutions.
    fn scaled(components: &[Component], factor: u32) -> Vec<Component> {
        components
            .iter()
            .map(|c| {
                component(
                    c.bounds.x * factor,
                    c.bounds.y * factor,
                    c.bounds.width * factor,
                    c.bounds.height * factor,
                )
            })
            .collect()
    }

    /// A lowercase `i`: an x-height stem with a dot above it.
    fn dotted_i(x: u32) -> Vec<Component> {
        vec![component(x, 4, 3, 2), component(x, 10, 3, 12)]
    }

    /// A colon: two dots stacked, with no body under either.
    fn colon(x: u32) -> Vec<Component> {
        vec![component(x, 12, 3, 3), component(x, 19, 3, 3)]
    }

    /// A full-height letter body, as a neighbour for the punctuation cases.
    fn letter(x: u32) -> Component {
        component(x, 8, 8, 14)
    }

    #[test]
    fn a_dot_merges_into_the_stem_below_it() {
        let glyphs = group_line(&dotted_i(0));
        assert_eq!(glyphs.len(), 1, "an i is one character, not two");
        assert_eq!(glyphs[0].parts.len(), 2);
        assert_eq!(glyphs[0].bounds(), Rect::new(0, 4, 3, 18));
    }

    #[test]
    fn a_colon_stays_one_glyph_and_does_not_attach_to_the_letter_beside_it() {
        // The case the whole design turns on. Both dots are marks with no body beneath them, so
        // they cluster with each other rather than being welded onto the neighbouring letter.
        let mut components = vec![letter(0)];
        components.extend(colon(10));

        let glyphs = group_line(&components);
        assert_eq!(glyphs.len(), 2, "the letter and the colon are separate characters");
        assert_eq!(glyphs[0].parts.len(), 1, "the letter keeps to itself");
        assert_eq!(glyphs[1].parts.len(), 2, "both dots of the colon are one character");
        assert_eq!(glyphs[1].bounds(), Rect::new(10, 12, 3, 10));
    }

    #[test]
    fn a_diaeresis_and_a_colon_have_the_same_geometry_and_different_outcomes() {
        // Two dots stacked over a body, versus two dots stacked over nothing.
        let diaeresis = vec![
            component(0, 3, 2, 2),
            component(4, 3, 2, 2),
            component(0, 9, 7, 13),
        ];
        let with_body = group_line(&diaeresis);
        assert_eq!(with_body.len(), 1, "both dots belong to the letter under them");
        assert_eq!(with_body[0].parts.len(), 3);

        let without_body = group_line(&colon(0));
        assert_eq!(without_body.len(), 1, "and with no letter under them they are a colon");
        assert_eq!(without_body[0].parts.len(), 2);
    }

    #[test]
    fn a_cedilla_below_the_body_merges_upward() {
        // The inverse of a diacritic: the mark is under the letter rather than over it.
        let c_cedilla = vec![component(0, 8, 8, 12), component(3, 21, 3, 3)];
        let glyphs = group_line(&c_cedilla);
        assert_eq!(glyphs.len(), 1);
        assert_eq!(glyphs[0].parts.len(), 2);
    }

    #[test]
    fn an_exclamation_mark_keeps_its_dot() {
        let bang = vec![component(0, 8, 2, 10), component(0, 20, 2, 2)];
        assert_eq!(group_line(&bang).len(), 1);
    }

    #[test]
    fn a_full_stop_stands_alone() {
        // A lone mark with no body over or under it is a character in its own right.
        let sentence = vec![letter(0), component(10, 19, 3, 3)];
        let glyphs = group_line(&sentence);
        assert_eq!(glyphs.len(), 2);
        assert_eq!(glyphs[1].parts.len(), 1);
    }

    #[test]
    fn a_mark_does_not_attach_to_a_body_it_does_not_sit_over() {
        // The dot is horizontally clear of the letter, so it is punctuation and not an accent.
        let apart = vec![letter(0), component(30, 4, 3, 2)];
        assert_eq!(group_line(&apart).len(), 2);
    }

    #[test]
    fn a_mark_too_far_above_its_body_is_not_merged() {
        // Guards against pulling a mark down from the line above when bands run together.
        let far = vec![component(0, 0, 3, 2), component(0, 40, 3, 12)];
        assert_eq!(group_line(&far).len(), 2);
    }

    #[test]
    fn the_same_layout_groups_identically_at_two_resolutions() {
        // The property the percentage thresholds exist for. A pixel threshold tuned at the larger
        // size would merge half the line at the smaller one.
        let base = dotted_i(0);
        for factor in [1, 2, 4] {
            let glyphs = group_line(&scaled(&base, factor));
            assert_eq!(glyphs.len(), 1, "scale factor {factor} changed the grouping");
            assert_eq!(glyphs[0].parts.len(), 2);
        }
    }

    #[test]
    fn glyphs_come_back_left_to_right() {
        let mut components = vec![letter(20), letter(0)];
        components.extend(dotted_i(10));
        let glyphs = group_line(&components);
        assert_eq!(glyphs.len(), 3);
        assert_eq!(glyphs[0].bounds().x, 0);
        assert_eq!(glyphs[1].bounds().x, 10);
        assert_eq!(glyphs[2].bounds().x, 20);
    }

    #[test]
    fn mismatched_slices_are_a_configuration_error_not_a_panic() {
        let err = group(&[letter(0)], &[], GroupingRules::default()).unwrap_err();
        assert!(matches!(err, Error::Config(_)), "got {err:?}");
    }

    // --- line assignment ---

    fn mask(rows: &[&str]) -> BinaryMask {
        let height = u32::try_from(rows.len()).unwrap();
        let width = u32::try_from(rows[0].len()).unwrap();
        let bits = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c == '#'))
            .collect();
        BinaryMask::from_bits(width, height, bits).unwrap()
    }

    #[test]
    fn blank_rows_separate_the_bands() {
        let bands = line_bands(&mask(&["##..", "##..", "....", "....", "..##", "..##"]));
        assert_eq!(bands.len(), 2);
        assert_eq!(bands[0], LineBand { top: 0, bottom: 2 });
        assert_eq!(bands[1], LineBand { top: 4, bottom: 6 });
        assert_eq!(bands[0].height(), 2);
    }

    #[test]
    fn a_band_running_to_the_last_row_is_still_closed() {
        let bands = line_bands(&mask(&["....", "##.."]));
        assert_eq!(bands, vec![LineBand { top: 1, bottom: 2 }]);
    }

    #[test]
    fn components_are_assigned_to_the_band_they_sit_in() {
        let m = mask(&["##..", "##..", "....", "....", "..##", "..##"]);
        let components = [component(0, 0, 2, 2), component(2, 4, 2, 2)];
        assert_eq!(assign_lines(&m, &components).unwrap(), vec![0, 1]);
    }

    #[test]
    fn grouping_never_crosses_a_line_boundary() {
        // The failure this ordering exists to prevent: a dot on the line below being read as the
        // accent of a letter on the line above.
        let components = [component(0, 0, 3, 12), component(0, 20, 3, 2)];
        let lines = vec![0, 1];
        let glyphs = group(&components, &lines, GroupingRules::default()).unwrap();

        assert_eq!(glyphs.len(), 2, "components on different lines never merge");
        assert_eq!(glyphs[0].line, 0);
        assert_eq!(glyphs[1].line, 1);
    }

    #[test]
    fn no_components_means_no_line_assignments() {
        assert!(assign_lines(&mask(&["...."]), &[]).unwrap().is_empty());
    }

    #[test]
    fn components_from_a_different_image_are_rejected() {
        let err = assign_lines(&mask(&["....", "...."]), &[letter(0)]).unwrap_err();
        assert!(matches!(err, Error::Config(_)), "got {err:?}");
    }
}
