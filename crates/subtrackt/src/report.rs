//! What an extraction run has to say for itself.
//!
//! §4 of the architecture document notes that per-cue confidence has nowhere to live in Sovereign's
//! `ExtractedSubtitle`. Whatever the answer to that turns out to be (#13), the extractor has to
//! produce the numbers before anything can store them.

use std::fmt;

use subtrackt_core::Confidence;

use crate::config::UnmatchedPolicy;

/// Counters and the gate decision for one extracted track.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Codec packets read.
    pub packets: u64,
    /// Subtitle images decoded.
    pub images: u64,
    /// Glyphs segmented.
    pub glyphs: u64,
    /// Glyphs identified within threshold.
    pub matched: u64,
    /// Glyphs with no reference within threshold.
    pub unmatched: u64,
    /// Glyphs matched but with a runner-up too close to call.
    pub ambiguous: u64,
    /// Glyphs answered from the session cache.
    pub cache_hits: u64,
    /// Glyphs whose line was too short or too uniform to measure against.
    ///
    /// These fall back to shape alone, which is how the pipeline behaved before #37. A large count
    /// means the material is mostly short lines — or that the anchors have stopped being found, and
    /// the feature is quietly doing nothing.
    pub glyphs_without_metrics: u64,
    /// Distinct glyph shapes the stream contained.
    ///
    /// Tens of thousands of glyphs reduce to a few hundred shapes, which is what makes clustering
    /// them cheap. A count close to the glyph count means normalisation has stopped collapsing
    /// repeat renderings onto the same vector, and everything downstream gets more expensive and
    /// less accurate at once.
    pub distinct_shapes: u64,
    /// Groups those shapes formed, each matched against the reference set exactly once.
    ///
    /// Compare with the character count of the material: far more clusters than distinct
    /// characters means the radius is too tight to absorb the stream's own variation, and far
    /// fewer means it is loose enough to be merging different characters together.
    pub clusters: u64,
    /// Cues written.
    pub cues: u64,
    /// Cues dropped under [`UnmatchedPolicy::Drop`].
    pub cues_dropped: u64,
    /// Characters post-correction substituted.
    ///
    /// Watch it against `ambiguous`: the corrector is only allowed to touch a glyph the matcher
    /// flagged, so this can never exceed that count, and a ratio anywhere near 1 would mean the
    /// rules had stopped refusing anything.
    pub corrections: u64,
    /// Name of the corrector that ran, or an empty string for a run that never assembled a cue.
    pub corrector: &'static str,
    /// Name of the reference set used, so a bad extraction can be traced to its data.
    pub reference_set: String,
}

impl Report {
    /// Fold a cue's tally into the totals.
    pub fn record(&mut self, confidence: Confidence) {
        self.glyphs += u64::from(confidence.total());
        self.matched += u64::from(confidence.matched);
        self.unmatched += u64::from(confidence.unmatched);
        self.ambiguous += u64::from(confidence.ambiguous);
    }

    /// The track-level tally, saturating at [`u32::MAX`] for a track longer than any real film.
    #[must_use]
    pub fn confidence(&self) -> Confidence {
        let cast = |v: u64| u32::try_from(v).unwrap_or(u32::MAX);
        Confidence {
            matched: cast(self.matched),
            unmatched: cast(self.unmatched),
            ambiguous: cast(self.ambiguous),
        }
    }

    /// Whether the configured policy abandons this track.
    #[must_use]
    pub fn is_rejected_by(&self, policy: UnmatchedPolicy) -> bool {
        policy.rejects(self.confidence())
    }

    /// Session cache hit rate in `0.0..=1.0`.
    #[must_use]
    pub fn cache_hit_rate(&self) -> f32 {
        if self.glyphs == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.cache_hits as f32 / self.glyphs as f32
        }
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} cues from {} images ({} packets); glyphs {} matched / {} unmatched / {} ambiguous; \
             cache {:.0}%; corrections {} ({})",
            self.cues,
            self.images,
            self.packets,
            self.matched,
            self.unmatched,
            self.ambiguous,
            self.cache_hit_rate() * 100.0,
            self.corrections,
            // Named even when it is `none`, because "post-correction was off" and "post-correction
            // ran and changed nothing" are different facts about a track and a summary that
            // printed `0` for both would hide the difference.
            self.corrector,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_cues_accumulates_the_track_tally() {
        let mut report = Report::default();
        report.record(Confidence { matched: 10, unmatched: 1, ambiguous: 2 });
        report.record(Confidence { matched: 5, unmatched: 0, ambiguous: 0 });

        assert_eq!(report.glyphs, 16);
        assert_eq!(report.confidence().matched, 15);
        assert_eq!(report.confidence().unmatched, 1);
        assert_eq!(report.confidence().ambiguous, 2);
    }

    #[test]
    fn the_gate_reads_the_accumulated_tally() {
        let mut report = Report::default();
        report.record(Confidence { matched: 99, unmatched: 1, ambiguous: 0 });
        assert!(report.is_rejected_by(UnmatchedPolicy::FailTrack));
        assert!(!report.is_rejected_by(UnmatchedPolicy::Threshold { min_ratio: 0.5 }));
    }

    #[test]
    fn an_empty_run_reports_a_zero_hit_rate_rather_than_dividing_by_zero() {
        assert!((Report::default().cache_hit_rate() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_summary_line_names_the_numbers_that_matter() {
        let mut report = Report {
            cues: 3,
            images: 3,
            packets: 12,
            corrections: 2,
            corrector: "context",
            ..Report::default()
        };
        report.record(Confidence { matched: 40, unmatched: 2, ambiguous: 1 });
        report.cache_hits = 21;

        let line = report.to_string();
        assert!(line.contains("3 cues"), "{line}");
        assert!(line.contains("40 matched / 2 unmatched / 1 ambiguous"), "{line}");
        assert!(line.contains("cache 50%"), "{line}");
        assert!(line.contains("corrections 2 (context)"), "{line}");
    }

    #[test]
    fn the_summary_tells_a_corrector_that_did_nothing_from_one_that_never_ran() {
        // Both report zero corrections and they mean different things, so the name has to be
        // there: one track was left alone on purpose, the other was examined and found clean.
        let off = Report { corrector: "none", ..Report::default() }.to_string();
        let on = Report { corrector: "context", ..Report::default() }.to_string();
        assert!(off.contains("corrections 0 (none)"), "{off}");
        assert!(on.contains("corrections 0 (context)"), "{on}");
    }
}
