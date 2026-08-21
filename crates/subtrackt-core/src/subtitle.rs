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
    #[must_use]
    pub fn ratio(self) -> f32 {
        if self.total() == 0 {
            return 1.0;
        }
        f32::from(u16::try_from(self.matched).unwrap_or(u16::MAX))
            / f32::from(u16::try_from(self.total()).unwrap_or(u16::MAX))
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

    fn cue(matched: u32, unmatched: u32) -> Cue {
        Cue {
            span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(1_000)),
            lines: vec!["hello".into()],
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
