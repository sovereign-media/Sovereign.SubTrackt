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

use std::collections::HashMap;
use std::fmt;

use subtrackt_core::{Confidence, Cue, GlyphMatch};

use crate::layout::{AssembledCue, LayoutRules};

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
    /// Which arm of the corrector fired.
    pub rule: CorrectionRule,
}

/// Which arm of [`ContextCorrector`] produced a substitution.
///
/// Carried per correction rather than counted per run, because the two arms rest on different
/// evidence and a summary that added them together would hide which one a bad substitution came
/// from. `Report` splits the count for the same reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorrectionRule {
    /// The characters either side of the glyph, within its word.
    Context,
    /// A token the same track read clearly elsewhere, matched case-folded.
    Vocabulary {
        /// The clear token that decided it.
        token: String,
        /// How many times the track read that token clearly.
        occurrences: usize,
    },
}

impl fmt::Display for CorrectionLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cue {} line {} col {}: {:?} -> {:?} in {:?}",
            self.cue, self.line, self.at, self.from, self.to, self.word
        )?;
        // A rule that fires on evidence has to show the evidence, or it is a dictionary with extra
        // steps. The context arm's evidence is the word itself and is already printed.
        if let CorrectionRule::Vocabulary { token, occurrences } = &self.rule {
            write!(f, " (vocabulary: {token:?} x{occurrences})")?;
        }
        Ok(())
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

    /// Look at the whole track before correcting any of it.
    ///
    /// Called once, with every cue the pipeline assembled, before [`correct`](Self::correct) runs
    /// on any of them. A decision needing the whole track cannot be made while answers are already
    /// being handed out — the same argument that makes `GlyphMatcher::prepare` a separate pass.
    ///
    /// Defaulted to nothing, so a corrector that needs no such pass is unaffected.
    fn observe(&mut self, cues: &[AssembledCue]) {
        let _ = cues;
    }

    /// How many distinct clear tokens the corrector learned, for reporting.
    ///
    /// Zero for a corrector that learns nothing, which is what makes "the rule never fired because
    /// nothing supported it" distinguishable from "the rule fired and gained nothing".
    fn vocabulary_size(&self) -> usize {
        0
    }

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

    /// Every character in the group, which is every candidate a substitution may produce.
    fn members(&self) -> Vec<char> {
        let mut out = vec![self.digit, self.upper, self.lower];
        out.extend_from_slice(self.strays);
        out
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

/// How a track's own clear vocabulary is built and consulted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VocabularyRules {
    /// How many times a token must be read *clearly* before it counts as evidence.
    ///
    /// One clear occurrence that was itself a misread becomes evidence for a substitution, which is
    /// the failure mode this exists for. The default is a guess and the sweep is what should settle
    /// it.
    pub min_occurrences: usize,
    /// Shortest token that may be corrected, in characters.
    ///
    /// A bare `l` is not a word, and a two-character token folds onto far too much.
    pub min_len: usize,
    /// Whether a candidate may match the *start* of a clear token rather than all of it.
    ///
    /// A track says `Looking`, `looks` and `looked` more often than any one of them, so exact
    /// matching leaves evidence on the table. The obvious reach is a stemmer, and it is the wrong
    /// reach: a lemmatizer carries a lexicon, which is the dictionary objection wearing a hat, and
    /// a stemmer over-collapses by design — `universe` and `university` share a stem.
    ///
    /// Prefix matching gets most of it with no dependency and no knowledge of any language. It
    /// over-matches, and over-matching is harmless *here*: the substitution decides one character
    /// within a confusion set, not which word the line holds. Any clear token extending `line`
    /// argues for `l` at position zero, which is the only thing being asked.
    pub prefix_match: bool,
}

impl Default for VocabularyRules {
    fn default() -> Self {
        // All three measured on a real Blu-ray, in `docs/post-correction.md`.
        //
        // `min_occurrences` at one because raising it to two costs four correct substitutions and
        // prevents none: nothing was ever made worse at any setting, so the guard it offers is
        // against a failure that has not been observed. `min_len` makes no difference on that
        // track at all — every token the arm fired on was four characters or more — and stays at
        // two because a one-character token folds onto far too much.
        //
        // `prefix_match` on because it found nine substitutions against exact matching's seven,
        // with zero cues made worse either way. It over-matches by design, and over-matching is
        // harmless here: the substitution decides one character within a confusion set, not which
        // word the line holds.
        Self { min_occurrences: 1, min_len: 2, prefix_match: true }
    }
}

