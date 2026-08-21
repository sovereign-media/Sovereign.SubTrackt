//! The per-stream session cache.
//!
//! Once a glyph variant has been identified in a stream, every later occurrence of the same
//! feature vector is an O(1) lookup instead of a scan over the reference set. Subtitle text repeats
//! heavily, so the hit rate should be high after the first few cues.
//!
//! Scope is per-stream, as the architecture document describes. Whether it should persist per file
//! or per library — and what invalidates it when the reference set changes — is #10.

use std::collections::HashMap;

use subtrackt_core::{FeatureVector, GlyphMatch};

/// Maps a feature vector to the match it produced.
#[derive(Debug, Clone, Default)]
pub struct SessionCache {
    entries: HashMap<u64, GlyphMatch>,
    hits: u64,
    misses: u64,
}

impl SessionCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a vector, counting the hit or miss.
    pub fn get(&mut self, features: &FeatureVector) -> Option<GlyphMatch> {
        if let Some(hit) = self.entries.get(&features.cache_key()) {
            self.hits += 1;
            Some(hit.clone())
        } else {
            self.misses += 1;
            None
        }
    }

    /// Record a match.
    ///
    /// Unmatched glyphs are cached too. That is deliberate: a glyph the reference set cannot
    /// identify will recur throughout the stream, and rescanning the whole reference set for each
    /// occurrence is the worst case this cache exists to avoid.
    pub fn insert(&mut self, features: &FeatureVector, result: GlyphMatch) {
        self.entries.insert(features.cache_key(), result);
    }

    /// Cache hits so far.
    #[must_use]
    pub const fn hits(&self) -> u64 {
        self.hits
    }

    /// Cache misses so far.
    #[must_use]
    pub const fn misses(&self) -> u64 {
        self.misses
    }

    /// Distinct glyph variants seen.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing has been cached.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Hit rate in `0.0..=1.0`, or `0.0` before any lookup.
    #[must_use]
    pub fn hit_rate(&self) -> f32 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.hits as f32 / total as f32
        }
    }

    /// Drop everything, for reuse across streams.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.hits = 0;
        self.misses = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(bit: usize) -> FeatureVector {
        let mut v = FeatureVector::EMPTY;
        v.set(bit);
        v
    }

    fn matched(c: char) -> GlyphMatch {
        GlyphMatch { character: Some(c), distance: 2, runner_up_distance: 30 }
    }

    #[test]
    fn a_repeated_glyph_is_answered_from_the_cache() {
        let mut cache = SessionCache::new();
        let v = vector(5);

        assert!(cache.get(&v).is_none());
        cache.insert(&v, matched('e'));
        assert_eq!(cache.get(&v).unwrap().character, Some('e'));
        assert_eq!(cache.get(&v).unwrap().character, Some('e'));

        assert_eq!(cache.hits(), 2);
        assert_eq!(cache.misses(), 1);
        assert!((cache.hit_rate() - 2.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn distinct_vectors_do_not_collide() {
        let mut cache = SessionCache::new();
        cache.insert(&vector(1), matched('a'));
        cache.insert(&vector(2), matched('b'));
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(&vector(2)).unwrap().character, Some('b'));
    }

    #[test]
    fn unmatched_glyphs_are_cached_so_they_are_not_rescanned_every_occurrence() {
        let mut cache = SessionCache::new();
        let v = vector(9);
        cache.insert(&v, GlyphMatch::unmatched(140));
        let hit = cache.get(&v).unwrap();
        assert!(hit.character.is_none());
        assert_eq!(hit.distance, 140);
    }

    #[test]
    fn clearing_resets_counters_as_well_as_entries() {
        let mut cache = SessionCache::new();
        cache.insert(&vector(1), matched('a'));
        cache.get(&vector(1));
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.hits(), 0);
        assert!((cache.hit_rate() - 0.0).abs() < f32::EPSILON);
    }
}
