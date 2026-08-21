//! Connected component labelling.
//!
//! Not implemented — see #5.

use subtrackt_core::{Error, Rect, Result};

use crate::binarize::BinaryMask;

/// Constraints on what counts as a glyph-sized component.
///
/// Both bounds are needed. Without the lower one, compression noise and stray anti-aliasing pixels
/// become glyphs; without the upper one, a background box or a frame border becomes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentFilter {
    /// Components with fewer foreground pixels than this are discarded.
    pub min_area: u64,
    /// Components covering more than this fraction of the image, in percent, are discarded.
    pub max_coverage_percent: u32,
}

impl Default for ComponentFilter {
    fn default() -> Self {
        Self { min_area: 4, max_coverage_percent: 50 }
    }
}

impl ComponentFilter {
    /// Whether a component of the given bounds and pixel count survives the filter.
    #[must_use]
    pub fn accepts(self, bounds: Rect, pixels: u64, image_area: u64) -> bool {
        if pixels < self.min_area {
            return false;
        }
        if image_area == 0 {
            return true;
        }
        bounds.area() * 100 / image_area <= u64::from(self.max_coverage_percent)
    }
}

/// One labelled connected component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Component {
    /// Tight bounding box in mask coordinates.
    pub bounds: Rect,
    /// Foreground pixels in the component.
    pub pixels: u64,
}

/// Label 8-connected foreground components in a mask.
///
/// # Errors
/// Returns [`Error::Unsupported`] until #5 lands.
pub fn label(_mask: &BinaryMask, _filter: ComponentFilter) -> Result<Vec<Component>> {
    Err(Error::unsupported("connected component labelling", 5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labelling_reports_the_tracking_issue() {
        let err = label(&BinaryMask::blank(4, 4), ComponentFilter::default()).unwrap_err();
        assert!(matches!(err, Error::Unsupported { issue: 5, .. }), "got {err:?}");
    }

    #[test]
    fn the_filter_drops_specks_and_full_image_boxes() {
        let filter = ComponentFilter::default();
        let image_area = 1_000;
        assert!(
            !filter.accepts(Rect::new(0, 0, 1, 1), 1, image_area),
            "a speck is not a glyph"
        );
        assert!(
            filter.accepts(Rect::new(0, 0, 8, 12), 40, image_area),
            "a glyph-sized box is"
        );
        assert!(
            !filter.accepts(Rect::new(0, 0, 40, 25), 900, image_area),
            "a box covering the whole image is a background, not a glyph"
        );
    }
}
