//! Locating a text line's baseline and cap height, so a glyph's size can mean something.
//!
//! This is the runtime half of
//! [#37](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/37). The feature vector
//! normalises every glyph to fill the grid, which is what makes one character agree with itself
//! across resolutions and also what makes `o` agree with `O` — #10 measured `I`, `l` and `|` at
//! distance *zero*. What separates those pairs is not their shape but how tall they stand in their
//! line, and that is only knowable relative to the line.
//!
//! Two anchors are needed and neither is in the image as such:
//!
//! - the **baseline**, which most characters sit on
//! - the **cap height**, which capitals, digits and ascenders reach
//!
//! Both are estimated as *modes* rather than extremes. A minimum or maximum would be decided by one
//! glyph: a single comma drags the baseline down, a single `É` pushes the cap height up, and every
//! measurement on the line shifts with it. A mode asks what the glyphs on the line mostly agree on,
//! which is what a baseline is.
//!
//! Lines that cannot answer report [`LineMetrics::UNKNOWN`] rather than a plausible number. A line
//! of one or two glyphs genuinely has no baseline to find, and a fabricated one would be
//! indistinguishable from a measured one while biasing every match on that line.

use subtrackt_core::{LineMetrics, Rect};

use crate::group::{GroupedGlyph, LineBand};

/// How the anchors are found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricRules {
    /// How far apart two edges may sit and still count as the same one, in percent of band height.
    ///
    /// Glyphs on a baseline do not share a pixel row exactly: rounded letters overshoot it and
    /// rasterisation moves edges by a pixel either way. This is the width of the bucket the mode is
    /// taken over — a fraction of a measured quantity, because the same title ships at several
    /// resolutions and a pixel count would mean something different at each.
    pub edge_tolerance_percent: u32,
    /// Fewest glyphs a line needs before its metrics are trusted.
    ///
    /// Below this there is no mode worth taking, only a guess dressed as a measurement.
    pub min_glyphs: usize,
    /// Fewest glyphs that must agree on the baseline for it to count.
    ///
    /// In percent of the line's glyphs. A line whose bottoms scatter has no baseline, which happens
    /// to lines of nothing but punctuation.
    pub min_agreement_percent: u32,
    /// Fewest glyphs that must reach a row before it can be the cap line, in percent of the line's
    /// glyphs standing on the baseline.
    ///
    /// The cap line is the *highest* row enough glyphs reach — not the highest row anything
    /// reaches. One accented capital rises above cap height and would otherwise define it alone.
    pub min_cap_support_percent: u32,
    /// How much taller the tallest standing glyph must be than the shortest, in percent of the
    /// tallest, before the cap line means anything.
    ///
    /// A line of uniform height carries no information about which height it is. `NO ONE SAW` and
    /// `no one saw` present identically: same shapes, same one height, and nothing to say whether
    /// that height is cap or x. Measuring either would make every glyph on the line read as a
    /// capital, so such a line reports unknown and falls back to shape alone.
    pub min_height_variety_percent: u32,
}

impl Default for MetricRules {
    fn default() -> Self {
        Self {
            edge_tolerance_percent: 8,
            min_glyphs: 4,
            min_agreement_percent: 40,
            min_cap_support_percent: 15,
            min_height_variety_percent: 12,
        }
    }
}

/// The anchors of one text line, in image rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineAnchors {
    /// The row most glyph bottoms sit on.
    pub baseline: u32,
    /// The row the tall glyphs mostly reach up to.
    pub cap_top: u32,
}

impl LineAnchors {
    /// Cap height: the distance from the cap line down to the baseline.
    #[must_use]
    pub const fn cap_height(self) -> u32 {
        self.baseline.saturating_sub(self.cap_top)
    }
}

/// The most popular value in `values`, where values within `tolerance` count as the same.
///
/// Ties break towards the larger value, which only matters for the baseline: with two equally
/// popular bottoms, the lower one is more likely the real baseline and the higher one a row of
/// glyphs that happen to stop short.
fn mode(values: &[u32], tolerance: u32) -> Option<(u32, usize)> {
    values
        .iter()
        .map(|candidate| {
            let members: Vec<u32> = values
                .iter()
                .copied()
                .filter(|v| v.abs_diff(*candidate) <= tolerance)
                .collect();
            // Report the bucket's own centre rather than the candidate that seeded it, so the
            // answer does not depend on which member happened to be tried first.
            let sum: u32 = members.iter().sum();
            let centre = sum / u32::try_from(members.len()).unwrap_or(1);
            (centre, members.len())
        })
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
}

