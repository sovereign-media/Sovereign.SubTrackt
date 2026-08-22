//! Choosing which reference set to read a title with.
//!
//! Answers the half of [#62][62] that measurement supports. `docs/reference-set.md` prices the
//! problem: the same Blu-ray reads at **7.1%** character error with a reference set built from the
//! typeface it was authored in and at **17% to 62%** with one that was not. Nothing in the binary
//! can know which is which, so until a title is fitted the tool matches no glyphs at all.
//!
//! Two things this deliberately does not do.
//!
//! **It does not decide.** [#63][63] measured four statistics against a leave-one-out over eight
//! typefaces and none separates a good read from a bad one — a systematically wrong set is *by
//! construction* a low-distance one. So this ranks candidates and reports the scores; whether the
//! winner is good enough is not a question it can answer, and pretending otherwise would produce
//! the confident wrong answer §4 of #1 rejected general OCR to avoid.
//!
//! **It does not build reference sets from fonts.** Rasterising a typeface needs a font engine, and
//! the one this project already uses requires a newer compiler than the shipped crates are held to.
//! Generating a `.subtref` therefore stays an `xtask` step, and whether it belongs in a distributed
//! binary is [#16][16]'s question. What ships here is the choice among sets that already exist,
//! which is the part the measurements are about.
//!
//! [62]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/62
//! [63]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/63
//! [16]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/16

use std::collections::HashMap;

use subtrackt_core::Result;
use subtrackt_core::progress::{Phase, Progress, Silent};
use subtrackt_glyph::matcher::MatchThresholds;
use subtrackt_glyph::{HammingMatcher, ReferenceSet};

use crate::survey::GlyphSurvey;

/// How one candidate reference set scored against a title's glyphs.
#[derive(Debug, Clone, PartialEq)]
pub struct Fit {
    /// The set's name, as it will appear in `--report` if this one is chosen.
    pub name: String,
    /// Glyph occurrences the set identified.
    pub matched: u64,
    /// Glyph occurrences it could not.
    pub unmatched: u64,
    /// Distinct shapes scanned, which is far fewer than the occurrences they stand for.
    pub distinct: u64,
    /// Summed distance of the occurrences that matched.
    pub distance_sum: u64,
    /// The score this is ranked on, in cells. Lower fits better.
    ///
    /// Mean distance over *every* glyph, charging each unread one the match ceiling — **not** the
    /// mean over the glyphs that matched, which is what `Report::mean_match_distance` reports as
    /// `fit` and which is the wrong statistic for this job. A mean taken over matches alone rewards
    /// a set that recognises a tenth of a track at close range over one that recognises all of it
    /// at medium range, and at the scale a real font directory offers that is not hypothetical:
    /// `docs/reference-set.md` records a symbol face winning Georgia-rendered material on it and
    /// then reading that material at 79.8% character error. Charging the unread removed it, at 0.7
    /// points of CER on average across ten materials against mean-over-matched's 15.3.
    ///
    /// The ceiling is a *lower bound* on what an unread glyph really cost — it was rejected for
    /// exceeding that distance — which keeps the number honest in the direction that matters.
    pub score: f32,
}

impl Fit {
    /// Fraction of glyph occurrences the set identified.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn coverage(&self) -> f32 {
        let total = self.matched + self.unmatched;
        if total == 0 {
            return 0.0;
        }
        self.matched as f32 / total as f32
    }

    /// The ranking score. See the field of the same name for why it charges the unread glyphs.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    fn score(matched: u64, unmatched: u64, distance_sum: u64, ceiling: u32) -> f32 {
        let total = matched + unmatched;
        if total == 0 {
            return f32::from(u16::try_from(ceiling).unwrap_or(u16::MAX));
        }
        (distance_sum as f32 + unmatched as f32 * ceiling as f32) / total as f32
    }
}

