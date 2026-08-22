//! Scoring read text against a character-bigram model of its declared language.
//!
//! [#101](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/101). `docs/fit-confidence.md`
//! closes #63 on five statistics that all failed, and names the one thing left untried: evidence
//! from **outside the candidate set entirely**. This is that, built to be neither of the two things
//! that objection usually comes with — it is not a dictionary, and it does not add a dependency.
//!
//! ## Why a permutation is the thing to look for
//!
//! A systematically wrong reference set produces a **near-permutation of the alphabet**:
//! `reference.rs` records the observed case, `t` read as `I` everywhere and `1` as `4`. A
//! permutation preserves the letter-frequency histogram *exactly*, which is why no self-derived,
//! language-free statistic can see it — every self-consistency measure is invariant under it.
//!
//! What a permutation destroys is **bigram structure**: `Tl`, `Ie`, `ln` where `Th`, `he`, `in`
//! belong. That is the standard cryptanalytic separation between a substitution cipher and
//! plaintext, it needs a language prior and nothing else, and it is the one signal none of the five
//! could reach.
//!
//! ## Why it survives both mechanisms that killed the five
//!
//! `docs/fit-confidence.md` leaves a standing filter: *does the statistic consult the reference
//! glyph the matcher chose for this query glyph?*
//!
//! **It does not.** It reads the distribution of *output characters* and never the cost that
//! produced them, so "a systematically wrong set is by construction a low-distance one" — the
//! mechanism that killed the first four — has nothing to bite on. And it is not measured off
//! decoded ink, so the noise floor that killed the fifth (a track sits 0.37–0.76 from its own font
//! while the closest font pairs sit 0.22 apart) is a property of a channel this never touches.
//!
//! ## Two constraints, built in from the start
//!
//! - **Silence, not a number, when the prior does not apply.** [`Table::score`] returns `None`
//!   rather than a figure when there is too little Latin-script text to score. Same choice
//!   `LineMetrics::UNKNOWN` makes for an unmeasurable line, and for the same reason: a fabricated
//!   figure is indistinguishable from a real one and would quietly bias the decision.
//! - **This can only ever produce a warning.** It ranks a read against a language, not against
//!   ground truth. #62's outcome — the fitted set is a proposal a user accepts — does not change.

/// Letters the model covers.
///
/// The 26 Latin letters, case-folded, with accented Latin-1 forms folded onto their base letter.
/// Digits and punctuation break the chain rather than being binned: a subtitle's `?` says nothing
/// about whether the alphabet was permuted. See [`index`] for why the accents do not get the same
/// treatment.
const LETTERS: usize = 26;

/// Add-k smoothing, in the same units the counts are in.
///
/// One count, which is the standard Laplace choice. What it is doing here is bounding the penalty a
/// single unseen bigram can impose: without it, one `qz` would send the whole score to negative
/// infinity and the statistic would be a detector for rare letters rather than for permutations.
const SMOOTHING: f64 = 1.0;

/// Fewest scorable bigrams before a score means anything.
///
/// A cue or two of text can be unrepresentative by chance. This is a track-level statistic, and a
/// track has thousands; the floor is here so that a caller who hands over a fragment gets silence.
const MIN_BIGRAMS: usize = 200;

/// A character-bigram log-probability table for one language.
///
/// `log_p[a][b]` is the natural log of P(next letter is `b` | this letter is `a`). Roughly 700
/// bytes once quantised, which is the size #101 promises: 26 x 26 values.
pub struct Table {
    log_p: [[f64; LETTERS]; LETTERS],
}

/// The index of a letter, or `None` for anything outside the covered alphabet.
///
/// Accented Latin-1 letters fold onto their base letter, and that is not a convenience. A wrong
/// reference set substitutes an accented form for a plain one constantly — Trebuchet material read
/// with a Corbel set comes out as `Foííow the yeííow íine` — and a model that treated `í` as
/// unscorable would **drop those bigrams from the average entirely**, scoring a bad read on the
/// fragments between its own mistakes. That is the mechanism `docs/fit-confidence.md` records for
/// mean match distance, arriving a third time: the thing that makes the answer wrong is the thing
/// that removes it from the evidence.
fn index(c: char) -> Option<usize> {
    let folded = match c {
        '\u{c0}'..='\u{c5}' | '\u{e0}'..='\u{e5}' => 'a',
        '\u{c7}' | '\u{e7}' => 'c',
        '\u{c8}'..='\u{cb}' | '\u{e8}'..='\u{eb}' => 'e',
        '\u{cc}'..='\u{cf}' | '\u{ec}'..='\u{ef}' => 'i',
        '\u{d1}' | '\u{f1}' => 'n',
        '\u{d2}'..='\u{d6}' | '\u{f2}'..='\u{f6}' => 'o',
        '\u{d9}'..='\u{dc}' | '\u{f9}'..='\u{fc}' => 'u',
        '\u{dd}' | '\u{fd}' | '\u{ff}' => 'y',
        '\u{df}' => 's',
        other => other.to_ascii_lowercase(),
    };
    folded
        .is_ascii_lowercase()
        .then(|| folded as usize - 'a' as usize)
}

