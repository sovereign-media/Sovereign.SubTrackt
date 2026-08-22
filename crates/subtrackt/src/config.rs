//! Pipeline configuration, including the accuracy gate.

use subtrackt_core::{Confidence, SubtitleFormat};
use subtrackt_glyph::binarize::Threshold;
use subtrackt_glyph::cluster::ClusterRules;
use subtrackt_glyph::matcher::MatchThresholds;
use subtrackt_glyph::split::SplitRules;
use subtrackt_text::correct::VocabularyRules;
use subtrackt_text::layout::LayoutRules;

/// Fraction of glyphs that must match for a track to be worth keeping.
///
/// A floor against a track that could not be *read*, not a standard for one read *well*. Two
/// measurements bound it and neither leaves much room:
///
/// * The pipeline's own ceiling case — a fixture read with a reference set built from the very font
///   that rendered it — matches **93.9%** of its glyphs. Any floor above that is a gate that never
///   opens, which is exactly how `FailTrack` failed.
/// * The library survey scored 56 titles at the matcher's operational threshold and reports
///   **48 of 56 at or above 90%**, with a median of 96.5%.
///
/// So: 90%. It is the one value the corpus reports a title count for rather than one interpolated
/// between its rows, and it clears the ceiling case with margin.
///
/// Deliberately not tuned finer. `docs/architecture.md` records why that would be false precision —
/// coverage turns out to be a weak predictor of correctness, so fitting this figure would be
/// fitting it to the wrong quantity.
pub const DEFAULT_MIN_MATCHED: f32 = 0.90;

/// What to do about a cue containing a glyph the matcher could not identify.
///
/// This is the §4 accuracy gate, and it is the reason a glyph matcher is worth building where a
/// general OCR engine was not. Tesseract's failure mode is a plausible wrong word with a
/// confidence score attached; this pipeline's failure mode is a glyph that matched nothing, which
/// is a fact rather than an estimate. Because it is a fact, there is something worth deciding here.
///
/// #13 decided the default from corpus numbers; see `docs/architecture.md`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnmatchedPolicy {
    /// Omit the cue. The output is clean but silently incomplete, which is its own hazard.
    Drop,
    /// Emit the cue with a placeholder character where the glyph was. Honest, and useless to a
    /// viewer mid-sentence.
    Placeholder,
    /// Abort the track so the caller can fall back to burn-in.
    ///
    /// Not the default any more, and not a viable one. It rejects on a *single* unmatched glyph,
    /// and the library survey put the median title at 96.5% coverage — hundreds of unmatched
    /// glyphs in a typical film. This refuses essentially every track ever authored, which is not
    /// conservatism, it is the gate never opening.
    FailTrack,
    /// Abort only if the matched fraction falls below a floor. **The default.**
    Threshold {
        /// Minimum fraction of glyphs that must match, in `0.0..=1.0`.
        min_ratio: f32,
    },
}

impl Default for UnmatchedPolicy {
    /// A floor, from the corpus.
    ///
    /// Written by hand rather than derived because the chosen variant carries a value, and the
    /// value is the whole decision.
    fn default() -> Self {
        Self::Threshold { min_ratio: DEFAULT_MIN_MATCHED }
    }
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

