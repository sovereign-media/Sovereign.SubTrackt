//! What a leaning line does to the two measurements taken before the matcher ever sees it.
//!
//! [#115](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/115). The italic act is
//! 6% of 10 Cloverfield Lane's characters and a third of the errors left on it, and the issue
//! proposes normalising the slant out of a line before the line is segmented. Before any of that
//! is built there are two questions that cost one pass over a disc each, and both of them can kill
//! a different half of the proposal:
//!
//! - **Is the gap between two italic boxes clamped at zero?** [`layout.rs`] measures the space
//!   between two glyphs as `next.x.saturating_sub(this.right())` — a gap between *bounding boxes*.
//!   A slanted ascender's box is mostly slant, so it overhangs the box of the letter after it and
//!   the subtraction saturates. #40's rule cuts at the widest jump between consecutive sorted gaps,
//!   which needs the line's gaps to be bimodal; a run of clamped zeros collapses the letter-gap
//!   mode onto the floor and takes the band with it. If few italic gaps are actually zero, the
//!   mechanism is something else and the proposal's first prediction is wrong.
//! - **Can one number tell a leaning line from an upright one?** That is the proposal's bonus — an
//!   `<i>` on the output — and it is also the estimator every deskew would use. It needs no
//!   pipeline change to answer, because the release subtitle already marks its italic cues.
//!
//! Both are reported here rather than in two tools because they are one pass over the same
//! material and they share every join: cue to release cue by time, glyph to line by the assembler's
//! own reading order.
//!
//! The slant estimate is [#115]'s **candidate C**, the image-moment one, and it is here first for
//! the reason the issue gives — a Radon sweep hand-rolled in std is not free, and if a one-pass
//! moment answers the question then the sweep is bought for nothing. `mark.rs` reads a diacritic's
//! direction from the same second moment, so this is a second reading of machinery #48 already
//! measured rather than new machinery.
//!
//! The caveat `disc.rs` opens with applies to every italic figure below: a release subtitle is an
//! independent transcript, so its `<i>` is what *that* release thought was italic. It is evidence
//! about a distribution over hundreds of cues and would not be evidence about any single one.
//!
//! [`layout.rs`]: subtrackt_text::layout

// Every ratio here divides one count of pixels, gaps or cues by another, and a feature film holds
// tens of thousands of each — far inside the 2^53 an f64 counts exactly. Same reasoning
// `geometry.rs` records for the same allow.
#![allow(clippy::cast_precision_loss)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Context as _;
use subtrackt::{Config, GlyphSurvey, Pipeline, UnmatchedPolicy};
use subtrackt_core::Rect;
use subtrackt_glyph::ReferenceSet;
use subtrackt_glyph::binarize::BinaryMask;
use subtrackt_text::layout::{LayoutRules, split_threshold};

use crate::disc;

/// Ink pixels a line needs before its slant is worth reading.
///
/// A slant estimate is a ratio of two second moments, and a second moment over a handful of pixels
/// is noise. `mark.rs` guards its own moment the same way and for the same reason. Below this the
/// line reports **unknown** — never upright — which is the boundary `CLAUDE.md` requires and the
/// choice `MarkSlope::NONE` and `LineMetrics::known` already make.
const MIN_INK: usize = 200;

/// Glyphs a line needs before its slant is worth reading.
///
/// Separate from the ink floor because they fail differently: one full stop at 1080p clears neither,
/// but a single large `O` clears the ink floor while carrying no stem to lean. Both are required.
const MIN_GLYPHS: usize = 4;

/// Shear below which a line is read as leaning, in percent.
///
/// Only a reading aid, and it is here for one reason: **not every release marks its italics.** Both
/// of Gone Girl's English sidecars carry no `<i>` anywhere, so the release-labelled tables above
/// have one column on that disc and cannot say anything about the other. The estimator can, because
/// it never opens a subtitle file — so the same split is taken a second time from the ink alone,
/// and a disc whose transcript lost the distinction still contributes.
///
/// The number is read off the two discs that *do* carry labels: their best cuts are -7.9 and -6.9,
/// their upright bodies reach +3.5 and +4.3 at p90, and their italic bodies reach -12.2 and -10.6 at
/// p75. Anything in the empty band between would do, which is what makes it worth printing rather
/// than tuning. It is a slope, so it is already scale-free and needs no cap height to divide by.
const LEANING_PERCENT: f64 = -6.0;

