//! Normalising a glyph onto the fixed grid.
//!
//! The whole design rests on one property: the same character, rendered at any of the resolutions
//! bitmap subtitles ship at, must land on nearly the same bit vector. Everything here serves that.
//!
//! **Area coverage, not point sampling.** The architecture document says bilinear interpolation.
//! Bilinear is the right tool for magnifying; here the usual case is the reverse — a 40px glyph
//! collapsing onto a 16-cell grid, a 2.5× reduction — and point-sampling a binary mask at that
//! ratio aliases badly, so whether a thin stroke survives depends on where the sample points
//! happen to fall. Each output cell instead measures the *fraction* of the source rectangle it
//! covers that is foreground, with partial pixels weighted by how much of them falls inside. That
//! is what makes a 480p and a 1080p render of the same glyph agree.

// Coverage is computed in f32. Glyph boxes are at most a few thousand pixels on a side and the
// grid is 16 cells, all far inside the 2^24 range f32 represents exactly, so the precision-loss
// lint has nothing to warn about here.
#![allow(clippy::cast_precision_loss)]

use subtrackt_core::{FEATURE_GRID, FeatureVector, Rect, Result};

use crate::binarize::{BinaryMask, CoverageMask};

/// Fraction of a cell that must be foreground for its bit to be set, in percent.
///
/// Half is the neutral choice: it makes the vector of a shape and the vector of the same shape
/// scaled up agree, because area is preserved under scaling while stroke count is not.
const CELL_COVERAGE_PERCENT: f32 = 50.0;

/// How a glyph's aspect ratio is handled when it is squeezed onto a square grid.
///
/// This is the substantive decision in #7. Stretching both an `l` and an `M` to fill the grid
/// discards the width difference that tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AspectPolicy {
    /// Stretch to fill the grid on both axes. Maximum detail, no width information.
    Stretch,
    /// Preserve aspect ratio and centre within the grid, leaving empty cells at the sides.
    ///
    /// The default, because it keeps `l` and `M` apart in the vector itself rather than relying on
    /// a second feature the matcher would have to weigh.
    #[default]
    Letterbox,
}

/// Normalise the glyph at `bounds` in `mask` onto the fixed grid.
///
/// # Errors
/// Returns [`subtrackt_core::Error::Config`] if `bounds` has zero width or height, which means a
/// component filter let an empty box through.
pub fn vectorize(mask: &BinaryMask, bounds: Rect, policy: AspectPolicy) -> Result<FeatureVector> {
    vectorize_with(bounds, policy, |x, y| if mask.get(x, y) { 1.0 } else { 0.0 })
}

/// Normalise a glyph from a [`CoverageMask`] rather than a binary one.
///
/// Identical geometry; the only difference is that each source pixel contributes its ink fraction
/// instead of a whole unit or nothing. `docs/glyph-stability.md` records why: two renderings of one
/// letterform differ mostly in their anti-aliasing ramp, and a binary mask turns that gradient into
/// whole flipped pixels before this function ever sees it. Reading the ramp directly lets the
/// per-cell figure move smoothly with it, so the 50% decision each cell makes lands in the same
/// place more often.
///
/// # Errors
/// As [`vectorize`].
pub fn vectorize_coverage(
    mask: &CoverageMask,
    bounds: Rect,
    policy: AspectPolicy,
) -> Result<FeatureVector> {
    vectorize_with(bounds, policy, |x, y| f32::from(mask.get(x, y)) / 255.0)
}