/// The tokens a track read clearly, case-folded, with how often each occurred.
///
/// Evidence from the material rather than an assertion about a language. `docs/post-correction.md`
/// rules out a dictionary — it is unverifiable, English-only, and guesses hardest at names and
/// invented nouns, which is where a subtitle is least replaceable. A word the *same track* already
/// read clearly is none of those things.
///
/// Empty by default, and an empty vocabulary reproduces the behaviour that existed before it, which
/// is what makes every test written against the context arm a regression guard.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vocabulary {
    tokens: HashMap<String, usize>,
    rules: VocabularyRules,
}

impl Vocabulary {
    /// An empty vocabulary, which corrects nothing.
    #[must_use]
    pub fn new(rules: VocabularyRules) -> Self {
        Self { tokens: HashMap::new(), rules }
    }

    /// Build from every cue the pipeline assembled, before any correction has touched them.
    ///
    /// A token is admitted only if **every** character in it came from a match that named a
    /// character *and* was unambiguous. No placeholders and no close calls — which keeps the
    /// evidence set and the correction set disjoint, exactly as the context arm already does, so no
    /// substitution can become the evidence for the next one.
    #[must_use]
    pub fn observed(cues: &[AssembledCue], rules: VocabularyRules, ambiguity_margin: u32) -> Self {
        let mut tokens: HashMap<String, usize> = HashMap::new();

        for assembled in cues {
            for (line, origins) in assembled.cue.lines.iter().zip(&assembled.origins) {
                let chars: Vec<char> = line.chars().collect();
                if chars.len() != origins.len() {
                    // The provenance does not describe this line, so nothing here can be called
                    // clear. Skipping it costs evidence; trusting it would invent some.
                    continue;
                }
                for (start, end) in token_spans(&chars) {
                    let clear = (start..end).all(|at| {
                        origins[at].as_ref().is_some_and(|m| {
                            m.character.is_some() && m.is_unambiguous(ambiguity_margin)
                        })
                    });
                    if !clear {
                        continue;
                    }
                    let token: String = chars[start..end]
                        .iter()
                        .flat_map(|c| c.to_lowercase())
                        .collect();
                    if token.chars().count() >= rules.min_len {
                        *tokens.entry(token).or_insert(0) += 1;
                    }
                }
            }
        }

        Self { tokens, rules }
    }

    /// Whether anything was learned at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// How many distinct tokens were read clearly.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// The clear token supporting `candidate`, if exactly one does.
    ///
    /// Case-folded on the way in. Returns the token and its count so the correction log can show
    /// the evidence rather than assert it.
    fn support(&self, candidate: &str) -> Option<(&str, usize)> {
        if self.rules.prefix_match {
            let mut found: Option<(&str, usize)> = None;
            for (token, count) in &self.tokens {
                if *count >= self.rules.min_occurrences && token.starts_with(candidate) {
                    // Several clear tokens can extend one candidate — `line` sits under `linear`
                    // and `lines` — and they all argue for the same character. The shortest is
                    // reported because it is the closest thing to what was actually read.
                    if found.is_none_or(|(best, _)| token.len() < best.len()) {
                        found = Some((token.as_str(), *count));
                    }
                }
            }
            return found;
        }
        self.tokens
            .get_key_value(candidate)
            .filter(|(_, count)| **count >= self.rules.min_occurrences)
            .map(|(token, count)| (token.as_str(), *count))
    }
}

/// The alphanumeric-bounded token spans in a line.
///
/// Splits on whitespace, then trims leading and trailing characters that are neither alphanumeric
/// nor part of a word — so `lazy,` yields `lazy` and `"don't"` yields `don't`. Internal apostrophes
/// and hyphens survive because they are part of the word a track would repeat.
fn token_spans(chars: &[char]) -> Vec<(usize, usize)> {
    let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '-';
    let mut spans = Vec::new();
    let mut at = 0usize;

    while at < chars.len() {
        if chars[at].is_whitespace() {
            at += 1;
            continue;
        }
        let run_end = chars[at..]
            .iter()
            .position(|c| c.is_whitespace())
            .map_or(chars.len(), |p| at + p);

        let mut start = at;
        let mut end = run_end;
        while start < end && !chars[start].is_alphanumeric() {
            start += 1;
        }
        while end > start && !chars[end - 1].is_alphanumeric() {
            end -= 1;
        }
        if start < end && chars[start..end].iter().copied().all(is_word) {
            spans.push((start, end));
        }
        at = run_end;
    }
    spans
}

