//! Wiring the stages together.
//!
//! The control flow here is complete: open, select a stream, decode packets, segment, match,
//! assemble, gate, write. The stages it calls into are at varying stages of completion, so a run
//! against a real file currently stops at the first unimplemented one and says which issue tracks
//! it. That is the point of building it this way round — the shape of the pipeline is settled and
//! testable before the expensive parts exist, and each stage can be dropped in without touching
//! the others.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use subtrackt_core::progress::{Phase, Progress, Silent};
use subtrackt_core::{
    Error, GlyphMatcher, LineMetrics, MarkSlope, Rect, Result, Segmenter, SubtitleImage, TextTrack,
};
use subtrackt_demux::{StreamInfo, SubtitleSource};
use subtrackt_glyph::binarize::{Binarizer, BinaryMask};
use subtrackt_glyph::feature;
use subtrackt_glyph::matcher::HammingMatcher;
use subtrackt_glyph::reference;
use subtrackt_glyph::split::{self, SplitRules};
use subtrackt_text::correct::{ContextCorrector, CorrectionLog, NoopCorrector, PostCorrector};
use subtrackt_text::format::writer_with_provenance;
use subtrackt_text::layout::SpatialAssembler;

use crate::config::{Config, UnmatchedPolicy};
use crate::report::{Cost, Report};

/// The result of a run.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// The extracted track.
    pub track: TextTrack,
    /// Counters and the gate decision.
    pub report: Report,
    /// Every substitution post-correction made, in cue order.
    ///
    /// Empty unless [`Config::post_correct`] was set. It is carried out of the run rather than
    /// merely counted because a correction that leaves no trace is the failure mode the whole
    /// stage is built to avoid: a caller has to be able to see what was rewritten, not just how
    /// often.
    pub corrections: Vec<CorrectionLog>,
    /// Every glyph the matcher would not name, in cue order.
    ///
    /// Counted in [`Report::unmatched`] as well; this says *which*. #98 is the reason it is carried
    /// rather than merely tallied, and the argument is the one `corrections` already makes: the
    /// project's whole thesis is that an unmatched glyph is a **fact** rather than a confidence
    /// score, and a fact a caller cannot inspect is doing only half its job. A user whose reference
    /// set is missing a character learns which one from here and from nowhere else.
    ///
    /// Always collected, with no flag to switch it off, because an unread glyph is by construction
    /// rare — a few percent of a track — and a feature film's worth is tens of kilobytes against the
    /// tens of thousands of glyphs the run already holds resident.
    pub unread: Vec<UnreadGlyph>,
    /// The stream that was read.
    pub stream: StreamInfo,
    /// What the run cost, phase by phase.
    ///
    /// Separate from [`Self::report`] on purpose — see [`Cost`] for why a duration must not live
    /// beside the numbers that reach a written file.
    pub cost: Cost,
}

/// One glyph that matched nothing, and everything known about it that is not its shape.
///
/// Deliberately not the [`FeatureVector`](subtrackt_core::FeatureVector). The vector is a lossy
/// 16x16 projection and says nothing a reader can act on; what a reader needs is where the glyph
/// sat, how big it was, and whether its line could be measured at all — because a glyph on an
/// unmeasurable line was matched on shape alone and failed for a different reason than one that had
/// metrics and still found nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnreadGlyph {
    /// Which cue it came from, counting from zero.
    pub cue: usize,
    /// Which text line within that cue.
    pub line: usize,
    /// Where it sat, in subtitle-plane coordinates.
    pub bounds: Rect,
    /// Where it stood in its line, or [`LineMetrics::UNKNOWN`] if the line was unmeasurable.
    pub metrics: LineMetrics,
    /// How far the nearest reference entry was — the one that was still too far to accept.
    ///
    /// A glyph rejected at 52 cells against a 51-cell ceiling is a different problem from one
    /// rejected at 140, and the count alone cannot tell them apart.
    pub distance: u32,
}

impl Outcome {
    /// Serialise the track in the configured format.
    ///
    /// # Errors
    /// Propagates writer failures.
    pub fn render(&self, config: &Config) -> Result<String> {
        self.render_dated(config, crate::provenance::today_utc())
    }

    /// Serialise the track, with the date the note should carry supplied rather than read.
    ///
    /// [`Self::render`] reads the clock, which makes it the one function in this crate whose output
    /// is not a function of its inputs. That is unavoidable — #129 asks for the date — but it is
    /// containable, and this is the containment: a caller that needs the same bytes twice, and
    /// every test that compares output, goes through here.
    ///
    /// # Errors
    /// Propagates writer failures.
    pub fn render_dated(&self, config: &Config, today: (i64, u32, u32)) -> Result<String> {
        let note = config
            .provenance
            .writes(config.format)
            .then(|| crate::provenance::note(&self.report, today));
        writer_with_provenance(config.format, note).to_string(&self.track)
    }
}

/// Runs the extraction pipeline.
pub struct Pipeline {
    config: Config,
    /// Overrides the embedded set. Nothing is embedded — `docs/reference-set.md` measured a
    /// shipped set as worse than none — so supplying one from a file is the only way to match
    /// anything at all.
    reference: Option<subtrackt_glyph::ReferenceSet>,
}

impl Pipeline {
    /// Build a pipeline.
    #[must_use]
    pub const fn new(config: Config) -> Self {
        Self { config, reference: None }
    }

    /// Use `reference` instead of the embedded set.
    #[must_use]
    pub fn with_reference(mut self, reference: subtrackt_glyph::ReferenceSet) -> Self {
        self.reference = Some(reference);
        self
    }

    /// The reference set this pipeline will match against.
    #[must_use]
    pub fn reference(&self) -> subtrackt_glyph::ReferenceSet {
        self.reference.clone().unwrap_or_else(reference::embedded)
    }

    /// The configuration in force.
    #[must_use]
    pub const fn config(&self) -> &Config {
        &self.config
    }

    /// List the bitmap subtitle streams in a file without decoding anything.
    ///
    /// # Errors
    /// Propagates demux failures.
    pub fn list(path: impl AsRef<Path>) -> Result<Vec<StreamInfo>> {
        Ok(subtrackt_demux::open(path)?.streams().to_vec())
    }

    /// Extract a track from a file.
    ///
    /// # Errors
    /// Propagates any stage failure. Notably, if the configured [`UnmatchedPolicy`] rejects the
    /// track, this returns [`Error::TrackRejected`] rather than a partial track — the caller is
    /// expected to fall back to burn-in rather than to ship half a subtitle. Every counter behind
    /// that decision is in [`Report`], so the caller can log why it happened.
    pub fn run(&self, path: impl AsRef<Path>) -> Result<Outcome> {
        self.run_watched(path, &Silent)
    }