/// Normalise a glyph onto the grid with its line's slant taken out of the **sampling**.
///
/// [#122](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/122). #14 priced slant at
/// 47 cells median, the most expensive axis in `docs/glyph-stability.md` — above bold's 38 and well
/// above the 31-cell median distance to an entirely *different character*. Sampling along the
/// line's own slant takes it to 26, which is below the 27-cell distance to a different character:
/// a deskewed italic glyph is nearer its own upright entry than a letter is to its nearest
/// neighbour in the alphabet. `docs/italic-slant.md` has the sweep.
///
/// **The sampling, not the pixels.** Each output cell's preimage is a *parallelogram* rather than a
/// rectangle, so the glyph is never resampled and no interpolation or nearest-neighbour rounding
/// stands between the ink and the grid. That form was chosen before it was benched and the reason
/// is a pattern rather than a preference: #99, #110 and #113 were each one side of this pipeline
/// putting a measurement through a quantisation the other side never saw, and a deskewed glyph is
/// matched against an **upright** reference vector — so a resample here would be exactly that
/// asymmetry, with the reference side blind to it.
///
/// `shear` is `k` in `x' = x - k·y`, from [`crate::slant::line_shear`]. Zero reproduces
/// [`vectorize`] to within the difference between `f32` and `f64`, which
/// `a_zero_shear_reproduces_the_ordinary_vectoriser` pins.
///
/// # Errors
/// Returns [`subtrackt_core::Error::Config`] if `bounds` has zero width or height, or if the ink
/// inside it is empty — a glyph with no ink has no deskewed box to letterbox onto, and inventing
/// one would be a fabricated measurement.
pub fn vectorize_sheared(
    bounds: Rect,
    shear: f64,
    policy: AspectPolicy,
    ink: impl Fn(u32, u32) -> f32,
) -> Result<FeatureVector> {
    if bounds.width == 0 || bounds.height == 0 {
        return Err(subtrackt_core::Error::Config(format!(
            "cannot vectorize a {}x{} glyph box",
            bounds.width, bounds.height
        )));
    }
    let (x0, width) = sheared_extent(bounds, shear, &ink).ok_or_else(|| {
        subtrackt_core::Error::Config(format!(
            "the {}x{} box at ({}, {}) holds no ink to deskew",
            bounds.width, bounds.height, bounds.x, bounds.y
        ))
    })?;
    let height = f64::from(bounds.height);

    let grid = f64::from(u32::try_from(FEATURE_GRID).unwrap_or(u32::MAX));
    let (inner_x, inner_y, inner_w, inner_h) = match policy {
        AspectPolicy::Stretch => (0.0, 0.0, grid, grid),
        AspectPolicy::Letterbox => {
            let scale = (grid / width).min(grid / height);
            let (w, h) = (width * scale, height * scale);
            ((grid - w) / 2.0, (grid - h) / 2.0, w, h)
        }
    };

    let mut vector = FeatureVector::EMPTY;
    for cell_y in 0..FEATURE_GRID {
        for cell_x in 0..FEATURE_GRID {
            let (cx, cy) = (cell(cell_x), cell(cell_y));
            let coverage = sheared_cell_coverage(
                &ink,
                bounds,
                (x0, width, height),
                shear,
                (
                    (cx - inner_x) / inner_w,
                    (cx + 1.0 - inner_x) / inner_w,
                    (cy - inner_y) / inner_h,
                    (cy + 1.0 - inner_y) / inner_h,
                ),
            );
            if coverage * 100.0 >= f64::from(CELL_COVERAGE_PERCENT) {
                vector.set(cell_y * FEATURE_GRID + cell_x);
            }
        }
    }
    Ok(vector)
}

/// A grid index as a coordinate. The grid is sixteen cells, so nothing is lost.
#[allow(clippy::cast_precision_loss)]
fn cell(index: usize) -> f64 {
    index as f64
}

/// Where the ink in `bounds` begins and ends once the shear is out, as `(left, width)`.
///
/// In the glyph's own frame — `y` measured from the top of `bounds` — because the vector is
/// letterboxed and therefore translation-invariant, so the pivot cannot reach the answer and a
/// local one keeps the numbers small.
///
/// Taken over each pixel's **square** rather than its top-left corner: a pixel at row `y` occupies
/// rows `y..y+1`, and under a shear those two rows do not map to the same column. Reading the
/// corner alone would lose most of a stem's width at the extremes.
///
/// `None` when the box holds no ink at all.
fn sheared_extent(bounds: Rect, shear: f64, ink: &impl Fn(u32, u32) -> f32) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for y in 0..bounds.height {
        for x in 0..bounds.width {
            if ink(bounds.x + x, bounds.y + y) <= 0.0 {
                continue;
            }
            let (x, y) = (f64::from(x), f64::from(y));
            for (cx, cy) in [(x, y), (x + 1.0, y), (x, y + 1.0), (x + 1.0, y + 1.0)] {
                let sheared = cx - shear * cy;
                lo = lo.min(sheared);
                hi = hi.max(sheared);
            }
        }
    }
    (hi > lo).then_some((lo, hi - lo))
}

