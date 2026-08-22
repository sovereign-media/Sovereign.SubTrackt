//! Turning a sequence of matched glyphs back into lines of text.
//!
//! The hard part is spacing. A proportional typeface has no single space width, and the gap inside
//! a kerned pair can be wider than the gap around a real space elsewhere on the same line. So the
//! threshold is derived per line from the gaps actually observed.
//!
//! Deriving it per line rather than fixing it matters twice over: it survives the same title
//! shipping at 480p and 1080p, and it survives one cue being set larger than another. That property
//! is the one thing no spacing rule here may lose, and every rule below is a ranking over the
//! line's own gaps for exactly that reason — nothing is expressed in pixels.
//!
//! *Which* ranking is #40. #11 took the median gap to stand in for "normal letter spacing" and
//! called anything several times wider a word break; #15 later made it scorable, and it finds 21 of
//! the fixture's 29 spaces. See [`SpacingRule`] for why, and for what replaced it.

use subtrackt_core::{
    Confidence, Cue, Error, Glyph, GlyphMatch, Result, SubtitleImage, TextAssembler,
};

/// How a line's word breaks are found.
///
/// Every variant ranks the line's own gaps, so every variant survives a resolution change. They
/// differ in what they do with the ranking, and #40 measured the difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpacingRule {
    /// #11's rule: a gap wider than [`LayoutRules::space_gap_percent`] of the line's median gap.
    ///
    /// It rests on the median estimating *normal letter spacing*, which holds only while letter
    /// gaps are the majority of the line's gaps. Short words break it outright — `- Is it 1 or l?`
    /// has nine gaps of which five are word gaps, so the median **is** a word gap, the threshold
    /// lands above everything present, and the line comes back as one word. Short words are
    /// ordinary in dialogue.
    ///
    /// It also cuts in the wrong place even where the median is sound: a fixed multiple has no
    /// reason to land in the empty band between two separated clusters, and on
    /// `The quick brown fox jumps` it falls at 12 against a word gap of 11.
    ///
    /// Kept because it is the baseline every replacement is scored against, not because it is a
    /// setting worth choosing.
    MedianMultiple,
    /// Cut at the widest jump between consecutive sorted gaps.
    ///
    /// Finds the split rather than assuming a multiple of anything. A line's gaps are bimodal —
    /// letter gaps and word gaps, with an empty band between — and the widest jump is that band.
    /// Ties go to the lower cut, which is the direction that finds a space a higher cut would miss;
    /// the two decisiveness tests on [`LayoutRules`] are what stop that becoming a licence to
    /// invent.
    ///
    /// **What ships**, on the measurement in #40: 23 of 23 scorable spaces against the median
    /// rule's 15, none invented, CER 15.9% to 11.0% and WER 56.8% to 32.4%.
    #[default]
    WidestSplit,
    /// Cut where the two classes separate best, by Otsu's between-class variance.
    ///
    /// The same idea as [`WidestSplit`](Self::WidestSplit) with a criterion that weighs how many
    /// gaps fall either side of the cut and not only how wide the jump is, so a single freak gap is
    /// less able to carry the split on its own.
    ///
    /// Measured **identically** to `WidestSplit` on every fixture line — same spaces, same CER,
    /// same WER. It is kept for the same reason `ClusterRules` keeps its machinery: a second
    /// criterion agreeing to the character is what says the cut is in the data rather than in one
    /// criterion's arithmetic. `WidestSplit` ships because it is the simpler of two equals and
    /// needs no floating point.
    OtsuSplit,
}

