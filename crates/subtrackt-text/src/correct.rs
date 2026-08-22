//! Post-correction of ambiguous reads.
//!
//! The reason for the caution here is the reason the whole project exists. A general OCR engine's
//! failure mode is a confident wrong answer, which is what the earlier investigation objected to.
//! A glyph matcher's failure mode is a detectable non-answer — and a careless corrector converts
//! the second back into the first. Rewriting `1` to `l` inside a proper noun invents text and
//! leaves no trace that it did.
//!
//! So the corrector shipped here is built out of refusals rather than out of guesses, and every one
//! of them is structural rather than a matter of tuning:
//!
//! * **Only a glyph the matcher itself flagged as ambiguous may be touched.** Everything else was
//!   read clearly and is none of this stage's business.
//! * **Only within a confusion set.** A character may be exchanged for one that a size-normalised
//!   binary vector genuinely cannot tell it apart from, and for nothing else. There are two sets,
//!   they are small on purpose, and adding to them wants a measurement.
//! * **Never an insertion or a deletion.** One character in, one character out, so the corrector
//!   cannot change what a line *says*, only which of two indistinguishable shapes it says it with.
//!   This is why `rn`/`m` and `cl`/`d` are out of scope even though [#12] lists them: they are a
//!   segmentation result — two components fused, or one split — and resolving them here would mean
//!   inventing or destroying a character on the strength of a guess about English.
//! * **Evidence on both sides.** A substitution needs a clearly-read letter or digit to the left
//!   *and* to the right, inside the same word. One-sided context is what turns `I'm` into `l'm`
//!   and `1st` into `lst`, and no amount of care over the sets prevents that.
//!
//! What the refusals cost is real, and `docs/post-correction.md` records it: a word-initial
//! ambiguous glyph has nothing to its left, so `Iazy` stays `Iazy`. Correcting it would mean
//! rewriting the first letter of `Iowa` too, and this stage does not get to make that trade.
//!
//! [#12]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/12

use std::fmt;

use subtrackt_core::{Confidence, Cue, GlyphMatch};

use crate::layout::LayoutRules;

/// A record of one substitution, so corrections are auditable rather than invisible.
///
/// One entry per character changed. A caller that logs these has the whole of what post-correction
/// did to a track, which is the price of the stage being allowed to do anything at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrectionLog {
    /// Which cue, counted over everything the pipeline assembled.
    ///
    /// Not an index into the written track. Correction runs before the unmatched-glyph policy, so
    /// a cue ahead of this one may be dropped before anything is written. Numbering by what was
    /// read rather than by what survived keeps the log pointing at the same cue whatever the
    /// policy does with it.
    pub cue: usize,
    /// Line index within the cue.
    pub line: usize,
    /// Offset of the substituted character within the line, counted in `char`s rather than bytes.
    pub at: usize,
    /// The character the matcher settled on.
    pub from: char,
    /// The character post-correction replaced it with.
    pub to: char,
    /// The whole word the substitution landed in, as it reads afterwards.
    pub word: String,
}

impl fmt::Display for CorrectionLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cue {} line {} col {}: {:?} -> {:?} in {:?}",
            self.cue, self.line, self.at, self.from, self.to, self.word
        )
    }
}

/// Rewrites text that the matcher flagged as ambiguous.
pub trait PostCorrector {
    /// Correct one cue in place, appending to `log` for every character changed.
    ///
    /// `origins` is [`crate::layout::AssembledCue::origins`]: one entry per `char` of each line of
    /// `cue`, naming the match that produced it. It is the only sound way to tell which characters
    /// were ambiguous, and an implementation that finds it does not line up with `cue` must leave
    /// the cue alone rather than correct against a mapping it cannot trust.
    ///
    /// Implementations must respect two constraints, both of which exist to keep a correction from
    /// becoming an invention:
    ///
    /// * only characters whose [`GlyphMatch::is_unambiguous`] was false may be substituted;
    /// * no character may be inserted or deleted.
    fn correct(
        &self,
        cue: &mut Cue,
        origins: &[Vec<Option<GlyphMatch>>],
        index: usize,
        log: &mut Vec<CorrectionLog>,
    );

    /// A short name for the extraction summary.
    fn name(&self) -> &'static str;
}

/// The corrector that does nothing, for a run with post-correction switched off.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopCorrector;