impl Table {
    /// Build a table by counting bigrams in a corpus.
    ///
    /// Letters only, case-folded, and a run of anything else **breaks** the chain rather than
    /// being skipped over — so `to be` contributes `to` and `be` but never `ob`. Skipping instead
    /// would teach the model bigrams that never occur inside a word, which is exactly the structure
    /// it exists to notice the absence of.
    #[must_use]
    pub fn from_corpus(corpus: &str) -> Self {
        let mut counts = [[0u64; LETTERS]; LETTERS];
        let mut previous: Option<usize> = None;
        for c in corpus.chars() {
            match index(c) {
                Some(current) => {
                    if let Some(before) = previous {
                        counts[before][current] += 1;
                    }
                    previous = Some(current);
                }
                None => previous = None,
            }
        }

        let mut log_p = [[0.0f64; LETTERS]; LETTERS];
        for (row, out) in counts.iter().zip(log_p.iter_mut()) {
            #[allow(clippy::cast_precision_loss)]
            let total = row.iter().sum::<u64>() as f64 + SMOOTHING * LETTERS as f64;
            for (count, cell) in row.iter().zip(out.iter_mut()) {
                #[allow(clippy::cast_precision_loss)]
                let p = (*count as f64 + SMOOTHING) / total;
                *cell = p.ln();
            }
        }
        Self { log_p }
    }

    /// Mean log-probability per bigram of `text`, or `None` if there is too little to score.
    ///
    /// The mean rather than the sum, so a long track and a short one are comparable. Higher is more
    /// English-like; the scale is natural log, so `-2.5` is roughly "the average letter had one of
    /// twelve plausible successors" and `-3.26` is the floor a uniform alphabet would give.
    #[must_use]
    pub fn score(&self, text: &str) -> Option<f64> {
        let mut total = 0.0f64;
        let mut scored = 0usize;
        let mut previous: Option<usize> = None;
        for c in text.chars() {
            match index(c) {
                Some(current) => {
                    if let Some(before) = previous {
                        total += self.log_p[before][current];
                        scored += 1;
                    }
                    previous = Some(current);
                }
                None => previous = None,
            }
        }
        #[allow(clippy::cast_precision_loss)]
        (scored >= MIN_BIGRAMS).then(|| total / scored as f64)
    }

    /// As [`Self::score`], with every unread character charged the uniform floor.
    ///
    /// The repair #63 already made once, one stage downstream. Its mean match distance averaged
    /// over the glyphs that *matched*, so "a set that recognises a tenth of the track at close
    /// range scores better than one that recognises all of it at medium range"; the fix was to
    /// charge every unread glyph the match ceiling. [`Self::score`] has exactly that shape — a
    /// replacement character is not a Latin letter, so it **breaks the chain** and the bigrams that
    /// survive to be scored are only the ones the set was confident about.
    ///
    /// So each replacement character is charged [`Self::uniform_floor`], which is what a bigram
    /// drawn from a uniform alphabet is worth, and counted. That is a *lower bound* on what it
    /// actually cost — the glyph was rejected outright, so a correct read of it would have scored
    /// higher — which keeps the statistic honest in the direction that matters, exactly as #63's
    /// charged mean does.
    #[must_use]
    pub fn score_charged(&self, text: &str) -> Option<f64> {
        let mut total = 0.0f64;
        let mut scored = 0usize;
        let mut previous: Option<usize> = None;
        for c in text.chars() {
            if c == REPLACEMENT {
                total += Self::uniform_floor();
                scored += 1;
                previous = None;
                continue;
            }
            match index(c) {
                Some(current) => {
                    if let Some(before) = previous {
                        total += self.log_p[before][current];
                        scored += 1;
                    }
                    previous = Some(current);
                }
                None => previous = None,
            }
        }
        #[allow(clippy::cast_precision_loss)]
        (scored >= MIN_BIGRAMS).then(|| total / scored as f64)
    }