/// Rules for reconstructing text from glyph geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutRules {
    /// How word breaks are found.
    pub spacing: SpacingRule,
    /// Fraction of the line's median glyph width a cut must reach to be a word break, in percent.
    ///
    /// The first half of the decisiveness test, and the reason a split rule cannot invent spaces
    /// inside a single word. A widest jump exists on every line, including one holding no word gap
    /// at all, so something has to say whether the split means anything.
    ///
    /// It is measured against a *glyph* rather than against another gap because at subtitle
    /// resolutions no ratio between two gaps can do the job: rasterisation quantises kerning to
    /// one and two pixels, and one against two is the same 2:1 ratio a real word break shows. A
    /// character is the only thing on the line whose size is not that small. Typographically it is
    /// also the right question — a word space is a sizeable fraction of a character and a kerning
    /// gap is not.
    ///
    /// Fifty comes from the fixture: real word gaps run 0.61 to 1.60 of the median glyph width,
    /// and the widest kerning gap on any line reaches 0.5. Both are ratios of two pixel counts
    /// from the same line, so the 480p/1080p property survives.
    pub split_min_width_percent: u32,
    /// Multiple of the low cluster's median gap a cut must reach to be a word break, in percent.
    ///
    /// The second half, and it catches what the first cannot: a line whose gaps are large but
    /// uniform passes a test against glyph width and still holds no word break. A word space runs
    /// to several times the inter-letter gap, and the fixture's tightest real break reaches 2.2.
    ///
    /// Against the low cluster rather than the whole line on purpose. The whole-line median is
    /// precisely what [`SpacingRule::MedianMultiple`] gets wrong on a line of short words; the
    /// median of the gaps *below* the cut asks the same question of a population that word gaps
    /// cannot contaminate.
    ///
    /// A line failing either test gets **no spaces**, not a guess — the same choice
    /// `LineMetrics::UNKNOWN` makes, and for the same reason.
    pub split_min_cluster_percent: u32,
    /// A gap wider than this multiple of the line's median inter-glyph gap, in percent, is a space.
    pub space_gap_percent: u32,
    /// Character substituted for a glyph the matcher could not identify.
    pub placeholder: char,
    /// Whether a leading `-` is followed by a space even when the gap alone would not warrant one.
    ///
    /// A dash opening a line marks a second speaker. Set tight against the following word it reads
    /// as a hyphen, which changes what the line means.
    pub preserve_speaker_dash: bool,
    /// Distance margin below which a match counts as ambiguous, for the confidence tally.
    ///
    /// Should agree with the matcher's own margin; the pipeline wires them from one source.
    pub ambiguity_margin: u32,
}

impl Default for LayoutRules {
    fn default() -> Self {
        Self {
            spacing: SpacingRule::default(),
            split_min_width_percent: 50,
            split_min_cluster_percent: 200,
            space_gap_percent: 250,
            placeholder: '\u{fffd}',
            preserve_speaker_dash: true,
            ambiguity_margin: 8,
        }
    }
}

/// Assembles cues from glyph geometry and their matches.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpatialAssembler {
    rules: LayoutRules,
}

impl SpatialAssembler {
    /// An assembler using the given rules.
    #[must_use]
    pub const fn new(rules: LayoutRules) -> Self {
        Self { rules }
    }

    /// The rules in force.
    #[must_use]
    pub const fn rules(&self) -> LayoutRules {
        self.rules
    }

    /// Build one line of text from the glyphs assigned to it, and say what produced each
    /// character of it.
    fn render_line(&self, line: &[(Glyph, GlyphMatch)]) -> (String, Vec<Option<GlyphMatch>>) {
        let gaps: Vec<u32> = line
            .windows(2)
            .map(|pair| pair[1].0.bounds.x.saturating_sub(pair[0].0.bounds.right()))
            .collect();
        // The yardstick the decisiveness test measures against. A word space is a sizeable
        // fraction of a character; a kerning gap is not, and no ratio between two *gaps* can tell
        // those apart once rasterisation has quantised them to single pixels.
        let mut widths: Vec<u32> = line.iter().map(|(g, _)| g.bounds.width).collect();
        widths.sort_unstable();
        let width = median_of_sorted(&widths);
        let breaks = word_breaks(&gaps, width, self.rules);

        let mut out = String::new();
        let mut origins = Vec::with_capacity(line.len());
        for (index, (_, matched)) in line.iter().enumerate() {
            if index > 0 {
                let opens_with_dash = self.rules.preserve_speaker_dash
                    && index == 1
                    && line[0].1.character == Some('-');

                if opens_with_dash || breaks[index - 1] {
                    out.push(' ');
                    origins.push(None);
                }
            }
            out.push(matched.character.unwrap_or(self.rules.placeholder));
            origins.push(Some(matched.clone()));
        }
        (out, origins)
    }

