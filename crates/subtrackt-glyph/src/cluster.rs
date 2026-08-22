//! Grouping a stream's own shapes together before any of them is matched.
//!
//! This is the mechanism [#10](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/10)
//! became after #14 measured why the obvious design cannot work. Two renderings of the *same*
//! character sit a median 46 cells apart; two *different* characters sit a median 31 apart. No
//! threshold separates those distributions, so matching each glyph against a fixed reference set
//! independently is asking a question that has no right answer.
//!
//! What makes the problem tractable is that the expensive variation is *between* streams, not
//! within one. A title is authored once: one typeface, one weight, one palette, one resolution. Its
//! glyphs vary only along the cheap axes — rendering size at a median 11 cells, anti-aliasing at 8
//! — while weight (38) and slant (47) are constant. So a stream's own shapes cluster cleanly even
//! though the library's do not.
//!
//! Clustering first therefore does two things at once. It cancels exactly the variation that
//! defeats a fixed set, and it replaces per-glyph noise with a per-cluster consensus, so the vector
//! that reaches the reference set is one that many instances voted on rather than one instance's
//! accidents.
//!
//! It is also *less* work than matching each glyph. A feature-length film yields a few hundred
//! distinct shapes out of tens of thousands of glyphs, so clustering runs over hundreds of items
//! and the reference set is scanned once per cluster rather than once per glyph.

use std::collections::HashMap;

use subtrackt_core::{FEATURE_BITS, FEATURE_WORDS, FeatureVector, LineMetrics};

/// A shape together with where it sat in its line — what identifies a glyph for grouping.
///
/// Shape alone stopped identifying a glyph in #37: an `o` and an `O` can normalise to the same
/// vector, and treating them as one shape would undo the feature that separates them.
pub type Shape = (FeatureVector, LineMetrics);

/// What a shape is stored under. `LineMetrics` is not `Hash`, so its fields are unpacked.
type Key = ([u64; FEATURE_WORDS], u32, i32, bool);

fn key_of(shape: &Shape) -> Key {
    (
        *shape.0.words(),
        shape.1.height_percent,
        shape.1.descent_percent,
        shape.1.known,
    )
}

/// How a stream's shapes are grouped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClusterRules {
    /// How far a shape may sit from a cluster's centroid and still belong to it, in percent of
    /// [`FEATURE_BITS`].
    ///
    /// Expressed as a fraction rather than a cell count so a grid-size change does not silently
    /// change how permissive clustering is — the same rule the match thresholds follow.
    pub radius_percent: u32,
    /// How much a percentage point of line-metric difference is worth, in hundredths of a cell.
    ///
    /// Mirrors `MatchThresholds::metric_weight`, and for the same reason: two glyphs of identical
    /// shape at different heights are different characters, so grouping them by shape alone would
    /// merge exactly what #37 set out to separate.
    pub metric_weight: u32,
    /// How many times centroids are recomputed and shapes reassigned after the first pass.
    ///
    /// The first pass is order-dependent: it walks shapes most-frequent-first and drops each into
    /// the nearest cluster that will take it, so an early leader that turns out to be off-centre
    /// keeps whatever it collected. Recomputing the centroid and reassigning fixes that. Two passes
    /// is not a tuned figure; it is where movement stops on real streams, and the loop exits early
    /// when nothing moves anyway.
    pub refine_passes: u32,
}