/// Find a line's baseline and cap height from the glyphs on it.
///
/// Returns `None` when the line cannot support the estimate.
#[must_use]
pub fn anchors(band: LineBand, boxes: &[Rect], rules: MetricRules) -> Option<LineAnchors> {
    if boxes.len() < rules.min_glyphs || band.height() == 0 {
        return None;
    }
    let tolerance = band.height() * rules.edge_tolerance_percent / 100;

    // The baseline is where glyph bottoms agree. Descenders and floating marks are the minority and
    // fall outside the bucket, which is exactly what taking a mode is for.
    let bottoms: Vec<u32> = boxes.iter().map(|b| b.y + b.height).collect();
    let (baseline, agreed) = mode(&bottoms, tolerance)?;
    if agreed * 100 < boxes.len() * rules.min_agreement_percent as usize {
        return None;
    }

    // The cap line is where the tops of the *tall* glyphs agree. Only glyphs standing on the
    // baseline are asked: a superscript or a floating mark reaches high without being cap height.
    let standing: Vec<&Rect> = boxes
        .iter()
        .filter(|b| (b.y + b.height).abs_diff(baseline) <= tolerance)
        .collect();
    if standing.is_empty() {
        return None;
    }

    // A line whose glyphs are all one height cannot say which height that is. Refusing here is
    // what stops an all-lowercase line from reading as all capitals.
    let tallest = standing.iter().map(|b| b.height).max()?;
    let shortest = standing.iter().map(|b| b.height).min()?;
    if (tallest - shortest) * 100 < tallest * rules.min_height_variety_percent {
        return None;
    }

    // The cap line is the highest row that enough glyphs reach. Taking the highest row *any* glyph
    // reaches would hand the decision to a single accented capital, which sits above cap height by
    // design — the same argument that made the baseline a mode rather than a maximum.
    let tops: Vec<u32> = standing.iter().map(|b| b.y).collect();
    let support = (standing.len() * rules.min_cap_support_percent as usize / 100).max(2);
    let cap_top = tops
        .iter()
        .map(|candidate| {
            let reaching = tops.iter().filter(|t| t.abs_diff(*candidate) <= tolerance);
            let members: Vec<u32> = reaching.copied().collect();
            let sum: u32 = members.iter().sum();
            (sum / u32::try_from(members.len()).unwrap_or(1), members.len())
        })
        .filter(|(_, count)| *count >= support)
        .map(|(centre, _)| centre)
        .min()?;

    let anchors = LineAnchors { baseline, cap_top };
    (anchors.cap_height() > 0).then_some(anchors)
}

/// Measure one glyph against its line's anchors.
#[must_use]
pub fn measure(bounds: Rect, anchors: LineAnchors) -> LineMetrics {
    let unit = anchors.cap_height();
    if unit == 0 {
        return LineMetrics::UNKNOWN;
    }
    let unit = i64::from(unit);

    let height = i64::from(bounds.height) * 100 / unit;
    let bottom = i64::from(bounds.y) + i64::from(bounds.height);
    let descent = (bottom - i64::from(anchors.baseline)) * 100 / unit;

    LineMetrics::new(
        u32::try_from(height).unwrap_or(u32::MAX),
        i32::try_from(descent).unwrap_or(0),
    )
}

