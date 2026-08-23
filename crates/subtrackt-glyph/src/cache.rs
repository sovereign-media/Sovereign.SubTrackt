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

/// What identifies a glyph for caching: its shape, where it sits in its line, and which way its
/// mark leans.
///
/// Shape alone is not enough and stopped being enough in #37. An `o` and an `O` can normalise to
/// the same vector — that is the whole reason line metrics exist — so keying on the vector would
/// hand the second one the first one's answer and make the new feature invisible. #48 adds a second
/// case of exactly that: an `à` and an `á` normalise to nearly the same vector *and* stand at the
/// same height, so the mark has to be in the key or the term that separates them never gets asked.
#[must_use]
pub fn cache_key(
    features: &FeatureVector,
    metrics: LineMetrics,
    mark: MarkSlope,
    aspect: InkAspect,
) -> u64 {
    let mut key = features.cache_key();
    // Mix each measured field in with the same FNV step the vector key uses. An unmeasured field
    // contributes nothing, so a glyph that has no mark keys the same way it did before there was
    // one to have.
    let mut mix = |bytes: [u8; 4]| {
        for byte in bytes {
            key ^= u64::from(byte);
            key = key.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    if metrics.known {
        mix(u32::to_le_bytes(metrics.height_percent));
        mix(i32::to_le_bytes(metrics.descent_percent));
    }
    if mark.known {
        mix(i32::to_le_bytes(mark.percent));
    }
    // #109 adds the third case, and it is the sharpest of them: an `l` and an `I` on one line share
    // a vector, a height and a descent, and carry no mark. Every field of the key before this one is
    // identical for the two, so without this the first of them scanned would answer for both and the
    // width term would never be reached.
    if aspect.known {
        mix(u32::to_le_bytes(aspect.permille));
    }
    key
}

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
        assert!((cache.hit_rate() - 2.0 / 3.0).abs() < 1e-6);
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

    #[test]
    fn clearing_resets_counters_as_well_as_entries() {
        let mut cache = SessionCache::new();
        cache.insert(
            &vector(1),
            LineMetrics::UNKNOWN,
            MarkSlope::NONE,
            InkAspect::UNKNOWN,
            matched('a'),
        );
        cache.get(&vector(1), LineMetrics::UNKNOWN, MarkSlope::NONE, InkAspect::UNKNOWN);
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.hits(), 0);
        assert!((cache.hit_rate() - 0.0).abs() < f32::EPSILON);
    }
}