    /// Extract a track, reporting where the run has got to.
    ///
    /// Identical to [`Self::run`] but for the observer. A film is minutes of work in two shapes --
    /// packets arriving with no total in sight, then every decoded image segmented, clustered and
    /// matched -- and a front end that says nothing across either is indistinguishable from a hung
    /// one. What gets drawn, and whether anything does, belongs entirely to `progress`.
    ///
    /// # Errors
    /// As [`Self::run`].
    pub fn run_watched(&self, path: impl AsRef<Path>, progress: &dyn Progress) -> Result<Outcome> {
        let path = path.as_ref();
        let mut source = subtrackt_demux::open(path)?;

        let stream = self.choose_stream(source.as_ref())?;
        source.select(stream.index)?;

        let mut decoder = subtrackt_decode::decoder_for(stream.codec.ffmpeg_name())?;
        // VOBSUB carries its palette out of band; without this a subpicture has colour indices
        // and no colours.
        decoder.configure(&stream.codec_private)?;
        let mut matcher = HammingMatcher::new(self.reference(), self.config.matching)?
            .with_cluster_rules(self.config.clustering);
        let segmenter = ImageSegmenter::new(
            Binarizer::new(self.config.binarize),
            self.config.grey_coverage,
            !carries_a_slanted_cut(matcher.references()),
        );
        let assembler = SpatialAssembler::new(self.config.layout_rules());

        let mut report = Report {
            reference_set: matcher.references().name().to_owned(),
            ..Report::default()
        };
        let mut images = Vec::new();

        // Indeterminate: the source is streamed until it is exhausted, so how many packets there
        // are is not known until there are none left.
        let mut cost = Cost::default();
        let started = Instant::now();
        progress.begin(Phase::Decode, None);
        while let Some(packet) = source.next_packet()? {
            report.packets += 1;
            progress.advance(report.packets);
            images.extend(decoder.push(packet.pts, &packet.payload)?);
        }
        images.extend(decoder.finish()?);
        progress.end();
        // Read after `finish`, which is where the trailing cue is invented, and before the decoder
        // is dropped. See `BitmapDecoder::unterminated_cues` for why this was unreachable until
        // #147: the count existed on both decoders and `decoder_for` erases the type that had it.
        report.unterminated_cues = decoder.unterminated_cues();
        cost.decode = started.elapsed();
        report.images = images.len().try_into().unwrap_or(u64::MAX);
        // Every decoded bitmap is resident at once, because nothing is segmented until the last
        // packet has arrived. Counted rather than estimated: #145 turns on how large this actually
        // is, and the answer is a property of the material rather than of the code.
        cost.image_bytes = images
            .iter()
            .map(|image| image.bitmap.pixels().len() as u64)
            .sum();

        let mut corrections = Vec::new();
        let mut unread = Vec::new();
        let mut stages = Stages {
            segmenter: &segmenter,
            matcher: &mut matcher,
            assembler: &assembler,
        };
        let mut tally = Tally {
            report: &mut report,
            corrections: &mut corrections,
            unread: &mut unread,
            cost: &mut cost,
        };
        let track = self.build_track(&images, &mut stages, &mut tally, progress)?;

        report.cues = track.cues.len().try_into().unwrap_or(u64::MAX);
        report.cache_hits = stages.matcher.cache_hits();

        if report.is_rejected_by(self.config.unmatched) {
            // Every number behind the decision, because the caller is being told to fall back to
            // burn-in and that is expensive enough to deserve a reason.
            return Err(Error::TrackRejected {
                policy: self.config.unmatched.name(),
                matched: report.matched,
                unmatched: report.unmatched,
                required: self.config.unmatched.required_ratio(),
            });
        }

        Ok(Outcome { track, report, corrections, unread, stream, cost })
    }

    /// The corrector this configuration asks for.
    ///
    /// Off is a corrector too, not an absent one. Keeping the switched-off case on the same code
    /// path means the reporting, the logging and the cue loop are identical either way, so nothing
    /// can behave differently for a reason other than the corrections themselves.
    fn corrector(&self) -> Box<dyn PostCorrector> {
        if self.config.post_correct {
            // One margin, from the matching thresholds, shared with the confidence tally the
            // assembler produced. A corrector working to a different one would rewrite glyphs the
            // report had already called clean.
            let corrector = ContextCorrector::new(self.config.matching.ambiguity_margin());
            // The vocabulary is an arm of this corrector, not a stage of its own — two correctors
            // in sequence could let one's output become the other's evidence, which is the cascade
            // `docs/post-correction.md` guarantees cannot happen.
            if self.config.track_vocabulary {
                Box::new(corrector.with_vocabulary(self.config.vocabulary))
            } else {
                Box::new(corrector)
            }
        } else {
            Box::new(NoopCorrector)
        }
    }

    /// Pick the configured stream, or the first one.
    pub(crate) fn choose_stream(&self, source: &dyn SubtitleSource) -> Result<StreamInfo> {
        let streams = source.streams();
        match self.config.stream {
            Some(index) => streams
                .iter()
                .find(|s| s.index == index)
                .cloned()
                .ok_or_else(|| Error::Demux(format!("no subtitle stream with index {index}"))),
            None => streams
                .first()
                .cloned()
                .ok_or_else(|| Error::Demux("no bitmap subtitle stream found".into())),
        }
    }