/// One line of one cue, as the two measurements below see it.
struct Line {
    /// Which cue, so a line on the wrong side of a threshold can be found.
    cue: usize,
    /// Whether the release marked the cue italic.
    italic: bool,
    /// Gaps between consecutive glyph boxes, signed — negative where the boxes overlap.
    ///
    /// Signed is the whole point. The runtime's `saturating_sub` cannot distinguish a gap that is
    /// genuinely zero from one that is minus four, and the difference between those two is the
    /// difference between "the letters touch" and "the rule has no band left to cut in".
    gaps: Vec<i64>,
    /// Median glyph width, the yardstick #40's first decisiveness test measures against.
    width: u32,
    /// The shear that makes the line's pooled ink covariance cross term vanish, in percent.
    ///
    /// `None` where the line carries too little ink or too few glyphs to say. See [`MIN_INK`].
    shear: Option<f64>,
    /// The same gaps, measured between the glyphs' **deskewed** ink extents.
    ///
    /// The remedy, priced without building it. Nothing here shears a bitmap or moves a component:
    /// the line's own shear is applied to each glyph's ink to ask where its leftmost and rightmost
    /// columns *would* be once it stood upright, and the gaps are measured between those. If the
    /// clamped run above is really the slant, these are the gaps the spacing rule should have been
    /// given, and they should look like the upright column.
    deskewed: Vec<f64>,
}

/// Measure gaps and slant over a real disc, split by what the release says was italic.
///
/// # Errors
/// Fails if the media, the reference set or the release subtitle cannot be read, or if the pass
/// over the file fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let media: PathBuf = args
        .first()
        .context("usage: slant <media> <reference.subtref> <release.srt>")?
        .into();
    let set: PathBuf = args.get(1).context("missing the reference set")?.into();
    let release = args.get(2).context("missing the release subtitle")?;

    let reference =
        ReferenceSet::decode(&std::fs::read(&set)?).map_err(|e| anyhow::anyhow!("{e}"))?;
    // Masks on, because the slant estimate reads ink rather than the letterboxed vector — at 16x16
    // a stem is one to three cells and its lean is quantised away. Placeholder rather than the
    // default gate, for the reason `xtask unread` gives: a policy that refuses the track would
    // leave nothing to measure.
    let config = Config {
        unmatched: UnmatchedPolicy::Placeholder,
        glyph_masks: true,
        ..Config::default()
    };
    let pipeline = Pipeline::new(config).with_reference(reference);

    // Two passes, joined by index, exactly as `xtask glyph-geometry` does: the extraction knows
    // when each cue is on screen and nothing about a glyph's box, the survey knows every box and
    // nothing about time. The assertion is here rather than assumed because a silent off-by-one
    // would mislabel the whole film's style.
    let outcome = pipeline
        .run(&media)
        .with_context(|| format!("extracting {}", media.display()))?;
    let survey = pipeline
        .survey(&media, None)
        .with_context(|| format!("surveying {}", media.display()))?;
    anyhow::ensure!(
        outcome.track.cues.len() == survey.cues,
        "the extraction produced {} cues and the survey saw {} images; they cannot be joined by \
         index",
        outcome.track.cues.len(),
        survey.cues
    );

    let want = disc::read(release)?;
    let lines = measure(&outcome, &survey, &want);

    println!(
        "{}: {} cues, {} glyphs, {} lines paired to the release",
        media.display(),
        survey.cues,
        survey.glyphs.len(),
        lines.len()
    );
    gaps(&lines);
    slant(&lines);
    Ok(())
}