/// Resolves ambiguous reads from the characters either side of them.
///
/// The whole of its judgement is this: a character the matcher could not call, sitting between two
/// it could, belongs to the same class as its neighbours. `He11o` reads as a word with two digits
/// wedged into it, and digits do not appear inside words — so the two glyphs the matcher had
/// already declined to call become `l`. Nothing here knows any English beyond that, which is the
/// point. A rule that needed a dictionary would need one per language, and would guess hardest at
/// exactly the words it had never seen.
/// Loses `Copy` with the vocabulary, which owns a map. Every method that used to take `self` by
/// value takes `&self` instead; nothing else about them changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextCorrector {
    /// Distance margin below which a match counts as ambiguous.
    ///
    /// Must be the margin the assembler tallied with, or the corrector and the confidence count
    /// disagree about which glyphs are even in play.
    ambiguity_margin: u32,
    /// What the track read clearly, consulted only when the context arm declines.
    ///
    /// Empty unless [`observe`](PostCorrector::observe) filled it, and an empty one reproduces the
    /// behaviour that existed before this arm did.
    vocabulary: Vocabulary,
    /// Whether the vocabulary arm runs at all.
    use_vocabulary: bool,
    /// How the vocabulary is built, kept so `observe` can build it.
    vocabulary_rules: VocabularyRules,
}

impl Default for ContextCorrector {
    fn default() -> Self {
        Self::new(LayoutRules::default().ambiguity_margin)
    }
}

impl ContextCorrector {
    /// A corrector agreeing with `ambiguity_margin` about what a close call is.
    #[must_use]
    pub fn new(ambiguity_margin: u32) -> Self {
        Self {
            ambiguity_margin,
            vocabulary: Vocabulary::default(),
            use_vocabulary: false,
            vocabulary_rules: VocabularyRules::default(),
        }
    }

    /// Enable the track-vocabulary arm, with the rules it should be built under.
    #[must_use]
    pub fn with_vocabulary(mut self, rules: VocabularyRules) -> Self {
        self.use_vocabulary = true;
        self.vocabulary_rules = rules;
        self
    }

    /// The vocabulary in force, for reporting how much evidence existed at all.
    #[must_use]
    pub const fn vocabulary(&self) -> &Vocabulary {
        &self.vocabulary
    }

    /// The margin in force.
    #[must_use]
    pub const fn ambiguity_margin(&self) -> u32 {
        self.ambiguity_margin
    }