    /// Segment, match and assemble every image into cues, applying the unmatched-glyph policy.
    fn build_track(
        &self,
        images: &[SubtitleImage],
        stages: &mut Stages<'_>,
        tally: &mut Tally<'_>,
        progress: &dyn Progress,
    ) -> Result<TextTrack> {
        // Segment everything before matching anything. The matcher groups a stream's own shapes
        // and matches the groups, which it cannot do while answers are already being handed out —
        // see `subtrackt_glyph::cluster` for why identifying glyphs one at a time cannot work.
        let mut per_image: Vec<Vec<subtrackt_core::Glyph>> = Vec::with_capacity(images.len());
        // Three reported phases rather than one, because this is three passes over the same images
        // with a whole-stream barrier between them. One bar filling up three times under a single
        // label would say less than three that each name what is running.
        let total: u64 = images.len().try_into().unwrap_or(u64::MAX);
        let mut done = 0u64;
        let started = Instant::now();
        progress.begin(Phase::Segment, Some(total));
        for image in images {
            per_image.push(stages.segmenter.segment(image)?);
            done += 1;
            progress.advance(done);
        }
        progress.end();
        tally.cost.segment = started.elapsed();

        let all_glyphs: Vec<subtrackt_core::Glyph> = per_image
            .iter()
            .flat_map(|glyphs| glyphs.iter().cloned())
            .collect();
        // One call with no way to see inside it, so a spinner rather than a bar. It still earns
        // its line: on a feature film the grouping pass is seconds of otherwise silent work.
        let started = Instant::now();
        progress.begin(Phase::Cluster, None);
        stages.matcher.prepare(&all_glyphs)?;
        progress.end();
        tally.cost.cluster = started.elapsed();
        // Both copies, because both are resident at this moment: `per_image` owns every glyph and
        // `all_glyphs` is a second contiguous copy of the same ones. #144 row 3 is whether the
        // second is worth its bytes, and a figure counting one copy could not say.
        tally.cost.glyph_bytes =
            2 * all_glyphs.len() as u64 * std::mem::size_of::<subtrackt_core::Glyph>() as u64;
        tally.report.distinct_shapes = stages.matcher.distinct_shapes();
        tally.report.clusters = stages.matcher.clusters();
        tally.report.glyphs_without_metrics = all_glyphs
            .iter()
            .filter(|g| !g.metrics.known)
            .count()
            .try_into()
            .unwrap_or(u64::MAX);

        // Assemble every cue before correcting any of them. #60's vocabulary arm needs the whole
        // track's clear tokens, and a decision needing the whole track cannot be made while
        // answers are already being handed out — the same argument that makes `matcher.prepare` a
        // separate pass. Every image is already resident, so this costs nothing but the ordering.
        let read_cues = self.read_images(images, per_image, stages, tally, progress)?;

        let mut corrector = self.corrector();
        corrector.observe(&read_cues);

        let mut cues = Vec::with_capacity(read_cues.len());
        for (index, read) in read_cues.into_iter().enumerate() {
            let mut cue = read.cue;

            // Post-correction, before the policy runs. It can only exchange one ambiguous
            // character for another, so it moves no glyph between the matched and unmatched
            // tallies and the gate below decides on exactly the same numbers either way. The
            // cheap pre-check keeps the corrector away from cues that were read cleanly.
            if subtrackt_text::correct::has_correctable_glyphs(cue.confidence) {
                corrector.correct(&mut cue, &read.origins, index, tally.corrections);
            }

            // Per-cue policy. The track-level gate runs afterwards over the accumulated tally,
            // because "one unread glyph in a feature" and "40% of the track unread" deserve
            // different answers and only the second is visible at track level.
            if !cue.confidence.is_complete() && self.config.unmatched == UnmatchedPolicy::Drop {
                tally.report.cues_dropped += 1;
                continue;
            }
            cues.push(cue);
        }

        tally.report.corrections = tally.corrections.len().try_into().unwrap_or(u64::MAX);
        tally.report.vocabulary_corrections = tally
            .corrections
            .iter()
            .filter(|c| {
                matches!(c.rule, subtrackt_text::correct::CorrectionRule::Vocabulary { .. })
            })
            .count()
            .try_into()
            .unwrap_or(u64::MAX);
        tally.report.vocabulary_tokens = corrector.vocabulary_size().try_into().unwrap_or(u64::MAX);
        tally.report.corrector = corrector.name();
        Ok(TextTrack::new(cues, None))
    }

    /// Match, defuse and assemble every image, in the order a whole-stream matcher needs.
    ///
    /// Split out of [`Self::build_track`] because the two are different jobs at different
    /// altitudes: that one sequences the three passes and owns the phase boundaries, this one is
    /// the innermost loop of the run. #154 is what forced the split — adding a timing to each
    /// phase put the combined function past the length the lint allows, which is the lint working.
    fn read_images(
        &self,
        images: &[SubtitleImage],
        per_image: Vec<Vec<subtrackt_core::Glyph>>,
        stages: &mut Stages<'_>,
        tally: &mut Tally<'_>,
        progress: &dyn Progress,
    ) -> Result<Vec<subtrackt_text::layout::AssembledCue>> {
        let total: u64 = images.len().try_into().unwrap_or(u64::MAX);
        let mut read_cues = Vec::with_capacity(images.len());
        let mut done = 0u64;
        let started = Instant::now();
        progress.begin(Phase::Read, Some(total));
        for (cue, (image, mut glyphs)) in images.iter().zip(per_image).enumerate() {
            let mut identified = Vec::with_capacity(glyphs.len());
            for glyph in &glyphs {
                identified.push(stages.matcher.match_glyph(glyph)?);
            }

            // Two characters that touched became one component the matcher could not read, and
            // #106 measured that at 28% of a real disc's remaining errors. Recovering them needs
            // the answer, so it happens here rather than in the segmenter -- and only when
            // something actually failed, which is why the mask is recomputed rather than kept for
            // every image in the film.
            if self.config.defuse == crate::config::Defusing::On
                && identified.iter().any(|m| m.character.is_none())
            {
                let mask = stages.segmenter.mask(image);
                let recovered =
                    defuse(&mask, &glyphs, &identified, stages.matcher, self.config.split)?;
                if let Some((new_glyphs, new_matches)) = recovered {
                    glyphs = new_glyphs;
                    identified = new_matches;
                    tally.report.defused += 1;
                }
            }

            // Named here rather than counted, because this is the one place a glyph and the answer
            // it did not get are both in hand. See `Outcome::unread`.
            tally.unread.extend(
                glyphs
                    .iter()
                    .zip(&identified)
                    .filter(|(_, m)| m.character.is_none())
                    .map(|(glyph, m)| UnreadGlyph {
                        cue,
                        line: glyph.line,
                        bounds: glyph.bounds,
                        metrics: glyph.metrics,
                        distance: m.distance,
                    }),
            );

            // How well the matched glyphs fitted, not just how many did. See `Report::distance_sum`
            // for why the second number cannot stand in for the first.
            tally.report.distance_sum += identified
                .iter()
                .filter(|m| m.character.is_some())
                .map(|m| u64::from(m.distance))
                .sum::<u64>();

            let read = stages
                .assembler
                .assemble_annotated(image, &glyphs, &identified)?;
            tally.report.record(read.cue.confidence);
            read_cues.push(read);
            done += 1;
            progress.advance(done);
        }
        progress.end();
        // Correction and the per-cue policy run under this phase's heading too: they are the rest
        // of turning a matched glyph into a written cue, and a timing that stopped at the loop
        // would hand the remainder to nobody.
        tally.cost.read = started.elapsed();
        Ok(read_cues)
    }
}