/// Build one [`Line`] per text line the release can label.
fn measure(outcome: &subtrackt::Outcome, survey: &GlyphSurvey, want: &[disc::Cue]) -> Vec<Line> {
    let mut by_cue: Vec<Vec<usize>> = vec![Vec::new(); survey.cues];
    for (index, glyph) in survey.glyphs.iter().enumerate() {
        if let Some(slot) = by_cue.get_mut(glyph.cue) {
            slot.push(index);
        }
    }

    let mut out = Vec::new();
    for (cue_index, members) in by_cue.iter().enumerate() {
        let Some(cue) = outcome.track.cues.get(cue_index) else {
            continue;
        };
        let at = i64::try_from(cue.span.start.as_millis()).unwrap_or(i64::MAX);
        // A cue with no partner inside the tolerance is dropped rather than assumed upright. The
        // release is the only thing here that knows the style, so an unpaired cue has no style —
        // and defaulting one would put the upright column's own premise beyond checking.
        let Some(release) = nearest(want, at) else {
            continue;
        };

        let mut per_line: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &index in members {
            per_line
                .entry(survey.glyphs[index].line)
                .or_default()
                .push(index);
        }

        for (_, mut line) in per_line {
            // The assembler's own reading order. Reproduced rather than borrowed because what is
            // wanted here is the geometry it ranks over, not the string it returns.
            line.sort_by_key(|&index| survey.glyphs[index].bounds.x);

            let gaps: Vec<i64> = line
                .windows(2)
                .map(|pair| {
                    let (this, next) =
                        (&survey.glyphs[pair[0]].bounds, &survey.glyphs[pair[1]].bounds);
                    i64::from(next.x) - i64::from(this.right())
                })
                .collect();
            let mut widths: Vec<u32> = line
                .iter()
                .map(|&index| survey.glyphs[index].bounds.width)
                .collect();
            widths.sort_unstable();
            let width = widths.get(widths.len() / 2).copied().unwrap_or(0);

            let masks: Vec<(&BinaryMask, Rect)> = line
                .iter()
                .filter_map(|&index| {
                    let glyph = &survey.glyphs[index];
                    glyph.mask.as_ref().map(|mask| (mask, glyph.bounds))
                })
                .collect();

            let shear = shear_of(&masks, line.len());
            let deskewed =
                shear.map_or_else(Vec::new, |shear| deskewed_gaps(&masks, shear / 100.0));
            out.push(Line {
                cue: cue_index,
                italic: release.italic,
                gaps,
                width,
                shear,
                deskewed,
            });
        }
    }
    out
}

/// The gaps between consecutive glyphs once the line's own slant is taken out of their extents.
///
/// A glyph's deskewed extent is the range of `x' = x - k·y` over its ink, taken over the pixel
/// *squares* rather than their top-left corners — a pixel at row `y` occupies rows `y..y+1`, and
/// under a shear those two rows do not map to the same column. Reading the corner alone would lose
/// half a stem's width at the extremes, which is the whole quantity being measured here.
///
/// Fractional and unrounded on purpose. The runtime's own gap is an integer subtraction that
/// saturates at zero; whether the fractional value is bimodal is exactly the question, and rounding
/// it here would be answering with the instrument.
fn deskewed_gaps(masks: &[(&BinaryMask, Rect)], shear: f64) -> Vec<f64> {
    let extents: Vec<(f64, f64)> = masks
        .iter()
        .filter_map(|(mask, bounds)| {
            let (mut lo, mut hi) = (f64::MAX, f64::MIN);
            for y in 0..bounds.height {
                for x in 0..bounds.width {
                    if !mask.get(x, y) {
                        continue;
                    }
                    let (px, py) = (f64::from(bounds.x + x), f64::from(bounds.y + y));
                    for (cx, cy) in [
                        (px, py),
                        (px + 1.0, py),
                        (px, py + 1.0),
                        (px + 1.0, py + 1.0),
                    ] {
                        let sheared = cx - shear * cy;
                        lo = lo.min(sheared);
                        hi = hi.max(sheared);
                    }
                }
            }
            (hi > lo).then_some((lo, hi))
        })
        .collect();
    extents
        .windows(2)
        .map(|pair| pair[1].0 - pair[0].1)
        .collect()
}

/// The release cue starting nearest `at`, within the tolerance the score uses.
fn nearest(cues: &[disc::Cue], at: i64) -> Option<&disc::Cue> {
    cues.iter()
        .filter(|cue| (cue.start_ms - at).abs() <= disc::TOLERANCE_MS)
        .min_by_key(|cue| (cue.start_ms - at).abs())
}

