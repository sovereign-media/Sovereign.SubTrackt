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
//!
//! **What is ranked is #121.** Every rule here is a ranking over the line's gaps, and until #121 a
//! gap was the space between two *bounding boxes*. A slanted letter's box is mostly slant, so it
//! overhangs the box after it and the subtraction saturated: on a real Blu-ray 27% of an italic
//! line's gaps arrived at these rules already at zero, against 0.7% of an upright line's. Every
//! ranking below is only as good as what it ranks, and a run of clamped zeros collapses the
//! letter-gap mode onto the floor and takes the band with it. A gap is now the space between two
//! [`UprightSpan`](subtrackt_core::UprightSpan)s — the same ink, measured along the line's own
//! slant — and `docs/italic-slant.md` has the measurement.

use subtrackt_core::{
    Confidence, Cue, Error, Glyph, GlyphMatch, Result, SPAN_TENTHS, SubtitleImage, TextAssembler,
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
    /// A line where **no** candidate clears both tests gets no spaces, not a guess — the same choice
    /// `LineMetrics::UNKNOWN` makes, and for the same reason. See [`split_threshold`] for why that
    /// is now a statement about the whole ranking rather than about its first entry.
    pub split_min_cluster_percent: u32,
    /// A gap wider than this multiple of the line's median inter-glyph gap, in percent, is a space.
    pub space_gap_percent: u32,
    /// Whether a gap is measured band by band down the line rather than between whole boxes.
    ///
    /// **On since #225**, and the road there is worth the paragraph because the obvious version of
    /// it does not work.
    ///
    /// #219 measured the defect: a box gap understates the space between two letters whenever one
    /// of them is widened by ink at a height the other does not occupy -- 29 points in front of a
    /// `j`, 46 in front of a `T`. Gone Girl fuses `jag` to the word before it 80 times in Swedish
    /// and `jeg` 62 times in Norwegian, against six `you` in its English track.
    ///
    /// #222 turned it on and made **600 cues worse** across the bench. The mechanism was a *third*
    /// population appearing in the line's gap distribution: a full stop has ink in one band only,
    /// the band it shares with the letter before it holds that letter's narrow foot, and the honest
    /// distance there is genuinely wide. `blunder.` became `blunder .` on every disc.
    ///
    /// #225 swept both decisiveness constants against it and **no setting of either works** -- the
    /// floors that suppress the punctuation gaps also refuse real word breaks, and at a width floor
    /// of 80% the Wanda track reads 6.8% CER against 1.3%. What works is
    /// [`Self::band_gap_min_shared`], which removes the population rather than moving a threshold.
    ///
    /// At two shared bands: **66 cues better and 17 worse** across the nine-track bench, character
    /// error down or level on every scored track, and the Scandinavian glued-word counts down from
    /// 80 to 22 and 62 to 19. `docs/word-gap.md` has all of it.
    ///
    /// Falls back to [`UprightSpan`](subtrackt_core::UprightSpan) wherever the bands are unknown --
    /// a line whose anchors were not found, a part cut out of a fused component, a pair that does
    /// not share enough bands. The fallback is not a degraded path; it is what every gap was
    /// measured by before #219.
    pub band_gaps: bool,
    /// How many bands two glyphs must share before [`Self::band_gaps`] answers for them.
    ///
    /// **Two**, and it is a population filter rather than a threshold -- which is why it works
    /// where #225's sweep of the two decisiveness constants did not. A pair that faces the other
    /// over one band only is a full stop or an apostrophe beside a letter, where the single shared
    /// band holds the letter's narrow foot; handing those back to the box measurement removes
    /// **97%** of what #222 cost, from 600 worse cues to 17.
    ///
    /// Not three. `j` shares exactly two bands with the letter before it -- the letter has no ink
    /// below the baseline, which is the whole reason its hook widens the box unopposed -- so a floor
    /// of three gives most of the defect back: Swedish glued `jag` returns from 22 to 71 and
    /// Norwegian `jeg` from 19 to 53, for one further cue on the English bench.
    ///
    /// Read only when [`Self::band_gaps`] is on.
    pub band_gap_min_shared: u32,
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
            band_gaps: true,
            band_gap_min_shared: 2,
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
    fn render_line(&self, line: &[(&Glyph, GlyphMatch)]) -> (String, Vec<Option<GlyphMatch>>) {
        let gaps: Vec<u32> = line
            .windows(2)
            .map(|pair| gap(pair[0].0, pair[1].0, self.rules))
            .collect();
        // The yardstick the decisiveness test measures against. A word space is a sizeable
        // fraction of a character; a kerning gap is not, and no ratio between two *gaps* can tell
        // those apart once rasterisation has quantised them to single pixels.
        //
        // In the same unit as the gaps, and from the same measurement. Both matter. The unit is
        // what keeps the two decisiveness tests ratios rather than pixel counts; the measurement is
        // what keeps them *comparable*, and taking one side from the deskewed span and the other
        // from the box is the mistake #99, #110 and #113 each made once.
        //
        // It is also the truer width. A slanted letter's box is mostly slant — 47 pixels where the
        // ink stands 40 wide, on the first italic line of a real disc — so the box says a word gap
        // must clear a bar that the character never actually set. Deskewed, both sides shrink
        // together and the ratio is the one #40 measured.
        let mut widths: Vec<u32> = line.iter().map(|(g, _)| glyph_width(g)).collect();
        widths.sort_unstable();
        let width = median_of_sorted(&widths);
        // A gap *inside* a repeated punctuation mark is not a candidate word break, and #134 is
        // what it costs to let one pretend to be. An ellipsis puts two gaps between the line's
        // kerning and its word gaps -- a third cluster where the rule assumes two -- and the widest
        // jump then ties across its edges. On a long line the tie broke low, nothing cleared the
        // decisiveness bar, and the whole line came back as `Hewasoneofthosemenwho...`. On a short
        // one the dots *were* the widest jump and their 5px boxes dragged the median glyph width
        // down far enough for the bar to be cleared, so `Otto...` read `Otto. . .`.
        //
        // Both are the same mistake and neither is fixed by moving a threshold. The gaps were never
        // word gaps: three full stops in a row are one token, and the distribution the splitter
        // reasons about should not contain the distances between them.
        let inside_mark = repeated_mark_gaps(line);
        let breaks = word_breaks(&gaps, &inside_mark, width, self.rules);

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
            origins.push(Some(*matched));
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
        let mut italic = Vec::with_capacity(line_count);
        let mut origins = Vec::with_capacity(line_count);

        for line_index in 0..line_count {
            // Borrowed. A `Glyph` carries its feature vector inline, so collecting them by value
            // copied every glyph of the cue once per assembly; nothing here mutates one, and the
            // sort below reorders the references rather than the glyphs.
            let mut members: Vec<(&Glyph, GlyphMatch)> = glyphs
                .iter()
                .zip(matches)
                .filter(|(g, _)| g.line == line_index)
                .map(|(g, m)| (g, *m))
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
                // Every glyph on a line carries its line's slant, so any of them answers — and
                // pushed here rather than collected separately so the flag cannot come adrift of
                // the line it describes. An empty line is dropped above and takes its flag with it.
                italic.push(members.iter().any(|(g, _)| g.slant.leans()));
                origins.push(rendered_origins);
            }
        }

        let cue = Cue {
            span: image.span,
            lines,
            italic,
            confidence,
            forced: image.forced,
        };
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

/// How wide one glyph's ink stands, in tenths of a pixel.
///
/// The deskewed span where the line's slant was measurable and the bounding box where it was not —
/// the same pairing [`gap`] makes, so a line is measured entirely one way or entirely the other.
fn glyph_width(glyph: &Glyph) -> u32 {
    if glyph.upright.known {
        return u32::try_from(glyph.upright.right - glyph.upright.left).unwrap_or(0);
    }
    glyph.bounds.width * SPAN_TENTHS.unsigned_abs()
}

/// The space between two glyphs standing side by side on a line, in tenths of a pixel.
///
/// [`UprightSpan`](subtrackt_core::UprightSpan) where both glyphs have one and the bounding boxes
/// where either does not — which is the same measurement, since a span with no slant to take out
/// *is* the box, and `a_zero_shear_span_is_the_glyph_box` pins the two together. So a line whose
/// slant could not be measured is laid out exactly as it was before #121, rather than being laid
/// out against a shear nothing measured.
///
/// Saturating at zero, still. The signed gap a span can express is a better fact and this is not
/// the place to start using it: every rule below ranks gaps, and a negative gap ranks below a zero
/// one in exactly the way a clamped one does. What #121 changes is how *many* of them are down
/// there, not what happens to the ones that are.
fn gap(this: &Glyph, next: &Glyph, rules: LayoutRules) -> u32 {
    let banded = rules
        .band_gaps
        .then(|| {
            this.bands
                .gap_to_over(next.bands, rules.band_gap_min_shared)
        })
        .flatten();
    let tenths = banded
        .or_else(|| this.upright.gap_to(next.upright))
        .unwrap_or_else(|| {
            let boxes = i64::from(next.bounds.x) - i64::from(this.bounds.right());
            i32::try_from(boxes * i64::from(SPAN_TENTHS)).unwrap_or(i32::MAX)
        });
    u32::try_from(tenths).unwrap_or(0)
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
fn word_breaks(
    gaps: &[u32],
    inside_mark: &[bool],
    glyph_width: u32,
    rules: LayoutRules,
) -> Vec<bool> {
    if rules.spacing == SpacingRule::MedianMultiple {
        let median = median_gap(gaps);
        return gaps
            .iter()
            .zip(inside_mark)
            .map(|(gap, skip)| !skip && is_space(*gap, median, rules))
            .collect();
    }
    // The threshold is derived from the gaps that could *be* word gaps. Leaving the others in
    // moves the cut and, worse, gives the criterion a cluster to find a split in that is not one.
    let candidates: Vec<u32> = gaps
        .iter()
        .zip(inside_mark)
        .filter(|(_, skip)| !**skip)
        .map(|(gap, _)| *gap)
        .collect();
    let threshold = split_threshold(&candidates, glyph_width, rules);
    gaps.iter()
        .zip(inside_mark)
        .map(|(gap, skip)| !skip && threshold.is_some_and(|cut| *gap >= cut))
        .collect()
}

/// Which gaps sit between two glyphs that read as the **same punctuation mark**.
///
/// An ellipsis, a run of dashes, a `''` standing in for a quotation mark. Each is one token whose
/// internal spacing is a property of the typeface rather than of the sentence, so those distances
/// tell a word-splitting rule nothing and mislead it in both directions -- see the call site.
///
/// Alphanumerics are deliberately excluded: `ll` and `oo` are two letters of one word, and their
/// gap is exactly the kerning measurement the rule is built to learn from.
fn repeated_mark_gaps(line: &[(&Glyph, GlyphMatch)]) -> Vec<bool> {
    line.windows(2)
        .map(|pair| match (pair[0].1.character, pair[1].1.character) {
            (Some(left), Some(right)) => {
                left == right && !left.is_alphanumeric() && !left.is_whitespace()
            }
            _ => false,
        })
        .collect()
}

/// The gap at or above which a break is a word break, or `None` when the line's gaps do not
/// separate into two classes decisively enough to say.
///
/// `None` is a real answer and not a failure: a line holding one word has no word gaps, every
/// splitting criterion will still find *a* split in it, and reporting one would insert a space into
/// the middle of a word. Saying nothing leaves the word intact.
///
/// **Every candidate cut is tried, best-separated first, and the first decisive one wins.** #143
/// settled that against the opposite reading, which this function used to state: that the failure
/// of the best candidate is the strongest evidence available that there is nothing on the line.
///
/// It is not, and the reason is that **the bar does not move**. The argument for stopping was that
/// the second-best jump is by construction a worse-separated one, so walking would offer
/// progressively weaker evidence until something cleared the bar. True about separation, and the
/// two decisiveness tests are not about separation — they are absolute ratios, against the line's
/// median glyph width and against the low cluster's median gap. A cut either clears them or it does
/// not. Ranking decides the *order* candidates are tried in; the guards decide *admission*, and
/// they are unchanged. So a less-well-separated cut that still clears both yardsticks is a real
/// word gap that the best-separated cut happened to sit beside.
///
/// #134's line is that case exactly. Its widest jump lands under an ellipsis and clears neither
/// yardstick, and the line came back as `Hewasoneofthosemenwho...` — every one of its six real word
/// gaps thrown away because the *first* candidate was the wrong one. The next candidate is 15, and
/// all six are above it.
///
/// Measured on the seven-track bench: **16 cues better, 2 worse**. Both of the two are Airplane!'s
/// SDH track reading `MAN ON INTERCOM:` where the release sidecar writes `MAN:` — the extraction
/// became *more* correct and scored worse against a sidecar of a different convention, which is the
/// hazard `CLAUDE.md` records against The Prestige. No line anywhere on the bench gained a space it
/// should not have; the improvements are lines that previously came back with no spaces at all.
///
/// **`gaps` and `glyph_width` must be in the same unit**, and the function does not care which one.
/// Both decisiveness tests are ratios between two quantities the caller supplies, so pixels work
/// and so do the tenths of a pixel [`SpatialAssembler`] hands it — which is the resolution #121
/// needs, since a word gap and a kerning gap sit three pixels apart and rounding each edge to a
/// whole pixel would put a third of that on the measurement.
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

    for cut in ranked_cuts(&sorted, rules.spacing) {
        let threshold = sorted[cut + 1];
        let cluster = median_of_sorted(&sorted[..=cut]).max(1);
        if threshold * 100 >= rules.split_min_width_percent * glyph_width
            && threshold * 100 >= rules.split_min_cluster_percent * cluster
        {
            return Some(threshold);
        }
    }
    None
}

/// Every cut worth trying, best separated first, ties going to the **lower** cut.
///
/// Equal neighbouring values never make a cut: a boundary drawn through two identical gaps would
/// put the same measurement on both sides of it.
fn ranked_cuts(sorted: &[u32], rule: SpacingRule) -> Vec<usize> {
    let mut scored: Vec<(f64, usize)> = match rule {
        SpacingRule::OtsuSplit => otsu_scores(sorted),
        _ => jump_scores(sorted),
    };
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, index)| index).collect()
}

