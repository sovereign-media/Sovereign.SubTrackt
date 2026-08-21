//! Measuring how much a glyph's vector moves under the variation real subtitles contain.
//!
//! This is #14, and it decides a load-bearing question. If the spread *within* one character stays
//! well below the distance *between* characters, one reference vector per character works and the
//! session cache is an optimisation. If the two distributions overlap, a fixed set cannot separate
//! characters on its own and the cache becomes the mechanism.
//!
//! The variants generated here are the ones §4 of #1 names: weight, slant, rendering size, and the
//! threshold and outline effects that change how much of a glyph survives binarization.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, bail};
use fontdue::{Font, FontSettings};
use subtrackt_core::{FeatureVector, Rect};
use subtrackt_glyph::binarize::BinaryMask;
use subtrackt_glyph::feature::{AspectPolicy, vectorize};
use subtrackt_glyph::reference::Style;

/// Rendering sizes, bracketing the 21–50 px glyph heights the survey measured across the library.
const SIZES: [f32; 5] = [24.0, 32.0, 48.0, 64.0, 96.0];

/// Ink thresholds, standing in for anti-aliasing and palette variation.
const INK_LEVELS: [u8; 3] = [96, 128, 160];

/// The size and ink level treated as the canonical rendering everything else is measured from.
const CANONICAL_SIZE: f32 = 48.0;
const CANONICAL_INK: u8 = 128;

/// Which way a glyph's edge is nudged, standing in for outline thickness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    /// The glyph as rendered.
    AsIs,
    /// One pixel thicker, as if the outline were included in the mask.
    Thicker,
    /// One pixel thinner, as if the threshold ate into the fill.
    Thinner,
}

/// One rendering of one character.
struct Variant {
    style: Style,
    axis: &'static str,
    vector: FeatureVector,
}

/// Grow or shrink the foreground by one pixel, 4-connected.
fn nudge(mask: &BinaryMask, edge: Edge) -> BinaryMask {
    if edge == Edge::AsIs {
        return mask.clone();
    }
    let grow = edge == Edge::Thicker;
    let mut out = BinaryMask::blank(mask.width(), mask.height());

    for y in 0..mask.height() {
        for x in 0..mask.width() {
            let here = mask.get(x, y);
            let neighbours = [
                mask.get(x.wrapping_sub(1), y),
                mask.get(x + 1, y),
                mask.get(x, y.wrapping_sub(1)),
                mask.get(x, y + 1),
            ];
            let value = if grow {
                here || neighbours.iter().any(|n| *n)
            } else {
                here && neighbours.iter().all(|n| *n)
            };
            out.set(x, y, value);
        }
    }
    out
}

/// Rasterise one character under one set of conditions and normalise it.
fn render(font: &Font, ch: char, size: f32, ink: u8, edge: Edge) -> Option<FeatureVector> {
    let (metrics, coverage) = font.rasterize(ch, size);
    let width = u32::try_from(metrics.width).ok()?;
    let height = u32::try_from(metrics.height).ok()?;
    if width == 0 || height == 0 {
        return None;
    }

    let bits: Vec<bool> = coverage.iter().map(|c| *c >= ink).collect();
    let mask = nudge(&BinaryMask::from_bits(width, height, bits).ok()?, edge);
    if mask.foreground_count() == 0 {
        return None;
    }
    vectorize(&mask, Rect::new(0, 0, width, height), AspectPolicy::Letterbox).ok()
}

/// Which axis of variation a rendering represents, relative to the canonical one.
fn axis_of(style: Style, size: f32, ink: u8, edge: Edge) -> &'static str {
    match style {
        Style::Bold => return "weight (bold)",
        Style::Italic => return "slant (italic)",
        Style::BoldItalic => return "weight + slant",
        Style::Regular => {}
    }
    if edge != Edge::AsIs {
        "outline / edge (1px)"
    } else if ink != CANONICAL_INK {
        "anti-aliasing threshold"
    } else if (size - CANONICAL_SIZE).abs() > f32::EPSILON {
        "rendering size"
    } else {
        "canonical"
    }
}

/// Every variant of one character.
fn variants_of(faces: &[(Style, Font)], ch: char) -> Vec<Variant> {
    let mut out = Vec::new();
    for (style, font) in faces {
        for size in SIZES {
            for ink in INK_LEVELS {
                for edge in [Edge::AsIs, Edge::Thicker, Edge::Thinner] {
                    if let Some(vector) = render(font, ch, size, ink, edge) {
                        out.push(Variant {
                            style: *style,
                            axis: axis_of(*style, size, ink, edge),
                            vector,
                        });
                    }
                }
            }
        }
    }
    out
}

