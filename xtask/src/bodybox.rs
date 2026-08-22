//! Would vectorizing the body alone separate `á` from `é`, and what would it cost?
//!
//! [#100](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/100), benched before
//! anything is built — the convention #37, #46 and #48 established, and the one that caught #48
//! being wrong about its own target.
//!
//! `xtask mark-sweep`'s census found every `á` read as `é` and none as `à`: the accents this
//! pipeline gets wrong are wrong in the **base letter**, which is the one axis a slope term cannot
//! see. The diagnosis #48 recorded is that [`feature::vectorize`](subtrackt_glyph::feature) runs on
//! the *merged* box — base plus mark, unioned by `group` before `feature` ever sees it — so `á` is
//! letterboxed into roughly the bottom five-sixths of the grid while `a` fills all of it. `á` is a
//! third shape, different from `a` and from `e` alike, and which it lands nearer is close to
//! arbitrary.
//!
//! The fix #100 proposes is to hand `vectorize` the **body's** box instead. This measures whether
//! that would work, and — the half #100 does not mention — what it would cost.
//!
//! ## The cost nobody named
//!
//! `mark::mark_box` calls every part except the tallest a mark. It does not know about accents; it
//! knows about *parts*. So `i` and `j` have marks too, and their bodies are bare stems. Dropping
//! the mark from the vector makes `i` the same shape as `l`, `I` and `|` — the pair #10 measured at
//! distance **zero** and the one `docs/error-census.md` still records as the largest error class on
//! a real disc.
//!
//! That is not an argument against #100. It is the reason this bench reports both columns for every
//! character rather than only for the eight the census names.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, bail};
use fontdue::{Font, FontSettings};
use subtrackt_core::{FeatureVector, Rect};
use subtrackt_glyph::binarize::BinaryMask;
use subtrackt_glyph::ccl::{self, ComponentFilter};
use subtrackt_glyph::feature::{AspectPolicy, vectorize};
use subtrackt_glyph::matcher::MatchThresholds;

/// Rendering sizes each character's own noise is measured across.
///
/// The range `docs/library-survey.md` measured real subtitle glyphs at. #14's rule is what the
/// gaps below are judged against: two renderings of one character sit further apart than two
/// different characters do, so a gap only counts when it beats the wobble.
const SIZES: [f32; 6] = [21.0, 24.0, 30.0, 36.0, 42.0, 50.0];

/// Ink thresholds, bracketing the binarizer's half.
const INKS: [u8; 3] = [96, 128, 160];

/// Which box a character is letterboxed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Box2 {
    /// Every part unioned — base plus mark. What ships.
    Merged,
    /// The tallest part alone, which is what `mark::mark_box` already calls the body.
    Body,
}

/// Rasterise one character and normalise it from the chosen box.
///
/// Connected components are labelled the way `font::mark_for` labels them, and the body is picked
/// the way `mark::mark_box` picks it — by height, held as an index rather than compared by value,
/// because a diaeresis arrives as two parts of identical bounds.
fn vector(font: &Font, ch: char, px: f32, ink: u8, which: Box2) -> Option<FeatureVector> {
    let (metrics, coverage) = font.rasterize(ch, px);
    let width = u32::try_from(metrics.width).ok()?;
    let height = u32::try_from(metrics.height).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    let bits: Vec<bool> = coverage.iter().map(|c| *c >= ink).collect();
    let mask = BinaryMask::from_bits(width, height, bits).ok()?;
    if mask.foreground_count() == 0 {
        return None;
    }

    let parts = ccl::label(&mask, ComponentFilter::permissive()).ok()?;
    if parts.is_empty() {
        return None;
    }
    let bounds = match which {
        Box2::Merged => parts.iter().map(|p| p.bounds).reduce(Rect::union)?,
        Box2::Body => parts
            .iter()
            .max_by_key(|p| p.bounds.height)
            .map(|p| p.bounds)?,
    };
    vectorize(&mask, bounds, AspectPolicy::Letterbox).ok()
}

/// One character under one box choice: its canonical vector and its own spread.
struct Measured {
    ch: char,
    canonical: FeatureVector,
    /// Median pairwise distance across every size and ink threshold: the character's own wobble.
    noise: u32,
    /// How many connected components it rasterises to at the canonical rendering.
    parts: usize,
    /// Height as a percentage of the font's cap height, from the **merged** rasterisation.
    ///
    /// Deliberately the merged one under both box choices, because that is what the runtime
    /// measures: `metrics::measure_all` runs on the grouped glyph, and #100 proposes changing which
    /// box `vectorize` sees, not which box the line metrics are read from. It matters — an
    /// accented letter is taller than its base, so #37's term separates `a` from `á` even when the
    /// shape vector no longer can, and a bench that left it out would overstate the damage.
    height: i32,
    /// How far the glyph's bottom sits below the baseline, as a percentage of cap height.
    descent: i32,
}

