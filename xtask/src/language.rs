//! Which orthographies can a reference set spell, and what does it do with the ones it cannot?
//!
//! [#189](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/189) §2 and §3. The issue
//! found `å` read as `à` and `¿` read as `J` on one disc, and named the general question those two
//! are samples of: for every character a language needs and `charset()` lacks, does the matcher
//! **reject** it or does it find a **confident wrong home**?
//!
//! The distinction is the whole point, and it is `CLAUDE.md`'s first rule stated in the small. A
//! rejected character is a fact — it reaches the caller as an unmatched glyph, it is counted, and
//! the threshold gate can act on it. A rehomed one is invented data: `J` is a perfectly ordinary
//! character to find in a subtitle, so no coverage figure, no gate and no census can tell it from a
//! `J` that was really there. **Nothing downstream can catch the second kind.** That is what this
//! measures, and it is why the two verdicts are reported in separate columns rather than summed.
//!
//! ## What it does
//!
//! Two reference sets from the same faces, through the same normalisation:
//!
//! - the **real** one, [`charset()`] under [`RENDERINGS`], which is what `gen-reference` writes;
//! - a **probe** set over the characters `charset()` omits, which is never written anywhere.
//!
//! Then every probe entry is scanned against the real set's matcher at the shipped thresholds. A
//! probe character cannot match itself — it is not in the set — so whatever comes back is either
//! `unread`, or the character this pipeline would silently emit in its place on a real disc.
//!
//! The probe is generated through [`generate_over`] rather than rasterised here, and that is not
//! tidiness: `font`'s module documentation says the reference side must go through the *same*
//! transform as the runtime, and a probe normalised by a second copy of that code would answer a
//! question about the copy.
//!
//! ## What it cannot say
//!
//! The probe is the font's own outline, cleanly rasterised at 96px. A real glyph arrives from a
//! decoded bitmap at 21 to 50px, so this is the **best case** for rejection: a noisier glyph moves
//! in some direction, and nothing here says which. Read a rehoming as "this happens", not as "this
//! is how often".
//!
//! It also says nothing about characters the set *has*. §3 of #189 — `í` read as `Í` half the time
//! — is a confusion between two characters both present, which is a different instrument's
//! question and belongs with the confusions `xtask separability` already ranks.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use anyhow::Context as _;
use subtrackt_glyph::font::{
    Crop, Face, RENDERINGS, Rendering, charset, generate_over, generate_under,
};
use subtrackt_glyph::matcher::{HammingMatcher, MatchThresholds};
use subtrackt_glyph::reference::Style;

/// Which writing system a language is set in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    /// Latin, so the question of which characters are missing is a question about a handful.
    Latin,
    /// Something else entirely, named. Out of scope for reading — #189 §4's honest rejection is the
    /// deliverable for these, not a read — and listed so the table is the whole library rather than
    /// the part that happens to be tractable.
    Other(&'static str),
}

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
struct Language {
    /// The ISO 639-2 tag containers actually carry, which is the /B/ variant: `ger`, not `deu`.
    tag: &'static str,
    /// The name `subtrackt list` prints.
    name: &'static str,
    /// Which script it is set in.
    script: Script,
    /// Letters beyond ASCII the orthography requires, both cases.
    letters: &'static str,
    /// Punctuation beyond ASCII the orthography requires.
    punctuation: &'static str,
    /// What this row deliberately leaves out, or an ambiguity in the tag. Empty where there is none.
    note: &'static str,
}

