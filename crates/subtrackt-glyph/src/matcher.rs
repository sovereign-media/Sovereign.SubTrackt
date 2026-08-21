//! Nearest-reference matching by Hamming distance.
//!
//! The scan is implemented; what is missing is reference data to scan (#9) and the SIMD work to
//! scan it faster (#10). With an empty reference set every glyph comes back unmatched, which is the
//! correct answer rather than a placeholder one — and it is what makes the accuracy gate meaningful
//! from the first commit.

use subtrackt_core::{Error, FEATURE_BITS, FeatureVector, Glyph, GlyphMatch, GlyphMatcher, Result};

use crate::cache::SessionCache;
use crate::reference::ReferenceSet;

/// Matching thresholds.
///
/// Both are expressed in percent of [`FEATURE_BITS`] rather than in raw cell counts, so changing
/// the grid size does not silently change how permissive the matcher is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchThresholds {
    /// A candidate further than this fraction of the vector away is not a match at all.
    pub max_distance_percent: u32,
    /// A winner beating the runner-up by less than this is reported as ambiguous, and is the only
    /// kind of glyph post-correction is allowed to touch.
    pub ambiguity_margin_percent: u32,
}

impl Default for MatchThresholds {
    fn default() -> Self {
        Self { max_distance_percent: 20, ambiguity_margin_percent: 3 }
    }
}

impl MatchThresholds {
    /// The distance ceiling in cells.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn max_distance(self) -> u32 {
        (FEATURE_BITS as u32) * self.max_distance_percent / 100
    }

    /// The ambiguity margin in cells.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn ambiguity_margin(self) -> u32 {
        (FEATURE_BITS as u32) * self.ambiguity_margin_percent / 100
    }
}

/// Matches glyphs against a [`ReferenceSet`], with a [`SessionCache`] in front.
pub struct HammingMatcher {
    references: ReferenceSet,
    thresholds: MatchThresholds,
    cache: SessionCache,
}

impl HammingMatcher {
    /// Build a matcher over a reference set.
    ///
    /// # Errors
    /// Returns [`Error::Config`] if the set was generated for a different grid size than this
    /// build uses — comparing across grid sizes yields distances that mean nothing, and failing
    /// here is far better than returning confident nonsense.
    pub fn new(references: ReferenceSet, thresholds: MatchThresholds) -> Result<Self> {
        if !references.matches_build_grid() {
            return Err(Error::Config(format!(
                "reference set '{}' was generated for a {}-cell grid, this build uses {FEATURE_BITS}",
                references.name(),
                references.grid() * references.grid()
            )));
        }
        Ok(Self { references, thresholds, cache: SessionCache::new() })
    }

    /// The reference set in use.
    #[must_use]
    pub const fn references(&self) -> &ReferenceSet {
        &self.references
    }

    /// The session cache, for reporting.
    #[must_use]
    pub const fn cache(&self) -> &SessionCache {
        &self.cache
    }

    /// Scan the reference set, ignoring the cache.
    #[must_use]
    pub fn scan(&self, features: &FeatureVector) -> GlyphMatch {
        let mut best: Option<(u32, char)> = None;
        let mut runner_up = u32::MAX;

        for entry in self.references.entries() {
            let distance = features.distance(&entry.features);
            match best {
                Some((best_distance, _)) if distance >= best_distance => {
                    runner_up = runner_up.min(distance);
                }
                Some((best_distance, _)) => {
                    runner_up = runner_up.min(best_distance);
                    best = Some((distance, entry.character));
                }
                None => best = Some((distance, entry.character)),
            }
        }

        match best {
            Some((distance, character)) if distance <= self.thresholds.max_distance() => {
                GlyphMatch {
                    character: Some(character),
                    distance,
                    runner_up_distance: runner_up,
                }
            }
            Some((distance, _)) => GlyphMatch::unmatched(distance),
            // An empty reference set matches nothing, at maximum distance.
            None => GlyphMatch::unmatched(u32::MAX),
        }
    }

    /// The ambiguity margin this matcher was configured with.
    #[must_use]
    pub const fn ambiguity_margin(&self) -> u32 {
        self.thresholds.ambiguity_margin()
    }
}

