//! Scoring an extraction against known-good text.
//!
//! Everything measured before this crate gained a scorer was *coverage* — whether a glyph looked
//! like something in the reference set. Coverage and correctness are different quantities and can
//! diverge without warning: a glyph can match confidently and be the wrong character. This is the
//! instrument that tells them apart, and until it exists no accuracy claim in this project means
//! anything.
//!
//! Deliberately in the library rather than in tooling. A caller integrating this has the same
//! question — Sovereign already holds text subtitle tracks for many titles, and scoring a bitmap
//! extraction against one of those is the obvious way to validate a run in production.

use crate::core::TextTrack;

/// How far an extraction is from its ground truth.
///
/// Rates are errors per reference unit, so `0.0` is perfect and values above `1.0` are possible
/// when the extraction invents more than it gets right.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    /// Edit distance in characters.
    pub character_errors: usize,
    /// Characters in the reference.
    pub reference_characters: usize,
    /// Edit distance in whitespace-separated words.
    pub word_errors: usize,
    /// Words in the reference.
    pub reference_words: usize,
}

impl Score {
    /// Character error rate. An empty reference scores `0.0` when the extraction is also empty.
    #[must_use]
    pub fn character_error_rate(&self) -> f64 {
        ratio(self.character_errors, self.reference_characters)
    }

    /// Word error rate.
    #[must_use]
    pub fn word_error_rate(&self) -> f64 {
        ratio(self.word_errors, self.reference_words)
    }

    /// Whether the extraction matched the reference exactly.
    #[must_use]
    pub const fn is_exact(&self) -> bool {
        self.character_errors == 0
    }

    /// Combine two scores, for rolling a corpus-wide total.
    ///
    /// Summing errors and units separately, rather than averaging rates, weights each track by its
    /// length — which is what a corpus figure should do.
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        Self {
            character_errors: self.character_errors + other.character_errors,
            reference_characters: self.reference_characters + other.reference_characters,
            word_errors: self.word_errors + other.word_errors,
            reference_words: self.reference_words + other.reference_words,
        }
    }
}

fn ratio(errors: usize, units: usize) -> f64 {
    if units == 0 {
        return if errors == 0 { 0.0 } else { 1.0 };
    }
    // A subtitle track is thousands of characters, nowhere near f64's exact integer range, so the
    // precision lint has nothing to warn about here.
    #[allow(clippy::cast_precision_loss)]
    {
        errors as f64 / units as f64
    }
}

