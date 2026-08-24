//! What an extraction run has to say for itself.
//!
//! §4 of the architecture document notes that per-cue confidence has nowhere to live in Sovereign's
//! `ExtractedSubtitle`. Whatever the answer to that turns out to be (#13), the extractor has to
//! produce the numbers before anything can store them.

use std::fmt;
use std::time::Duration;

use subtrackt_core::Confidence;

use crate::config::UnmatchedPolicy;

/// What the line-metrics estimate did, line by line rather than glyph by glyph.
///
/// #184. `glyphs_without_metrics` says how much of a track is matched on shape alone and nothing
/// said *why*: every guard in `metrics::anchors` returned the same `None`, so a track reporting one
/// glyph in seven unmeasured could not say whether one rule had declined every time or six had
/// declined once each. They want opposite work — a line of two glyphs has no evidence to find,
/// while an all-capitals line has evidence and no way to read it — so the counts are what decide
/// which is worth doing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineCensus {
    /// Lines whose baseline and cap height were found.
    pub measured: u64,
    /// Refused for having fewer glyphs than the estimate needs.
    pub too_few_glyphs: u64,
    /// Refused for a band of no height.
    pub flat_band: u64,
    /// Refused because the glyph bottoms agree on no row.
    pub no_baseline: u64,
    /// Refused because nothing stands on the baseline that was found.
    pub nothing_on_the_baseline: u64,
    /// Refused because every glyph standing on the baseline is the same height.
    ///
    /// The all-capitals line, and the one refusal with evidence available elsewhere: `NO ONE SAW`
    /// and `no one saw` are indistinguishable *to the line itself*, and not to a line that knows
    /// what scale the track is drawn at. It is also by far the largest — 420 of King Kong's 451.
    pub one_height: u64,
    /// Refused because no row carried enough support to be the cap line.
    pub no_cap_line: u64,
    /// Of the refusals, how many stood in an image where another line measured.
    ///
    /// The number that says whether a fallback has anything to fall back *to*. A refusing line
    /// alone in its image has no sibling to borrow a scale from.
    pub refused_with_a_measured_sibling: u64,
    /// Lines that refused and were then measured against the scale the rest of the track is drawn
    /// at.
    ///
    /// An approximation, and counted for the reason `CLAUDE.md` §Failing gives: a reader has to be
    /// able to tell a track measured from its own ink from one carrying a borrowed scale. Only the
    /// one-height refusal is reachable this way — see `pipeline::borrow_a_track_scale`.
    pub borrowed_a_track_scale: u64,
}

impl LineCensus {
    /// A census that has seen nothing, in a form a `const fn` can start from.
    pub const EMPTY: Self = Self {
        measured: 0,
        too_few_glyphs: 0,
        flat_band: 0,
        no_baseline: 0,
        nothing_on_the_baseline: 0,
        one_height: 0,
        no_cap_line: 0,
        refused_with_a_measured_sibling: 0,
        borrowed_a_track_scale: 0,
    };

    /// Every line that refused, whatever refused it.
    #[must_use]
    pub const fn refused(&self) -> u64 {
        self.too_few_glyphs
            + self.flat_band
            + self.no_baseline
            + self.nothing_on_the_baseline
            + self.one_height
            + self.no_cap_line
    }

    /// Every line the estimate was asked about.
    #[must_use]
    pub const fn seen(&self) -> u64 {
        self.measured + self.refused()
    }