/// Shape distance plus the weighted metric penalty: what the shipped matcher computes.
fn combined(a: &Measured, b: &Measured) -> u32 {
    let metric = a.height.abs_diff(b.height) + a.descent.abs_diff(b.descent);
    a.canonical.distance(&b.canonical) + metric * MatchThresholds::default().metric_weight() / 100
}

fn median(values: &mut [u32]) -> u32 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

fn measure(font: &Font, which: Box2) -> Vec<Measured> {
    // Cap height from a capital H's ink rather than from a font table, because it has to mean the
    // same thing as the runtime's anchor — the tallest ink on the line.
    let unit = i32::try_from(font.metrics('H', crate::RENDER_PX).height)
        .unwrap_or(1)
        .max(1);
    let mut out = Vec::new();
    for ch in crate::charset() {
        let Some(canonical) = vector(font, ch, crate::RENDER_PX, 128, which) else {
            continue;
        };
        let renders: Vec<FeatureVector> = SIZES
            .iter()
            .flat_map(|px| INKS.iter().map(move |ink| (*px, *ink)))
            .filter_map(|(px, ink)| vector(font, ch, px, ink, which))
            .collect();
        let mut noise = Vec::new();
        for (index, a) in renders.iter().enumerate() {
            for b in &renders[index + 1..] {
                noise.push(a.distance(b));
            }
        }
        let metrics = font.metrics(ch, crate::RENDER_PX);
        out.push(Measured {
            ch,
            canonical,
            noise: median(&mut noise),
            parts: part_count(font, ch),
            height: i32::try_from(metrics.height).unwrap_or(0) * 100 / unit,
            // fontdue reports ymin as the offset of the bitmap's bottom from the baseline,
            // negative when a glyph descends; the runtime measures downwards, so the sign flips.
            descent: -metrics.ymin * 100 / unit,
        });
    }
    out
}

/// Connected components at the canonical rendering: how many parts `group` would see.
fn part_count(font: &Font, ch: char) -> usize {
    let (metrics, coverage) = font.rasterize(ch, crate::RENDER_PX);
    let (Ok(width), Ok(height)) = (u32::try_from(metrics.width), u32::try_from(metrics.height))
    else {
        return 0;
    };
    if width == 0 || height == 0 {
        return 0;
    }
    let bits: Vec<bool> = coverage.iter().map(|c| *c >= 128).collect();
    let Ok(mask) = BinaryMask::from_bits(width, height, bits) else {
        return 0;
    };
    ccl::label(&mask, ComponentFilter::permissive()).map_or(0, |p| p.len())
}

/// Pairs within the ambiguity margin, closest first.
///
/// `with_metrics` decides whether the line-metric term is counted. Both are reported: shape alone
/// says what the representation carries, and shape plus metrics says what the matcher would
/// actually do, which is the number that decides anything.
fn ambiguous(measured: &[Measured], with_metrics: bool) -> Vec<(u32, char, char)> {
    let margin = MatchThresholds::default().ambiguity_margin();
    let mut pairs = Vec::new();
    for (index, a) in measured.iter().enumerate() {
        for b in &measured[index + 1..] {
            let distance = if with_metrics {
                combined(a, b)
            } else {
                a.canonical.distance(&b.canonical)
            };
            if distance <= margin {
                pairs.push((distance, a.ch, b.ch));
            }
        }
    }
    pairs.sort_unstable();
    pairs
}

/// Run the bench.
///
/// # Errors
/// Propagates font loading failures.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let path = crate::accuracy::find_font(args.first())
        .context("no font found; pass one explicitly, e.g. xtask body-box arial.ttf")?;
    let bytes =
        std::fs::read(Path::new(&path)).with_context(|| format!("reading {}", path.display()))?;
    let font = Font::from_bytes(bytes.as_slice(), FontSettings::default())
        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;

    let merged = measure(&font, Box2::Merged);
    let body = measure(&font, Box2::Body);
    if merged.is_empty() {
        bail!("the font rasterised nothing");
    }

    println!("font: {}", path.display());
    println!(
        "characters: {}   ambiguity margin: {} cells   ceiling: {} cells",
        merged.len(),
        MatchThresholds::default().ambiguity_margin(),
        MatchThresholds::default().max_distance()
    );

    accents(&merged, &body);
    multipart(&merged, &body);
    ambiguity(&merged, &body);
    marks(&font, &body);
    Ok(())
}