/// Levenshtein distance between two sequences, with a rolling row.
fn edit_distance<T: PartialEq>(reference: &[T], hypothesis: &[T]) -> usize {
    if reference.is_empty() {
        return hypothesis.len();
    }
    if hypothesis.is_empty() {
        return reference.len();
    }

    let mut previous: Vec<usize> = (0..=hypothesis.len()).collect();
    let mut current = vec![0usize; hypothesis.len() + 1];

    for (i, r) in reference.iter().enumerate() {
        current[0] = i + 1;
        for (j, h) in hypothesis.iter().enumerate() {
            let substitution = previous[j] + usize::from(r != h);
            let deletion = previous[j + 1] + 1;
            let insertion = current[j] + 1;
            current[j + 1] = substitution.min(deletion).min(insertion);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[hypothesis.len()]
}

/// Score one extraction against its ground truth.
///
/// Both sides are compared as plain text: cue boundaries are ignored, because a stage that merges
/// or splits cues should show up as the text errors it causes rather than as a structural mismatch
/// that hides them.
#[must_use]
pub fn score_text(reference: &str, hypothesis: &str) -> Score {
    let reference_chars: Vec<char> = reference.chars().collect();
    let hypothesis_chars: Vec<char> = hypothesis.chars().collect();

    let reference_words: Vec<&str> = reference.split_whitespace().collect();
    let hypothesis_words: Vec<&str> = hypothesis.split_whitespace().collect();

    Score {
        character_errors: edit_distance(&reference_chars, &hypothesis_chars),
        reference_characters: reference_chars.len(),
        word_errors: edit_distance(&reference_words, &hypothesis_words),
        reference_words: reference_words.len(),
    }
}

/// Score a whole track against reference lines, one line per cue line.
#[must_use]
pub fn score_track(reference: &str, track: &TextTrack) -> Score {
    let extracted: Vec<String> = track.cues.iter().map(super::core::Cue::text).collect();
    score_text(reference.trim(), extracted.join("\n").trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Confidence, Cue, TimeSpan, Timestamp};

    #[test]
    fn identical_text_scores_zero() {
        let score = score_text("Hello there.", "Hello there.");
        assert!(score.is_exact());
        assert!((score.character_error_rate() - 0.0).abs() < f64::EPSILON);
        assert!((score.word_error_rate() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_single_wrong_character_costs_one_edit() {
        // The classic confusion this project exists to detect: l for 1.
        let score = score_text("l1lo", "lllo");
        assert_eq!(score.character_errors, 1);
        assert_eq!(score.reference_characters, 4);
        assert!((score.character_error_rate() - 0.25).abs() < 1e-12);
    }

    #[test]
    fn insertions_and_deletions_both_count() {
        assert_eq!(score_text("abc", "abxc").character_errors, 1, "insertion");
        assert_eq!(score_text("abc", "ac").character_errors, 1, "deletion");
        assert_eq!(score_text("abc", "").character_errors, 3, "everything missing");
        assert_eq!(score_text("", "abc").character_errors, 3, "everything invented");
    }

    #[test]
    fn word_errors_count_whole_words_not_characters() {
        // One badly-read word is one word error however many letters are wrong inside it.
        let score = score_text("the quick brown fox", "the qvick brown fox");
        assert_eq!(score.word_errors, 1);
        assert_eq!(score.reference_words, 4);
        assert_eq!(score.character_errors, 1);
    }

    #[test]
    fn whitespace_differences_do_not_count_as_word_errors() {
        let score = score_text("one two", "one   two");
        assert_eq!(score.word_errors, 0, "word scoring is whitespace-insensitive");
        assert!(score.character_errors > 0, "but character scoring is not");
    }

    #[test]
    fn an_empty_reference_is_perfect_only_if_nothing_was_extracted() {
        assert!((score_text("", "").character_error_rate() - 0.0).abs() < f64::EPSILON);
        assert!((score_text("", "spurious").character_error_rate() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_completely_unread_track_scores_one() {
        // What the pipeline produces today with no reference set: every glyph a placeholder.
        let score = score_text("Hello", "\u{fffd}\u{fffd}\u{fffd}\u{fffd}\u{fffd}");
        assert!((score.character_error_rate() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn merging_weights_by_length_rather_than_averaging_rates() {
        // A 1-error 100-character track and a 1-error 2-character track must not average to 25%.
        let long = Score {
            character_errors: 1,
            reference_characters: 100,
            word_errors: 0,
            reference_words: 20,
        };
        let short = Score {
            character_errors: 1,
            reference_characters: 2,
            word_errors: 1,
            reference_words: 1,
        };
        let total = long.merge(short);
        assert_eq!(total.character_errors, 2);
        assert_eq!(total.reference_characters, 102);
        assert!((total.character_error_rate() - 2.0 / 102.0).abs() < 1e-12);
    }

    #[test]
    fn scoring_a_track_joins_its_cues() {
        let cue = |text: &str| Cue {
            span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(1_000)),
            lines: vec![text.to_owned()],
            confidence: Confidence::default(),
            forced: false,
        };
        let track = TextTrack::new(vec![cue("first line"), cue("second line")], None);
        assert!(score_track("first line\nsecond line", &track).is_exact());
    }

    #[test]
    fn cue_boundaries_do_not_affect_the_score_by_themselves() {
        // A stage that splits a cue in two should be judged on the text it produces, not punished
        // for the structural difference.
        let track = TextTrack::new(
            vec![Cue {
                span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(500)),
                lines: vec!["one".into(), "two".into()],
                confidence: Confidence::default(),
                forced: false,
            }],
            None,
        );
        assert!(score_track("one\ntwo", &track).is_exact());
    }

    #[test]
    fn edit_distance_is_symmetric_and_zero_on_equal_input() {
        let a: Vec<char> = "kitten".chars().collect();
        let b: Vec<char> = "sitting".chars().collect();
        assert_eq!(edit_distance(&a, &b), 3);
        assert_eq!(edit_distance(&b, &a), 3);
        assert_eq!(edit_distance(&a, &a), 0);
    }
}
