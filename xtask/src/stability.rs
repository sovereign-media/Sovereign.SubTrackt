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
use subtrackt_core::{FEATURE_GRID, FeatureVector, Rect};
use subtrackt_glyph::binarize::{BinaryMask, CoverageMask};
use subtrackt_glyph::feature::{AspectPolicy, vectorize, vectorize_coverage};
use subtrackt_glyph::reference::Style;

/// Rendering sizes, bracketing the 21–50 px glyph heights the survey measured across the library.
const SIZES: [f32; 5] = [24.0, 32.0, 48.0, 64.0, 96.0];

/// Ink thresholds, standing in for anti-aliasing and palette variation.
const INK_LEVELS: [u8; 3] = [96, 128, 160];

/// The size and ink level treated as the canonical rendering everything else is measured from.
const CANONICAL_SIZE: f32 = 48.0;
const CANONICAL_INK: u8 = 128;

/// Fraction of a grid cell that must be ink for its bit to be set, in percent.
///
/// `feature::CELL_COVERAGE_PERCENT`, which is private there and duplicated here rather than
/// widened: this is a bench asking what that choice implies under a sheared sampling, not a second
/// consumer of it. `geometry.rs` records the same reasoning for the same duplication of `font::INK`.
const CELL_COVERAGE_PERCENT: f64 = 50.0;

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

