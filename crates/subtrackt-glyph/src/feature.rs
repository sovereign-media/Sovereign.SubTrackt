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

use crate::binarize::BinaryMask;

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
    /// Stretch to fill, and carry the aspect ratio as a separate matched feature.
    StretchWithAspect,
}

/// Normalise the glyph at `bounds` in `mask` onto the fixed grid.
///
/// # Errors
/// Returns [`subtrackt_core::Error::Config`] if `bounds` has zero width or height, which means a
/// component filter let an empty box through.
pub fn vectorize(mask: &BinaryMask, bounds: Rect, policy: AspectPolicy) -> Result<FeatureVector> {
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
        AspectPolicy::Stretch | AspectPolicy::StretchWithAspect => (0.0, 0.0, grid, grid),
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
                mask,
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
fn cell_coverage(mask: &BinaryMask, bounds: Rect, u0: f32, u1: f32, v0: f32, v1: f32) -> f32 {
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
            if !mask.get(bounds.x + px, bounds.y + py) {
                continue;
            }
            covered += span_overlap(x0, x1, px) * weight_y;
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

/// The glyph's aspect ratio in hundredths, for [`AspectPolicy::StretchWithAspect`].
///
/// Zero-height boxes report `0` rather than dividing by zero; a component with no height should
/// have been filtered out before reaching here.
#[must_use]
pub fn aspect_ratio_centi(bounds: Rect) -> u32 {
    if bounds.height == 0 {
        return 0;
    }
    bounds.width * 100 / bounds.height
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a mask from rows of `#` and `.`.
    fn mask(rows: &[&str]) -> BinaryMask {
        let height = u32::try_from(rows.len()).unwrap();
        let width = u32::try_from(rows[0].len()).unwrap();
        let bits = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c == '#'))
            .collect();
        BinaryMask::from_bits(width, height, bits).unwrap()
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
        BinaryMask::from_bits(width, height, bits).unwrap()
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

    #[test]
    fn aspect_ratio_separates_a_narrow_glyph_from_a_wide_one() {
        let narrow = aspect_ratio_centi(Rect::new(0, 0, 3, 20));
        let wide = aspect_ratio_centi(Rect::new(0, 0, 18, 20));
        assert!(narrow < wide);
        assert_eq!(narrow, 15);
        assert_eq!(wide, 90);
    }

    #[test]
    fn a_degenerate_box_does_not_divide_by_zero() {
        assert_eq!(aspect_ratio_centi(Rect::new(0, 0, 5, 0)), 0);
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
