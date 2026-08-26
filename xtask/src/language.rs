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
// #230 moved this table into the core crate, because the pipeline began asking the question
// too and `LANGUAGES`'s own doc records the worry that a second copy would drift. This file
// keeps the instrument and no longer keeps the data.
use subtrackt_core::orthography::{LANGUAGES, TYPOGRAPHY};
use subtrackt_glyph::reference::Style;

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
        // `Script` prints itself since #230 unified the two enums -- the shim here carried a
        // `Latin` and an `Other(&str)` because it had no reason to know the rest, and the
        // pipeline's own guard has always known all of them.
        let script = language.script.to_string();
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
        // `Script` prints itself since #230 unified the two enums -- the shim here carried a
        // `Latin` and an `Other(&str)` because it had no reason to know the rest, and the
        // pipeline's own guard has always known all of them.
        let script = language.script.to_string();
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
            if language.script != subtrackt_core::Script::Latin {
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