/// Try to read every unmatched component as two characters that touched.
///
/// Returns the rebuilt glyph and match lists when at least one component was recovered, and `None`
/// when nothing changed — so the caller can leave its own vectors alone in the overwhelmingly
/// common case.
///
/// The safety argument is entirely in the acceptance rule, and it is worth restating where the code
/// is. This runs **only** over components the matcher already returned `unmatched` for, and a cut is
/// kept **only** if every part matches within the ceiling. So the failure mode is bounded to
/// unmatched → matched: it cannot turn a match into a wrong answer, which is the direction
/// `docs/post-correction.md` says a recovery stage has to fail in.
fn defuse(
    mask: &BinaryMask,
    glyphs: &[subtrackt_core::Glyph],
    answers: &[subtrackt_core::GlyphMatch],
    matcher: &mut HammingMatcher,
    rules: SplitRules,
) -> Result<Option<(Vec<subtrackt_core::Glyph>, Vec<subtrackt_core::GlyphMatch>)>> {
    let mut out_glyphs = Vec::with_capacity(glyphs.len());
    let mut out_answers = Vec::with_capacity(answers.len());
    let mut changed = false;

    for (glyph, answer) in glyphs.iter().zip(answers) {
        let recovered = if answer.character.is_some() {
            None
        } else {
            recover(mask, glyph, matcher, rules, rules.max_cuts)?
        };
        if let Some(parts) = recovered {
            changed = true;
            for (part, part_answer) in parts {
                out_glyphs.push(part);
                out_answers.push(part_answer);
            }
        } else {
            out_glyphs.push(glyph.clone());
            out_answers.push(answer.clone());
        }
    }
    Ok(changed.then_some((out_glyphs, out_answers)))
}

/// Cut one component and read the parts, or give up.
///
/// Recursive on the left part only in the sense that `cuts` bounds the total depth: a part that is
/// itself unreadable gets one more chance to be two characters, which is what a three-character
/// fusion needs. `docs/error-census.md` found exactly one on the disc.
fn recover(
    mask: &BinaryMask,
    glyph: &subtrackt_core::Glyph,
    matcher: &mut HammingMatcher,
    rules: SplitRules,
    cuts: usize,
) -> Result<Option<Vec<(subtrackt_core::Glyph, subtrackt_core::GlyphMatch)>>> {
    if cuts == 0 || !glyph.metrics.known {
        // An unmeasurable line gives the parts no metrics to be scored against, and the parts are
        // exactly where the metric term matters most -- an `r` and a `t` differ in height and in
        // nothing else the shape vector keeps. Refusing is the same choice `LineMetrics::UNKNOWN`
        // makes everywhere else.
        return Ok(None);
    }

    for column in split::cut_columns(mask, glyph.bounds, rules) {
        let Some((left, right)) = split::parts_at(mask, glyph.bounds, column) else {
            continue;
        };
        let mut parts = Vec::with_capacity(2);
        let mut all_read = true;
        for bounds in [left, right] {
            let Some(part) = part_glyph(mask, glyph, bounds) else {
                all_read = false;
                break;
            };
            let answer = matcher.match_glyph(&part)?;
            if answer.character.is_some() {
                parts.push((part, answer));
                continue;
            }
            // One more cut, for the three-character case. Anything still unread after that fails
            // the whole cut rather than being kept as a partial recovery: half a fusion read and
            // half not is a wrong answer with a plausible shape, which is worse than the
            // placeholder it replaced.
            if let Some(deeper) = recover(mask, &part, matcher, rules, cuts - 1)? {
                parts.extend(deeper);
            } else {
                all_read = false;
                break;
            }
        }
        if all_read && parts.len() >= 2 {
            return Ok(Some(parts));
        }
    }
    Ok(None)
}

/// One part of a cut component, with metrics rescaled to the same line the parent was measured
/// against.
///
/// The parent carries its height and descent as percentages of its line's cap height, so the cap
/// height in pixels is recoverable from the pair — and that is the only way to give a part metrics
/// without carrying the line's anchors down here. Re-measuring the line would be the alternative
/// and would mean re-running `metrics::measure_all` over a segmentation that no longer matches the
/// one it produced.
fn part_glyph(
    mask: &BinaryMask,
    parent: &subtrackt_core::Glyph,
    bounds: Rect,
) -> Option<subtrackt_core::Glyph> {
    if parent.metrics.height_percent == 0 {
        return None;
    }
    let cap = i64::from(parent.bounds.height) * 100 / i64::from(parent.metrics.height_percent);
    if cap <= 0 {
        return None;
    }
    // The baseline, in plane coordinates: the parent's bottom edge less however far it descended.
    let baseline =
        i64::from(parent.bounds.bottom()) - i64::from(parent.metrics.descent_percent) * cap / 100;
    let height = i64::from(bounds.height) * 100 / cap;
    let descent = (i64::from(bounds.bottom()) - baseline) * 100 / cap;

    Some(subtrackt_core::Glyph {
        bounds,
        line: parent.line,
        features: feature::vectorize(mask, bounds, feature::AspectPolicy::default()).ok()?,
        metrics: LineMetrics::new(
            u32::try_from(height).unwrap_or(0),
            i32::try_from(descent).unwrap_or(0),
        ),
        // A part of a fused component has no mark: the fusion is two bodies touching, and anything
        // `group` had attached as a mark travelled with the whole component. Saying `NONE` is the
        // honest answer and costs nothing, since the term is off.
        mark: MarkSlope::NONE,
        // The slant, on the other hand, is a property of the *line* and a part stands on the same
        // line its parent did, so it inherits it outright.
        slant: parent.slant,
        // The aspect ratio, on the other hand, is exactly what a part has and its parent did not:
        // the cut is what gave it a box of its own, and unlike the metrics above it needs nothing
        // recovered to measure.
        aspect: subtrackt_core::InkAspect::measure(bounds.width, bounds.height),
        // A part reports its box, and that is an approximation rather than a measurement. #121's
        // span is the ink of one *labelled component*, and a part is by definition half of one —
        // there is no label that names it. The deskewed set is not even the same set: under a shear
        // the vertical cut `split` makes maps to a slanted line, so "left of the column" and "left
        // of the deskewed column" hold different pixels. What makes the box tolerable here is what
        // put the part on this path at all: the two characters were *touching*, so the gap this
        // span would report between them is near zero either way, and the outer edges — the ones a
        // word break is measured against — are the parent's own, which the box has right.
        upright: subtrackt_core::UprightSpan::of_box(bounds),
    })
}

