//! Pipeline configuration, including the accuracy gate.

use subtrackt_core::{Confidence, SubtitleFormat};
use subtrackt_glyph::binarize::Threshold;
use subtrackt_glyph::cluster::ClusterRules;
use subtrackt_glyph::matcher::MatchThresholds;
use subtrackt_glyph::split::SplitRules;
use subtrackt_text::correct::Lexicon;
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

/// Where a line that cannot measure itself gets its scale.
///
/// #184. `metrics::anchors` refuses a line whose standing glyphs are all one height, because such a
/// line cannot say whether that height is cap or x — `NO ONE SAW` and `no one saw` present
/// identically. On King Kong that refusal is **420 of 451 unmeasured lines** and one glyph in
/// seven, every one of them then matched with the only term that separates `o` from `O` switched
/// off.
///
/// The line is not short of evidence about where it *sits*: its bottoms agreed on a baseline. It is
/// short of a **scale**, and a scale is the one thing a subtitle track has in common from end to
/// end, because a stream is authored once.
///
/// An enum rather than a `bool` for [`Defusing`]'s reason: **on** is the default and [`Config`]
/// keeps its derived one. The default is a measurement — `docs/error-census.md` §"The scale a line
/// cannot find in itself" has the bench table — and it is a weaker claim than [`Defusing`]'s: this
/// can change a read rather than only recover an unread one, and it did, 9 times against 75 the
/// other way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineScale {
    /// Take the cap height the rest of the track is drawn at, and keep the line's own baseline.
    #[default]
    FromTheTrack,
    /// Report unknown, and match the line's glyphs on shape alone.
    FromTheLineAlone,
}

/// Whether an extracted file records what produced it.
///
/// Three-valued rather than a `bool` because the honest answer differs by format, and #129 did not
/// want that difference buried in a caller. **`WebVTT` has `NOTE`; `SubRip` has no comment syntax at
/// all** — a note there is text before the first cue, which our parser skips and most parsers skip
/// and a strict one may refuse. So the default writes one where the format defines somewhere to
/// put it, and asking for the other case is explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProvenancePolicy {
    /// A note where the format has syntax for one: `WebVTT` yes, `SubRip` no.
    #[default]
    WhereLegal,
    /// A note in every format, including `SubRip`'s non-standard leading text.
    Always,
    /// No note. The bytes are what the format defines and nothing else.
    Never,
}

impl ProvenancePolicy {
    /// Whether a note should be written for this format.
    #[must_use]
    pub const fn writes(self, format: SubtitleFormat) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::WhereLegal => matches!(format, SubtitleFormat::Vtt),
        }
    }
}