/// The shear that would stand a line's ink upright, as a percentage: x' = x - k·y.
///
/// Each glyph's ink contributes its covariance about **its own** centroid, and the sums are pooled
/// across the line. Pooling the raw pixels instead would measure the line's layout — a row of
/// letters is enormously wider than it is tall, so its cross term is dominated by where the words
/// sit and by a baseline that a descender or a comma pulls off level. What slant actually is, is a
/// property every glyph carries separately and shares with its neighbours, so the per-glyph moment
/// is the one that has it.
///
/// `k = Cxy / Cyy` is the shear that makes the pooled cross term vanish, which is the definition of
/// "the stems now stand vertical". It is a slope — pixels of x per pixel of y — so it is
/// dimensionless and survives the resolution change `CLAUDE.md` requires every threshold to survive.
/// Its sign follows the plane's: y grows downward, so an italic leaning to the right at the top has
/// a **negative** cross term.
fn shear_of(masks: &[(&BinaryMask, Rect)], glyphs: usize) -> Option<f64> {
    if glyphs < MIN_GLYPHS {
        return None;
    }
    let (mut cyy, mut cxy, mut ink) = (0f64, 0f64, 0usize);
    for (mask, bounds) in masks {
        let (mut count, mut sum_x, mut sum_y) = (0f64, 0f64, 0f64);
        for y in 0..bounds.height {
            for x in 0..bounds.width {
                if mask.get(x, y) {
                    count += 1.0;
                    sum_x += f64::from(x);
                    sum_y += f64::from(y);
                }
            }
        }
        if count == 0.0 {
            continue;
        }
        let (mean_x, mean_y) = (sum_x / count, sum_y / count);
        for y in 0..bounds.height {
            for x in 0..bounds.width {
                if mask.get(x, y) {
                    let (dx, dy) = (f64::from(x) - mean_x, f64::from(y) - mean_y);
                    cyy += dy * dy;
                    cxy += dx * dy;
                }
            }
        }
        ink += ink_of(mask, *bounds);
    }
    if ink < MIN_INK || cyy <= 0.0 {
        return None;
    }
    Some(cxy / cyy * 100.0)
}

/// Ink pixels inside one glyph's box.
fn ink_of(mask: &BinaryMask, bounds: Rect) -> usize {
    (0..bounds.height)
        .flat_map(|y| (0..bounds.width).map(move |x| (x, y)))
        .filter(|(x, y)| mask.get(*x, *y))
        .count()
}

/// Percentile of an already-sorted slice, `at` in percent.
///
/// Integer arithmetic on the index rather than a float one, because a percentile is a position in a
/// list and not a measurement — rounding one through `f64` is a cast this project has no reason to
/// make and clippy has every reason to object to.
fn percentile(sorted: &[f64], at: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[(sorted.len() - 1) * at.min(100) / 100]
}

/// A fractional gap as the runtime's own integer subtraction would have reported it.
///
/// Rounded and saturated at zero, which is exactly what `next.x.saturating_sub(this.right())` does
/// to a gap it cannot represent. Used only where a deskewed gap is handed to the *shipped* rule, so
/// that the two columns differ in the gap that was measured and in nothing else.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn as_runtime_gap(gap: f64) -> u32 {
    gap.round().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// Whether the shipped rule declines to place any space on this line.
///
/// `None` from `split_threshold` is the rule saying the line's gaps do not separate into two
/// classes decisively enough to cut — which on a line that holds one word is right, and on a line
/// of dialogue means the whole line arrives as one word.
fn refuses(line: &Line, gaps: &[u32]) -> bool {
    gaps.len() >= 2 && split_threshold(gaps, line.width, LayoutRules::default()).is_none()
}

/// The gaps of a group of lines as the runtime saw them, and as a deskew would have.
fn both_gaps(members: &[&Line]) -> (Vec<u32>, Vec<u32>) {
    let before = members
        .iter()
        .flat_map(|line| {
            line.gaps
                .iter()
                .map(|gap| u32::try_from(*gap).unwrap_or(0))
                .collect::<Vec<u32>>()
        })
        .collect();
    let after = members
        .iter()
        .flat_map(|line| {
            line.deskewed
                .iter()
                .map(|gap| as_runtime_gap(*gap))
                .collect::<Vec<u32>>()
        })
        .collect();
    (before, after)
}

