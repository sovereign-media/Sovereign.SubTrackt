//! The reference glyph set the matcher compares against.
//!
//! The in-memory shape is implemented; generating, compressing and embedding real reference data
//! is #9, and *what should be in it* is #8 — the typeface survey that the architecture document
//! calls the whole risk. Nothing is embedded here on purpose: shipping a guessed reference set is
//! worse than shipping none, because a title in an unlisted typeface would degrade to confident
//! garbage rather than to a clean failure.

use subtrackt_core::{FEATURE_GRID, FeatureVector};

/// Typographic variant of a reference glyph.
///
/// Whether variants need separate reference vectors at all is #14. If one vector per character
/// survives bold, italic and outline variation, this collapses to a single entry per character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Style {
    /// Upright, normal weight.
    #[default]
    Regular,
    /// Bold.
    Bold,
    /// Italic or oblique.
    Italic,
    /// Bold italic.
    BoldItalic,
}

/// One reference glyph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceEntry {
    /// The character this vector stands for.
    pub character: char,
    /// Which typographic variant it was rendered as.
    pub style: Style,
    /// The normalised vector.
    pub features: FeatureVector,
}

/// A named collection of reference glyphs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceSet {
    name: String,
    grid: usize,
    entries: Vec<ReferenceEntry>,
}

impl ReferenceSet {
    /// Build a set, recording the grid size its vectors were generated at.
    #[must_use]
    pub fn new(name: impl Into<String>, entries: Vec<ReferenceEntry>) -> Self {
        Self { name: name.into(), grid: FEATURE_GRID, entries }
    }

    /// An empty set, which is what the binary ships with today.
    #[must_use]
    pub fn empty() -> Self {
        Self::new("empty", Vec::new())
    }

    /// The set's name, surfaced in `--version` so a bad extraction can be traced to its data.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The grid size the vectors were generated at.
    ///
    /// A set generated at a different grid size than the running binary cannot be compared
    /// against, and the matcher must refuse it rather than produce meaningless distances.
    #[must_use]
    pub const fn grid(&self) -> usize {
        self.grid
    }

    /// Whether this set was generated for the grid size this build uses.
    #[must_use]
    pub const fn matches_build_grid(&self) -> bool {
        self.grid == FEATURE_GRID
    }

    /// The entries.
    #[must_use]
    pub fn entries(&self) -> &[ReferenceEntry] {
        &self.entries
    }

    /// Number of reference glyphs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the set holds no glyphs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The reference set compiled into this binary.
///
/// Empty until #8 says what belongs in it and #9 builds the generator.
#[must_use]
pub fn embedded() -> ReferenceSet {
    ReferenceSet::empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_set_is_empty_until_the_survey_says_what_goes_in_it() {
        let set = embedded();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn a_set_records_the_grid_it_was_generated_for() {
        let set = ReferenceSet::new("test", vec![]);
        assert_eq!(set.grid(), FEATURE_GRID);
        assert!(set.matches_build_grid());
        assert_eq!(set.name(), "test");
    }

    #[test]
    fn a_set_generated_for_another_grid_is_detectable() {
        let mut set = ReferenceSet::new("stale", vec![]);
        set.grid = FEATURE_GRID * 2;
        assert!(!set.matches_build_grid());
    }
}