impl GlyphMatcher for HammingMatcher {
    fn match_glyph(&mut self, glyph: &Glyph) -> Result<GlyphMatch> {
        if let Some(hit) = self.cache.get(&glyph.features) {
            return Ok(hit);
        }
        let result = self.scan(&glyph.features);
        self.cache.insert(&glyph.features, result.clone());
        Ok(result)
    }

    fn cache_hits(&self) -> u64 {
        self.cache.hits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::{ReferenceEntry, Style};
    use subtrackt_core::Rect;

    fn vector(bits: &[usize]) -> FeatureVector {
        let mut v = FeatureVector::EMPTY;
        for bit in bits {
            v.set(*bit);
        }
        v
    }

    fn entry(character: char, bits: &[usize]) -> ReferenceEntry {
        ReferenceEntry { character, style: Style::Regular, features: vector(bits) }
    }

    fn glyph(bits: &[usize]) -> Glyph {
        Glyph { bounds: Rect::new(0, 0, 8, 12), line: 0, features: vector(bits) }
    }

    fn matcher(entries: Vec<ReferenceEntry>) -> HammingMatcher {
        HammingMatcher::new(ReferenceSet::new("test", entries), MatchThresholds::default()).unwrap()
    }

    #[test]
    fn an_exact_reference_wins_at_distance_zero() {
        // 'B' has to sit further from the query than the ambiguity margin (7 cells, being 3% of
        // the 256-cell vector) for the win to count as clear-cut. Twenty disjoint bits gives 23.
        let b: Vec<usize> = (10..30).collect();
        let mut m = matcher(vec![entry('A', &[1, 2, 3]), entry('B', &b)]);
        let result = m.match_glyph(&glyph(&[1, 2, 3])).unwrap();
        assert_eq!(result.character, Some('A'));
        assert_eq!(result.distance, 0);
        assert_eq!(
            result.runner_up_distance, 23,
            "3 query bits + 20 reference bits, disjoint"
        );
        assert!(result.is_unambiguous(m.ambiguity_margin()));
    }

    #[test]
    fn a_near_miss_still_matches_within_the_threshold() {
        let mut m = matcher(vec![entry('A', &[1, 2, 3, 4])]);
        let result = m.match_glyph(&glyph(&[1, 2, 3])).unwrap();
        assert_eq!(result.character, Some('A'));
        assert_eq!(result.distance, 1);
    }

    #[test]
    fn a_glyph_beyond_the_threshold_is_reported_unmatched_not_forced_to_the_nearest() {
        let far: Vec<usize> = (0..80).collect();
        let mut m = matcher(vec![entry('A', &[1])]);
        let result = m.match_glyph(&glyph(&far)).unwrap();
        assert!(
            result.character.is_none(),
            "must not force a match onto the nearest reference"
        );
        assert!(result.distance > MatchThresholds::default().max_distance());
    }

    #[test]
    fn two_close_references_are_reported_as_ambiguous() {
        let mut m = matcher(vec![entry('0', &[1, 2, 3, 4]), entry('O', &[1, 2, 3, 5])]);
        let result = m.match_glyph(&glyph(&[1, 2, 3, 4])).unwrap();
        assert_eq!(result.character, Some('0'));
        assert!(
            !result.is_unambiguous(m.ambiguity_margin()),
            "a one-cell gap is not a clear win"
        );
    }

    #[test]
    fn an_empty_reference_set_matches_nothing_rather_than_guessing() {
        let mut m = matcher(vec![]);
        assert!(m.match_glyph(&glyph(&[1, 2])).unwrap().character.is_none());
    }

    #[test]
    fn repeated_glyphs_come_from_the_cache() {
        let mut m = matcher(vec![entry('A', &[1, 2, 3])]);
        m.match_glyph(&glyph(&[1, 2, 3])).unwrap();
        m.match_glyph(&glyph(&[1, 2, 3])).unwrap();
        assert_eq!(m.cache_hits(), 1);
        assert_eq!(m.cache().len(), 1);
    }

    #[test]
    fn thresholds_scale_with_the_grid_rather_than_being_raw_cell_counts() {
        let t = MatchThresholds::default();
        assert_eq!(t.max_distance(), u32::try_from(FEATURE_BITS).unwrap() * 20 / 100);
        assert!(t.ambiguity_margin() < t.max_distance());
    }
}