/// A shear as tenths of a percent, saturating rather than wrapping.
///
/// A shear outside a few tenths is not a subtitle line, so the clamp never fires on real material;
/// it is written because a wrapped sign would turn an italic line into a violently upright one and
/// nothing downstream would say so.
#[allow(clippy::cast_possible_truncation)]
fn permille(shear: f64) -> i32 {
    (shear * 1000.0)
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

/// Whether a reference set holds an entry for a *slanted* rendering of anything.
///
/// The one question that decides #122. A set with such an entry can read an italic letter as it is
/// drawn and reads it well — 2.0% on a real disc — and deskewing the glyph then moves it away from
/// the entry that fitted. A set without one is reading italic text against upright vectors, which
/// is #14's 47-cell axis paid in full, and on the same disc that is 47.1% CER.
///
/// Read off the set rather than configured, because it is a property of the set and a user who
/// generated one from a single font file did not make a choice about slant — they simply have no
/// italic to offer. The documented first invocation of `gen-reference` produces exactly that set.
fn carries_a_slanted_cut(reference: &subtrackt_glyph::ReferenceSet) -> bool {
    use subtrackt_glyph::reference::Style;
    reference
        .entries()
        .iter()
        .any(|entry| matches!(entry.style, Style::Italic | Style::BoldItalic))
}

/// A glyph's ink aspect ratio, measured in whatever frame its vector was.
///
/// The deskewed width where the line's slant was measurable and the bounding box where it was not —
/// the same pairing the vector and the spacing rule make, so a glyph is measured entirely one way
/// or entirely the other.
fn upright_aspect(
    span: Option<subtrackt_core::UprightSpan>,
    bounds: Rect,
) -> subtrackt_core::InkAspect {
    let width = span.filter(|span| span.known).map_or(bounds.width, |span| {
        u32::try_from(span.right - span.left).unwrap_or(0)
            / subtrackt_core::SPAN_TENTHS.unsigned_abs()
    });
    subtrackt_core::InkAspect::measure(width, bounds.height)
}

/// The shear of each line that has one, with the pivot its spans are measured about.
///
/// A line missing from the map is one [`slant::line_shear`] declined to measure — too little ink,
/// too few glyphs. Its glyphs fall back to their bounding boxes, which is what the spacing rule
/// used before #121, rather than to a shear of zero that nothing measured.
///
/// The pivot is the line's own top edge. It is a translation and cancels in every gap, so it cannot
/// change an answer; it exists so a span reads as a number near the box it came from.
fn line_shears(
    labels: &subtrackt_glyph::ccl::LabelMap,
    grouped: &[subtrackt_glyph::group::GroupedGlyph],
) -> BTreeMap<usize, (f64, u32)> {
    use subtrackt_glyph::slant;

    let mut per_line: BTreeMap<usize, Vec<&subtrackt_glyph::group::GroupedGlyph>> = BTreeMap::new();
    for glyph in grouped {
        per_line.entry(glyph.line).or_default().push(glyph);
    }
    per_line
        .into_iter()
        .filter_map(|(line, members)| {
            let pivot = members.iter().map(|g| g.bounds().y).min().unwrap_or(0);
            slant::line_shear(labels, &members).map(|shear| (line, (shear, pivot)))
        })
        .collect()
}

/// Everything a run accumulates besides the cues themselves.
///
/// Grouped for the same reason [`Stages`] is: these four travel together through every step that
/// produces a cue, and threading them one by one put `build_track` over the argument limit as soon
/// as #154 added a fifth thing to fill in. They are also the same *kind* of thing — the outputs of
/// a run that are not the track.
struct Tally<'a> {
    report: &'a mut Report,
    corrections: &'a mut Vec<CorrectionLog>,
    unread: &'a mut Vec<UnreadGlyph>,
    cost: &'a mut Cost,
}

/// The stage instances one run is assembled from.
///
/// Grouped rather than passed one by one because they travel together and always will: they are
/// the fan of stage crates the architecture document describes, and a run either has all of them
/// or is not a run.
struct Stages<'a> {
    segmenter: &'a ImageSegmenter,
    matcher: &'a mut HammingMatcher,
    assembler: &'a SpatialAssembler,
}

/// Binarize, label, group and vectorize — the [`Segmenter`] side of `subtrackt-glyph`.
pub(crate) struct ImageSegmenter {
    binarizer: Binarizer,
    /// Whether the feature vector reads ink coverage rather than the binary mask.
    grey_coverage: bool,
    /// Whether to sample a leaning line's glyphs along its own slant.
    ///
    /// **Decided by the reference set, not by a threshold.** #122 measured the deskew and the
    /// italic reference cut as what they are: two answers to the same question, not a stage and an
    /// improvement to it. On 10 Cloverfield Lane's italic act, against a set with no italic
    /// entries, deskewing is worth **47.1% CER down to 8.1%** — and against a set that carries
    /// #66's italic cut the same deskew takes 2.0% *up* to 5.4%, because the cut already holds an
    /// entry shaped like the ink and the deskew moves the glyph away from it.
    ///
    /// So this is on exactly when the set cannot read a slanted letter as it is drawn, which the
    /// set itself says. `docs/italic-slant.md` has the four-way table.
    deskew: bool,
}

impl ImageSegmenter {
    pub(crate) const fn new(binarizer: Binarizer, grey_coverage: bool, deskew: bool) -> Self {
        Self { binarizer, grey_coverage, deskew }
    }

    /// The foreground mask for an image.
    pub(crate) fn mask(&self, image: &SubtitleImage) -> BinaryMask {
        self.binarizer.mask(image)
    }

    /// Segment an image, and hand back the foreground mask it was segmented from.
    ///
    /// The mask is built on every call already and dropped at the end of it; this is the same work
    /// with the intermediate kept. It exists because a glyph's *un-normalised* ink is not
    /// recoverable from what [`Segmenter::segment`] returns — [`FeatureVector`] is letterboxed onto
    /// a 16x16 grid and thresholded per cell, which is a lossy projection built to make two
    /// renderings of one character converge.
    ///
    /// #63 is the caller: telling a good reference-set fit from a bad one needs the ink's *style*
    /// — stroke weight, contrast, the shape of a terminal — and at 16x16 a stem is one to three
    /// cells, so those are quantised away before anything can measure them. `xtask font-id`
    /// measured the cost of asking through the grid instead at 46 to 54 points of font-retrieval
    /// accuracy.
    ///
    /// Kept off the [`Segmenter`] trait: the trait is the stage contract every extraction runs
    /// through, and this is an instrument. Nothing on the matching path calls it.
    pub(crate) fn segment_with_mask(
        &self,
        image: &SubtitleImage,
    ) -> Result<(Vec<subtrackt_core::Glyph>, BinaryMask)> {
        let mask = self.mask(image);
        let glyphs = self.segment_from(image, &mask)?;
        Ok((glyphs, mask))
    }

