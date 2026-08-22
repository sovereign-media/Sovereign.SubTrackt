//! What a reference set confuses with itself.
//!
//! `xtask separability` asks this of a *font*, by rendering the charset and comparing the results.
//! This asks it of a `.subtref` — the artefact that actually ships, which may have been generated
//! from several faces, fitted to a title, or handed over by somebody else.
//!
//! The distinction stops being cosmetic with [#66][66]. A set carrying an upright and an italic
//! vector for every character has twice the entries, and the question is what that costs: whether
//! shearing a typeface moves letters into each other's neighbourhoods, and which ones.
//!
//! **A pair of entries naming the same character is not a confusion.** An upright `a` beside an
//! italic `a` sits at a small distance by construction, and the matcher does not treat it as
//! ambiguous — `scan_with` picks the runner-up from a *different* character, which is what #68
//! changed. Counting those would drown the number this exists to report, so they are excluded and
//! counted separately.
//!
//! [66]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/66

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context as _;

use subtrackt_glyph::ReferenceSet;
use subtrackt_glyph::matcher::MatchThresholds;
use subtrackt_glyph::reference::{ReferenceEntry, Style};

/// How a style reads in a table.
fn style_of(style: Style) -> &'static str {
    match style {
        Style::Regular => "reg",
        Style::Bold => "bold",
        Style::Italic => "ital",
        Style::BoldItalic => "bi",
    }
}

/// Distance between two entries under the shipped matcher.
fn distance(a: &ReferenceEntry, b: &ReferenceEntry, thresholds: MatchThresholds) -> u32 {
    thresholds.distance(&a.features, a.metrics, a.mark, b)
}

/// Report the pairs a set would call ambiguous.
///
/// # Errors
/// Fails if the file cannot be read or is not a reference set.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    anyhow::ensure!(
        args.iter().any(|a| !a.starts_with("--")),
        "usage: set-pairs <set.subtref> [more.subtref...]"
    );
    let thresholds = MatchThresholds::default();
    let margin = thresholds.ambiguity_margin();

    for path in args.iter().filter(|a| !a.starts_with("--")) {
        let bytes = std::fs::read(Path::new(path)).with_context(|| format!("reading {path}"))?;
        let set = ReferenceSet::decode(&bytes).map_err(|e| anyhow::anyhow!("{e}"))?;

        let mut styles: BTreeMap<&'static str, usize> = BTreeMap::new();
        for entry in set.entries() {
            *styles.entry(style_of(entry.style)).or_default() += 1;
        }
        let counted: Vec<String> = styles.iter().map(|(s, n)| format!("{n} {s}")).collect();

        println!(
            "\n--- {} ({} entries: {}) ---",
            set.name(),
            set.len(),
            counted.join(", ")
        );

        let entries = set.entries();
        let mut confusions: Vec<(u32, &ReferenceEntry, &ReferenceEntry)> = Vec::new();
        let mut same_character = 0usize;
        for (index, a) in entries.iter().enumerate() {
            for b in &entries[index + 1..] {
                let apart = distance(a, b, thresholds);
                if apart > margin {
                    continue;
                }
                if a.character == b.character {
                    // The same letter in two cuts. Close by construction, and not a confusion the
                    // matcher can make — see the module docs.
                    same_character += 1;
                } else {
                    confusions.push((apart, a, b));
                }
            }
        }
        confusions.sort_unstable_by_key(|(apart, a, b)| (*apart, a.character, b.character));

        println!("  pairs within the {margin}-cell ambiguity margin, excluding same-character:");
        println!("  {:<14} {:>9}", "pair", "distance");
        for (apart, a, b) in &confusions {
            println!(
                "  {} {:<4} / {} {:<4} {apart:>9}",
                a.character,
                style_of(a.style),
                b.character,
                style_of(b.style)
            );
        }

        // A cross-style confusion is the cost #66 is weighing; a within-style one exists already
        // and would exist in a single-style set too. Separating them is the whole comparison.
        let cross = confusions
            .iter()
            .filter(|(_, a, b)| a.style != b.style)
            .count();
        println!("\n  confusable pairs: {}", confusions.len());
        println!("    of those, across styles: {cross}");
        println!("    same character in two cuts, not counted: {same_character}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::{FeatureVector, LineMetrics, MarkSlope};

    fn entry(character: char, style: Style, bits: &[usize]) -> ReferenceEntry {
        let mut features = FeatureVector::EMPTY;
        for bit in bits {
            features.set(*bit);
        }
        ReferenceEntry {
            character,
            style,
            features,
            metrics: LineMetrics::UNKNOWN,
            mark: MarkSlope::NONE,
        }
    }

    #[test]
    fn two_cuts_of_one_letter_are_close_and_are_not_a_confusion() {
        // The exclusion the whole comparison rests on. An upright `a` and an italic `a` sit within
        // the margin by construction, and since #68 the matcher will not report one as the other's
        // runner-up — so counting them would drown the number that matters.
        let upright = entry('a', Style::Regular, &[1, 2, 3]);
        let italic = entry('a', Style::Italic, &[1, 2, 4]);
        let thresholds = MatchThresholds::default();

        assert!(
            distance(&upright, &italic, thresholds) <= thresholds.ambiguity_margin(),
            "the two cuts are close, which is why they have to be excluded rather than filtered \
             out by distance"
        );
        assert_eq!(upright.character, italic.character);
    }

    #[test]
    fn a_cross_style_pair_of_different_letters_is_a_confusion() {
        let upright = entry('l', Style::Regular, &[1, 2, 3]);
        let italic = entry('/', Style::Italic, &[1, 2, 4]);
        let thresholds = MatchThresholds::default();

        assert!(distance(&upright, &italic, thresholds) <= thresholds.ambiguity_margin());
        assert_ne!(
            upright.character, italic.character,
            "this one the matcher can get wrong"
        );
    }
}