/// Fraction of one grid cell that is ink, sampled along slanted columns.
///
/// The same area integration [`cell_coverage`] does, with the cell's preimage a parallelogram
/// rather than a rectangle. Within one source row the shear offset varies by at most `shear` — a
/// fifth of a pixel for an ordinary italic — and the row's own midpoint is used for the whole row.
/// That is a smooth approximation rather than a quantisation: it snaps nothing to a pixel boundary
/// and it shrinks as the rendering grows.
fn sheared_cell_coverage(
    ink: &impl Fn(u32, u32) -> f32,
    bounds: Rect,
    (x0, width, height): (f64, f64, f64),
    shear: f64,
    (u0, u1, v0, v1): (f64, f64, f64, f64),
) -> f64 {
    // Clipped to the glyph box, and divided by the *unclipped* area so a cell half outside the
    // glyph is at most half ink. Both are `cell_coverage`'s choices, kept so the two vectorisers
    // differ in the shape of the preimage and in nothing else.
    let full_area = (u1 - u0) * width * (v1 - v0) * height;
    if full_area <= 0.0 {
        return 0.0;
    }
    let (xa, xb) = (x0 + u0.clamp(0.0, 1.0) * width, x0 + u1.clamp(0.0, 1.0) * width);
    let (ya, yb) = (v0.clamp(0.0, 1.0) * height, v1.clamp(0.0, 1.0) * height);
    if xb <= xa || yb <= ya {
        return 0.0;
    }

    let mut covered = 0.0f64;
    let mut row = ya.floor();
    while row < yb {
        let weight_y = span_overlap_f64(ya, yb, row);
        if weight_y > 0.0 && row >= 0.0 && row < height {
            // The shear, read at the midpoint of the part of this row the cell actually covers.
            let shift = shear * f64::midpoint(ya.max(row), yb.min(row + 1.0));
            let (sxa, sxb) = (xa + shift, xb + shift);
            let mut column = sxa.floor();
            while column < sxb {
                if column >= 0.0 && column < f64::from(bounds.width) {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let (px, py) = (column as u32, row as u32);
                    let value = f64::from(ink(bounds.x + px, bounds.y + py));
                    if value > 0.0 {
                        covered += span_overlap_f64(sxa, sxb, column) * weight_y * value;
                    }
                }
                column += 1.0;
            }
        }
        row += 1.0;
    }
    covered / full_area
}

/// How much of the pixel starting at `index` falls inside the span `lo..hi`.
fn span_overlap_f64(lo: f64, hi: f64, index: f64) -> f64 {
    (hi.min(index + 1.0) - lo.max(index)).max(0.0)
}

/// The shared body of [`vectorize`] and [`vectorize_coverage`], parameterised by how much ink a
/// source pixel holds.
///
/// **Not shared with [`vectorize_sheared`], and #144 row 11 was wrong to read the two as
/// duplicates.** They are the same area integration over preimages that differ, and a rectangle is
/// a parallelogram at zero shear — so the merge looks free, and
/// `a_zero_shear_reproduces_the_ordinary_vectoriser` looks like proof it is. Three things separate
/// them, and two are behaviour rather than arithmetic:
///
/// - **What is letterboxed.** This scales `bounds`; the sheared one scales the *deskewed ink
///   extent*, which is narrower than the box wherever the ink does not reach both edges.
/// - **An empty box.** This returns the empty vector; the sheared one refuses, because a glyph with
///   no ink has no deskewed box and inventing one would be a fabricated measurement.
/// - **Float width.** `f32` here and `f64` there, and the zero-shear test allows for the gap.
///
/// Reconciling those is a change to what the matcher sees, and #154 measured the reason to make it
/// away: segmentation is 85–90% of a run and a whole feature is 1.2 seconds, so there is no cost
/// argument for touching this path. It should be merged when something *needs* it, against a bench,
/// and not for the tidiness.
fn vectorize_with(
    bounds: Rect,
    policy: AspectPolicy,
    ink: impl Fn(u32, u32) -> f32,
) -> Result<FeatureVector> {
    if bounds.width == 0 || bounds.height == 0 {
        return Err(subtrackt_core::Error::Config(format!(
            "cannot vectorize a {}x{} glyph box",
            bounds.width, bounds.height
        )));
    }

    // The sub-rectangle of the grid the glyph is drawn into. Under Letterbox it is the largest
    // centred box with the glyph's aspect ratio; otherwise it is the whole grid.
    let grid = FEATURE_GRID as f32;
    let (inner_x, inner_y, inner_w, inner_h) = match policy {
        AspectPolicy::Stretch => (0.0, 0.0, grid, grid),
        AspectPolicy::Letterbox => {
            let width = bounds.width as f32;
            let height = bounds.height as f32;
            let scale = (grid / width).min(grid / height);
            let w = width * scale;
            let h = height * scale;
            ((grid - w) / 2.0, (grid - h) / 2.0, w, h)
        }
    };

    let mut vector = FeatureVector::EMPTY;
    for cell_y in 0..FEATURE_GRID {
        for cell_x in 0..FEATURE_GRID {
            let coverage = cell_coverage(
                &ink,
                bounds,
                (cell_x as f32 - inner_x) / inner_w,
                (cell_x as f32 + 1.0 - inner_x) / inner_w,
                (cell_y as f32 - inner_y) / inner_h,
                (cell_y as f32 + 1.0 - inner_y) / inner_h,
            );
            if coverage * 100.0 >= CELL_COVERAGE_PERCENT {
                vector.set(cell_y * FEATURE_GRID + cell_x);
            }
        }
    }
    Ok(vector)
}