    /// The fraction of glyphs this policy insists on reading.
    ///
    /// [`Self::FailTrack`] is the whole of it; the two per-cue policies never abandon a track and
    /// so demand nothing. Exists so the rejection error can say what the floor was rather than
    /// only that the track fell below it.
    #[must_use]
    pub const fn required_ratio(self) -> f32 {
        match self {
            Self::Drop | Self::Placeholder => 0.0,
            Self::FailTrack => 1.0,
            Self::Threshold { min_ratio } => min_ratio,
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
///
/// The bool count is what a configuration is. Each one is an independent switch a caller sets on
/// its own — `grey_coverage` is a vectoriser decision, `post_correct` a text-stage one,
/// `glyph_masks` a survey one — and folding them into a state enum to satisfy the lint would invent
/// Whether an unreadable component is retried as two characters that touched.
///
/// #106. 8-connected labelling fuses characters that touch at a corner — an `r`'s arm reaches over
/// the letter after it — and [`ccl`](subtrackt_glyph::ccl) pins that as accepted behaviour with no
/// pass anywhere in the tree that undoes it. `docs/error-census.md` measures the cost on a real
/// Blu-ray at **28% of its remaining errors**, second only to `l` read as `I`.
///
/// An enum rather than a `bool` so that **on** can be the default while [`Config`] keeps its
/// derived one, and so a caller reads `Defusing::Off` rather than `defuse: false` at the call site.
///
/// On by default, which is unusual for a recovery stage here and is earned by the acceptance rule
/// rather than by the size of the gain. It fires only where the matcher already returned
/// `unmatched`, and it keeps a cut only if **every** part matches within the ceiling — so it can
/// move a glyph from unread to read and can never turn a match into a wrong answer. That is the
/// direction `docs/post-correction.md` requires a recovery stage to fail in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Defusing {
    /// Cut an unreadable component and keep the cut if every part reads.
    #[default]
    On,
    /// Leave it unread.
    Off,
}

/// combinations that do not exist and hide the ones that do.
#[allow(clippy::struct_excessive_bools)]
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
    /// Whether a component the matcher cannot read is retried as two characters that touched.
    pub defuse: Defusing,
    /// How a component is offered up for cutting when [`Config::defuse`] is [`Defusing::On`].
    pub split: SplitRules,
    /// Glyph matching thresholds.
    pub matching: MatchThresholds,
    /// How the stream's own shapes are grouped before any is matched.
    pub clustering: ClusterRules,
    /// Text reconstruction rules.
    pub layout: LayoutRules,
    /// What happens to unread glyphs.
    pub unmatched: UnmatchedPolicy,
    /// Whether ambiguous reads are resolved from the characters around them.
    ///
    /// Off, and `docs/post-correction.md` records the measurement that keeps it there rather than
    /// a preference. The corrector cannot touch a glyph the matcher read clearly and cannot change
    /// a line's length, so switching it on is not dangerous — it is simply not yet shown to be
    /// worth it, and this project does not turn on a stage that rewrites text on a hunch.
    pub post_correct: bool,
    /// Whether post-correction may also resolve a word-edge glyph from the track's own vocabulary.
    ///
    /// An arm of the corrector rather than a stage, and gated behind [`Self::post_correct`]. Off
    /// for the reason that is off: the only comparison available for a real track is another
    /// release's subtitle, which is evidence rather than hand-verified ground truth.
    pub track_vocabulary: bool,
    /// How that vocabulary is built and consulted.
    pub vocabulary: VocabularyRules,
    /// Whether a survey keeps each glyph's un-normalised ink alongside its feature vector.
    ///
    /// Off, and it costs nothing while it is: the mask is built during segmentation either way and
    /// dropped at the end of it, so this only decides whether a copy is kept. Keeping one is not
    /// free at scale — a feature film is tens of thousands of glyphs — and the two commands that
    /// survey in anger, `glyphs` and `fit`, have no use for it.
    ///
    /// It exists because the feature vector is a lossy projection *by design*: letterboxed onto a
    /// 16x16 grid and thresholded per cell, built so two renderings of one character converge.
    /// Anything asking what a glyph's ink is *like* rather than which character it is — stroke
    /// weight, contrast, the shape of a terminal — cannot be answered from it. `xtask font-id`
    /// measured that gap at 46 to 54 points of font-retrieval accuracy, which is what #63 needs and
    /// what nothing else in the pipeline does.
    ///
    /// Nothing on the matching path reads this. It changes what a survey carries, never what an
    /// extraction decides.
    pub glyph_masks: bool,
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
    fn the_default_gate_is_a_floor_rather_than_a_demand_for_perfection() {
        // `FailTrack` was the default and rejected on a single unmatched glyph. The library survey
        // put the median title at 96.5% coverage — hundreds of unmatched glyphs in a typical film —
        // so it refused essentially every track ever authored. See `docs/architecture.md`.
        let policy = Config::default().unmatched;
        assert_eq!(policy, UnmatchedPolicy::Threshold { min_ratio: DEFAULT_MIN_MATCHED });
        assert!(!policy.rejects(Confidence { matched: 965, unmatched: 35, ambiguous: 0 }));
        assert!(
            !policy.rejects(Confidence { matched: 123, unmatched: 8, ambiguous: 0 }),
            "and it has to clear the pipeline ceiling case at 93.9%, or it is FailTrack again"
        );
        assert!(
            UnmatchedPolicy::FailTrack.rejects(Confidence {
                matched: 965,
                unmatched: 35,
                ambiguous: 0
            }),
            "which is what the old default did to the median title"
        );
        assert!(
            !Config::default().post_correct,
            "post-correction stays off until the measurement says otherwise"
        );
    }

    #[test]
    fn the_floor_refuses_a_track_nothing_could_be_read_from() {
        // The case that actually reaches a user today: no reference set, so no glyph matches.
        // Whatever else the floor does, it has to catch this one.
        let nothing = Confidence { matched: 0, unmatched: 400, ambiguous: 0 };
        assert!(Config::default().unmatched.rejects(nothing));
    }

    #[test]
    fn every_policy_says_what_fraction_it_insists_on() {
        // So the rejection error can name the floor rather than only report falling below it.
        assert!((UnmatchedPolicy::FailTrack.required_ratio() - 1.0).abs() < f32::EPSILON);
        assert!((UnmatchedPolicy::Drop.required_ratio() - 0.0).abs() < f32::EPSILON);
        assert!(
            (UnmatchedPolicy::Threshold { min_ratio: 0.9 }.required_ratio() - 0.9).abs()
                < f32::EPSILON
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
