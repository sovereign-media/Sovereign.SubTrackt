//! What each language's standard orthography requires, and therefore what it cannot spell.
//!
//! Built for [#189](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/189)'s reader,
//! which counts characters an extraction produced that its declared language has no use for. It
//! lived in `xtask` for as long as it was only ever read by an instrument.
//!
//! [#230](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/230) is the pipeline asking
//! the same question *before* the answer is committed rather than after, so the table moved here.
//! One table and not two: `LANGUAGES`'s own doc records the worry that a second copy would drift,
//! and a copy in a library crate beside a copy in an xtask is that worry realised.
//!
//! Nothing here detects anything. Every function takes a tag the container declared and a character
//! the matcher is considering, and a tag this table does not carry is answered `None` — which every
//! caller must read as *pass*, for [`crate::Script`]'s reason: a wrong refusal costs a caller an
//! expensive fallback on a track that would have read.

use crate::Script;

/// One language's orthographic demands, from first principles.
///
/// "Requires" means the standard orthography cannot be written without it: `å` in Swedish, `ñ` in
/// Spanish, `¿` in Spanish. Not what a disc might happen to draw — a typographic quote or an
/// em dash is a *typesetting* choice and lives in [`TYPOGRAPHY`], separately, because a set that
/// lacks one fails on some material in every language rather than on all material in one.
///
/// Loanword-only characters are excluded and named in `note` where the call was close. Finnish
/// takes `š` in a handful of borrowings and the language is perfectly writable without it.
///
/// **Where the call was close, the character goes in.** The rule is asymmetric on purpose, because
/// the two mistakes cost different things. A row that lists one character too many understates a
/// gap by one; a row that lists one too few makes `scripts/language/census.py` call a real word
/// impossible, which puts false entries in the only column that instrument prints. That is the
/// lesson `scripts/bench/roster.json` records at length about sidecars, applied here. Swedish `é`
/// was excluded on the first pass and the Norwegian census flagged `én` — a real word — nine times.
pub struct Language {
    /// The ISO 639-2 tag containers actually carry, which is the /B/ variant: `ger`, not `deu`.
    pub tag: &'static str,
    /// The name `subtrackt list` prints.
    pub name: &'static str,
    /// Which script it is set in.
    ///
    /// [`crate::Script`] rather than a name, which is what moving the table out of `xtask` bought:
    /// the enum there carried a `Latin` and an `Other(&str)` because it had no reason to know the
    /// rest, and the pipeline's own script guard has always known all of them. One vocabulary.
    pub script: Script,
    /// Letters beyond ASCII the orthography requires, both cases.
    pub letters: &'static str,
    /// Punctuation beyond ASCII the orthography requires.
    pub punctuation: &'static str,
    /// Letters the orthography does not require and the language's subtitles draw anyway.
    ///
    /// #230, and it exists because the gate and the census ask **different questions**. The census
    /// asks what a language *needs* -- `letters` -- and a row that listed a loanword there would
    /// understate a gap by one, which is the mistake this table's own rule tells you to avoid in
    /// the other direction. The gate asks what a track may *draw*, and on that question English's
    /// empty `letters` row is orthographically correct and empirically wrong: without this the
    /// bench refuses `resume`, `voila`, `Espanol` and `ese` -- with their accents -- off two discs,
    /// every one of them a real word in an English subtitle.
    ///
    /// Consulted by [`can_spell`] and by nothing else, so every figure `xtask language-coverage`
    /// has published stays exactly what it was.
    pub loanwords: &'static str,
    /// What this row deliberately leaves out, or an ambiguity in the tag. Empty where there is none.
    pub note: &'static str,
}