/// Every language tag `scripts/language/survey.py` found in the library, and what each one needs.
///
/// Fifty tags over 1,316 files. The order is the survey's — most files first — so the rows that
/// matter most are the ones read first, and the point of the table is visible from the top three:
/// Spanish and French are on more discs than the bench has ever looked at, and both need characters
/// the set does not have.
const LANGUAGES: &[Language] = &[
    Language {
        tag: "eng",
        name: "English",
        script: Script::Latin,
        letters: "",
        punctuation: "",
        note: "the only language every published figure in this repository is about",
    },
    Language {
        tag: "spa",
        name: "Spanish",
        script: Script::Latin,
        letters: "áéíóúüñÁÉÍÓÚÜÑ",
        punctuation: "¿¡",
        note: "",
    },
    Language {
        tag: "fre",
        name: "French",
        script: Script::Latin,
        letters: "àâæçéèêëîïôùûüÿœÀÂÆÇÉÈÊËÎÏÔÙÛÜŸŒ",
        punctuation: "«»",
        note: "the guillemets are the quotation mark, not an alternative to one",
    },
    Language {
        tag: "ger",
        name: "German",
        script: Script::Latin,
        letters: "äöüßÄÖÜ",
        punctuation: "",
        note: "capital ẞ is optional in the orthography and vanishingly rare in subtitles",
    },
    Language {
        tag: "por",
        name: "Portuguese",
        script: Script::Latin,
        letters: "áâãàçéêíóôõúÁÂÃÀÇÉÊÍÓÔÕÚ",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "dut",
        name: "Dutch",
        script: Script::Latin,
        letters: "éëïöüÉËÏÖÜ",
        punctuation: "",
        note: "IJ is two letters, not a ligature, so it costs the set nothing",
    },
    Language {
        tag: "swe",
        name: "Swedish",
        script: Script::Latin,
        letters: "åäöéÅÄÖÉ",
        punctuation: "",
        note: "é is a handful of words and is in the row anyway: a census that called idé \
               impossible would put false entries in the one column it prints",
    },
    Language {
        tag: "nor",
        name: "Norwegian",
        script: Script::Latin,
        letters: "æøåéÆØÅÉ",
        punctuation: "",
        note: "é distinguishes én from en and is required, rare as it is",
    },
    Language {
        tag: "ita",
        name: "Italian",
        script: Script::Latin,
        letters: "àèéìòóùÀÈÉÌÒÓÙ",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "fin",
        name: "Finnish",
        script: Script::Latin,
        letters: "äöÄÖ",
        punctuation: "",
        note: "å is in the alphabet for Swedish names only; š and ž are loanwords",
    },
    Language {
        tag: "dan",
        name: "Danish",
        script: Script::Latin,
        letters: "æøåéÆØÅÉ",
        punctuation: "",
        note: "é marks stress on a final syllable -- ené, allé -- as in Norwegian",
    },
    Language {
        tag: "chi",
        name: "Chinese",
        script: Script::Other("Han"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "cze",
        name: "Czech",
        script: Script::Latin,
        letters: "áčďéěíňóřšťúůýžÁČĎÉĚÍŇÓŘŠŤÚŮÝŽ",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "kor",
        name: "Korean",
        script: Script::Other("Hangul"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "gre",
        name: "Greek",
        script: Script::Other("Greek"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "jpn",
        name: "Japanese",
        script: Script::Other("Han, kana"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "rus",
        name: "Russian",
        script: Script::Other("Cyrillic"),
        letters: "",
        punctuation: "",
        note: "the track #189 measured reading 83% of its glyphs as confident Latin garbage",
    },
    Language {
        tag: "pol",
        name: "Polish",
        script: Script::Latin,
        letters: "ąćęłńóśźżĄĆĘŁŃÓŚŹŻ",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "rum",
        name: "Romanian",
        script: Script::Latin,
        letters: "ăâîșțĂÂÎȘȚ",
        punctuation: "",
        note: "ş and ţ with cedilla are the pre-2005 encoding of ș and ț and appear on older discs",
    },
    Language {
        tag: "tur",
        name: "Turkish",
        script: Script::Latin,
        letters: "çğıİöşüÇĞÖŞÜ",
        punctuation: "",
        note: "dotless ı and dotted İ are distinct letters, not case variants of i and I",
    },
    Language {
        tag: "hun",
        name: "Hungarian",
        script: Script::Latin,
        letters: "áéíóöőúüűÁÉÍÓÖŐÚÜŰ",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "tha",
        name: "Thai",
        script: Script::Other("Thai"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "ice",
        name: "Icelandic",
        script: Script::Latin,
        letters: "áéíóúýþðæöÁÉÍÓÚÝÞÐÆÖ",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "bul",
        name: "Bulgarian",
        script: Script::Other("Cyrillic"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "ara",
        name: "Arabic",
        script: Script::Other("Arabic"),
        letters: "",
        punctuation: "",
        note: "right to left, and cursive: the segmenter's assumptions do not hold either",
    },
    Language {
        tag: "hrv",
        name: "Croatian",
        script: Script::Latin,
        letters: "čćđšžČĆĐŠŽ",
        punctuation: "",
        note: "dž, lj and nj are digraphs of two letters and cost the set nothing",
    },
    Language {
        tag: "slv",
        name: "Slovenian",
        script: Script::Latin,
        letters: "čšžČŠŽ",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "heb",
        name: "Hebrew",
        script: Script::Other("Hebrew"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "est",
        name: "Estonian",
        script: Script::Latin,
        letters: "äöõüšžÄÖÕÜŠŽ",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "lit",
        name: "Lithuanian",
        script: Script::Latin,
        letters: "ąčėęįšųūžĄČĖĘĮŠŲŪŽ",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "ind",
        name: "Indonesian",
        script: Script::Latin,
        letters: "",
        punctuation: "",
        note: "ASCII throughout, which makes it the one non-English language the set already covers",
    },
    Language {
        tag: "hin",
        name: "Hindi",
        script: Script::Other("Devanagari"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "lav",
        name: "Latvian",
        script: Script::Latin,
        letters: "āčēģīķļņšūžĀČĒĢĪĶĻŅŠŪŽ",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "srp",
        name: "Serbian",
        script: Script::Latin,
        letters: "čćđšžČĆĐŠŽ",
        punctuation: "",
        note: "written in either script; the Latin cut is what discs carry, and it is Croatian's",
    },
    Language {
        tag: "slo",
        name: "Slovak",
        script: Script::Latin,
        letters: "áäčďéíĺľňóôŕšťúýžÁÄČĎÉÍĹĽŇÓÔŔŠŤÚÝŽ",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "ukr",
        name: "Ukrainian",
        script: Script::Other("Cyrillic"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "may",
        name: "Malay",
        script: Script::Latin,
        letters: "",
        punctuation: "",
        note: "ASCII throughout, as Indonesian",
    },
    Language {
        tag: "nob",
        name: "Norwegian Bokmal",
        script: Script::Latin,
        letters: "æøåéÆØÅÉ",
        punctuation: "",
        note: "a second tag for a language already tagged nor on 155 files",
    },
    Language {
        tag: "scc",
        name: "Serbian",
        script: Script::Latin,
        letters: "čćđšžČĆĐŠŽ",
        punctuation: "",
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
        note: "Latin script and the largest demand in the library: five tones over twelve vowels, \
               stacked over the vowel's own mark, which is two marks on one body",
    },
    Language {
        tag: "kaz",
        name: "Kazakh",
        script: Script::Other("Cyrillic"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "cat",
        name: "Catalan",
        script: Script::Latin,
        letters: "àçèéíïòóúüÀÇÈÉÍÏÒÓÚÜ",
        punctuation: "·",
        note: "the interpunct is a letter's business here: l·l is a distinct digraph from ll",
    },
    Language {
        tag: "tam",
        name: "Tamil",
        script: Script::Other("Tamil"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "tel",
        name: "Telugu",
        script: Script::Other("Telugu"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "scr",
        name: "Croatian",
        script: Script::Latin,
        letters: "čćđšžČĆĐŠŽ",
        punctuation: "",
        note: "the deprecated tag for hrv",
    },
    Language {
        tag: "aze",
        name: "Azerbaijani",
        script: Script::Latin,
        letters: "çəğıİöşüÇƏĞÖŞÜ",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "frs",
        name: "Eastern Frisian",
        script: Script::Latin,
        letters: "äöüÄÖÜ",
        punctuation: "",
        note: "one file, and the tag is more likely a muxer's mistake for fre or fry than a claim",
    },
    Language {
        tag: "geo",
        name: "Georgian",
        script: Script::Other("Georgian"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "grc",
        name: "Ancient Greek",
        script: Script::Other("Greek"),
        letters: "",
        punctuation: "",
        note: "",
    },
    Language {
        tag: "lat",
        name: "Latin",
        script: Script::Latin,
        letters: "",
        punctuation: "",
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
const TYPOGRAPHY: &str = "\u{2018}\u{2019}\u{201c}\u{201d}\u{201e}\u{2013}\u{2014}\u{2026}";

/// What the matcher did with one probe character in one face.
#[derive(Debug, Clone, Copy)]
enum Verdict {
    /// Matched something, at this distance, to this character. The dangerous answer.
    Rehomed {
        character: char,
        distance: u32,
        ambiguous: bool,
    },
    /// Nothing within threshold. The correct answer, at the nearest distance seen.
    Rejected { distance: u32 },
    /// The face has no outline for it, so this run cannot say.
    NoOutline,
}

/// One probe character's verdict in every face that was loaded.
struct Probed {
    character: char,
    verdicts: Vec<(Style, Verdict)>,
}

impl Probed {
    /// Whether any face rehomed it, which is the column that matters.
    fn rehomes(&self) -> bool {
        self.verdicts
            .iter()
            .any(|(_, v)| matches!(v, Verdict::Rehomed { .. }))
    }

    /// Whether every face that could draw it rejected it.
    fn rejected(&self) -> bool {
        !self.rehomes()
            && self
                .verdicts
                .iter()
                .any(|(_, v)| matches!(v, Verdict::Rejected { .. }))
    }

    /// The closest rehoming across faces, for sorting: nearest is most confident, so worst.
    fn worst_distance(&self) -> u32 {
        self.verdicts
            .iter()
            .filter_map(|(_, v)| match v {
                Verdict::Rehomed { distance, .. } => Some(*distance),
                _ => None,
            })
            .min()
            .unwrap_or(u32::MAX)
    }
}

/// The faces named on the command line, as bytes plus the style each stands for.
fn load_faces(args: &[String]) -> anyhow::Result<Vec<(Style, Vec<u8>)>> {
    let regular = args
        .first()
        .context("usage: language-coverage <regular.ttf> [--italic F] [--bold F]")?;
    let mut paths = vec![(PathBuf::from(regular), Style::Regular)];
    for (flag, style) in [("--italic", Style::Italic), ("--bold", Style::Bold)] {
        if let Some(at) = args.iter().position(|a| a == flag) {
            let path = args
                .get(at + 1)
                .with_context(|| format!("{flag} needs a font"))?;
            paths.push((PathBuf::from(path), style));
        }
    }
    let mut out = Vec::new();
    for (path, style) in paths {
        let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        out.push((style, bytes));
    }
    Ok(out)
}

/// Every character any language in the table requires, with what requires it.
fn demanded() -> BTreeMap<char, Vec<&'static str>> {
    let mut out: BTreeMap<char, Vec<&'static str>> = BTreeMap::new();
    for language in LANGUAGES {
        for ch in language.letters.chars().chain(language.punctuation.chars()) {
            out.entry(ch).or_default().push(language.tag);
        }
    }
    out
}

/// Run every probe character through the real set's matcher.
fn probe(
    faces: &[Face<'_>],
    wanted: &[char],
    matcher: &HammingMatcher,
) -> anyhow::Result<Vec<Probed>> {
    // The ink-cropped rendering alone, because that is the one the runtime's normalisation
    // produces: `Crop::Raster` exists only so a bench can reproduce what the tool did before #99,
    // and probing with it would ask what a matcher would have said two versions ago. Filtered
    // rather than indexed so the choice survives someone adding a rendering.
    let renderings: Vec<Rendering> = RENDERINGS
        .iter()
        .copied()
        .filter(|r| r.crop == Crop::Ink)
        .collect();
    let generated = generate_over("probe", faces, false, &renderings, wanted)?;

    let mut verdicts: BTreeMap<char, Vec<(Style, Verdict)>> = BTreeMap::new();
    for entry in generated.set.entries() {
        let found = matcher.scan_with(&entry.features, entry.metrics, entry.mark, entry.aspect);
        let verdict = match found.character {
            Some(character) => Verdict::Rehomed {
                character,
                distance: found.distance,
                ambiguous: !found.is_unambiguous(matcher.ambiguity_margin()),
            },
            None => Verdict::Rejected { distance: found.distance },
        };
        verdicts
            .entry(entry.character)
            .or_default()
            .push((entry.style, verdict));
    }

    Ok(wanted
        .iter()
        .map(|&character| Probed {
            character,
            verdicts: verdicts.remove(&character).unwrap_or_else(|| {
                faces
                    .iter()
                    .map(|f| (f.style, Verdict::NoOutline))
                    .collect()
            }),
        })
        .collect())
}

/// `-> a 12` for a rehoming, `unread 63` for a rejection.
fn describe(verdict: Verdict) -> String {
    match verdict {
        Verdict::Rehomed { character, distance, ambiguous } => {
            format!("-> {character} {distance}{}", if ambiguous { "?" } else { "" })
        }
        Verdict::Rejected { distance } => format!("unread {distance}"),
        Verdict::NoOutline => "no outline".to_owned(),
    }
}

/// The per-language table: what each orthography needs, and how much of it the set has.
fn report_languages(present: &BTreeSet<char>, probed: &BTreeMap<char, &Probed>) {
    println!("\nCoverage by language\n");
    println!(
        "{:<5} {:<18} {:<10} {:>8} {:>7} {:>6} {:>8} {:>7}",
        "tag", "name", "script", "required", "in set", "absent", "rehomed", "unread"
    );
    for language in LANGUAGES {
        let script = match language.script {
            Script::Latin => "Latin",
            Script::Other(name) => name,
        };
        let required: Vec<char> = language
            .letters
            .chars()
            .chain(language.punctuation.chars())
            .collect();
        let (mut have, mut rehomed, mut unread) = (0usize, 0usize, 0usize);
        for ch in &required {
            if present.contains(ch) {
                have += 1;
            } else if probed.get(ch).is_some_and(|p| p.rehomes()) {
                rehomed += 1;
            } else if probed.get(ch).is_some_and(|p| p.rejected()) {
                unread += 1;
            }
        }
        println!(
            "{:<5} {:<18} {:<10} {:>8} {:>7} {:>6} {:>8} {:>7}",
            language.tag,
            language.name,
            script,
            required.len(),
            have,
            required.len() - have,
            rehomed,
            unread
        );
    }
    println!(
        "\n  A non-Latin row reads 0 required because this table is about characters the set could\n  \
         plausibly gain. What those tracks need is #189 §4's guard, not a wider charset."
    );

    // The notes are where every close call is recorded, and printing them under the table rather
    // than leaving them in the source is the difference between a judgement and an assertion: a
    // reader who disagrees that Finnish does not require `å` can see that the row decided it.
    println!("\nWhere a row made a call\n");
    for language in LANGUAGES.iter().filter(|l| !l.note.is_empty()) {
        println!("  {:<5} {}", language.tag, language.note);
    }
}

/// The per-character table: what the matcher does with a character the set does not have.
fn report_characters(
    probed: &[Probed],
    demands: &BTreeMap<char, Vec<&'static str>>,
    styles: &[Style],
) {
    let mut order: Vec<&Probed> = probed.iter().collect();
    // Most dangerous first: a rehoming at a short distance is one the matcher is most sure of, and
    // sureness is exactly what makes it unfindable downstream.
    order.sort_by_key(|p| (p.worst_distance(), p.character));

    println!("\nWhat the set does with a character it lacks\n");
    print!("{:<6} {:<8} {:<24}", "char", "codepoint", "needed by");
    for style in styles {
        print!(" {:<16}", format!("{style:?}"));
    }
    println!();
    for entry in order {
        let needed = demands
            .get(&entry.character)
            .map_or_else(|| "typography".to_owned(), |tags| tags.join(" "));
        let needed = if needed.len() > 23 {
            format!("{}...", &needed[..20])
        } else {
            needed
        };
        print!(
            "{:<6} U+{:04X}   {:<24}",
            entry.character,
            u32::from(entry.character),
            needed
        );
        for style in styles {
            let verdict = entry
                .verdicts
                .iter()
                .find(|(s, _)| s == style)
                .map_or(Verdict::NoOutline, |(_, v)| *v);
            print!(" {:<16}", describe(verdict));
        }
        println!();
    }
    println!(
        "\n  `-> x N` is the character the pipeline would emit instead, at N cells. `?` marks a\n  \
         match inside the ambiguity margin, which is the only kind post-correction may touch."
    );
}

/// Print the table itself, one language per line, tab separated.
///
/// So that [`LANGUAGES`] is the *only* copy. `scripts/language/census.py` needs the same
/// orthographies to judge an extraction against, and a second table written in Python would drift
/// the first time one of them gained a character — silently, and in the direction of agreeing with
/// whichever tool was consulted last.
///
/// Deliberately not JSON. Nothing here needs a serialiser, and a tab-separated line is readable by
/// a person as well as by a script, which a hand-rolled JSON writer would not be.
fn emit_alphabets() {
    for language in LANGUAGES {
        let script = match language.script {
            Script::Latin => "Latin",
            Script::Other(name) => name,
        };
        println!(
            "{}\t{}\t{}\t{}\t{}",
            language.tag, language.name, script, language.letters, language.punctuation
        );
    }
}

pub fn run(args: &[String]) -> anyhow::Result<()> {
    if args.iter().any(|a| a == "--emit-alphabets") {
        emit_alphabets();
        return Ok(());
    }
    let loaded = load_faces(args)?;
    let faces: Vec<Face<'_>> = loaded
        .iter()
        .map(|(style, bytes)| Face { bytes, style: *style })
        .collect();
    let styles: Vec<Style> = loaded.iter().map(|(style, _)| *style).collect();

    let real = generate_under("real", &faces, false, &RENDERINGS)?;
    let thresholds = MatchThresholds::default();
    let entries = real.set.len();
    let matcher = HammingMatcher::new(real.set, thresholds)?;

    let present: BTreeSet<char> = charset().into_iter().collect();
    let demands = demanded();
    let wanted: Vec<char> = demands
        .keys()
        .copied()
        .chain(TYPOGRAPHY.chars())
        .filter(|ch| !present.contains(ch))
        .collect::<BTreeSet<char>>()
        .into_iter()
        .collect();

    println!(
        "reference set: {entries} entries over {} face(s), {} characters; ceiling {} cells, margin {}",
        faces.len(),
        present.len(),
        thresholds.max_distance(),
        thresholds.ambiguity_margin()
    );
    println!("probing {} characters the set does not have", wanted.len());

    let probed = probe(&faces, &wanted, &matcher)?;
    let by_char: BTreeMap<char, &Probed> = probed.iter().map(|p| (p.character, p)).collect();

    report_languages(&present, &by_char);
    report_characters(&probed, &demands, &styles);

    let rehomed = probed.iter().filter(|p| p.rehomes()).count();
    let rejected = probed.iter().filter(|p| p.rejected()).count();
    println!(
        "\n{} absent characters: {rehomed} rehome silently, {rejected} come back unread, {} have no outline",
        probed.len(),
        probed.len() - rehomed - rejected
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_two_rows_claim_the_same_language_tag() {
        // `census.py` looks a track's orthography up by tag. Two rows for one tag would make the
        // answer depend on which the parser saw last, which is a bug that reads as a measurement.
        let mut seen = BTreeSet::new();
        for language in LANGUAGES {
            assert!(seen.insert(language.tag), "{} appears twice", language.tag);
        }
    }

    #[test]
    fn a_row_requires_no_ascii_character() {
        // The table is a table of *gaps*. `charset()` is ASCII printable plus a list, so an ASCII
        // character in a row could never be absent and would inflate the `required` column with
        // characters the set has had since the first commit.
        for language in LANGUAGES {
            for ch in language.letters.chars().chain(language.punctuation.chars()) {
                assert!(
                    !ch.is_ascii(),
                    "{} lists {ch:?}, which is ASCII and therefore already in the set",
                    language.tag
                );
            }
        }
    }

    #[test]
    fn a_row_lists_no_character_twice() {
        for language in LANGUAGES {
            let mut seen = BTreeSet::new();
            for ch in language.letters.chars().chain(language.punctuation.chars()) {
                assert!(seen.insert(ch), "{} lists {ch:?} twice", language.tag);
            }
        }
    }

    #[test]
    fn every_required_character_is_one_codepoint_rather_than_a_combining_sequence() {
        // The failure this catches is silent and total. The segmenter delivers a glyph, the matcher
        // names a character, and a row written as `a` plus a combining ring would ask for something
        // no reference entry can ever be. It would report the gap as covered and the disc as clean.
        for language in LANGUAGES {
            for ch in language.letters.chars() {
                assert!(
                    !matches!(u32::from(ch), 0x0300..=0x036F),
                    "{} lists a combining mark, {ch:?}: write the precomposed character instead",
                    language.tag
                );
            }
        }
    }

    #[test]
    fn a_non_latin_row_requires_nothing_because_a_wider_charset_is_not_its_answer() {
        // The convention the `required` column depends on. A Cyrillic track is not a charset that
        // needs 33 more entries; it is a track this pipeline should refuse, which is #189's fourth
        // section. Listing its alphabet here would put 33 characters into a table that decides what
        // a reference set grows to, and imply the growing was the plan.
        for language in LANGUAGES {
            if language.script != Script::Latin {
                assert!(
                    language.letters.is_empty() && language.punctuation.is_empty(),
                    "{} is not Latin and lists characters anyway",
                    language.tag
                );
            }
        }
    }

    #[test]
    fn no_character_is_both_required_and_merely_typographic() {
        // The two lists mean different things -- one fails a language, the other fails whatever
        // material happens to be typeset with it -- and a character in both would be counted under
        // whichever the reader consulted first.
        let required: BTreeSet<char> = demanded().into_keys().collect();
        for ch in TYPOGRAPHY.chars() {
            assert!(
                !required.contains(&ch),
                "{ch:?} is in TYPOGRAPHY and is also required by a language"
            );
        }
    }

    #[test]
    fn english_requires_nothing_beyond_ascii_which_is_why_nothing_caught_any_of_this() {
        // Not a tautology worth pinning for its own sake: it is the reason every instrument in this
        // repository could pass while the set could not spell two thirds of the library. If this
        // row ever gains a character, the English bench starts covering it and this document's
        // argument changes shape.
        let english = LANGUAGES.iter().find(|l| l.tag == "eng").unwrap();
        assert!(english.letters.is_empty() && english.punctuation.is_empty());
    }
}
