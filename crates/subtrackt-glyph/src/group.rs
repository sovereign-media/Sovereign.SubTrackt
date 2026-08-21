//! Line assignment and diacritic grouping.
//!
//! Not implemented — see #6.
//!
//! Connected components are not characters. The dot of an `i`, the accent of an `é` and the two
//! dots of a diaeresis each arrive as their own component and belong to the glyph they sit above;
//! a cedilla belongs to the one above it. The catch is that a colon has exactly the geometry of a
//! diacritic pair and must not be merged.

use subtrackt_core::{Error, Result};

use crate::binarize::BinaryMask;
use crate::ccl::Component;

/// Thresholds for merging a component into its neighbour.
///
/// Everything here is a fraction of the measured line height rather than a pixel count: the same
/// title ships at several resolutions, and an absolute threshold that works at 1080p will merge
/// half a line at 480p.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupingRules {
    /// A component shorter than this fraction of line height, in percent, may be a diacritic.
    pub diacritic_max_height_percent: u32,
    /// Maximum vertical gap between a diacritic and its base, in percent of line height.
    pub max_gap_percent: u32,
    /// Minimum horizontal overlap with the base, in percent of the diacritic's width.
    pub min_overlap_percent: u32,
}

impl Default for GroupingRules {
    fn default() -> Self {
        Self {
            diacritic_max_height_percent: 40,
            max_gap_percent: 25,
            min_overlap_percent: 50,
        }
    }
}

/// Components merged into one character, in reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupedGlyph {
    /// The components that make up this character.
    pub parts: Vec<Component>,
    /// Which text line it belongs to, counting from the top.
    pub line: usize,
}

/// Split components into text lines using the mask's row projection.
///
/// # Errors
/// Returns [`Error::Unsupported`] until #6 lands.
pub fn assign_lines(_mask: &BinaryMask, _components: &[Component]) -> Result<Vec<usize>> {
    Err(Error::unsupported("line assignment", 6))
}

/// Merge diacritics onto their base glyphs.
///
/// # Errors
/// Returns [`Error::Unsupported`] until #6 lands.
pub fn group(
    _components: &[Component],
    _lines: &[usize],
    _rules: GroupingRules,
) -> Result<Vec<GroupedGlyph>> {
    Err(Error::unsupported("diacritic grouping", 6))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping_reports_the_tracking_issue() {
        assert!(matches!(
            assign_lines(&BinaryMask::blank(4, 4), &[]),
            Err(Error::Unsupported { issue: 6, .. })
        ));
        assert!(matches!(
            group(&[], &[], GroupingRules::default()),
            Err(Error::Unsupported { issue: 6, .. })
        ));
    }

    #[test]
    fn the_default_rules_are_relative_not_absolute() {
        // Guards the property that matters: every threshold is a percentage of line height, so
        // the same rules hold at 480p and 1080p.
        let rules = GroupingRules::default();
        assert!(rules.diacritic_max_height_percent <= 100);
        assert!(rules.max_gap_percent <= 100);
        assert!(rules.min_overlap_percent <= 100);
    }
}