    /// The refusals, largest first, as name and count, dropping the ones that never fired.
    #[must_use]
    pub fn reasons(&self) -> Vec<(&'static str, u64)> {
        let mut rows = vec![
            ("too few glyphs", self.too_few_glyphs),
            ("flat band", self.flat_band),
            ("no baseline", self.no_baseline),
            ("nothing on the baseline", self.nothing_on_the_baseline),
            ("one height", self.one_height),
            ("no cap line", self.no_cap_line),
        ];
        rows.retain(|(_, count)| *count > 0);
        rows.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        rows
    }
}

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
    /// Times the session cache was consulted.
    ///
    /// Not the glyph count, which is what [`Self::cache_hit_rate`] used to divide by. #106's
    /// de-fusing matches the parts of a component it is trying to cut, and those lookups do not
    /// become glyphs when the cut is rejected — so on a track with many unreadable components the
    /// hits outnumbered the glyphs and the rate came back above 100%.
    pub cache_lookups: u64,
    /// Sum of the Hamming distances of every glyph that matched.
    ///
    /// Coverage says how many glyphs found *a* reference; this says how well they fitted it, and
    /// the two answer different questions. A reference set built from the wrong typeface still
    /// matches nearly everything, just further away — so a mean rising towards the threshold is the
    /// signal that a track is being read confidently and wrongly, which coverage alone cannot see.
    pub distance_sum: u64,
    /// Glyphs whose line was too short or too uniform to measure against.
    ///
    /// These fall back to shape alone, which is how the pipeline behaved before #37. A large count
    /// means the material is mostly short lines — or that the anchors have stopped being found, and
    /// the feature is quietly doing nothing.
    pub glyphs_without_metrics: u64,
    /// The same question asked per line, and answered with which rule declined.
    ///
    /// #184. `glyphs_without_metrics` counts the consequence; this says the cause.
    pub lines: LineCensus,
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
    /// Images in which at least one unreadable component was recovered as two characters.
    ///
    /// #106. Watch it against `unmatched`: this stage can only move a glyph from unread to read, so
    /// a rising count with a flat unmatched count would mean it had started firing on something
    /// other than a fusion.
    pub defused: u64,
    /// Cues whose end time the stream never gave, and the decoder had to invent.
    ///
    /// Approximating is allowed here — the alternative is losing a line of dialogue outright — on
    /// the condition `CLAUDE.md` attaches to it: the approximation is counted, so a reader can tell
    /// a track whose timing is entirely the stream's from one carrying a guess. Both decoders had
    /// counted it since the day they made it; until #147 nothing could read the count, because
    /// `decoder_for` hands back a trait object and the count was an inherent method.
    ///
    /// Essentially always zero. A non-zero figure on a whole film is one trailing cue; a large one
    /// means the stream is not being framed the way the decoder thinks it is.
    pub unterminated_cues: u64,
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
    /// Distinct clear tokens the track's own vocabulary learned, or zero when it did not run.
    ///
    /// Reported because "the rule never fired" and "the rule fired and gained nothing" are
    /// different results, and only this number tells them apart.
    pub vocabulary_tokens: u64,
    /// How many of those came from the track's own vocabulary rather than from context.
    ///
    /// Split out because the two arms rest on different evidence, and a summary that added them
    /// together would hide which one a bad substitution came from. Always zero unless
    /// `Config::track_vocabulary` is on.
    pub vocabulary_corrections: u64,
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

    /// Mean distance of the glyphs that matched, or zero when none did.
    ///
    /// Read it against [`MatchThresholds::max_distance`](subtrackt_glyph::matcher::MatchThresholds::max_distance):
    /// a mean well below the ceiling means the reference set fits the material, and a mean pressed
    /// up against it means glyphs are being accepted because the threshold is generous rather than
    /// because they resemble anything.
    #[must_use]
    pub fn mean_match_distance(&self) -> f32 {
        if self.matched == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.distance_sum as f32 / self.matched as f32
        }
    }

    /// Share of glyphs whose line could not be measured, in `0.0..=1.0`.
    ///
    /// The denominator is every glyph the run segmented. See
    /// [`Self::glyphs_without_metrics`](Self#structfield.glyphs_without_metrics) for what falls
    /// back when it cannot be measured, and `docs/glyph-hit-list.md` for what that costs.
    #[must_use]
    pub fn unmeasured_line_share(&self) -> f32 {
        if self.glyphs == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_possible_truncation)]
        {
            #[allow(clippy::cast_precision_loss)]
            {
                (self.glyphs_without_metrics as f64 / self.glyphs as f64) as f32
            }
        }
    }

    /// Session cache hit rate in `0.0..=1.0`.
    #[must_use]
    pub fn cache_hit_rate(&self) -> f32 {
        if self.cache_lookups == 0 {
            return 0.0;
        }
        // In `f64` for the reason `Confidence::ratio` gives: both operands routinely pass what a
        // narrower type holds exactly, and only the quotient needs to be `f32`.
        #[allow(clippy::cast_possible_truncation)]
        {
            #[allow(clippy::cast_precision_loss)]
            {
                (self.cache_hits as f64 / self.cache_lookups as f64) as f32
            }
        }
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} cues from {} images ({} packets); glyphs {} matched / {} unmatched / {} ambiguous \
             ({:.1}% read); fit {:.1}; unmeasured lines {:.0}%; cache {:.0}%;{}{} \
             corrections {}{} ({})",
            self.cues,
            self.images,
            self.packets,
            self.matched,
            self.unmatched,
            self.ambiguous,
            // The figure the gate actually reads, spelled out rather than left to be divided out
            // of the two counts before it.
            self.confidence().ratio() * 100.0,
            self.mean_match_distance(),
            // What share of glyphs stood on a line whose baseline and cap height could not be
            // found. #37's term is off for every one of those, so an `o` and an `O` are compared on
            // shape alone and cannot be told apart — which is #118's case-pair family. The counter
            // existed from the day the feature did and was never printed, so nobody could watch it.
            self.unmeasured_line_share() * 100.0,
            self.cache_hit_rate() * 100.0,
            // Silent when nothing fused, so an ordinary run reads exactly as it did before #106.
            // Named when it did, because a recovered fusion is two characters that would otherwise
            // have been a placeholder and a hole, and that belongs in the one line a caller reads.
            if self.defused > 0 {
                format!(" defused {};", self.defused)
            } else {
                String::new()
            },
            // Silent at zero, like `defused` above: an ordinary track invents nothing and should
            // read exactly as it did before this counter existed. Named the moment it does, because
            // an invented end time is the one approximation the pipeline is allowed to make.
            if self.unterminated_cues > 0 {
                format!(" invented timing on {} cue(s);", self.unterminated_cues)
            } else {
                String::new()
            },
            self.corrections,
            // The two arms rest on different evidence, so the split is shown whenever the second
            // one fired. Silent when it did not, so an ordinary run reads exactly as before.
            if self.vocabulary_corrections > 0 {
                format!(
                    " (context {}, vocabulary {})",
                    self.corrections - self.vocabulary_corrections,
                    self.vocabulary_corrections
                )
            } else {
                String::new()
            },
            // Named even when it is `none`, because "post-correction was off" and "post-correction
            // ran and changed nothing" are different facts about a track and a summary that
            // printed `0` for both would hide the difference.
            self.corrector,
        )
    }
}