/// Each cut scored by the width of the jump across it.
fn jump_scores(sorted: &[u32]) -> Vec<(f64, usize)> {
    (0..sorted.len() - 1)
        .filter(|index| sorted[index + 1] > sorted[*index])
        .map(|index| (f64::from(sorted[index + 1] - sorted[index]), index))
        .collect()
}

/// Each cut scored by Otsu's between-class variance.
fn otsu_scores(sorted: &[u32]) -> Vec<(f64, usize)> {
    let total: f64 = sorted.iter().map(|gap| f64::from(*gap)).sum();
    #[allow(clippy::cast_precision_loss)]
    let count = sorted.len() as f64;
    let mut low_sum = 0.0;
    let mut scored = Vec::new();
    for index in 0..sorted.len() - 1 {
        low_sum += f64::from(sorted[index]);
        if sorted[index] == sorted[index + 1] {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let low_count = (index + 1) as f64;
        let high_count = count - low_count;
        let separation = low_sum / low_count - (total - low_sum) / high_count;
        scored.push((low_count * high_count * separation * separation, index));
    }
    scored
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

    /// The same glyph, with its ink declared band by band rather than as one box.
    ///
    /// Spans are in tenths of a pixel, matching `UprightBands`. `EMPTY_BAND` is a band the glyph
    /// does not reach at all.
    fn banded(x: u32, width: u32, spans: [(i32, i32); subtrackt_core::SPACING_BANDS]) -> Glyph {
        Glyph {
            bands: subtrackt_core::UprightBands::new(spans),
            ..glyph(x, width, 0)
        }
    }

    #[test]
    fn a_gap_is_measured_between_ink_unless_the_boxes_are_asked_for() {
        // Which way round the default sits, pinned. #219 measured the box understating the space in
        // front of a `j` by 29 points; #225 shipped the correction. `docs/word-gap.md` has it.
        let empty = subtrackt_core::UprightBands::EMPTY_BAND;
        let body = [empty, (0, 100), (0, 100), empty];
        let far = [empty, (400, 500), (400, 500), empty];
        let (this, next) = (banded(0, 10, body), banded(20, 10, far));

        let boxes = LayoutRules { band_gaps: false, ..LayoutRules::default() };
        let bands = LayoutRules { band_gaps: true, ..LayoutRules::default() };
        assert_eq!(gap(&this, &next, boxes), 100, "the box gap, in tenths");
        assert_eq!(
            gap(&this, &next, bands),
            300,
            "the band gap, which is three times as wide"
        );
    }

    #[test]
    fn a_pair_with_too_few_shared_bands_falls_back_to_the_box_rather_than_to_nothing() {
        // The apostrophe-beside-a-comma case, and the population #225's floor of two exists for:
        // there is no height at which they face each other over enough of the line to be worth
        // measuring -- and the caller has to answer anyway.
        let empty = subtrackt_core::UprightBands::EMPTY_BAND;
        let high = banded(0, 10, [(0, 100), (0, 100), empty, empty]);
        let low = banded(20, 10, [empty, empty, (200, 300), (200, 300)]);
        let bands = LayoutRules::default();
        assert_eq!(gap(&high, &low, bands), 100, "the box gap, unchanged");
    }

    #[test]
    fn one_shared_band_is_not_enough_because_a_full_stop_is_the_pair_that_has_one() {
        // #225's whole finding, in the small. The letter and the stop face each other only where
        // the letter's foot is, so the honest distance there is wide -- and `split_threshold` reads
        // wide as a word break. Two shared bands hands the pair back to the box, which had it right
        // by accident, and that is 97% of what turning ink gaps on cost.
        let empty = subtrackt_core::UprightBands::EMPTY_BAND;
        let letter = banded(0, 10, [empty, (0, 100), (0, 40), empty]);
        let stop = banded(20, 10, [empty, empty, (300, 340), empty]);

        let one = LayoutRules { band_gap_min_shared: 1, ..LayoutRules::default() };
        assert_eq!(
            gap(&letter, &stop, one),
            260,
            "the honest ink distance, which reads as a space"
        );
        assert_eq!(
            gap(&letter, &stop, LayoutRules::default()),
            100,
            "the box, at the shipped floor"
        );
    }

    fn glyph(x: u32, width: u32, line: usize) -> Glyph {
        Glyph {
            bounds: Rect::new(x, 0, width, 10),
            line,
            features: FeatureVector::EMPTY,
            // Layout works from geometry, not from where a glyph stands in its line, which way its
            // accent leans, or how wide it stands against its cap height.
            metrics: subtrackt_core::LineMetrics::UNKNOWN,
            mark: subtrackt_core::MarkSlope::NONE,
            aspect: subtrackt_core::InkAspect::UNKNOWN,
            upright: subtrackt_core::UprightSpan::of_box(Rect::new(x, 0, width, 10)),
            // Unknown, so every existing spacing test still exercises the box path it was written
            // against. #219's bands are opted into by the tests that are about them.
            bands: subtrackt_core::UprightBands::UNKNOWN,
            slant: subtrackt_core::Slant::UPRIGHT,
        }
    }

    /// Mark every glyph as standing on a leaning line.
    fn leaning_glyphs(glyphs: &mut [Glyph]) {
        for glyph in glyphs {
            glyph.slant = subtrackt_core::Slant::new(-160);
        }
    }

    #[test]
    fn a_leaning_line_comes_back_flagged_and_an_upright_one_does_not() {
        let (mut glyphs, matches) = lay_out("hi", 1);
        leaning_glyphs(&mut glyphs);
        let (mut upright, upright_matches) = lay_out("no", 1);
        for glyph in &mut upright {
            glyph.line = 1;
        }
        glyphs.extend(upright);
        let matches: Vec<GlyphMatch> = matches.into_iter().chain(upright_matches).collect();

        let cue = SpatialAssembler::default()
            .assemble(&image(), &glyphs, &matches)
            .expect("assembles");
        assert_eq!(cue.italic, vec![true, false]);
        assert_eq!(cue.italic.len(), cue.lines.len());
    }

    #[test]
    fn a_flag_is_dropped_with_the_empty_line_it_described() {
        // `lines` and `italic` are parallel and the assembler drops a line that rendered to
        // nothing. A flag left behind would shift every flag after it onto the wrong line.
        let (mut glyphs, mut matches) = lay_out("hi", 1);
        leaning_glyphs(&mut glyphs);
        let (mut blank, blank_matches) = lay_out(" ", 1);
        for glyph in &mut blank {
            glyph.line = 1;
        }
        glyphs.extend(blank);
        matches.extend(blank_matches);

        let cue = SpatialAssembler::default()
            .assemble(&image(), &glyphs, &matches)
            .expect("assembles");
        assert_eq!(cue.lines.len(), cue.italic.len());
        assert_eq!(cue.italic, vec![true]);
    }

    fn matched(c: char) -> GlyphMatch {
        GlyphMatch {
            character: Some(c),
            distance: 1,
            runner_up_distance: 60,
            runner_up: None,
        }
    }

    /// A line of `glyphs` whose boxes all overlap their neighbour's by `overhang` pixels, as a
    /// slanted line's do, and whose deskewed spans sit `gap` apart — except where `text` has a
    /// space, which puts them `word` apart instead.
    ///
    /// The boxes are deliberately hostile: every gap between them is zero or negative, which is the
    /// state a real italic line arrives in and the one the runtime could not previously see past.
    fn leaning(text: &str, overhang: u32, gap: i32, word: i32) -> (Vec<Glyph>, Vec<GlyphMatch>) {
        let (ink, tenths) = (12i32, SPAN_TENTHS);
        let (mut glyphs, mut matches) = (Vec::new(), Vec::new());
        let (mut left, mut box_x) = (0i32, 0u32);
        let mut pending = 0;
        for ch in text.chars() {
            if ch == ' ' {
                pending = word;
                continue;
            }
            left += pending;
            let mut g = glyph(box_x, ink.unsigned_abs() + overhang, 0);
            g.upright = subtrackt_core::UprightSpan::new(left, left + ink * tenths);
            glyphs.push(g);
            matches.push(matched(ch));
            left += ink * tenths;
            box_x += ink.unsigned_abs();
            pending = gap;
        }
        (glyphs, matches)
    }

    #[test]
    fn a_word_break_is_found_on_a_line_whose_boxes_all_overlap() {
        // #121. Every box on this line overhangs the next, so every gap the old measurement could
        // report is zero — the widest jump between them is zero too, and the line comes back as one
        // word. The ink says otherwise and the spans carry it.
        let (glyphs, matches) = leaning("the quick fox", 6, 20, 160);
        for pair in glyphs.windows(2) {
            let boxed = i64::from(pair[1].bounds.x) - i64::from(pair[0].bounds.right());
            assert!(boxed <= 0, "the boxes were meant to overlap; they gapped by {boxed}");
        }
        let cue = SpatialAssembler::default()
            .assemble(&image(), &glyphs, &matches)
            .expect("assembles");
        assert_eq!(cue.lines, vec!["the quick fox"]);
    }

    #[test]
    fn a_line_whose_slant_was_not_measured_is_laid_out_from_its_boxes() {
        // The fallback, and the reason it is safe to leave the whole pipeline switched on: a line
        // the estimator declined to measure is laid out exactly as it was before #121. Here the
        // boxes hold the only usable evidence and the spans hold none.
        let (mut glyphs, matches) = lay_out("the quick fox", 1);
        for glyph in &mut glyphs {
            glyph.upright = subtrackt_core::UprightSpan::UNKNOWN;
        }
        let cue = SpatialAssembler::default()
            .assemble(&image(), &glyphs, &matches)
            .expect("assembles");
        assert_eq!(cue.lines, vec!["the quick fox"]);
    }

    #[test]
    fn a_span_that_says_the_letters_touch_does_not_invent_a_space() {
        // The other direction, which is the one that costs two word errors rather than one. A line
        // holding a single word has no word gap, and every splitting criterion will still find *a*
        // split in it — the decisiveness tests are what stop that becoming a licence to invent, and
        // they must keep working on a measurement expressed in tenths.
        let (glyphs, matches) = leaning("Understood", 6, 20, 20);
        let cue = SpatialAssembler::default()
            .assemble(&image(), &glyphs, &matches)
            .expect("assembles");
        assert_eq!(cue.lines, vec!["Understood"]);
    }

    #[test]
    fn a_deskewed_line_survives_a_resolution_change() {
        // The property no spacing rule here may lose. Both sides of every ratio are now in tenths
        // of a pixel rather than pixels, which changes the unit and must not change the answer.
        let small = leaning("the quick fox", 6, 20, 160);
        let large = leaning("the quick fox", 24, 80, 640);
        let assemble = |(glyphs, matches): &(Vec<Glyph>, Vec<GlyphMatch>)| {
            SpatialAssembler::default()
                .assemble(&image(), glyphs, matches)
                .expect("assembles")
                .lines
        };
        assert_eq!(assemble(&small), assemble(&large));
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

    /// Cue 751 of Airplane!, "He was one of those men who...", as extracted.
    ///
    /// Three clusters rather than two: kerning at 1-7, the ellipsis at 11, and word gaps at 15-22.
    const ELLIPSIS_LINE: [u32; 23] = [
        7, 17, 1, 4, 20, 6, 6, 20, 3, 15, 4, 6, 4, 4, 22, 6, 6, 19, 3, 6, 7, 11, 11,
    ];
    /// The median glyph width of that line, so the decisiveness test has its real yardstick.
    const ELLIPSIS_LINE_WIDTH: u32 = 28;

    #[test]
    fn a_third_cluster_does_not_cost_the_line_every_space_it_had() {
        // #134. The two gaps inside `...` sit between the kerning and the word gaps, so the widest
        // jump ties across that cluster's edges -- 7->11 and 11->15 are both 4. The tie breaks low,
        // the cut lands under the ellipsis, and 11 clears neither yardstick against a 28px glyph.
        //
        // Returning None there threw the whole line away: six word gaps at 15-22, each of which
        // passes both tests outright, came back with nothing between them and the disc read
        // `Hewasoneofthosemenwho...`.
        //
        // The gaps are excluded rather than the threshold moved, because they were never candidate
        // word gaps: three full stops are one token. With them gone the jump is 7->15, an outright
        // winner at 8 rather than a tie at 4.
        let inside_mark: Vec<bool> = ELLIPSIS_LINE
            .iter()
            .enumerate()
            .map(|(index, _)| index >= ELLIPSIS_LINE.len() - 2)
            .collect();

        for spacing in [SpacingRule::WidestSplit, SpacingRule::OtsuSplit] {
            let rules = LayoutRules { spacing, ..LayoutRules::default() };
            let breaks = word_breaks(&ELLIPSIS_LINE, &inside_mark, ELLIPSIS_LINE_WIDTH, rules);
            assert_eq!(
                breaks.iter().filter(|b| **b).count(),
                6,
                "{spacing:?} did not find the six word gaps"
            );
            assert!(
                !breaks[breaks.len() - 1] && !breaks[breaks.len() - 2],
                "{spacing:?} put a space inside the ellipsis"
            );
        }

        // Without the exclusion the line *used* to lose every space, which was the bug as it
        // shipped. It does not any more, and that is #143 rather than a weakening of this test:
        // the candidate walk reaches the same six gaps from the other side. The best cut is still
        // the one under the ellipsis and still fails both yardsticks; the walk then tries the next
        // and finds 15, which is the cut the exclusion arrives at by removing the ellipsis from
        // the distribution instead.
        //
        // Two independent fixes for one failure, which is worth having: the exclusion is right
        // about what the gaps *are* -- three full stops are one token, and their spacing is a
        // property of the typeface -- and the walk is right about what a failed candidate means.
        // Neither makes the other unnecessary, because the exclusion also moves the threshold and
        // the low cluster's median that the guards are measured against.
        let none = vec![false; ELLIPSIS_LINE.len()];
        let breaks =
            word_breaks(&ELLIPSIS_LINE, &none, ELLIPSIS_LINE_WIDTH, LayoutRules::default());
        assert_eq!(
            breaks.iter().filter(|b| **b).count(),
            6,
            "the walk reaches the six word gaps even with the ellipsis left in"
        );
        assert!(
            !breaks[breaks.len() - 1] && !breaks[breaks.len() - 2],
            "and still puts no space inside the ellipsis"
        );
    }

    #[test]
    fn only_a_repeated_mark_is_excluded_and_never_a_repeated_letter() {
        // `ll` and `oo` are two letters of one word and their gap is exactly the kerning the rule
        // is built to learn from. Excluding those would blind it to its own yardstick.
        let glyphs: Vec<(Glyph, GlyphMatch)> =
            [('.', 0), ('.', 10), ('l', 20), ('l', 30), ('.', 40)]
                .iter()
                .map(|(c, x)| (glyph(*x, 4, 0), matched(*c)))
                .collect();
        let line: Vec<(&Glyph, GlyphMatch)> = glyphs.iter().map(|(g, m)| (g, *m)).collect();
        assert_eq!(repeated_mark_gaps(&line), vec![true, false, false, false]);
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
        let matches = vec![GlyphMatch {
            character: Some('0'),
            distance: 8,
            runner_up_distance: 9,
            runner_up: None,
        }];

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
            GlyphMatch {
                character: Some('B'),
                distance: 8,
                runner_up_distance: 9,
                runner_up: None,
            },
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