    /// The body of both entry points, over a mask the caller owns.
    fn segment_from(
        &self,
        image: &SubtitleImage,
        mask: &BinaryMask,
    ) -> Result<Vec<subtrackt_core::Glyph>> {
        use subtrackt_glyph::ccl::{self, ComponentFilter};
        use subtrackt_glyph::feature::{self, AspectPolicy};
        use subtrackt_glyph::group::{self, GroupingRules};
        use subtrackt_glyph::metrics::{self, MetricRules};
        use subtrackt_glyph::{mark, slant};

        // Components and lines are yes-or-no questions and need the binary mask. Only the feature
        // vector reads the coverage plane, and only when asked to.
        let coverage = self.grey_coverage.then(|| self.binarizer.coverage(image));
        // The map, not just the boxes. A slanted letter's box contains its neighbour's ink, so
        // `slant` cannot read a component off the mask the way `feature` and `mark` do — see
        // `ccl::Component::label` for what that would cost.
        let (components, labels) = ccl::label_with_map(mask, ComponentFilter::default())?;
        // One banding, used for both the assignment and the metrics. A band of nothing but accents
        // is not a text line — see `group::text_lines` — and a caller that banded twice could
        // measure line anchors against a different set than it grouped by.
        let bands = group::text_lines(mask, &components, GroupingRules::default());
        let lines = group::assign_to(&bands, &components)?;
        let grouped = group::group(&components, &lines, GroupingRules::default())?;

        // Where each glyph stands in its line, which the feature vector cannot express and which is
        // the only thing separating `o` from `O`. Measured per line, from that line's own ink.
        let measured = metrics::measure_all(&bands, &grouped, MetricRules::default());

        // How far each line leans, and therefore where its glyphs' ink would stand if it did not.
        // Per line rather than per glyph because that is the unit slant belongs to: `A`, `V` and
        // `w` have diagonal ink that is not slant, and #14 found slant constant within a stream.
        let shears = line_shears(&labels, &grouped);

        grouped
            .iter()
            .zip(measured)
            .map(|(glyph, line_metrics)| {
                let bounds = glyph
                    .parts
                    .iter()
                    .map(|p| p.bounds)
                    .reduce(subtrackt_core::Rect::union)
                    .unwrap_or_default();
                // `None` unless the whole configuration wants a deskewed vector *and* this
                // line was measurable. The span below is computed either way: spacing wants it on
                // every leaning line, and #121 measured that separately.
                let shear = self
                    .deskew
                    .then(|| shears.get(&glyph.line).map(|(shear, _)| *shear))
                    .flatten();
                let aspect;
                // How much ink one pixel holds, for this glyph and no other. The label test is what
                // #122 adds beyond the shear: a slanted letter's box contains its neighbour's ink,
                // and a sheared sampling would drag that neighbour's foot across the grid rather
                // than leave it in a corner. `ccl::Component::label` has the mechanism.
                let ink = |x: u32, y: u32| {
                    let label = labels.at(x, y);
                    if label == subtrackt_glyph::ccl::NO_LABEL
                        || !glyph.parts.iter().any(|p| p.label == label)
                    {
                        return 0.0;
                    }
                    coverage
                        .as_ref()
                        .map_or(1.0, |c| f32::from(c.get(x, y)) / 255.0)
                };
                Ok(subtrackt_core::Glyph {
                    bounds,
                    line: glyph.line,
                    features: match shear {
                        Some(shear) => {
                            feature::vectorize_sheared(bounds, shear, AspectPolicy::default(), ink)
                        }
                        None => coverage.as_ref().map_or_else(
                            || feature::vectorize(mask, bounds, AspectPolicy::default()),
                            |c| feature::vectorize_coverage(c, bounds, AspectPolicy::default()),
                        ),
                    }?,
                    metrics: line_metrics,
                    // Read off the binary mask before the boxes are merged: once base and mark
                    // share a bounding box, letterboxing scales the direction away.
                    mark: mark::slope(mask, glyph),
                    // #121: where the ink stands once the line's lean is divided out, which the
                    // box gets wrong by most of a stem on an italic line. A line whose shear could
                    // not be measured reports the box, which is what the spacing rule used before
                    // any of this — never a fabricated zero shear.
                    upright: {
                        let span = shears.get(&glyph.line).map_or_else(
                            || slant::box_span(bounds),
                            |(shear, pivot)| slant::upright_span(&labels, glyph, *shear, *pivot),
                        );
                        aspect = upright_aspect(shear.map(|_| span), bounds);
                        span
                    },
                    // The same box the vector was built from. #109: letterboxing keeps this ratio,
                    // and keeps it only to within a grid cell — the `l`/`I` difference is a fifth of
                    // one. Nothing about the line enters it, so it is measurable on a line whose
                    // metrics are not.
                    //
                    // #122 moves it into the *deskewed* frame wherever the vector moved too. A
                    // slanted `l` stands across a third of cap height where its ink is an eighth,
                    // so the box ratio of an italic glyph is a fact about the slant rather than
                    // about the letter — and the reference entry it is scored against was rendered
                    // upright. Measuring one side sheared and the other not is what #99, #110 and
                    // #113 each cost a release to find.
                    aspect,
                    // #123. Read off the ink and not off the matcher's answer, so it works on a
                    // set that carries no italic entries — which is the set the deskew above exists
                    // for. Every glyph on a line carries its line's figure; the assembler needs one
                    // per line and only glyphs reach it.
                    slant: shears
                        .get(&glyph.line)
                        .map_or(subtrackt_core::Slant::UPRIGHT, |(shear, _)| {
                            subtrackt_core::Slant::new(permille(*shear))
                        }),
                })
            })
            .collect()
    }
}

impl Segmenter for ImageSegmenter {
    fn segment(&self, image: &SubtitleImage) -> Result<Vec<subtrackt_core::Glyph>> {
        let mask = self.mask(image);
        self.segment_from(image, &mask)
    }