/// Everything the pipeline needs beyond an input path.
///
/// The bool count is what a configuration is. Each one is an independent switch a caller sets on
/// its own — `grey_coverage` is a vectoriser decision, `post_correct` a text-stage one,
/// `glyph_masks` a survey one — and folding them into a state enum to satisfy the lint would invent
/// combinations that do not exist and hide the ones that do.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
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
    /// Where a line whose glyphs are all one height gets the scale it cannot find in itself.
    pub line_scale: LineScale,
    /// Glyph matching thresholds.
    pub matching: MatchThresholds,
    /// How the stream's own shapes are grouped before any is matched.
    pub clustering: ClusterRules,
    /// Text reconstruction rules.
    pub layout: LayoutRules,
    /// What happens to unread glyphs.
    pub unmatched: UnmatchedPolicy,
    /// Whether the written file records what produced it, and what it read.
    pub provenance: ProvenancePolicy,
    /// Whether ambiguous reads are resolved from the characters around them.
    ///
    /// **On since #185**, and the criterion `docs/post-correction.md` had named for the life of the
    /// file is what turned it on: the same table, over real tracks with hand-verified ground truth,
    /// still showing zero lines made worse. `scripts/truth/` holds that truth — the first 300 cues
    /// of A Fish Called Wanda, read off the images by eye — and this arm improves 3 cues of it and
    /// worsens none.
    ///
    /// An enum would say this better and a `bool` is what [`Config`]'s derived `Default` can carry
    /// as `false`; see [`Defusing`] for the shape that solves it. This is a `bool` because it was
    /// one before it was true, and [`Self::default`] sets it rather than deriving it.
    pub post_correct: bool,
    /// A word list for #236's arm, empty unless a caller supplied one.
    ///
    /// A `Lexicon` rather than a path, and the library never opens the file: a word list is *data*
    /// and the crate that reads a format is the CLI's business, which keeps `CLAUDE.md`'s
    /// dependency rule out of the question entirely. Empty is the default and an empty one switches
    /// the arm off rather than firing it on nothing.
    pub lexicon: Lexicon,
    /// Whether the matcher may answer with a letter the declared language cannot spell.
    ///
    /// #230, and **on**, against the criterion #185 used to flip post-correction's default: a table
    /// over the bench showing no cue made worse. It reads **239 cues better and 0 worse**, with
    /// every scored track improving and A Fish Called Wanda going 1.3% to 1.0%.
    ///
    /// It does nothing at all on a track with no declaration, which is most of what the default
    /// touches: #180 found 21 of 50 titles declaring a language on the track this pipeline chooses.
    /// So the default is safe in the direction that matters -- a track that says nothing is read
    /// exactly as it was -- and the gain is available to the ones that speak up.
    pub restrict_to_language: bool,
    /// The language to read the track as, overriding whatever the container declares.
    ///
    /// `None` means take the container's word for it, which is what every caller did before #230.
    /// A bare `.sup` is one PGS stream with no container at all, so without this there is nothing
    /// for either language gate to read on the format the bench is dumped in.
    pub language: Option<String>,
    /// Whether post-correction may also resolve a word-edge glyph from the track's own vocabulary.
    ///
    /// An arm of the corrector rather than a stage, and gated behind [`Self::post_correct`]. **Off,
    /// and the only one of the three still off after #185** — not for want of ground truth but for
    /// want of firings: since #110 gave the matcher an ink aspect ratio, this arm makes zero
    /// substitutions on every disc of the bench, so the verified table has nothing to say about it.
    /// Its two unobserved failure modes are unchanged: a proper noun that case-folds onto a common
    /// word, and a single clear occurrence that was itself a misread.
    pub track_vocabulary: bool,
    /// Whether post-correction may promote a one-character word to `I`.
    ///
    /// The third arm, and the only one that knows a language: `l` is not a word and `I` is. **On
    /// since #185**, on the same verified table as [`Self::post_correct`], where it is much the
    /// larger half — 54 of the 300 verified cues improved and none made worse.
    ///
    /// What stands behind the rule itself is a measurement rather than an assertion: across 77
    /// English release subtitles, a lone lowercase `l` occurs 641 times and every one is a misread
    /// `I`. Only the contraction half needs a language, and it asks the container.
    /// `docs/post-correction.md` §"The one-character word" has the rest.
    pub lone_words: bool,
    /// Assert that the track is English, whatever the container says.
    ///
    /// Only [`Self::lone_words`]'s contraction half reads this, and only because the container so
    /// often says nothing: 15 of the 50 corpus titles carry neither a language tag nor a title
    /// naming one. Without an override those tracks lose a third of what the arm can do, on the
    /// absence of a label rather than on any evidence about the text.
    ///
    /// A blunt flag rather than a language tag because [`Config`] is `Copy` and a `String` would
    /// end that, and because one consumer does not justify a vocabulary of tags. It asserts; it
    /// does not detect.
    pub assume_english: bool,
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
    /// Whether to refuse a track whose declared language is written in a script the reference set
    /// holds no character of.
    ///
    /// **On.** It is the only gate in this pipeline that consults evidence from outside the read,
    /// and #218 measured why one was needed: nothing computed from the read can tell a wrong script
    /// from a wrong typeface, because to everything downstream they are the same event. Mean match
    /// distance over one disc is 26.5 to 37.3 for five non-Latin tracks and 23.1 to 34.9 for the
    /// same English track read with six wrong typefaces -- the second band contains the first.
    ///
    /// It refuses only on a *fact*: the container named a language, the language has a known
    /// script, and the set holds not one character of it. An unknown tag, an untagged stream and a
    /// set that holds even a single character of the script all pass. `docs/language-coverage.md`
    /// has what it catches.
    ///
    /// Off is for a caller who knows better than the container -- a mistagged stream, or a set
    /// deliberately fitted to something the tag does not describe.
    pub check_declared_script: bool,
}

/// Written out rather than derived, since #185.
///
/// Every field here was a derived zero until post-correction earned its default, and a derive
/// cannot express "false for six of these and true for two". Spelling the whole thing out has a
/// second use that is worth more than the lines it costs: a reader asking what a plain
/// `subtrackt extract` does now reads one list rather than seventeen doc comments, and adding a
/// field forces a decision here rather than defaulting it silently.
impl Default for Config {
    fn default() -> Self {
        Self {
            stream: None,
            format: SubtitleFormat::default(),
            binarize: Threshold::default(),
            grey_coverage: false,
            defuse: Defusing::default(),
            split: SplitRules::default(),
            line_scale: LineScale::default(),
            matching: MatchThresholds::default(),
            clustering: ClusterRules::default(),
            layout: LayoutRules::default(),
            unmatched: UnmatchedPolicy::default(),
            provenance: ProvenancePolicy::default(),
            // The two #185 turned on. `docs/post-correction.md` §"What flipped it" has the table.
            restrict_to_language: true,
            language: None,
            lexicon: Lexicon::default(),
            post_correct: true,
            lone_words: true,
            // The arm that stays off, and not for want of ground truth — for want of firings.
            track_vocabulary: false,
            assume_english: false,
            vocabulary: VocabularyRules::default(),
            glyph_masks: false,
            check_declared_script: true,
        }
    }
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
    }

    #[test]
    fn the_arms_the_verified_table_turned_on_are_on_and_the_other_one_is_not() {
        // #185. The criterion `docs/post-correction.md` names is a *table*, not a preference, and
        // it was met on 300 cues of A Fish Called Wanda read off the disc by eye: 3 cues better
        // from the context arm, 54 from the one-character word, none worse from either.
        let config = Config::default();
        assert!(config.post_correct, "the context arm: 3 verified cues better, 0 worse");
        assert!(
            config.lone_words,
            "the one-character word: 54 verified cues better, 0 worse"
        );
        assert!(
            !config.track_vocabulary,
            "and the arm that fires on nothing stays off: since #110 it makes no substitution on              any disc of the bench, so the verified table has nothing to say about it"
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