/// One rendering of one character, in both feature representations.
///
/// Both come from the *same* rasterisation. That is the whole point of measuring them together:
/// the physical variation is identical, so any difference in spread belongs to the representation
/// rather than to a differently-generated sample.
struct Variant {
    style: Style,
    axis: &'static str,
    /// Whether the slant was taken out of the sampling before this vector was built.
    ///
    /// Kept out of the intra- and inter-character distributions entirely. Those are #14's headline
    /// figures and a deskewed variant is not a rendering any material contains — it is a *proposal*
    /// about how to read one, and letting it into the population the design is judged against would
    /// flatter the design with its own remedy.
    deskewed: bool,
    /// From the binary mask, as the pipeline built vectors before grey coverage existed.
    vector: FeatureVector,
    /// From the ink coverage plane, keeping the anti-aliasing ramp as a magnitude.
    grey: FeatureVector,
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

/// Grow or shrink ink by one pixel on a coverage plane.
///
/// Grey morphology — a max or min over the same 4-neighbourhood the binary version uses. Choosing
/// it rather than something smoother is what keeps the comparison exact: thresholding commutes
/// with a flat structuring element, so the binary vectors this file produces are bit-for-bit the
/// ones #14 measured, and the grey column is the only thing that is new.
fn nudge_coverage(mask: &CoverageMask, edge: Edge) -> CoverageMask {
    if edge == Edge::AsIs {
        return mask.clone();
    }
    let grow = edge == Edge::Thicker;
    let mut values = Vec::with_capacity((mask.width() * mask.height()) as usize);

    for y in 0..mask.height() {
        for x in 0..mask.width() {
            let here = mask.get(x, y);
            let neighbours = [
                mask.get(x.wrapping_sub(1), y),
                mask.get(x + 1, y),
                mask.get(x, y.wrapping_sub(1)),
                mask.get(x, y + 1),
            ];
            values.push(if grow {
                neighbours.iter().fold(here, |a, b| a.max(*b))
            } else {
                neighbours.iter().fold(here, |a, b| a.min(*b))
            });
        }
    }
    CoverageMask::from_values(mask.width(), mask.height(), values)
        .expect("the value count is the pixel count by construction")
}

/// One rasterisation, before either vectoriser sees it.
///
/// Split out of [`render`] for #115: a deskew has to estimate the line's slant from the ink, and
/// the ink is exactly what the feature vector has already thrown away. Nothing about the two
/// vectors below changed when this was extracted.
struct Raster {
    mask: BinaryMask,
    grey: CoverageMask,
}

/// Rasterise one character under one set of conditions.
fn raster(font: &Font, ch: char, size: f32, ink: u8, edge: Edge) -> Option<Raster> {
    let (metrics, coverage) = font.rasterize(ch, size);
    let width = u32::try_from(metrics.width).ok()?;
    let height = u32::try_from(metrics.height).ok()?;
    if width == 0 || height == 0 {
        return None;
    }

    // The ink axis models a disc author giving the anti-aliasing ramp more or less weight, so it
    // belongs to the *rendering* and is applied here, in coverage space. Thresholding the rescaled
    // ramp at the runtime's fixed 128 selects exactly the pixels that thresholding the original at
    // `ink` would, so the binary side of this measurement is unchanged from #14 — while the grey
    // side sees the variation as the change in contrast that it physically is.
    let scaled: Vec<u8> = coverage
        .iter()
        .map(|c| u8::try_from(u32::from(*c) * 128 / u32::from(ink)).unwrap_or(u8::MAX))
        .collect();

    let bits: Vec<bool> = scaled.iter().map(|c| *c >= CANONICAL_INK).collect();
    let mask = nudge(&BinaryMask::from_bits(width, height, bits).ok()?, edge);
    if mask.foreground_count() == 0 {
        return None;
    }
    let grey = nudge_coverage(&CoverageMask::from_values(width, height, scaled).ok()?, edge);
    Some(Raster { mask, grey })
}

/// Normalise one rasterisation onto the grid, in both representations.
fn render(raster: &Raster) -> Option<(FeatureVector, FeatureVector)> {
    let bounds = Rect::new(0, 0, raster.mask.width(), raster.mask.height());
    Some((
        vectorize(&raster.mask, bounds, AspectPolicy::Letterbox).ok()?,
        vectorize_coverage(&raster.grey, bounds, AspectPolicy::Letterbox).ok()?,
    ))
}

/// The shear that stands a rendering's ink upright, pooled over every glyph in it.
///
/// `x' = x - k·y` with `k = Cxy / Cyy`, the shear that makes the pooled covariance cross term
/// vanish — which is what "the stems now stand vertical" means as an equation. Each glyph
/// contributes its covariance about **its own** centroid, so the estimate is a property of the
/// letterforms and not of where they were laid out. `mark.rs` reads a diacritic's direction from
/// the same second moment, and `xtask slant` reads a real disc's lines with this same estimator.
///
/// Pooled over the whole charset rather than per character, because a slant estimate from one
/// letter is not one: `A`, `V`, `w` and `y` have diagonal ink that is not slant, and #14 found
/// slant to be constant within a stream. A line is the unit that has it.
fn pooled_shear<'a>(rasters: impl Iterator<Item = &'a Raster>) -> f64 {
    let (mut cyy, mut cxy) = (0f64, 0f64);
    for raster in rasters {
        let mask = &raster.mask;
        let (mut count, mut sum_x, mut sum_y) = (0f64, 0f64, 0f64);
        for y in 0..mask.height() {
            for x in 0..mask.width() {
                if mask.get(x, y) {
                    count += 1.0;
                    sum_x += f64::from(x);
                    sum_y += f64::from(y);
                }
            }
        }
        if count == 0.0 {
            continue;
        }
        let (mean_x, mean_y) = (sum_x / count, sum_y / count);
        for y in 0..mask.height() {
            for x in 0..mask.width() {
                if mask.get(x, y) {
                    let (dx, dy) = (f64::from(x) - mean_x, f64::from(y) - mean_y);
                    cyy += dy * dy;
                    cxy += dx * dy;
                }
            }
        }
    }
    if cyy <= 0.0 { 0.0 } else { cxy / cyy }
}

