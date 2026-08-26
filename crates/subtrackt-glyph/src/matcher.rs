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

use subtrackt_core::orthography;
use subtrackt_core::{
    Error, FEATURE_BITS, FeatureVector, Glyph, GlyphMatch, GlyphMatcher, InkAspect, LineMetrics,
    MarkSlope, Result,
};

use crate::cache::SessionCache;
use crate::cluster::{ClusterRules, Shapes, cluster};
use crate::reference::{ReferenceEntry, ReferenceSet};
use crate::weights::Weights;

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
    /// What a full 100 points of difference in mark direction is worth, in tenths of a percent of
    /// [`FEATURE_BITS`].
    ///
    /// The same unit as `metric_weight_permille` and for the same reasons — a fraction of the
    /// vector rather than a cell count, and priced per 100 points because that is what
    /// [`MarkSlope::difference`] counts in.
    ///
    /// **Zero, which is to say the term is off**, and it is off because `xtask mark-sweep`
    /// measured it across eight conditions and it bought nothing. An acute against a grave measures
    /// about 131 points apart, so any setting above about 20 pushes the wrong-leaning candidate
    /// clear of the ambiguity margin — and the ambiguity tally does fall, by 12 to 20%. CER does
    /// not move at any setting on any condition, and above 78 it gets worse, up to 2.1 points,
    /// because the term starts rejecting correct characters outright.
    ///
    /// The census that explains it is in `docs/glyph-stability.md`. The accents the pipeline gets
    /// wrong are wrong in the *base letter*, not the direction: `á` read as `é`, `è` as `ò`. Both
    /// members of those pairs carry the same mark, so this term cannot separate them by
    /// construction — and the pairs it can separate were never being read wrongly.
    ///
    /// Kept rather than removed for the same reason `ClusterRules::radius_percent` is: the
    /// machinery is the instrument that measured the question, and turning it on is one number
    /// once there is evidence for it. #43 changed the conditions substantially — a reference set
    /// fitted to the title removes the base-letter confusions that dominated when this was measured
    /// — so the sweep is worth re-running against a fitted set.
    pub mark_weight_permille: u32,
    /// What a **full cap height** of difference in ink width is worth, in tenths of a percent of
    /// [`FEATURE_BITS`].
    ///
    /// The third term, and #109's. The shape vector cannot separate `l` from `I` — #10 measured
    /// them at distance zero — and #37's line metrics cannot either, because both stand from the
    /// baseline to the cap line. What does separate them is that `I` is 7% wider, which is a fact
    /// about the typeface that survives to the disc: every upright `l` on a real Blu-ray is five
    /// pixels wide and every `I` is six.
    ///
    /// Priced per full cap height, matching the other two, and applied to a difference counted in
    /// **tenths of a percent** of one. The finer unit is not a detail: the gap between an `l` and an
    /// `I` is eight tenths of a percent of cap height, so in whole percent it would be a difference
    /// of one point, and one point at the metric weight rounds to zero cells.
    ///
    /// The setting is what `xtask width-sweep` measured, running the whole pipeline at each
    /// candidate against **three** discs and scoring each result — 180 to 200 is the window all
    /// three agree on, and 190 is its middle. Below it the term cannot move a cell; above it, it
    /// starts overruling shape for characters a disc draws a point or two from what the reference
    /// carries. `docs/error-census.md` has the sweep.
    pub width_weight_permille: u32,
}

