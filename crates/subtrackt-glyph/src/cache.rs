//! The per-stream session cache.
//!
//! Once a glyph variant has been identified in a stream, every later occurrence of the same
//! feature vector is an O(1) lookup instead of a scan over the reference set. Subtitle text repeats
//! heavily, so the hit rate should be high after the first few cues.
//!
//! Scope is per-stream, as the architecture document describes. Whether it should persist per file
//! or per library — and what invalidates it when the reference set changes — is #10.

use std::collections::HashMap;

use subtrackt_core::{FeatureVector, GlyphMatch, InkAspect, LineMetrics, MarkSlope};

/// What identifies a glyph for caching: its shape, where it sits in its line, which way its mark
/// leans, and how wide its ink stands.
///
/// Exactly the tuple [`crate::cluster::Shape`] is, and deliberately the same one: the clusterer and
/// the cache have to agree about when two glyphs are the same glyph, or one of them groups what the
/// other separates.
///
/// **Exact, not hashed, since #148.** This used to be an FNV hash of the same four fields, stored
/// as the map's key with nothing checked on the way out — so a collision returned a different
/// glyph's character. `FeatureVector::cache_key`'s "collisions only cost a re-match" was true where
/// it is written and was not true here. The probability was negligible: a track carries a few
/// thousand distinct shapes against 2^64, which `--report` prints and anyone can divide. The reason
/// to fix it is not the probability, it is that an exact key was already sitting one module away,
/// and a confident wrong answer is the failure mode this project exists to avoid rather than one to
/// price.
pub type Key = crate::cluster::Shape;

/// The key a glyph's measurements make.
#[must_use]
pub fn cache_key(
    features: &FeatureVector,
    metrics: LineMetrics,
    mark: MarkSlope,
    aspect: InkAspect,
) -> Key {
    (*features, metrics, mark, aspect)
}

/// Maps a feature vector to the match it produced.
#[derive(Debug, Clone, Default)]
pub struct SessionCache {
    entries: HashMap<Key, GlyphMatch>,
    hits: u64,
    misses: u64,
}

impl SessionCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a glyph, counting the hit or miss.
    pub fn get(
        &mut self,
        features: &FeatureVector,
        metrics: LineMetrics,
        mark: MarkSlope,
        aspect: InkAspect,
    ) -> Option<GlyphMatch> {
        if let Some(hit) = self
            .entries
            .get(&cache_key(features, metrics, mark, aspect))
        {
            self.hits += 1;
            Some(*hit)
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
    pub fn insert(
        &mut self,
        features: &FeatureVector,
        metrics: LineMetrics,
        mark: MarkSlope,
        aspect: InkAspect,
        result: GlyphMatch,
    ) {
        self.entries
            .insert(cache_key(features, metrics, mark, aspect), result);
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
        GlyphMatch {
            character: Some(c),
            distance: 2,
            runner_up_distance: 30,
            runner_up: None,
        }
    }

    #[test]
    fn a_repeated_glyph_is_answered_from_the_cache() {
        let mut cache = SessionCache::new();
        let v = vector(5);

        assert!(
            cache
                .get(&v, LineMetrics::UNKNOWN, MarkSlope::NONE, InkAspect::UNKNOWN)
                .is_none()
        );
        cache.insert(
            &v,
            LineMetrics::UNKNOWN,
            MarkSlope::NONE,
            InkAspect::UNKNOWN,
            matched('e'),
        );
        assert_eq!(
            cache
                .get(&v, LineMetrics::UNKNOWN, MarkSlope::NONE, InkAspect::UNKNOWN)
                .unwrap()
                .character,
            Some('e')
        );
        assert_eq!(
            cache
                .get(&v, LineMetrics::UNKNOWN, MarkSlope::NONE, InkAspect::UNKNOWN)
                .unwrap()
                .character,
            Some('e')
        );

        assert_eq!(cache.hits(), 2);
        assert_eq!(cache.misses(), 1);
    }

    #[test]
    fn one_shape_at_two_widths_is_two_keys_because_it_is_two_characters() {
        // #110. An `l` and an `I` on one line agree in vector, in height, in descent and in mark:
        // every field of this key but the last is identical for the two. Without the ratio the
        // first of them scanned would answer for both, and the term that separates them would never
        // be reached.
        let v = vector(5);
        assert_ne!(
            cache_key(&v, LineMetrics::new(100, 0), MarkSlope::NONE, InkAspect::new(119)),
            cache_key(&v, LineMetrics::new(100, 0), MarkSlope::NONE, InkAspect::new(143)),
        );
        // And a glyph with no ratio keys the same way it did before there was one to have.
        assert_eq!(
            cache_key(&v, LineMetrics::UNKNOWN, MarkSlope::NONE, InkAspect::UNKNOWN),
            cache_key(&v, LineMetrics::UNKNOWN, MarkSlope::NONE, InkAspect::UNKNOWN),
        );
    }

    #[test]
    fn distinct_vectors_do_not_collide() {
        let mut cache = SessionCache::new();
        cache.insert(
            &vector(1),
            LineMetrics::UNKNOWN,
            MarkSlope::NONE,
            InkAspect::UNKNOWN,
            matched('a'),
        );
        cache.insert(
            &vector(2),
            LineMetrics::UNKNOWN,
            MarkSlope::NONE,
            InkAspect::UNKNOWN,
            matched('b'),
        );
        assert_eq!(cache.len(), 2);
        assert_eq!(
            cache
                .get(&vector(2), LineMetrics::UNKNOWN, MarkSlope::NONE, InkAspect::UNKNOWN)
                .unwrap()
                .character,
            Some('b')
        );
    }

    #[test]
    fn unmatched_glyphs_are_cached_so_they_are_not_rescanned_every_occurrence() {
        let mut cache = SessionCache::new();
        let v = vector(9);
        cache.insert(
            &v,
            LineMetrics::UNKNOWN,
            MarkSlope::NONE,
            InkAspect::UNKNOWN,
            GlyphMatch::unmatched(140),
        );
        let hit = cache
            .get(&v, LineMetrics::UNKNOWN, MarkSlope::NONE, InkAspect::UNKNOWN)
            .unwrap();
        assert!(hit.character.is_none());
        assert_eq!(hit.distance, 140);
    }
}
