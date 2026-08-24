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

use std::collections::HashMap;

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

/// Levenshtein distance between two sequences.
///
/// Bit-parallel, after Myers (1999) and Hyyrö's blocked extension: the dynamic programming column
/// is carried as two bitmasks — one bit per reference position — so sixty-four cells advance per
/// machine word instead of one. Exact, not approximate. It computes the same number
/// the obvious rolling-row recurrence does, and is tested against it directly —
/// `edit_distance_rows` is kept in the test module for exactly that.
///
/// **This is on the bench's critical path, which is why it is worth the complexity.**
/// `docs/cost-baseline.md` measured `xtask srt-score` at 7.4s against a feature, versus 1.1s to
/// produce the extraction being scored — the largest single cost in the tree, paid twice per change
/// per disc by the standing rule to bench before and after. The naive recurrence is O(n·m) over two
/// hundred-thousand-character transcripts, which is where all of it went.
///
/// The rolling-row implementation stays in the tree as the oracle rather than as a fallback. A
/// subtle bug here would move every accuracy figure this project has published, so the guarantee
/// wanted is not "it looks right" but "it agrees with the obvious implementation on everything we
/// can enumerate" — which is what `the_fast_and_the_obvious_distance_agree_on_every_short_pair`
/// checks.
#[must_use]
pub fn edit_distance<T: Eq + std::hash::Hash>(reference: &[T], hypothesis: &[T]) -> usize {
    // `reference` becomes the bit-parallel axis, so the shorter side goes there: the work is
    // O(len(hypothesis) · blocks) and blocks is len(reference) / 64 rounded up. Distance is
    // symmetric, so this is free — and only available here, because `score_row`'s caller needs a
    // particular orientation and cannot swap.
    if reference.len() > hypothesis.len() {
        return edit_distance(hypothesis, reference);
    }
    *score_row(reference, hypothesis).last().unwrap_or(&0)
}

/// Edit distance from all of `reference` to **every prefix** of `hypothesis`.
///
/// `row[j]` is the distance to `hypothesis[..j]`, so `row[0]` is `reference.len()` and the last
/// entry is [`edit_distance`]. One row of the matrix, in the orientation given rather than the
/// cheaper one — a caller that only wants the number should use `edit_distance`, which is free to
/// swap the sides.
///
/// This is the whole of what Hirschberg's algorithm needs: two of these, one forward and one over
/// both sequences reversed, locate the column where an optimal alignment crosses a midpoint.
/// Computing them bit-parallel rather than cell by cell is the difference between a track-level
/// traceback taking ten seconds and taking well under one, which `xtask srt-score` pays per scored
/// track per bench pass.
///
/// The running score falls out of the recurrence [`edit_distance`] already advances: after each
/// symbol of `hypothesis` the horizontal delta out of the last block *is* the change to the
/// bottom-right cell, so recording it per symbol costs one push.
#[must_use]
pub fn score_row<T: Eq + std::hash::Hash>(reference: &[T], hypothesis: &[T]) -> Vec<usize> {
    if reference.is_empty() {
        return (0..=hypothesis.len()).collect();
    }
    let mut row = Vec::with_capacity(hypothesis.len() + 1);
    row.push(reference.len());

    let blocks = reference.len().div_ceil(64);
    // One bitmask per distinct symbol, marking where in `reference` it occurs. Built once per call
    // and read once per symbol of `hypothesis`, which is what makes the inner loop a few machine
    // instructions rather than a comparison.
    let mut positions: HashMap<&T, Vec<u64>> = HashMap::new();
    for (index, symbol) in reference.iter().enumerate() {
        positions.entry(symbol).or_insert_with(|| vec![0; blocks])[index / 64] |= 1 << (index % 64);
    }
    let absent = vec![0u64; blocks];

    // The column's vertical deltas as bit vectors: `positive` marks the rows one greater than the
    // row above, `negative` those one less. Every cell of the column is one of the two or neither,
    // which is the property that lets a column be two words instead of a thousand numbers.
    let mut positive = vec![u64::MAX; blocks];
    let mut negative = vec![0u64; blocks];
    let mut score = reference.len();

    // Only the last reference row carries the answer, and in the final block that row is not
    // necessarily the top bit.
    let last_bit = 1u64 << ((reference.len() - 1) % 64);

    for symbol in hypothesis {
        let equal = positions.get(symbol).unwrap_or(&absent);
        // The horizontal delta entering the block, in `-1..=1`. It starts at +1 because the
        // boundary column of the matrix is `0, 1, 2, ...`: each new symbol costs one more before
        // any of the reference has been considered.
        let mut horizontal: i32 = 1;

        for (block, eq) in equal.iter().enumerate() {
            let mut eq = *eq;
            let vp = positive[block];
            let vn = negative[block];

            // An incoming -1 is modelled as a match at the block's first row, which is what
            // carries the deficit across the word boundary without a second addition.
            if horizontal < 0 {
                eq |= 1;
            }
            let xv = eq | vn;
            let xh = (((eq & vp).wrapping_add(vp)) ^ vp) | eq;
            let mut hp = vn | !(xh | vp);
            let mut hn = vp & xh;

            // Read the outgoing delta before shifting. In the final block that is the reference's
            // last row; in every earlier one it is the top of the word.
            let probe = if block + 1 == blocks {
                last_bit
            } else {
                1 << 63
            };
            let out = if hp & probe != 0 {
                1
            } else if hn & probe != 0 {
                -1
            } else {
                0
            };

            hp <<= 1;
            hn <<= 1;
            if horizontal > 0 {
                hp |= 1;
            } else if horizontal < 0 {
                hn |= 1;
            }
            positive[block] = hn | !(xv | hp);
            negative[block] = hp & xv;
            horizontal = out;
        }
        // `horizontal` now holds the delta out of the last block, which is the change to D[m][j].
        score = score.wrapping_add_signed(horizontal as isize);
        row.push(score);
    }
    row
}

