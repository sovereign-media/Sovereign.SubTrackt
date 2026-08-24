//! What the reference side and the material side cost each other, measured rather than assumed.
//!
//! [#99](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/99). `docs/glyph-stability.md`
//! prices a one-pixel edge shift at 30 cells — equal to the median distance to an entirely different
//! letter — and rendering size at another 11. Both of those are **manufactured on the reference
//! side**, and nothing had ever measured across the two sides:
//!
//! - [`font::vector_for`](subtrackt_glyph::font::vector_for) rasterises a **plain** glyph at
//!   `RENDER_PX = 96.0` and thresholds it at coverage 128.
//! - Real material is anti-aliased fill inside a 1px dark outline at 21–50px, and the binarizer
//!   keeps the **fill only** — so the material glyph's edge sits where the ramp between fill and
//!   *outline* crosses, not where the ramp between glyph and *background* does.
//!
//! Every failed variance experiment in this project moved the track side. This measures the gap
//! between the sides, which is a different quantity and had no instrument.
//!
//! The material renderings come from [`crate::fixture::render_line`] and
//! [`crate::fixture::core_palette`] — the fixture generator itself, not a second copy of its rules
//! — and go through [`Binarizer`] exactly as the runtime would. If they did not, the answer could be
//! wrong for a reason nobody was measuring.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context as _;
use fontdue::{Font, FontSettings};
use subtrackt_core::{FeatureVector, IndexedBitmap, Rect, SubtitleImage, TimeSpan, Timestamp};
use subtrackt_glyph::binarize::{Binarizer, BinaryMask};
use subtrackt_glyph::feature::{AspectPolicy, vectorize};

/// Glyph heights the library survey found across 56 titles, sampled.
///
/// The bench reports every one of them because #99's first prediction is about the *shape* of the
/// curve — "at least 20 cells at 21px, never below 10 at any surveyed size" — and a single size
/// could satisfy that by accident.
const MATERIAL_SIZES: [f32; 7] = [21.0, 25.0, 29.0, 33.0, 38.0, 43.0, 50.0];

/// The size the decomposition holds constant.
///
/// Mid-range, so neither term of "size against treatment" is being measured at an extreme.
const PIVOT: f32 = 33.0;

/// The distance ceiling, in cells: 20% of a 256-bit vector.
///
/// Spelled from the shipped thresholds rather than written down, because a bench that hard-coded 51
/// would keep answering the old question after someone changed the real one.
fn ceiling() -> u32 {
    subtrackt_glyph::matcher::MatchThresholds::default().max_distance()
}

/// One character's vector as the *material* delivers it, at one rendering size.
///
/// The full runtime path for a single isolated glyph: fill-inside-outline into a palette-indexed
/// plane, through [`Binarizer`], cropped to its ink, letterboxed. Connected-component labelling is
/// skipped for the reason `font::mark_for` skips grouping — a character rendered on its own is
/// already one component, and its bounding box is exactly the box labelling would hand over.
fn material_vector(font: &Font, ch: char, px: f32) -> Option<FeatureVector> {
    let rendered = crate::fixture::render_line(font, &ch.to_string(), px).ok()?;
    let bitmap =
        IndexedBitmap::new(rendered.width, rendered.height, rendered.pixels.clone()).ok()?;
    let image = SubtitleImage {
        span: TimeSpan::new(Timestamp::from_millis(0), Timestamp::from_millis(1)),
        position: Rect::new(0, 0, rendered.width, rendered.height),
        bitmap,
        palette: crate::fixture::core_palette(),
        forced: false,
    };

    let mask = Binarizer::default().mask(&image);
    let bounds = ink_bounds(&mask)?;
    vectorize(&mask, bounds, AspectPolicy::Letterbox).ok()
}