    /// Whether the glyph behind character `at` was one the matcher could not call outright.
    fn is_ambiguous(&self, origins: &[Option<GlyphMatch>], at: usize) -> bool {
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
    fn evidence(&self, chars: &[char], origins: &[Option<GlyphMatch>], at: usize) -> Option<Class> {
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
        &self,
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
    ///
    /// Two arms, in order. The context arm decides from the characters either side; the vocabulary
    /// arm runs **only when it declines**, so whatever the vocabulary adds is strictly on top of
    /// the behaviour that was measured without it.
    fn resolve(
        &self,
        chars: &[char],
        origins: &[Option<GlyphMatch>],
        at: usize,
    ) -> Option<(char, char, CorrectionRule)> {
        if !self.is_ambiguous(origins, at) {
            return None;
        }
        let from = chars[at];
        let set = confusion_for(from)?;

        if let (Some(left), Some(right)) =
            (self.look(chars, origins, at, -1), self.look(chars, origins, at, 1))
            && left == right
        {
            // The word says one thing about this position, and that is the whole context rule.
            let to = set.member(left);
            if to != from {
                return Some((from, to, CorrectionRule::Context));
            }
            return None;
        }

        self.resolve_from_vocabulary(chars, at, from, set)
    }

    /// The second arm: a token the track itself read clearly, matched case-folded.
    ///
    /// The context arm cannot fire at a word edge, because it needs evidence on *both* sides —
    /// which is why `Iazy` stays wrong and why `Iowa` stays right, on identical evidence. This arm
    /// reaches for different evidence: `Look` read clearly elsewhere argues that `Iook` is `look`,
    /// and `I` folds to `i` so the reading itself is one of the candidates.
    ///
    /// Nothing folds onto `iowa`, so `Iowa` is refused — on the *absence* of evidence rather than
    /// on a threshold, which is what keeps the refusal honest.
    fn resolve_from_vocabulary(
        &self,
        chars: &[char],
        at: usize,
        from: char,
        set: &Confusion,
    ) -> Option<(char, char, CorrectionRule)> {
        if !self.use_vocabulary || self.vocabulary.is_empty() {
            return None;
        }
        let (start, end) = token_spans(chars)
            .into_iter()
            .find(|(start, end)| (*start..*end).contains(&at))?;
        if end - start < self.vocabulary.rules.min_len {
            return None;
        }

        // One candidate per member of the confusion set, substituted at `at` and nowhere else. The
        // no-insertion invariant is unchanged, and the other ambiguous characters in the word stay
        // as-read — a named cost, like two-sided evidence, and the non-cascading choice.
        let mut winner: Option<(char, &str, usize)> = None;
        let mut found = 0usize;
        for candidate in set.members() {
            let folded: String = chars[start..end]
                .iter()
                .enumerate()
                .flat_map(|(offset, c)| {
                    let c = if start + offset == at { candidate } else { *c };
                    c.to_lowercase().collect::<Vec<char>>()
                })
                .collect();
            if let Some((token, count)) = self.vocabulary.support(&folded) {
                found += 1;
                winner = Some((candidate, token, count));
            }
        }

        // Exactly one. Zero is no evidence and two is contradictory evidence, and both are refusals.
        if found != 1 {
            return None;
        }
        let (to, token, occurrences) = winner?;
        (to != from).then(|| {
            (
                from,
                to,
                CorrectionRule::Vocabulary { token: token.to_owned(), occurrences },
            )
        })
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
            let changes: Vec<(usize, char, char, CorrectionRule)> = (0..chars.len())
                .filter_map(|at| {
                    self.resolve(&chars, line_origins, at)
                        .map(|(from, to, rule)| (at, from, to, rule))
                })
                .collect();
            if changes.is_empty() {
                continue;
            }

            for (at, _, to, _) in &changes {
                chars[*at] = *to;
            }
            for (at, from, to, rule) in changes {
                log.push(CorrectionLog {
                    cue: index,
                    line: line_index,
                    at,
                    from,
                    to,
                    word: word_around(&chars, at),
                    rule,
                });
            }
            *line = chars.into_iter().collect();
        }
    }

    fn observe(&mut self, cues: &[AssembledCue]) {
        if self.use_vocabulary {
            self.vocabulary =
                Vocabulary::observed(cues, self.vocabulary_rules, self.ambiguity_margin);
        }
    }

    fn vocabulary_size(&self) -> usize {
        self.vocabulary.len()
    }

    fn name(&self) -> &'static str {
        if self.use_vocabulary {
            "context+vocabulary"
        } else {
            "context"
        }
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

    /// A corrector with a vocabulary learned from `evidence`, every character of it read clearly.
    fn with_clear_vocabulary(evidence: &[&str], rules: VocabularyRules) -> ContextCorrector {
        let cues: Vec<AssembledCue> = evidence
            .iter()
            .map(|line| AssembledCue { cue: cue(&[line]), origins: vec![origins(line, &[])] })
            .collect();
        let mut corrector = ContextCorrector::new(4).with_vocabulary(rules);
        corrector.observe(&cues);
        corrector
    }

    /// Correct one line and return what it reads afterwards, with the log.
    fn corrected(
        corrector: &ContextCorrector,
        line: &str,
        ambiguous: &[usize],
    ) -> (String, Vec<CorrectionLog>) {
        let mut subject = cue(&[line]);
        let mut log = Vec::new();
        corrector.correct(&mut subject, &[origins(line, ambiguous)], 0, &mut log);
        (subject.lines[0].clone(), log)
    }

    #[test]
    fn a_word_edge_glyph_is_corrected_from_a_word_the_track_read_clearly() {
        // #60's case. The context arm cannot fire at a word edge because it needs evidence on both
        // sides — `Iazy` has nothing to its left. A clear `Lazy` elsewhere in the track folds onto
        // the same token and settles it.
        let corrector = with_clear_vocabulary(&["Lazy dogs"], VocabularyRules::default());
        let (line, log) = corrected(&corrector, "over the Iazy dog", &[9]);

        assert_eq!(line, "over the lazy dog");
        assert_eq!(log.len(), 1);
        assert!(
            matches!(&log[0].rule, CorrectionRule::Vocabulary { token, .. } if token == "lazy"),
            "the evidence is named in the log rather than asserted: {:?}",
            log[0].rule
        );
    }

    #[test]
    fn a_proper_noun_with_no_clear_twin_is_left_alone() {
        // The refusal that matters, and the reason it is safe: it rests on the *absence* of
        // evidence rather than on a threshold. Nothing in this track folds onto `iowa`, so nothing
        // argues for changing it — and the evidence for rewriting `Iowa` is otherwise identical to
        // the evidence for rewriting `Iazy`.
        let corrector =
            with_clear_vocabulary(&["Lazy dogs", "Look here"], VocabularyRules::default());
        let (line, log) = corrected(&corrector, "to Iowa in 2015", &[3]);

        assert_eq!(line, "to Iowa in 2015", "left exactly as the matcher read it");
        assert!(log.is_empty());
    }

    #[test]
    fn contradictory_evidence_is_a_refusal_rather_than_a_coin_toss() {
        // Two candidates both supported says the vocabulary knows two words that fit, which is not
        // evidence for either. Zero and two are both refusals; only exactly one is a correction.
        let corrector =
            with_clear_vocabulary(&["lit lamps", "1it code"], VocabularyRules::default());
        let (line, log) = corrected(&corrector, "the Iit sign", &[4]);

        assert_eq!(line, "the Iit sign");
        assert!(log.is_empty());
    }

    #[test]
    fn an_empty_vocabulary_reproduces_the_context_arm_exactly() {
        // The regression guard the whole design rests on: the second arm runs only where the first
        // declined, so with nothing learned the behaviour is the one that was measured before it.
        let bare = ContextCorrector::new(4);
        let empty = ContextCorrector::new(4).with_vocabulary(VocabularyRules::default());

        for line in ["over the Iazy dog", "He11o there", "to Iowa in 2015"] {
            let ambiguous: Vec<usize> = line
                .char_indices()
                .filter(|(_, c)| matches!(c, 'I' | '1' | 'l' | 'O' | '0'))
                .map(|(at, _)| line[..at].chars().count())
                .collect();
            assert_eq!(
                corrected(&bare, line, &ambiguous).0,
                corrected(&empty, line, &ambiguous).0,
                "{line:?} must read the same with an empty vocabulary"
            );
        }
    }

    #[test]
    fn the_vocabulary_only_admits_words_every_character_of_which_was_read_clearly() {
        // The evidence set and the correction set stay disjoint, exactly as the context arm keeps
        // them. A token containing an ambiguous glyph is not evidence — it is another glyph waiting
        // to be decided — so a track cannot talk itself into a substitution.
        let line = "Lazy dogs";
        let cues = vec![AssembledCue {
            cue: cue(&[line]),
            // The `L` is a close call, so `lazy` must not be learned.
            origins: vec![origins(line, &[0])],
        }];
        let mut corrector = ContextCorrector::new(4).with_vocabulary(VocabularyRules::default());
        corrector.observe(&cues);

        assert_eq!(
            corrector.vocabulary().len(),
            1,
            "only `dogs` was read clearly throughout"
        );
        assert_eq!(corrected(&corrector, "over the Iazy dog", &[9]).0, "over the Iazy dog");
    }

    #[test]
    fn a_token_shorter_than_the_minimum_is_not_corrected() {
        // A bare `l` is not a word, and a one-character token folds onto far too much.
        let corrector = with_clear_vocabulary(&["a lot"], VocabularyRules::default());
        assert_eq!(corrected(&corrector, "x I y", &[2]).0, "x I y");
    }

    #[test]
    fn prefix_matching_finds_the_inflected_form_a_track_actually_says() {
        // A track says `Looking` far more often than `look`. Prefix matching gets that with no
        // stemmer and no lexicon, and it over-matches harmlessly: any clear token extending `look`
        // argues for `l` at position zero, which is the only thing being asked.
        let corrector = with_clear_vocabulary(&["Looking around"], VocabularyRules::default());
        let (line, log) = corrected(&corrector, "Iook out", &[0]);

        assert_eq!(line, "look out");
        assert!(matches!(
            &log[0].rule,
            CorrectionRule::Vocabulary { token, .. } if token == "looking"
        ));

        // And exact matching, which prefix matching was measured against, does not.
        let exact = VocabularyRules { prefix_match: false, ..VocabularyRules::default() };
        let exact = with_clear_vocabulary(&["Looking around"], exact);
        assert_eq!(corrected(&exact, "Iook out", &[0]).0, "Iook out");
    }

    #[test]
    fn a_rarely_seen_token_can_be_required_to_repeat_before_it_counts() {
        // One clear occurrence that was itself a misread becomes evidence for a substitution, which
        // is the failure `min_occurrences` exists for.
        let rules = VocabularyRules { min_occurrences: 2, ..VocabularyRules::default() };
        let once = with_clear_vocabulary(&["Lazy dogs"], rules);
        assert_eq!(corrected(&once, "over the Iazy dog", &[9]).0, "over the Iazy dog");

        let twice = with_clear_vocabulary(&["Lazy dogs", "Lazy cats"], rules);
        assert_eq!(corrected(&twice, "over the Iazy dog", &[9]).0, "over the lazy dog");
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