impl Default for ClusterRules {
    /// A radius of zero: every distinct shape keeps its own label decision.
    ///
    /// Which is to say clustering is **off by default**, because `xtask cluster-sweep` measured it
    /// and it does not help. The reasoning that motivated it was sound and the measurement still
    /// disagreed; `docs/glyph-stability.md` records the numbers.
    ///
    /// The short version is that a radius has a ceiling it cannot reach. Clustering needs a
    /// character's own renderings to be closer to each other than the nearest *different* character
    /// is — and in the reference set `I`, `l` and `|` are at distance **zero** from one another,
    /// with `!` four cells away. Letterboxing normalises a vertical bar to a vertical bar whatever
    /// its height, so no radius above nothing can group a stream's variation without first merging
    /// characters that were never distinguishable. The smallest radius tried, five cells, already
    /// merges fifteen pairs.
    ///
    /// So this is not a knob to tune. It stays because the machinery is the instrument that
    /// measured the question, and because the finding it produced — that the feature vector cannot
    /// separate confusable characters at all — is what the next experiment has to attack.
    fn default() -> Self {
        Self { radius_percent: 0, metric_weight: 50, refine_passes: 2 }
    }
}

impl ClusterRules {
    /// The radius in cells.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn radius(self) -> u32 {
        (FEATURE_BITS as u32) * self.radius_percent / 100
    }
}

/// The distinct shapes a stream contains, with how often each occurred.
///
/// Counting distinct shapes rather than carrying every glyph is not only an optimisation. A
/// centroid should be the consensus of the *renderings* weighted by how common they are, and a
/// shape that occurs four hundred times is four hundred votes for the way that character usually
/// looks.
#[derive(Debug, Default, Clone)]
pub struct Shapes {
    counts: HashMap<Key, (Shape, u64)>,
    total: u64,
}

impl Shapes {
    /// An empty tally.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one occurrence of a shape.
    pub fn add(&mut self, features: &FeatureVector, metrics: LineMetrics) {
        let shape = (*features, metrics);
        self.counts.entry(key_of(&shape)).or_insert((shape, 0)).1 += 1;
        self.total += 1;
    }

    /// How many distinct shapes were seen.
    #[must_use]
    pub fn distinct(&self) -> usize {
        self.counts.len()
    }

    /// How many glyphs were recorded in total.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.total
    }

    /// Whether nothing has been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// The distinct shapes, most frequent first.
    ///
    /// The order is fully determined — ties break on the raw words — because clustering's first
    /// pass depends on it, and a matcher whose output changed between runs of the same file would
    /// be untestable.
    #[must_use]
    pub fn by_frequency(&self) -> Vec<(Shape, u64)> {
        let mut out: Vec<(Shape, u64)> = self.counts.values().copied().collect();
        out.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| key_of(&a.0).cmp(&key_of(&b.0))));
        out
    }
}

/// One group of shapes the stream treats as the same character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cluster {
    /// The consensus vector, which is what gets matched against the reference set.
    pub centroid: FeatureVector,
    /// The consensus line metrics, matched alongside the centroid.
    pub centroid_metrics: LineMetrics,
    /// The distinct shapes in this cluster, with their occurrence counts.
    pub members: Vec<(Shape, u64)>,
    /// Total glyph occurrences across all members.
    pub weight: u64,
}

impl Cluster {
    /// The furthest any member sits from the centroid, in cells.
    #[must_use]
    pub fn spread(&self) -> u32 {
        self.members
            .iter()
            .map(|((v, _), _)| self.centroid.distance(v))
            .max()
            .unwrap_or(0)
    }

    /// The centroid as a shape.
    #[must_use]
    pub const fn centre(&self) -> Shape {
        (self.centroid, self.centroid_metrics)
    }
}