impl Default for MatchThresholds {
    fn default() -> Self {
        Self {
            max_distance_percent: 20,
            ambiguity_margin_percent: 3,
            metric_weight_permille: 196,
            mark_weight_permille: 0,
            width_weight_permille: 190,
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

    /// The exchange rates this matcher prices its three measured terms at.
    ///
    /// The same type [`ClusterRules::weights`](crate::cluster::ClusterRules::weights) returns, and
    /// `clustering_prices_every_term_exactly_as_the_matcher_does` is what holds them together.
    #[must_use]
    pub const fn weights(self) -> Weights {
        Weights {
            metric_permille: self.metric_weight_permille,
            mark_permille: self.mark_weight_permille,
            width_permille: self.width_weight_permille,
        }
    }

    /// Distance between a glyph and a reference: shape, plus the line-metric term.
    ///
    /// The metric term is `points × weight ÷ 100`: the weight is what a whole cap height costs and
    /// the difference is in percentage points of one.
    ///
    /// Each term is omitted rather than defaulted when either side lacks it. A glyph on a line too
    /// short to locate a baseline, or one whose accent never reached its body, is compared on what
    /// is left — which is worse than the full comparison and much better than being scored against
    /// a height or a direction that was never measured.
    #[must_use]
    pub fn distance(
        self,
        shape: &FeatureVector,
        metrics: LineMetrics,
        mark: MarkSlope,
        aspect: InkAspect,
        entry: &ReferenceEntry,
    ) -> u32 {
        self.weights().distance(
            &(*shape, metrics, mark, aspect),
            &(entry.features, entry.metrics, entry.mark, entry.aspect),
        )
    }
}

/// Matches glyphs against a [`ReferenceSet`] by clustering them first.
pub struct HammingMatcher {
    references: ReferenceSet,
    /// Which entries the scan is allowed to answer with, when a language says some are impossible.
    ///
    /// [#230](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/230). Empty means no
    /// restriction, which is the state every caller that does not ask for one gets and the state a
    /// track whose language is unknown, untagged or absent from the table gets — every uncertainty
    /// resolves to a pass, which is #218's rule and holds here for #218's reason.
    ///
    /// A mask over *entries* rather than a filter on the answer, and the difference is the whole
    /// point: a masked entry is never scored, so the winner is whatever the remaining set returns
    /// and the runner-up is a real second choice. Striking the character out afterwards would leave
    /// the glyph unread, which is a worse answer than the one below it.
    allowed: Vec<bool>,
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
            allowed: Vec::new(),
            thresholds,
            rules: ClusterRules::default(),
            cache: SessionCache::new(),
            distinct_shapes: 0,
            clusters: 0,
            scans: 0,
        })
    }

    /// Refuse every entry the declared language's orthography cannot spell.
    ///
    /// [#230](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/230). #218's script
    /// guard refuses a whole track before the read when the set holds not one character of the
    /// declared script; this is the same evidence at the resolution of one character, applied
    /// during it. Both refuse on a **fact from outside the read** — the container named a language,
    /// the language has a documented orthography, and this character is not in it — which is the
    /// standard `docs/script-guard.md` sets for a gate here, and neither computes anything from the
    /// read to decide.
    ///
    /// Returns the matcher unchanged for a tag the table does not carry, and never restricts to
    /// nothing: a mask that refused every entry would leave a whole track unread, which is a state
    /// no orthography implies and no caller could act on.
    #[must_use]
    pub fn restricted_to_language(mut self, tag: &str) -> Self {
        let mask: Vec<bool> = self
            .references
            .entries()
            .iter()
            .map(|entry| orthography::can_spell(tag, entry.character) != Some(false))
            .collect();
        if mask.iter().any(|allowed| *allowed) {
            self.allowed = mask;
        }
        self
    }

    /// How many entries the language mask refuses, and how many the set holds.
    ///
    /// `(0, n)` where nothing is masked. Reported rather than only applied, because a gate that
    /// fires silently is a gate nobody can audit — which is what `Report::glyphs_without_metrics`
    /// cost this project for as long as it existed and was never printed.
    #[must_use]
    pub fn language_mask(&self) -> (usize, usize) {
        (
            self.allowed.iter().filter(|allowed| !**allowed).count(),
            self.references.entries().len(),
        )
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
        self.scan_with(features, LineMetrics::UNKNOWN, MarkSlope::NONE, InkAspect::UNKNOWN)
    }

    /// Scan the reference set for a glyph whose position in its line, or mark, is known.
    #[must_use]
    pub fn scan_with(
        &self,
        features: &FeatureVector,
        metrics: LineMetrics,
        mark: MarkSlope,
        aspect: InkAspect,
    ) -> GlyphMatch {
        // The winner, and the nearest entry naming a *different* character. Tracking the second as
        // a (distance, character) pair rather than a bare minimum is what keeps the runner-up
        // meaningful when a set holds several entries for one letter: the second-nearest entry is
        // then the same letter in another style, and reporting it would say the matcher could not
        // decide between `a` and `a`. See `GlyphMatch::runner_up_distance`.
        let mut best: Option<(u32, char)> = None;
        let mut runner_up: Option<(u32, char)> = None;

        for (index, entry) in self.references.entries().iter().enumerate() {
            if !self.allowed.is_empty() && !self.allowed[index] {
                continue;
            }
            let distance = self
                .thresholds
                .distance(features, metrics, mark, aspect, entry);
            match best {
                None => best = Some((distance, entry.character)),
                Some((best_distance, winner)) if entry.character == winner => {
                    // Another rendering of the character already winning — a second style, or a
                    // second weight. It can improve the winner and can never be its own runner-up.
                    if distance < best_distance {
                        best = Some((distance, winner));
                    }
                }
                Some((best_distance, winner)) if distance < best_distance => {
                    // A new winner, and the character it displaced is by definition a different one.
                    runner_up = Some((best_distance, winner));
                    best = Some((distance, entry.character));
                }
                Some(_) => {
                    if runner_up.is_none_or(|(d, _)| distance < d) {
                        runner_up = Some((distance, entry.character));
                    }
                }
            }
        }
        let runner_up = runner_up.map_or(u32::MAX, |(distance, _)| distance);

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
            shapes.add(&glyph.features, glyph.metrics, glyph.mark, glyph.aspect);
        }
        self.distinct_shapes = shapes.distinct().try_into().unwrap_or(u64::MAX);

        let clusters = cluster(&shapes, self.rules);
        self.clusters = clusters.len().try_into().unwrap_or(u64::MAX);

        // Scan every centroid before touching the cache, because scanning borrows the reference set
        // and filling the cache borrows the matcher.
        let answers: Vec<GlyphMatch> = clusters
            .iter()
            .map(|c| {
                self.scan_with(&c.centroid, c.centroid_metrics, c.centroid_mark, c.centroid_aspect)
            })
            .collect();
        self.scans += answers.len().try_into().unwrap_or(u64::MAX);

        for (group, answer) in clusters.iter().zip(answers) {
            for ((features, metrics, mark, width), _) in &group.members {
                self.cache.insert(features, *metrics, *mark, *width, answer);
            }
        }
        Ok(())
    }

    fn match_glyph(&mut self, glyph: &Glyph) -> Result<GlyphMatch> {
        if let Some(hit) = self
            .cache
            .get(&glyph.features, glyph.metrics, glyph.mark, glyph.aspect)
        {
            return Ok(hit);
        }
        // Only reachable without a preparation pass, or for a shape it did not see. Answering
        // from a bare scan is worse than answering from a cluster, but it is still an answer, and
        // silently returning "unmatched" for a glyph the matcher simply had not been shown would
        // be a much harder failure to notice.
        let result = self.scan_with(&glyph.features, glyph.metrics, glyph.mark, glyph.aspect);
        self.scans += 1;
        self.cache
            .insert(&glyph.features, glyph.metrics, glyph.mark, glyph.aspect, result);
        Ok(result)
    }

    fn cache_hits(&self) -> u64 {
        self.cache.hits()
    }

    fn cache_lookups(&self) -> u64 {
        self.cache.hits() + self.cache.misses()
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
            mark: MarkSlope::NONE,
            aspect: InkAspect::UNKNOWN,
        }
    }

    fn glyph(bits: &[usize]) -> Glyph {
        Glyph {
            bounds: Rect::new(0, 0, 8, 12),
            line: 0,
            features: vector(bits),
            metrics: LineMetrics::UNKNOWN,
            mark: MarkSlope::NONE,
            aspect: InkAspect::UNKNOWN,
            upright: subtrackt_core::UprightSpan::UNKNOWN,
            bands: subtrackt_core::UprightBands::UNKNOWN,
            slant: subtrackt_core::Slant::UPRIGHT,
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
    fn a_language_mask_hands_the_glyph_to_the_next_candidate_rather_than_leaving_it_unread() {
        // #230's whole design in one assertion. The mask is over *entries*, so a masked entry is
        // never scored and the winner is whatever the remaining set returns -- striking the
        // character out after the scan would leave the glyph unread, which is a worse answer than
        // the one below it and is the failure `CLAUDE.md` opens with.
        let mut acute = FeatureVector::EMPTY;
        acute.set(0);
        acute.set(1);
        let mut bare = FeatureVector::EMPTY;
        bare.set(0);

        let entries = vec![
            ReferenceEntry {
                character: '\u{cd}',
                style: Style::Regular,
                features: acute,
                metrics: LineMetrics::UNKNOWN,
                mark: MarkSlope::NONE,
                aspect: InkAspect::UNKNOWN,
            },
            ReferenceEntry {
                character: 'I',
                style: Style::Regular,
                features: bare,
                metrics: LineMetrics::UNKNOWN,
                mark: MarkSlope::NONE,
                aspect: InkAspect::UNKNOWN,
            },
        ];

        let open = matcher(entries.clone());
        assert_eq!(
            open.scan(&acute).character,
            Some('\u{cd}'),
            "unmasked, the acute wins outright"
        );

        let masked = matcher(entries).restricted_to_language("eng");
        assert_eq!(
            masked.scan(&acute).character,
            Some('I'),
            "English cannot spell it, so the runner-up answers instead of nothing"
        );
        assert_eq!(masked.language_mask(), (1, 2));
    }

    #[test]
    fn a_language_the_table_does_not_carry_masks_nothing() {
        // #218's rule, which this gate shares and for the same reason: a wrong refusal costs a
        // caller an expensive fallback on a track that would have read, and a missed one costs only
        // what the pipeline already did. Every uncertainty resolves to a pass.
        let mut acute = FeatureVector::EMPTY;
        acute.set(0);
        let entries = vec![ReferenceEntry {
            character: '\u{cd}',
            style: Style::Regular,
            features: acute,
            metrics: LineMetrics::UNKNOWN,
            mark: MarkSlope::NONE,
            aspect: InkAspect::UNKNOWN,
        }];
        let m = matcher(entries).restricted_to_language("zzz");
        assert_eq!(m.language_mask(), (0, 1));
        assert_eq!(m.scan(&acute).character, Some('\u{cd}'));
    }

    #[test]
    fn a_mask_that_would_refuse_every_entry_is_not_applied_at_all() {
        // A set of nothing but Spanish-only letters read as English would mask itself to nothing,
        // and a matcher that can answer with no character leaves a whole track unread. That is a
        // state no orthography implies and no caller could act on, so the mask declines instead.
        let mut v = FeatureVector::EMPTY;
        v.set(0);
        let entries = vec![ReferenceEntry {
            character: '\u{f1}',
            style: Style::Regular,
            features: v,
            metrics: LineMetrics::UNKNOWN,
            mark: MarkSlope::NONE,
            aspect: InkAspect::UNKNOWN,
        }];
        let m = matcher(entries).restricted_to_language("eng");
        assert_eq!(m.language_mask(), (0, 1), "not applied, rather than applied to nothing");
        assert_eq!(m.scan(&v).character, Some('\u{f1}'));
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
            let weight = Weights::cells_at(permille, bits);
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
        assert_eq!(Weights::cells_at(permille, 256), 50);
        // Which is to say an `o` against an `O` — 28 points of cap height — still costs 14 cells.
        assert_eq!(28 * Weights::cells_at(permille, 256) / 100, 14);
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
            mark: MarkSlope::NONE,
            aspect: InkAspect::UNKNOWN,
        };

        let none = MarkSlope::NONE;
        assert_eq!(
            t.distance(&shape, LineMetrics::new(104, 0), none, InkAspect::UNKNOWN, &reference),
            0
        );
        assert_eq!(
            t.distance(&shape, LineMetrics::new(76, 0), none, InkAspect::UNKNOWN, &reference),
            28 * t.weights().metric() / 100
        );
        assert_eq!(
            t.distance(&shape, LineMetrics::UNKNOWN, none, InkAspect::UNKNOWN, &reference),
            0
        );
    }

    /// A reference `à`: a grave, which leans the opposite way to an acute.
    fn accented() -> ReferenceEntry {
        ReferenceEntry {
            character: '\u{e0}',
            style: Style::Regular,
            features: vector(&[1, 2, 3]),
            metrics: LineMetrics::UNKNOWN,
            mark: MarkSlope::new(67),
            aspect: InkAspect::UNKNOWN,
        }
    }

    #[test]
    fn a_second_entry_for_the_winning_character_is_not_its_own_runner_up() {
        // #66. A reference set may hold one entry per style for a character, and the second-nearest
        // entry is then the same letter in another weight. Reporting it as the runner-up would say
        // the matcher could not decide between `a` and `a`, and every glyph in a track carrying
        // style variants would come back ambiguous — which would quietly corrupt the accuracy gate,
        // post-correction's candidate count, and `xtask separability`'s own predicate.
        let upright = entry('a', &[1, 2, 3]);
        let italic = ReferenceEntry {
            style: Style::Italic,
            features: vector(&[1, 2, 4]),
            ..entry('a', &[])
        };
        let other = entry('b', &[60, 61, 62, 63]);

        let m = matcher(vec![upright, italic, other]);
        let result = m.scan(&vector(&[1, 2, 3]));

        assert_eq!(result.character, Some('a'));
        assert_eq!(result.distance, 0, "the upright entry matches exactly");
        assert!(
            result.is_unambiguous(m.ambiguity_margin()),
            "the runner-up is `b` at distance {}, not the other `a`",
            result.runner_up_distance
        );
    }

    #[test]
    fn the_closest_entry_wins_whichever_style_it_came_from() {
        // The other half of the same change: style variants exist so an italic glyph can find an
        // italic vector. The letterform decides, and the style byte is never read to make it.
        let upright = entry('a', &[1, 2, 3]);
        let italic = ReferenceEntry {
            style: Style::Italic,
            features: vector(&[40, 41, 42]),
            ..entry('a', &[])
        };

        let m = matcher(vec![upright, italic]);
        assert_eq!(m.scan(&vector(&[40, 41, 42])).distance, 0, "the italic vector matched");
        assert_eq!(m.scan(&vector(&[1, 2, 3])).distance, 0, "and so did the upright one");
    }

    #[test]
    fn a_set_with_one_entry_per_character_is_unaffected_by_the_runner_up_rule() {
        // Every set ever generated has one entry per character, so the change above is a no-op on
        // all of them. Pinned because "this changes nothing today" is the claim that made it safe
        // to land ahead of the thing that needs it.
        let m = matcher(vec![
            entry('a', &[1, 2, 3]),
            entry('b', &[1, 2, 60]),
            entry('c', &[60, 61, 62]),
        ]);
        let result = m.scan(&vector(&[1, 2, 3]));
        assert_eq!(result.character, Some('a'));
        assert_eq!(result.distance, 0);
        assert_eq!(result.runner_up_distance, 2, "`b` differs in two cells");
    }

    #[test]
    fn the_mark_term_is_off_by_default_because_measuring_it_found_nothing_for_it_to_do() {
        // Pinned so the setting is a decision rather than an oversight. `xtask mark-sweep` moved
        // CER on none of eight conditions and made it worse on four; the accents the pipeline gets
        // wrong differ in their base letter, not their direction, and this term cannot see that.
        // See `docs/glyph-stability.md`.
        let t = MatchThresholds::default();
        assert_eq!(t.mark_weight_permille, 0);
        assert_eq!(
            t.distance(
                &vector(&[1, 2, 3]),
                LineMetrics::UNKNOWN,
                MarkSlope::new(-64),
                InkAspect::UNKNOWN,
                &accented()
            ),
            0,
            "with the term off, two opposite accents are the same distance apart as none at all"
        );
    }

    #[test]
    fn a_wrong_leaning_accent_costs_a_demotion_rather_than_a_rejection() {
        // What the term does when it is switched on. An acute against a grave measures about 131
        // points apart, and at the sweep's mid setting that has to buy a demotion: past the
        // ambiguity margin, short of the distance ceiling. A mark is a handful of pixels on real
        // material, and a term strong enough to reject on its own turns a marginal rasterisation
        // into an unmatched glyph — which the sweep watched happen above 78.
        let t = MatchThresholds { mark_weight_permille: 78, ..MatchThresholds::default() };
        let shape = vector(&[1, 2, 3]);
        let reference = accented();

        let cost = 131 * t.weights().mark() / 100;
        assert_eq!(
            t.distance(
                &shape,
                LineMetrics::UNKNOWN,
                MarkSlope::new(-64),
                InkAspect::UNKNOWN,
                &reference
            ),
            cost
        );
        assert!(cost > t.ambiguity_margin(), "a wrong accent has to be visible");
        assert!(cost < t.max_distance(), "but not a rejection on its own");

        // A glyph whose mark never reached its body is compared on shape alone rather than charged
        // for a direction that was never delivered.
        assert_eq!(
            t.distance(
                &shape,
                LineMetrics::UNKNOWN,
                MarkSlope::NONE,
                InkAspect::UNKNOWN,
                &reference
            ),
            0
        );
    }

    #[test]
    fn the_width_term_separates_an_l_from_an_i_that_the_shape_vector_cannot() {
        // #109 and #110, as a test. The two are the same 256 bits, the same height and the same
        // descent, and carry no mark — every other term in this function returns zero for them by
        // construction — so whatever the aspect ratio charges is the whole of the decision.
        let t = MatchThresholds::default();
        let shape = vector(&[1, 2, 3]);
        let metrics = LineMetrics::new(100, 0);
        let entry = |character, permille| ReferenceEntry {
            character,
            style: Style::Regular,
            features: shape,
            metrics,
            mark: MarkSlope::NONE,
            aspect: InkAspect::new(permille),
        };
        // What a set carries for Arial: rasterised at the size subtitles are drawn at, where the
        // stems land on different whole pixels the way the material's do.
        let (narrow, wide) = (entry('l', 125), entry('I', 150));
        // A glyph a real disc drew five pixels wide on a forty-two pixel line.
        let observed = InkAspect::new(119);

        let to_l = t.distance(&shape, metrics, MarkSlope::NONE, observed, &narrow);
        let to_i = t.distance(&shape, metrics, MarkSlope::NONE, observed, &wide);
        assert!(to_l < to_i, "an `l` has to read as an `l`: {to_l} against {to_i}");
        assert_eq!(
            t.distance(&shape, metrics, MarkSlope::NONE, InkAspect::UNKNOWN, &narrow),
            t.distance(&shape, metrics, MarkSlope::NONE, InkAspect::UNKNOWN, &wide),
            "and with no ratio measured the two are indistinguishable again, as they were"
        );
    }

    #[test]
    fn the_shipped_width_weight_is_the_window_three_discs_agree_on() {
        // Pinned so the setting is a decision rather than a number. `xtask width-sweep` runs the
        // whole pipeline at each candidate against three Blu-rays and scores every result: 180 to
        // 200 is where all three are at their best, and 190 is the middle of it. See
        // `docs/error-census.md`.
        let shipped = MatchThresholds::default();
        assert_eq!(shipped.width_weight_permille, 190);

        // What the window's *ceiling* is for. `s` and `S` share a shape and differ in height, so
        // #37's term is what should decide them and this one must not overrule it — and at 512px it
        // did, because the outline's ratios sat on the wrong side of what the disc draws and the
        // weight was high enough to charge for it. At the shipped pair of settings the two
        // candidates cost the same, so the term declines to decide a pair it cannot see.
        let shape = vector(&[]);
        let entry = |character, permille| ReferenceEntry {
            character,
            style: Style::Regular,
            features: shape,
            metrics: LineMetrics::UNKNOWN,
            mark: MarkSlope::NONE,
            aspect: InkAspect::new(permille),
        };
        let to = |t: MatchThresholds, observed, e: &ReferenceEntry| {
            t.distance(&shape, LineMetrics::UNKNOWN, MarkSlope::NONE, observed, e)
        };
        let (capital, lower) = (entry('S', 738), entry('s', 774));
        let observed = InkAspect::new(758);
        assert_eq!(
            to(shipped, observed, &lower),
            to(shipped, observed, &capital),
            "an `s` between the two entries must be left to the term that can tell them apart"
        );

        // And the floor. `l` against `I` is 25 points, which the shipped weight turns into a cell
        // and a weight of 60 does not — that is the bound the sweep found by measurement.
        let (narrow, wide) = (entry('l', 125), entry('I', 150));
        let stem = InkAspect::new(119);
        assert!(
            to(shipped, stem, &narrow) < to(shipped, stem, &wide),
            "the pair the term exists for has to be decided"
        );
        let below = MatchThresholds { width_weight_permille: 60, ..shipped };
        assert_eq!(
            to(below, stem, &narrow),
            to(below, stem, &wide),
            "under the floor the difference rounds to zero and the pair is a tie again"
        );
    }

    #[test]
    fn two_accents_leaning_the_same_way_are_not_separated_by_the_mark_at_all() {
        // The finding that decided #48, as a test. `á` and `é` both carry an acute, so their
        // slopes agree and the term contributes nothing however hard it is weighted — and those
        // are the pairs the pipeline actually gets wrong. The pairs it can separate were already
        // being read correctly.
        let t = MatchThresholds { mark_weight_permille: 293, ..MatchThresholds::default() };
        let acute = MarkSlope::new(-66);
        let reference = ReferenceEntry { mark: MarkSlope::new(-64), ..accented() };
        assert_eq!(
            t.distance(
                &vector(&[1, 2, 3]),
                LineMetrics::UNKNOWN,
                acute,
                InkAspect::UNKNOWN,
                &reference
            ),
            2 * t.weights().mark() / 100,
            "two acutes differ only by rasterisation noise, whatever letters they sit on"
        );
    }
}