/// Every language tag `scripts/language/survey.py` found in the library, and what each one needs.
///
/// Fifty tags over 1,316 files. The order is the survey's — most files first — so the rows that
/// matter most are the ones read first, and the point of the table is visible from the top three:
/// Spanish and French are on more discs than the bench has ever looked at, and both need characters
/// the set does not have.
pub const LANGUAGES: &[Language] = &[
    Language {
        tag: "eng",
        name: "English",
        script: Script::Latin,
        letters: "",
        punctuation: "",
        // What French and Spanish require, in **lowercase**, because those are the two languages
        // this library's English material borrows from and a loanword in English prose is written
        // lowercase. The capitals are deliberately absent, and that is the whole of why this set
        // costs nothing: `I` with an acute is 331 of the 630 impossible characters the bench
        // produces and no English word wants one, so admitting the capitals would hand back over
        // half the gain to buy a word nobody writes.
        loanwords: "\u{e1}\u{e0}\u{e2}\u{e4}\u{e7}\u{e9}\u{e8}\u{ea}\u{eb}\u{ed}\u{ef}\u{f3}\u{f4}\u{f6}\u{fa}\u{f9}\u{fb}\u{fc}\u{f1}\u{e6}\u{153}",
        note: "the only language every published figure in this repository is about",
    },
    Language {
        tag: "spa",
        name: "Spanish",
        script: Script::Latin,
        letters: "áéíóúüñÁÉÍÓÚÜÑ",
        punctuation: "¿¡",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "fre",
        name: "French",
        script: Script::Latin,
        letters: "àâæçéèêëîïôùûüÿœÀÂÆÇÉÈÊËÎÏÔÙÛÜŸŒ",
        punctuation: "«»",
        loanwords: "",
        note: "the guillemets are the quotation mark, not an alternative to one",
    },
    Language {
        tag: "ger",
        name: "German",
        script: Script::Latin,
        letters: "äöüßÄÖÜ",
        punctuation: "",
        loanwords: "",
        note: "capital ẞ is optional in the orthography and vanishingly rare in subtitles",
    },
    Language {
        tag: "por",
        name: "Portuguese",
        script: Script::Latin,
        letters: "áâãàçéêíóôõúÁÂÃÀÇÉÊÍÓÔÕÚ",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "dut",
        name: "Dutch",
        script: Script::Latin,
        letters: "éëïöüÉËÏÖÜ",
        punctuation: "",
        loanwords: "",
        note: "IJ is two letters, not a ligature, so it costs the set nothing",
    },
    Language {
        tag: "swe",
        name: "Swedish",
        script: Script::Latin,
        letters: "åäöéÅÄÖÉ",
        punctuation: "",
        loanwords: "",
        note: "é is a handful of words and is in the row anyway: a census that called idé \
               impossible would put false entries in the one column it prints",
    },
    Language {
        tag: "nor",
        name: "Norwegian",
        script: Script::Latin,
        letters: "æøåéÆØÅÉ",
        punctuation: "",
        loanwords: "",
        note: "é distinguishes én from en and is required, rare as it is",
    },
    Language {
        tag: "ita",
        name: "Italian",
        script: Script::Latin,
        letters: "àèéìòóùÀÈÉÌÒÓÙ",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "fin",
        name: "Finnish",
        script: Script::Latin,
        letters: "äöÄÖ",
        punctuation: "",
        loanwords: "",
        note: "å is in the alphabet for Swedish names only; š and ž are loanwords",
    },
    Language {
        tag: "dan",
        name: "Danish",
        script: Script::Latin,
        letters: "æøåéÆØÅÉ",
        punctuation: "",
        loanwords: "",
        note: "é marks stress on a final syllable -- ené, allé -- as in Norwegian",
    },
    Language {
        tag: "chi",
        name: "Chinese",
        script: Script::Han,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "cze",
        name: "Czech",
        script: Script::Latin,
        letters: "áčďéěíňóřšťúůýžÁČĎÉĚÍŇÓŘŠŤÚŮÝŽ",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "kor",
        name: "Korean",
        script: Script::Korean,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "gre",
        name: "Greek",
        script: Script::Greek,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "jpn",
        name: "Japanese",
        script: Script::Japanese,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "rus",
        name: "Russian",
        script: Script::Cyrillic,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "the track #189 measured reading 83% of its glyphs as confident Latin garbage",
    },
    Language {
        tag: "pol",
        name: "Polish",
        script: Script::Latin,
        letters: "ąćęłńóśźżĄĆĘŁŃÓŚŹŻ",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "rum",
        name: "Romanian",
        script: Script::Latin,
        letters: "ăâîșțĂÂÎȘȚ",
        punctuation: "",
        loanwords: "",
        note: "ş and ţ with cedilla are the pre-2005 encoding of ș and ț and appear on older discs",
    },
    Language {
        tag: "tur",
        name: "Turkish",
        script: Script::Latin,
        letters: "çğıİöşüÇĞÖŞÜ",
        punctuation: "",
        loanwords: "",
        note: "dotless ı and dotted İ are distinct letters, not case variants of i and I",
    },
    Language {
        tag: "hun",
        name: "Hungarian",
        script: Script::Latin,
        letters: "áéíóöőúüűÁÉÍÓÖŐÚÜŰ",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "tha",
        name: "Thai",
        script: Script::Thai,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "ice",
        name: "Icelandic",
        script: Script::Latin,
        letters: "áéíóúýþðæöÁÉÍÓÚÝÞÐÆÖ",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "bul",
        name: "Bulgarian",
        script: Script::Cyrillic,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "ara",
        name: "Arabic",
        script: Script::Arabic,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "right to left, and cursive: the segmenter's assumptions do not hold either",
    },
    Language {
        tag: "hrv",
        name: "Croatian",
        script: Script::Latin,
        letters: "čćđšžČĆĐŠŽ",
        punctuation: "",
        loanwords: "",
        note: "dž, lj and nj are digraphs of two letters and cost the set nothing",
    },
    Language {
        tag: "slv",
        name: "Slovenian",
        script: Script::Latin,
        letters: "čšžČŠŽ",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "heb",
        name: "Hebrew",
        script: Script::Hebrew,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "est",
        name: "Estonian",
        script: Script::Latin,
        letters: "äöõüšžÄÖÕÜŠŽ",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "lit",
        name: "Lithuanian",
        script: Script::Latin,
        letters: "ąčėęįšųūžĄČĖĘĮŠŲŪŽ",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "ind",
        name: "Indonesian",
        script: Script::Latin,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "ASCII throughout, which makes it the one non-English language the set already covers",
    },
    Language {
        tag: "hin",
        name: "Hindi",
        script: Script::Devanagari,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "lav",
        name: "Latvian",
        script: Script::Latin,
        letters: "āčēģīķļņšūžĀČĒĢĪĶĻŅŠŪŽ",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "srp",
        name: "Serbian",
        script: Script::Latin,
        letters: "čćđšžČĆĐŠŽ",
        punctuation: "",
        loanwords: "",
        note: "written in either script; the Latin cut is what discs carry, and it is Croatian's",
    },
    Language {
        tag: "slo",
        name: "Slovak",
        script: Script::Latin,
        letters: "áäčďéíĺľňóôŕšťúýžÁÄČĎÉÍĹĽŇÓÔŔŠŤÚÝŽ",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "ukr",
        name: "Ukrainian",
        script: Script::Cyrillic,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "may",
        name: "Malay",
        script: Script::Latin,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "ASCII throughout, as Indonesian",
    },
    Language {
        tag: "nob",
        name: "Norwegian Bokmal",
        script: Script::Latin,
        letters: "æøåéÆØÅÉ",
        punctuation: "",
        loanwords: "",
        note: "a second tag for a language already tagged nor on 155 files",
    },
    Language {
        tag: "scc",
        name: "Serbian",
        script: Script::Latin,
        letters: "čćđšžČĆĐŠŽ",
        punctuation: "",
        loanwords: "",
        note: "the deprecated tag for srp, still on nine files",
    },
    Language {
        tag: "vie",
        name: "Vietnamese",
        script: Script::Latin,
        letters: concat!(
            "àáâãèéêìíòóôõùúýăđĩũơưạảấầẩẫậắằẳẵặẹẻẽếềểễệỉịọỏốồổỗộớờởỡợụủứừửữựỳỵỷỹ",
            "ÀÁÂÃÈÉÊÌÍÒÓÔÕÙÚÝĂĐĨŨƠƯẠẢẤẦẨẪẬẮẰẲẴẶẸẺẼẾỀỂỄỆỈỊỌỎỐỒỔỖỘỚỜỞỠỢỤỦỨỪỬỮỰỲỴỶỸ",
        ),
        punctuation: "",
        loanwords: "",
        note: "Latin script and the largest demand in the library: five tones over twelve vowels, \
               stacked over the vowel's own mark, which is two marks on one body",
    },
    Language {
        tag: "kaz",
        name: "Kazakh",
        script: Script::Cyrillic,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "cat",
        name: "Catalan",
        script: Script::Latin,
        letters: "àçèéíïòóúüÀÇÈÉÍÏÒÓÚÜ",
        punctuation: "·",
        loanwords: "",
        note: "the interpunct is a letter's business here: l·l is a distinct digraph from ll",
    },
    Language {
        tag: "tam",
        name: "Tamil",
        script: Script::Tamil,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "tel",
        name: "Telugu",
        script: Script::Telugu,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "scr",
        name: "Croatian",
        script: Script::Latin,
        letters: "čćđšžČĆĐŠŽ",
        punctuation: "",
        loanwords: "",
        note: "the deprecated tag for hrv",
    },
    Language {
        tag: "aze",
        name: "Azerbaijani",
        script: Script::Latin,
        letters: "çəğıİöşüÇƏĞÖŞÜ",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "frs",
        name: "Eastern Frisian",
        script: Script::Latin,
        letters: "äöüÄÖÜ",
        punctuation: "",
        loanwords: "",
        note: "one file, and the tag is more likely a muxer's mistake for fre or fry than a claim",
    },
    Language {
        tag: "geo",
        name: "Georgian",
        script: Script::Georgian,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "grc",
        name: "Ancient Greek",
        script: Script::Greek,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
    Language {
        tag: "lat",
        name: "Latin",
        script: Script::Latin,
        letters: "",
        punctuation: "",
        loanwords: "",
        note: "",
    },
];

/// Punctuation no orthography requires and many discs draw anyway.
///
/// Held apart from [`LANGUAGES`] on purpose. A missing letter fails one language; a missing curly
/// quote fails *every* language on the discs that typeset one, English included — so it is a
/// different kind of gap and summing it into a per-language row would hide that. It is probed the
/// same way, because the question is the same question: reject, or rehome?
/// Disjoint from every language row by construction: a character an orthography *requires* belongs
/// to that orthography, and `no_character_is_both_required_and_merely_typographic` holds it. The
/// guillemets are French's quotation mark and the interpunct is a Catalan letter's business, so
/// neither is here even though a disc in another language may draw them.
pub const TYPOGRAPHY: &str = "\u{2018}\u{2019}\u{201c}\u{201d}\u{201e}\u{2013}\u{2014}\u{2026}";

/// The characters a language's standard orthography requires beyond ASCII.
///
/// `None` where the table does not carry the tag, which every caller must treat as *pass*.
#[must_use]
pub fn required(tag: &str) -> Option<(&'static str, &'static str)> {
    row_for(tag).map(|row| (row.letters, row.punctuation))
}

/// The row for a tag, with any region suffix dropped and case folded.
///
/// Containers carry `por-BR` and `ENG`. The row is per language, so the suffix goes.
fn row_for(tag: &str) -> Option<&'static Language> {
    let tag = tag.trim().to_ascii_lowercase();
    let tag = tag.split(['-', '_']).next().unwrap_or(&tag);
    LANGUAGES.iter().find(|row| row.tag == tag)
}