/// What one run cost, as opposed to what it read.
///
/// Deliberately **not** part of [`Report`], and the separation is load-bearing rather than tidy.
/// `Report` is the tally of what came out of a track, and [`crate::provenance::note`] writes parts
/// of it into the extracted file — so a duration anywhere near it is one careless change away from
/// making an artefact's bytes depend on how fast the machine was that produced it. A cost is a
/// property of the *run*; a tally is a property of the *track*. Only one of those belongs in a
/// subtitle file.
///
/// **Nothing here is a process measurement.** `unsafe_code` is forbidden across the workspace and
/// no OS memory API is called; the byte counts are the pipeline's own accounting of what it is
/// holding. That is the decomposition worth having anyway — knowing a run peaked at 900 MB says
/// less than knowing 870 MB of it was decoded bitmaps. True process peak is the bench harness's
/// job, measured from outside.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cost {
    /// Streaming packets out of the container and decoding them into bitmaps.
    pub decode: Duration,
    /// Cutting every decoded bitmap into glyphs.
    pub segment: Duration,
    /// Grouping the stream's own shapes, before any of them is matched.
    pub cluster: Duration,
    /// Matching glyphs and assembling them into cues.
    pub read: Duration,
    /// Decoded bitmap bytes held at once.
    ///
    /// The pipeline segments nothing until every packet has been decoded, so this is the whole
    /// track's pixel data resident simultaneously rather than a running figure. It is the number
    /// that decides whether that is worth changing.
    pub image_bytes: u64,
    /// Glyph bytes held at once, counting each glyph where it is stored.
    ///
    /// Larger than one copy of the glyphs: they are held per image *and* flattened into a second
    /// contiguous list for the grouping pass, so this counts both.
    pub glyph_bytes: u64,
}