impl PostCorrector for NoopCorrector {
    fn correct(
        &self,
        _cue: &mut Cue,
        _origins: &[Vec<Option<GlyphMatch>>],
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

/// What the characters around an ambiguous read agree on, when they agree at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// A decimal digit.
    Digit,
    /// An uppercase letter.
    Upper,
    /// A lowercase letter.
    Lower,
}

impl Class {
    /// The class of a character, or `None` for anything carrying no evidence: punctuation, the
    /// unmatched placeholder, whitespace.
    fn of(ch: char) -> Option<Self> {
        if ch.is_ascii_digit() {
            return Some(Self::Digit);
        }
        if !ch.is_alphabetic() {
            return None;
        }
        // Unicode-wide on purpose: an accented letter is evidence about its word exactly as an
        // unaccented one is, and the fixture's `jalapeño` is a real case of it.
        if ch.is_uppercase() {
            Some(Self::Upper)
        } else {
            Some(Self::Lower)
        }
    }
}

/// One group of mutually indistinguishable shapes, indexed by character class.
struct Confusion {
    /// The group's digit.
    digit: char,
    /// Its uppercase letter.
    upper: char,
    /// Its lowercase letter.
    lower: char,
    /// Shapes with no class of their own. They can be corrected *away from* — a `|` in the middle
    /// of a word was an `l` — but never *to*, because no context ever argues for one.
    strays: &'static [char],
}

impl Confusion {
    /// Whether this group contains `ch`.
    fn contains(&self, ch: char) -> bool {
        ch == self.digit || ch == self.upper || ch == self.lower || self.strays.contains(&ch)
    }

    /// The member of this group belonging to `class`.
    const fn member(&self, class: Class) -> char {
        match class {
            Class::Digit => self.digit,
            Class::Upper => self.upper,
            Class::Lower => self.lower,
        }
    }
}

/// Characters a size-normalised binary glyph vector cannot separate.
///
/// Deliberately two entries. Every character added here is a character the corrector is newly
/// allowed to rewrite, so this table is the blast radius of the whole stage, and it should grow
/// only against a measurement. `5`/`S`, `8`/`B` and `2`/`Z` are the obvious candidates and are left
/// out until one of them is shown to fix more than it costs.
const CONFUSIONS: [Confusion; 2] = [
    Confusion { digit: '0', upper: 'O', lower: 'o', strays: &[] },
    Confusion { digit: '1', upper: 'I', lower: 'l', strays: &['|'] },
];

/// The group `ch` belongs to, if any.
fn confusion_for(ch: char) -> Option<&'static Confusion> {
    CONFUSIONS.iter().find(|set| set.contains(ch))
}

/// The word `at` sits in: the run of non-whitespace around it.
fn word_around(chars: &[char], at: usize) -> String {
    let start = chars[..at]
        .iter()
        .rposition(|c| c.is_whitespace())
        .map_or(0, |p| p + 1);
    let end = chars[at..]
        .iter()
        .position(|c| c.is_whitespace())
        .map_or(chars.len(), |p| at + p);
    chars[start..end].iter().collect()
}

/// Resolves ambiguous reads from the characters either side of them.
///
/// The whole of its judgement is this: a character the matcher could not call, sitting between two
/// it could, belongs to the same class as its neighbours. `He11o` reads as a word with two digits
/// wedged into it, and digits do not appear inside words — so the two glyphs the matcher had
/// already declined to call become `l`. Nothing here knows any English beyond that, which is the
/// point. A rule that needed a dictionary would need one per language, and would guess hardest at
/// exactly the words it had never seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextCorrector {
    /// Distance margin below which a match counts as ambiguous.
    ///
    /// Must be the margin the assembler tallied with, or the corrector and the confidence count
    /// disagree about which glyphs are even in play.
    ambiguity_margin: u32,
}

impl Default for ContextCorrector {
    fn default() -> Self {
        Self::new(LayoutRules::default().ambiguity_margin)
    }
}

impl ContextCorrector {
    /// A corrector agreeing with `ambiguity_margin` about what a close call is.
    #[must_use]
    pub const fn new(ambiguity_margin: u32) -> Self {
        Self { ambiguity_margin }
    }

    /// The margin in force.
    #[must_use]
    pub const fn ambiguity_margin(&self) -> u32 {
        self.ambiguity_margin
    }