/// The consensus vector of a weighted set of shapes: each cell takes the majority verdict.
///
/// Majority is the right centre for Hamming space specifically — it is the vector minimising total
/// distance to the members, the same way a median rather than a mean minimises absolute deviation.
/// A cell is set when strictly more than half the weight says so, which leaves an exact tie
/// background; there is no principled way to break one, and background is the answer that adds no
/// ink the members did not agree on.
#[must_use]
pub fn centroid(members: &[(Shape, u64)]) -> Shape {
    let total: u64 = members.iter().map(|(_, count)| *count).sum();
    if total == 0 {
        return (FeatureVector::EMPTY, LineMetrics::UNKNOWN);
    }

    let mut out = FeatureVector::EMPTY;
    for bit in 0..FEATURE_BITS {
        let set: u64 = members
            .iter()
            .filter(|((v, _), _)| v.get(bit))
            .map(|(_, count)| *count)
            .sum();
        if set * 2 > total {
            out.set(bit);
        }
    }

    // Metrics average rather than vote: they are magnitudes, not per-cell decisions, and the mean
    // of several measurements of one character is a better estimate than any of them. Only members
    // that *have* metrics contribute, and the result is known only if most of the weight does.
    let known: u64 = members
        .iter()
        .filter(|((_, m), _)| m.known)
        .map(|(_, count)| *count)
        .sum();
    let metrics = if known * 2 > total {
        let weighted = |pick: fn(&LineMetrics) -> i64| -> i64 {
            let sum: i64 = members
                .iter()
                .filter(|((_, m), _)| m.known)
                .map(|((_, m), count)| pick(m) * i64::try_from(*count).unwrap_or(0))
                .sum();
            sum / i64::try_from(known).unwrap_or(1)
        };
        LineMetrics::new(
            u32::try_from(weighted(|m| i64::from(m.height_percent))).unwrap_or(0),
            i32::try_from(weighted(|m| i64::from(m.descent_percent))).unwrap_or(0),
        )
    } else {
        LineMetrics::UNKNOWN
    };

    (out, metrics)
}

/// Distance between two shapes: Hamming on the vector, plus the weighted metric difference.
#[must_use]
pub fn distance(a: &Shape, b: &Shape, rules: ClusterRules) -> u32 {
    let base = a.0.distance(&b.0);
    a.1.difference(b.1)
        .map_or(base, |points| base + points * rules.metric_weight / 100)
}