    /// Assemble a cue and keep the per-character provenance post-correction needs.
    ///
    /// # Errors
    /// Same as [`TextAssembler::assemble`].
    pub fn assemble_annotated(
        &self,
        image: &SubtitleImage,
        glyphs: &[Glyph],
        matches: &[GlyphMatch],
    ) -> Result<AssembledCue> {
        if glyphs.len() != matches.len() {
            return Err(Error::Config(format!(
                "assemble got {} glyphs and {} matches; they must be index-aligned",
                glyphs.len(),
                matches.len()
            )));
        }

        let confidence = matches
            .iter()
            .fold(Confidence::default(), |mut tally, matched| {
                if matched.character.is_some() {
                    tally.matched += 1;
                    if !matched.is_unambiguous(self.rules.ambiguity_margin) {
                        tally.ambiguous += 1;
                    }
                } else {
                    tally.unmatched += 1;
                }
                tally
            });

        let line_count = glyphs.iter().map(|g| g.line).max().map_or(0, |m| m + 1);
        let mut lines = Vec::with_capacity(line_count);
        let mut origins = Vec::with_capacity(line_count);

        for line_index in 0..line_count {
            let mut members: Vec<(Glyph, GlyphMatch)> = glyphs
                .iter()
                .zip(matches)
                .filter(|(g, _)| g.line == line_index)
                .map(|(g, m)| (g.clone(), m.clone()))
                .collect();
            if members.is_empty() {
                continue;
            }
            // Reading order is the assembler's responsibility; upstream ordering is not a
            // guarantee it should depend on.
            members.sort_by_key(|(g, _)| g.bounds.x);

            let (rendered, rendered_origins) = self.render_line(&members);
            if !rendered.trim().is_empty() {
                lines.push(rendered);
                origins.push(rendered_origins);
            }
        }

        let cue = Cue { span: image.span, lines, confidence, forced: image.forced };
        Ok(AssembledCue { cue, origins })
    }
}

/// A cue together with what produced each of its characters.
///
/// Post-correction has to know which characters came from a glyph the matcher could not call
/// outright, and only the assembler knows: by the time a [`Cue`] exists its characters have been
/// sorted into reading order, split across lines, and had spaces inserted between them. Handing a
/// corrector the cue and the match list separately would make it re-derive that mapping, and a
/// corrector working from a *guess* about which glyph produced which character is precisely the
/// thing this stage must not be.
#[derive(Debug, Clone, PartialEq)]
pub struct AssembledCue {
    /// The cue.
    pub cue: Cue,
    /// One entry per line of [`Cue::lines`], and within it one entry per `char` of that line: the
    /// match that produced the character, or `None` for a space the assembler inserted.
    pub origins: Vec<Vec<Option<GlyphMatch>>>,
}

/// Median of the observed gaps, or zero when there are none to measure.
fn median_gap(gaps: &[u32]) -> u32 {
    if gaps.is_empty() {
        return 0;
    }
    let mut sorted = gaps.to_vec();
    sorted.sort_unstable();
    median_of_sorted(&sorted)
}

/// Median of an already-sorted slice, or zero when it is empty.
fn median_of_sorted(sorted: &[u32]) -> u32 {
    sorted.get(sorted.len() / 2).copied().unwrap_or(0)
}

