//! Normalising a glyph onto the fixed grid.
//!
//! Not implemented — see #7.

use subtrackt_core::{Error, FeatureVector, Rect, Result};

use crate::binarize::BinaryMask;

/// How a glyph's aspect ratio is handled when it is squeezed onto a square grid.
///
/// This is the substantive decision in #7. Stretching both an `l` and an `M` to fill the grid
/// discards the width difference that tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AspectPolicy {
    /// Stretch to fill the grid on both axes. Maximum detail, no width information.
    Stretch,
    /// Preserve aspect ratio and centre within the grid, leaving empty cells at the sides.
    #[default]
    Letterbox,
    /// Stretch to fill, and carry the aspect ratio as a separate matched feature.
    StretchWithAspect,
}

/// Normalise the glyph at `bounds` in `mask` onto the fixed grid.
///
/// # Errors
/// Returns [`Error::Unsupported`] until #7 lands.
pub fn vectorize(
    _mask: &BinaryMask,
    _bounds: Rect,
    _policy: AspectPolicy,
) -> Result<FeatureVector> {
    Err(Error::unsupported("glyph feature vectoring", 7))
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

    #[test]
    fn vectoring_reports_the_tracking_issue() {
        let err =
            vectorize(&BinaryMask::blank(8, 8), Rect::new(0, 0, 8, 8), AspectPolicy::default())
                .unwrap_err();
        assert!(matches!(err, Error::Unsupported { issue: 7, .. }), "got {err:?}");
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
}