/// The same character rendered **plain** at an arbitrary size, thresholded the way the reference
/// side thresholds.
///
/// This is `font::vector_for` with the size unpinned, and it exists only so the gap can be
/// decomposed. Comparing it against the reference at 96px isolates *size*; comparing it against
/// [`material_vector`] at the same size isolates *edge treatment*. Without both, a single number
/// says the two sides disagree and nothing about which half to fix.
/// Which box the normalisation letterboxes.
///
/// The third mismatch between the two sides, and the one nothing had named.
/// [`font::vector_for`](subtrackt_glyph::font::vector_for) letterboxes the **rasteriser's** box —
/// fontdue returns a bitmap that includes every pixel with any coverage at all, down to 1 — while
/// the runtime letterboxes a *connected component's* box, which is the ink that survived
/// thresholding. At a threshold of 128 those differ by a row or a column on most glyphs, and
/// letterboxing is exactly the operation that turns a row into a whole grid cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Crop {
    /// The rasteriser's box, which is what the reference side uses today.
    Raster,
    /// The bounding box of what the threshold kept, which is what the runtime uses.
    Ink,
}

/// One character rasterised at `px` and thresholded at `ink`.
fn threshold_vector(font: &Font, ch: char, px: f32, ink: u8, crop: Crop) -> Option<FeatureVector> {
    let (metrics, coverage) = font.rasterize(ch, px);
    let width = u32::try_from(metrics.width).ok()?;
    let height = u32::try_from(metrics.height).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    let bits: Vec<bool> = coverage.iter().map(|c| *c >= ink).collect();
    let mask = BinaryMask::from_bits(width, height, &bits).ok()?;
    let bounds = if crop == Crop::Ink {
        ink_bounds(&mask)?
    } else {
        Rect::new(0, 0, width, height)
    };
    vectorize(&mask, bounds, AspectPolicy::Letterbox).ok()
}