/// Measure every glyph on every line of an image.
///
/// Returns one [`LineMetrics`] per entry of `glyphs`, in the same order.
#[must_use]
pub fn measure_all(
    bands: &[LineBand],
    glyphs: &[GroupedGlyph],
    rules: MetricRules,
) -> Vec<LineMetrics> {
    // One set of anchors per line, from that line's glyphs only. Pooling across lines would be
    // wrong even though it would be steadier: a two-line cue can hold two different type sizes.
    let anchors: Vec<Option<LineAnchors>> = bands
        .iter()
        .enumerate()
        .map(|(index, band)| {
            let boxes: Vec<Rect> = glyphs
                .iter()
                .filter(|g| g.line == index)
                .map(GroupedGlyph::bounds)
                .collect();
            anchors(*band, &boxes, rules)
        })
        .collect();

    glyphs
        .iter()
        .map(|glyph| {
            anchors
                .get(glyph.line)
                .copied()
                .flatten()
                .map_or(LineMetrics::UNKNOWN, |a| measure(glyph.bounds(), a))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ccl::Component;

    /// A glyph occupying `bounds` on `line`.
    fn glyph(line: usize, x: u32, y: u32, width: u32, height: u32) -> GroupedGlyph {
        GroupedGlyph {
            parts: vec![Component {
                bounds: Rect::new(x, y, width, height),
                pixels: u64::from(width * height),
            }],
            line,
        }
    }

    /// A line of mixed capitals and x-height letters on a baseline at row 40, cap height 30.
    ///
    /// Mixed rather than uniform because that is what a line of text is, and because a line of one
    /// height carries no information about which height it is — see `min_height_variety_percent`.
    fn capitals() -> Vec<Rect> {
        let mut boxes: Vec<Rect> = (0..3).map(|i| Rect::new(i * 20, 10, 14, 30)).collect();
        boxes.extend((3..7).map(|i| Rect::new(i * 20, 18, 14, 22)));
        boxes
    }

    fn band() -> LineBand {
        LineBand { top: 10, bottom: 41 }
    }

    #[test]
    fn the_baseline_is_where_most_glyph_bottoms_agree() {
        let anchors = anchors(band(), &capitals(), MetricRules::default()).unwrap();
        assert_eq!(anchors.baseline, 40);
        assert_eq!(anchors.cap_top, 10);
        assert_eq!(anchors.cap_height(), 30);
    }

    #[test]
    fn one_descender_does_not_drag_the_baseline_down() {
        // The reason the estimate is a mode and not a maximum. A single `p` on a line of capitals
        // would move the baseline eight pixels and shift every measurement on the line with it.
        let mut boxes = capitals();
        boxes.push(Rect::new(200, 18, 14, 30)); // a descender: bottom at 48
        let anchors = anchors(band(), &boxes, MetricRules::default()).unwrap();
        assert_eq!(anchors.baseline, 40, "the majority still decides");
    }

    #[test]
    fn one_tall_accent_does_not_push_the_cap_height_up() {
        // The same argument at the other end: an `É` reaches above cap height, and taking a maximum
        // would make every other glyph on the line measure short.
        let mut boxes = capitals();
        boxes.push(Rect::new(200, 2, 14, 38)); // accented capital, top at 2
        let anchors = anchors(band(), &boxes, MetricRules::default()).unwrap();
        assert_eq!(anchors.cap_top, 10, "the majority still decides");
    }

    #[test]
    fn a_capital_measures_full_height_and_a_lowercase_letter_measures_less() {
        // The property the whole feature exists for: `o` and `O` must not measure the same.
        let anchors = anchors(band(), &capitals(), MetricRules::default()).unwrap();

        let capital = measure(Rect::new(0, 10, 14, 30), anchors);
        let lowercase = measure(Rect::new(0, 18, 14, 22), anchors);

        assert_eq!(capital.height_percent, 100);
        assert_eq!(lowercase.height_percent, 73);
        assert_eq!(capital.descent_percent, 0);
        assert_eq!(lowercase.descent_percent, 0, "both sit on the baseline");
        assert!(capital.difference(lowercase).unwrap() > 20, "and they are far apart");
    }

    #[test]
    fn a_descender_reads_below_the_baseline_and_a_floating_mark_above_it() {
        let anchors = anchors(band(), &capitals(), MetricRules::default()).unwrap();

        // A `p`: x-height tall, dropping 8 rows past the baseline.
        let descender = measure(Rect::new(0, 18, 14, 30), anchors);
        assert!(descender.descent_percent > 20, "got {descender:?}");

        // A hyphen: short, floating clear of the baseline entirely.
        let hyphen = measure(Rect::new(0, 22, 14, 4), anchors);
        assert!(
            hyphen.descent_percent < -30,
            "a floating mark must read negative, or it merges with an underscore: {hyphen:?}"
        );
    }

    #[test]
    fn a_line_with_too_few_glyphs_reports_nothing_rather_than_guessing() {
        // Two glyphs have no mode. A plausible baseline invented here would be indistinguishable
        // from a measured one and would bias every match on the line.
        let boxes = vec![Rect::new(0, 10, 14, 30), Rect::new(20, 10, 14, 30)];
        assert!(anchors(band(), &boxes, MetricRules::default()).is_none());
    }

    #[test]
    fn a_line_whose_bottoms_scatter_has_no_baseline() {
        // A line of nothing but punctuation at assorted heights. There is no baseline to find and
        // saying so is the correct answer.
        let boxes: Vec<Rect> = (0..6)
            .map(|i| Rect::new(i * 20, 10 + i * 4, 6, 6))
            .collect();
        assert!(anchors(band(), &boxes, MetricRules::default()).is_none());
    }

    #[test]
    fn an_unknown_metric_contributes_no_distance_rather_than_a_penalty() {
        let known = LineMetrics::new(100, 0);
        assert_eq!(known.difference(LineMetrics::UNKNOWN), None);
        assert_eq!(LineMetrics::UNKNOWN.difference(known), None);
        assert_eq!(known.difference(known), Some(0));
    }

    #[test]
    fn a_zero_height_line_is_rejected_rather_than_dividing_by_zero() {
        let flat = LineBand { top: 10, bottom: 10 };
        assert!(anchors(flat, &capitals(), MetricRules::default()).is_none());
        assert_eq!(
            measure(Rect::new(0, 0, 4, 4), LineAnchors { baseline: 10, cap_top: 10 }),
            LineMetrics::UNKNOWN
        );
    }

    #[test]
    fn every_glyph_gets_a_metric_and_they_follow_their_own_lines() {
        // Two lines at different sizes. Pooling them would make the smaller line's capitals measure
        // as lowercase, which is the failure this per-line split exists to avoid.
        let bands = [
            LineBand { top: 10, bottom: 41 },
            LineBand { top: 60, bottom: 76 },
        ];
        // Each line mixes cap-height and x-height glyphs, as a line of text does.
        let mut glyphs: Vec<GroupedGlyph> = (0..3).map(|i| glyph(0, i * 20, 10, 14, 30)).collect();
        glyphs.extend((3..7).map(|i| glyph(0, i * 20, 18, 14, 22)));
        glyphs.extend((0..3).map(|i| glyph(1, i * 20, 60, 7, 15)));
        glyphs.extend((3..7).map(|i| glyph(1, i * 20, 64, 7, 11)));

        let measured = measure_all(&bands, &glyphs, MetricRules::default());
        assert_eq!(measured.len(), glyphs.len());
        assert!(measured.iter().all(|m| m.known));
        assert_eq!(measured[0].height_percent, 100);
        assert_eq!(measured[3].height_percent, 73, "and an x-height letter is not");
        assert_eq!(
            measured[7].height_percent, 100,
            "a capital on the smaller line is still a capital"
        );
    }

    #[test]
    fn a_glyph_on_an_unmeasurable_line_reports_unknown() {
        let bands = [LineBand { top: 10, bottom: 41 }];
        let glyphs = vec![glyph(0, 0, 10, 14, 30), glyph(0, 20, 10, 14, 30)];
        let measured = measure_all(&bands, &glyphs, MetricRules::default());
        assert!(measured.iter().all(|m| !m.known));
    }

    #[test]
    fn tolerance_is_a_fraction_of_the_line_rather_than_a_pixel_count() {
        // The same title ships at several resolutions. A tolerance in pixels would be generous at
        // 480p and useless at 1080p.
        let small = LineBand { top: 0, bottom: 20 };
        let large = LineBand { top: 0, bottom: 200 };
        let rules = MetricRules::default();

        // Bottoms scattered by 4% of the band height must bucket together at either size.
        let scatter = |height: u32| -> Vec<Rect> {
            (0..8)
                .map(|i| {
                    // Bottoms jittered by 4% of the band, heights mixed so the line has variety.
                    let jitter = (i % 2) * (height * 4 / 100);
                    let tall = i < 4;
                    let glyph_height = if tall { height * 2 / 3 } else { height / 2 };
                    Rect::new(i * 20, height - glyph_height - jitter, 10, glyph_height)
                })
                .collect()
        };
        assert!(anchors(small, &scatter(20), rules).is_some());
        assert!(anchors(large, &scatter(200), rules).is_some());
    }
}
