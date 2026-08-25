//! Which writing system a language is set in, and which one a character belongs to.
//!
//! Exists for [#218](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/218). A track
//! declared `rus` read against a Latin reference set produces confident Latin garbage — `ceoeü`,
//! `I?peôcma���lo` — and the only thing that refuses it today is the threshold gate, which counts
//! unread glyphs and lands seven points below its floor. That is margin, not design.
//!
//! ## Why this could not be a statistic
//!
//! [`fit-confidence.md`](https://github.com/sovereign-media/Sovereign.SubTrackt/blob/main/docs/fit-confidence.md)
//! records six statistics measured against the neighbouring question — is this read *good* — and
//! none of them separates. #218 measured a seventh against this one, and the reason it fails is
//! worth stating here rather than in a document, because it is what justifies a table of language
//! tags living in a library crate that otherwise holds only types:
//!
//! **Nothing computed from the read can tell a wrong script from a wrong typeface**, because both
//! are the same event — the set cannot spell this track — and the read has no way to know why. On
//! one disc, mean match distance is 26.5 to 37.3 for the five non-Latin tracks and 23.1 to 34.9 for
//! the same English track read with six wrong typefaces. The bands do not merely overlap; the first
//! sits inside the second.
//!
//! The container's declared language is evidence from outside the read entirely. It is the only
//! such evidence this pipeline has, and comparing it against what a reference set actually contains
//! is a *fact* rather than a threshold — which is the difference `CLAUDE.md` asks for.
//!
//! ## What it deliberately does not do
//!
//! It does not detect. [`Script::of_char`] answers a question about a codepoint and
//! [`Script::of_language`] answers one about a tag; neither looks at a bitmap, and nothing here
//! guesses at an untagged stream. 658 files in the library carry one — the muxer leaves the default
//! untagged, which is #180 — and a guard that refused those would refuse the bench.

use core::fmt;

/// A writing system, at the granularity a reference set can be asked about.
///
/// Coarser than Unicode's script property on purpose. Japanese is written in Han and two kana
/// syllabaries at once and this calls the whole thing [`Script::Japanese`], because the question
/// being asked is "can this reference set spell a track in this language", and the answer for
/// Japanese is no in the same way for all three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Script {
    /// Latin, which is every language this project has ever read.
    Latin,
    /// Greek.
    Greek,
    /// Cyrillic.
    Cyrillic,
    /// Hebrew.
    Hebrew,
    /// Arabic.
    Arabic,
    /// Devanagari.
    Devanagari,
    /// Tamil.
    Tamil,
    /// Telugu.
    Telugu,
    /// Thai.
    Thai,
    /// Georgian.
    Georgian,
    /// Armenian.
    Armenian,
    /// Han, as used for Chinese.
    Han,
    /// Han and kana together, as used for Japanese.
    Japanese,
    /// Hangul, with the Han a Korean text may mix in.
    Korean,
}

impl fmt::Display for Script {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Latin => "Latin",
            Self::Greek => "Greek",
            Self::Cyrillic => "Cyrillic",
            Self::Hebrew => "Hebrew",
            Self::Arabic => "Arabic",
            Self::Devanagari => "Devanagari",
            Self::Tamil => "Tamil",
            Self::Telugu => "Telugu",
            Self::Thai => "Thai",
            Self::Georgian => "Georgian",
            Self::Armenian => "Armenian",
            Self::Han => "Han",
            Self::Japanese => "Japanese",
            Self::Korean => "Korean",
        };
        f.write_str(name)
    }
}