/// Fraction of one grid cell that is foreground, in `0.0..=1.0`.
///
/// The four arguments are the cell's extent in *glyph-relative* coordinates, where 0.0 is the left
/// or top edge of the bounding box and 1.0 the right or bottom. Anything outside `0.0..=1.0` is
/// letterbox padding and contributes nothing.
fn cell_coverage(
    ink: &impl Fn(u32, u32) -> f32,
    bounds: Rect,
    u0: f32,
    u1: f32,
    v0: f32,
    v1: f32,
) -> f32 {
    let width = bounds.width as f32;
    let height = bounds.height as f32;

    // Clip to the glyph box, so letterbox padding reads as background rather than as a repeat of
    // the edge pixels.
    let x0 = (u0.clamp(0.0, 1.0)) * width;
    let x1 = (u1.clamp(0.0, 1.0)) * width;
    let y0 = (v0.clamp(0.0, 1.0)) * height;
    let y1 = (v1.clamp(0.0, 1.0)) * height;

    // The cell's full area before clipping. Dividing by this rather than by the clipped area is
    // deliberate: a cell half outside the glyph is at most half foreground.
    let full_area = (u1 - u0) * width * (v1 - v0) * height;
    if full_area <= 0.0 || x1 <= x0 || y1 <= y0 {
        return 0.0;
    }

    let mut covered = 0.0f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for py in (y0.floor() as u32)..(y1.ceil() as u32) {
        let weight_y = span_overlap(y0, y1, py);
        if weight_y <= 0.0 {
            continue;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        for px in (x0.floor() as u32)..(x1.ceil() as u32) {
            let value = ink(bounds.x + px, bounds.y + py);
            if value <= 0.0 {
                continue;
            }
            covered += span_overlap(x0, x1, px) * weight_y * value;
        }
    }

    covered / full_area
}

/// How much of integer pixel `index` falls inside the span `lo..hi`.
fn span_overlap(lo: f32, hi: f32, index: u32) -> f32 {
    let pixel_lo = index as f32;
    let pixel_hi = pixel_lo + 1.0;
    (hi.min(pixel_hi) - lo.max(pixel_lo)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stem `width` wide and `height` tall whose top sits `lean` columns right of its foot, drawn
    /// into a canvas wide enough to hold the lean.
    fn leaning_stem(width: u32, height: u32, lean: u32) -> BinaryMask {
        let mut mask = BinaryMask::blank(width + lean, height);
        for y in 0..height {
            let shift = lean * (height - 1 - y) / (height - 1).max(1);
            for x in 0..width {
                mask.set(x + shift, y, true);
            }
        }
        mask
    }

    /// A capital `H` filling the box, so its ink touches all four edges.
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

    /// The same `H`, leaned so that `x' = x - k·y` stands it upright again.
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

    fn whole(mask: &BinaryMask) -> Rect {
        Rect::new(0, 0, mask.width(), mask.height())
    }

    fn sheared_of(mask: &BinaryMask, shear: f64) -> FeatureVector {
        vectorize_sheared(whole(mask), shear, AspectPolicy::Letterbox, |x, y| {
            if mask.get(x, y) { 1.0 } else { 0.0 }
        })
        .expect("has ink")
    }

    #[test]
    fn a_zero_shear_reproduces_the_ordinary_vectoriser() {
        // The sheared sampler is the shipped one with a parallelogram in place of a rectangle, so
        // at zero shear it has to *be* the shipped one. Without this, any figure the deskew moves
        // could be the difference between two integrators rather than the effect of a shear.
        for mask in [aitch(21, 33), aitch(9, 40), leaning_stem(4, 30, 0)] {
            assert_eq!(
                vectorize(&mask, whole(&mask), AspectPolicy::Letterbox).unwrap(),
                sheared_of(&mask, 0.0),
                "a {}x{} glyph",
                mask.width(),
                mask.height()
            );
        }
    }

    #[test]
    fn shearing_a_leaning_glyph_upright_moves_it_toward_the_upright_vector() {
        // #14's most expensive axis, in one assertion. 47 cells median across a real charset, and
        // the whole of #122 is that a shear takes most of it back.
        let upright = aitch(21, 33);
        let leaning = leaning_aitch(21, 33, -0.2);
        let want = vectorize(&upright, whole(&upright), AspectPolicy::Letterbox).unwrap();
        let before = vectorize(&leaning, whole(&leaning), AspectPolicy::Letterbox).unwrap();
        let after = sheared_of(&leaning, -0.2);
        assert!(
            after.distance(&want) < before.distance(&want),
            "leaning {} cells away, deskewed {}",
            before.distance(&want),
            after.distance(&want)
        );
    }

    #[test]
    fn a_deskewed_stem_reads_the_same_at_two_resolutions() {
        // The property the whole normalisation exists for, and a sheared sampling may not lose it:
        // the same letterform at 480p and at 1080p must land on nearly the same bits.
        let small = sheared_of(&leaning_stem(3, 20, 4), -0.2);
        let large = sheared_of(&leaning_stem(9, 60, 12), -0.2);
        assert!(
            small.distance(&large) <= 8,
            "the same stem at two sizes sat {} cells apart",
            small.distance(&large)
        );
    }

    #[test]
    fn a_box_with_no_ink_is_a_configuration_error_not_a_guess() {
        // A glyph with no ink has no deskewed box, and inventing one would put a fabricated
        // measurement into the matcher. `CLAUDE.md` has the rule.
        let blank = BinaryMask::blank(8, 8);
        assert!(
            vectorize_sheared(whole(&blank), -0.2, AspectPolicy::Letterbox, |_, _| 0.0).is_err()
        );
    }

    #[test]
    fn a_zero_sized_box_is_a_configuration_error_for_the_sheared_sampler_too() {
        let mask = aitch(8, 8);
        assert!(
            vectorize_sheared(Rect::new(0, 0, 0, 8), -0.2, AspectPolicy::Letterbox, |x, y| {
                if mask.get(x, y) { 1.0 } else { 0.0 }
            })
            .is_err()
        );
    }

    #[test]
    fn a_deskewed_glyph_sits_nearer_its_upright_self_than_a_leaning_one_does() {
        // The same claim as above at the size real subtitles are drawn, where a stem is three or
        // four pixels and the grid is sixteen cells — which is where a normalisation is most able
        // to throw the difference away.
        let upright = leaning_stem(4, 40, 0);
        let leaning = leaning_stem(4, 40, 8);
        let want = vectorize(&upright, whole(&upright), AspectPolicy::Letterbox).unwrap();
        let before = vectorize(&leaning, whole(&leaning), AspectPolicy::Letterbox).unwrap();
        let after = sheared_of(&leaning, -0.2);
        assert!(after.distance(&want) < before.distance(&want));
    }
    /// Build a mask from rows of `#` and `.`.
    fn mask(rows: &[&str]) -> BinaryMask {
        let height = u32::try_from(rows.len()).unwrap();
        let width = u32::try_from(rows[0].len()).unwrap();
        let bits: Vec<bool> = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c == '#'))
            .collect();
        BinaryMask::from_bits(width, height, &bits).unwrap()
    }

    fn full_box(mask: &BinaryMask) -> Rect {
        Rect::new(0, 0, mask.width(), mask.height())
    }

    /// The tight box around the foreground, which is what connected components hand over in the
    /// real pipeline. Using the whole mask instead would give every glyph the same aspect ratio
    /// and quietly defeat letterboxing.
    fn tight_box(mask: &BinaryMask) -> Rect {
        let mut min_x = u32::MAX;
        let mut min_y = u32::MAX;
        let mut max_x = 0;
        let mut max_y = 0;
        for y in 0..mask.height() {
            for x in 0..mask.width() {
                if mask.get(x, y) {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        Rect::new(min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
    }

    /// Render a letter analytically at any size, so two resolutions are both genuine renders
    /// rather than one being an upscale of the other.
    fn render(letter: char, width: u32, height: u32) -> BinaryMask {
        let mut bits = vec![false; (width * height) as usize];
        let w = width as f32;
        let h = height as f32;

        for y in 0..height {
            for x in 0..width {
                let fx = (x as f32 + 0.5) / w;
                let fy = (y as f32 + 0.5) / h;
                let on = match letter {
                    // An elliptical ring.
                    'O' => {
                        let dx = (fx - 0.5) * 2.0;
                        let dy = (fy - 0.5) * 2.0;
                        let r = dx.mul_add(dx, dy * dy).sqrt();
                        (0.55..=1.0).contains(&r)
                    }
                    // A vertical stem with a foot.
                    'L' => fx < 0.25 || fy > 0.8,
                    // A plain vertical stem.
                    'I' => (0.4..0.6).contains(&fx),
                    // A solid block.
                    _ => true,
                };
                bits[(y * width + x) as usize] = on;
            }
        }
        BinaryMask::from_bits(width, height, &bits).unwrap()
    }

    fn vector_of(letter: char, width: u32, height: u32) -> FeatureVector {
        let m = render(letter, width, height);
        vectorize(&m, tight_box(&m), AspectPolicy::default()).unwrap()
    }

    #[test]
    fn a_solid_box_fills_every_cell() {
        let m = mask(&["####", "####", "####", "####"]);
        let v = vectorize(&m, full_box(&m), AspectPolicy::Stretch).unwrap();
        assert_eq!(v.popcount(), u32::try_from(FEATURE_GRID * FEATURE_GRID).unwrap());
    }

    #[test]
    fn an_empty_box_sets_no_cells() {
        let m = mask(&["....", "....", "....", "...."]);
        let v = vectorize(&m, full_box(&m), AspectPolicy::Stretch).unwrap();
        assert_eq!(v.popcount(), 0);
    }

    #[test]
    fn a_zero_sized_box_is_a_configuration_error_not_a_panic() {
        let m = mask(&["####"]);
        assert!(vectorize(&m, Rect::new(0, 0, 0, 1), AspectPolicy::Stretch).is_err());
        assert!(vectorize(&m, Rect::new(0, 0, 4, 0), AspectPolicy::Stretch).is_err());
    }

    #[test]
    fn stretching_fills_the_grid_and_letterboxing_leaves_margins() {
        // A wide, short glyph. Stretched it occupies every cell; letterboxed it keeps its shape
        // and leaves the top and bottom of the grid empty.
        let m = mask(&["########", "########"]);
        let stretched = vectorize(&m, full_box(&m), AspectPolicy::Stretch).unwrap();
        let boxed = vectorize(&m, full_box(&m), AspectPolicy::Letterbox).unwrap();

        assert_eq!(
            stretched.popcount(),
            u32::try_from(FEATURE_GRID * FEATURE_GRID).unwrap()
        );
        assert!(
            boxed.popcount() < stretched.popcount(),
            "letterboxing must leave padding"
        );
        assert!(boxed.popcount() > 0);
    }

    #[test]
    fn letterboxing_keeps_a_narrow_glyph_narrow() {
        // The reason Letterbox is the default. Stretched to fill the grid, a thin column and a
        // solid block are literally the same vector; letterboxed, the column stays a column.
        let narrow = render('I', 8, 40);
        let block = render('#', 8, 40);
        let (nb, bb) = (tight_box(&narrow), tight_box(&block));

        let stretched_narrow = vectorize(&narrow, nb, AspectPolicy::Stretch).unwrap();
        let stretched_block = vectorize(&block, bb, AspectPolicy::Stretch).unwrap();
        assert_eq!(
            stretched_narrow, stretched_block,
            "stretching collapses the width difference entirely"
        );

        let boxed_narrow = vectorize(&narrow, nb, AspectPolicy::Letterbox).unwrap();
        let boxed_block = vectorize(&block, bb, AspectPolicy::Letterbox).unwrap();
        assert!(
            boxed_narrow.distance(&boxed_block) > 0,
            "letterboxing must keep them apart"
        );
    }

    #[test]
    fn the_same_character_at_two_resolutions_lands_on_nearly_the_same_vector() {
        // The property the entire design rests on. If this does not hold, nothing downstream can
        // work, and the architecture document says so explicitly.
        let budget = u32::try_from(FEATURE_GRID * FEATURE_GRID).unwrap() / 10;

        for letter in ['O', 'L', 'I'] {
            // Roughly 480p and 1080p renders of the same character.
            let small = vector_of(letter, 12, 18);
            let large = vector_of(letter, 27, 40);
            let distance = small.distance(&large);

            assert!(
                distance <= budget,
                "{letter} moved {distance} cells between resolutions, budget {budget}"
            );
        }
    }

    #[test]
    fn different_characters_stay_much_further_apart_than_the_same_one_across_scales() {
        // Scale invariance is only worth anything if it does not also collapse distinct
        // characters together.
        let o_small = vector_of('O', 12, 18);
        let o_large = vector_of('O', 27, 40);
        let l_large = vector_of('L', 27, 40);

        let across_scale = o_small.distance(&o_large);
        let across_letters = o_large.distance(&l_large);

        assert!(
            across_letters > across_scale * 3,
            "O-to-L is {across_letters} but O-across-scales is {across_scale}; \
             the matcher needs a much wider margin than that"
        );
    }

    #[test]
    fn vectorizing_reads_only_inside_the_glyph_box() {
        // The bounding box is a window onto a larger mask, and neighbouring glyphs must not leak
        // into the vector.
        let m = mask(&["####....", "####....", "....####", "....####"]);
        let left = vectorize(&m, Rect::new(0, 0, 4, 2), AspectPolicy::Stretch).unwrap();
        let right = vectorize(&m, Rect::new(4, 2, 4, 2), AspectPolicy::Stretch).unwrap();

        assert_eq!(left.popcount(), u32::try_from(FEATURE_GRID * FEATURE_GRID).unwrap());
        assert_eq!(left, right, "both windows hold a solid block and must agree");
    }

    #[test]
    fn a_glyph_smaller_than_the_grid_still_vectorizes() {
        // Upscaling rather than downscaling: a 3x5 component from a low-resolution DVD subtitle.
        let m = mask(&["###", "#..", "###", "#..", "###"]);
        let v = vectorize(&m, full_box(&m), AspectPolicy::Letterbox).unwrap();
        assert!(v.popcount() > 0);
        assert!(v.popcount() < u32::try_from(FEATURE_GRID * FEATURE_GRID).unwrap());
    }

    /// Build a coverage plane from rows of digits 0-9, scaled to 0..=255.
    fn grey(rows: &[&str]) -> CoverageMask {
        let height = u32::try_from(rows.len()).unwrap();
        let width = u32::try_from(rows[0].len()).unwrap();
        let values = rows
            .iter()
            .flat_map(|r| {
                r.chars()
                    .map(|c| u8::try_from(c.to_digit(10).unwrap() * 255 / 9).unwrap())
            })
            .collect();
        CoverageMask::from_values(width, height, values).unwrap()
    }

    #[test]
    fn a_hard_edged_coverage_plane_gives_the_same_vector_as_the_binary_mask() {
        // The property that makes the two paths comparable at all: where there is no anti-aliasing
        // to read, reading it must change nothing.
        let rows = ["9900", "9900", "0099", "0099"];
        let binary = mask(&["##..", "##..", "..##", "..##"]);
        let coverage = grey(&rows);

        for policy in [AspectPolicy::Stretch, AspectPolicy::Letterbox] {
            let bounds = Rect::new(0, 0, 4, 4);
            assert_eq!(
                vectorize(&binary, bounds, policy).unwrap(),
                vectorize_coverage(&coverage, bounds, policy).unwrap(),
                "{policy:?} disagrees on a plane with no partial coverage"
            );
        }
    }

    #[test]
    fn uniform_grey_decides_every_cell_the_same_way_at_the_halfway_point() {
        let bounds = Rect::new(0, 0, 4, 4);
        let dim = grey(&["4444", "4444", "4444", "4444"]);
        let bright = grey(&["5555", "5555", "5555", "5555"]);

        assert_eq!(
            vectorize_coverage(&dim, bounds, AspectPolicy::Stretch)
                .unwrap()
                .popcount(),
            0,
            "below half coverage sets nothing"
        );
        assert_eq!(
            vectorize_coverage(&bright, bounds, AspectPolicy::Stretch)
                .unwrap()
                .popcount(),
            u32::try_from(FEATURE_GRID * FEATURE_GRID).unwrap(),
            "above half coverage sets everything"
        );
    }

    #[test]
    fn the_coverage_path_sees_differences_the_binary_path_cannot() {
        // The reason the coverage path exists at all, stated as the property that distinguishes it.
        //
        // Two planes whose every pixel falls on the same side of the binarizer's threshold produce
        // *identical* binary masks and therefore identical binary vectors — the difference between
        // them is invisible. The same two planes carry different amounts of ink, so a cell that
        // averages several pixels can land on different sides of the 50% decision.
        //
        // A cell spans three source pixels here, which is why it takes a downscale: at one pixel
        // per cell there is nothing to average and the two representations must agree.
        let size = u32::try_from(FEATURE_GRID * 3).unwrap();
        let bounds = Rect::new(0, 0, size, size);

        // Every third column is solid; the rest sit at `dim`, always below the threshold.
        let plane = |dim: u8| {
            let values: Vec<u8> = (0..size * size)
                .map(|i| if i % size % 3 == 0 { 255 } else { dim })
                .collect();
            CoverageMask::from_values(size, size, values).unwrap()
        };
        let binary_of = |dim: u8| {
            let bits: Vec<bool> = (0..size * size)
                .map(|i| (if i % size % 3 == 0 { 255 } else { dim }) >= 128u8)
                .collect();
            BinaryMask::from_bits(size, size, &bits).unwrap()
        };

        let (faint, near) = (20u8, 120u8);
        assert_eq!(
            vectorize(&binary_of(faint), bounds, AspectPolicy::Stretch).unwrap(),
            vectorize(&binary_of(near), bounds, AspectPolicy::Stretch).unwrap(),
            "both planes binarize identically, so the binary path cannot tell them apart"
        );
        assert_ne!(
            vectorize_coverage(&plane(faint), bounds, AspectPolicy::Stretch).unwrap(),
            vectorize_coverage(&plane(near), bounds, AspectPolicy::Stretch).unwrap(),
            "the coverage path must read the ink the threshold discarded"
        );
    }

    #[test]
    fn more_ink_never_sets_fewer_cells() {
        let size = u32::try_from(FEATURE_GRID * 3).unwrap();
        let bounds = Rect::new(0, 0, size, size);
        let counts: Vec<u32> = (0..=8u8)
            .map(|step| {
                let dim = step * 30;
                let values: Vec<u8> = (0..size * size)
                    .map(|i| if i % size % 3 == 0 { 255 } else { dim })
                    .collect();
                vectorize_coverage(
                    &CoverageMask::from_values(size, size, values).unwrap(),
                    bounds,
                    AspectPolicy::Stretch,
                )
                .unwrap()
                .popcount()
            })
            .collect();

        assert!(
            counts.windows(2).all(|w| w[0] <= w[1]),
            "coverage must be monotonic in ink: {counts:?}"
        );
        assert!(
            counts[0] < counts[8],
            "more ink must eventually set more cells: {counts:?}"
        );
    }

    #[test]
    fn a_zero_sized_box_is_rejected_on_the_coverage_path_too() {
        let coverage = grey(&["99", "99"]);
        assert!(
            vectorize_coverage(&coverage, Rect::new(0, 0, 0, 2), AspectPolicy::Stretch).is_err()
        );
    }

    #[test]
    fn vectoring_a_full_line_of_glyphs_is_fast_enough_to_be_irrelevant() {
        // Loose regression guard against an accidental quadratic in the coverage loop. A cue holds
        // tens of glyphs; this does 500 at 1080p glyph sizes.
        let m = render('O', 27, 40);
        let bounds = full_box(&m);

        let start = std::time::Instant::now();
        for _ in 0..500 {
            let _ = vectorize(&m, bounds, AspectPolicy::Letterbox).unwrap();
        }
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 500, "500 glyphs took {elapsed:?}");
    }
}