    /// What a table over a uniform alphabet would score: the floor any real text must beat.
    ///
    /// Printed beside the measured scores so a reader can tell "English" from "not obviously
    /// anything", rather than having to calibrate an unfamiliar unit against the other rows.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn uniform_floor() -> f64 {
        (1.0f64 / LETTERS as f64).ln()
    }
}

/// What the pipeline writes where a glyph matched nothing.
const REPLACEMENT: char = '�';

/// The corpus the table is derived from: the Declaration of Independence, 1776.
///
/// Public domain by age and by authorship, reproducible exactly, and long enough that the dominant
/// English bigrams — `th`, `he`, `in`, `er`, `an`, `re` — are all well attested. #101 asks for a
/// public-domain text derived **at bench time** and for nothing to be embedded in the library
/// unless the statistic clears #63's bar, so this lives in xtask.
///
/// Its register is nothing like subtitle dialogue, and that is deliberate rather than tolerated. If
/// a bigram model built from eighteenth-century legal prose separates a good read of a film from a
/// bad one, the signal is the structure of written English and not the genre — which is the claim
/// #101 is actually making. A corpus matched to the material would leave that untested.
pub const CORPUS: &str = "\
When in the Course of human events, it becomes necessary for one people to dissolve the political \
bands which have connected them with another, and to assume among the powers of the earth, the \
separate and equal station to which the Laws of Nature and of Nature's God entitle them, a decent \
respect to the opinions of mankind requires that they should declare the causes which impel them to \
the separation. \
We hold these truths to be self-evident, that all men are created equal, that they are endowed by \
their Creator with certain unalienable Rights, that among these are Life, Liberty and the pursuit \
of Happiness. That to secure these rights, Governments are instituted among Men, deriving their \
just powers from the consent of the governed, That whenever any Form of Government becomes \
destructive of these ends, it is the Right of the People to alter or to abolish it, and to \
institute new Government, laying its foundation on such principles and organizing its powers in \
such form, as to them shall seem most likely to effect their Safety and Happiness. Prudence, \
indeed, will dictate that Governments long established should not be changed for light and \
transient causes; and accordingly all experience hath shewn, that mankind are more disposed to \
suffer, while evils are sufferable, than to right themselves by abolishing the forms to which they \
are accustomed. But when a long train of abuses and usurpations, pursuing invariably the same \
Object evinces a design to reduce them under absolute Despotism, it is their right, it is their \
duty, to throw off such Government, and to provide new Guards for their future security. Such has \
been the patient sufferance of these Colonies; and such is now the necessity which constrains them \
to alter their former Systems of Government. The history of the present King of Great Britain is a \
history of repeated injuries and usurpations, all having in direct object the establishment of an \
absolute Tyranny over these States. To prove this, let Facts be submitted to a candid world. \
He has refused his Assent to Laws, the most wholesome and necessary for the public good. \
He has forbidden his Governors to pass Laws of immediate and pressing importance, unless suspended \
in their operation till his Assent should be obtained; and when so suspended, he has utterly \
neglected to attend to them. \
He has refused to pass other Laws for the accommodation of large districts of people, unless those \
people would relinquish the right of Representation in the Legislature, a right inestimable to them \
and formidable to tyrants only. \
He has called together legislative bodies at places unusual, uncomfortable, and distant from the \
depository of their public Records, for the sole purpose of fatiguing them into compliance with his \
measures. \
He has dissolved Representative Houses repeatedly, for opposing with manly firmness his invasions \
on the rights of the people. \
He has refused for a long time, after such dissolutions, to cause others to be elected; whereby the \
Legislative powers, incapable of Annihilation, have returned to the People at large for their \
exercise; the State remaining in the mean time exposed to all the dangers of invasion from without, \
and convulsions within. \
He has endeavoured to prevent the population of these States; for that purpose obstructing the Laws \
for Naturalization of Foreigners; refusing to pass others to encourage their migrations hither, and \
raising the conditions of new Appropriations of Lands. \
He has obstructed the Administration of Justice, by refusing his Assent to Laws for establishing \
Judiciary powers. \
He has made Judges dependent on his Will alone, for the tenure of their offices, and the amount and \
payment of their salaries. \
He has erected a multitude of New Offices, and sent hither swarms of Officers to harrass our \
people, and eat out their substance. \
He has kept among us, in times of peace, Standing Armies without the Consent of our legislatures. \
He has affected to render the Military independent of and superior to the Civil power. \
He has combined with others to subject us to a jurisdiction foreign to our constitution, and \
unacknowledged by our laws; giving his Assent to their Acts of pretended Legislation: \
For Quartering large bodies of armed troops among us: \
For protecting them, by a mock Trial, from punishment for any Murders which they should commit on \
the Inhabitants of these States: \
For cutting off our Trade with all parts of the world: \
For imposing Taxes on us without our Consent: \
For depriving us in many cases, of the benefits of Trial by Jury: \
For transporting us beyond Seas to be tried for pretended offences: \
For abolishing the free System of English Laws in a neighbouring Province, establishing therein an \
Arbitrary government, and enlarging its Boundaries so as to render it at once an example and fit \
instrument for introducing the same absolute rule into these Colonies: \
For taking away our Charters, abolishing our most valuable Laws, and altering fundamentally the \
Forms of our Governments: \
For suspending our own Legislatures, and declaring themselves invested with power to legislate for \
us in all cases whatsoever. \
He has abdicated Government here, by declaring us out of his Protection and waging War against us. \
He has plundered our seas, ravaged our Coasts, burnt our towns, and destroyed the lives of our \
people. \
He is at this time transporting large Armies of foreign Mercenaries to compleat the works of death, \
desolation and tyranny, already begun with circumstances of Cruelty and perfidy scarcely paralleled \
in the most barbarous ages, and totally unworthy the Head of a civilized nation. \
He has constrained our fellow Citizens taken Captive on the high Seas to bear Arms against their \
Country, to become the executioners of their friends and Brethren, or to fall themselves by their \
Hands. \
He has excited domestic insurrections amongst us, and has endeavoured to bring on the inhabitants \
of our frontiers, the merciless Indian Savages, whose known rule of warfare, is an undistinguished \
destruction of all ages, sexes and conditions. \
In every stage of these Oppressions We have Petitioned for Redress in the most humble terms: Our \
repeated Petitions have been answered only by repeated injury. A Prince whose character is thus \
marked by every act which may define a Tyrant, is unfit to be the ruler of a free people. \
Nor have We been wanting in attentions to our British brethren. We have warned them from time to \
time of attempts by their legislature to extend an unwarrantable jurisdiction over us. We have \
reminded them of the circumstances of our emigration and settlement here. We have appealed to their \
native justice and magnanimity, and we have conjured them by the ties of our common kindred to \
disavow these usurpations, which, would inevitably interrupt our connections and correspondence. \
They too have been deaf to the voice of justice and of consanguinity. We must, therefore, acquiesce \
in the necessity, which denounces our Separation, and hold them, as we hold the rest of mankind, \
Enemies in War, in Peace Friends. \
We, therefore, the Representatives of the united States of America, in General Congress, Assembled, \
appealing to the Supreme Judge of the world for the rectitude of our intentions, do, in the Name, \
and by Authority of the good People of these Colonies, solemnly publish and declare, That these \
United Colonies are, and of Right ought to be Free and Independent States; that they are Absolved \
from all Allegiance to the British Crown, and that all political connection between them and the \
State of Great Britain, is and ought to be totally dissolved; and that as Free and Independent \
States, they have full Power to levy War, conclude Peace, contract Alliances, establish Commerce, \
and to do all other Acts and Things which Independent States may of right do. And for the support \
of this Declaration, with a firm reliance on the protection of divine Providence, we mutually \
pledge to each other our Lives, our Fortunes and our sacred Honor.";