impl Cost {
    /// Wall clock across every phase.
    #[must_use]
    pub fn total(&self) -> Duration {
        self.decode + self.segment + self.cluster + self.read
    }
}

impl fmt::Display for Cost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mib = |bytes: u64| {
            #[allow(clippy::cast_precision_loss)]
            {
                bytes as f64 / (1024.0 * 1024.0)
            }
        };
        write!(
            f,
            "decode {:.1}s; segment {:.1}s; cluster {:.1}s; read {:.1}s; total {:.1}s; \
             resident {:.1} MiB images / {:.1} MiB glyphs",
            self.decode.as_secs_f64(),
            self.segment.as_secs_f64(),
            self.cluster.as_secs_f64(),
            self.read.as_secs_f64(),
            self.total().as_secs_f64(),
            mib(self.image_bytes),
            mib(self.glyph_bytes),
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
        assert!((Report::default().mean_match_distance() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn how_well_glyphs_matched_is_reported_separately_from_how_many_did() {
        // `docs/reference-set.md` measured two sets matching the same 93.9% of glyphs and reading
        // 2.4 times differently, so coverage cannot stand in for fit. These are two numbers.
        let mut close = Report::default();
        close.record(Confidence { matched: 10, unmatched: 0, ambiguous: 0 });
        close.distance_sum = 130;

        let mut far = Report::default();
        far.record(Confidence { matched: 10, unmatched: 0, ambiguous: 0 });
        far.distance_sum = 227;

        assert!((close.confidence().ratio() - far.confidence().ratio()).abs() < f32::EPSILON);
        assert!((close.mean_match_distance() - 13.0).abs() < 1e-6);
        assert!((far.mean_match_distance() - 22.7).abs() < 1e-4);
    }

    #[test]
    fn unmatched_glyphs_do_not_drag_the_mean_distance_down() {
        // A glyph that matched nothing has no distance to a reference, only a best-effort figure
        // for diagnostics. Averaging it in would make a badly-read track look like a close fit.
        let mut report = Report::default();
        report.record(Confidence { matched: 2, unmatched: 8, ambiguous: 0 });
        report.distance_sum = 40;
        assert!((report.mean_match_distance() - 20.0).abs() < 1e-6);
    }

    #[test]
    fn a_hit_rate_cannot_exceed_one_however_many_lookups_missed_a_glyph() {
        // #140 found `cache 101%` on a real VOBSUB track. The numerator counted every consultation
        // of the session cache and the denominator counted *glyphs*, and #106's de-fusing consults
        // it for the parts of a component it is trying to cut -- lookups that never become glyphs
        // when the cut is rejected. A rate above one is a counter disagreeing with itself.
        let mut report = Report::default();
        report.record(Confidence { matched: 100, unmatched: 0, ambiguous: 0 });
        report.cache_hits = 130;
        report.cache_lookups = 140;

        assert!(report.cache_hit_rate() <= 1.0, "{}", report.cache_hit_rate());
        assert!((report.cache_hit_rate() - 130.0 / 140.0).abs() < 1e-6);
    }

    #[test]
    fn a_run_that_never_consulted_the_cache_reports_no_rate_rather_than_a_division() {
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
        report.cache_lookups = 42;
        report.glyphs_without_metrics = 6;

        let line = report.to_string();
        assert!(line.contains("3 cues"), "{line}");
        assert!(line.contains("40 matched / 2 unmatched / 1 ambiguous"), "{line}");
        assert!(line.contains("cache 50%"), "{line}");
        // #118: the share of glyphs matched without #37's term. It was counted from the day the
        // feature existed and printed nowhere, so a track reading badly because its lines could not
        // be measured looked exactly like one reading badly for any other reason. King Kong is 14%.
        assert!(line.contains("unmeasured lines 14%"), "{line}");
        assert!(line.contains("fit 0.0"), "{line}");
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
