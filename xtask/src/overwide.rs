//! Components that matched something they are much too wide to be.
//!
//! [#183](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/183). #106's de-fusing pass
//! is offered **only** components the matcher returned `unmatched` for, and that is where its whole
//! safety argument lives: a wrong cut costs nothing, because its parts fail to match and the glyph
//! stays exactly as it was. The cost of the rule is that a fusion which *matches something* is
//! invisible to it — and `docs/glyph-hit-list.md` supposed that was a fifth of Gone Girl's `l`
//! class, on the reading that a fused `l` matches `I`.
//!
//! Before anything is cut, the population has to be counted, and counting it needs a definition
//! that does not beg the question. This is that definition: a component **disagrees with its own
//! answer** when its ink stands wider, against its own height, than the reference entry it is
//! nearest for the character it matched. `InkAspect` is carried on both sides already and is
//! already priced by the matcher, so nothing new is measured here — what is new is asking the
//! question per glyph after the answer is known.
//!
//! Excess is a percentage **of the entry's own aspect** rather than a count of pixels, for the
//! reason `CLAUDE.md` §Numbers gives: the same title ships at several resolutions, and a doubling is
//! a doubling at all of them.
//!
//! The answer, on three discs, is that the population is **empty**. `docs/error-census.md` §"The
//! fusion that reads, and why it stays unread" has what that settles.

// Every ratio here divides one count of glyphs by another, well inside what an `f64` counts
// exactly. Same allow, and the same reasoning, as `geometry.rs`.
#![allow(clippy::cast_precision_loss)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Context as _;
use subtrackt::{Config, Pipeline};
use subtrackt_core::GlyphMatcher as _;
use subtrackt_glyph::matcher::{HammingMatcher, MatchThresholds};

/// Excess bands the table reports, in percent of the matched entry's own aspect.
///
/// The top band is open-ended on purpose: a component three times as wide as the character it
/// matched is not a rendering difference by any reading.
const BANDS: [u32; 6] = [0, 25, 50, 100, 200, 300];

/// Excess past which a component is named rather than only counted, in the same units.
///
/// Twice the width the character is drawn at. A rendering difference is not a doubling.
const WIDE_ENOUGH_TO_NAME: u32 = 100;

/// Rows printed before the remainder is summarised. The remainder is *stated*, per `xtask unread`.
const ROWS: usize = 15;

/// One over-wide component: how far past its entry, what it was read as, and both aspects.
type Wide = (u32, char, u32, u32);

/// Count the matched components that stand wider than what they matched.
///
/// # Errors
/// Fails if the media or the reference set cannot be read, or if the pass fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let media: PathBuf = args
        .first()
        .context("usage: overwide <media> <reference.subtref> [--stream N]")?
        .into();
    let set: PathBuf = args.get(1).context("missing the reference set")?.into();
    let stream = match args.iter().position(|a| a == "--stream") {
        Some(at) => Some(
            args.get(at + 1)
                .context("--stream needs a number")?
                .parse()
                .context("--stream takes a number")?,
        ),
        None => None,
    };

    let reference = crate::util::load_reference(&set)?;
    let pipeline =
        Pipeline::new(Config { stream, ..Config::default() }).with_reference(reference.clone());
    let survey = pipeline
        .survey(&media, None)
        .with_context(|| format!("surveying {}", media.display()))?;

    // The same matcher an extraction runs, prepared over the same glyphs, so the answers below are
    // the answers that would have been written.
    let mut matcher = HammingMatcher::new(reference.clone(), MatchThresholds::default())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let glyphs: Vec<subtrackt_core::Glyph> = survey.glyphs.iter().map(to_glyph).collect();
    matcher
        .prepare(&glyphs)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Every entry for each character. A set carries one per style, and a glyph only has to agree
    // with *one* of them to be an ordinary rendering: comparing against the narrowest instead
    // reports every italic glyph on the disc as over-wide, since a slanted stem covers more columns
    // than an upright one, which is #122's whole subject.
    let mut entries: BTreeMap<char, Vec<u32>> = BTreeMap::new();
    for entry in reference.entries() {
        if entry.aspect.known {
            entries
                .entry(entry.character)
                .or_default()
                .push(entry.aspect.permille);
        }
    }

    let mut read = 0u64;
    let mut bands = [0u64; BANDS.len()];
    let mut worst: Vec<Wide> = Vec::new();
    let mut unmatched: Vec<u32> = Vec::new();
    for glyph in &glyphs {
        let answer = matcher
            .match_glyph(glyph)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let (Some(character), true) = (answer.character, glyph.aspect.known) else {
            if glyph.aspect.known {
                unmatched.push(glyph.aspect.permille);
            }
            continue;
        };
        read += 1;
        // The entry this glyph is *closest* to, which is the one that would have to be wrong for
        // the glyph to be an ordinary rendering of what it was read as.
        let Some(&entry) = entries
            .get(&character)
            .and_then(|all| all.iter().min_by_key(|e| e.abs_diff(glyph.aspect.permille)))
        else {
            continue;
        };
        if entry == 0 || glyph.aspect.permille <= entry {
            bands[0] += 1;
            continue;
        }
        let excess = (glyph.aspect.permille - entry) * 100 / entry;
        let band = BANDS
            .iter()
            .rposition(|floor| excess >= *floor)
            .unwrap_or(0);
        bands[band] += 1;
        if excess >= WIDE_ENOUGH_TO_NAME {
            worst.push((excess, character, glyph.aspect.permille, entry));
        }
    }

    report(read, &bands, &worst, &unmatched);
    Ok(())
}