    fn binarize(&self, image: &SubtitleImage) -> Result<subtrackt_core::IndexedBitmap> {
        self.binarizer.mask_as_bitmap(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::{IndexedBitmap, Palette, PaletteEntry, Rect, TimeSpan, Timestamp};
    use subtrackt_glyph::matcher::MatchThresholds;

    /// A reference entry for `character` in one style. Only the style is under test here.
    fn entry(
        character: char,
        style: subtrackt_glyph::reference::Style,
    ) -> subtrackt_glyph::reference::ReferenceEntry {
        subtrackt_glyph::reference::ReferenceEntry {
            character,
            style,
            features: subtrackt_core::FeatureVector::EMPTY,
            metrics: subtrackt_core::LineMetrics::UNKNOWN,
            mark: subtrackt_core::MarkSlope::NONE,
            aspect: subtrackt_core::InkAspect::UNKNOWN,
        }
    }

    #[test]
    fn a_set_generated_from_one_font_carries_no_slanted_cut() {
        // Which is what makes #122 decidable rather than heuristic. The documented first
        // invocation of `gen-reference` takes one font file, so this is the default a user gets and
        // it is the configuration the deskew is worth 39 points of italic CER to.
        use subtrackt_glyph::reference::Style;
        let plain = subtrackt_glyph::ReferenceSet::new(
            "one face",
            vec![entry('a', Style::Regular), entry('b', Style::Regular)],
        );
        assert!(!carries_a_slanted_cut(&plain));
    }

    #[test]
    fn a_set_with_an_italic_cut_says_so_and_a_bold_one_does_not() {
        // Bold is upright. A set carrying a bold face can no more read a slanted letter than a
        // regular-only one can, so it must not turn the deskew off.
        use subtrackt_glyph::reference::Style;
        let bold = subtrackt_glyph::ReferenceSet::new(
            "regular and bold",
            vec![entry('a', Style::Regular), entry('a', Style::Bold)],
        );
        assert!(!carries_a_slanted_cut(&bold));

        for slanted in [Style::Italic, Style::BoldItalic] {
            let set = subtrackt_glyph::ReferenceSet::new(
                "with a slant",
                vec![entry('a', Style::Regular), entry('a', slanted)],
            );
            assert!(carries_a_slanted_cut(&set), "{slanted:?}");
        }
    }

    #[test]
    fn a_deskewed_glyph_is_measured_wide_the_way_its_vector_was() {
        // Both sides of #113's ink ratio have to be read in one frame. A slanted `l` stands across
        // a third of cap height where its ink is an eighth, so feeding the box width beside a
        // deskewed vector would be #99, #110 and #113's mistake a fourth time.
        let bounds = Rect::new(10, 0, 30, 40);
        let sheared = subtrackt_core::UprightSpan::new(100, 300);
        assert_eq!(
            upright_aspect(Some(sheared), bounds),
            subtrackt_core::InkAspect::measure(20, 40)
        );
        assert_eq!(upright_aspect(None, bounds), subtrackt_core::InkAspect::measure(30, 40));
    }

    #[test]
    fn a_glyph_on_an_unmeasurable_line_keeps_its_box_ratio() {
        // The boundary `CLAUDE.md` requires, carried through to the ratio: a span that was never
        // measured must not become a width.
        let bounds = Rect::new(0, 0, 30, 40);
        assert_eq!(
            upright_aspect(Some(subtrackt_core::UprightSpan::UNKNOWN), bounds),
            subtrackt_core::InkAspect::measure(30, 40)
        );
    }
    /// An 8x8 image holding one 4x4 block: big enough to survive the component area filter, and
    /// small enough relative to the image to survive the coverage filter.
    fn image() -> SubtitleImage {
        let mut palette = Palette::transparent(2);
        palette.set(1, PaletteEntry { y: 235, cb: 128, cr: 128, alpha: 255 });

        let pixels: Vec<u8> = (0..8)
            .flat_map(|y| (0..8).map(move |x| u8::from((2..6).contains(&x) && (2..6).contains(&y))))
            .collect();

        SubtitleImage {
            span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(1_000)),
            position: Rect::new(0, 0, 8, 8),
            bitmap: IndexedBitmap::new(8, 8, pixels).unwrap(),
            palette,
            forced: false,
        }
    }

    #[test]
    fn segmenting_with_the_mask_kept_returns_the_same_glyphs_as_segmenting_without() {
        // The instrument must not change what it observes. `segment_with_mask` exists so #63 can
        // read a glyph's un-normalised ink, and if it segmented even slightly differently from the
        // shipped path then every measurement taken through it would be about a pipeline nobody
        // runs.
        let segmenter = ImageSegmenter::new(Binarizer::default(), false, false);
        let plain = segmenter.segment(&image()).unwrap();
        let (with_mask, mask) = segmenter.segment_with_mask(&image()).unwrap();
        assert_eq!(plain, with_mask);
        assert_eq!((mask.width(), mask.height()), (8, 8), "the whole image, not a glyph");
    }

    #[test]
    fn the_kept_mask_carries_ink_the_feature_vector_cannot_express() {
        // The 4x4 block is 16 pixels of ink in a 4x4 box: solid. The feature vector letterboxes
        // that onto 16x16 and says which cells are inked, so it can say the shape is square but
        // not that the stroke is four pixels wide -- which is the whole reason this exists.
        let segmenter = ImageSegmenter::new(Binarizer::default(), false, false);
        let (glyphs, mask) = segmenter.segment_with_mask(&image()).unwrap();
        assert_eq!(glyphs.len(), 1);

        let cropped = mask.crop(glyphs[0].bounds).unwrap();
        assert_eq!(
            (cropped.width(), cropped.height()),
            (glyphs[0].bounds.width, glyphs[0].bounds.height)
        );
        assert_eq!(
            cropped.foreground_count(),
            (cropped.width() * cropped.height()) as usize,
            "a solid block crops to solid ink, at its own resolution"
        );
    }

    #[test]
    fn the_binarizer_runs_end_to_end_inside_the_segmenter() {
        let segmenter = ImageSegmenter::new(Binarizer::default(), false, false);
        let bitmap = segmenter.binarize(&image()).unwrap();
        assert_eq!(bitmap.width(), 8);
        assert_eq!(bitmap.get(3, 3), Some(1), "inside the block is foreground");
        assert_eq!(bitmap.get(0, 0), Some(0), "outside it is not");
    }

    #[test]
    fn segmentation_carries_a_component_through_to_a_feature_vector() {
        let segmenter = ImageSegmenter::new(Binarizer::default(), false, false);
        // Segmentation is complete now, so this no longer fails at all. What the test still
        // pins is that a glyph-sized component makes it all the way to a feature vector.
        let glyphs = segmenter.segment(&image()).unwrap();
        assert_eq!(glyphs.len(), 1, "the 4x4 block is one glyph");
        assert!(
            glyphs[0].features.popcount() > 0,
            "and it vectorizes to something non-empty"
        );
    }

    #[test]
    fn a_missing_input_file_fails_at_demux_not_deeper_in() {
        let err = Pipeline::new(Config::default())
            .run("no_such_file.sup")
            .unwrap_err();
        assert!(matches!(err, Error::Io { .. }), "got {err:?}");
    }

    #[test]
    fn an_unknown_extension_is_refused_before_anything_is_read() {
        let err = Pipeline::list("subtitles.avi").unwrap_err();
        assert!(matches!(err, Error::Demux(_)), "got {err:?}");
    }

    /// Two 6x10 blocks in a 24x14 image, joined by a two-pixel bridge or not.
    ///
    /// `bridged` is the shape a corner touch makes: one connected component that is two characters,
    /// with **no empty column** between them, because if there were one they would never have been
    /// labelled together in the first place. Without it the same two characters segment normally,
    /// which is what the reference set is built from.
    fn two_blocks(bridged: bool) -> SubtitleImage {
        let mut palette = Palette::transparent(2);
        palette.set(1, PaletteEntry { y: 235, cb: 128, cr: 128, alpha: 255 });

        let ink = move |x: u32, y: u32| {
            let inside = (2..12).contains(&y);
            let left = (4..10).contains(&x);
            let right = (12..18).contains(&x);
            let bridge = bridged && (10..12).contains(&x) && y == 7;
            (inside && (left || right)) || bridge
        };
        let pixels: Vec<u8> = (0..14)
            .flat_map(|y| (0..24).map(move |x| u8::from(ink(x, y))))
            .collect();

        SubtitleImage {
            span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(1_000)),
            position: Rect::new(0, 0, 24, 14),
            bitmap: IndexedBitmap::new(24, 14, pixels).unwrap(),
            palette,
            forced: false,
        }
    }