/// Group a stream's shapes.
///
/// Returns clusters ordered by weight, heaviest first, so a caller reporting the top few is
/// reporting the ones that carry the stream.
#[must_use]
pub fn cluster(shapes: &Shapes, rules: ClusterRules) -> Vec<Cluster> {
    let ranked = shapes.by_frequency();
    if ranked.is_empty() {
        return Vec::new();
    }
    let radius = rules.radius();

    // First pass: walk shapes most-frequent-first, dropping each into the nearest cluster within
    // the radius and opening a new one otherwise. Frequency order matters — the commonest rendering
    // of a character is the one least likely to be a distorted outlier, so it makes the better
    // leader.
    let mut centroids: Vec<Shape> = Vec::new();
    let mut assignment: Vec<usize> = Vec::with_capacity(ranked.len());
    for (shape, _) in &ranked {
        match nearest(&centroids, shape, rules) {
            Some((index, distance)) if distance <= radius => assignment.push(index),
            _ => {
                centroids.push(*shape);
                assignment.push(centroids.len() - 1);
            }
        }
    }

    // Refinement: recompute each centroid from everything that landed in it, then reassign. No new
    // clusters open here, so this cannot fragment what the first pass built — it only corrects
    // shapes that joined a cluster before its centre was known.
    for _ in 0..rules.refine_passes {
        centroids = recentre(&ranked, &assignment, centroids.len());
        let mut moved = false;
        for (slot, (shape, _)) in ranked.iter().enumerate() {
            if let Some((index, _)) = nearest(&centroids, shape, rules) {
                if index != assignment[slot] {
                    assignment[slot] = index;
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }

    build(&ranked, &assignment, centroids.len())
}

/// The nearest centroid to a shape, as `(index, distance)`.
fn nearest(centroids: &[Shape], shape: &Shape, rules: ClusterRules) -> Option<(usize, u32)> {
    centroids
        .iter()
        .enumerate()
        .map(|(index, centroid)| (index, distance(centroid, shape, rules)))
        .min_by_key(|(_, distance)| *distance)
}

/// Recompute every centroid from its current membership.
fn recentre(ranked: &[(Shape, u64)], assignment: &[usize], count: usize) -> Vec<Shape> {
    let mut buckets: Vec<Vec<(Shape, u64)>> = vec![Vec::new(); count];
    for (slot, entry) in ranked.iter().enumerate() {
        buckets[assignment[slot]].push(*entry);
    }
    buckets.iter().map(|members| centroid(members)).collect()
}

/// Turn the assignment into clusters, dropping any that refinement emptied.
fn build(ranked: &[(Shape, u64)], assignment: &[usize], count: usize) -> Vec<Cluster> {
    let mut buckets: Vec<Vec<(Shape, u64)>> = vec![Vec::new(); count];
    for (slot, entry) in ranked.iter().enumerate() {
        buckets[assignment[slot]].push(*entry);
    }

    let mut out: Vec<Cluster> = buckets
        .into_iter()
        .filter(|members| !members.is_empty())
        .map(|members| {
            let (centroid, centroid_metrics) = centroid(&members);
            Cluster {
                centroid,
                centroid_metrics,
                weight: members.iter().map(|(_, count)| *count).sum(),
                members,
            }
        })
        .collect();

    // Heaviest first, ties broken on the centroid so the order is reproducible.
    out.sort_unstable_by(|a, b| {
        b.weight
            .cmp(&a.weight)
            .then_with(|| a.centroid.words().cmp(b.centroid.words()))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rules that actually group, since the shipped default does not.
    ///
    /// Eight percent is the radius the sweep in `xtask cluster-sweep` centred on. These tests are
    /// about whether the algorithm does what it says, not about whether grouping helps.
    fn grouping() -> ClusterRules {
        ClusterRules { radius_percent: 8, metric_weight: 50, refine_passes: 2 }
    }

    /// A vector with the given bits set.
    fn vector(bits: &[usize]) -> FeatureVector {
        let mut v = FeatureVector::EMPTY;
        for bit in bits {
            v.set(*bit);
        }
        v
    }

    /// A shape with the given bits set and no line metrics.
    fn shape(bits: &[usize]) -> Shape {
        (vector(bits), LineMetrics::UNKNOWN)
    }

    /// A vector of `count` consecutive bits from `start`.
    fn run(start: usize, count: usize) -> FeatureVector {
        vector(&(start..start + count).collect::<Vec<_>>())
    }

    /// Add `times` occurrences of a shape.
    ///
    /// These tests are about grouping by shape, so every shape carries unknown metrics — which
    /// makes the metric term contribute nothing and leaves the shape distance to decide, exactly as
    /// it did before metrics existed.
    fn add(shapes: &mut Shapes, v: FeatureVector, times: u64) {
        for _ in 0..times {
            shapes.add(&v, LineMetrics::UNKNOWN);
        }
    }

    #[test]
    fn shapes_are_counted_by_distinct_value() {
        let mut shapes = Shapes::new();
        add(&mut shapes, run(0, 10), 5);
        add(&mut shapes, run(50, 10), 2);

        assert_eq!(shapes.distinct(), 2);
        assert_eq!(shapes.total(), 7);
        assert!(!shapes.is_empty());

        let ranked = shapes.by_frequency();
        assert_eq!(ranked[0].1, 5, "the commonest shape leads");
        assert_eq!(ranked[1].1, 2);
    }

    #[test]
    fn frequency_order_is_reproducible_when_counts_tie() {
        // Clustering's first pass depends on this order, so a matcher whose answer changed between
        // runs of the same file would be untestable.
        let mut first = Shapes::new();
        let mut second = Shapes::new();
        for bit in [0usize, 40, 80, 120] {
            first.add(&run(bit, 10), LineMetrics::UNKNOWN);
        }
        for bit in [120usize, 80, 40, 0] {
            second.add(&run(bit, 10), LineMetrics::UNKNOWN);
        }
        assert_eq!(first.by_frequency(), second.by_frequency());
    }

    #[test]
    fn a_centroid_takes_the_majority_verdict_on_every_cell() {
        let members = vec![
            (shape(&[0, 1, 2]), 1),
            (shape(&[0, 1, 3]), 1),
            (shape(&[0, 1, 4]), 1),
        ];
        let (c, _) = centroid(&members);

        assert!(c.get(0) && c.get(1), "cells all three agree on stay");
        assert!(!c.get(2) && !c.get(3) && !c.get(4), "one vote in three is noise");
        assert_eq!(c.popcount(), 2);
    }

    #[test]
    fn a_centroid_is_weighted_by_how_often_each_shape_occurred() {
        // The point of counting occurrences. One malformed rendering must not outvote four hundred
        // clean ones just by being present.
        let members = vec![(shape(&[0, 1]), 400), (shape(&[0, 1, 2, 3, 4, 5]), 1)];
        assert_eq!(centroid(&members).0, vector(&[0, 1]));
    }

    #[test]
    fn an_exactly_split_cell_stays_background() {
        let members = vec![(shape(&[0]), 5), (shape(&[]), 5)];
        assert_eq!(
            centroid(&members).0,
            FeatureVector::EMPTY,
            "a tie adds no ink the members did not agree on"
        );
    }

    #[test]
    fn an_empty_membership_has_an_empty_centroid_rather_than_dividing_by_zero() {
        assert_eq!(centroid(&[]).0, FeatureVector::EMPTY);
        assert_eq!(centroid(&[(shape(&[1, 2]), 0)]).0, FeatureVector::EMPTY);
    }

    #[test]
    fn renderings_of_one_character_collapse_into_a_single_cluster() {
        // The property the redesign exists for: variants of one shape, each within the radius of
        // the common rendering, must come back as one thing to match rather than as five.
        let mut shapes = Shapes::new();
        let base: Vec<usize> = (0..40).collect();
        add(&mut shapes, vector(&base), 100);
        for extra in 40..45 {
            let mut with = base.clone();
            with.push(extra);
            add(&mut shapes, vector(&with), 10);
        }

        let clusters = cluster(&shapes, grouping());
        assert_eq!(clusters.len(), 1, "six renderings of one character");
        assert_eq!(clusters[0].members.len(), 6);
        assert_eq!(clusters[0].weight, 150);
        assert_eq!(
            clusters[0].centroid,
            vector(&base),
            "the consensus is the shape they share, not any one variant"
        );
    }

    #[test]
    fn different_characters_stay_in_different_clusters() {
        // The other half: absorbing variation must not also absorb the distinctions.
        // Runs sized off the vector rather than in absolute cells: the radius is a percentage of
        // FEATURE_BITS, so a fixed 40-cell run would sit inside the radius on a larger grid and the
        // test would be asserting the opposite of what it names.
        let span = FEATURE_BITS / 6;
        let mut shapes = Shapes::new();
        add(&mut shapes, run(0, span), 100);
        add(&mut shapes, run(FEATURE_BITS / 3, span), 80);
        add(&mut shapes, run(FEATURE_BITS * 2 / 3, span), 60);

        let clusters = cluster(&shapes, grouping());
        assert_eq!(clusters.len(), 3);
        assert_eq!(clusters[0].weight, 100, "heaviest first");
        assert_eq!(clusters[2].weight, 60);
    }

    #[test]
    fn a_shape_just_outside_the_radius_opens_its_own_cluster() {
        let rules = grouping();
        let mut inside = Shapes::new();
        add(&mut inside, run(0, 40), 10);
        add(&mut inside, run(0, 40 + rules.radius() as usize), 5);
        assert_eq!(cluster(&inside, rules).len(), 1);

        let mut outside = Shapes::new();
        add(&mut outside, run(0, 40), 10);
        add(&mut outside, run(0, 41 + rules.radius() as usize), 5);
        assert_eq!(cluster(&outside, rules).len(), 2);
    }

    #[test]
    fn refinement_moves_a_shape_that_joined_the_wrong_cluster_first() {
        // The first pass is order-dependent: a shape can land in a cluster whose centre later moves
        // away from it. Without refinement it would stay there, and its label would come from the
        // wrong consensus.
        let mut shapes = Shapes::new();
        add(&mut shapes, run(0, 40), 50);
        add(&mut shapes, run(0, 58), 40);
        add(&mut shapes, run(0, 76), 30);

        let refined = cluster(&shapes, ClusterRules::default());
        let raw = cluster(&shapes, ClusterRules { refine_passes: 0, ..ClusterRules::default() });

        // Every shape must end up with the centroid it is actually nearest to.
        for c in &refined {
            for ((member, _), _) in &c.members {
                let mine = c.centroid.distance(member);
                for other in &refined {
                    assert!(
                        mine <= other.centroid.distance(member),
                        "a member sits closer to another cluster's centroid"
                    );
                }
            }
        }
        assert!(!raw.is_empty(), "the unrefined pass still produces clusters");
    }

    #[test]
    fn every_shape_lands_in_exactly_one_cluster() {
        let mut shapes = Shapes::new();
        for start in (0..200).step_by(7) {
            add(&mut shapes, run(start, 30), u64::try_from(start).unwrap() + 1);
        }

        let clusters = cluster(&shapes, grouping());
        let members: u64 = clusters.iter().map(|c| c.members.len() as u64).sum();
        let weight: u64 = clusters.iter().map(|c| c.weight).sum();

        assert_eq!(members, shapes.distinct() as u64, "no shape lost or duplicated");
        assert_eq!(weight, shapes.total(), "no occurrence lost or duplicated");
        assert!(
            clusters.iter().all(|c| !c.members.is_empty()),
            "no empty clusters survive"
        );
    }

    #[test]
    fn clustering_nothing_yields_nothing() {
        assert!(cluster(&Shapes::new(), ClusterRules::default()).is_empty());
    }

    #[test]
    fn the_radius_scales_with_the_grid_rather_than_being_a_raw_cell_count() {
        let rules = ClusterRules { radius_percent: 8, ..ClusterRules::default() };
        assert_eq!(rules.radius(), u32::try_from(FEATURE_BITS).unwrap() * 8 / 100);
        assert_eq!(ClusterRules { radius_percent: 0, ..rules }.radius(), 0);
        assert_eq!(
            ClusterRules::default().radius(),
            0,
            "clustering ships off; the sweep measured every radius worse"
        );
    }

    #[test]
    fn a_zero_radius_keeps_every_distinct_shape_apart() {
        // Which is the degenerate case worth pinning: with no radius, clustering must reduce to
        // exactly the behaviour that existed before it — one label decision per distinct shape.
        let mut shapes = Shapes::new();
        add(&mut shapes, run(0, 40), 10);
        add(&mut shapes, run(0, 41), 10);
        add(&mut shapes, run(0, 42), 10);

        let clusters = cluster(&shapes, ClusterRules { radius_percent: 0, ..grouping() });
        assert_eq!(clusters.len(), 3);
    }

    #[test]
    fn spread_reports_the_furthest_member_from_the_centre() {
        let c = Cluster {
            centroid: vector(&[0, 1, 2]),
            centroid_metrics: LineMetrics::UNKNOWN,
            members: vec![(shape(&[0, 1, 2]), 5), (shape(&[0, 1, 2, 3, 4]), 1)],
            weight: 6,
        };
        assert_eq!(c.spread(), 2);
    }

    #[test]
    fn clustering_a_realistic_shape_count_is_fast_enough_to_be_irrelevant() {
        // A feature-length film yields a few hundred distinct shapes. If this were accidentally
        // quadratic in *glyphs* rather than in distinct shapes it would not show up on a fixture.
        let mut shapes = Shapes::new();
        for start in 0..400usize {
            add(&mut shapes, run(start % 200, 20 + start % 30), 50);
        }

        let start = std::time::Instant::now();
        let clusters = cluster(&shapes, grouping());
        let elapsed = start.elapsed();

        assert!(!clusters.is_empty());
        assert!(elapsed.as_millis() < 2_000, "clustering took {elapsed:?}");
    }
}