/// Percentiles of a sorted sample.
fn percentiles(sorted: &[u32]) -> [u32; 5] {
    let at = |q: usize| sorted[(sorted.len() * q / 100).min(sorted.len() - 1)];
    [at(5), at(25), at(50), at(75), at(95)]
}

fn print_distribution(label: &str, values: &mut [u32]) {
    if values.is_empty() {
        println!("  {label}: no samples");
        return;
    }
    values.sort_unstable();
    let [p5, p25, p50, p75, p95] = percentiles(values);
    println!(
        "  {label:<32} n={:<7} p5={p5:<4} p25={p25:<4} p50={p50:<4} p75={p75:<4} p95={p95:<4} max={}",
        values.len(),
        values.last().copied().unwrap_or(0)
    );
}

/// Load whichever style variants of a face were given on the command line.
fn load_faces(paths: &[(Style, &String)]) -> anyhow::Result<Vec<(Style, Font)>> {
    let mut out = Vec::new();
    for (style, path) in paths {
        let bytes = std::fs::read(Path::new(path)).with_context(|| format!("reading {path}"))?;
        let font = Font::from_bytes(bytes.as_slice(), FontSettings::default())
            .map_err(|e| anyhow::anyhow!("{path}: {e}"))?;
        out.push((*style, font));
    }
    Ok(out)
}

/// What the measurement found, before it is printed.
#[derive(Default)]
struct Findings {
    /// Every pair of variants of the same character.
    intra: Vec<u32>,
    /// The same, restricted to pairs that are both upright and regular.
    intra_regular: Vec<u32>,
    /// From each character's canonical vector to the nearest *other* character.
    inter: Vec<u32>,
    /// Movement from canonical, grouped by which axis moved.
    by_axis: BTreeMap<&'static str, Vec<u32>>,
    characters: usize,
    per_character: usize,
}

fn gather(faces: &[(Style, Font)], charset: &[char]) -> Findings {
    let mut found = Findings { characters: charset.len(), ..Findings::default() };
    let mut canonical: BTreeMap<char, FeatureVector> = BTreeMap::new();
    let mut all: BTreeMap<char, Vec<Variant>> = BTreeMap::new();

    for &ch in charset {
        let variants = variants_of(faces, ch);
        if let Some(base) = variants.iter().find(|v| v.axis == "canonical") {
            canonical.insert(ch, base.vector);
        }
        found.per_character = found.per_character.max(variants.len());
        all.insert(ch, variants);
    }

    for (ch, variants) in &all {
        if let Some(base) = canonical.get(ch) {
            for v in variants {
                found
                    .by_axis
                    .entry(v.axis)
                    .or_default()
                    .push(base.distance(&v.vector));
            }
        }
        for (index, a) in variants.iter().enumerate() {
            for b in &variants[index + 1..] {
                let distance = a.vector.distance(&b.vector);
                found.intra.push(distance);
                // Upright regular is what the bulk of dialogue is; italics are for emphasis and
                // foreign speech, so this split is the one that matters operationally.
                if a.style == Style::Regular && b.style == Style::Regular {
                    found.intra_regular.push(distance);
                }
            }
        }
    }

    for (ch, base) in &canonical {
        if let Some(nearest) = canonical
            .iter()
            .filter(|(other, _)| *other != ch)
            .map(|(_, v)| base.distance(v))
            .min()
        {
            found.inter.push(nearest);
        }
    }
    found
}

fn report(found: &mut Findings, faces: usize) {
    println!(
        "characters: {}   faces: {faces}   variants per character: {}",
        found.characters, found.per_character
    );

    println!("\n--- the two distributions that decide the design ---");
    print_distribution("intra-character, all styles", &mut found.intra);
    print_distribution("intra-character, regular upright", &mut found.intra_regular);
    print_distribution("inter-character (nearest other)", &mut found.inter);

    println!("\n--- movement from canonical, by axis ---");
    for (axis, values) in &mut found.by_axis {
        print_distribution(axis, values);
    }

    let intra_p75 = percentiles(&found.intra)[3];
    let regular_p75 = percentiles(&found.intra_regular)[3];
    let inter_p25 = percentiles(&found.inter)[1];

    println!("\n--- verdict ---");
    println!("  all styles:      intra p75 {intra_p75} against inter p25 {inter_p25}");
    println!("  regular upright: intra p75 {regular_p75} against inter p25 {inter_p25}");
    if regular_p75 < inter_p25 {
        println!("  The distributions separate. One reference vector per character is workable.");
    } else {
        println!(
            "  The distributions OVERLAP, even restricted to upright regular text.\n  \
             A single vector per character cannot separate characters under this much variation."
        );
    }
}