/// Levenshtein distance between two sequences, with a rolling row.
///
/// The obvious recurrence, kept as the oracle [`edit_distance`] is tested against rather than as a
/// path anything takes. Compiled only for tests, since that is the only thing that calls it — but
/// deleting it would leave the bit-parallel version with nothing to be right *against*.
#[cfg(test)]
fn edit_distance_rows<T: PartialEq>(reference: &[T], hypothesis: &[T]) -> usize {
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
    /// A deterministic sequence, so a failure names an input rather than a seed.
    fn pseudo_random(len: usize, alphabet: usize, seed: u64) -> Vec<u8> {
        let mut state = seed | 1;
        (0..len)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                u8::try_from((state >> 33) % alphabet as u64).unwrap()
            })
            .collect()
    }

    #[test]
    fn the_fast_and_the_obvious_distance_agree_on_every_short_pair() {
        // The whole safety argument for the bit-parallel implementation. It is not obviously
        // correct by reading -- that is the point of it -- so what stands behind it is agreement
        // with the recurrence anyone can check, over every pair that can be enumerated.
        //
        // Every string over {0,1,2} up to length 5 against every other: 364 x 364 pairs, covering
        // insertions, deletions, substitutions, empty sides, repeats and total mismatches.
        let mut all: Vec<Vec<u8>> = vec![Vec::new()];
        let mut frontier = vec![Vec::new()];
        for _ in 0..5 {
            let mut next = Vec::new();
            for word in &frontier {
                for symbol in 0..3u8 {
                    let mut grown: Vec<u8> = word.clone();
                    grown.push(symbol);
                    next.push(grown);
                }
            }
            all.extend(next.iter().cloned());
            frontier = next;
        }

        for a in &all {
            for b in &all {
                assert_eq!(edit_distance(a, b), edit_distance_rows(a, b), "{a:?} vs {b:?}");
            }
        }
    }

    #[test]
    fn the_score_row_is_right_at_every_column_and_not_only_the_last() {
        // The tests around this one check the corner, which is all `edit_distance` returns.
        // Hirschberg reads the *whole* row and picks a split from it, so a row that were right only
        // at its end would put the split in the wrong column and produce a valid-looking alignment
        // of the wrong text. Checked against the obvious recurrence, one prefix at a time.
        for reference in ["", "a", "abc", "banana", "the quick brown fox"] {
            for hypothesis in ["", "a", "abd", "bananas", "the quick brown fox", "xyzzy"] {
                let r: Vec<char> = reference.chars().collect();
                let h: Vec<char> = hypothesis.chars().collect();
                let row = score_row(&r, &h);
                assert_eq!(row.len(), h.len() + 1, "{reference:?} vs {hypothesis:?}");
                for (j, distance) in row.iter().enumerate() {
                    assert_eq!(
                        *distance,
                        edit_distance_rows(&r, &h[..j]),
                        "{reference:?} vs {hypothesis:?}[..{j}]"
                    );
                }
            }
        }

        // Same property past 64 symbols, where a second word opens and the horizontal carry
        // between blocks starts deciding the running score.
        for len in [63usize, 64, 65, 130] {
            let reference = pseudo_random(len, 4, 7);
            let hypothesis = pseudo_random(len + 11, 4, 99);
            let row = score_row(&reference, &hypothesis);
            for (j, distance) in row.iter().enumerate() {
                assert_eq!(
                    *distance,
                    edit_distance_rows(&reference, &hypothesis[..j]),
                    "{len}/{j}"
                );
            }
        }
    }

    #[test]
    fn the_two_distances_agree_past_the_width_of_one_machine_word() {
        // The blocked path, which the short pairs above never reach. 64 is where a second block
        // opens and the horizontal carry between blocks starts mattering, so the lengths bracket
        // it and run well past it.
        for len in [63usize, 64, 65, 127, 128, 129, 200, 501] {
            for seed in 1..6u64 {
                let a = pseudo_random(len, 4, seed);
                let b = pseudo_random(len + usize::try_from(seed).unwrap() * 7, 4, seed * 31);
                assert_eq!(
                    edit_distance(&a, &b),
                    edit_distance_rows(&a, &b),
                    "len {len} seed {seed}"
                );
            }
        }
    }

    #[test]
    fn distance_is_symmetric_and_zero_only_against_itself() {
        for seed in 1..8u64 {
            let a = pseudo_random(150, 5, seed);
            let b = pseudo_random(150, 5, seed + 100);
            assert_eq!(edit_distance(&a, &b), edit_distance(&b, &a));
            assert_eq!(edit_distance(&a, &a), 0);
            assert!(edit_distance(&a, &b) > 0);
        }
    }

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
            italic: Vec::new(),
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
                italic: Vec::new(),
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