/// Score one reference set against a surveyed title.
///
/// Scans one shape per *distinct* glyph rather than one per occurrence. Subtitle text repeats
/// heavily — a feature film's twenty thousand glyphs are a few hundred shapes — so this is the
/// difference between fitting a title in a second and fitting it in a minute, and it is the same
/// economy [`crate::Pipeline`] already relies on through its session cache.
///
/// # Errors
/// Returns [`subtrackt_core::Error::Config`] if the set was generated for a different grid size
/// than this build uses, which the matcher refuses rather than comparing across.
pub fn score_set(
    survey: &GlyphSurvey,
    reference: ReferenceSet,
    thresholds: MatchThresholds,
) -> Result<Fit> {
    let name = reference.name().to_owned();
    let matcher = HammingMatcher::new(reference, thresholds)?;

    // Occurrences per distinct shape. The key has to carry metrics and the mark as well as the
    // vector, for the reason `cache::cache_key` gives: an `o` and an `O` normalise alike, and so do
    // an `à` and an `á`, so keying on shape alone would collapse glyphs the matcher separates.
    let mut counts: HashMap<u64, (usize, u64)> = HashMap::new();
    for (index, glyph) in survey.glyphs.iter().enumerate() {
        let key = subtrackt_glyph::cache::cache_key(&glyph.features, glyph.metrics, glyph.mark);
        counts.entry(key).or_insert((index, 0)).1 += 1;
    }

    let (mut read, mut unread, mut distance_sum) = (0u64, 0u64, 0u64);
    for (index, occurrences) in counts.values() {
        let glyph = &survey.glyphs[*index];
        let result = matcher.scan_with(&glyph.features, glyph.metrics, glyph.mark);
        if result.character.is_some() {
            read += occurrences;
            distance_sum += u64::from(result.distance) * occurrences;
        } else {
            unread += occurrences;
        }
    }

    Ok(Fit {
        score: Fit::score(read, unread, distance_sum, thresholds.max_distance()),
        name,
        matched: read,
        unmatched: unread,
        distinct: counts.len() as u64,
        distance_sum,
    })
}

/// Score every candidate and rank them, best first.
///
/// A set that cannot be used at all — generated for another grid size — is dropped rather than
/// failing the run, because a directory of candidates is a thing a user accumulates and one stale
/// file in it should not stop the other fifty being considered. The count of what was dropped is
/// the caller's to report.
///
/// # Errors
/// Does not fail on an unusable candidate; propagates nothing today, and returns [`Result`] so that
/// a future candidate source which can fail does not change the signature.
pub fn rank(
    survey: &GlyphSurvey,
    candidates: Vec<ReferenceSet>,
    thresholds: MatchThresholds,
) -> Result<(Vec<Fit>, usize)> {
    rank_watched(survey, candidates, thresholds, &Silent)
}