/// Run the measurement and print both distributions.
///
/// # Errors
/// Propagates font loading failures.
pub fn measure(args: &[String]) -> anyhow::Result<()> {
    let regular = args.first().context(
        "usage: measure-stability <regular.ttf> [bold.ttf] [italic.ttf] [bold-italic.ttf]",
    )?;

    let mut wanted = vec![(Style::Regular, regular)];
    for (index, style) in [Style::Bold, Style::Italic, Style::BoldItalic]
        .iter()
        .enumerate()
    {
        if let Some(path) = args.get(index + 1) {
            wanted.push((*style, path));
        }
    }
    let faces = load_faces(&wanted)?;
    if faces.is_empty() {
        bail!("no usable fonts");
    }

    let charset: Vec<char> = (0x21u8..0x7F)
        .map(char::from)
        .filter(char::is_ascii_alphanumeric)
        .collect();

    let mut found = gather(&faces, &charset);
    report(&mut found, faces.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(rows: &[&str]) -> BinaryMask {
        let height = u32::try_from(rows.len()).unwrap();
        let width = u32::try_from(rows[0].len()).unwrap();
        let bits = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c == '#'))
            .collect();
        BinaryMask::from_bits(width, height, bits).unwrap()
    }

    #[test]
    fn thickening_grows_the_foreground_by_one_pixel() {
        let grown = nudge(&mask(&[".....", ".....", "..#..", ".....", "....."]), Edge::Thicker);
        assert!(grown.get(2, 2), "the original pixel stays");
        assert!(grown.get(1, 2) && grown.get(3, 2) && grown.get(2, 1) && grown.get(2, 3));
        assert!(!grown.get(1, 1), "growth is 4-connected, so corners stay clear");
        assert_eq!(grown.foreground_count(), 5);
    }

    #[test]
    fn thinning_shrinks_the_foreground_by_one_pixel() {
        let thinned = nudge(&mask(&["###", "###", "###"]), Edge::Thinner);
        assert!(thinned.get(1, 1), "only the interior survives");
        assert_eq!(thinned.foreground_count(), 1);
    }

    #[test]
    fn thinning_can_erase_a_thin_stroke_entirely() {
        // Which is the point of measuring this axis: a one-pixel threshold shift destroys
        // hairlines, and that is a far bigger change to a vector than it sounds.
        assert_eq!(nudge(&mask(&["#", "#", "#"]), Edge::Thinner).foreground_count(), 0);
    }

    #[test]
    fn leaving_the_edge_alone_is_the_identity() {
        let original = mask(&["#.#", ".#.", "#.#"]);
        assert_eq!(nudge(&original, Edge::AsIs), original);
    }

    #[test]
    fn the_canonical_rendering_is_the_one_everything_is_measured_from() {
        assert_eq!(
            axis_of(Style::Regular, CANONICAL_SIZE, CANONICAL_INK, Edge::AsIs),
            "canonical"
        );
        assert_eq!(
            axis_of(Style::Bold, CANONICAL_SIZE, CANONICAL_INK, Edge::AsIs),
            "weight (bold)"
        );
        assert_eq!(
            axis_of(Style::Regular, 24.0, CANONICAL_INK, Edge::AsIs),
            "rendering size"
        );
        assert_eq!(
            axis_of(Style::Regular, CANONICAL_SIZE, 96, Edge::AsIs),
            "anti-aliasing threshold"
        );
        assert_eq!(
            axis_of(Style::Regular, CANONICAL_SIZE, CANONICAL_INK, Edge::Thinner),
            "outline / edge (1px)"
        );
    }

    #[test]
    fn percentiles_are_ordered_and_in_range() {
        let sorted: Vec<u32> = (0..=100).collect();
        let [p5, p25, p50, p75, p95] = percentiles(&sorted);
        assert!(p5 <= p25 && p25 <= p50 && p50 <= p75 && p75 <= p95);
        assert_eq!(p50, 50);
    }

    #[test]
    fn percentiles_do_not_panic_on_a_single_sample() {
        assert_eq!(percentiles(&[7]), [7, 7, 7, 7, 7]);
    }
}
