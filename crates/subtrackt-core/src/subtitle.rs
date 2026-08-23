//! What flows between the decode stage and the output stage.

use crate::bitmap::{IndexedBitmap, Palette, Rect};
use crate::time::TimeSpan;

/// One decoded bitmap subtitle: pixels, palette, placement and timing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleImage {
    /// When the subtitle is on screen.
    pub span: TimeSpan,
    /// Placement within the subtitle plane.
    pub position: Rect,
    /// Palette-indexed pixels.
    pub bitmap: IndexedBitmap,
    /// The palette in force for this image.
    pub palette: Palette,
    /// Whether the source stream marked this image as forced (foreign-dialogue) subtitles,
    /// which callers usually want to route to a separate output track.
    pub forced: bool,
}

/// How much of a cue was read cleanly.
///
/// Unlike a general OCR engine's per-character probability, this is a count of glyphs the matcher
/// *could not identify at all*. That distinction is the point: it is checkable, not estimated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Confidence {
    /// Glyphs matched within threshold.
    pub matched: u32,
    /// Glyphs with no reference within threshold.
    pub unmatched: u32,
    /// Glyphs matched, but with a runner-up too close to call outright.
    pub ambiguous: u32,
}

impl Confidence {
    /// Total glyphs considered.
    #[must_use]
    pub const fn total(self) -> u32 {
        self.matched + self.unmatched
    }

    /// Whether every glyph in the cue was identified.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.unmatched == 0
    }

    /// Fraction of glyphs identified, in `0.0..=1.0`. An empty cue counts as fully read.
    ///
    /// **Computed in `f64` because the operands are `u32` and both are routinely larger than a
    /// `u16`.** This used to narrow with `u16::try_from(..).unwrap_or(u16::MAX)`, which saturates:
    /// a feature film with 66,438 glyphs divided 65,535 by 65,535 and reported **100.0% read while
    /// 68 glyphs were unmatched**. Worse than the display, this is the number
    /// [`UnmatchedPolicy::Threshold`](../../subtrackt/config/enum.UnmatchedPolicy.html) gates on,
    /// so a 100,000-glyph track that was 40% unread computed 91.6% and was *accepted* against a
    /// floor of 90%. The gate this project's whole thesis rests on was disabled on long tracks.
    ///
    /// The narrowing was there to satisfy `clippy::cast_precision_loss`, since `f32::from(u16)` is
    /// lossless where `u32 as f32` is not. `f64::from(u32)` is lossless too and needs no cast at
    /// all, so the lint is satisfied by being *right* rather than by rounding the inputs. Only the
    /// quotient narrows to `f32`, and a ratio in `0.0..=1.0` has precision to spare there.
    #[must_use]
    pub fn ratio(self) -> f32 {
        if self.total() == 0 {
            return 1.0;
        }
        #[allow(clippy::cast_possible_truncation)]
        {
            (f64::from(self.matched) / f64::from(self.total())) as f32
        }
    }

    /// Combine two tallies, for rolling a track-level total.
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        Self {
            matched: self.matched + other.matched,
            unmatched: self.unmatched + other.unmatched,
            ambiguous: self.ambiguous + other.ambiguous,
        }
    }
}

/// One timed block of text, ready to be written out.
#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    /// When the cue is on screen.
    pub span: TimeSpan,
    /// The cue's text, one entry per rendered line.
    pub lines: Vec<String>,
    /// Which of those lines were set in a leaning face, one entry per entry of [`Self::lines`].
    ///
    /// **A flag rather than markup inside the string**, and the reason is arithmetic: post
    /// correction, [`Self::text`], [`Self::is_empty`] and `xtask srt-score` all read those strings,
    /// and every one of them would have to learn to strip an `<i>` it did not put there. That is
    /// four places that can silently disagree against one field that cannot. Only the writers turn
    /// this into markup, which is where a format's own spelling belongs.
    ///
    /// A line that could not be measured is `false` and is written untagged — the same answer as a
    /// line measured upright, because they are the same answer to the only question the output
    /// asks. [`Slant`](crate::Slant) has the rest of that argument.
    ///
    /// Shorter than [`Self::lines`] only in a cue nothing measured, which
    /// [`Self::line_is_italic`] treats as upright rather than as a panic.
    pub italic: Vec<bool>,
    /// How completely the cue was read.
    pub confidence: Confidence,
    /// Carried through from [`SubtitleImage::forced`].
    pub forced: bool,
}