/// One character's row, by character.
fn find(set: &[Measured], ch: char) -> Option<&Measured> {
    set.iter().find(|m| m.ch == ch)
}

/// #100's first prediction, checked: does the body-only `á` sit at `a` and clear `e`?
fn accents(merged: &[Measured], body: &[Measured]) {
    println!("\n--- the census characters (#100's prediction 1) ---");
    println!(
        "  {:<10} {:>8} {:>9} {:>8} {:>9} {:>9}",
        "character", "merged", "to rival", "body", "+metrics", "to rival"
    );

    // Each accented character, the plain letter under it, and the letter the census says it is
    // actually read as. `docs/glyph-stability.md` has the table this comes from.
    for (accented, base, rival) in [
        ('\u{e0}', 'a', 'e'),
        ('\u{e1}', 'a', 'e'),
        ('\u{e8}', 'e', 'o'),
        ('\u{e9}', 'e', 'o'),
        ('\u{f2}', 'o', 'e'),
        ('\u{f3}', 'o', 'e'),
        ('\u{f9}', 'u', 'o'),
        ('\u{fa}', 'u', 'o'),
    ] {
        let cells = [merged, body].map(|set| {
            let (Some(v), Some(b), Some(r)) =
                (find(set, accented), find(set, base), find(set, rival))
            else {
                return (u32::MAX, u32::MAX, u32::MAX, 0);
            };
            (
                v.canonical.distance(&b.canonical),
                combined(v, b),
                v.canonical.distance(&r.canonical),
                v.noise,
            )
        });
        let show = |v: u32| {
            if v == u32::MAX {
                "-".to_owned()
            } else {
                v.to_string()
            }
        };
        println!(
            "  {accented} vs {base}/{rival} {:>8} {:>9} {:>8} {:>9} {:>9}   noise {}/{}",
            show(cells[0].0),
            show(cells[0].2),
            show(cells[1].0),
            show(cells[1].1),
            show(cells[1].2),
            cells[0].3,
            cells[1].3
        );
    }
    println!(
        "\n  \"to base\" is the distance to the plain letter underneath; \"to rival\" is the\n  \
         distance to the letter the census says it is actually read as. The prediction is that\n  \
         the body-only vector sits within noise of its base and clears its rival by 20 cells.\n  \
         The `+metrics` column is the same body-only pair under the full shipped distance, and it\n  \
         is what says whether the accented letter is still separable from its own base."
    );
}

/// The cost #100 does not name: every character that rasterises to more than one part.
fn multipart(merged: &[Measured], body: &[Measured]) {
    println!("\n--- what else has a 'mark' ---");
    println!("  `mark_box` calls every part except the tallest a mark. It does not know what an");
    println!("  accent is, so a dot counts. These are the characters the change would also move:");
    println!(
        "\n  {:<10} {:>6} {:>16} {:>16} {:>18}",
        "character", "parts", "merged noise", "body noise", "merged vs body"
    );
    let mut moved = 0usize;
    for m in merged {
        let Some(b) = body.iter().find(|b| b.ch == m.ch) else {
            continue;
        };
        let shift = m.canonical.distance(&b.canonical);
        if shift == 0 {
            continue;
        }
        moved += 1;
        if m.parts <= 1 {
            continue;
        }
        println!(
            "  {:<10} {:>6} {:>16} {:>16} {:>18}",
            m.ch, m.parts, m.noise, b.noise, shift
        );
    }
    println!("\n  {moved} of {} characters change vector at all", merged.len());
}

/// What the change does to the set as a whole: pairs inside the ambiguity margin.
fn ambiguity(merged: &[Measured], body: &[Measured]) {
    println!("\n--- pairs inside the ambiguity margin ---");
    println!("  more entries close together means more chances for a wrong character to sit");
    println!("  nearer than the right one. #66 measured one extra cut costing 0.2 points of CER");
    println!("  for a 17% rise in this count, so it is the price side of the ledger.");

    let mut by_box: BTreeMap<&str, Vec<(u32, char, char)>> = BTreeMap::new();
    by_box.insert("1. merged, shape alone", ambiguous(merged, false));
    by_box.insert("2. merged + metrics (shipped)", ambiguous(merged, true));
    by_box.insert("3. body only, shape alone", ambiguous(body, false));
    by_box.insert("4. body only + metrics", ambiguous(body, true));

    for (label, pairs) in &by_box {
        println!("\n  {label}: {} pairs within the margin", pairs.len());
        let zero = pairs.iter().filter(|(d, _, _)| *d == 0).count();
        println!("    {zero} of them at distance zero");
        for (distance, a, b) in pairs.iter().take(14) {
            println!("    {a} / {b}  {distance}");
        }
        if pairs.len() > 14 {
            println!("    ... {} more", pairs.len() - 14);
        }
    }
}