/// The bounding box of everything the mask calls foreground.
fn ink_bounds(mask: &BinaryMask) -> Option<Rect> {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (u32::MAX, u32::MAX, 0u32, 0u32);
    let mut any = false;
    for y in 0..mask.height() {
        for x in 0..mask.width() {
            if mask.get(x, y) {
                any = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    any.then(|| Rect::new(min_x, min_y, max_x - min_x + 1, max_y - min_y + 1))
}

/// Percentiles of a sample, sorted in place.
fn percentiles(values: &mut [u32]) -> Option<[u32; 3]> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    // See `crate::util::percentile` for the index and #165 for why it is that one.
    let at = |q: u32| crate::util::percentile(values, q).unwrap_or(0);
    Some([at(50), at(95), values[values.len() - 1]])
}

/// One way a reference glyph could be rendered.
///
/// Three scalars, and between them they are the whole of the reference-versus-material mismatch.
/// `the_outlined_treatment_is_exactly_a_higher_ink_threshold` is what collapses it to this: a
/// disc's fill is a separate palette entry from its anti-aliased edge and the binarizer keeps fill
/// only, so "outlined" is not a shape the reference has to draw — it is a threshold sitting above
/// the 128 the reference side uses, on a box cropped to what survived it.
#[derive(Debug, Clone, Copy)]
struct Rendering {
    px: f32,
    ink: u8,
    crop: Crop,
}

/// The ink threshold the reference side uses today.
const REFERENCE_INK: u8 = 128;

/// The ink threshold a disc's fill implies, as the fixture generator models it.
const MATERIAL_INK: u8 = crate::fixture::FILL_INK;

const fn shipped(px: f32) -> Rendering {
    Rendering { px, ink: REFERENCE_INK, crop: Crop::Raster }
}

const fn material(px: f32) -> Rendering {
    Rendering { px, ink: MATERIAL_INK, crop: Crop::Ink }
}

/// One candidate reference set: every rendering it would carry per character.
///
/// A list rather than one rendering, because the format already carries several entries per
/// character — that is how `gen-reference --italic` puts two cuts in one set — and
/// `HammingMatcher::scan_with` already handles them: a second entry for the winning character can
/// improve the winner and can never become its own runner-up. So a multi-entry set is a change to
/// what `gen-reference` writes, not to the matcher.
struct Candidate {
    label: &'static str,
    renderings: &'static [Rendering],
}

/// The reference sets worth pricing, in cells.
///
/// The first row is what shipped before #99, so every row below reads as a difference from the
/// tool's own history. Rows two to four move one scalar at a time, so a gain can be attributed
/// rather than merely observed.
///
/// **This table ranks the candidates in the wrong order**, and that is worth leaving visible. It
/// picked a four-size material set on both of its columns; `xtask render-sweep` then read every `t`
/// as an `I` with it, and the pair that actually halves a real disc's error rate — the same
/// rasterisation under two different crop boxes — is nothing special here. Cells are not
/// characters. `docs/glyph-stability.md` records the same lesson about `xtask separability`, and
/// `docs/reference-rendering.md` records this instance of it.
const CANDIDATES: [Candidate; 10] = [
    Candidate {
        label: "96px ink128 raster (shipped)",
        renderings: &[shipped(96.0)],
    },
    Candidate { label: "  crop to ink only", renderings: &[INK_96] },
    Candidate {
        label: "  material ink only",
        renderings: &[Rendering { px: 96.0, ink: MATERIAL_INK, crop: Crop::Raster }],
    },
    Candidate { label: "  both, still 96px", renderings: &[material(96.0)] },
    Candidate {
        label: "both boxes (now shipped)",
        renderings: &[shipped(96.0), INK_96],
    },
    Candidate { label: "material 50px", renderings: &[material(50.0)] },
    Candidate { label: "material 21px", renderings: &[material(21.0)] },
    Candidate {
        label: "material 21+50px",
        renderings: &[material(21.0), material(50.0)],
    },
    Candidate {
        label: "material 21+29+38+50px",
        renderings: &[
            material(21.0),
            material(29.0),
            material(38.0),
            material(50.0),
        ],
    },
    Candidate {
        label: "shipped 96 + material 21+50",
        renderings: &[shipped(96.0), material(21.0), material(50.0)],
    },
];

/// The shipped rendering with only the crop changed.
const INK_96: Rendering = Rendering { px: 96.0, ink: REFERENCE_INK, crop: Crop::Ink };

/// Sizes the candidates are *scored* at.
///
/// Deliberately disjoint from every size in [`CANDIDATES`] and [`MATERIAL_SIZES`]. A candidate
/// evaluated at its own rendering size scores zero against itself and would win by construction,
/// which would make the table a description of the arithmetic rather than of the material.
const EVAL_SIZES: [f32; 7] = [22.0, 26.0, 30.0, 35.0, 40.0, 45.0, 49.0];

/// Render one character under one set of conditions.
fn render_as(font: &Font, ch: char, rendering: Rendering) -> Option<FeatureVector> {
    threshold_vector(font, ch, rendering.px, rendering.ink, rendering.crop)
}

/// Everything measured for one character.
struct Row {
    ch: char,
    /// Reference-to-material distance under the shipped rendering, one per size in
    /// [`MATERIAL_SIZES`].
    gaps: Vec<(f32, u32)>,
    /// The character's own spread across the material sizes: every pair, so a gap can be judged
    /// against the wobble the character shows on its own rather than against a fixed number.
    ///
    /// #14's rule, and the one #48 was scored against: two renderings of one character sit further
    /// apart than two different characters do, so a gap only counts when it beats the wobble.
    noise: Vec<u32>,
    /// What the material vector's nearest shipped reference is, at the pivot size.
    reads_as: Option<char>,
    /// Distance to that nearest reference.
    nearest: u32,
}

impl Row {
    fn gap_at(&self, px: f32) -> Option<u32> {
        self.gaps
            .iter()
            .find(|(size, _)| (size - px).abs() < f32::EPSILON)
            .map(|(_, gap)| *gap)
    }

    fn median_noise(&self) -> u32 {
        let mut noise = self.noise.clone();
        percentiles(&mut noise).map_or(0, |[p50, _, _]| p50)
    }
}

/// How one candidate reference rendering reads the material.
struct Scored {
    label: &'static str,
    /// Distance from each material sample to its own character's reference entry.
    gaps: Vec<u32>,
    /// Samples whose nearest entry is further than the ceiling.
    unread: usize,
    /// Samples whose nearest entry names a different character.
    wrong: usize,
    /// Samples where that different character differs only in case.
    wrong_case_only: usize,
    /// Samples scored in total.
    samples: usize,
    /// Characters that went unread at one or more evaluation sizes, with how many.
    ///
    /// The aggregate cannot answer #99's real question. `docs/error-census.md` says one character —
    /// the full stop — is 48.8% of a real disc's errors and 87% of its unread glyphs, so a candidate
    /// that halves the unread count while still failing on that character has bought almost nothing.
    unread_chars: Vec<(char, usize)>,
}

/// Score every candidate rendering against material at [`EVAL_SIZES`].
///
/// This is the measurement #99 asks for and the one that decides it. The gap distribution says the
/// two sides disagree; only this says whether closing it would change an answer, and by how much.
fn score_candidates(font: &Font, charset: &[char]) -> Vec<Scored> {
    // Materialise once. Rendering 139 characters at 7 sizes through the binarizer is the expensive
    // part, and every candidate is scored against the same samples on purpose — a candidate scored
    // against its own re-render would be measuring the rasteriser's determinism.
    let material: Vec<(char, Vec<FeatureVector>)> = charset
        .iter()
        .map(|&ch| {
            (
                ch,
                EVAL_SIZES
                    .iter()
                    .filter_map(|&px| material_vector(font, ch, px))
                    .collect(),
            )
        })
        .collect();

    let ceiling = ceiling();
    CANDIDATES
        .iter()
        .map(|candidate| {
            let references: Vec<(char, FeatureVector)> = charset
                .iter()
                .flat_map(|&ch| {
                    candidate
                        .renderings
                        .iter()
                        .filter_map(move |r| render_as(font, ch, *r).map(|v| (ch, v)))
                })
                .collect();

            let mut scored = Scored {
                label: candidate.label,
                gaps: Vec::new(),
                unread: 0,
                wrong: 0,
                wrong_case_only: 0,
                samples: 0,
                unread_chars: Vec::new(),
            };
            for (ch, vectors) in &material {
                let own: Vec<&FeatureVector> = references
                    .iter()
                    .filter(|(other, _)| other == ch)
                    .map(|(_, v)| v)
                    .collect();
                if own.is_empty() {
                    continue;
                }
                for vector in vectors {
                    scored.samples += 1;
                    // The nearest of this character's own entries, because that is what the matcher
                    // would use. A set carrying several renderings of one character is scored on
                    // its best, exactly as `scan_with` scores it.
                    scored.gaps.push(
                        own.iter()
                            .map(|v| vector.distance(v))
                            .min()
                            .unwrap_or(u32::MAX),
                    );
                    let Some((got, distance)) = references
                        .iter()
                        .map(|(other, r)| (*other, vector.distance(r)))
                        .min_by_key(|(_, d)| *d)
                    else {
                        continue;
                    };
                    if distance > ceiling {
                        scored.unread += 1;
                        match scored.unread_chars.iter_mut().find(|(c, _)| c == ch) {
                            Some((_, count)) => *count += 1,
                            None => scored.unread_chars.push((*ch, 1)),
                        }
                    } else if got != *ch {
                        scored.wrong += 1;
                        // Case pairs are separated by #37's line-metric term, which this bench
                        // excludes on purpose — it is measuring rendering, and the metric term is a
                        // different axis that would mask it. Counting them separately is what keeps
                        // the `wrong` column from claiming a failure the shipped pipeline does not
                        // have.
                        if got.to_lowercase().eq(ch.to_lowercase()) {
                            scored.wrong_case_only += 1;
                        }
                    }
                }
            }
            scored
        })
        .collect()
}

/// Measure the two sides against each other for one font.
///
/// # Errors
/// Propagates font loading failures.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let path = crate::accuracy::find_font(args.first())
        .context("no font found; pass one explicitly, e.g. xtask reference-render arial.ttf")?;
    let bytes =
        std::fs::read(Path::new(&path)).with_context(|| format!("reading {}", path.display()))?;
    let font = Font::from_bytes(bytes.as_slice(), FontSettings::default())
        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;

    let charset = crate::charset();
    // Every character's reference vector, exactly as `gen-reference` would write it into a
    // `.subtref`. This is the side under examination, so it comes from the shipped code.
    let references: BTreeMap<char, FeatureVector> = charset
        .iter()
        .filter_map(|&ch| crate::vector_for(&font, ch, false).map(|v| (ch, v)))
        .collect();

    let mut rows = Vec::new();
    for &ch in &charset {
        let Some(reference) = references.get(&ch) else {
            continue;
        };
        let material: Vec<(f32, FeatureVector)> = MATERIAL_SIZES
            .iter()
            .filter_map(|&px| material_vector(&font, ch, px).map(|v| (px, v)))
            .collect();
        if material.is_empty() {
            continue;
        }

        let gaps = material
            .iter()
            .map(|(px, v)| (*px, reference.distance(v)))
            .collect();
        let mut noise = Vec::new();
        for (index, (_, a)) in material.iter().enumerate() {
            for (_, b) in &material[index + 1..] {
                noise.push(a.distance(b));
            }
        }

        let (reads_as, nearest) = material
            .iter()
            .find(|(px, _)| (px - PIVOT).abs() < f32::EPSILON)
            .and_then(|(_, v)| {
                references
                    .iter()
                    .map(|(other, r)| (*other, v.distance(r)))
                    .min_by_key(|(_, d)| *d)
            })
            .map_or((None, u32::MAX), |(ch, d)| (Some(ch), d));

        rows.push(Row { ch, gaps, noise, reads_as, nearest });
    }

    if let Some(at) = args.iter().position(|a| a == "--show") {
        show_grids(&font, args.get(at + 1).map_or("tIl.", String::as_str));
        return Ok(());
    }

    let mut scored = score_candidates(&font, &charset);
    report(&rows, &mut scored, path.display().to_string().as_str());
    Ok(())
}

fn report(rows: &[Row], scored: &mut [Scored], font_path: &str) {
    let ceiling = ceiling();
    println!("font: {font_path}");
    println!("characters: {}   ceiling: {ceiling} cells", rows.len());
    println!(
        "\n  reference side: plain, {}px, ink 128 -- what font::vector_for produces today",
        subtrackt_glyph::font::RENDER_PX
    );
    println!("  material side:  anti-aliased fill inside a 1px outline, through Binarizer");

    println!("\n--- the shipped reference against material, by rendering size ---");
    println!(
        "  {:>5} {:>7} {:>7} {:>7} {:>11} {:>13}",
        "px", "gap p50", "p95", "max", "over noise", "over ceiling"
    );
    for px in MATERIAL_SIZES {
        let mut gaps: Vec<u32> = rows.iter().filter_map(|r| r.gap_at(px)).collect();
        let over_noise = rows
            .iter()
            .filter(|r| r.gap_at(px).is_some_and(|g| g > r.median_noise()))
            .count();
        let over_ceiling = rows
            .iter()
            .filter(|r| r.gap_at(px).is_some_and(|g| g > ceiling))
            .count();
        let Some([p50, p95, max]) = percentiles(&mut gaps) else {
            continue;
        };
        println!(
            "  {px:>5.0} {p50:>7} {p95:>7} {max:>7} {over_noise:>7} /{:<3} {over_ceiling:>8} /{:<3}",
            rows.len(),
            rows.len()
        );
    }
    println!(
        "  median of each character's own noise across these sizes: {} cells",
        median_of(rows.iter().map(Row::median_noise))
    );

    candidates(scored, ceiling);
    worst(rows, ceiling);
}

/// Median of an iterator of counts.
fn median_of(values: impl Iterator<Item = u32>) -> u32 {
    let mut values: Vec<u32> = values.collect();
    percentiles(&mut values).map_or(0, |[p50, _, _]| p50)
}

/// Price every candidate reference rendering against the same material samples.
fn candidates(scored: &mut [Scored], ceiling: u32) {
    println!(
        "\n--- candidate reference renderings, scored on material at {}-{}px ---",
        EVAL_SIZES[0],
        EVAL_SIZES[EVAL_SIZES.len() - 1]
    );
    println!(
        "  none of the evaluation sizes is a candidate's own, so nothing scores against itself"
    );
    println!(
        "\n  {:<26} {:>7} {:>5} {:>5} {:>9} {:>9} {:>11}",
        "rendering", "gap p50", "p95", "max", "unread", "wrong", "of which case"
    );
    for candidate in scored.iter_mut() {
        let samples = candidate.samples;
        let Some([p50, p95, max]) = percentiles(&mut candidate.gaps) else {
            continue;
        };
        println!(
            "  {:<26} {p50:>7} {p95:>5} {max:>5} {:>9} {:>9} {:>11}",
            candidate.label,
            format!("{} / {samples}", candidate.unread),
            format!("{} / {samples}", candidate.wrong),
            candidate.wrong_case_only
        );
    }
    println!(
        "\n  unread means the nearest entry sits beyond the {ceiling}-cell ceiling; wrong means it\n  \
         names another character. The case column is the part #37's line-metric term separates and\n  \
         this bench excludes on purpose, so it is not a failure the shipped pipeline has."
    );

    // Which characters, not how many. `docs/error-census.md` measured one character as half a real
    // disc's errors, so an aggregate that improved while that character stayed unread would be a
    // candidate that bought nothing where it counts.
    println!(
        "
  characters unread, by candidate (of {} evaluation sizes each):",
        EVAL_SIZES.len()
    );
    for candidate in scored.iter_mut() {
        candidate
            .unread_chars
            .sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let listed: Vec<String> = candidate
            .unread_chars
            .iter()
            .map(|(c, n)| format!("{} x{n}", show(*c)))
            .collect();
        println!(
            "    {:<26} {}",
            candidate.label,
            if listed.is_empty() {
                "none".to_owned()
            } else {
                listed.join("   ")
            }
        );
    }
}

/// The characters the shipped gap is worst for, and what they read as.
fn worst(rows: &[Row], ceiling: u32) {
    println!("\n--- worst characters under the shipped rendering, at {PIVOT:.0}px ---");
    let mut ranked: Vec<&Row> = rows.iter().collect();
    ranked.sort_by_key(|r| std::cmp::Reverse(r.gap_at(PIVOT).unwrap_or(0)));
    println!(
        "  {:>4} {:>6} {:>7} {:>9} {:>9}  reads as",
        "char", "gap", "noise", "nearest", "verdict"
    );
    for row in ranked.iter().take(16) {
        let Some(gap) = row.gap_at(PIVOT) else {
            continue;
        };
        println!(
            "  {:>4} {gap:>6} {:>7} {:>9} {:>9}  {}",
            show(row.ch),
            row.median_noise(),
            row.nearest,
            if row.nearest > ceiling {
                "unread"
            } else if row.reads_as == Some(row.ch) {
                "correct"
            } else {
                "WRONG"
            },
            row.reads_as.map_or_else(|| "-".to_owned(), show)
        );
    }
}

fn art(vector: &FeatureVector) -> Vec<String> {
    (0..subtrackt_core::FEATURE_GRID)
        .map(|y| {
            (0..subtrackt_core::FEATURE_GRID)
                .map(|x| {
                    if vector.get(y * subtrackt_core::FEATURE_GRID + x) {
                        '#'
                    } else {
                        '.'
                    }
                })
                .collect()
        })
        .collect()
}

/// Print the grid for a few characters, side by side across the candidates.
///
/// A distance says two vectors disagree; only the grid says *how*, and #99 needed that twice — once
/// to find that the reference letterboxes the rasteriser's box rather than the ink's, and once to
/// find what a `t` turns into when it does not.
fn show_grids(font: &Font, chars: &str) {
    for ch in chars.chars() {
        println!(
            "
--- {} ---",
            show(ch)
        );
        let mut columns: Vec<(String, Vec<String>)> = Vec::new();
        for candidate in &CANDIDATES {
            if let Some(v) = render_as(font, ch, candidate.renderings[0]) {
                columns.push((candidate.label.trim().to_owned(), art(&v)));
            }
        }
        if let Some(v) = material_vector(font, ch, 33.0) {
            columns.push(("MATERIAL 33px".to_owned(), art(&v)));
        }
        let labels: Vec<String> = columns
            .iter()
            .map(|(label, _)| format!("{:<17}", &label[..label.len().min(17)]))
            .collect();
        println!("  {}", labels.join(" "));
        for row in 0..subtrackt_core::FEATURE_GRID {
            let cells: Vec<String> = columns
                .iter()
                .map(|(_, art)| format!("{:<17}", art[row]))
                .collect();
            println!("  {}", cells.join(" "));
        }
    }
}

/// A character as it should appear in a table cell.
fn show(c: char) -> String {
    match c {
        ' ' => "space".to_owned(),
        c if c.is_control() => format!("U+{:04X}", c as u32),
        c => c.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(rows: &[&str]) -> BinaryMask {
        let height = u32::try_from(rows.len()).unwrap();
        let width = u32::try_from(rows[0].len()).unwrap();
        let bits: Vec<bool> = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c == '#'))
            .collect();
        BinaryMask::from_bits(width, height, &bits).unwrap()
    }

    #[test]
    fn ink_bounds_crop_to_the_foreground_rather_than_the_plane() {
        // The material side renders onto a margined canvas, so a bench that vectorized the whole
        // plane would letterbox the margin along with the glyph and manufacture the very gap it is
        // measuring.
        let bounds = ink_bounds(&mask(&["....", ".##.", ".#..", "...."])).unwrap();
        assert_eq!(bounds, Rect::new(1, 1, 2, 2));
    }

    #[test]
    fn a_mask_with_no_ink_has_no_bounds_rather_than_an_empty_one() {
        assert_eq!(ink_bounds(&mask(&["..", ".."])), None);
    }

    #[test]
    fn the_ceiling_is_read_from_the_shipped_thresholds_rather_than_written_down() {
        // 51 today. A bench that hard-coded it would keep answering the old question after someone
        // changed the real one, which is exactly how #45 cost 12.8 points.
        assert_eq!(
            ceiling(),
            subtrackt_glyph::matcher::MatchThresholds::default().max_distance()
        );
    }

    #[test]
    fn the_outlined_treatment_is_exactly_a_higher_ink_threshold() {
        // The finding that makes the fix a one-line change rather than a rasteriser. A disc's fill
        // is a *separate palette entry* from its anti-aliased edge, and the binarizer keeps fill
        // only -- so the material's mask is the glyph thresholded at the authoring tool's fill
        // cut-off, which sits above the 128 the reference side uses. There is no outline in the
        // mask at all; the outline's effect is entirely that it moves where the fill ends.
        //
        // Pinned because the whole of #99's fix rests on it: if it were false, `gen-reference`
        // would have to draw outlines rather than move a threshold.
        let Some(path) = crate::accuracy::find_font(None) else {
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let Ok(font) = Font::from_bytes(bytes.as_slice(), FontSettings::default()) else {
            return;
        };
        for ch in ['e', 'M', '.', '8'] {
            let outlined = material_vector(&font, ch, 33.0);
            let thresholded =
                threshold_vector(&font, ch, 33.0, crate::fixture::FILL_INK, Crop::Ink);
            assert_eq!(outlined, thresholded, "{ch:?} disagrees between the two paths");
        }
    }

    #[test]
    fn percentiles_of_nothing_is_nothing_rather_than_zero() {
        // A character that rasterises to nothing at one size must drop out of the distribution, not
        // enter it as a zero-cell gap and flatter the result.
        assert_eq!(percentiles(&mut []), None);
        assert_eq!(percentiles(&mut [4, 1, 9]), Some([4, 9, 9]));
    }
}