/// How often the shipped rule refuses a group of lines, before and after the deskew, in percent.
fn refusal_rates(members: &[&Line]) -> (f64, f64) {
    let scored: Vec<&&Line> = members.iter().filter(|l| l.gaps.len() >= 2).collect();
    if scored.is_empty() {
        return (0.0, 0.0);
    }
    let rate = |count: usize| count as f64 * 100.0 / scored.len() as f64;
    let before = scored
        .iter()
        .filter(|line| {
            let gaps: Vec<u32> = line
                .gaps
                .iter()
                .map(|gap| u32::try_from(*gap).unwrap_or(0))
                .collect();
            refuses(line, &gaps)
        })
        .count();
    let after = scored
        .iter()
        .filter(|line| {
            let gaps: Vec<u32> = line
                .deskewed
                .iter()
                .map(|gap| as_runtime_gap(*gap))
                .collect();
            refuses(line, &gaps)
        })
        .count();
    (rate(before), rate(after))
}

/// The two groups every table below is split into, and how a line is put in one.
type Split = (&'static str, fn(&Line) -> bool);
const BY_RELEASE: [Split; 2] = [
    ("upright", |line| !line.italic),
    ("italic", |line| line.italic),
];
/// The same split taken from the ink, for a disc whose sidecar never marked its italics.
const BY_INK: [Split; 2] = [
    ("upright", |line| line.shear.is_some_and(|s| s >= LEANING_PERCENT)),
    ("leaning", |line| line.shear.is_some_and(|s| s < LEANING_PERCENT)),
];

/// Is the space between two italic boxes clamped at zero?
fn gaps(lines: &[Line]) {
    println!("\n--- gaps between consecutive glyph boxes (#115) ---");
    println!(
        "  the runtime measures `next.x.saturating_sub(this.right())`, so everything at or below"
    );
    println!("  zero arrives at the spacing rule as the same number.");
    println!(
        "  {:>9} {:>8} {:>8} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "", "lines", "gaps", "negative", "zero", "clamped", "p50", "p90"
    );
    for (name, belongs) in BY_RELEASE {
        let members: Vec<&Line> = lines.iter().filter(|l| belongs(l)).collect();
        let all: Vec<i64> = members
            .iter()
            .flat_map(|l| l.gaps.iter().copied())
            .collect();
        if all.is_empty() {
            continue;
        }
        let negative = all.iter().filter(|gap| **gap < 0).count();
        let zero = all.iter().filter(|gap| **gap == 0).count();
        let mut sorted: Vec<f64> = all.iter().map(|gap| *gap as f64).collect();
        sorted.sort_by(f64::total_cmp);
        println!(
            "  {name:>9} {:>8} {:>8} {:>8.1}% {:>8.1}% {:>8.1}% {:>9.0} {:>9.0}",
            members.len(),
            all.len(),
            negative as f64 * 100.0 / all.len() as f64,
            zero as f64 * 100.0 / all.len() as f64,
            (negative + zero) as f64 * 100.0 / all.len() as f64,
            percentile(&sorted, 50),
            percentile(&sorted, 90),
        );
    }

    relative_gaps(lines);
    deskewed_histogram(lines);
    refusals(lines);
}

/// The same gaps as a fraction of the line's median glyph width.
///
/// The yardstick #40's first decisiveness test measures against, and the only scale-free way to put
/// a 1080p line beside a 480p one.
fn relative_gaps(lines: &[Line]) {
    println!("\n  the same gaps as a percentage of the line's median glyph width");
    println!(
        "  {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "", "p10", "p25", "p50", "p75", "p90"
    );
    for (name, belongs) in BY_RELEASE {
        let mut relative: Vec<f64> = lines
            .iter()
            .filter(|l| belongs(l) && l.width > 0)
            .flat_map(|l| {
                l.gaps
                    .iter()
                    .map(move |gap| *gap as f64 * 100.0 / f64::from(l.width))
            })
            .collect();
        if relative.is_empty() {
            continue;
        }
        relative.sort_by(f64::total_cmp);
        println!(
            "  {name:>9} {:>9.0} {:>9.0} {:>9.0} {:>9.0} {:>9.0}",
            percentile(&relative, 10),
            percentile(&relative, 25),
            percentile(&relative, 50),
            percentile(&relative, 75),
            percentile(&relative, 90),
        );
    }
}

/// The same gaps once the line's own slant is divided back out of the glyph extents.
///
/// Nothing is shifted, sheared or resegmented to produce these — see [`deskewed_gaps`] — so this is
/// the remedy priced rather than built.
fn deskewed_histogram(lines: &[Line]) {
    println!("\n  the same gaps, measured between deskewed ink extents");
    println!(
        "  {:>9} {:>8} {:>9} {:>9} {:>9} {:>9}",
        "", "gaps", "at or below 0", "p25", "p50", "p90"
    );
    for (name, belongs) in BY_RELEASE {
        let mut all: Vec<f64> = lines
            .iter()
            .filter(|l| belongs(l))
            .flat_map(|l| l.deskewed.iter().copied())
            .collect();
        if all.is_empty() {
            continue;
        }
        let clamped = all.iter().filter(|gap| **gap <= 0.0).count();
        all.sort_by(f64::total_cmp);
        println!(
            "  {name:>9} {:>8} {:>8.1}% {:>9.1} {:>9.1} {:>9.1}",
            all.len(),
            clamped as f64 * 100.0 / all.len() as f64,
            percentile(&all, 25),
            percentile(&all, 50),
            percentile(&all, 90),
        );
    }
}

/// What the clamping costs, in the only currency that matters: whether the shipped rule can still
/// find a band to cut in.
///
/// Printed twice, split by the release and then by the ink. The second is not a redundant view: one
/// of the three discs this was run against carries no `<i>` in either of its English sidecars, so
/// the release-labelled table has a single column on it and this one has both.
fn refusals(lines: &[Line]) {
    for (title, splits) in [
        ("what the shipped rule does with them", BY_RELEASE),
        ("the same, split by the estimator rather than by the release", BY_INK),
    ] {
        println!("\n  {title}");
        println!(
            "  {:>9} {:>8} {:>9} {:>12} {:>12}",
            "", "lines", "clamped", "no cut", "deskewed"
        );
        for (name, belongs) in splits {
            let members: Vec<&Line> = lines.iter().filter(|l| belongs(l)).collect();
            let (before, _) = both_gaps(&members);
            if before.is_empty() {
                continue;
            }
            let clamped = before.iter().filter(|gap| **gap == 0).count();
            let (was, now) = refusal_rates(&members);
            println!(
                "  {name:>9} {:>8} {:>8.1}% {:>11.1}% {:>11.1}%",
                members.len(),
                clamped as f64 * 100.0 / before.len() as f64,
                was,
                now,
            );
        }
    }
}

/// The slant estimate, and how well it alone would tell the two acts apart.
fn slant(lines: &[Line]) {
    println!("\n--- slant, from the pooled ink moment (#115 candidate C) ---");
    println!(
        "  x' = x - k·y, k the shear that makes the pooled cross term vanish. Negative leans right."
    );
    println!(
        "  {:>9} {:>8} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "", "lines", "unknown", "p10", "p25", "p50", "p75", "p90"
    );
    for (name, italic) in [("upright", false), ("italic", true)] {
        let members: Vec<&Line> = lines.iter().filter(|l| l.italic == italic).collect();
        if members.is_empty() {
            continue;
        }
        let mut known: Vec<f64> = members.iter().filter_map(|l| l.shear).collect();
        known.sort_by(f64::total_cmp);
        if known.is_empty() {
            continue;
        }
        println!(
            "  {name:>9} {:>8} {:>8.1}% {:>9.1} {:>9.1} {:>9.1} {:>9.1} {:>9.1}",
            members.len(),
            (members.len() - known.len()) as f64 * 100.0 / members.len() as f64,
            percentile(&known, 10),
            percentile(&known, 25),
            percentile(&known, 50),
            percentile(&known, 75),
            percentile(&known, 90),
        );
    }

    // Per cue rather than per line, because that is the granularity an `<i>` is written at and
    // because #14 found slant constant within a stream — a cue's lines are all the same act, so
    // pooling them is free evidence rather than an assumption.
    let mut per_cue: BTreeMap<usize, (bool, Vec<f64>)> = BTreeMap::new();
    for line in lines {
        if let Some(shear) = line.shear {
            let slot = per_cue.entry(line.cue).or_insert((line.italic, Vec::new()));
            slot.1.push(shear);
        }
    }
    let cues: Vec<(bool, f64)> = per_cue
        .values()
        .map(|(italic, shears)| (*italic, shears.iter().sum::<f64>() / shears.len() as f64))
        .collect();
    let italic_cues = cues.iter().filter(|(italic, _)| *italic).count();
    println!(
        "\n  as a per-cue detector: {} cues measurable, {italic_cues} of them italic",
        cues.len()
    );
    if italic_cues == 0 || italic_cues == cues.len() {
        println!("  one class only; there is nothing here to separate");
        return;
    }

    // Every threshold the data itself offers, scored, and the best one printed. A cut chosen this
    // way is fitted to this disc and is not a shippable constant — what it reports is the
    // *separability* of the two populations, which is the question a detector has to clear before
    // any threshold is worth choosing. `docs/reference-set.md` records the same distinction.
    let mut candidates: Vec<f64> = cues.iter().map(|(_, shear)| *shear).collect();
    candidates.sort_by(f64::total_cmp);
    let mut best = (0usize, f64::NAN, 0usize, 0usize);
    for cut in &candidates {
        let right = cues
            .iter()
            .filter(|(italic, shear)| *italic == (shear < cut))
            .count();
        if right > best.0 {
            let missed = cues
                .iter()
                .filter(|(italic, shear)| *italic && shear >= cut)
                .count();
            let false_italic = cues
                .iter()
                .filter(|(italic, shear)| !*italic && shear < cut)
                .count();
            best = (right, *cut, missed, false_italic);
        }
    }
    println!(
        "  the best cut this disc offers is {:.1}: {} of {} cues right — {:.1}%",
        best.1,
        best.0,
        cues.len(),
        best.0 as f64 * 100.0 / cues.len() as f64
    );
    println!(
        "  {} italic cues read upright, {} upright cues read italic",
        best.2, best.3
    );

    // Where the misses sat, not just how many. A cue missed at -7 is a threshold question and one
    // missed at 0 is a cue that does not lean — a release marking a whole song or a whole card
    // italic while the disc set it upright, which is the transcript disagreeing rather than the
    // estimator failing. Only the second kind bounds what a detector can ever reach.
    let mut missed: Vec<f64> = cues
        .iter()
        .filter(|(italic, shear)| *italic && *shear >= best.1)
        .map(|(_, shear)| *shear)
        .collect();
    missed.sort_by(f64::total_cmp);
    if let (Some(first), Some(last)) = (missed.first(), missed.last()) {
        println!("  the italic cues read upright sat between {first:.1} and {last:.1}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stem `width` wide and `height` tall, sheared so that `x' = x - k·y` stands it upright.
    ///
    /// With `k` negative the top of the stem sits to the right of its foot, which is what an italic
    /// looks like on a plane whose y grows downward.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn stem(width: u32, height: u32, k: f64) -> BinaryMask {
        let lean = (k.abs() * f64::from(height)).ceil() as u32;
        let mut mask = BinaryMask::blank(width + lean, height);
        for y in 0..height {
            let shift = (k * f64::from(y) + f64::from(lean)).round().max(0.0) as u32;
            for x in 0..width {
                mask.set(x + shift, y, true);
            }
        }
        mask
    }

    /// Lay masks out left to right, one every `pitch` pixels.
    fn laid_out(masks: &[BinaryMask], pitch: u32) -> Vec<(&BinaryMask, Rect)> {
        masks
            .iter()
            .enumerate()
            .map(|(index, mask)| {
                let at = u32::try_from(index).unwrap() * pitch;
                (mask, Rect::new(at, 0, mask.width(), mask.height()))
            })
            .collect()
    }

    #[test]
    fn an_upright_line_reports_no_slant() {
        let masks: Vec<BinaryMask> = (0..5).map(|_| stem(4, 40, 0.0)).collect();
        let shear = shear_of(&laid_out(&masks, 10), masks.len()).expect("enough ink and glyphs");
        assert!(shear.abs() < 1.0, "an upright line leaned by {shear}%");
    }

    #[test]
    fn a_line_leaning_right_reports_a_negative_shear() {
        // The plane's y grows downward, so ink standing further right at the top has a negative
        // covariance cross term. Getting this sign backwards would deskew an italic into a worse
        // italic, and no distance figure downstream would say which had happened.
        let masks: Vec<BinaryMask> = (0..5).map(|_| stem(4, 40, -0.2)).collect();
        let shear = shear_of(&laid_out(&masks, 20), masks.len()).expect("enough ink and glyphs");
        assert!(shear < -10.0, "a right-leaning line reported {shear}%");
    }

    #[test]
    fn the_estimate_does_not_move_when_the_glyphs_are_laid_out_differently() {
        // Each glyph contributes its covariance about its own centroid, so where the letters sit
        // along the line cannot reach the answer. Pooling the raw pixels instead would fail this,
        // and it is the whole reason the estimator is written the way it is.
        let masks: Vec<BinaryMask> = (0..5).map(|_| stem(4, 40, -0.2)).collect();
        let tight = shear_of(&laid_out(&masks, 8), masks.len()).expect("enough ink and glyphs");
        let loose = shear_of(&laid_out(&masks, 400), masks.len()).expect("enough ink and glyphs");
        assert!((tight - loose).abs() < 0.001, "{tight}% against {loose}%");
    }

    #[test]
    fn a_line_with_too_few_glyphs_reports_unknown_rather_than_upright() {
        // The boundary `CLAUDE.md` requires. An unmeasurable line and a line measured as upright
        // are different facts, and only one of them may be written without an italic tag.
        let masks: Vec<BinaryMask> = (0..MIN_GLYPHS - 1).map(|_| stem(4, 60, -0.2)).collect();
        assert_eq!(shear_of(&laid_out(&masks, 10), masks.len()), None);
    }

    #[test]
    fn a_line_with_too_little_ink_reports_unknown_rather_than_upright() {
        let masks: Vec<BinaryMask> = (0..5).map(|_| stem(1, 4, 0.0)).collect();
        assert_eq!(shear_of(&laid_out(&masks, 10), masks.len()), None);
    }

    #[test]
    fn deskewing_recovers_a_gap_the_bounding_boxes_had_swallowed() {
        // The mechanism, in miniature. Two leaning stems set far enough apart to be separate words:
        // their boxes are mostly slant, so the boxes overlap and the runtime's saturating
        // subtraction reports zero — while the ink itself is nowhere near touching.
        let masks: Vec<BinaryMask> = (0..2).map(|_| stem(3, 40, -0.25)).collect();
        let placed = laid_out(&masks, 11);
        let boxed = i64::from(placed[1].1.x) - i64::from(placed[0].1.right());
        assert!(boxed <= 0, "the boxes were meant to overlap; they gapped by {boxed}");

        let gaps = deskewed_gaps(&placed, -0.25);
        assert_eq!(gaps.len(), 1);
        assert!(gaps[0] > 5.0, "the deskewed gap was {}", gaps[0]);
    }

    #[test]
    fn a_zero_shear_leaves_the_gaps_where_the_boxes_had_them() {
        let masks: Vec<BinaryMask> = (0..2).map(|_| stem(3, 40, 0.0)).collect();
        let gaps = deskewed_gaps(&laid_out(&masks, 11), 0.0);
        assert_eq!(gaps, vec![8.0]);
    }

    #[test]
    fn percentiles_are_ordered_and_land_on_the_sample() {
        let sorted: Vec<f64> = (0..=100).map(f64::from).collect();
        assert!((percentile(&sorted, 50) - 50.0).abs() < f64::EPSILON);
        assert!(percentile(&sorted, 10) <= percentile(&sorted, 90));
        assert!((percentile(&sorted, 100) - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn an_empty_sample_has_no_percentile_to_report() {
        assert!((percentile(&[], 50)).abs() < f64::EPSILON);
    }
}