    /// Whether the glyph behind character `at` was one the matcher could not call outright.
    fn is_ambiguous(self, origins: &[Option<GlyphMatch>], at: usize) -> bool {
        origins.get(at).is_some_and(|origin| {
            origin
                .as_ref()
                .is_some_and(|m| m.character.is_some() && !m.is_unambiguous(self.ambiguity_margin))
        })
    }

    /// What class character `at` argues for, if it argues for anything.
    ///
    /// Both halves matter. An ambiguous neighbour is not evidence — it is another glyph waiting to
    /// be decided — and a comma or a placeholder belongs to no class and says nothing either way.
    fn evidence(self, chars: &[char], origins: &[Option<GlyphMatch>], at: usize) -> Option<Class> {
        if self.is_ambiguous(origins, at) {
            return None;
        }
        Class::of(chars[at])
    }

    /// Scan outward from `at` for the nearest evidence, stopping at the word boundary.
    ///
    /// `step` is `-1` to look left and `1` to look right. Characters carrying no evidence are
    /// stepped over rather than stopped at, which is what lets the second `1` of `He11o` take its
    /// answer from the `o` beyond it.
    fn look(
        self,
        chars: &[char],
        origins: &[Option<GlyphMatch>],
        at: usize,
        step: isize,
    ) -> Option<Class> {
        let mut index = isize::try_from(at).ok()?;
        loop {
            index += step;
            let position = usize::try_from(index).ok()?;
            let ch = *chars.get(position)?;
            if ch.is_whitespace() {
                return None;
            }
            if let Some(class) = self.evidence(chars, origins, position) {
                return Some(class);
            }
        }
    }

    /// What character `at` should be, or `None` to leave it alone.
    fn resolve(
        self,
        chars: &[char],
        origins: &[Option<GlyphMatch>],
        at: usize,
    ) -> Option<(char, char)> {
        if !self.is_ambiguous(origins, at) {
            return None;
        }
        let from = chars[at];
        let set = confusion_for(from)?;

        let left = self.look(chars, origins, at, -1)?;
        let right = self.look(chars, origins, at, 1)?;
        if left != right {
            // The word says two different things about this position, so it says nothing.
            return None;
        }

        let to = set.member(left);
        (to != from).then_some((from, to))
    }
}

impl PostCorrector for ContextCorrector {
    fn correct(
        &self,
        cue: &mut Cue,
        origins: &[Vec<Option<GlyphMatch>>],
        index: usize,
        log: &mut Vec<CorrectionLog>,
    ) {
        if origins.len() != cue.lines.len() {
            // The provenance does not describe this cue. Correcting anyway would mean deciding
            // which glyphs were ambiguous by guessing, which is the one thing this stage may not
            // do, and leaving a cue as the matcher read it is always defensible.
            return;
        }

        for (line_index, (line, line_origins)) in cue.lines.iter_mut().zip(origins).enumerate() {
            let mut chars: Vec<char> = line.chars().collect();
            if chars.len() != line_origins.len() {
                continue;
            }

            // Decide everything before changing anything. Evidence only ever comes from characters
            // the matcher read clearly and corrections only ever land on ones it did not, so the
            // two sets are disjoint and a substitution can never become evidence for the next one.
            // Deciding first makes that a property of the code rather than of the iteration order.
            let changes: Vec<(usize, char, char)> = (0..chars.len())
                .filter_map(|at| {
                    self.resolve(&chars, line_origins, at)
                        .map(|(from, to)| (at, from, to))
                })
                .collect();
            if changes.is_empty() {
                continue;
            }

            for (at, _, to) in &changes {
                chars[*at] = *to;
            }
            for (at, from, to) in changes {
                log.push(CorrectionLog {
                    cue: index,
                    line: line_index,
                    at,
                    from,
                    to,
                    word: word_around(&chars, at),
                });
            }
            *line = chars.into_iter().collect();
        }
    }

    fn name(&self) -> &'static str {
        "context"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::{TimeSpan, Timestamp};

    /// A read the matcher was sure of.
    fn clear(ch: char) -> GlyphMatch {
        GlyphMatch { character: Some(ch), distance: 2, runner_up_distance: 60 }
    }

    /// A read with a runner-up inside the margin: exactly what post-correction exists for.
    fn close(ch: char) -> GlyphMatch {
        GlyphMatch { character: Some(ch), distance: 8, runner_up_distance: 9 }
    }

