//! Post-correction of ambiguous reads.
//!
//! Implemented as a trait with a no-op default, and it stays that way until #12 shows a real
//! corrector nets out positive on the test corpus.
//!
//! The reason for the caution is the reason the whole project exists. A general OCR engine's
//! failure mode is a confident wrong answer, which is what the earlier investigation objected to.
//! A glyph matcher's failure mode is a detectable non-answer — and a careless corrector converts
//! the second back into the first. Rewriting `1` to `l` inside a proper noun invents text and
//! leaves no trace that it did.

use subtrackt_core::{Confidence, Cue, GlyphMatch};

/// A record of one substitution, so corrections are auditable rather than invisible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrectionLog {
    /// Cue index within the track.
    pub cue: usize,
    /// The text before correction.
    pub before: String,
    /// The text after.
    pub after: String,
}

/// Rewrites text that the matcher flagged as ambiguous.
pub trait PostCorrector {
    /// Correct one cue in place, appending to `log` for every change made.
    ///
    /// Implementations must respect two constraints, both of which exist to keep a correction from
    /// becoming an invention:
    ///
    /// * only characters whose [`GlyphMatch::is_unambiguous`] was false may be substituted;
    /// * no character may be inserted or deleted.
    fn correct(
        &self,
        cue: &mut Cue,
        matches: &[GlyphMatch],
        index: usize,
        log: &mut Vec<CorrectionLog>,
    );

    /// A short name for the extraction summary.
    fn name(&self) -> &'static str;
}

/// The default corrector: does nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopCorrector;

impl PostCorrector for NoopCorrector {
    fn correct(
        &self,
        _cue: &mut Cue,
        _matches: &[GlyphMatch],
        _index: usize,
        _log: &mut Vec<CorrectionLog>,
    ) {
    }

    fn name(&self) -> &'static str {
        "none"
    }
}

/// Whether a cue holds anything a corrector would be allowed to touch.
///
/// Cheap enough to call before running a corrector, and it keeps the corrector away from cues that
/// were read cleanly.
#[must_use]
pub const fn has_correctable_glyphs(confidence: Confidence) -> bool {
    confidence.ambiguous > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::{TimeSpan, Timestamp};

    fn cue() -> Cue {
        Cue {
            span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(500)),
            lines: vec!["He11o".into()],
            confidence: Confidence { matched: 5, unmatched: 0, ambiguous: 2 },
            forced: false,
        }
    }

    #[test]
    fn the_default_corrector_changes_nothing_and_logs_nothing() {
        let mut c = cue();
        let before = c.clone();
        let mut log = Vec::new();
        NoopCorrector.correct(&mut c, &[], 0, &mut log);
        assert_eq!(c, before);
        assert!(log.is_empty());
        assert_eq!(NoopCorrector.name(), "none");
    }

    #[test]
    fn a_cleanly_read_cue_offers_a_corrector_nothing_to_do() {
        assert!(!has_correctable_glyphs(Confidence {
            matched: 9,
            unmatched: 0,
            ambiguous: 0
        }));
        assert!(has_correctable_glyphs(Confidence {
            matched: 9,
            unmatched: 0,
            ambiguous: 1
        }));
    }
}