/// Print the census.
fn report(read: u64, bands: &[u64], worst: &[Wide], unmatched: &[u32]) {
    println!("\n--- components wider than what they matched (#183) ---");
    println!("  {read} matched glyphs, each against the entry it is nearest\n");
    println!("  {:>16}  {:>8}  {:>7}", "excess over entry", "glyphs", "share");
    for (index, floor) in BANDS.iter().enumerate() {
        let label = match BANDS.get(index + 1) {
            Some(next) => format!("{floor}-{next}%"),
            None => format!("{floor}%+"),
        };
        println!(
            "  {label:>16}  {:>8}  {:>6.2}%",
            bands[index],
            bands[index] as f64 * 100.0 / read.max(1) as f64
        );
    }

    println!(
        "\n  {} unmatched components, the widest at {} permille -- #106 already reaches those",
        unmatched.len(),
        unmatched.iter().max().copied().unwrap_or(0)
    );

    // Grouped by the character they were read as, because a fusion recurs: the same two letters
    // touch wherever the same word does, and a thousand rows saying `l` would bury the one saying
    // something else.
    let mut by_character: BTreeMap<char, (u64, u32, u32)> = BTreeMap::new();
    for (excess, character, _, entry) in worst {
        let row = by_character
            .entry(*character)
            .or_insert((0, *excess, *entry));
        row.0 += 1;
        row.1 = row.1.max(*excess);
    }
    let mut rows: Vec<(char, (u64, u32, u32))> = by_character.into_iter().collect();
    rows.sort_unstable_by_key(|(_, (count, ..))| std::cmp::Reverse(*count));
    println!("\n  read as, where the ink stands {WIDE_ENOUGH_TO_NAME}% wider than its entry:");
    if rows.is_empty() {
        println!("    nothing: no component matched a character it is far too wide to be");
    }
    for (character, (count, worst_excess, entry)) in rows.iter().take(ROWS) {
        println!("    {character:?}  n={count:<6} worst {worst_excess:>5}%  entry {entry}");
    }
    if rows.len() > ROWS {
        println!("    ... and {} more characters", rows.len() - ROWS);
    }
}

/// A survey record as the matcher wants it.
///
/// The survey carries every field the match key is made of, which is what makes this faithful: the
/// answers here are the answers an extraction would have written, not an approximation of them.
fn to_glyph(record: &subtrackt::GlyphRecord) -> subtrackt_core::Glyph {
    subtrackt_core::Glyph {
        bounds: record.bounds,
        line: record.line,
        features: record.features,
        metrics: record.metrics,
        mark: record.mark,
        aspect: record.aspect,
        upright: subtrackt_core::UprightSpan::of_box(record.bounds),
        slant: subtrackt_core::Slant::UPRIGHT,
    }
}