    fn cue(lines: &[&str]) -> Cue {
        Cue {
            span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(500)),
            lines: lines.iter().map(|l| (*l).to_owned()).collect(),
            confidence: Confidence { matched: 5, unmatched: 0, ambiguous: 2 },
            forced: false,
        }
    }

    /// Build the origins for one line, marking the characters at `ambiguous` as close calls and
    /// every other non-space character as read clearly.
    fn origins(line: &str, ambiguous: &[usize]) -> Vec<Option<GlyphMatch>> {
        line.chars()
            .enumerate()
            .map(|(at, ch)| {
                if ch == ' ' {
                    None
                } else if ambiguous.contains(&at) {
                    Some(close(ch))
                } else {
                    Some(clear(ch))
                }
            })
            .collect()
    }

    /// Correct one line and return what it became.
    fn correct(line: &str, ambiguous: &[usize]) -> (String, Vec<CorrectionLog>) {
        let mut c = cue(&[line]);
        let mut log = Vec::new();
        ContextCorrector::default().correct(&mut c, &[origins(line, ambiguous)], 0, &mut log);
        (c.lines[0].clone(), log)
    }

    #[test]
    fn digits_wedged_inside_a_word_become_the_letters_they_were_read_as() {
        assert_eq!(correct("He11o", &[2, 3]).0, "Hello");
        assert_eq!(correct("HELL0S", &[4]).0, "HELLOS");
        assert_eq!(correct("l0ve", &[1]).0, "love");
    }

    #[test]
    fn a_letter_wedged_inside_a_number_becomes_the_digit_it_was_read_as() {
        assert_eq!(correct("2O15", &[1]).0, "2015");
        assert_eq!(correct("1l1", &[1]).0, "111");
    }

    #[test]
    fn a_case_that_disagrees_with_the_rest_of_its_word_is_corrected() {
        // The error the fixture actually produces: `jalapeño` read with a capital I for the l.
        assert_eq!(correct("jaIape\u{f1}o", &[2]).0, "jalape\u{f1}o");
        assert_eq!(correct("WlLL", &[1]).0, "WILL");
    }

    #[test]
    fn a_stray_shape_is_corrected_away_from_but_never_to() {
        // `|` is in the set and has no class of its own, so context can rescue it...
        assert_eq!(correct("ja|apeno", &[2]).0, "jalapeno");
        // ...but nothing ever argues for it, so no correction can produce one.
        for set in &CONFUSIONS {
            for class in [Class::Digit, Class::Upper, Class::Lower] {
                assert!(!set.strays.contains(&set.member(class)));
            }
        }
    }

    #[test]
    fn a_glyph_the_matcher_read_clearly_is_never_touched() {
        // The first constraint of the whole stage. `He11o` with the digits read confidently is a
        // line that says `He11o`, and this is not the place to disagree.
        let (text, log) = correct("He11o", &[]);
        assert_eq!(text, "He11o");
        assert!(log.is_empty());
    }

    #[test]
    fn a_word_initial_ambiguous_glyph_is_left_alone_however_much_it_looks_wrong() {
        // `Iazy` is `lazy` and everyone can see it. Correcting it means correcting the `I` of
        // `Iowa` on identical evidence, so the answer is to leave both and say so.
        assert_eq!(correct("Iazy", &[0]).0, "Iazy");
        assert_eq!(correct("Iowa", &[0]).0, "Iowa");
    }

    #[test]
    fn a_word_final_ambiguous_glyph_is_left_alone_too() {
        // Same rule from the other end, and it is what keeps `l` a plausible last letter.
        assert_eq!(correct("wilI", &[3]).0, "wilI");
        assert_eq!(correct("HELL0", &[4]).0, "HELL0");
    }

    #[test]
    fn one_sided_context_never_decides_anything() {
        // The two cases that make one-sided context indefensible, whatever it would buy elsewhere.
        assert_eq!(correct("I'm", &[0]).0, "I'm");
        assert_eq!(correct("1st", &[0]).0, "1st");
    }

    #[test]
    fn a_word_whose_sides_disagree_is_left_alone() {
        // `No.1` reads a letter to the left and a digit to the right; a rule that broke the tie
        // would be inventing the answer rather than reading it.
        assert_eq!(correct("No.1", &[1]).0, "No.1");
        assert_eq!(correct("H2O", &[1]).0, "H2O");
    }

    #[test]
    fn a_character_outside_the_confusion_sets_is_never_substituted() {
        // Ambiguous, surrounded, and still none of this stage's business: `S` is not in a set.
        assert_eq!(correct("aSa", &[1]).0, "aSa");
        assert_eq!(correct("1S1", &[1]).0, "1S1");
    }

    #[test]
    fn correction_never_changes_how_many_characters_a_line_has() {
        // The constraint that keeps a correction from becoming an invention. `rn`/`m` and `cl`/`d`
        // are excluded by this and not by an oversight.
        for (line, ambiguous) in [("He11o", &[2, 3][..]), ("2O15", &[1]), ("jaIapeno", &[2])] {
            let (corrected, _) = correct(line, ambiguous);
            assert_eq!(corrected.chars().count(), line.chars().count(), "{line}");
        }
    }

    #[test]
    fn an_unmatched_glyph_is_neither_corrected_nor_treated_as_evidence() {
        // A placeholder is the honest report of a glyph nothing matched. Reading a class out of it
        // would let an unread glyph decide a neighbour it knows nothing about.
        let mut c = cue(&["a\u{fffd}1a"]);
        let origins = vec![
            Some(clear('a')),
            Some(GlyphMatch::unmatched(200)),
            Some(close('1')),
            Some(clear('a')),
        ];

        let mut log = Vec::new();
        ContextCorrector::default().correct(&mut c, &[origins], 0, &mut log);
        assert_eq!(
            c.lines[0], "a\u{fffd}la",
            "the `a` beyond it is the evidence; the placeholder is neither evidence nor a target"
        );
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn every_correction_is_logged_with_where_it_happened_and_what_it_changed() {
        let (text, log) = correct("The Ye11ow car", &[6, 7]);
        assert_eq!(text, "The Yellow car");
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].from, '1');
        assert_eq!(log[0].to, 'l');
        assert_eq!(log[0].at, 6);
        assert_eq!(log[0].line, 0);
        assert_eq!(
            log[0].word, "Yellow",
            "the log names the corrected word, not the raw one"
        );
        assert!(log[0].to_string().contains("col 6"), "{}", log[0]);
        assert!(log[1].to_string().contains("'1' -> 'l'"), "{}", log[1]);
    }

    #[test]
    fn a_cue_whose_provenance_does_not_line_up_is_left_untouched() {
        // Correcting against a mapping the corrector cannot trust is how a stage that must not
        // invent text ends up inventing text. Two ways it can fail to line up, both refused.
        let mut wrong_lines = cue(&["He11o"]);
        let mut log = Vec::new();
        ContextCorrector::default().correct(&mut wrong_lines, &[], 0, &mut log);
        assert_eq!(wrong_lines.lines[0], "He11o", "no origins for the line");

        let mut short = cue(&["He11o"]);
        ContextCorrector::default().correct(&mut short, &[vec![Some(clear('H'))]], 0, &mut log);
        assert_eq!(short.lines[0], "He11o", "origins shorter than the line");
        assert!(log.is_empty());
    }

    #[test]
    fn each_line_of_a_cue_is_corrected_against_its_own_context() {
        let lines = ["He11o", "2O15"];
        let mut c = cue(&lines);
        let mut log = Vec::new();
        let origins = vec![origins(lines[0], &[2, 3]), origins(lines[1], &[1])];

        ContextCorrector::default().correct(&mut c, &origins, 4, &mut log);
        assert_eq!(c.lines, vec!["Hello".to_owned(), "2015".to_owned()]);
        assert_eq!(log.len(), 3);
        assert!(log.iter().all(|entry| entry.cue == 4));
        assert_eq!(log[2].line, 1, "the second line's correction says so");
    }

    #[test]
    fn the_default_corrector_changes_nothing_and_logs_nothing() {
        let mut c = cue(&["He11o"]);
        let before = c.clone();
        let mut log = Vec::new();
        NoopCorrector.correct(&mut c, &[origins("He11o", &[2, 3])], 0, &mut log);
        assert_eq!(c, before);
        assert!(log.is_empty());
        assert_eq!(NoopCorrector.name(), "none");
        assert_eq!(ContextCorrector::default().name(), "context");
    }

    #[test]
    fn the_corrector_and_the_confidence_tally_agree_on_what_a_close_call_is() {
        // One source of truth, or a cue reads as clean while the corrector rewrites it anyway.
        assert_eq!(
            ContextCorrector::default().ambiguity_margin(),
            LayoutRules::default().ambiguity_margin
        );
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