/// Rank candidates, reporting how far through them the scoring has got.
///
/// Identical to [`rank`] but for the observer. Determinate throughout: a directory of candidate
/// sets is counted before the first one is scored.
///
/// # Errors
/// As [`rank`].
pub fn rank_watched(
    survey: &GlyphSurvey,
    candidates: Vec<ReferenceSet>,
    thresholds: MatchThresholds,
    progress: &dyn Progress,
) -> Result<(Vec<Fit>, usize)> {
    let total = candidates.len();
    progress.begin(Phase::Score, Some(total.try_into().unwrap_or(u64::MAX)));
    let mut scanned = 0u64;
    let mut fits: Vec<Fit> = candidates
        .into_iter()
        .filter_map(|set| {
            let fit = score_set(survey, set, thresholds).ok();
            scanned += 1;
            progress.advance(scanned);
            fit
        })
        .collect();
    progress.end();

    // Ties break on the name so a run over the same directory is reproducible. A fitter whose
    // answer moved between runs of the same file would be untestable, which is the reason
    // `cluster::by_frequency` orders the way it does.
    fits.sort_by(|a, b| {
        a.score
            .total_cmp(&b.score)
            .then_with(|| a.name.cmp(&b.name))
    });
    let unusable = total - fits.len();
    Ok((fits, unusable))
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::{FeatureVector, LineMetrics, MarkSlope, Rect};
    use subtrackt_glyph::reference::{ReferenceEntry, Style};

    use crate::survey::GlyphRecord;

    fn vector(bits: &[usize]) -> FeatureVector {
        let mut v = FeatureVector::EMPTY;
        for bit in bits {
            v.set(*bit);
        }
        v
    }

    fn record(bits: &[usize]) -> GlyphRecord {
        GlyphRecord {
            cue: 0,
            line: 0,
            bounds: Rect::new(0, 0, 8, 12),
            features: vector(bits),
            metrics: LineMetrics::UNKNOWN,
            mark: MarkSlope::NONE,
        }
    }

    fn survey_of(glyphs: Vec<GlyphRecord>) -> GlyphSurvey {
        GlyphSurvey {
            stream: subtrackt_demux::StreamInfo {
                index: 0,
                codec: subtrackt_demux::BitmapCodec::Pgs,
                language: None,
                title: None,
                plane_width: 1920,
                plane_height: 1080,
                codec_private: Vec::new(),
            },
            cues: 1,
            span: None,
            glyphs,
        }
    }

    fn set(name: &str, entries: &[(char, &[usize])]) -> ReferenceSet {
        ReferenceSet::new(
            name,
            entries
                .iter()
                .map(|(character, bits)| ReferenceEntry {
                    character: *character,
                    style: Style::Regular,
                    features: vector(bits),
                    metrics: LineMetrics::UNKNOWN,
                    mark: MarkSlope::NONE,
                })
                .collect(),
        )
    }

    #[test]
    fn a_set_that_matches_everything_exactly_scores_zero() {
        let survey = survey_of(vec![record(&[1, 2]), record(&[3, 4])]);
        let fit = score_set(
            &survey,
            set("exact", &[('a', &[1, 2]), ('b', &[3, 4])]),
            MatchThresholds::default(),
        )
        .unwrap();

        assert_eq!(fit.matched, 2);
        assert_eq!(fit.unmatched, 0);
        assert!(fit.score.abs() < f32::EPSILON, "scored {}", fit.score);
    }

    #[test]
    fn an_empty_set_matches_nothing_and_scores_the_ceiling() {
        // The state the binary ships in. It has to score worst rather than best, and an empty set
        // matching nothing means its mean-over-matched is *zero* — which is the clearest possible
        // demonstration of why that statistic cannot rank candidates.
        let survey = survey_of(vec![record(&[1, 2])]);
        let thresholds = MatchThresholds::default();
        let fit = score_set(&survey, ReferenceSet::empty(), thresholds).unwrap();

        assert_eq!(fit.matched, 0);
        assert_eq!(fit.unmatched, 1);
        #[allow(clippy::cast_precision_loss)]
        let ceiling = thresholds.max_distance() as f32;
        assert!((fit.score - ceiling).abs() < f32::EPSILON, "scored {}", fit.score);
    }

    #[test]
    fn a_set_that_reads_a_little_at_close_range_loses_to_one_that_reads_everything() {
        // The measured hazard, as a test. `docs/reference-set.md` records a symbol face winning
        // Georgia-rendered material on mean-over-matched and reading it at 79.8%. Here `narrow`
        // matches one glyph of four perfectly and `broad` matches all four at some distance; the
        // ranking must prefer `broad`.
        let survey = survey_of(vec![
            record(&[1]),
            record(&[2, 3]),
            record(&[4, 5]),
            record(&[6, 7]),
        ]);
        let narrow = set("narrow", &[('a', &[1])]);
        let broad = set(
            "broad",
            &[
                ('a', &[1, 9]),
                ('b', &[2, 3, 9]),
                ('c', &[4, 5, 9]),
                ('d', &[6, 7, 9]),
            ],
        );

        let (ranked, unusable) =
            rank(&survey, vec![narrow, broad], MatchThresholds::default()).unwrap();
        assert_eq!(unusable, 0);
        assert_eq!(ranked[0].name, "broad", "ranked {ranked:?}");
        assert!((ranked[0].coverage() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn repeated_glyphs_are_scanned_once_and_counted_every_time() {
        // The economy the whole fit depends on: a film's twenty thousand glyphs are a few hundred
        // shapes. Coverage has to reflect the occurrences even though the scan sees the shapes.
        let survey = survey_of(vec![record(&[1, 2]), record(&[1, 2]), record(&[1, 2])]);
        let fit = score_set(&survey, set("exact", &[('a', &[1, 2])]), MatchThresholds::default())
            .unwrap();

        assert_eq!(fit.distinct, 1, "one shape");
        assert_eq!(fit.matched, 3, "three occurrences");
    }

    #[test]
    fn ranking_is_reproducible_when_two_candidates_score_alike() {
        let survey = survey_of(vec![record(&[1, 2])]);
        let entries: &[(char, &[usize])] = &[('a', &[1, 2])];
        let (ranked, _) = rank(
            &survey,
            vec![set("zebra", entries), set("alpha", entries)],
            MatchThresholds::default(),
        )
        .unwrap();
        assert_eq!(ranked[0].name, "alpha", "ties break on the name, not on input order");
    }
}