/// Which of a line's gaps are word breaks.
///
/// Computed for the whole line at once because every rule but #11's needs the distribution rather
/// than one gap at a time — that is the substance of #40.
fn word_breaks(gaps: &[u32], glyph_width: u32, rules: LayoutRules) -> Vec<bool> {
    if rules.spacing == SpacingRule::MedianMultiple {
        let median = median_gap(gaps);
        return gaps
            .iter()
            .map(|gap| is_space(*gap, median, rules))
            .collect();
    }
    let threshold = split_threshold(gaps, glyph_width, rules);
    gaps.iter()
        .map(|gap| threshold.is_some_and(|cut| *gap >= cut))
        .collect()
}

/// The gap at or above which a break is a word break, or `None` when the line's gaps do not
/// separate into two classes decisively enough to say.
///
/// `None` is a real answer and not a failure: a line holding one word has no word gaps, every
/// splitting criterion will still find *a* split in it, and reporting one would insert a space into
/// the middle of a word. Saying nothing leaves the word intact.
///
/// # Panics
/// Does not. The cut index returned by either criterion is always less than `sorted.len() - 1`, so
/// both the indexing and the slice below are in bounds.
#[must_use]
pub fn split_threshold(gaps: &[u32], glyph_width: u32, rules: LayoutRules) -> Option<u32> {
    if gaps.len() < 2 || glyph_width == 0 {
        return None;
    }
    let mut sorted = gaps.to_vec();
    sorted.sort_unstable();

    let cut = match rules.spacing {
        SpacingRule::OtsuSplit => otsu_cut(&sorted),
        _ => widest_jump_cut(&sorted),
    }?;
    let threshold = sorted[cut + 1];

    // Both decisiveness tests, against two different yardsticks. Either one alone admits a split
    // that is not there; see the field documentation for which case each of them catches.
    let cluster = median_of_sorted(&sorted[..=cut]).max(1);
    let decisive = threshold * 100 >= rules.split_min_width_percent * glyph_width
        && threshold * 100 >= rules.split_min_cluster_percent * cluster;
    decisive.then_some(threshold)
}

/// Index of the gap below the widest jump between consecutive sorted gaps.
///
/// Strictly-greater comparison, so a tie goes to the **lower** cut. That is deliberate: on
/// `The quick brown fox jumps` the jumps 6 to 11 and 11 to 16 are both five wide, and only the
/// lower of them puts `fox jumps` on the far side of the boundary.
fn widest_jump_cut(sorted: &[u32]) -> Option<usize> {
    let mut best = None;
    let mut widest = 0;
    for index in 0..sorted.len() - 1 {
        let jump = sorted[index + 1] - sorted[index];
        if jump > widest {
            widest = jump;
            best = Some(index);
        }
    }
    best
}

/// Index of the gap below the cut that maximises Otsu's between-class variance.
///
/// Equal neighbouring values are skipped as cut points, since a class boundary drawn through
/// identical gaps would put the same measurement on both sides of it.
// Gap widths are pixel counts on a subtitle plane and their sums stay far inside the range f64
// represents exactly, so the precision-loss lint has nothing to warn about here.
#[allow(clippy::cast_precision_loss)]
fn otsu_cut(sorted: &[u32]) -> Option<usize> {
    let total: f64 = sorted.iter().map(|gap| f64::from(*gap)).sum();
    let count = sorted.len() as f64;

    let (mut best, mut best_score) = (None, 0.0);
    let mut low_sum = 0.0;
    for index in 0..sorted.len() - 1 {
        low_sum += f64::from(sorted[index]);
        if sorted[index] == sorted[index + 1] {
            continue;
        }
        let low_count = (index + 1) as f64;
        let high_count = count - low_count;
        let separation = low_sum / low_count - (total - low_sum) / high_count;
        let score = low_count * high_count * separation * separation;
        if score > best_score {
            best_score = score;
            best = Some(index);
        }
    }
    best
}

impl TextAssembler for SpatialAssembler {
    fn assemble(
        &self,
        image: &SubtitleImage,
        glyphs: &[Glyph],
        matches: &[GlyphMatch],
    ) -> Result<Cue> {
        self.assemble_annotated(image, glyphs, matches)
            .map(|assembled| assembled.cue)
    }
}