/// The box the deskewed ink occupies, as `(x, y, width, height)` in continuous coordinates.
///
/// Fractional on the x axis on purpose. The whole caveat #115 records against this candidate is
/// that #99, #110 and #113 were each one side of the pipeline quantising a measurement the other
/// side did not — so the deskew must not introduce a rounding of its own. Nothing here is rounded:
/// the box is the exact image of the rasteriser's own box under the shear, and the sampling below
/// integrates against it directly.
///
/// The ink criterion is coverage above zero, which is the criterion the *upright* side is already
/// letterboxed to: `fontdue` crops its bitmap to exactly the pixels the outline touches. Using the
/// binarised ink here and the raster box there would be the same mismatch again, one issue later.
fn sheared_bounds(raster: &Raster, k: f64) -> Option<(f64, f64, f64, f64)> {
    let grey = &raster.grey;
    let mut found = false;
    let (mut x_lo, mut x_hi) = (f64::MAX, f64::MIN);
    let (mut y_lo, mut y_hi) = (u32::MAX, 0u32);
    for y in 0..grey.height() {
        for x in 0..grey.width() {
            if grey.get(x, y) == 0 {
                continue;
            }
            found = true;
            y_lo = y_lo.min(y);
            y_hi = y_hi.max(y + 1);
            for (cx, cy) in [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)] {
                let sheared = f64::from(cx) - k * f64::from(cy);
                x_lo = x_lo.min(sheared);
                x_hi = x_hi.max(sheared);
            }
        }
    }
    (found && x_hi > x_lo && y_hi > y_lo)
        .then(|| (x_lo, f64::from(y_lo), x_hi - x_lo, f64::from(y_hi - y_lo)))
}

/// How much of the pixel starting at `index` falls inside the span `lo..hi`.
fn overlap(lo: f64, hi: f64, index: f64) -> f64 {
    (hi.min(index + 1.0) - lo.max(index)).max(0.0)
}

/// Fraction of one grid cell that is ink, sampled along slanted columns.
///
/// The same area integration `feature::vectorize` does, with one change: the cell's preimage is a
/// **parallelogram** rather than a rectangle, because the sampling is what carries the shear. The
/// glyph's pixels are never resampled, so no interpolation and no nearest-neighbour rounding stands
/// between the ink and the grid — which is the form #115 asks for and the reason it does so.
///
/// Within one source row the shear offset varies by at most `k`, a fifth of a pixel for an ordinary
/// italic, and the row's own midpoint is used for the whole row. That is a smooth approximation
/// rather than a quantisation: it snaps nothing to a pixel boundary and it shrinks as the rendering
/// grows.
fn sheared_cell(
    ink: &impl Fn(u32, u32) -> f64,
    dims: (u32, u32),
    k: f64,
    bounds: (f64, f64, f64, f64),
    u: (f64, f64),
    v: (f64, f64),
) -> f64 {
    let (width, height) = dims;
    let (bx, by, bw, bh) = bounds;
    let full_area = (u.1 - u.0) * bw * (v.1 - v.0) * bh;
    if full_area <= 0.0 {
        return 0.0;
    }
    // Clipped to the glyph box, so letterbox padding reads as background — and divided by the
    // *unclipped* area, so a cell half outside the glyph is at most half ink. Both are
    // `cell_coverage`'s choices, reproduced because a bench that normalised differently would be
    // measuring its own arithmetic.
    let (xa, xb) = (bx + u.0.clamp(0.0, 1.0) * bw, bx + u.1.clamp(0.0, 1.0) * bw);
    let (ya, yb) = (by + v.0.clamp(0.0, 1.0) * bh, by + v.1.clamp(0.0, 1.0) * bh);
    if xb <= xa || yb <= ya {
        return 0.0;
    }

    let mut covered = 0.0f64;
    let mut row = ya.floor();
    while row < yb {
        let weight_y = overlap(ya, yb, row);
        if weight_y > 0.0 && row >= 0.0 && row < f64::from(height) {
            // The shear, read at the midpoint of the part of this row the cell actually covers.
            let shift = k * f64::midpoint(ya.max(row), yb.min(row + 1.0));
            let (sxa, sxb) = (xa + shift, xb + shift);
            let mut column = sxa.floor();
            while column < sxb {
                if column >= 0.0 && column < f64::from(width) {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let value = ink(column as u32, row as u32);
                    if value > 0.0 {
                        covered += overlap(sxa, sxb, column) * weight_y * value;
                    }
                }
                column += 1.0;
            }
        }
        row += 1.0;
    }
    covered / full_area
}