/// Whether `tag`'s orthography can spell `character` at all.
///
/// `None` where nothing can be said — the tag is absent from the table, the character is ASCII, or
/// it is not a letter. `Some(false)` is the only answer that is a *fact* a caller may act on: this
/// language does not use this letter.
///
/// Case is not folded and must not be. A row lists both cases of every letter it requires, so
/// folding here would let `Á` through on the strength of `á` in a language that writes only one of
/// them — and the table is the place that decision belongs.
#[must_use]
pub fn can_spell(tag: &str, character: char) -> Option<bool> {
    if character.is_ascii() {
        return None;
    }
    // **Letters only**, and this is the narrowing that keeps the answer a fact. An orthography's
    // claim on a letter is solid: English does not write `Í`, in any typeface, on any disc. Its
    // claim on punctuation is not — `TYPOGRAPHY` exists precisely because a curly quote and an em
    // dash are *typesetting* choices that fail every language on the discs that draw one, and a
    // musical note is drawn on SDH discs in every language on earth. A row that lists `¿` under
    // Spanish says Spanish needs it; it does not say English never draws one.
    //
    // The `TYPOGRAPHY` set is a subset of what this skips, and is not consulted here for that
    // reason. It remains what `xtask language-coverage` probes, which is a different question:
    // what a *set* can draw, rather than what a *language* can spell.
    if !character.is_alphabetic() {
        return None;
    }
    // A character of a different script is not this language's business to allow or refuse: the
    // script guard is what answers that, before the read, and answering it twice in two places is
    // how the two would come to disagree.
    if Script::of_language(tag) != Script::of_char(character) {
        return None;
    }
    let row = row_for(tag)?;
    Some(
        row.letters.contains(character)
            || row.punctuation.contains(character)
            || row.loanwords.contains(character),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_language_the_table_does_not_carry_is_answered_pass_rather_than_refused() {
        // #218's rule, which this shares: every uncertainty resolves to a pass, because a wrong
        // refusal costs a caller an expensive fallback on a track that would have read.
        assert_eq!(can_spell("zzz", '\u{e9}'), None);
        assert_eq!(required("zzz"), None);
    }

    #[test]
    fn ascii_and_punctuation_are_nobody_s_orthography() {
        // A curly quote fails every language on the discs that typeset one, English included, so it
        // is not a fact about any of them -- and a musical note is drawn on an SDH disc in every
        // language there is. Only a letter carries an orthography's claim, which is the narrowing
        // that keeps every answer this gate acts on a fact.
        assert_eq!(can_spell("swe", 'a'), None);
        assert_eq!(can_spell("eng", '\u{266a}'), None, "a note is not English's business");
        assert_eq!(can_spell("eng", '\u{bf}'), None, "nor is an inverted question mark");
        assert_eq!(can_spell("swe", '\u{2019}'), None);
    }

    #[test]
    fn english_draws_a_loanword_and_still_refuses_the_capital_of_the_same_letter() {
        // The asymmetry the `loanwords` field exists for, and the reason it costs nothing. English
        // writes `resume` with two acutes and `Espanol` with a tilde; it never writes a capital
        // `I` with an acute, which is over half of what the matcher invents.
        assert_eq!(can_spell("eng", '\u{e9}'), Some(true), "resume");
        assert_eq!(can_spell("eng", '\u{f1}'), Some(true), "Espanol");
        assert_eq!(can_spell("eng", '\u{e0}'), Some(true), "voila");
        assert_eq!(can_spell("eng", '\u{cd}'), Some(false), "no English word wants it");
        assert_eq!(can_spell("eng", '\u{ec}'), Some(false), "nor a grave on an i");
    }

    #[test]
    fn a_loanword_letter_is_never_listed_as_required() {
        // The two fields answer different questions and must not blur. `letters` is what the
        // census reads to say a language *needs* a character; putting a loanword there would
        // understate a gap by one, which is the mistake the table's own rule forbids.
        for row in LANGUAGES {
            for ch in row.loanwords.chars() {
                assert!(
                    !row.letters.contains(ch),
                    "{}: {ch} is both required and a loanword",
                    row.tag
                );
            }
        }
    }

    #[test]
    fn english_cannot_spell_an_acute_and_spanish_can() {
        assert_eq!(can_spell("eng", '\u{cd}'), Some(false));
        assert_eq!(can_spell("spa", '\u{cd}'), Some(true));
    }

    #[test]
    fn a_character_of_another_script_is_the_guard_s_question_and_not_this_one_s() {
        // Two gates, two facts. The script guard refuses a Cyrillic track before the read; this
        // says nothing about a Cyrillic character, so the two can never disagree about one.
        assert_eq!(can_spell("eng", '\u{416}'), None);
    }

    #[test]
    fn a_tag_carrying_a_region_still_finds_its_row() {
        // Containers carry `por-BR` and `eng_US`. The row is per language, so the suffix is dropped.
        assert_eq!(can_spell("eng-US", '\u{cd}'), Some(false));
        assert_eq!(can_spell("ENG", '\u{cd}'), Some(false));
    }

    #[test]
    fn a_two_letter_tag_is_a_pass_because_the_library_carries_none() {
        // Every one of the 51 rows is an ISO 639-2/B tag, because that is what
        // `scripts/language/survey.py` found over 1,316 files -- containers in this library declare
        // `eng`, never `en`. `Script::of_language` knows both, because a *script* is cheap to be
        // right about; a missing row here resolves to a pass, which is the safe direction and the
        // one #218's rule requires. If a two-letter tag ever turns up, this test is where it lands.
        assert_eq!(can_spell("en", '\u{cd}'), None);
    }

    #[test]
    fn every_row_lists_both_cases_of_every_letter_it_requires() {
        // The rule `can_spell` depends on by refusing to fold case. A row that listed only the
        // lowercase of a letter would make the gate refuse a real capital, which is the false
        // refusal this whole design is built to avoid.
        for row in LANGUAGES {
            for ch in row.letters.chars() {
                // Single-character mappings only. German `ss` uppercases to two letters, and the
                // row deliberately carries no capital for it -- its own note says why, and a
                // two-letter expansion is not a character this gate could ever refuse.
                if ch.to_uppercase().count() != 1 || ch.to_lowercase().count() != 1 {
                    continue;
                }
                for other in ch.to_uppercase().chain(ch.to_lowercase()) {
                    // And ASCII case-mates are nobody's business: Turkish `dotless i` uppercases to
                    // plain `I`, which `can_spell` passes without consulting a row at all.
                    assert!(
                        row.letters.contains(other) || !other.is_alphabetic() || other.is_ascii(),
                        "{}: {ch} is listed and {other} is not",
                        row.tag
                    );
                }
            }
        }
    }
}