/// Whether a gap is wide enough to be a word break.
///
/// Split out because it is the single number #11 turns on, and it should be testable without a
/// full image.
#[must_use]
pub fn is_space(gap: u32, median_gap: u32, rules: LayoutRules) -> bool {
    if median_gap == 0 {
        return false;
    }
    gap * 100 / median_gap >= rules.space_gap_percent
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::{FeatureVector, IndexedBitmap, Palette, Rect, TimeSpan, Timestamp};

    fn image() -> SubtitleImage {
        SubtitleImage {
            span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(1_000)),
            position: Rect::new(0, 0, 2, 2),
            bitmap: IndexedBitmap::blank(2, 2),
            palette: Palette::transparent(2),
            forced: false,
        }
    }

    fn glyph(x: u32, width: u32, line: usize) -> Glyph {
        Glyph {
            bounds: Rect::new(x, 0, width, 10),
            line,
            features: FeatureVector::EMPTY,
            // Layout works from geometry, not from where a glyph stands in its line.
            metrics: subtrackt_core::LineMetrics::UNKNOWN,
        }
    }

    fn matched(c: char) -> GlyphMatch {
        GlyphMatch { character: Some(c), distance: 1, runner_up_distance: 60 }
    }

    /// Lay out `text` on one line, with `scale` pixels per unit so the same fixture can be run at
    /// two resolutions. A space in `text` becomes a wide gap; everything else a kerned gap.
    fn lay_out(text: &str, scale: u32) -> (Vec<Glyph>, Vec<GlyphMatch>) {
        let (mut glyphs, mut matches) = (Vec::new(), Vec::new());
        let mut x = 0;
        for ch in text.chars() {
            if ch == ' ' {
                x += 5 * scale; // a word gap
                continue;
            }
            glyphs.push(glyph(x, 6 * scale, 0));
            matches.push(matched(ch));
            x += 7 * scale; // glyph width plus one unit of kerning
        }
        (glyphs, matches)
    }

    fn assemble(glyphs: &[Glyph], matches: &[GlyphMatch]) -> Cue {
        SpatialAssembler::default()
            .assemble(&image(), glyphs, matches)
            .unwrap()
    }

    #[test]
    fn a_kerned_gap_is_not_a_space_but_a_word_gap_is() {
        let rules = LayoutRules::default();
        assert!(!is_space(2, 3, rules), "a tight kerned pair must not become a space");
        assert!(is_space(9, 3, rules), "a word gap must");
    }

    #[test]
    fn a_line_with_no_measurable_gaps_inserts_no_spaces() {
        // One glyph on a line means no median to compare against; guessing here would produce
        // spurious spaces in short cues.
        assert!(!is_space(50, 0, LayoutRules::default()));
    }

    /// The gaps `- Is it 1 or l?` actually produces, from the table in #40, and the median glyph
    /// width of that line. Real numbers rather than invented ones, so a test that passes here says
    /// something about the fixture rather than about the arithmetic.
    const SHORT_WORDS: [u32; 9] = [4, 4, 5, 6, 15, 17, 17, 18, 21];
    const SHORT_WORDS_WIDTH: u32 = 11;

    /// The gaps `The quick brown fox jumps` actually produces. Two jumps of five, 6 to 11 and
    /// 11 to 16, which is the tie the cut has to resolve downwards.
    const TIED_JUMPS: [u32; 20] = [
        1, 2, 2, 2, 3, 3, 4, 4, 5, 5, 5, 6, 6, 6, 6, 6, 11, 16, 16, 16,
    ];
    const TIED_JUMPS_WIDTH: u32 = 18;

    fn split(gaps: &[u32], width: u32, spacing: SpacingRule) -> Option<u32> {
        split_threshold(gaps, width, LayoutRules { spacing, ..LayoutRules::default() })
    }

    #[test]
    fn a_line_whose_gaps_do_not_separate_gets_no_spaces_rather_than_an_invented_one() {
        // Every splitting criterion finds *a* split, including in a line holding one word, so the
        // decisiveness test is the only thing standing between this rule and a space in the middle
        // of a word. These are one word's kerning gaps: varied, but nowhere near a word break.
        for spacing in [SpacingRule::WidestSplit, SpacingRule::OtsuSplit] {
            // Kerning quantised to one and two pixels. The 2:1 ratio here is the same ratio a real
            // word break shows, which is why glyph width has to be the yardstick.
            assert_eq!(split(&[1, 2, 1, 2, 3, 2, 1], 18, spacing), None);
            // Large but uniform: passes against glyph width, and there is still no break in it.
            assert_eq!(split(&[9, 9, 9, 10, 10, 11], 18, spacing), None, "uniformly loose");
            assert_eq!(split(&[0, 0, 1, 1], 19, spacing), None, "glyphs that nearly touch");
            assert_eq!(split(&SHORT_WORDS, 0, spacing), None, "no width to measure against");
        }
    }

    #[test]
    fn a_line_of_short_words_finds_the_spaces_the_median_rule_cannot() {
        // Five of these nine gaps are word gaps, so the median *is* one and #11's threshold lands
        // above everything on the line. This is the failure #40 exists for.
        let median = median_gap(&SHORT_WORDS);
        assert_eq!(median, 15, "the median is itself a word gap");
        assert!(
            !SHORT_WORDS
                .iter()
                .any(|gap| is_space(*gap, median, LayoutRules::default())),
            "the median rule finds nothing at all on this line"
        );

        for spacing in [SpacingRule::WidestSplit, SpacingRule::OtsuSplit] {
            assert_eq!(split(&SHORT_WORDS, SHORT_WORDS_WIDTH, spacing), Some(15));
            let found = SHORT_WORDS.iter().filter(|gap| **gap >= 15).count();
            assert_eq!(found, 5, "all five word gaps, and none of the letter gaps");
        }
    }

    #[test]
    fn a_tie_between_two_jumps_cuts_at_the_lower_one() {
        // 6 to 11 and 11 to 16 are both five wide. Cutting high loses the space in `fox jumps`,
        // and there is no evidence on the line for preferring it.
        assert_eq!(split(&TIED_JUMPS, TIED_JUMPS_WIDTH, SpacingRule::WidestSplit), Some(11));
        assert_eq!(
            TIED_JUMPS.iter().filter(|gap| **gap >= 11).count(),
            4,
            "four word gaps on a five-word line"
        );
    }

    #[test]
    fn the_two_split_criteria_agree_on_the_lines_that_motivated_them() {
        // They are independent arithmetic over the same ranking. Agreement is what says the cut is
        // in the data; if this ever diverges, one of them is fitting noise.
        for (gaps, width) in [
            (SHORT_WORDS.as_slice(), SHORT_WORDS_WIDTH),
            (TIED_JUMPS.as_slice(), TIED_JUMPS_WIDTH),
        ] {
            assert_eq!(
                split(gaps, width, SpacingRule::WidestSplit),
                split(gaps, width, SpacingRule::OtsuSplit)
            );
        }
    }

    #[test]
    fn a_split_is_a_ranking_and_so_survives_a_change_of_resolution() {
        // The property #11 was built for and #40 must not lose: scaling every gap must not move
        // the decision, because the same title ships at 480p and 1080p.
        for spacing in [SpacingRule::WidestSplit, SpacingRule::OtsuSplit] {
            for scale in [2, 3, 7] {
                let scaled: Vec<u32> = SHORT_WORDS.iter().map(|gap| gap * scale).collect();
                assert_eq!(split(&scaled, SHORT_WORDS_WIDTH * scale, spacing), Some(15 * scale));
                let tight: Vec<u32> = [1, 2, 1, 2, 3].iter().map(|gap| gap * scale).collect();
                assert_eq!(split(&tight, 18 * scale, spacing), None, "and no split stays no split");
            }
        }
    }

    #[test]
    fn word_gaps_become_spaces_and_kerning_does_not() {
        let (glyphs, matches) = lay_out("HELLO THERE", 1);
        assert_eq!(assemble(&glyphs, &matches).text(), "HELLO THERE");
    }

    #[test]
    fn the_same_line_reads_the_same_at_two_resolutions() {
        // The property the per-line median exists for. A fixed pixel threshold tuned at 1080p
        // would run every word together at 480p.
        for scale in [1, 2, 3, 5] {
            let (glyphs, matches) = lay_out("HELLO THERE", scale);
            assert_eq!(
                assemble(&glyphs, &matches).text(),
                "HELLO THERE",
                "scale {scale} changed the spacing"
            );
        }
    }

    #[test]
    fn a_single_glyph_line_produces_that_glyph() {
        let cue = assemble(&[glyph(0, 6, 0)], &[matched('A')]);
        assert_eq!(cue.text(), "A");
    }

    #[test]
    fn output_carries_no_doubled_or_trailing_spaces() {
        let (glyphs, matches) = lay_out("A B  C", 2);
        let text = assemble(&glyphs, &matches).text();
        assert!(!text.contains("  "), "doubled space in {text:?}");
        assert_eq!(text.trim_end(), text, "trailing space in {text:?}");
        assert_eq!(text.trim_start(), text, "leading space in {text:?}");
    }

    #[test]
    fn a_speaker_dash_keeps_its_space_from_the_following_word() {
        // Set tight, a leading dash reads as a hyphen and changes what the line means.
        let mut glyphs = vec![glyph(0, 4, 0)];
        let mut matches = vec![matched('-')];
        for (index, ch) in "Yes".chars().enumerate() {
            let index = u32::try_from(index).unwrap();
            glyphs.push(glyph(6 + index * 7, 6, 0));
            matches.push(matched(ch));
        }

        assert_eq!(assemble(&glyphs, &matches).text(), "- Yes");
    }

    #[test]
    fn a_dash_mid_line_is_left_alone() {
        let mut glyphs = vec![glyph(0, 6, 0)];
        let mut matches = vec![matched('A')];
        glyphs.push(glyph(7, 4, 0));
        matches.push(matched('-'));
        glyphs.push(glyph(12, 6, 0));
        matches.push(matched('B'));

        assert_eq!(
            assemble(&glyphs, &matches).text(),
            "A-B",
            "only a leading dash is a speaker"
        );
    }

    #[test]
    fn two_lines_come_back_as_two_lines_in_order() {
        let mut glyphs = Vec::new();
        let mut matches = Vec::new();
        for (index, ch) in "AB".chars().enumerate() {
            glyphs.push(glyph(u32::try_from(index).unwrap() * 7, 6, 0));
            matches.push(matched(ch));
        }
        for (index, ch) in "CD".chars().enumerate() {
            glyphs.push(glyph(u32::try_from(index).unwrap() * 7, 6, 1));
            matches.push(matched(ch));
        }

        let cue = assemble(&glyphs, &matches);
        assert_eq!(cue.lines, vec!["AB".to_owned(), "CD".to_owned()]);
        assert_eq!(cue.text(), "AB\nCD");
    }

    #[test]
    fn glyphs_are_ordered_by_position_not_by_arrival() {
        // Reading order is this stage's job; depending on upstream ordering would be a silent
        // coupling that breaks the first time a stage reorders anything.
        let glyphs = vec![glyph(14, 6, 0), glyph(0, 6, 0), glyph(7, 6, 0)];
        let matches = vec![matched('C'), matched('A'), matched('B')];
        assert_eq!(assemble(&glyphs, &matches).text(), "ABC");
    }

    #[test]
    fn an_unmatched_glyph_becomes_the_placeholder_and_is_counted() {
        let glyphs = vec![glyph(0, 6, 0), glyph(7, 6, 0)];
        let matches = vec![matched('A'), GlyphMatch::unmatched(200)];

        let cue = assemble(&glyphs, &matches);
        assert_eq!(cue.text(), "A\u{fffd}");
        assert_eq!(cue.confidence.matched, 1);
        assert_eq!(cue.confidence.unmatched, 1);
        assert!(!cue.confidence.is_complete(), "the gate has to be able to see this");
    }

    #[test]
    fn a_close_runner_up_is_tallied_as_ambiguous_without_changing_the_text() {
        let glyphs = vec![glyph(0, 6, 0)];
        let matches = vec![GlyphMatch { character: Some('0'), distance: 8, runner_up_distance: 9 }];

        let cue = assemble(&glyphs, &matches);
        assert_eq!(cue.text(), "0", "an ambiguous read is still the matcher's answer");
        assert_eq!(cue.confidence.matched, 1);
        assert_eq!(cue.confidence.ambiguous, 1, "but post-correction needs to know");
    }

    #[test]
    fn a_cue_with_no_glyphs_has_no_lines() {
        let cue = assemble(&[], &[]);
        assert!(cue.lines.is_empty());
        assert!(cue.is_empty());
    }

    #[test]
    fn the_forced_flag_and_timing_come_from_the_image() {
        let mut img = image();
        img.forced = true;
        let cue = SpatialAssembler::default()
            .assemble(&img, &[glyph(0, 6, 0)], &[matched('A')])
            .unwrap();

        assert!(cue.forced);
        assert_eq!(cue.span, img.span);
    }

    #[test]
    fn every_character_of_an_assembled_line_says_which_glyph_produced_it() {
        // What post-correction stands on. A corrector that had to re-derive this mapping would be
        // guessing which glyph produced which character, and a guess is what this stage may not be.
        let (glyphs, matches) = lay_out("AB CD", 1);
        let assembled = SpatialAssembler::default()
            .assemble_annotated(&image(), &glyphs, &matches)
            .unwrap();

        assert_eq!(assembled.cue.lines, vec!["AB CD".to_owned()]);
        assert_eq!(assembled.origins.len(), 1);
        assert_eq!(
            assembled.origins[0].len(),
            assembled.cue.lines[0].chars().count(),
            "one entry per character, spaces included"
        );

        let characters: Vec<Option<char>> = assembled.origins[0]
            .iter()
            .map(|origin| origin.as_ref().and_then(|m| m.character))
            .collect();
        assert_eq!(
            characters,
            vec![Some('A'), Some('B'), None, Some('C'), Some('D')],
            "the inserted space came from no glyph and has to say so"
        );
    }

    #[test]
    fn provenance_follows_the_reading_order_the_assembler_imposed() {
        // Origins are aligned with the *rendered* line, not with the order the glyphs arrived in,
        // which is the whole reason the assembler is the one producing them.
        let glyphs = vec![glyph(14, 6, 0), glyph(0, 6, 0), glyph(7, 6, 0)];
        let matches = vec![
            matched('C'),
            matched('A'),
            GlyphMatch { character: Some('B'), distance: 8, runner_up_distance: 9 },
        ];

        let assembled = SpatialAssembler::default()
            .assemble_annotated(&image(), &glyphs, &matches)
            .unwrap();
        assert_eq!(assembled.cue.lines, vec!["ABC".to_owned()]);

        let ambiguous: Vec<bool> = assembled.origins[0]
            .iter()
            .map(|origin| {
                origin
                    .as_ref()
                    .is_some_and(|m| !m.is_unambiguous(LayoutRules::default().ambiguity_margin))
            })
            .collect();
        assert_eq!(
            ambiguous,
            vec![false, true, false],
            "the close call is the middle character"
        );
    }

    #[test]
    fn mismatched_glyph_and_match_slices_are_a_configuration_error_not_a_panic() {
        let err = SpatialAssembler::default()
            .assemble(&image(), &[glyph(0, 6, 0)], &[])
            .unwrap_err();
        assert!(matches!(err, Error::Config(_)), "got {err:?}");
    }
}