/// Language tags to scripts.
///
/// Every tag `scripts/language/survey.py` found across the library, plus the ISO 639-2/T and 639-1
/// spellings of each, because a container may carry any of the three and the muxer chooses. The /B/
/// codes are the ones actually seen — `ger`, `fre`, `rum` — and the /T/ ones are here because
/// nothing costs less than a second row.
///
/// **Only entries this can be certain about.** A tag missing from the table is `None`, and `None`
/// never refuses anything. A wrong entry here would refuse a track that reads perfectly, which is
/// far worse than a missing one letting a bad track through to the gate that already exists.
///
/// Serbian and Azerbaijani are deliberately **absent** despite being in the library. Both are
/// written in either script, and the discs surveyed carry the Latin cut — so an entry saying
/// "Cyrillic" would refuse a readable track. That is the asymmetry this table is built around.
const LANGUAGE_SCRIPTS: &[(&str, Script)] = &[
    // Latin, and by far the largest group. Present rather than omitted because a *positive* Latin
    // answer is what lets a caller say "this set and this track agree" rather than only "no
    // objection", and because #218's guard is stated as a comparison rather than a blocklist.
    ("eng", Script::Latin),
    ("en", Script::Latin),
    ("spa", Script::Latin),
    ("es", Script::Latin),
    ("fre", Script::Latin),
    ("fra", Script::Latin),
    ("fr", Script::Latin),
    ("ger", Script::Latin),
    ("deu", Script::Latin),
    ("de", Script::Latin),
    ("por", Script::Latin),
    ("pt", Script::Latin),
    ("dut", Script::Latin),
    ("nld", Script::Latin),
    ("nl", Script::Latin),
    ("swe", Script::Latin),
    ("sv", Script::Latin),
    ("nor", Script::Latin),
    ("nob", Script::Latin),
    ("nno", Script::Latin),
    ("no", Script::Latin),
    ("nb", Script::Latin),
    ("ita", Script::Latin),
    ("it", Script::Latin),
    ("fin", Script::Latin),
    ("fi", Script::Latin),
    ("dan", Script::Latin),
    ("da", Script::Latin),
    ("cze", Script::Latin),
    ("ces", Script::Latin),
    ("cs", Script::Latin),
    ("pol", Script::Latin),
    ("pl", Script::Latin),
    ("rum", Script::Latin),
    ("ron", Script::Latin),
    ("ro", Script::Latin),
    ("tur", Script::Latin),
    ("tr", Script::Latin),
    ("hun", Script::Latin),
    ("hu", Script::Latin),
    ("ice", Script::Latin),
    ("isl", Script::Latin),
    ("is", Script::Latin),
    ("hrv", Script::Latin),
    ("scr", Script::Latin),
    ("hr", Script::Latin),
    ("slv", Script::Latin),
    ("sl", Script::Latin),
    ("est", Script::Latin),
    ("et", Script::Latin),
    ("lit", Script::Latin),
    ("lt", Script::Latin),
    ("lav", Script::Latin),
    ("lv", Script::Latin),
    ("ind", Script::Latin),
    ("id", Script::Latin),
    ("may", Script::Latin),
    ("msa", Script::Latin),
    ("ms", Script::Latin),
    ("slo", Script::Latin),
    ("slk", Script::Latin),
    ("sk", Script::Latin),
    ("vie", Script::Latin),
    ("vi", Script::Latin),
    ("cat", Script::Latin),
    ("ca", Script::Latin),
    ("lat", Script::Latin),
    ("la", Script::Latin),
    ("frs", Script::Latin),
    ("fry", Script::Latin),
    // Everything else.
    ("rus", Script::Cyrillic),
    ("ru", Script::Cyrillic),
    ("ukr", Script::Cyrillic),
    ("uk", Script::Cyrillic),
    ("bul", Script::Cyrillic),
    ("bg", Script::Cyrillic),
    ("bel", Script::Cyrillic),
    ("mac", Script::Cyrillic),
    ("mkd", Script::Cyrillic),
    ("kaz", Script::Cyrillic),
    ("gre", Script::Greek),
    ("ell", Script::Greek),
    ("el", Script::Greek),
    ("grc", Script::Greek),
    ("heb", Script::Hebrew),
    ("he", Script::Hebrew),
    ("iw", Script::Hebrew),
    ("ara", Script::Arabic),
    ("ar", Script::Arabic),
    ("per", Script::Arabic),
    ("fas", Script::Arabic),
    ("fa", Script::Arabic),
    ("urd", Script::Arabic),
    ("ur", Script::Arabic),
    ("hin", Script::Devanagari),
    ("hi", Script::Devanagari),
    ("mar", Script::Devanagari),
    ("nep", Script::Devanagari),
    ("tam", Script::Tamil),
    ("ta", Script::Tamil),
    ("tel", Script::Telugu),
    ("te", Script::Telugu),
    ("tha", Script::Thai),
    ("th", Script::Thai),
    ("geo", Script::Georgian),
    ("kat", Script::Georgian),
    ("ka", Script::Georgian),
    ("arm", Script::Armenian),
    ("hye", Script::Armenian),
    ("hy", Script::Armenian),
    ("chi", Script::Han),
    ("zho", Script::Han),
    ("zh", Script::Han),
    ("yue", Script::Han),
    ("jpn", Script::Japanese),
    ("ja", Script::Japanese),
    ("kor", Script::Korean),
    ("ko", Script::Korean),
];