    fn fused_image() -> SubtitleImage {
        two_blocks(true)
    }

    /// The reference set the two blocks produce when they do **not** touch.
    ///
    /// Not circular, and worth saying why: this is exactly what `gen-reference` writes — a
    /// character's vector as the pipeline's own normalisation produces it. The question the test
    /// asks is whether a *cut* half lands close enough to that to be read, which is the same
    /// question a real fusion asks.
    fn block_set() -> subtrackt_glyph::ReferenceSet {
        let segmenter = ImageSegmenter::new(Binarizer::default(), false, false);
        let glyphs = segmenter.segment(&two_blocks(false)).unwrap();
        assert_eq!(glyphs.len(), 2, "without a bridge they are two components");
        subtrackt_glyph::ReferenceSet::new(
            "blocks",
            glyphs
                .iter()
                .map(|g| subtrackt_glyph::reference::ReferenceEntry {
                    character: 'x',
                    style: subtrackt_glyph::reference::Style::Regular,
                    features: g.features,
                    metrics: LineMetrics::UNKNOWN,
                    mark: MarkSlope::NONE,
                    aspect: subtrackt_core::InkAspect::UNKNOWN,
                })
                .collect(),
        )
    }

    #[test]
    fn two_characters_that_touched_are_read_as_two_rather_than_left_unread() {
        // The whole of #106 as one assertion. The fused component matches nothing, both halves
        // match the block, so the pass replaces one unread glyph with two read ones.
        let segmenter = ImageSegmenter::new(Binarizer::default(), false, false);
        let (mut glyphs, mask) = segmenter.segment_with_mask(&fused_image()).unwrap();
        assert_eq!(glyphs.len(), 1, "the bridge makes it one component");
        // A line of one glyph has no measurable anchors, and the pass refuses a glyph whose line
        // could not be measured -- see `recover`. Standing the glyph on a measured line is what a
        // real cue does; there is nothing else in this image to make one out of.
        glyphs[0].metrics = LineMetrics::new(100, 0);

        let mut matcher = HammingMatcher::new(block_set(), MatchThresholds::default()).unwrap();
        let answers: Vec<subtrackt_core::GlyphMatch> = glyphs
            .iter()
            .map(|g| matcher.match_glyph(g).unwrap())
            .collect();
        assert!(answers[0].character.is_none(), "a fused pair reads as nothing");

        let recovered = defuse(&mask, &glyphs, &answers, &mut matcher, SplitRules::default())
            .unwrap()
            .expect("the fusion is recoverable");
        assert_eq!(recovered.0.len(), 2, "one component became two glyphs");
        assert!(
            recovered.1.iter().all(|m| m.character == Some('x')),
            "and both of them read"
        );
        assert!(
            recovered.0[0].bounds.right() <= recovered.0[1].bounds.x + 1,
            "in left-to-right order: {:?}",
            recovered.0
        );
    }

    #[test]
    fn a_component_that_already_read_is_never_cut() {
        // The safety property, and the reason this pass is allowed to be permissive about *where*
        // it cuts. It never sees a glyph the matcher answered, so it cannot turn a match into a
        // wrong answer however wrong a cut would have been.
        let segmenter = ImageSegmenter::new(Binarizer::default(), false, false);
        let (glyphs, mask) = segmenter.segment_with_mask(&fused_image()).unwrap();
        let answers = vec![subtrackt_core::GlyphMatch {
            character: Some('q'),
            distance: 0,
            runner_up_distance: 99,
        }];
        let mut matcher = HammingMatcher::new(block_set(), MatchThresholds::default()).unwrap();
        assert_eq!(
            defuse(&mask, &glyphs, &answers, &mut matcher, SplitRules::default()).unwrap(),
            None,
            "nothing was unread, so nothing is proposed"
        );
    }

    #[test]
    fn a_cut_whose_parts_do_not_both_read_is_refused_outright() {
        // Half a fusion read and half not is a wrong answer with a plausible shape, which is worse
        // than the placeholder it would replace. An empty reference set makes every part unread, so
        // every candidate cut has to be rejected and the glyph left exactly as it was.
        let segmenter = ImageSegmenter::new(Binarizer::default(), false, false);
        let (glyphs, mask) = segmenter.segment_with_mask(&fused_image()).unwrap();
        let mut matcher = HammingMatcher::new(
            subtrackt_glyph::ReferenceSet::new("empty", Vec::new()),
            MatchThresholds::default(),
        )
        .unwrap();
        let answers: Vec<subtrackt_core::GlyphMatch> = glyphs
            .iter()
            .map(|g| matcher.match_glyph(g).unwrap())
            .collect();
        assert_eq!(
            defuse(&mask, &glyphs, &answers, &mut matcher, SplitRules::default()).unwrap(),
            None
        );
    }

    #[test]
    fn a_part_is_measured_against_the_same_line_its_parent_was() {
        // The parts have to carry line metrics or the matcher scores them on shape alone -- and an
        // `r` against a `t` differs in height and in little else the shape vector keeps. The cap
        // height is recovered from the parent's own pair, so a part half the parent's height
        // reports half the height percentage.
        let segmenter = ImageSegmenter::new(Binarizer::default(), false, false);
        let (glyphs, mask) = segmenter.segment_with_mask(&fused_image()).unwrap();
        let mut parent = glyphs[0].clone();
        parent.metrics = LineMetrics::new(100, 0);

        let half = Rect::new(parent.bounds.x, parent.bounds.y, parent.bounds.width, 5);
        let part = part_glyph(&mask, &parent, half).expect("the box has ink");
        assert_eq!(part.metrics.height_percent, 50, "half the cap height");
        assert!(part.metrics.known, "and measured rather than fabricated");
    }

    #[test]
    fn a_part_of_a_glyph_on_an_unmeasurable_line_is_refused() {
        // `LineMetrics::UNKNOWN` carries no cap height, so there is nothing to scale a part
        // against. Inventing one would be exactly the fabrication `LineMetrics` exists to refuse.
        let segmenter = ImageSegmenter::new(Binarizer::default(), false, false);
        let (glyphs, mask) = segmenter.segment_with_mask(&fused_image()).unwrap();
        let mut matcher = HammingMatcher::new(block_set(), MatchThresholds::default()).unwrap();
        let mut glyph = glyphs[0].clone();
        glyph.metrics = LineMetrics::UNKNOWN;
        assert_eq!(
            recover(&mask, &glyph, &mut matcher, SplitRules::default(), 2).unwrap(),
            None
        );
    }
}