impl Cue {
    /// The cue's text with lines joined by `\n`.
    #[must_use]
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Whether the cue has no renderable text.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|line| line.trim().is_empty())
    }

    /// Whether line `index` was set in a leaning face.
    ///
    /// `false` for a line nothing measured, which is the same thing the output does with it. Reading
    /// past the end is that case rather than a panic: a `Cue` built before this field existed, or by
    /// a test, carries no flags and is upright text as far as any writer is concerned.
    #[must_use]
    pub fn line_is_italic(&self, index: usize) -> bool {
        self.italic.get(index).copied().unwrap_or(false)
    }

    /// Whether every line the cue renders leans.
    ///
    /// The granularity a release subtitle marks, and what a per-cue comparison is scored against.
    #[must_use]
    pub fn is_italic(&self) -> bool {
        !self.lines.is_empty()
            && self
                .lines
                .iter()
                .enumerate()
                .all(|(index, line)| line.trim().is_empty() || self.line_is_italic(index))
    }
}

/// A whole extracted track.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextTrack {
    /// Cues in presentation order.
    pub cues: Vec<Cue>,
    /// BCP 47 language tag, when the source stream declared one.
    pub language: Option<String>,
    /// Track-level tally, the sum of every cue's [`Confidence`].
    pub confidence: Confidence,
}

impl TextTrack {
    /// Build a track from cues, recomputing the track-level tally.
    #[must_use]
    pub fn new(cues: Vec<Cue>, language: Option<String>) -> Self {
        let confidence = cues
            .iter()
            .fold(Confidence::default(), |acc, c| acc.merge(c.confidence));
        Self { cues, language, confidence }
    }

    /// Whether the track has no cues.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }
}

/// Output formats the writer stage can emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubtitleFormat {
    /// `SubRip`.
    #[default]
    Srt,
    /// `WebVTT`, the format Sovereign serves as a text rendition.
    Vtt,
}

impl SubtitleFormat {
    /// The conventional file extension, without a leading dot.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Vtt => "vtt",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::Timestamp;