#[cfg(test)]
mod tests {
    use super::*;

    fn english() -> Table {
        Table::from_corpus(CORPUS)
    }

    /// Enough text to clear [`MIN_BIGRAMS`], in the register the pipeline actually reads.
    const DIALOGUE: &str = "Just talk to me, okay? I can't believe you just left. \
        Look at the line ahead of you and tell me what you see, because I have been \
        standing here for an hour and nobody has said a single word about any of it. \
        The quick brown fox jumps over the lazy dog, and the dog does not care at all. \
        Follow the yellow line to the door and wait there until somebody comes to get you. \
        Nobody said anything about the door being locked, and nobody has come back since \
        the morning, which is the part that worries me more than the rest of it together. \
        Tell them what happened here and let them decide whether any of it was worth doing.";

    /// A permutation of the alphabet: the failure mode a wrong reference set produces.
    fn permute(text: &str) -> String {
        // Rotate by thirteen. Every letter maps to exactly one other and back, so the letter
        // *frequency histogram* is preserved to the character -- which is the whole point. Anything
        // derived from the track's own consistency is invariant under this.
        text.chars()
            .map(|c| match c {
                'a'..='z' => (((c as u8 - b'a' + 13) % 26) + b'a') as char,
                'A'..='Z' => (((c as u8 - b'A' + 13) % 26) + b'A') as char,
                other => other,
            })
            .collect()
    }

