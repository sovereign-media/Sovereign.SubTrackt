//! Pipeline configuration, including the accuracy gate.

use subtrackt_core::{Confidence, SubtitleFormat};
use subtrackt_glyph::binarize::Threshold;
use subtrackt_glyph::cluster::ClusterRules;
use subtrackt_glyph::matcher::MatchThresholds;
use subtrackt_text::layout::LayoutRules;

/// What to do about a cue containing a glyph the matcher could not identify.
///
/// This is the §4 accuracy gate, and it is the reason a glyph matcher is worth building where a
/// general OCR engine was not. Tesseract's failure mode is a plausible wrong word with a
/// confidence score attached; this pipeline's failure mode is a glyph that matched nothing, which
/// is a fact rather than an estimate. Because it is a fact, there is something worth deciding here.
///
/// Which variant should be the default is #13, and it wants corpus numbers rather than instinct.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum UnmatchedPolicy {
    /// Omit the cue. The output is clean but silently incomplete, which is its own hazard.
    Drop,
    /// Emit the cue with a placeholder character where the glyph was. Honest, and useless to a
    /// viewer mid-sentence.
    Placeholder,
    /// Abort the track so the caller can fall back to burn-in. Conservative, and the current
    /// default: a track that cannot be read completely is not obviously better than a track that
    /// keeps its pixels.
    #[default]
    FailTrack,
    /// Abort only if the matched fraction falls below a floor.
    Threshold {
        /// Minimum fraction of glyphs that must match, in `0.0..=1.0`.
        min_ratio: f32,
    },
}

impl UnmatchedPolicy {
    /// Whether a track with this tally should be abandoned in favour of a fallback.
    #[must_use]
    pub fn rejects(self, confidence: Confidence) -> bool {
        match self {
            Self::Drop | Self::Placeholder => false,
            Self::FailTrack => !confidence.is_complete(),
            Self::Threshold { min_ratio } => confidence.ratio() < min_ratio,
        }
    }

    /// A short name for the extraction summary.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Drop => "drop",
            Self::Placeholder => "placeholder",
            Self::FailTrack => "fail-track",
            Self::Threshold { .. } => "threshold",
        }
    }
}

/// Everything the pipeline needs beyond an input path.
#[derive(Debug, Clone, Copy, Default)]
pub struct Config {
    /// Which stream to read, or `None` for the first bitmap subtitle stream found.
    pub stream: Option<u32>,
    /// Output format.
    pub format: SubtitleFormat,
    /// Foreground/background thresholding.
    pub binarize: Threshold,
    /// Whether the feature vector is built from ink coverage rather than the binary mask.
    ///
    /// #14 found that a glyph's vector moves further between two renderings of the same character
    /// than between two different characters, and the binarizer experiments in
    /// `docs/glyph-stability.md` showed the threshold's *placement* was not the cause. This is the
    /// alternative: keep the anti-aliasing ramp as a magnitude all the way into the vector.
    pub grey_coverage: bool,
    /// Glyph matching thresholds.
    pub matching: MatchThresholds,
    /// How the stream's own shapes are grouped before any is matched.
    pub clustering: ClusterRules,
    /// Text reconstruction rules.
    pub layout: LayoutRules,
    /// What happens to unread glyphs.
    pub unmatched: UnmatchedPolicy,
    /// Whether post-correction runs. Off until #12 shows it helps.
    pub post_correct: bool,
}

impl Config {
    /// The layout rules with the ambiguity margin taken from the matching thresholds.
    ///
    /// One source of truth: the confidence tally and the matcher must not be able to disagree
    /// about what counts as a close call, or a cue could be reported clean while post-correction
    /// thinks it is not.
    #[must_use]
    pub fn layout_rules(&self) -> LayoutRules {
        LayoutRules {
            ambiguity_margin: self.matching.ambiguity_margin(),
            ..self.layout
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLEAN: Confidence = Confidence { matched: 100, unmatched: 0, ambiguous: 0 };
    const ONE_BAD: Confidence = Confidence { matched: 99, unmatched: 1, ambiguous: 0 };
    const HALF_BAD: Confidence = Confidence { matched: 50, unmatched: 50, ambiguous: 0 };

    #[test]
    fn the_default_is_conservative() {
        let policy = Config::default().unmatched;
        assert_eq!(policy, UnmatchedPolicy::FailTrack);
        assert!(
            !Config::default().post_correct,
            "post-correction stays off until measured"
        );
    }

    #[test]
    fn fail_track_rejects_on_a_single_unread_glyph() {
        assert!(!UnmatchedPolicy::FailTrack.rejects(CLEAN));
        assert!(UnmatchedPolicy::FailTrack.rejects(ONE_BAD));
    }

    #[test]
    fn drop_and_placeholder_never_abandon_the_track() {
        assert!(!UnmatchedPolicy::Drop.rejects(HALF_BAD));
        assert!(!UnmatchedPolicy::Placeholder.rejects(HALF_BAD));
    }

    #[test]
    fn a_threshold_tolerates_one_bad_glyph_but_not_half_a_track() {
        let policy = UnmatchedPolicy::Threshold { min_ratio: 0.98 };
        assert!(!policy.rejects(CLEAN));
        assert!(!policy.rejects(ONE_BAD), "99% read should pass a 98% floor");
        assert!(policy.rejects(HALF_BAD));
    }

    #[test]
    fn the_layout_rules_take_their_ambiguity_margin_from_the_matcher() {
        let config = Config::default();
        assert_eq!(
            config.layout_rules().ambiguity_margin,
            config.matching.ambiguity_margin(),
            "the tally and the matcher must agree on what a close call is"
        );
    }

    #[test]
    fn every_policy_has_a_name_for_the_summary() {
        assert_eq!(UnmatchedPolicy::FailTrack.name(), "fail-track");
        assert_eq!(UnmatchedPolicy::Threshold { min_ratio: 0.9 }.name(), "threshold");
    }
}