    fn spoken(lines: &[&str], italic: &[bool]) -> Cue {
        Cue {
            span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(1_000)),
            lines: lines.iter().map(|l| (*l).to_owned()).collect(),
            italic: italic.to_vec(),
            confidence: Confidence::default(),
            forced: false,
        }
    }

    #[test]
    fn a_feature_length_track_reports_the_ratio_it_actually_read() {
        // Gone Girl, as extracted: 66,370 matched and 68 unmatched. Both counts are larger than a
        // `u16`, and the old narrowing saturated both to 65,535 and reported a clean 100%.
        let track = Confidence { matched: 66_370, unmatched: 68, ambiguous: 7_046 };
        assert!((track.ratio() - 0.998_977).abs() < 1e-5, "{}", track.ratio());
        assert!(track.ratio() < 1.0, "68 unmatched glyphs is not a complete read");
    }

    #[test]
    fn a_track_the_gate_should_refuse_is_not_rounded_into_passing() {
        // The failure that matters. Saturating both operands turned 60% into 91.6%, which clears
        // the 0.90 floor -- so a track that was 40% unread would have been accepted and shipped
        // instead of routed to burn-in.
        let bad = Confidence { matched: 60_000, unmatched: 40_000, ambiguous: 0 };
        assert!((bad.ratio() - 0.6).abs() < 1e-6, "{}", bad.ratio());
        assert!(bad.ratio() < 0.90, "the floor has to be able to see this");
    }

    #[test]
    fn the_ratio_is_monotonic_across_the_old_saturation_point() {
        // Reading *more* of a track can never lower its ratio. The narrowing broke this: past
        // 65,535 the numerator stopped growing while the denominator did, and then both stopped.
        let mut previous = 0.0;
        for unmatched in [0_u32, 1, 100, 10_000, 60_000, 100_000] {
            let ratio =
                Confidence { matched: 200_000 - unmatched, unmatched, ambiguous: 0 }.ratio();
            assert!(ratio <= 1.0, "a ratio cannot exceed one: {ratio}");
            if previous > 0.0 {
                assert!(
                    ratio < previous,
                    "more unmatched must read lower: {ratio} vs {previous}"
                );
            }
            previous = ratio;
        }
    }

    #[test]
    fn a_cue_carrying_no_flags_is_upright_rather_than_a_panic() {
        // A `Cue` built before this field existed, or by a test, is ordinary upright text. Reading
        // past the end has to be that answer and not an index panic, because a writer walks every
        // line of every cue.
        let cue = spoken(&["Hello there."], &[]);
        assert!(!cue.line_is_italic(0));
        assert!(!cue.line_is_italic(9));
        assert!(!cue.is_italic());
    }

    #[test]
    fn a_flag_belongs_to_the_line_at_its_own_index() {
        let cue = spoken(&["upright", "leaning"], &[false, true]);
        assert!(!cue.line_is_italic(0));
        assert!(cue.line_is_italic(1));
    }

    #[test]
    fn a_cue_is_italic_only_when_every_line_it_renders_leans() {
        // The granularity a release subtitle marks, so it is what a per-cue comparison is scored
        // against -- and a cue with one leaning line and one upright one is neither, which is a
        // distinction the per-line flags keep and this answer deliberately loses.
        assert!(spoken(&["a", "b"], &[true, true]).is_italic());
        assert!(!spoken(&["a", "b"], &[true, false]).is_italic());
        assert!(!spoken(&[], &[]).is_italic(), "an empty cue leans no way at all");
    }

    #[test]
    fn a_blank_line_does_not_decide_whether_a_cue_leans() {
        // Otherwise a stray empty line would make an italic cue upright, and nothing about a line
        // with no ink is evidence about the face the rest of it was set in.
        assert!(spoken(&["leaning", "  "], &[true, false]).is_italic());
    }
    fn cue(matched: u32, unmatched: u32) -> Cue {
        Cue {
            span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(1_000)),
            lines: vec!["hello".into()],
            italic: Vec::new(),
            confidence: Confidence { matched, unmatched, ambiguous: 0 },
            forced: false,
        }
    }

    #[test]
    fn an_empty_cue_counts_as_fully_read() {
        assert!(Confidence::default().is_complete());
        assert!((Confidence::default().ratio() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_single_unmatched_glyph_makes_a_cue_incomplete() {
        let c = Confidence { matched: 19, unmatched: 1, ambiguous: 0 };
        assert!(!c.is_complete());
        assert_eq!(c.total(), 20);
        assert!((c.ratio() - 0.95).abs() < 1e-6);
    }

    #[test]
    fn track_confidence_is_the_sum_of_its_cues() {
        let track = TextTrack::new(vec![cue(10, 0), cue(5, 2)], Some("eng".into()));
        assert_eq!(track.confidence.matched, 15);
        assert_eq!(track.confidence.unmatched, 2);
        assert!(!track.is_empty());
    }

    #[test]
    fn cue_text_joins_lines_with_newlines() {
        let mut c = cue(1, 0);
        c.lines = vec!["one".into(), "two".into()];
        assert_eq!(c.text(), "one\ntwo");
        assert!(!c.is_empty());
    }
}