impl Script {
    /// The script a language tag is written in, or `None` where nothing here is certain.
    ///
    /// Case-insensitive, and a BCP 47 tag is read down to its primary subtag — `pt-BR` is
    /// Portuguese and `zh-Hans` is Han. A region or variant never changes the script of a language
    /// in this table; the two languages where a *script* subtag would matter, Serbian and
    /// Azerbaijani, are absent for exactly that reason.
    ///
    /// `None` for an unknown tag, and callers must treat `None` as "no objection". A guard that
    /// refused what it did not recognise would refuse every new tag before anyone noticed.
    #[must_use]
    pub fn of_language(tag: &str) -> Option<Self> {
        let primary = tag.split(['-', '_']).next().unwrap_or(tag).trim();
        if primary.is_empty() {
            return None;
        }
        LANGUAGE_SCRIPTS
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(primary))
            .map(|(_, script)| *script)
    }

    /// The script a character belongs to, or `None` for anything shared.
    ///
    /// Digits, spaces and punctuation are `None` rather than Latin, and that is the whole point of
    /// the return type: a reference set containing nothing but `0-9` and `.` can spell no language
    /// at all, and calling its full stop Latin would say it could spell English.
    ///
    /// Ranges rather than a property table. This is a *coarse* question — the caller wants to know
    /// whether a set has any Cyrillic in it — and a full Unicode script database would be a data
    /// dependency for an answer nobody needs at that resolution.
    #[must_use]
    pub fn of_char(ch: char) -> Option<Self> {
        match u32::from(ch) {
            // Latin, including the supplements and extended blocks a European orthography needs.
            0x0041..=0x005A | 0x0061..=0x007A | 0x00C0..=0x024F | 0x1E00..=0x1EFF => {
                Some(Self::Latin)
            }
            0x0370..=0x03FF | 0x1F00..=0x1FFF => Some(Self::Greek),
            0x0400..=0x052F => Some(Self::Cyrillic),
            0x0530..=0x058F => Some(Self::Armenian),
            0x0590..=0x05FF => Some(Self::Hebrew),
            0x0600..=0x06FF | 0x0750..=0x077F => Some(Self::Arabic),
            0x0900..=0x097F => Some(Self::Devanagari),
            0x0B80..=0x0BFF => Some(Self::Tamil),
            0x0C00..=0x0C7F => Some(Self::Telugu),
            0x0E00..=0x0E7F => Some(Self::Thai),
            0x10A0..=0x10FF => Some(Self::Georgian),
            // Kana before Han, because a set holding both is Japanese and a set holding only the
            // ideographs cannot be told from a Chinese one. Checked in this order so the more
            // specific evidence wins.
            0x3040..=0x30FF => Some(Self::Japanese),
            0xAC00..=0xD7AF | 0x1100..=0x11FF => Some(Self::Korean),
            0x4E00..=0x9FFF | 0x3400..=0x4DBF => Some(Self::Han),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_tag_objects_to_nothing_rather_than_guessing() {
        // The direction the whole guard is built to fail in. A tag nobody has seen must not refuse
        // a track, because the cost of a wrong refusal is a caller falling back to burn-in on a
        // track that would have read perfectly.
        assert_eq!(Script::of_language("xyz"), None);
        assert_eq!(Script::of_language(""), None);
        assert_eq!(Script::of_language("und"), None);
    }

    #[test]
    fn a_language_written_in_two_scripts_is_absent_rather_than_assigned_one() {
        // Serbian is the case that would have been wrong. It is written in Cyrillic and in Latin,
        // the discs surveyed carry the Latin cut, and an entry claiming Cyrillic would refuse a
        // readable track. Absence is the correct answer, and it has to be a deliberate one.
        for tag in ["srp", "scc", "sr", "aze", "az"] {
            assert_eq!(Script::of_language(tag), None, "{tag} should not be assigned a script");
        }
    }

    #[test]
    fn a_region_subtag_does_not_change_the_script() {
        assert_eq!(Script::of_language("pt-BR"), Some(Script::Latin));
        assert_eq!(Script::of_language("en_US"), Some(Script::Latin));
        assert_eq!(Script::of_language("ZH"), Some(Script::Han));
    }

    #[test]
    fn the_b_and_t_spellings_of_one_language_agree() {
        // A container carries whichever the muxer chose, and the library survey found the /B/ codes
        // -- ger, fre, rum. Disagreeing here would make the guard depend on the muxer.
        for (b, t) in [
            ("ger", "deu"),
            ("fre", "fra"),
            ("rum", "ron"),
            ("chi", "zho"),
        ] {
            assert_eq!(
                Script::of_language(b),
                Script::of_language(t),
                "{b} and {t} are the same language"
            );
        }
    }

    #[test]
    fn shared_punctuation_belongs_to_no_script() {
        // What stops a set being called Latin because it contains a full stop. Every reference set
        // ever generated holds ASCII punctuation and digits, so if these counted, every set would
        // spell every alphabetic language.
        for ch in [
            '.', ',', '?', '!', '0', '9', ' ', '-', '\u{2014}', '\u{266a}',
        ] {
            assert_eq!(Script::of_char(ch), None, "{ch:?} should belong to no script");
        }
    }

    #[test]
    fn an_accented_latin_letter_is_still_latin() {
        // The Latin-1 and Extended blocks are where every character #189's table asks for lives.
        for ch in [
            'a', 'Z', '\u{e5}', '\u{f8}', '\u{142}', '\u{219}', '\u{1ea1}',
        ] {
            assert_eq!(Script::of_char(ch), Some(Script::Latin), "{ch:?}");
        }
    }

    #[test]
    fn each_script_recognises_a_letter_of_its_own() {
        for (ch, script) in [
            ('\u{3b1}', Script::Greek),
            ('\u{430}', Script::Cyrillic),
            ('\u{5d0}', Script::Hebrew),
            ('\u{627}', Script::Arabic),
            ('\u{905}', Script::Devanagari),
            ('\u{b85}', Script::Tamil),
            ('\u{c05}', Script::Telugu),
            ('\u{e01}', Script::Thai),
            ('\u{10d0}', Script::Georgian),
            ('\u{561}', Script::Armenian),
            ('\u{4e00}', Script::Han),
            ('\u{3042}', Script::Japanese),
            ('\u{ac00}', Script::Korean),
        ] {
            assert_eq!(Script::of_char(ch), Some(script), "{ch:?}");
        }
    }

    #[test]
    fn no_two_rows_claim_the_same_tag() {
        // A duplicate would make the answer depend on table order, which is a bug that reads as a
        // decision.
        for (at, (tag, _)) in LANGUAGE_SCRIPTS.iter().enumerate() {
            for (other, _) in &LANGUAGE_SCRIPTS[at + 1..] {
                assert!(!tag.eq_ignore_ascii_case(other), "{tag} appears twice");
            }
        }
    }
}