/// Normalise a rasterisation onto the grid with the slant taken out of the sampling.
///
/// A `k` of zero reproduces [`render`] to within the difference between `f32` and `f64`, which is
/// what `a_zero_shear_reproduces_the_ordinary_vectoriser` pins.
fn render_sheared(raster: &Raster, k: f64) -> Option<(FeatureVector, FeatureVector)> {
    let bounds = sheared_bounds(raster, k)?;
    let dims = (raster.grey.width(), raster.grey.height());
    #[allow(clippy::cast_precision_loss)]
    let grid = FEATURE_GRID as f64;
    // Letterbox: the largest centred box on the grid with the deskewed glyph's aspect ratio.
    let scale = (grid / bounds.2).min(grid / bounds.3);
    let (inner_w, inner_h) = (bounds.2 * scale, bounds.3 * scale);
    let (inner_x, inner_y) = ((grid - inner_w) / 2.0, (grid - inner_h) / 2.0);

    let mut binary = FeatureVector::EMPTY;
    let mut grey = FeatureVector::EMPTY;
    for cell_y in 0..FEATURE_GRID {
        for cell_x in 0..FEATURE_GRID {
            #[allow(clippy::cast_precision_loss)]
            let (cx, cy) = (cell_x as f64, cell_y as f64);
            let u = ((cx - inner_x) / inner_w, (cx + 1.0 - inner_x) / inner_w);
            let v = ((cy - inner_y) / inner_h, (cy + 1.0 - inner_y) / inner_h);
            let from_mask = sheared_cell(
                &|x, y| f64::from(u8::from(raster.mask.get(x, y))),
                dims,
                k,
                bounds,
                u,
                v,
            );
            let from_grey = sheared_cell(
                &|x, y| f64::from(raster.grey.get(x, y)) / 255.0,
                dims,
                k,
                bounds,
                u,
                v,
            );
            if from_mask * 100.0 >= CELL_COVERAGE_PERCENT {
                binary.set(cell_y * FEATURE_GRID + cell_x);
            }
            if from_grey * 100.0 >= CELL_COVERAGE_PERCENT {
                grey.set(cell_y * FEATURE_GRID + cell_x);
            }
        }
    }
    Some((binary, grey))
}

/// The same axis, read after the slant has been taken out of the sampling.
const fn deskewed_axis(axis: &str) -> Option<&'static str> {
    match axis.as_bytes() {
        b"slant (italic)" => Some("slant (italic), deskewed"),
        b"weight + slant" => Some("weight + slant, deskewed"),
        _ => None,
    }
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