    #[test]
    fn a_permutation_of_the_alphabet_scores_far_below_the_text_it_came_from() {
        // The claim #101 rests on, stated as a test. If this were ever false the statistic would be
        // measuring something other than what it says.
        let table = english();
        let plain = table.score(DIALOGUE).expect("enough text");
        let scrambled = table
            .score(&permute(DIALOGUE))
            .expect("the same amount of text");
        assert!(
            plain > scrambled + 1.0,
            "English {plain:.2} against its own permutation {scrambled:.2}"
        );
    }

    #[test]
    fn a_permutation_leaves_the_letter_histogram_untouched() {
        // The other half of the same claim, and the reason no self-derived statistic can do this
        // job. Pinned so the test above cannot be read as "scrambled text looks different".
        let mut before = [0usize; LETTERS];
        let mut after = [0usize; LETTERS];
        for c in DIALOGUE.chars().filter_map(index) {
            before[c] += 1;
        }
        for c in permute(DIALOGUE).chars().filter_map(index) {
            after[c] += 1;
        }
        before.sort_unstable();
        after.sort_unstable();
        assert_eq!(before, after, "a permutation preserves the histogram exactly");
    }

    #[test]
    fn english_sits_above_a_uniform_alphabet_and_a_permutation_sits_below_it() {
        // The three-way ordering is the useful statement, and the lower half is the surprising
        // one: a permutation does not merely fail to look like English, it scores *worse than
        // chance*, because it actively produces bigrams English almost never has. That is the
        // margin the gate would live on.
        let table = english();
        let floor = Table::uniform_floor();
        let plain = table.score(DIALOGUE).unwrap();
        let scrambled = table.score(&permute(DIALOGUE)).unwrap();
        assert!(plain > floor, "English {plain:.2} against a uniform floor {floor:.2}");
        assert!(scrambled < floor, "a permutation {scrambled:.2} against {floor:.2}");
    }

    #[test]
    fn charging_the_unread_can_only_lower_a_score() {
        // The direction the charge has to run in. A set that read three-quarters of a track must
        // not be able to outscore one that read all of it by declining to answer, which is the
        // mechanism #63 documented for mean match distance and which this inherits verbatim.
        let table = english();
        let clean = DIALOGUE.to_owned();
        let holed: String = clean
            .chars()
            .enumerate()
            .map(|(i, c)| if i % 23 == 0 { REPLACEMENT } else { c })
            .collect();
        let uncharged = table.score(&holed).unwrap();
        let charged = table.score_charged(&holed).unwrap();
        assert!(
            charged < uncharged,
            "charged {charged:.2} against uncharged {uncharged:.2}"
        );
        assert!(
            (table.score_charged(&clean).unwrap() - table.score(&clean).unwrap()).abs()
                < f64::EPSILON,
            "text with nothing unread scores the same either way"
        );
    }

    #[test]
    fn too_little_text_scores_nothing_rather_than_something() {
        // The constraint #101 asks to be built in from the start: a fabricated figure is
        // indistinguishable from a real one, so there is none.
        let table = english();
        assert_eq!(table.score("Hello there."), None);
        assert_eq!(table.score(""), None);
        // Digits and punctuation are not Latin letters, however many of them there are.
        assert_eq!(table.score(&"0123456789 ".repeat(200)), None);
    }

    #[test]
    fn a_word_boundary_breaks_the_chain_rather_than_being_skipped_over() {
        // `to be` must contribute `to` and `be` and never `ob`. A model that skipped the space
        // would learn bigrams that occur in no word, which is precisely the structure it exists to
        // notice the absence of.
        let table = Table::from_corpus("ab ba");
        let joined = Table::from_corpus("abba");
        assert!(
            table.log_p[index('b').unwrap()][index('b').unwrap()]
                < joined.log_p[index('b').unwrap()][index('b').unwrap()],
            "the space has to break the chain"
        );
    }

    #[test]
    fn the_corpus_attests_the_bigrams_english_is_built_from() {
        // A corpus too thin to carry `th`, `he` and `in` would give a table that scored noise, and
        // the failure would show up only as an overlap in the bench -- which reads as a refuted
        // hypothesis rather than as a thin corpus.
        let table = english();
        let p = |a: char, b: char| table.log_p[index(a).unwrap()][index(b).unwrap()];
        assert!(p('t', 'h') > p('t', 'l'), "th over tl");
        assert!(p('h', 'e') > p('h', 'x'), "he over hx");
        assert!(p('i', 'n') > p('i', 'q'), "in over iq");
        assert!(p('q', 'u') > p('q', 'a'), "qu over qa");
    }
}
