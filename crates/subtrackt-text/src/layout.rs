//! Turning a sequence of matched glyphs back into lines of text.
//!
//! Not implemented — see #11.
//!
//! The hard part is spacing. A proportional typeface has no single space width, and the gap
//! between a kerned pair can exceed the gap around a real space elsewhere on the same line. So the
//! space threshold has to be derived per line from the observed gap distribution rather than set
//! as a constant, and certainly not as a constant in pixels — the same title ships at several
//! resolutions.

use subtrackt_core::{Cue, Error, Glyph, GlyphMatch, Result, SubtitleImage, TextAssembler};

/// Rules for reconstructing text from glyph geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutRules {
    /// A gap wider than this multiple of the line's median inter-glyph gap, in percent, is a space.
    pub space_gap_percent: u32,
    /// Character substituted for an unmatched glyph under
    /// [`crate::format`]-visible placeholder policy.
    pub placeholder: char,
    /// Whether a leading `-` is preserved as a speaker marker rather than treated as punctuation.
    pub preserve_speaker_dash: bool,
}

impl Default for LayoutRules {
    fn default() -> Self {
        Self {
            space_gap_percent: 250,
            placeholder: '\u{fffd}',
            preserve_speaker_dash: true,
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
}

impl TextAssembler for SpatialAssembler {
    fn assemble(
        &self,
        _image: &SubtitleImage,
        glyphs: &[Glyph],
        matches: &[GlyphMatch],
    ) -> Result<Cue> {
        if glyphs.len() != matches.len() {
            return Err(Error::Config(format!(
                "assemble got {} glyphs and {} matches; they must be index-aligned",
                glyphs.len(),
                matches.len()
            )));
        }
        Err(Error::unsupported("text reconstruction from glyph geometry", 11))
    }
}

/// Gap width, in percent of the median gap, above which a gap is a space.
///
/// Split out because it is the single number #11 has to get right, and it should be testable
/// without a full image.
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
    use subtrackt_core::{IndexedBitmap, Palette, Rect, TimeSpan, Timestamp};

    fn image() -> SubtitleImage {
        SubtitleImage {
            span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(1_000)),
            position: Rect::new(0, 0, 2, 2),
            bitmap: IndexedBitmap::blank(2, 2),
            palette: Palette::transparent(2),
            forced: false,
        }
    }

    #[test]
    fn assembling_reports_the_tracking_issue() {
        let err = SpatialAssembler::default()
            .assemble(&image(), &[], &[])
            .unwrap_err();
        assert!(matches!(err, Error::Unsupported { issue: 11, .. }), "got {err:?}");
    }

    #[test]
    fn mismatched_glyph_and_match_slices_are_a_configuration_error_not_a_panic() {
        let glyph = Glyph {
            bounds: Rect::new(0, 0, 4, 6),
            line: 0,
            features: subtrackt_core::FeatureVector::EMPTY,
        };
        let err = SpatialAssembler::default()
            .assemble(&image(), &[glyph], &[])
            .unwrap_err();
        assert!(matches!(err, Error::Config(_)), "got {err:?}");
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
}