/// Could the mark term put back what the body-only vector throws away?
///
/// The question the rest of this bench forces. If `á` normalises to the same vector as `a`, the
/// *only* thing left to tell them apart is `MarkSlope` — and #48 built that term to separate grave
/// from acute, which is a different question from separating **marked from unmarked**, and a
/// different question again from separating one mark from another.
///
/// The slopes come from the shipped `font::mark_for`, so this is the term as it exists rather than
/// as it might be.
fn marks(font: &Font, body: &[Measured]) {
    println!("\n--- what the mark term could put back ---");
    println!(
        "  {:<12} {:>8} {:>16} {:>22}",
        "pair", "slope", "shape distance", "slope difference"
    );
    let find = |ch: char| body.iter().find(|m| m.ch == ch);
    let thresholds = MatchThresholds::default();

    for family in [
        ['a', '\u{e0}', '\u{e1}', '\u{e2}', '\u{e4}'],
        ['e', '\u{e8}', '\u{e9}', '\u{ea}', '\u{eb}'],
        ['o', '\u{f2}', '\u{f3}', '\u{f4}', '\u{f6}'],
        ['u', '\u{f9}', '\u{fa}', '\u{fb}', '\u{fc}'],
    ] {
        let base = family[0];
        let base_slope = subtrackt_glyph::font::mark_for(font, base);
        for ch in family {
            let slope = subtrackt_glyph::font::mark_for(font, ch);
            let (Some(a), Some(b)) = (find(base), find(ch)) else {
                continue;
            };
            let shape = a.canonical.distance(&b.canonical);
            println!(
                "  {ch} against {base} {:>8} {:>16} {:>22}",
                if slope.known {
                    slope.percent.to_string()
                } else {
                    "none".to_owned()
                },
                shape,
                base_slope.difference(slope).map_or_else(
                    || "no comparison".to_owned(),
                    |points| format!("{points} points")
                )
            );
        }
    }
    println!(
        "\n  A mark's slope is only comparable against another mark's. An unmarked letter reports"
    );
    println!("  `MarkSlope::NONE`, and `difference` returns `None` for it rather than a number —");
    println!("  which is the right refusal, and also means the term contributes nothing at all to");
    println!("  the one pair a body-only vector creates.");
    println!(
        "  The weight is {} permille today, so even where a comparison exists it costs zero cells.",
        thresholds.mark_weight_permille
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_font() -> Option<Font> {
        let path = crate::accuracy::find_font(None)?;
        let bytes = std::fs::read(path).ok()?;
        Font::from_bytes(bytes.as_slice(), FontSettings::default()).ok()
    }

    #[test]
    fn an_accented_letter_normalises_differently_from_the_two_boxes() {
        // The premise of #100, checked directly: if the merged box and the body box gave the same
        // vector for an accented letter there would be nothing to propose.
        let Some(font) = a_font() else {
            return;
        };
        let merged = vector(&font, '\u{e1}', crate::RENDER_PX, 128, Box2::Merged).unwrap();
        let body = vector(&font, '\u{e1}', crate::RENDER_PX, 128, Box2::Body).unwrap();
        assert!(merged.distance(&body) > 0, "the accent has to move the vector");
    }

    #[test]
    fn a_single_part_character_is_the_same_from_either_box() {
        // And the control. An `o` has one component, so the two boxes are the same box, and any
        // difference would mean the labelling or the box choice was wrong rather than the
        // representation.
        let Some(font) = a_font() else {
            return;
        };
        let merged = vector(&font, 'o', crate::RENDER_PX, 128, Box2::Merged).unwrap();
        let body = vector(&font, 'o', crate::RENDER_PX, 128, Box2::Body).unwrap();
        assert_eq!(merged, body);
    }

    #[test]
    fn a_median_of_nothing_is_zero_rather_than_a_panic() {
        assert_eq!(median(&mut []), 0);
        assert_eq!(median(&mut [5, 1, 3]), 3);
    }
}