/// Every variant of every character, one rendering condition at a time.
///
/// #14 had the loops the other way round, one character at a time. The deskew needs this order: a
/// line estimates its own slant from every glyph standing on it, so the estimate has to be pooled
/// over a whole rendering of the charset before any of it is vectorised. Nothing else about the
/// sample changed — the same conditions produce the same variants in the same order.
fn collect_variants(
    faces: &[(Style, Font)],
    charset: &[char],
    deskew: bool,
) -> BTreeMap<char, Vec<Variant>> {
    let mut out: BTreeMap<char, Vec<Variant>> =
        charset.iter().map(|&ch| (ch, Vec::new())).collect();

    for (style, font) in faces {
        for size in SIZES {
            for ink in INK_LEVELS {
                for edge in [Edge::AsIs, Edge::Thicker, Edge::Thinner] {
                    let rasters: Vec<(char, Raster)> = charset
                        .iter()
                        .filter_map(|&ch| raster(font, ch, size, ink, edge).map(|r| (ch, r)))
                        .collect();
                    let axis = axis_of(*style, size, ink, edge);
                    for (ch, raster) in &rasters {
                        if let (Some(slot), Some((vector, grey))) =
                            (out.get_mut(ch), render(raster))
                        {
                            slot.push(Variant {
                                style: *style,
                                axis,
                                deskewed: false,
                                vector,
                                grey,
                            });
                        }
                    }

                    let Some(axis) = deskewed_axis(axis).filter(|_| deskew) else {
                        continue;
                    };
                    let shear = pooled_shear(rasters.iter().map(|(_, raster)| raster));
                    for (ch, raster) in &rasters {
                        if let (Some(slot), Some((vector, grey))) =
                            (out.get_mut(ch), render_sheared(raster, shear))
                        {
                            slot.push(Variant {
                                style: *style,
                                axis,
                                deskewed: true,
                                vector,
                                grey,
                            });
                        }
                    }
                }
            }
        }
    }
    out
}

/// The axes a deskew is asked to collapse, each beside the row it is measured against.
const DESKEWED: [(&str, &str); 2] = [
    ("slant (italic)", "slant (italic), deskewed"),
    ("weight + slant", "weight + slant, deskewed"),
];

/// Multiples of the estimated shear the sweep below tries.
///
/// A multiple rather than an absolute shear, because the estimate varies a little with rendering
/// size and a fixed `k` would be comparing different conditions to each other. One of these is
/// 0.00 — the undeskewed row — so the sweep contains its own control.
const SHEAR_MULTIPLES: [f64; 9] = [0.0, 0.25, 0.5, 0.75, 1.0, 1.15, 1.25, 1.5, 2.0];

/// Is the moment estimate the shear that actually minimises the distance?
///
/// "It moved the row" and "it moved the row as far as any shear could" are different claims, and
/// only the second lets a residual be read as a letterform. Arial Italic is drawn at 12 degrees,
/// `tan 12` is 0.213, and a moment estimate is not obliged to agree with a design angle: the
/// estimator zeroes a *cross term*, and a face whose round letters were redrawn rather than leaned
/// has ink that no single shear stands upright. This says which of the two the residual is.
///
/// Swept over exactly the population the `slant (italic)` row is taken from, so the 1.00 column is
/// that row and the 0.00 column is the undeskewed one.
fn shear_sweep(faces: &[(Style, Font)], charset: &[char]) {
    let Some((_, regular)) = faces.iter().find(|(style, _)| *style == Style::Regular) else {
        return;
    };
    let upright: BTreeMap<char, FeatureVector> = charset
        .iter()
        .filter_map(|&ch| {
            raster(regular, ch, CANONICAL_SIZE, CANONICAL_INK, Edge::AsIs)
                .and_then(|r| render(&r))
                .map(|(vector, _)| (ch, vector))
        })
        .collect();

    println!("\n=== is the moment estimate the best shear available? (#115) ===");
    println!("  median distance from the upright vector, by multiple of the estimated shear");
    print!("  {:<18} {:>8}", "face", "shear");
    for multiple in SHEAR_MULTIPLES {
        print!(" {multiple:>7.2}");
    }
    println!();

    for (style, font) in faces {
        if !matches!(style, Style::Italic | Style::BoldItalic) {
            continue;
        }
        let mut estimates: Vec<f64> = Vec::new();
        let mut rows: Vec<Vec<u32>> = vec![Vec::new(); SHEAR_MULTIPLES.len()];
        for size in SIZES {
            for ink in INK_LEVELS {
                for edge in [Edge::AsIs, Edge::Thicker, Edge::Thinner] {
                    let rasters: Vec<(char, Raster)> = charset
                        .iter()
                        .filter_map(|&ch| raster(font, ch, size, ink, edge).map(|r| (ch, r)))
                        .collect();
                    let estimate = pooled_shear(rasters.iter().map(|(_, r)| r));
                    estimates.push(estimate);
                    for (slot, multiple) in rows.iter_mut().zip(SHEAR_MULTIPLES) {
                        for (ch, raster) in &rasters {
                            if let (Some(base), Some((vector, _))) =
                                (upright.get(ch), render_sheared(raster, estimate * multiple))
                            {
                                slot.push(base.distance(&vector));
                            }
                        }
                    }
                }
            }
        }
        estimates.sort_by(f64::total_cmp);
        print!(
            "  {:<18} {:>8.3}",
            format!("{style:?}"),
            estimates.get(estimates.len() / 2).copied().unwrap_or(0.0)
        );
        for values in &mut rows {
            values.sort_unstable();
            print!(" {:>7}", values.get(values.len() / 2).copied().unwrap_or(0));
        }
        println!();
    }
    println!("  shear is the median estimate over the rendering conditions, as a slope");
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
    /// The same, split by character, which is what #115's third prediction is about: a true italic
    /// is not an oblique, and no shear recovers a letterform that was redrawn rather than leaned.
    by_axis_character: BTreeMap<&'static str, BTreeMap<char, Vec<u32>>>,
}

