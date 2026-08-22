//! Identifying glyphs: cluster the stream's own shapes, then match the clusters.
//!
//! The reference scan itself is a linear pass by Hamming distance and always was. What changed in
//! #10 is *what gets scanned*. Matching each glyph against the reference set independently cannot
//! work — #14 measured two renderings of one character a median 46 cells apart against 31 for two
//! different characters — so [`prepare`](GlyphMatcher::prepare) groups the stream's shapes first
//! and scans one consensus vector per group. See [`crate::cluster`] for why that is sound.
//!
//! Every glyph in a cluster then gets that cluster's answer, which means the per-glyph distance
//! reported is the *centroid's* distance to its reference rather than the individual glyph's. That
//! is deliberate: the identification was a cluster-level decision and the number should say so.
//!
//! With an empty reference set every cluster comes back unmatched, which is the correct answer
//! rather than a placeholder one — and it is what makes the accuracy gate meaningful.

use subtrackt_core::{
    Error, FEATURE_BITS, FeatureVector, Glyph, GlyphMatch, GlyphMatcher, LineMetrics, Result,
};

use crate::cache::SessionCache;
use crate::cluster::{ClusterRules, Shapes, cluster};
use crate::reference::{ReferenceEntry, ReferenceSet};

/// Matching thresholds.
///
/// Every one of these is a fraction of [`FEATURE_BITS`] rather than a raw cell count, so changing
/// the grid size does not silently change how permissive the matcher is.
///
/// The exchange rate below was the exception until #45, and what the exception cost is why the rule
/// is written down. Holding a cell count, the gap it prices between an `o` and an `O` was worth 5.5%
/// of a 256-bit vector and 1.4% of a 1024-bit one, so doubling the grid quietly stopped weighting
/// the feature #37 added to separate exactly that pair — measured at up to 12.8 points of CER, with
/// nothing erroring and no counter moving. See `docs/glyph-stability.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchThresholds {
    /// A candidate further than this fraction of the vector away is not a match at all.
    pub max_distance_percent: u32,
    /// A winner beating the runner-up by less than this is reported as ambiguous, and is the only
    /// kind of glyph post-correction is allowed to touch.
    pub ambiguity_margin_percent: u32,
    /// What a full cap-height difference in line metrics is worth, in tenths of a percent of
    /// [`FEATURE_BITS`].
    ///
    /// The shape vector cannot separate `o` from `O` — see [`LineMetrics`] — so a second term
    /// carries the difference in how tall each stands in its line. This is the exchange rate
    /// between the two, and it is the one number #37 has to choose by measurement: too low and the
    /// term does nothing, too high and it overrules shape, so a badly-segmented glyph of the right
    /// height beats a well-segmented one of the wrong height.
    ///
    /// Zero disables the term entirely, which is what a version 1 reference set effectively gets
    /// anyway since its entries carry no metrics.
    ///
    /// Priced per *full* cap height — 100 percentage points — because that is what
    /// [`LineMetrics::difference`] counts in, and in tenths of a percent because the measured value
    /// is 19.6% of the vector and 20% is a different number. In the units #37 reported, the shipped
    /// setting is 50 hundredths of a cell per point: an `o` against an `O` is 28 points, so it
    /// costs 14 cells at 16x16 and the same 5.5% of the vector at any other grid size.
    pub metric_weight_permille: u32,
}

impl Default for MatchThresholds {
    fn default() -> Self {
        Self {
            max_distance_percent: 20,
            ambiguity_margin_percent: 3,
            metric_weight_permille: 196,
        }
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

    /// What a full cap-height metric difference — 100 percentage points — costs, in cells.
    ///
    /// 50 cells on a 256-bit vector, which is the value #37 measured, and the same fraction of any
    /// other vector.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn metric_weight(self) -> u32 {
        Self::metric_weight_at(self.metric_weight_permille, FEATURE_BITS as u32)
    }

    /// The same conversion against an arbitrary vector length.
    ///
    /// Split out so a test can check the scale-free property across grid sizes. [`FEATURE_BITS`] is
    /// a compile-time constant, so a promise about what happens when it changes is otherwise
    /// unenforceable from inside one build — which is exactly how #45 survived unnoticed.
    const fn metric_weight_at(permille: u32, bits: u32) -> u32 {
        bits * permille / 1000
    }

    /// Distance between a glyph and a reference: shape, plus the line-metric term.
    ///
    /// The metric term is `points × weight ÷ 100`: the weight is what a whole cap height costs and
    /// the difference is in percentage points of one.
    ///
    /// When either side has no metrics the term is omitted rather than defaulted. A glyph on a line
    /// too short to locate a baseline is compared on shape alone, which is worse than the full
    /// comparison and much better than being scored against a fabricated height.
    #[must_use]
    pub fn distance(
        self,
        shape: &FeatureVector,
        metrics: LineMetrics,
        entry: &ReferenceEntry,
    ) -> u32 {
        let base = shape.distance(&entry.features);
        metrics
            .difference(entry.metrics)
            .map_or(base, |points| base + points * self.metric_weight() / 100)
    }
}

/// Matches glyphs against a [`ReferenceSet`] by clustering them first.
pub struct HammingMatcher {
    references: ReferenceSet,
    thresholds: MatchThresholds,
    rules: ClusterRules,
    /// Holds the answer for every shape [`prepare`](GlyphMatcher::prepare) saw, so matching a glyph
    /// is a lookup. Without a preparation pass it fills lazily and the matcher degrades to the
    /// per-glyph scan it used to be.
    cache: SessionCache,
    distinct_shapes: u64,
    clusters: u64,
    scans: u64,
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
        Ok(Self {
            references,
            thresholds,
            rules: ClusterRules::default(),
            cache: SessionCache::new(),
            distinct_shapes: 0,
            clusters: 0,
            scans: 0,
        })
    }

    /// Use different clustering rules.
    #[must_use]
    pub const fn with_cluster_rules(mut self, rules: ClusterRules) -> Self {
        self.rules = rules;
        self
    }

    /// Distinct shapes seen by the preparation pass.
    #[must_use]
    pub const fn distinct_shapes(&self) -> u64 {
        self.distinct_shapes
    }

    /// Clusters those shapes formed.
    #[must_use]
    pub const fn clusters(&self) -> u64 {
        self.clusters
    }

    /// How many times the reference set was scanned.
    ///
    /// This is the figure worth watching: one scan per *cluster* rather than one per glyph is the
    /// whole efficiency argument for the redesign, and a number close to the glyph count means
    /// clustering silently stopped working.
    #[must_use]
    pub const fn reference_scans(&self) -> u64 {
        self.scans
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
        self.scan_with(features, LineMetrics::UNKNOWN)
    }

    /// Scan the reference set for a glyph whose position in its line is known.
    #[must_use]
    pub fn scan_with(&self, features: &FeatureVector, metrics: LineMetrics) -> GlyphMatch {
        let mut best: Option<(u32, char)> = None;
        let mut runner_up = u32::MAX;

        for entry in self.references.entries() {
            let distance = self.thresholds.distance(features, metrics, entry);
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
    fn prepare(&mut self, glyphs: &[Glyph]) -> Result<()> {
        let mut shapes = Shapes::new();
        for glyph in glyphs {
            shapes.add(&glyph.features, glyph.metrics);
        }
        self.distinct_shapes = shapes.distinct().try_into().unwrap_or(u64::MAX);

        let clusters = cluster(&shapes, self.rules);
        self.clusters = clusters.len().try_into().unwrap_or(u64::MAX);

        // Scan every centroid before touching the cache, because scanning borrows the reference set
        // and filling the cache borrows the matcher.
        let answers: Vec<GlyphMatch> = clusters
            .iter()
            .map(|c| self.scan_with(&c.centroid, c.centroid_metrics))
            .collect();
        self.scans += answers.len().try_into().unwrap_or(u64::MAX);

        for (group, answer) in clusters.iter().zip(answers) {
            for ((features, metrics), _) in &group.members {
                self.cache.insert(features, *metrics, answer.clone());
            }
        }
        Ok(())
    }

    fn match_glyph(&mut self, glyph: &Glyph) -> Result<GlyphMatch> {
        if let Some(hit) = self.cache.get(&glyph.features, glyph.metrics) {
            return Ok(hit);
        }
        // Only reachable without a preparation pass, or for a shape it did not see. Answering
        // from a bare scan is worse than answering from a cluster, but it is still an answer, and
        // silently returning "unmatched" for a glyph the matcher simply had not been shown would
        // be a much harder failure to notice.
        let result = self.scan_with(&glyph.features, glyph.metrics);
        self.scans += 1;
        self.cache
            .insert(&glyph.features, glyph.metrics, result.clone());
        Ok(result)
    }

    fn cache_hits(&self) -> u64 {
        self.cache.hits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::ClusterRules;
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
        ReferenceEntry {
            character,
            style: Style::Regular,
            features: vector(bits),
            metrics: LineMetrics::UNKNOWN,
        }
    }

    fn glyph(bits: &[usize]) -> Glyph {
        Glyph {
            bounds: Rect::new(0, 0, 8, 12),
            line: 0,
            features: vector(bits),
            metrics: LineMetrics::UNKNOWN,
        }
    }

    fn matcher(entries: Vec<ReferenceEntry>) -> HammingMatcher {
        HammingMatcher::new(ReferenceSet::new("test", entries), MatchThresholds::default()).unwrap()
    }

    #[test]
    fn an_exact_reference_wins_at_distance_zero() {
        // 'B' has to sit further from the query than the ambiguity margin for the win to count as
        // clear-cut. The margin is a percentage of FEATURE_BITS, so the gap is sized off it rather
        // than written as a cell count that only holds at one grid size.
        let spare = MatchThresholds::default().ambiguity_margin() + 10;
        let b: Vec<usize> = (10..10 + usize::try_from(spare).unwrap()).collect();
        let mut m = matcher(vec![entry('A', &[1, 2, 3]), entry('B', &b)]);
        let result = m.match_glyph(&glyph(&[1, 2, 3])).unwrap();
        assert_eq!(result.character, Some('A'));
        assert_eq!(result.distance, 0);
        assert_eq!(
            result.runner_up_distance,
            3 + spare,
            "3 query bits + the reference's bits, disjoint"
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
        // Sized off the ceiling, which is a percentage of FEATURE_BITS.
        let far: Vec<usize> =
            (0..usize::try_from(MatchThresholds::default().max_distance()).unwrap() + 10).collect();
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
    fn preparing_scans_once_per_cluster_rather_than_once_per_glyph() {
        // The efficiency claim behind the redesign, stated as a test. Ten glyphs of two shapes must
        // cost two scans, not ten.
        let mut m = matcher(vec![
            entry('A', &[1, 2, 3]),
            entry('B', &(10..30).collect::<Vec<_>>()),
        ]);
        let glyphs: Vec<Glyph> = (0..10)
            .map(|i| {
                if i % 2 == 0 {
                    glyph(&[1, 2, 3])
                } else {
                    glyph(&[1, 2, 4])
                }
            })
            .collect();

        m.prepare(&glyphs).unwrap();
        assert_eq!(m.distinct_shapes(), 2, "ten glyphs, two shapes");
        assert_eq!(m.reference_scans(), 2, "one scan per cluster");

        for g in &glyphs {
            m.match_glyph(g).unwrap();
        }
        assert_eq!(m.reference_scans(), 2, "matching after preparing scans nothing further");
        assert_eq!(m.cache_hits(), 10, "every glyph came from the prepared labels");
    }

    #[test]
    fn a_prepared_glyph_gets_its_clusters_answer_not_its_own() {
        // The substantive difference between this and per-glyph matching: a shape that would match
        // nothing on its own takes the label its cluster agreed on.
        let base: Vec<usize> = (0..40).collect();
        let mut odd = base.clone();
        odd.extend([100, 101, 102, 103, 104]);

        let mut m = HammingMatcher::new(
            ReferenceSet::new("test", vec![entry('A', &base)]),
            MatchThresholds::default(),
        )
        .unwrap()
        .with_cluster_rules(ClusterRules { radius_percent: 8, ..ClusterRules::default() });

        // Nine clean renderings outvote one distorted member, so the centroid is the clean shape.
        let mut glyphs: Vec<Glyph> = (0..9).map(|_| glyph(&base)).collect();
        glyphs.push(glyph(&odd));

        m.prepare(&glyphs).unwrap();
        assert_eq!(m.clusters(), 1);
        let result = m.match_glyph(&glyph(&odd)).unwrap();
        assert_eq!(result.character, Some('A'));
        assert_eq!(
            result.distance, 0,
            "the distance reported is the centroid's, not the glyph's"
        );
    }

    #[test]
    fn the_shipped_default_leaves_every_distinct_shape_on_its_own() {
        // Clustering ships off, so preparing must reproduce exactly what per-glyph matching did.
        // See ClusterRules::default for the measurement behind that.
        let mut m = matcher(vec![
            entry('A', &[1, 2, 3]),
            entry('B', &(10..30).collect::<Vec<_>>()),
        ]);
        let glyphs = vec![glyph(&[1, 2, 3]), glyph(&[1, 2, 4]), glyph(&[1, 2, 3])];

        m.prepare(&glyphs).unwrap();
        assert_eq!(m.distinct_shapes(), 2);
        assert_eq!(m.clusters(), 2, "no grouping at the default radius");
        assert_eq!(m.match_glyph(&glyphs[0]).unwrap().distance, 0);
    }

    #[test]
    fn a_glyph_the_matcher_was_never_shown_is_still_answered() {
        // Returning "unmatched" for a glyph that simply was not in the preparation pass would be a
        // silent failure, and a far harder one to notice than a slow answer.
        let mut m = matcher(vec![entry('A', &[1, 2, 3])]);
        m.prepare(&[]).unwrap();
        let result = m.match_glyph(&glyph(&[1, 2, 3])).unwrap();
        assert_eq!(result.character, Some('A'));
        assert_eq!(m.reference_scans(), 1);
    }

    #[test]
    fn thresholds_scale_with_the_grid_rather_than_being_raw_cell_counts() {
        let t = MatchThresholds::default();
        assert_eq!(t.max_distance(), u32::try_from(FEATURE_BITS).unwrap() * 20 / 100);
        assert!(t.ambiguity_margin() < t.max_distance());
    }

    #[test]
    fn a_cap_height_costs_the_same_fraction_of_the_vector_at_any_grid_size() {
        // The defect #45 fixed. The exchange rate was a cell count, so at 32x32 it was worth a
        // quarter of what it was worth at 16x16 and the term separating `o` from `O` all but
        // vanished — silently, since nothing errors and no counter moves. FEATURE_BITS is fixed at
        // compile time, so the property is checked against the grid sizes the project might build
        // with rather than only the one it is built with.
        let permille = MatchThresholds::default().metric_weight_permille;
        for grid in [16_u32, 32, 64] {
            let bits = grid * grid;
            let weight = MatchThresholds::metric_weight_at(permille, bits);
            assert!(
                (weight * 1000).abs_diff(bits * permille) <= 1000,
                "at {grid}x{grid} a cap height costs {weight} of {bits} cells, which is not \
                 {permille} per mille to within the one cell that rounding allows"
            );
        }
    }

    #[test]
    fn the_shipped_weight_re_expresses_what_was_measured_rather_than_retuning_it() {
        // #37 measured the rate at 50 hundredths of a cell per percentage point on a 256-bit
        // vector; #45 changed only how that is written down. Pinned against 256 explicitly rather
        // than against FEATURE_BITS, so it stays a statement about the measured number and does
        // not fail the day the grid moves.
        let permille = MatchThresholds::default().metric_weight_permille;
        assert_eq!(MatchThresholds::metric_weight_at(permille, 256), 50);
        // Which is to say an `o` against an `O` — 28 points of cap height — still costs 14 cells.
        assert_eq!(28 * MatchThresholds::metric_weight_at(permille, 256) / 100, 14);
    }

    #[test]
    fn the_metric_term_charges_in_proportion_to_the_gap_and_nothing_for_an_unknown_one() {
        // Same shape, different heights: the case #37 exists for. The `o`/`O` gap has to cost 28%
        // of what a whole cap height costs, and an unmeasurable line has to cost nothing at all
        // rather than be scored against a fabricated height.
        let t = MatchThresholds::default();
        let shape = vector(&[1, 2, 3]);
        let reference = ReferenceEntry {
            character: 'O',
            style: Style::Regular,
            features: shape,
            metrics: LineMetrics::new(104, 0),
        };

        assert_eq!(t.distance(&shape, LineMetrics::new(104, 0), &reference), 0);
        assert_eq!(
            t.distance(&shape, LineMetrics::new(76, 0), &reference),
            28 * t.metric_weight() / 100
        );
        assert_eq!(t.distance(&shape, LineMetrics::UNKNOWN, &reference), 0);
    }
}