/// Build the distributions for one feature representation.
///
/// `pick` chooses which vector each variant contributes, which is the only thing that differs
/// between the two columns of the report.
fn analyse(all: &BTreeMap<char, Vec<Variant>>, pick: fn(&Variant) -> FeatureVector) -> Findings {
    let mut found = Findings::default();
    let mut canonical: BTreeMap<char, FeatureVector> = BTreeMap::new();

    for (ch, variants) in all {
        if let Some(base) = variants.iter().find(|v| v.axis == "canonical") {
            canonical.insert(*ch, pick(base));
        }
    }

    for (ch, variants) in all {
        if let Some(base) = canonical.get(ch) {
            for v in variants {
                let distance = base.distance(&pick(v));
                found.by_axis.entry(v.axis).or_default().push(distance);
                found
                    .by_axis_character
                    .entry(v.axis)
                    .or_default()
                    .entry(*ch)
                    .or_default()
                    .push(distance);
            }
        }
        for (index, a) in variants.iter().enumerate().filter(|(_, a)| !a.deskewed) {
            for b in variants[index + 1..].iter().filter(|b| !b.deskewed) {
                let distance = pick(a).distance(&pick(b));
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

/// What one shear did to each character, worst first.
///
/// #115's third prediction, printed rather than asserted: a shear recovers a letter that was
/// *leaned* and cannot recover one that was **redrawn**. Arial Italic redraws `a`, `e`, `f` and
/// friends outright, so the expected shape of this table is a long tail that falls onto the upright
/// vector and a short head that never will — and the head is the list of characters an italic
/// reference cut is still needed for.
fn per_character(found: &Findings, before: &str, after: &str) {
    let (Some(was), Some(now)) =
        (found.by_axis_character.get(before), found.by_axis_character.get(after))
    else {
        return;
    };
    let median = |values: &Vec<u32>| {
        let mut sorted = values.clone();
        sorted.sort_unstable();
        sorted.get(sorted.len() / 2).copied().unwrap_or(0)
    };

    let mut rows: Vec<(char, u32, u32)> = now
        .iter()
        .filter_map(|(ch, values)| {
            was.get(ch)
                .map(|before| (*ch, median(before), median(values)))
        })
        .collect();
    rows.sort_by_key(|(_, _, after)| std::cmp::Reverse(*after));

    println!("\n--- {after}, per character: median distance from the upright vector ---");
    println!("  the ten the shear helps least, then the ten it helps most");
    for chunk in [
        &rows[..rows.len().min(10)],
        &rows[rows.len().saturating_sub(10)..],
    ] {
        for (ch, was, now) in chunk {
            println!(
                "  {ch}   {was:>4} -> {now:>4}   {:+}",
                i64::from(*now) - i64::from(*was)
            );
        }
        println!("  ...");
    }
}

fn report(label: &str, found: &mut Findings) {
    println!("\n=== {label} ===");
    println!("--- the two distributions that decide the design ---");
    print_distribution("intra-character, all styles", &mut found.intra);
    print_distribution("intra-character, regular upright", &mut found.intra_regular);
    print_distribution("inter-character (nearest other)", &mut found.inter);

    println!("\n--- movement from canonical, by axis ---");
    for (axis, values) in &mut found.by_axis {
        print_distribution(axis, values);
    }

    for (before, after) in DESKEWED {
        let (Some(was), Some(now)) = (
            found.by_axis.get(before).map(|v| percentiles(v)[2]),
            found.by_axis.get(after).map(|v| percentiles(v)[2]),
        ) else {
            continue;
        };
        println!(
            "\n  {before}: median {was} cells, deskewed {now} — {} by {}",
            if now <= was { "FELL" } else { "ROSE" },
            was.abs_diff(now)
        );
        per_character(found, before, after);
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
            "  The distributions OVERLAP, even restricted to upright regular text.\n  \\n             A single vector per character cannot separate characters under this much variation."
        );
    }
}

/// Run the measurement and print both distributions.
///
/// # Errors
/// Propagates font loading failures.
pub fn measure(args: &[String]) -> anyhow::Result<()> {
    let deskew = args.iter().any(|a| a == "--deskew");
    let paths: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let regular = paths.first().context(
        "usage: measure-stability <regular.ttf> [bold.ttf] [italic.ttf] [bold-italic.ttf] \
         [--deskew]",
    )?;

    let mut wanted = vec![(Style::Regular, *regular)];
    for (index, style) in [Style::Bold, Style::Italic, Style::BoldItalic]
        .iter()
        .enumerate()
    {
        if let Some(path) = paths.get(index + 1) {
            wanted.push((*style, *path));
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

    let all = collect_variants(&faces, &charset, deskew);
    let per_character = all.values().map(Vec::len).max().unwrap_or(0);
    println!(
        "characters: {}   faces: {}   variants per character: {per_character}",
        charset.len(),
        faces.len()
    );

    if deskew {
        shear_sweep(&faces, &charset);
    }

    let mut binary = analyse(&all, |v| v.vector);
    let mut grey = analyse(&all, |v| v.grey);
    report("binary mask", &mut binary);
    report("grey coverage", &mut grey);

    // The comparison the experiment exists to make. Intra-character spread is what has to fall:
    // inter-character distance moving with it would just be a rescaling, not an improvement.
    println!("\n=== binary against grey ===");
    let margin = |f: &Findings| {
        let intra = percentiles(&f.intra_regular)[3];
        let inter = percentiles(&f.inter)[1];
        (intra, inter, i64::from(inter) - i64::from(intra))
    };
    let (bi, be, bm) = margin(&binary);
    let (gi, ge, gm) = margin(&grey);
    println!("  binary: intra p75 {bi:<4} inter p25 {be:<4} margin {bm:+}");
    println!("  grey:   intra p75 {gi:<4} inter p25 {ge:<4} margin {gm:+}");
    println!(
        "  intra-character spread {} by {} points; margin {} by {}",
        if gi <= bi { "FELL" } else { "ROSE" },
        i64::from(bi).abs_diff(i64::from(gi)),
        if gm >= bm { "improved" } else { "worsened" },
        bm.abs_diff(gm)
    );
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

    /// A raster whose grey plane is the mask, so the ink box and the raster box coincide.
    fn raster_of(mask: BinaryMask) -> Raster {
        let mut values = Vec::with_capacity((mask.width() * mask.height()) as usize);
        for y in 0..mask.height() {
            for x in 0..mask.width() {
                values.push(if mask.get(x, y) { 255 } else { 0 });
            }
        }
        let grey = CoverageMask::from_values(mask.width(), mask.height(), values)
            .expect("the value count is the pixel count by construction");
        Raster { mask, grey }
    }

    /// A capital `H` filling a `width` by `height` box, so its ink touches all four edges.
    fn aitch(width: u32, height: u32) -> BinaryMask {
        let mut mask = BinaryMask::blank(width, height);
        for y in 0..height {
            mask.set(0, y, true);
            mask.set(width - 1, y, true);
            if y == height / 2 {
                for x in 0..width {
                    mask.set(x, y, true);
                }
            }
        }
        mask
    }

    /// The same `H` leaned so that `x' = x - k·y` stands it upright again.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn leaning_aitch(width: u32, height: u32, k: f64) -> BinaryMask {
        let upright = aitch(width, height);
        let lean = (k.abs() * f64::from(height)).ceil() as u32;
        let mut mask = BinaryMask::blank(width + lean, height);
        for y in 0..height {
            let shift = (k * f64::from(y) + f64::from(lean)).round().max(0.0) as u32;
            for x in 0..width {
                if upright.get(x, y) {
                    mask.set(x + shift, y, true);
                }
            }
        }
        mask
    }

    #[test]
    fn a_zero_shear_reproduces_the_ordinary_vectoriser() {
        // The sheared sampler is the shipped one with a parallelogram in place of a rectangle, so
        // at zero shear it has to *be* the shipped one. Without this the deskewed rows below could
        // be reporting the difference between two integrators rather than the effect of a shear.
        let raster = raster_of(aitch(21, 33));
        let (plain, plain_grey) = render(&raster).expect("a vector");
        let (sheared, sheared_grey) = render_sheared(&raster, 0.0).expect("a vector");
        assert_eq!(plain, sheared, "binary");
        assert_eq!(plain_grey, sheared_grey, "grey");
    }

    #[test]
    fn shearing_a_leaning_glyph_upright_recovers_the_upright_vector() {
        let upright = raster_of(aitch(21, 33));
        let leaning = raster_of(leaning_aitch(21, 33, -0.2));
        let (want, _) = render(&upright).expect("a vector");
        let (before, _) = render(&leaning).expect("a vector");
        let (after, _) = render_sheared(&leaning, -0.2).expect("a vector");
        assert!(
            after.distance(&want) < before.distance(&want),
            "leaning {} cells away, deskewed {}",
            before.distance(&want),
            after.distance(&want)
        );
    }

    #[test]
    fn the_pooled_shear_of_an_upright_rendering_is_zero() {
        let rasters = [raster_of(aitch(21, 33)), raster_of(aitch(9, 33))];
        let shear = pooled_shear(rasters.iter());
        assert!(shear.abs() < 1e-9, "an upright rendering leaned by {shear}");
    }

    #[test]
    fn the_pooled_shear_of_a_leaning_rendering_is_negative() {
        // Same sign convention as `xtask slant` and as `mark::slope_of`: y grows downward, so ink
        // standing further right at the top has a negative cross term.
        let rasters = [raster_of(leaning_aitch(21, 33, -0.2))];
        assert!(pooled_shear(rasters.iter()) < -0.1);
    }

    #[test]
    fn a_deskewed_box_is_narrower_than_the_leaning_one_it_came_from() {
        // What the whole segmentation half of #115 turns on: a leaning glyph's box is mostly slant,
        // and taking the slant out gives the width back.
        let leaning = raster_of(leaning_aitch(21, 33, -0.2));
        let (_, _, wide, _) = sheared_bounds(&leaning, 0.0).expect("a box");
        let (_, _, narrow, _) = sheared_bounds(&leaning, -0.2).expect("a box");
        assert!(narrow < wide - 4.0, "{wide} wide, {narrow} deskewed");
    }

    #[test]
    fn a_glyph_with_no_ink_has_no_deskewed_box() {
        assert_eq!(sheared_bounds(&raster_of(BinaryMask::blank(8, 8)), -0.2), None);
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
